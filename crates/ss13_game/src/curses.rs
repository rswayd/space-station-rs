//! Magic and antagonist curses (server), after tg `code/datums/status_effects/debuffs/debuffs.dm`
//! (`necropolis_curse`, `trance`, `go_away`, `ants`, `rebuked`, `freezing_blast`,
//! `discoordinated`, `midas_blight`, `designated_target`) and `strandling.dm`.
//!
//! - [`NecropolisCurse`] ([`apply_necropolis_curse`]): 10 minutes (+5 per new curse) of one or
//!   more of blinding (the flag), wasting (0.75 burn every 5 s with `curse5.ogg`), grasping.
//! - [`Trance`] ([`apply_trance`]): mute, stunned 6 s and dizzy 40 s every second, the alert;
//!   the next words heard become a hypnosis trauma's phrase (6 s stun 1.5 s later).
//! - [`GoAway`] ([`apply_go_away`]): 10 s (30 s and gone for `deletes_mob`) flying a tile every
//!   0.2 s in one random direction, stunned; a map edge ends it.
//! - [`Ants`] ([`apply_ants`]): ants (fire ants 4 times as hurtful) bite every second by their
//!   number, scratching hurts a limb, rubbing the eyes blurs; the alert click shakes 10 off.
//! - [`Rebuked`] (30 s): every click cooldown twice as long. [`FreezingBlast`] (5 s): 1 more
//!   decisecond a tile. [`Discoordinated`]: the flag tool users check. [`DesignatedTarget`]
//!   (2 minutes): the flag projectiles check; water washes it off.
//! - [`MidasBlight`] ([`apply_midas_blight`]): 5 blood a second turns into 30 units of cursed
//!   gold a second, slowing by the gold in the blood; death ends it.
//! - [`RustCorruption`] ([`set_rust_corruption`]): silicons and bots rust (10 brute a second),
//!   others lose 0.75 units of every reagent a second.
//! - [`Strandling`] ([`apply_strandling`]): a breath lost every breath (2 s); a help click (also on
//!   oneself) takes it off in 3.5 s, wirecutters in 1.4 s.
// shortcut: no curse fullscreen, wasting visual, grasping curse hands (the projectile is not
// ported: the flag is kept), trance monochrome colour or deafness check, ants speech lines (the
// scream only) or washing off by cleaning, midas overlays and actionspeed, flare light and
// outline of the designated target, `TRAIT_DISCOORDINATED_TOOL_USER` users, the
// `deletes_mob` fade; the rust's disgust (not ported) and robotic limb damage. Fake virus and
// the tower of Babel live in `crate::events`.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::Spoke;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::breath::{AlertCategory, Alerts, Respiration};
use crate::chem::{name_of, tell_mob, visible};
use crate::combat::{CombatRng, NextMove};
use crate::health::{DamageType, Health, HealthSystems, MoveSpeedModifier, Stat};
use crate::status_effects::{self, Incap, StatusSystems, Timed};
use crate::tools::{Actor, DoAfter};
use crate::zone::BodyZone;

/// `CURSE_BLINDING`, `CURSE_WASTING`, `CURSE_GRASPING`.
pub const CURSE_BLINDING: u8 = 1;
pub const CURSE_WASTING: u8 = 2;
pub const CURSE_GRASPING: u8 = 4;
const NECRO_SECS: f64 = 600.0;
const NECRO_EXTRA_SECS: f64 = 300.0;
const NECRO_TICK_SECS: f64 = 5.0;
pub const CURSE_SOUND: &str = "sound/effects/curse/curse5.ogg";
/// `go_away`: `duration`, `deletes_mob` duration, `tick_interval`.
const GO_AWAY_SECS: f64 = 10.0;
const GO_AWAY_DELETES_SECS: f64 = 30.0;
const GO_AWAY_TICK_SECS: f64 = 0.2;
/// `ants`: `duration`, `damage_per_ant` (fire ants ×4).
const ANTS_SECS: f64 = 120.0;
const DAMAGE_PER_ANT: f32 = 0.0016;
const FIRE_DAMAGE_PER_ANT: f32 = 0.0064;
const ANTS_SHAKE_DS: f32 = 20.0;
/// `rebuked`, `freezing_blast`, `designated_target` durations; the blast's slowdown.
const REBUKED_SECS: f64 = 30.0;
const FREEZING_SECS: f64 = 5.0;
const FREEZING_SLOWDOWN: f32 = 1.0;
const DESIGNATED_SECS: f64 = 120.0;
/// `midas_blight`: `tick_interval`, `goldscale`; `BLOOD_VOLUME_SURVIVE + 50`.
const MIDAS_TICK_SECS: f64 = 0.2;
const MIDAS_GOLD_PER_SEC: f32 = 30.0;
const MIDAS_MIN_BLOOD: f32 = 172.0;
pub const CURSED_GOLD: &str = "/datum/reagent/gold/cursed";
/// `strandling`: `time_to_remove`, `STRANGLING_TOOL_MULTIPLIER`.
const STRAND_REMOVE_DS: f32 = 35.0;
const STRAND_TOOL_MULT: f32 = 0.4;
/// `SSmobs` breaths: once every 2 s.
const STRAND_TICK_SECS: f64 = 2.0;

/// `/datum/status_effect/necropolis_curse`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct NecropolisCurse {
    pub flags: u8,
    pub until: f64,
    next_tick: f64,
    /// `effect_next_activation` of the grasping hands.
    pub next_hand: f64,
}

/// `/datum/status_effect/trance`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Trance {
    pub until: f64,
    pub stun: bool,
    next_tick: f64,
}

/// `/datum/status_effect/go_away` (`deletes_mob`: the mob is gone at the end).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GoAway {
    pub until: f64,
    pub dir: (i32, i32),
    pub deletes: bool,
    next_tick: f64,
}

/// `/datum/status_effect/ants` (`fire`: `/datum/status_effect/ants/fire`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Ants {
    pub remaining: f32,
    pub fire: bool,
    pub until: f64,
    next_tick: f64,
}

/// `/datum/status_effect/rebuked` until this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Rebuked {
    pub until: f64,
}

/// `/datum/status_effect/freezing_blast` until this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FreezingBlast {
    pub until: f64,
}

/// `/datum/status_effect/discoordinated` (`TRAIT_DISCOORDINATED_TOOL_USER`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Discoordinated;

/// `/datum/status_effect/designated_target` (`TRAIT_DESIGNATED_TARGET`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct DesignatedTarget {
    pub until: f64,
}

/// `/datum/status_effect/midas_blight`: its end, the slowdown it added.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct MidasBlight {
    pub until: f64,
    pub slowdown: f32,
    next_tick: f64,
}

/// `/datum/status_effect/strandling`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Strandling {
    next_tick: f64,
}

/// Timed actions of this module (`crate::construction::Job::Curse`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurseJob {
    ShakeAnts,
    RemoveStrand,
}

pub struct CursesPlugin;

impl Plugin for CursesPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(rebuked_next_move)
            .add_systems(
                Update,
                (help_clicks, alert_clicks)
                    .after(crate::click::ResolveClicks)
                    .after(crate::medical::MedicalClicks)
                    .before(crate::combat::MeleeSystems)
                    .before(crate::inventory::inventory_intents),
            )
            .add_systems(
                Update,
                (
                    necropolis_tick,
                    trance_hear,
                    trance_tick,
                    go_away_tick,
                    ants_tick,
                    timed_upkeep,
                    midas_tick,
                    rust_tick,
                    strandling_tick,
                )
                    .chain()
                    .after(StatusSystems)
                    .before(HealthSystems),
            );
        // Wirecutters on a strangled mob.
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::HeldOnTarget, 3485, |w, c| {
            cut_strand(w, c.client, c.mob, c.held, c.target)
        });
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn rng_range(world: &mut World, low: u32, high: u32) -> u32 {
    world.get_resource_or_init::<CombatRng>().range(low, high)
}

fn prob(world: &mut World, percent: f32) -> bool {
    world.get_resource_or_init::<CombatRng>().chance(percent)
}

fn dead(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob).is_none_or(|s| *s == Stat::Dead)
}

fn alert(world: &mut World, mob: Entity, category: AlertCategory, state: Option<&'static str>) {
    let Some(mut alerts) = world.get_mut::<Alerts>(mob) else {
        return;
    };
    match state {
        Some(state) => alerts.throw(category, state, 0),
        None if alerts.get(category).is_some() => alerts.clear(category),
        None => {}
    }
}

fn ids<C: Component>(world: &mut World) -> Vec<Entity> {
    world
        .query_filtered::<Entity, With<C>>()
        .iter(world)
        .collect()
}

fn add_slowdown(world: &mut World, mob: Entity, delta: f32) {
    let current = world.get::<MoveSpeedModifier>(mob).map_or(0.0, |m| m.0);
    world
        .entity_mut(mob)
        .insert(MoveSpeedModifier(current + delta));
}

fn zone_damage(world: &mut World, mob: Entity, zone: BodyZone, amount: f32) {
    let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    else {
        return;
    };
    let mut new = stat;
    crate::health::apply_damage_zone(&mut health, &mut new, zone, DamageType::Brute, amount);
    world.entity_mut(mob).insert(health);
    if new != stat {
        world.entity_mut(mob).insert(new);
    }
}

/// The examine lines of the curses (`get_examine_text`).
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<Trance>(mob).is_some() {
        lines.push((
            Span::Warning,
            "Он кажется медлительным и рассеянным.".to_owned(),
        ));
    }
    if world.get::<Ants>(mob).is_some() {
        lines.push((Span::Warning, "Он покрыт муравьями!".to_owned()));
    }
    if world.get::<Strandling>(mob).is_some() {
        lines.push((
            Span::Warning,
            "Его душат нити дюраткани. Возможно, их удастся срезать.".to_owned(),
        ));
    }
}

/// `TRAIT_MUTE` of the trance.
pub fn mutes(world: &World, mob: Entity) -> bool {
    world.get::<Trance>(mob).is_some()
}

/// `TRAIT_DISCOORDINATED_TOOL_USER`.
pub fn discoordinated(world: &World, mob: Entity) -> bool {
    world.get::<Discoordinated>(mob).is_some()
}

/// `TRAIT_DESIGNATED_TARGET`.
pub fn designated(world: &World, mob: Entity) -> bool {
    world.get::<DesignatedTarget>(mob).is_some()
}

// --- Necropolis curse ------------------------------------------------------------------------

/// tg `apply_necropolis_curse(set_curse)`: no flags picks one of the three; an existing curse
/// gains the flags and 5 more minutes.
pub fn apply_necropolis_curse(world: &mut World, mob: Entity, flags: Option<u8>) {
    if world.get::<Stat>(mob).is_none() {
        return;
    }
    let flags = match flags {
        Some(f) => f,
        None => [CURSE_BLINDING, CURSE_WASTING, CURSE_GRASPING][rng_range(world, 0, 2) as usize],
    };
    let now = now(world);
    if let Some(mut curse) = world.get_mut::<NecropolisCurse>(mob) {
        curse.flags |= flags;
        curse.until += NECRO_EXTRA_SECS;
        return;
    }
    world.entity_mut(mob).insert(NecropolisCurse {
        flags,
        until: now + NECRO_SECS,
        next_tick: now + NECRO_TICK_SECS,
        next_hand: 0.0,
    });
}

fn necropolis_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<NecropolisCurse>(world) {
        let Some(curse) = world.get::<NecropolisCurse>(mob).copied() else {
            continue;
        };
        if now >= curse.until {
            world.entity_mut(mob).remove::<NecropolisCurse>();
            continue;
        }
        if now < curse.next_tick {
            continue;
        }
        if let Some(mut c) = world.get_mut::<NecropolisCurse>(mob) {
            c.next_tick += NECRO_TICK_SECS;
        }
        if dead(world, mob) {
            continue;
        }
        if curse.flags & CURSE_WASTING != 0 {
            if let Some(pos) = crate::explosions::turf_of(world, mob) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, CURSE_SOUND, 20.0, true).extrarange(-1.0),
                );
            }
            crate::medical::diseases::damage(world, mob, DamageType::Burn, 0.75);
        }
        if curse.flags & CURSE_GRASPING != 0
            && now >= curse.next_hand
            && let Some(mut c) = world.get_mut::<NecropolisCurse>(mob)
        {
            c.next_hand = now + 10.0;
        }
    }
}

// --- Trance ----------------------------------------------------------------------------------

/// tg `apply_status_effect(/datum/status_effect/trance, duration, stun)` (`STATUS_EFFECT_UNIQUE`):
/// carbons only. True when it started.
pub fn apply_trance(world: &mut World, mob: Entity, secs: f32, stun: bool) -> bool {
    if world.get::<crate::chem::MobReagents>(mob).is_none() || world.get::<Trance>(mob).is_some() {
        return false;
    }
    let now = now(world);
    world.entity_mut(mob).insert(Trance {
        until: now + f64::from(secs),
        stun,
        next_tick: now + 1.0,
    });
    crate::medical::traumas::refresh(world, mob);
    alert(world, mob, AlertCategory::Trance, Some("high"));
    const LINES: [&str; 4] = [
        "Ваши мысли замедляются...",
        "У вас внезапно сильно кружится голова...",
        "Вам кажется, что вы посреди сна...",
        "Вы чувствуете невероятное расслабление...",
    ];
    let line = LINES[rng_range(world, 0, 3) as usize];
    let name = name_of(world, mob);
    let others = if stun {
        format!("{name} замирает, его взгляд сосредотачивается на далёкой точке.")
    } else {
        String::new()
    };
    visible(world, mob, line, &others, Span::Warning);
    true
}

fn end_trance(world: &mut World, mob: Entity) {
    world.entity_mut(mob).remove::<Trance>();
    crate::medical::traumas::refresh(world, mob);
    alert(world, mob, AlertCategory::Trance, None);
    status_effects::set_timed(world, mob, Timed::Dizziness, 0.0, false);
    tell_mob(world, mob, Span::Warning, "Вы выходите из транса!");
}

fn trance_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<Trance>(world) {
        let Some(t) = world.get::<Trance>(mob).copied() else {
            continue;
        };
        if now >= t.until {
            end_trance(world, mob);
            continue;
        }
        if now < t.next_tick {
            continue;
        }
        if let Some(mut tr) = world.get_mut::<Trance>(mob) {
            tr.next_tick += 1.0;
        }
        if t.stun {
            status_effects::set(world, mob, Incap::Stun, 6.0);
        }
        status_effects::set_timed(world, mob, Timed::Dizziness, 40.0, false);
    }
}

/// `trance/hypnotize` on `COMSIG_MOVABLE_HEAR`: the heard words are the hypnosis phrase.
fn trance_hear(world: &mut World, mut spoke: Local<MessageCursor<Spoke>>) {
    let said: Vec<(Entity, TilePos, bool, String)> = spoke
        .read(world.resource::<Messages<Spoke>>())
        .map(|s| (s.speaker, s.pos, s.whisper, s.text.clone()))
        .collect();
    if said.is_empty() {
        return;
    }
    for mob in ids::<Trance>(world) {
        let Some(&at) = world.get::<TilePos>(mob) else {
            continue;
        };
        let Some((_, _, _, text)) = said.iter().find(|(speaker, pos, whisper, _)| {
            *speaker != mob
                && at
                    .chebyshev(*pos)
                    .is_some_and(|d| d <= if *whisper { 1 } else { 7 })
        }) else {
            continue;
        };
        let text = text.clone();
        world.entity_mut(mob).remove::<Trance>();
        crate::medical::traumas::refresh(world, mob);
        alert(world, mob, AlertCategory::Trance, None);
        status_effects::set_timed(world, mob, Timed::Dizziness, 0.0, false);
        tell_mob(world, mob, Span::Warning, "Вы выходите из транса!");
        if let Some(brain) =
            crate::medical::organs::organ(world, mob, crate::medical::organs::Slot::Brain)
        {
            let mut rng = crate::medical::fork_rng(world);
            crate::medical::traumas::gain_hypnosis(
                world,
                brain,
                &text,
                Some(crate::medical::traumas::Resilience::Surgery),
                &mut rng,
            );
        }
        // `addtimer(Stun(60), 15)`: the stun lands at once.
        status_effects::set(world, mob, Incap::Stun, 6.0);
    }
}

// --- Go away ---------------------------------------------------------------------------------

/// tg `apply_status_effect(/datum/status_effect/go_away)` (`STATUS_EFFECT_REPLACE`);
/// `deletes_mob` despawns the mob 5 s before its 30 s end.
pub fn apply_go_away(world: &mut World, mob: Entity, deletes: bool) {
    if world.get::<Stat>(mob).is_none() {
        return;
    }
    const DIRS: [(i32, i32); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
    let dir = DIRS[rng_range(world, 0, 3) as usize];
    let now = now(world);
    let secs = if deletes {
        GO_AWAY_DELETES_SECS - 5.0
    } else {
        GO_AWAY_SECS
    };
    world.entity_mut(mob).insert(GoAway {
        until: now + secs,
        dir,
        deletes,
        next_tick: now + GO_AWAY_TICK_SECS,
    });
    alert(world, mob, AlertCategory::GoAway, Some("high"));
}

fn end_go_away(world: &mut World, mob: Entity, deletes: bool) {
    world.entity_mut(mob).remove::<GoAway>();
    alert(world, mob, AlertCategory::GoAway, None);
    if deletes {
        // `fade_into_nothing`.
        world.entity_mut(mob).despawn();
    }
}

fn go_away_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<GoAway>(world) {
        let Some(g) = world.get::<GoAway>(mob).copied() else {
            continue;
        };
        if now >= g.until || (g.deletes && dead(world, mob)) {
            end_go_away(world, mob, g.deletes);
            continue;
        }
        if now < g.next_tick {
            continue;
        }
        if let Some(mut go) = world.get_mut::<GoAway>(mob) {
            go.next_tick += GO_AWAY_TICK_SECS;
        }
        status_effects::adjust(world, mob, Incap::Stun, 0.1);
        let Some(&pos) = world.get::<TilePos>(mob) else {
            continue;
        };
        let next = TilePos::new(pos.x + g.dir.0, pos.y + g.dir.1, pos.z);
        let inside = world
            .get_resource::<ss13_world::SpatialIndex>()
            .is_some_and(|i| i.in_bounds(next));
        if !inside {
            end_go_away(world, mob, g.deletes);
            continue;
        }
        // `forceMove(turf)`: through anything.
        world.entity_mut(mob).insert(next);
    }
}

// --- Ants ------------------------------------------------------------------------------------

/// tg `apply_status_effect(/datum/status_effect/ants, amount)` (`STATUS_EFFECT_REFRESH`): below
/// hard crit; more ants add up and start the 2 minutes over.
pub fn apply_ants(world: &mut World, mob: Entity, amount: f32, fire: bool) {
    let Some(&stat) = world.get::<Stat>(mob) else {
        return;
    };
    if matches!(stat, Stat::HardCrit | Stat::Dead) {
        return;
    }
    let now = now(world);
    let awake = !status_effects::knocked_out(world, mob);
    if let Some(mut ants) = world.get_mut::<Ants>(mob) {
        ants.remaining += amount;
        ants.until = now + ANTS_SECS;
        ants.fire |= fire;
        if awake {
            tell_mob(world, mob, Span::Danger, "На вас ЕЩЁ БОЛЬШЕ муравьёв!");
        }
        return;
    }
    world.entity_mut(mob).insert(Ants {
        remaining: amount,
        fire,
        until: now + ANTS_SECS,
        next_tick: now + 1.0,
    });
    let state = if fire { "fire_antalert" } else { "antalert" };
    alert(world, mob, AlertCategory::Ants, Some(state));
    if awake {
        tell_mob(world, mob, Span::Danger, "Вас облепили муравьи!");
    }
}

fn end_ants(world: &mut World, mob: Entity) {
    world.entity_mut(mob).remove::<Ants>();
    alert(world, mob, AlertCategory::Ants, None);
    tell_mob(
        world,
        mob,
        Span::Notice,
        "Все муравьи слезли с вашего тела!",
    );
}

/// `ants/tick` every second.
fn ants_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<Ants>(world) {
        let Some(ants) = world.get::<Ants>(mob).copied() else {
            continue;
        };
        if now >= ants.until {
            end_ants(world, mob);
            continue;
        }
        if now < ants.next_tick {
            continue;
        }
        let per_ant = if ants.fire {
            FIRE_DAMAGE_PER_ANT
        } else {
            DAMAGE_PER_ANT
        };
        let bite = ((ants.remaining * per_ant * 10.0).round() / 10.0).max(0.1);
        crate::medical::diseases::damage(world, mob, DamageType::Brute, bite);
        let mut remaining = ants.remaining;
        let awake = !status_effects::knocked_out(world, mob)
            && world.get::<Stat>(mob) == Some(&Stat::Conscious);
        if awake {
            if prob(world, 15.0) {
                crate::medical::diseases::emote(world, mob, "scream");
            }
            if prob(world, 50.0) {
                let roll = rng_range(world, 1, 50);
                let (text, zone) = match roll {
                    1..=8 => ("Вы чешете муравьёв на голове!", Some((BodyZone::Head, 0.4))),
                    9..=29 => {
                        let arm = [BodyZone::LArm, BodyZone::RArm][rng_range(world, 0, 1) as usize];
                        ("Вы чешете муравьёв на руках!", Some((arm, 1.2)))
                    }
                    30..=49 => {
                        let leg = [BodyZone::LLeg, BodyZone::RLeg][rng_range(world, 0, 1) as usize];
                        ("Вы чешете муравьёв на ноге!", Some((leg, 1.2)))
                    }
                    _ => ("Вы стираете муравьёв с глаз!", None),
                };
                tell_mob(world, mob, Span::Danger, text);
                match zone {
                    Some((zone, amount)) => zone_damage(world, mob, zone, amount),
                    None => {
                        crate::vision::set_eye_blur_if_lower(world, mob, 6.0);
                        remaining -= 5.0;
                    }
                }
            }
        }
        remaining -= 1.0;
        let crit = world
            .get::<Stat>(mob)
            .is_none_or(|s| matches!(s, Stat::HardCrit | Stat::Dead));
        if remaining <= 0.0 || crit {
            end_ants(world, mob);
            continue;
        }
        if let Some(mut a) = world.get_mut::<Ants>(mob) {
            a.remaining = remaining;
            a.next_tick += 1.0;
        }
    }
}

// --- Rebuked, freezing blast, discoordinated, designated target ------------------------------

/// tg `apply_status_effect(/datum/status_effect/rebuked)` (`STATUS_EFFECT_REFRESH`).
pub fn apply_rebuked(world: &mut World, mob: Entity) {
    apply_rebuked_for(world, mob, REBUKED_SECS);
}

/// [`apply_rebuked`] for `secs` (the lobster claw trophy's second).
pub fn apply_rebuked_for(world: &mut World, mob: Entity, secs: f64) {
    if world.get::<Stat>(mob).is_none() {
        return;
    }
    let until = now(world) + secs;
    world.entity_mut(mob).insert(Rebuked { until });
}

/// `next_move_modifier *= 2` while rebuked.
fn rebuked_next_move(
    insert: On<Insert<NextMove>>,
    time: Res<Time>,
    mut mobs: Query<&mut NextMove, With<Rebuked>>,
) {
    let Ok(mut next) = mobs.get_mut(insert.entity) else {
        return;
    };
    let now = time.elapsed_secs_f64();
    if next.0 > now {
        next.0 = now + (next.0 - now) * 2.0;
    }
}

/// tg `apply_status_effect(/datum/status_effect/freezing_blast)` (`STATUS_EFFECT_REPLACE`).
pub fn apply_freezing_blast(world: &mut World, mob: Entity) {
    if world.get::<Stat>(mob).is_none() {
        return;
    }
    let until = now(world) + FREEZING_SECS;
    if world.get::<FreezingBlast>(mob).is_none() {
        add_slowdown(world, mob, FREEZING_SLOWDOWN);
        alert(world, mob, AlertCategory::FreezingBlast, Some("frozen"));
    }
    world.entity_mut(mob).insert(FreezingBlast { until });
}

/// tg `apply_status_effect`/`remove_status_effect(/datum/status_effect/discoordinated)`.
pub fn set_discoordinated(world: &mut World, mob: Entity, on: bool) {
    if on && world.get::<Stat>(mob).is_some() {
        world.entity_mut(mob).insert(Discoordinated);
        alert(
            world,
            mob,
            AlertCategory::Discoordinated,
            Some("convulsing"),
        );
    } else if world.get::<Discoordinated>(mob).is_some() {
        world.entity_mut(mob).remove::<Discoordinated>();
        alert(world, mob, AlertCategory::Discoordinated, None);
    }
}

/// tg `apply_status_effect(/datum/status_effect/designated_target)` (`STATUS_EFFECT_REFRESH`).
pub fn apply_designated_target(world: &mut World, mob: Entity) {
    if world.get::<Stat>(mob).is_none() {
        return;
    }
    let until = now(world) + DESIGNATED_SECS;
    world.entity_mut(mob).insert(DesignatedTarget { until });
    alert(
        world,
        mob,
        AlertCategory::DesignatedTarget,
        Some("designated_target"),
    );
}

/// The ends of the timed flags; water washes the designated target off.
fn timed_upkeep(world: &mut World) {
    let now = now(world);
    for mob in ids::<Rebuked>(world) {
        if world.get::<Rebuked>(mob).is_some_and(|r| now >= r.until) {
            world.entity_mut(mob).remove::<Rebuked>();
        }
    }
    for mob in ids::<FreezingBlast>(world) {
        if world
            .get::<FreezingBlast>(mob)
            .is_some_and(|f| now >= f.until)
        {
            world.entity_mut(mob).remove::<FreezingBlast>();
            add_slowdown(world, mob, -FREEZING_SLOWDOWN);
            alert(world, mob, AlertCategory::FreezingBlast, None);
        }
    }
    for mob in ids::<DesignatedTarget>(world) {
        let wet = world
            .get::<crate::fire::WetStacks>(mob)
            .is_some_and(|w| w.stacks > 0.0);
        if wet
            || world
                .get::<DesignatedTarget>(mob)
                .is_some_and(|d| now >= d.until)
        {
            world.entity_mut(mob).remove::<DesignatedTarget>();
            alert(world, mob, AlertCategory::DesignatedTarget, None);
        }
    }
}

// --- Midas blight ----------------------------------------------------------------------------

/// The gold movespeed tiers of `midas_blight/tick` (`movespeed_modifier/status_effect/
/// midas_blight/*`), deciseconds a tile.
pub fn midas_slowdown(gold: f32) -> f32 {
    if gold < 50.0 {
        0.5
    } else if gold < 100.0 {
        1.0
    } else if gold < 200.0 {
        1.5
    } else {
        2.0
    }
}

/// tg `apply_status_effect(/datum/status_effect/midas_blight, duration)` (`REPLACE`): humans.
pub fn apply_midas_blight(world: &mut World, mob: Entity, secs: f32) {
    if world.get::<crate::chem::MobReagents>(mob).is_none() {
        return;
    }
    let now = now(world);
    let slowdown = world.get::<MidasBlight>(mob).map_or(0.0, |m| m.slowdown);
    world.entity_mut(mob).insert(MidasBlight {
        until: now + f64::from(secs),
        slowdown,
        next_tick: now + MIDAS_TICK_SECS,
    });
    alert(world, mob, AlertCategory::MidasBlight, Some("midas_blight"));
}

fn end_midas(world: &mut World, mob: Entity) {
    if let Some(m) = world.get::<MidasBlight>(mob).copied() {
        add_slowdown(world, mob, -m.slowdown);
    }
    world.entity_mut(mob).remove::<MidasBlight>();
    alert(world, mob, AlertCategory::MidasBlight, None);
}

fn midas_tick(world: &mut World) {
    let now = now(world);
    let secs = MIDAS_TICK_SECS as f32;
    for mob in ids::<MidasBlight>(world) {
        let Some(m) = world.get::<MidasBlight>(mob).copied() else {
            continue;
        };
        if now >= m.until {
            end_midas(world, mob);
            continue;
        }
        if now < m.next_tick {
            continue;
        }
        if let Some(mut mb) = world.get_mut::<MidasBlight>(mob) {
            mb.next_tick += MIDAS_TICK_SECS;
        }
        // `adjust_blood_volume(-5 * seconds, minimum = BLOOD_VOLUME_SURVIVE + 50)`.
        if let Some(mut blood) = world.get_mut::<crate::medical::blood::Blood>(mob)
            && blood.volume > MIDAS_MIN_BLOOD
        {
            blood.volume = (blood.volume - 5.0 * secs).max(MIDAS_MIN_BLOOD);
        }
        if let Some(chem) = world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone())
            && let Some(id) = chem.id(CURSED_GOLD)
            && let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob)
        {
            reagents.blood.add_reagent(
                &chem,
                id,
                MIDAS_GOLD_PER_SEC * secs,
                300.0,
                None,
                None,
                true,
                false,
            );
        }
        // `get_reagent_amount(/datum/reagent/gold, REAGENT_SUB_TYPE)`.
        let gold = crate::chem::blood_amount(world, mob, "/datum/reagent/gold")
            + crate::chem::blood_amount(world, mob, CURSED_GOLD);
        let wanted = midas_slowdown(gold);
        if wanted != m.slowdown {
            add_slowdown(world, mob, wanted - m.slowdown);
            if let Some(mut mb) = world.get_mut::<MidasBlight>(mob) {
                mb.slowdown = wanted;
            }
        }
        if dead(world, mob) {
            end_midas(world, mob);
        }
    }
}

// --- Rust corruption -----------------------------------------------------------------------

/// `/datum/status_effect/rust_corruption` (a heretic's rusted turf), ticking every 2 s.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct RustCorruption {
    next_tick: f64,
}

/// tg `apply_status_effect`/`remove_status_effect(/datum/status_effect/rust_corruption)`.
pub fn set_rust_corruption(world: &mut World, mob: Entity, on: bool) {
    if !on {
        world.entity_mut(mob).remove::<RustCorruption>();
    } else if world.get::<RustCorruption>(mob).is_none() && world.get::<Stat>(mob).is_some() {
        let next_tick = now(world) + 2.0;
        world.entity_mut(mob).insert(RustCorruption { next_tick });
    }
}

/// `rust_corruption/tick`: silicons and bots take 10 brute a second; others lose 0.75 units of
/// every reagent a second.
fn rust_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<RustCorruption>(world) {
        let due = world
            .get::<RustCorruption>(mob)
            .is_some_and(|r| now >= r.next_tick);
        if !due {
            continue;
        }
        if let Some(mut r) = world.get_mut::<RustCorruption>(mob) {
            r.next_tick += 2.0;
        }
        if world.get::<crate::silicon::Silicon>(mob).is_some()
            || world.get::<crate::bots::Bot>(mob).is_some()
        {
            crate::medical::diseases::damage(world, mob, DamageType::Brute, 20.0);
            continue;
        }
        if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) {
            reagents.blood.remove_all(1.5);
        }
    }
}

// --- Strandling ------------------------------------------------------------------------------

/// tg `apply_status_effect(/datum/status_effect/strandling)` (`STATUS_EFFECT_UNIQUE`).
pub fn apply_strandling(world: &mut World, mob: Entity) -> bool {
    if world.get::<Strandling>(mob).is_some() || world.get::<Respiration>(mob).is_none() {
        return false;
    }
    let next_tick = now(world) + STRAND_TICK_SECS;
    world.entity_mut(mob).insert(Strandling { next_tick });
    alert(world, mob, AlertCategory::Strandling, Some("strandling"));
    true
}

/// `strandling/on_breathe`: a breath lost each breath, unless a breathing tube bypasses it.
fn strandling_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<Strandling>(world) {
        let due = world
            .get::<Strandling>(mob)
            .is_some_and(|s| now >= s.next_tick);
        if !due {
            continue;
        }
        if let Some(mut s) = world.get_mut::<Strandling>(mob) {
            s.next_tick += STRAND_TICK_SECS;
        }
        // shortcut: no breathing tube implant (`ORGAN_SLOT_BREATHING_TUBE`) to bypass it.
        if dead(world, mob) {
            continue;
        }
        if let Some(mut r) = world.get_mut::<Respiration>(mob) {
            r.losebreath += 1.0;
        }
    }
}

/// `strandling/try_remove_effect`: by hand 3.5 s, with a tool 40% of it.
fn try_remove_strand(
    world: &mut World,
    client: Entity,
    user: Entity,
    victim: Entity,
    tool: Option<Entity>,
) {
    if status_effects::incapacitation(world, user).incapacitated
        || status_effects::incapacitation(world, user).hands_blocked
        || world.get::<DoAfter>(user).is_some()
    {
        return;
    }
    let what = if tool.is_some() {
        "срезать"
    } else {
        "снять"
    };
    let (u, v) = (name_of(world, user), name_of(world, victim));
    let whose = if user == victim {
        "своей".to_owned()
    } else {
        v
    };
    visible(
        world,
        user,
        &format!("Вы пытаетесь {what} нить с шеи {whose}..."),
        &format!("{u} пытается {what} нить с шеи {whose}..."),
        Span::Notice,
    );
    let delay = STRAND_REMOVE_DS
        * if tool.is_some() {
            STRAND_TOOL_MULT
        } else {
            1.0
        };
    crate::tools::do_after(
        world,
        Actor {
            client,
            mob: user,
            item: tool,
        },
        victim,
        delay,
        0,
        crate::construction::Job::Curse(CurseJob::RemoveStrand),
    );
}

/// Wirecutters on a strangled mob (`COMSIG_ATOM_TOOL_ACT(TOOL_WIRECUTTER)`).
fn cut_strand(
    world: &mut World,
    client: Entity,
    user: Entity,
    held: Option<Entity>,
    target: Entity,
) -> bool {
    let Some(tool) = held else {
        return false;
    };
    if world.get::<Strandling>(target).is_none()
        || world
            .get::<ss13_mapload::DmTool>(tool)
            .is_none_or(|t| t.behaviour != "cutters" && t.behaviour != "wirecutter")
    {
        return false;
    }
    try_remove_strand(world, client, user, target, Some(tool));
    true
}

/// Empty-handed help clicks (`COMSIG_CARBON_PRE_MISC_HELP`) on a strangled mob, oneself too.
fn help_clicks(world: &mut World, mut clicks: Local<MessageCursor<crate::click::ClickResolved>>) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (user, victim) = (click.mob, click.target);
        if click.modifiers.right
            || world.get::<Strandling>(victim).is_none()
            || world.get::<crate::afflictions::Choking>(victim).is_some()
            || world
                .resource::<crate::medical::MedicalHandled>()
                .0
                .contains(&user)
            || world
                .get::<crate::combat::CombatMode>(user)
                .is_some_and(|c| c.0)
            || world
                .get::<crate::inventory::Inventory>(user)
                .is_some_and(|i| i.active_item().is_some())
        {
            continue;
        }
        world
            .resource_mut::<crate::medical::MedicalHandled>()
            .0
            .insert(user);
        try_remove_strand(world, click.client, user, victim, None);
    }
}

/// `alert/status_effect/ants/Click`: shake them off (2 s, 10 ants).
fn alert_clicks(
    world: &mut World,
    mut acts: Local<MessageCursor<ss13_tgui::server::UiActed<crate::hud::AlertHudUi>>>,
) {
    let acts: Vec<(Entity, Entity, Entity, crate::breath::AlertClick)> = acts
        .read(world.resource::<Messages<ss13_tgui::server::UiActed<crate::hud::AlertHudUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, click) in acts {
        if actor != source
            || crate::breath::clicked_alert(world, actor, &click) != Some(AlertCategory::Ants)
            || status_effects::incapacitation(world, actor).incapacitated
            || world.get::<DoAfter>(actor).is_some()
        {
            continue;
        }
        tell_mob(
            world,
            actor,
            Span::Notice,
            "Вы начинаете стряхивать муравьёв!",
        );
        crate::tools::do_after(
            world,
            Actor {
                client,
                mob: actor,
                item: None,
            },
            actor,
            ANTS_SHAKE_DS,
            0,
            crate::construction::Job::Curse(CurseJob::ShakeAnts),
        );
    }
}

/// The end of this module's `do_after`s (`crate::construction::complete`).
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CurseJob) {
    match job {
        CurseJob::ShakeAnts => {
            if let Some(mut ants) = world.get_mut::<Ants>(mob) {
                ants.remaining -= 10.0;
                tell_mob(
                    world,
                    mob,
                    Span::Notice,
                    "Вам удаётся стряхнуть часть муравьёв!",
                );
            }
        }
        CurseJob::RemoveStrand => {
            let victim = action.target;
            if world.get::<Strandling>(victim).is_none() {
                return;
            }
            world.entity_mut(victim).remove::<Strandling>();
            alert(world, victim, AlertCategory::Strandling, None);
            let (u, v) = (name_of(world, mob), name_of(world, victim));
            visible(
                world,
                mob,
                &format!("Вы снимаете нить с шеи {v}."),
                &format!("{u} снимает нить с шеи {v}."),
                Span::Notice,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(CombatRng(5));
        world
    }

    fn advance(world: &mut World, secs: f32) {
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(secs));
    }

    fn carbon(world: &mut World) -> Entity {
        world
            .spawn((
                Stat::Conscious,
                Alerts::default(),
                Health::default(),
                crate::chem::MobReagents {
                    blood: crate::chem::mob_holder(),
                    stomach: crate::chem::mob_holder(),
                    drunk: 0.0,
                },
            ))
            .id()
    }

    #[test]
    fn necropolis_curse_stacks_flags_and_time() {
        let mut world = world();
        let mob = carbon(&mut world);
        apply_necropolis_curse(&mut world, mob, Some(CURSE_WASTING));
        apply_necropolis_curse(&mut world, mob, Some(CURSE_BLINDING));
        let curse = *world.get::<NecropolisCurse>(mob).unwrap();
        assert_eq!(curse.flags, CURSE_WASTING | CURSE_BLINDING);
        assert_eq!(curse.until, NECRO_SECS + NECRO_EXTRA_SECS);
        advance(&mut world, 5.0);
        necropolis_tick(&mut world);
        assert_eq!(world.get::<Health>(mob).unwrap().burn, 0.75);
    }

    #[test]
    fn trance_mutes_stuns_and_ends() {
        let mut world = world();
        let mob = carbon(&mut world);
        assert!(apply_trance(&mut world, mob, 3.0, true));
        assert!(mutes(&world, mob));
        advance(&mut world, 1.0);
        trance_tick(&mut world);
        let e = world
            .get::<crate::status_effects::StatusEffects>(mob)
            .unwrap();
        assert_eq!(e.stun, 6.0);
        assert_eq!(e.dizziness, 40.0);
        advance(&mut world, 2.0);
        trance_tick(&mut world);
        assert!(!mutes(&world, mob));
    }

    #[test]
    fn ants_bite_by_number_and_shake_off() {
        let mut world = world();
        let mob = carbon(&mut world);
        apply_ants(&mut world, mob, 100.0, true);
        assert_eq!(world.get::<Ants>(mob).unwrap().remaining, 100.0);
        apply_ants(&mut world, mob, 20.0, false);
        assert_eq!(world.get::<Ants>(mob).unwrap().remaining, 120.0);
        world.get_mut::<Ants>(mob).unwrap().remaining = 1.0;
        advance(&mut world, 1.0);
        ants_tick(&mut world);
        assert!(world.get::<Ants>(mob).is_none(), "the last ant left");
        assert!(world.get::<Health>(mob).unwrap().brute >= 0.1);
    }

    #[test]
    fn freezing_blast_slows_for_five_seconds() {
        let mut world = world();
        let mob = carbon(&mut world);
        apply_freezing_blast(&mut world, mob);
        apply_freezing_blast(&mut world, mob);
        assert_eq!(world.get::<MoveSpeedModifier>(mob).unwrap().0, 1.0);
        advance(&mut world, 5.0);
        timed_upkeep(&mut world);
        assert_eq!(world.get::<MoveSpeedModifier>(mob).unwrap().0, 0.0);
    }

    #[test]
    fn midas_tiers_follow_the_gold() {
        assert_eq!(midas_slowdown(10.0), 0.5);
        assert_eq!(midas_slowdown(60.0), 1.0);
        assert_eq!(midas_slowdown(150.0), 1.5);
        assert_eq!(midas_slowdown(250.0), 2.0);
    }
}
