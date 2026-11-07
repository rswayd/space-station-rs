//! Debuff status effects of items, creatures and gear (server), after tg
//! `code/datums/status_effects/debuffs/choke.dm`, `terrified.dm`, `spacer.dm`,
//! `slime/slimed.dm` (with the end of `slime/slime_leech.dm`) and `debuffs.dm`
//! (`crusher_mark`, `stacking/saw_bleed`, `neck_slice`, BandaStation's mute on the slit throat).
//!
//! - [`Choking`] ([`apply_choke`]): the item is inside the carbon; it cannot speak
//!   (`TRAIT_MUTE`, `crate::medical::traumas::refresh`) or eat, loses a breath a second
//!   (burning: 2 burn to the head and 2 stamina a second too), gags, has the `choke` moodlet and
//!   an examine line. The Heimlich: an empty-handed, non-combat click by someone else with two
//!   free hands, 7 s; the victim's chest gets a severe blunt wound and the item comes up. The
//!   effect may end on its own (`vomit_delay`) with a forced vomit.
//! - [`Terrified`] ([`apply_terrified`]): the alert, a scream, 100 terror on
//!   `crate::medical::fear`'s [`crate::medical::fear::Fearful`] (135 more each time again); the
//!   dark around the mob builds 15 terror a second; at no terror the fear and the effect end.
//! - [`GravitySickness`] / [`GravityWellness`] ([`set_spacer`]): the spacer's effects in and out
//!   of gravity (one replaces the other).
//! - [`Slimed`] ([`apply_slimed`]): 10 stacks of slime eat 2-4 brute a second until washed off
//!   (wet stacks, or a click on the alert: 1.5 s per stack) or the body is down to 10 health.
//!   A feeding slime lets go ([`leech_ended`]): unless the bio armor holds, the meal is slimed.
//! - [`CrusherMark`] ([`apply_crusher_mark`], [`detonate_crusher_mark`]), the saw bleeds
//!   ([`add_saw_bleed`]) and the slit throat ([`apply_neck_slice`]).
// shortcut: no choking camera shake, ash particles or `TRAIT_NOBREATH` pause; the item drops on
// the tile (tg throws it 2 tiles); the 0.2 brute per Heimlich tick lands at the end (14), no
// pull/facing checks during it, no samaritan award; the choke and slime survive a full heal.
// The spacer's disgust (no disgust in the port), moodlets and speed modifiers are the quirk's
// (not ported); the knockdown multiplier of the wellness is not applied. The slimed particles,
// crusher underlay, bleed overlays and splatters are not drawn. `getarmor(null, BIO)` reads
// the chest.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::UiActed;

use crate::breath::{AlertCategory, Alerts, Respiration};
use crate::chem::{name_of, tell_mob, visible};
use crate::combat::{CombatMode, CombatRng, NextMove};
use crate::health::{DamageType, Health, HealthSystems, Stat};
use crate::inventory::Inventory;
use crate::mood::MoodEvent;
use crate::status_effects::StatusSystems;
use crate::tools::{Actor, DoAfter};
use crate::zone::BodyZone;

/// `/datum/status_effect/choke::tick_interval`; the Heimlich's `do_after` (deciseconds).
const CHOKE_TICK_SECS: f64 = 0.2;
pub const HEIMLICH_DS: f32 = 70.0;
/// `/datum/looping_sound/choking`: `mid_length`.
const GAG_SECS: f64 = 1.6;
const GAGS: [&str; 5] = [
    "sound/mobs/humanoids/human/gag_vomit/gag1.ogg",
    "sound/mobs/humanoids/human/gag_vomit/gag2.ogg",
    "sound/mobs/humanoids/human/gag_vomit/gag3.ogg",
    "sound/mobs/humanoids/human/gag_vomit/gag4.ogg",
    "sound/mobs/humanoids/human/gag_vomit/gag5.ogg",
];
pub const CRACK_VOMIT: &str = "sound/mobs/humanoids/human/gag_vomit/crack_vomit.ogg";
pub const DESECRATION: [&str; 3] = [
    "sound/effects/desecration/desecration-01.ogg",
    "sound/effects/desecration/desecration-02.ogg",
    "sound/effects/desecration/desecration-03.ogg",
];
/// `TERROR_INITIAL_AMOUNT`, `STACK_TERROR_AMOUNT`.
pub const TERROR_INITIAL: f32 = 100.0;
pub const TERROR_STACK: f32 = 135.0;
/// `slimed`: `slime_stacks`, `tick_interval`, `MIN_HEALTH`, `MIN_WATER_STACKS`, the wash.
pub const SLIME_STACKS: f32 = 10.0;
const SLIMED_TICK_SECS: f64 = 3.0;
const SLIMED_MIN_HEALTH: f32 = 10.0;
const SLIMED_MIN_WATER: f32 = 5.0;
const SLIME_WASH_DS: f32 = 15.0;
/// `crusher_mark`: `duration`, `ready_delay`, `detonation_damage`; `MOB_SIZE_LARGE`.
pub const CRUSHER_MARK_SECS: f64 = 30.0;
pub const CRUSHER_READY_SECS: f64 = 0.8;
pub const CRUSHER_DETONATION: f32 = 50.0;
const MOB_SIZE_LARGE: u8 = 3;
/// `stacking/saw_bleed`: `tick_interval`, `delay_before_decay` (deciseconds, in seconds).
const SAW_TICK_SECS: f64 = 0.6;
const SAW_DECAY_DELAY_SECS: f64 = 0.5;
/// `neck_slice`: the default `tick_interval`.
const NECK_TICK_SECS: f64 = 1.0;

/// `/datum/mood_event/choke` (BandaStation's text).
const CHOKE_MOOD: MoodEvent = MoodEvent {
    id: "choke",
    description: "Я НЕ МОГУ ДЫШАТЬ!!!",
    mood_change: -10.0,
    timeout: 0.0,
    hidden: false,
};

/// `/datum/status_effect/choke`: the item stuck in the throat (held as a part of the mob).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Choking {
    pub item: Entity,
    /// `flaming` until this game time (0: not burning).
    pub flaming_until: f64,
    /// The `vomit_delay` end, `None` for `STATUS_EFFECT_PERMANENT`.
    pub vomit_at: Option<f64>,
    next_tick: f64,
    next_gag: f64,
}

/// `/datum/status_effect/terrified` (its terror lives on the mob's `Fearful`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Terrified;

/// `/datum/status_effect/spacer/gravity_sickness`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GravitySickness {
    /// `seconds_active` (seconds; tg adds a tenth of a decisecond per second, a unit slip).
    pub seconds_active: f32,
    /// `nerfed_effects_because_planetary`.
    pub planetary: bool,
    next_tick: f64,
}

/// `/datum/status_effect/spacer/gravity_wellness`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GravityWellness {
    /// `seconds_in_nograv` (seconds, same unit fix).
    pub seconds_in_nograv: f32,
    next_tick: f64,
}

/// `/datum/status_effect/slimed`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Slimed {
    pub stacks: f32,
    next_tick: f64,
}

/// `/datum/status_effect/crusher_mark` on a large mob.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CrusherMark {
    pub until: f64,
    pub ready_at: f64,
    pub boosted: bool,
}

/// One `/datum/status_effect/stacking/saw_bleed` kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SawBleedKind {
    /// `saw_bleed` (the cleaving saw): 10 stacks burst for 200 brute.
    Saw,
    /// `saw_bleed/bloodletting` (the cursed katana): 7 stacks burst for 20 brute.
    Bloodletting,
}

impl SawBleedKind {
    /// `stack_threshold` (= `max_stacks`) and `bleed_damage`.
    pub fn threshold(self) -> u32 {
        match self {
            Self::Saw => 10,
            Self::Bloodletting => 7,
        }
    }

    pub fn damage(self) -> f32 {
        match self {
            Self::Saw => 200.0,
            Self::Bloodletting => 20.0,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The stacks of one bleed and its next decay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BleedStacks {
    pub stacks: u32,
    next_tick: f64,
}

/// The saw bleeds on a mob, by [`SawBleedKind`] (two separate tg effects).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SawBleeds(pub [Option<BleedStacks>; 2]);

/// `/datum/status_effect/neck_slice` (BandaStation: `TRAIT_MUTE`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct NeckSlice {
    next_tick: f64,
}

/// Timed actions of this module (`crate::construction::Job::Affliction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfflictionJob {
    Heimlich,
    WashSlime,
}

pub struct AfflictionsPlugin;

impl Plugin for AfflictionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            choke_clicks
                .after(crate::click::ResolveClicks)
                .after(crate::medical::MedicalClicks)
                .after(crate::security::security_clicks)
                .before(crate::combat::MeleeSystems)
                .before(crate::inventory::inventory_intents),
        )
        .add_systems(
            Update,
            alert_clicks.in_set(ss13_tgui::server::UiSystems::Handle),
        )
        .add_systems(
            Update,
            (
                choke_tick,
                terrified_upkeep,
                spacer_tick,
                slimed_tick,
                marks_tick,
                saw_bleed_tick,
                neck_slice_tick,
            )
                .chain()
                .after(StatusSystems)
                .before(HealthSystems),
        );
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn throw_alert(world: &mut World, mob: Entity, category: AlertCategory, state: &'static str) {
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        alerts.throw(category, state, 0);
    }
}

fn clear_alert(world: &mut World, mob: Entity, category: AlertCategory) {
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob)
        && alerts.get(category).is_some()
    {
        alerts.clear(category);
    }
}

fn play(world: &mut World, mob: Entity, sound: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
    }
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

fn damage(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    crate::medical::diseases::damage(world, mob, kind, amount);
}

fn ids<C: Component>(world: &mut World) -> Vec<Entity> {
    world
        .query_filtered::<Entity, With<C>>()
        .iter(world)
        .collect()
}

/// The examine lines of these effects (`get_examine_text`), from
/// `crate::status_effects::examine_lines`.
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<Choking>(mob).is_some() {
        lines.push((Span::BoldWarning, "Он задыхается!".to_owned()));
    }
    if world.get::<Slimed>(mob).is_some() {
        lines.push((Span::Warning, "Он покрыт пузырящейся слизью!".to_owned()));
    }
    if world.get::<NeckSlice>(mob).is_some() {
        lines.push((
            Span::Warning,
            "Его шея перерезана и обильно кровоточит!".to_owned(),
        ));
    }
}

/// `TRAIT_MUTE` of these effects, for `crate::medical::traumas::refresh`.
pub fn mutes(world: &World, mob: Entity) -> bool {
    world.get::<Choking>(mob).is_some() || world.get::<NeckSlice>(mob).is_some()
}

// --- Choking ---------------------------------------------------------------------------------

/// tg `apply_status_effect(/datum/status_effect/choke, item, flaming, vomit_delay)`: carbons
/// only, one at a time; `vomit_delay` `None` chokes until helped. True when it started.
pub fn apply_choke(
    world: &mut World,
    mob: Entity,
    item: Entity,
    flaming: bool,
    vomit_delay: Option<f32>,
) -> bool {
    if world.get::<crate::chem::MobReagents>(mob).is_none()
        || world.get::<Choking>(mob).is_some()
        || world.get_entity(item).is_err()
    {
        return false;
    }
    // `transferItemToLoc(item, owner)` / `forceMove(owner)`.
    crate::stock_parts::attach(world, item, mob);
    let now = now(world);
    let flaming_until = if flaming {
        let secs = f64::from(rng_range(world, 15, 25));
        now + vomit_delay.map_or(secs, |d| secs.min(f64::from(d)))
    } else {
        0.0
    };
    world.entity_mut(mob).insert(Choking {
        item,
        flaming_until,
        vomit_at: vomit_delay.map(|d| now + f64::from(d)),
        next_tick: now + CHOKE_TICK_SECS,
        next_gag: now,
    });
    crate::mood::add_event(world, mob, "choke", &CHOKE_MOOD);
    crate::medical::traumas::refresh(world, mob);
    let name = name_of(world, mob);
    visible(
        world,
        mob,
        "Вы пытаетесь вдохнуть, но что-то мешает! Вы задыхаетесь!",
        &format!("{name} пытается что-то сказать, но не может! Он задыхается!"),
        Span::Danger,
    );
    true
}

/// `choke/on_remove`: the moodlet and the mute go; `vomit_up` brings the item out when it is
/// still inside.
pub fn end_choke(world: &mut World, mob: Entity) {
    let Some(choke) = world.get::<Choking>(mob).copied() else {
        return;
    };
    world.entity_mut(mob).remove::<Choking>();
    crate::mood::clear_event(world, mob, "choke");
    crate::medical::traumas::refresh(world, mob);
    let inside = world
        .get::<crate::stock_parts::PartOf>(choke.item)
        .is_some_and(|p| p.0 == mob);
    if !inside || world.get::<crate::chem::MobReagents>(mob).is_none() {
        return;
    }
    // `vomit(VOMIT_CATEGORY_DEFAULT | MOB_VOMIT_FORCE, lost_nutrition = 20, distance = 2)`.
    crate::medical::diseases::vomit(world, mob, 20.0, true, false, 2);
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        crate::stock_parts::detach(world, choke.item, pos);
    }
}

/// `choke/tick` every 0.2 s: the item gone ends it (`remove_choke`, `hazard_moved`), the delay
/// ends it with a vomit; the living lose a breath a second and burn while it is aflame.
fn choke_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<Choking>(world) {
        let Some(choke) = world.get::<Choking>(mob).copied() else {
            continue;
        };
        let inside = world
            .get::<crate::stock_parts::PartOf>(choke.item)
            .is_some_and(|p| p.0 == mob);
        if !inside {
            world.entity_mut(mob).remove::<Choking>();
            crate::mood::clear_event(world, mob, "choke");
            crate::medical::traumas::refresh(world, mob);
            continue;
        }
        if choke.vomit_at.is_some_and(|at| now >= at) {
            end_choke(world, mob);
            continue;
        }
        if now < choke.next_tick {
            continue;
        }
        if let Some(mut c) = world.get_mut::<Choking>(mob) {
            c.next_tick += CHOKE_TICK_SECS;
        }
        // `should_do_effects`.
        if dead(world, mob) {
            continue;
        }
        let secs = CHOKE_TICK_SECS as f32;
        if let Some(mut r) = world.get_mut::<Respiration>(mob) {
            r.losebreath += secs;
        }
        if now < choke.flaming_until {
            if let (Some(mut health), Some(&stat)) =
                (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
            {
                let mut new = stat;
                crate::health::apply_damage_zone(
                    &mut health,
                    &mut new,
                    BodyZone::Head,
                    DamageType::Burn,
                    2.0 * secs,
                );
                world.entity_mut(mob).insert(health);
                if new != stat {
                    world.entity_mut(mob).insert(new);
                }
            }
            crate::status_effects::adjust_stamina(world, mob, 2.0 * secs);
        }
        if now >= choke.next_gag {
            let gag = GAGS[rng_range(world, 0, 4) as usize];
            play(world, mob, gag, 90.0);
            if let Some(mut c) = world.get_mut::<Choking>(mob) {
                c.next_gag = now + GAG_SECS;
            }
        }
    }
}

/// `COMSIG_CARBON_ATTEMPT_EAT`: a choking mob cannot get food down. True when blocked (the
/// eater is told).
pub fn blocks_eating(world: &mut World, eater: Entity) -> bool {
    if world.get::<Choking>(eater).is_none() {
        return false;
    }
    tell_mob(world, eater, Span::Warning, "Не получается проглотить!");
    true
}

/// `heimlich` before work and during it (`thrusting_continues`): two free hands for a carbon
/// helper, a living victim still choking.
fn thrusting_continues(world: &mut World, helper: Entity, victim: Entity) -> bool {
    if let Some(inv) = world.get::<Inventory>(helper)
        && inv.hands.iter().any(Option::is_some)
    {
        tell_mob(world, helper, Span::Warning, "Нужны две свободные руки!");
        return false;
    }
    if dead(world, victim) {
        tell_mob(world, helper, Span::Warning, "Слишком поздно...");
        return false;
    }
    world.get::<Choking>(victim).is_some()
}

/// `COMSIG_CARBON_PRE_HELP` / `COMSIG_CARBON_PRE_MISC_HELP` of a choking mob: the help click
/// becomes the Heimlich. Runs before melee, which skips the taken clicks.
fn choke_clicks(world: &mut World, mut clicks: Local<MessageCursor<crate::click::ClickResolved>>) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    let t = now(world);
    for click in clicks {
        let (helper, victim) = (click.mob, click.target);
        if helper == victim || click.modifiers.right || world.get::<Choking>(victim).is_none() {
            continue;
        }
        let taken = world
            .resource::<crate::medical::MedicalHandled>()
            .0
            .contains(&helper)
            || world
                .resource::<crate::security::MeleeHandled>()
                .0
                .contains(&helper);
        if taken
            || world.get::<CombatMode>(helper).is_some_and(|c| c.0)
            || world.get::<NextMove>(helper).is_some_and(|n| t < n.0)
            || world.get::<Stat>(helper) != Some(&Stat::Conscious)
            || world
                .get::<Inventory>(helper)
                .is_some_and(|i| i.active_item().is_some())
        {
            continue;
        }
        world
            .resource_mut::<crate::medical::MedicalHandled>()
            .0
            .insert(helper);
        heimlich(world, click.client, helper, victim);
    }
}

/// `choke/heimlich`.
fn heimlich(world: &mut World, client: Entity, helper: Entity, victim: Entity) {
    if world.get::<DoAfter>(helper).is_some() {
        tell_mob(world, helper, Span::Warning, "Вы уже помогаете!");
        return;
    }
    if !thrusting_continues(world, helper, victim) {
        return;
    }
    let (h, v) = (name_of(world, helper), name_of(world, victim));
    let standing = world
        .get::<crate::health::BodyPosition>(victim)
        .is_none_or(|b| !b.lying());
    let carbon = world.get::<crate::chem::MobReagents>(helper).is_some();
    let others = if carbon && standing {
        format!("{h} обхватывает {v} руками за живот и начинает резко давить кулаками на себя!")
    } else {
        format!("{h} кладёт руки на спину {v} и начинает с силой бить по ней!")
    };
    visible(
        world,
        helper,
        &format!("Вы начинаете выбивать застрявшее из {v}!"),
        &others,
        Span::Warning,
    );
    tell_mob(
        world,
        victim,
        Span::BoldWarning,
        &format!("{h} изо всех сил давит вам на грудь. Как же больно!"),
    );
    let actor = Actor {
        client,
        mob: helper,
        item: None,
    };
    crate::tools::do_after(
        world,
        actor,
        victim,
        HEIMLICH_DS,
        0,
        crate::construction::Job::Affliction(AfflictionJob::Heimlich),
    );
}

/// The end of this module's `do_after`s (`crate::construction::complete`).
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: AfflictionJob) {
    match job {
        AfflictionJob::Heimlich => complete_heimlich(world, mob, action.target),
        AfflictionJob::WashSlime => {
            remove_slime_stacks(world, mob, 1.0);
            if world.get::<Slimed>(mob).is_some() {
                start_wash(world, action.client, mob);
            }
        }
    }
}

fn complete_heimlich(world: &mut World, helper: Entity, victim: Entity) {
    if !thrusting_continues(world, helper, victim) {
        return;
    }
    let Some(choke) = world.get::<Choking>(victim).copied() else {
        return;
    };
    // 0.2 brute each of the 70 ticks (`thrusting_continues`).
    damage(world, victim, DamageType::Brute, 14.0);
    let (v, item) = (name_of(world, victim), name_of(world, choke.item));
    visible(
        world,
        victim,
        "Вы выблёвываете эту проклятую пробку. ВЫ МОЖЕТЕ ДЫШАТЬ! Сломанная грудь — та ещё цена.",
        &format!("{v} выблёвывает {item}. Он выживет!"),
        Span::GreenText,
    );
    // `cause_wound_of_type_and_severity(WOUND_BLUNT, chest, WOUND_SEVERITY_SEVERE)`.
    if let Some(mut limbs) = world.get_mut::<crate::medical::wounds::Limbs>(victim) {
        crate::medical::wounds::apply_wound(
            limbs.limb_mut(BodyZone::Chest),
            crate::medical::wounds::WoundKind::Hairline,
        );
    }
    play(world, victim, CRACK_VOMIT, 120.0);
    end_choke(world, victim);
}

// --- Terrified -------------------------------------------------------------------------------

/// tg `apply_status_effect(/datum/status_effect/terrified)` (`STATUS_EFFECT_REFRESH`): the first
/// time the alert, a message, a scream and 100 terror; again 135 more.
pub fn apply_terrified(world: &mut World, mob: Entity) {
    if dead(world, mob) {
        return;
    }
    if world.get::<Terrified>(mob).is_some() {
        crate::medical::fear::add_terrified(world, mob, TERROR_STACK);
        return;
    }
    world.entity_mut(mob).insert(Terrified);
    throw_alert(world, mob, AlertCategory::Terrified, "terrified");
    tell_mob(
        world,
        mob,
        Span::Warning,
        "Тьма смыкается вокруг вас, тени пляшут на краю зрения... Кажется, за вами кто-то следит!",
    );
    crate::medical::diseases::emote(world, mob, "scream");
    crate::medical::fear::add_terrified(world, mob, TERROR_INITIAL);
}

/// The terror source gone (no terror left): the effect and its alert end.
fn terrified_upkeep(world: &mut World) {
    for mob in ids::<Terrified>(world) {
        let gone = world
            .get::<crate::medical::fear::Fearful>(mob)
            .is_none_or(|f| !f.terrified);
        if gone {
            world.entity_mut(mob).remove::<Terrified>();
            clear_alert(world, mob, AlertCategory::Terrified);
        }
    }
}

/// `nyctophobia/check_condition` of the terrified source: more unlit than lit open turfs in
/// `range(1)`; shadow species are never afraid of it.
pub fn dark_around(world: &World, mob: Entity) -> bool {
    let species = crate::species::def_of(world, mob).path;
    if ss13_mapload::fidelity::is_under(species, "/datum/species/shadow") {
        return false;
    }
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return false;
    };
    let Some(index) = world.get_resource::<ss13_world::SpatialIndex>() else {
        return false;
    };
    let (mut lit, mut unlit) = (0u32, 0u32);
    for dx in -1..=1 {
        for dy in -1..=1 {
            let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            let open = index.on_tile(p).iter().any(|&e| {
                crate::construction::path_of(world, e).is_some_and(|p| p.starts_with("/turf/open"))
            });
            if !open {
                continue;
            }
            // `check_lumcount_above(LIGHTING_TILE_IS_DARK)`.
            if crate::antag::nightmare::is_lit(crate::botany::lumcount(world, p), 0.2) {
                lit += 1;
            } else {
                unlit += 1;
            }
        }
    }
    lit < unlit
}

// --- Spacer ----------------------------------------------------------------------------------

/// Which spacer effect a mob has (`STATUS_EFFECT_REPLACE` on the shared id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spacer {
    None,
    Sickness { planetary: bool },
    Wellness,
}

/// tg `apply_status_effect(/datum/status_effect/spacer/*)` / `remove_status_effect`: carbons
/// only; one replaces the other.
pub fn set_spacer(world: &mut World, mob: Entity, effect: Spacer) {
    if world.get::<crate::chem::MobReagents>(mob).is_none() {
        return;
    }
    let now = now(world);
    world
        .entity_mut(mob)
        .remove::<(GravitySickness, GravityWellness)>();
    clear_alert(world, mob, AlertCategory::GravitySickness);
    match effect {
        Spacer::None => {}
        Spacer::Sickness { planetary } => {
            world.entity_mut(mob).insert(GravitySickness {
                seconds_active: 0.0,
                planetary,
                next_tick: now + 1.0,
            });
            throw_alert(world, mob, AlertCategory::GravitySickness, "paralysis");
        }
        Spacer::Wellness => {
            world.entity_mut(mob).insert(GravityWellness {
                seconds_in_nograv: 0.0,
                next_tick: now + 1.0,
            });
        }
    }
}

/// `gravity_sickness/tick` once a second: after 2 minutes off a planetary map, a floor of
/// stamina damage (5 a minute, up to 25) and, without internals, a breath lost every 10 s (up
/// to the minutes, at most 8).
pub fn sickness_step(world: &mut World, mob: Entity) {
    let Some(mut s) = world.get::<GravitySickness>(mob).copied() else {
        return;
    };
    s.seconds_active += 1.0;
    world.entity_mut(mob).insert(s);
    if s.planetary || s.seconds_active < 120.0 {
        return;
    }
    let minutes = (s.seconds_active / 60.0).floor();
    let stamina = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .map_or(0.0, |e| e.stamina);
    if stamina <= 0.0 {
        crate::status_effects::adjust_stamina(world, mob, (5.0 * minutes).min(25.0));
    }
    let internal = world
        .get::<crate::internals::Internals>(mob)
        .is_some_and(|i| i.0.is_some());
    if !internal
        && (s.seconds_active as u32).is_multiple_of(10)
        && let Some(mut r) = world.get_mut::<Respiration>(mob)
    {
        r.losebreath = (r.losebreath + 1.0).min(minutes).min(8.0);
    }
}

/// `gravity_wellness/tick` once a second: after 10 s without gravity 2 s less of every stun a
/// second, after a minute 4 stamina healed a second; gravity starts it over.
pub fn wellness_step(world: &mut World, mob: Entity) {
    let Some(mut w) = world.get::<GravityWellness>(mob).copied() else {
        return;
    };
    let nograv = crate::explosions::turf_of(world, mob)
        .is_some_and(|pos| !crate::multiz::has_gravity(world, pos));
    if !nograv {
        w.seconds_in_nograv = 0.0;
        world.entity_mut(mob).insert(w);
        return;
    }
    w.seconds_in_nograv += 1.0;
    world.entity_mut(mob).insert(w);
    if w.seconds_in_nograv >= 60.0 {
        crate::status_effects::adjust_stamina(world, mob, -4.0);
    }
    if w.seconds_in_nograv >= 10.0
        && let Some(mut e) = crate::status_effects::effects_mut(world, mob)
    {
        e.adjust_all_immobility(-2.0);
    }
}

fn spacer_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<GravitySickness>(world) {
        let due = world
            .get::<GravitySickness>(mob)
            .is_some_and(|s| now >= s.next_tick);
        if due {
            if let Some(mut s) = world.get_mut::<GravitySickness>(mob) {
                s.next_tick += 1.0;
            }
            sickness_step(world, mob);
        }
    }
    for mob in ids::<GravityWellness>(world) {
        let due = world
            .get::<GravityWellness>(mob)
            .is_some_and(|s| now >= s.next_tick);
        if due {
            if let Some(mut s) = world.get_mut::<GravityWellness>(mob) {
                s.next_tick += 1.0;
            }
            wellness_step(world, mob);
        }
    }
}

// --- Slime -----------------------------------------------------------------------------------

/// `get_organic_health`.
fn organic_health(world: &World, mob: Entity) -> f32 {
    world.get::<Health>(mob).map_or(0.0, Health::health)
}

/// tg `apply_status_effect(/datum/status_effect/slimed)` (`STATUS_EFFECT_UNIQUE`). True when it
/// started.
pub fn apply_slimed(world: &mut World, mob: Entity) -> bool {
    if world.get::<Slimed>(mob).is_some()
        || dead(world, mob)
        || organic_health(world, mob) <= SLIMED_MIN_HEALTH
    {
        return false;
    }
    let now = now(world);
    world.entity_mut(mob).insert(Slimed {
        stacks: SLIME_STACKS,
        next_tick: now + SLIMED_TICK_SECS,
    });
    throw_alert(world, mob, AlertCategory::Slimed, "slimed");
    tell_mob(
        world,
        mob,
        Span::Danger,
        "Вас покрыл толстый слой слизи! Найдите способ её смыть!",
    );
    true
}

fn end_slimed(world: &mut World, mob: Entity) {
    world.entity_mut(mob).remove::<Slimed>();
    clear_alert(world, mob, AlertCategory::Slimed);
}

/// `slimed/remove_stacks`.
pub fn remove_slime_stacks(world: &mut World, mob: Entity, stacks: f32) {
    let Some(mut s) = world.get_mut::<Slimed>(mob) else {
        return;
    };
    s.stacks -= stacks;
    if s.stacks <= 0.0 {
        end_slimed(world, mob);
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Вам удаётся полностью смыть с себя слой слизи.",
        );
        return;
    }
    if prob(world, 10.0) {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "Слой слизи понемногу истончается.",
        );
    }
}

/// `slimed/tick` every 3 s: a weakened body sheds it; wet stacks wash it off (5 a second for a
/// stack a second), else 2-4 brute a second.
fn slimed_tick(world: &mut World) {
    let now = now(world);
    let secs = SLIMED_TICK_SECS as f32;
    for mob in ids::<Slimed>(world) {
        let due = world.get::<Slimed>(mob).is_some_and(|s| now >= s.next_tick);
        if !due {
            continue;
        }
        if let Some(mut s) = world.get_mut::<Slimed>(mob) {
            s.next_tick += SLIMED_TICK_SECS;
        }
        if organic_health(world, mob) <= SLIMED_MIN_HEALTH {
            end_slimed(world, mob);
            tell_mob(
                world,
                mob,
                Span::Warning,
                "Вы чувствуете, как слой слизи сползает с вашего ослабевшего тела.",
            );
            continue;
        }
        let wet = world
            .get::<crate::fire::WetStacks>(mob)
            .map_or(0.0, |w| w.stacks);
        if wet > SLIMED_MIN_WATER * secs {
            crate::fire::adjust_wet_stacks(world, mob, -5.0 * secs);
            remove_slime_stacks(world, mob, secs);
            if world.get::<Slimed>(mob).is_none() {
                continue;
            }
        }
        let brute = rng_range(world, 2, 4) as f32 * secs;
        damage(world, mob, DamageType::Brute, brute);
        let pain = {
            let mut rng = world.get_resource_or_init::<CombatRng>();
            crate::medical::wounds::spt_prob(&mut rng, 10.0, secs).then(|| rng.range(0, 2))
        };
        if let Some(i) = pain {
            const LINES: [&str; 3] = [
                "Всё ваше тело кричит от боли",
                "Кажется, с вас слезает кожа",
                "Кажется, ваше тело плавится",
            ];
            let text = format!("{}, пока слизь разъедает вас!", LINES[i as usize]);
            tell_mob(world, mob, Span::Danger, &text);
        }
    }
}

/// `alert/status_effect/slimed/can_wash`.
fn can_wash(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
        && world.get::<DoAfter>(mob).is_none()
        && world
            .get::<crate::fire::WetStacks>(mob)
            .is_none_or(|w| w.stacks <= 0.0)
}

/// One `do_after(owner, 1.5 SECONDS, owner)` of `remove_slime`.
fn start_wash(world: &mut World, client: Entity, mob: Entity) {
    if !can_wash(world, mob) {
        return;
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    crate::tools::do_after(
        world,
        actor,
        mob,
        SLIME_WASH_DS,
        0,
        crate::construction::Job::Affliction(AfflictionJob::WashSlime),
    );
}

/// `alert/status_effect/slimed/Click`: scrub the slime off, a stack per 1.5 s.
fn alert_clicks(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<crate::hud::AlertHudUi>>>,
) {
    let acts: Vec<(Entity, Entity, Entity, crate::breath::AlertClick)> = acts
        .read(world.resource::<Messages<UiActed<crate::hud::AlertHudUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, click) in acts {
        if actor != source {
            continue;
        }
        if crate::breath::clicked_alert(world, actor, &click) == Some(AlertCategory::Slimed) {
            tell_mob(world, actor, Span::Notice, "Вы начинаете счищать слизь...");
            start_wash(world, client, actor);
        }
    }
}

/// `slime_leech/on_apply`: the meal's alert (`crate::science::xenobio::start_feeding`).
pub fn leech_started(world: &mut World, meal: Entity) {
    throw_alert(world, meal, AlertCategory::SlimeLeech, "slime_leech");
}

/// `slime_leech/on_buckle_end` (`crate::science::xenobio::stop_feeding`): unless the bio armor
/// holds (`prob(100 - armor)`), the meal is slimed.
pub fn leech_ended(world: &mut World, meal: Entity) {
    if world.get_entity(meal).is_err() {
        return;
    }
    clear_alert(world, meal, AlertCategory::SlimeLeech);
    let bio = crate::armor::getarmor(world, meal, BodyZone::Chest, "bio");
    if prob(world, 100.0 - bio) {
        apply_slimed(world, meal);
    }
}

// --- Mining and combat gear ------------------------------------------------------------------

/// tg `apply_status_effect(/datum/status_effect/crusher_mark, boosted)` (`STATUS_EFFECT_REFRESH`):
/// large mobs only; again, its 30 s start over. True when the mob is marked.
pub fn apply_crusher_mark(world: &mut World, mob: Entity, boosted: bool) -> bool {
    if dead(world, mob) || crate::mobs::pets::mob_size(world, mob) < MOB_SIZE_LARGE {
        return false;
    }
    let now = now(world);
    if let Some(mut mark) = world.get_mut::<CrusherMark>(mob) {
        mark.until = now + CRUSHER_MARK_SECS;
        return true;
    }
    world.entity_mut(mob).insert(CrusherMark {
        until: now + CRUSHER_MARK_SECS,
        ready_at: now + CRUSHER_READY_SECS,
        boosted,
    });
    true
}

/// The mark can be detonated (`ready_delay` passed).
pub fn crusher_mark_ready(world: &World, mob: Entity) -> bool {
    world
        .get::<CrusherMark>(mob)
        .is_some_and(|m| now(world) >= m.ready_at)
}

/// `crusher_mark/detonate`: 50 brute (bomb armor blocks), plus the crusher's `backstab_bonus`
/// on a melee hit that is boosted or from behind; the mark goes. The damage dealt, `None`
/// without a mark.
pub fn detonate_crusher_mark(
    world: &mut World,
    mob: Entity,
    backstab_bonus: Option<f32>,
    behind: bool,
) -> Option<f32> {
    let mark = world.get::<CrusherMark>(mob).copied()?;
    world.entity_mut(mob).remove::<CrusherMark>();
    let mut total = CRUSHER_DETONATION;
    if let Some(bonus) = backstab_bonus
        && (mark.boosted || behind)
    {
        total += bonus;
    }
    let armor = crate::armor::getarmor(world, mob, BodyZone::Chest, "bomb");
    let dealt = crate::armor::blocked_damage(total, armor);
    damage(world, mob, DamageType::Brute, dealt);
    Some(dealt)
}

fn marks_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<CrusherMark>(world) {
        let over = world
            .get::<CrusherMark>(mob)
            .is_some_and(|m| now >= m.until)
            || dead(world, mob);
        if over {
            world.entity_mut(mob).remove::<CrusherMark>();
        }
    }
}

/// tg `apply_status_effect(saw_bleed, stacks)` / `add_stacks`: up to the threshold, which bursts
/// for the kind's brute and ends it; new stacks put the decay off by 0.5 s. The dead gain none.
pub fn add_saw_bleed(world: &mut World, mob: Entity, kind: SawBleedKind, added: i32) {
    if dead(world, mob) {
        return;
    }
    let now = now(world);
    let mut bleeds = world.get::<SawBleeds>(mob).copied().unwrap_or_default();
    let mut stack = bleeds.0[kind.index()].unwrap_or(BleedStacks {
        stacks: 0,
        next_tick: now + SAW_TICK_SECS,
    });
    let stacks = (stack.stacks as i32 + added).min(kind.threshold() as i32);
    if stacks <= 0 {
        bleeds.0[kind.index()] = None;
    } else if stacks as u32 >= kind.threshold() {
        // `threshold_cross_effect`, consumed.
        bleeds.0[kind.index()] = None;
        play_desecration(world, mob);
        damage(world, mob, DamageType::Brute, kind.damage());
    } else {
        stack.stacks = stacks as u32;
        if added > 0 {
            stack.next_tick += SAW_DECAY_DELAY_SECS;
        }
        bleeds.0[kind.index()] = Some(stack);
    }
    if bleeds == SawBleeds::default() {
        world.entity_mut(mob).remove::<SawBleeds>();
    } else {
        world.entity_mut(mob).insert(bleeds);
    }
}

fn play_desecration(world: &mut World, mob: Entity) {
    let file = DESECRATION[rng_range(world, 0, 2) as usize];
    play(world, mob, file, 100.0);
}

/// `stacking/tick` every 0.6 s: a stack decays; death ends it.
fn saw_bleed_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<SawBleeds>(world) {
        if dead(world, mob) {
            world.entity_mut(mob).remove::<SawBleeds>();
            continue;
        }
        for kind in [SawBleedKind::Saw, SawBleedKind::Bloodletting] {
            let due = world
                .get::<SawBleeds>(mob)
                .and_then(|b| b.0[kind.index()])
                .is_some_and(|s| now >= s.next_tick);
            if !due {
                continue;
            }
            if let Some(mut b) = world.get_mut::<SawBleeds>(mob)
                && let Some(s) = b.0[kind.index()].as_mut()
            {
                s.next_tick += SAW_TICK_SECS;
            }
            add_saw_bleed(world, mob, kind, -1);
        }
    }
}

fn head_bleeding(world: &World, mob: Entity) -> Option<bool> {
    let limbs = world.get::<crate::medical::wounds::Limbs>(mob)?;
    let head = limbs.limb(BodyZone::Head);
    if head.missing {
        return None;
    }
    Some(head.wounds.iter().any(|w| {
        let def = w.def();
        def.wounding == crate::medical::wounds::WoundType::Slash
            && def.severity > 1
            && w.blood_flow > 0.0
    }))
}

/// tg `apply_status_effect(/datum/status_effect/neck_slice)` (`STATUS_EFFECT_UNIQUE`): humans
/// with a head; mute while it lasts (BandaStation). True when it started.
pub fn apply_neck_slice(world: &mut World, mob: Entity) -> bool {
    if world.get::<NeckSlice>(mob).is_some()
        || world.get::<crate::human::HumanLook>(mob).is_none()
        || head_bleeding(world, mob).is_none()
    {
        return false;
    }
    let now = now(world);
    world.entity_mut(mob).insert(NeckSlice {
        next_tick: now + NECK_TICK_SECS,
    });
    crate::medical::traumas::refresh(world, mob);
    true
}

/// `neck_slice/tick` every second: death, a lost head or no severe slash bleeding on it ends
/// it; else 10% to gasp, gag or choke.
fn neck_slice_tick(world: &mut World) {
    let now = now(world);
    for mob in ids::<NeckSlice>(world) {
        let due = world
            .get::<NeckSlice>(mob)
            .is_some_and(|n| now >= n.next_tick);
        if !due {
            continue;
        }
        if let Some(mut n) = world.get_mut::<NeckSlice>(mob) {
            n.next_tick += NECK_TICK_SECS;
        }
        if dead(world, mob) || head_bleeding(world, mob) != Some(true) {
            world.entity_mut(mob).remove::<NeckSlice>();
            crate::medical::traumas::refresh(world, mob);
            continue;
        }
        if prob(world, 10.0) {
            let emote = ["gasp", "gag", "choke"][rng_range(world, 0, 2) as usize];
            crate::medical::diseases::emote(world, mob, emote);
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
        world.insert_resource(CombatRng(3));
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
    fn choking_takes_the_item_in_and_ends_when_it_leaves() {
        let mut world = world();
        let mob = carbon(&mut world);
        let item = world.spawn(TilePos::new(1, 1, 0)).id();
        let animal = world.spawn(Stat::Conscious).id();
        assert!(
            !apply_choke(&mut world, animal, item, false, None),
            "carbons only"
        );
        assert!(apply_choke(&mut world, mob, item, true, None));
        assert!(
            !apply_choke(&mut world, mob, item, false, None),
            "one at a time"
        );
        assert!(world.get::<TilePos>(item).is_none());
        assert_eq!(
            world.get::<crate::stock_parts::PartOf>(item).unwrap().0,
            mob
        );
        assert!(mutes(&world, mob));
        let mut lines = Vec::new();
        examine_lines(&world, mob, &mut lines);
        assert_eq!(lines[0].1, "Он задыхается!");
        world
            .entity_mut(item)
            .remove::<crate::stock_parts::PartOf>();
        choke_tick(&mut world);
        assert!(world.get::<Choking>(mob).is_none());
        assert!(!mutes(&world, mob));
    }

    #[test]
    fn heimlich_needs_two_free_hands_and_a_living_victim() {
        let mut world = world();
        let victim = carbon(&mut world);
        let item = world.spawn_empty().id();
        assert!(apply_choke(&mut world, victim, item, false, None));
        let mut inv = Inventory::default();
        let helper = world.spawn((Stat::Conscious, inv.clone())).id();
        assert!(thrusting_continues(&mut world, helper, victim));
        inv.hands[0] = Some(item);
        world.entity_mut(helper).insert(inv);
        assert!(!thrusting_continues(&mut world, helper, victim));
        world.entity_mut(helper).insert(Inventory::default());
        world.entity_mut(victim).insert(Stat::Dead);
        assert!(!thrusting_continues(&mut world, helper, victim));
    }

    #[test]
    fn slime_washes_off_by_stacks_and_spares_the_weak() {
        let mut world = world();
        let mob = carbon(&mut world);
        assert!(apply_slimed(&mut world, mob));
        assert!(!apply_slimed(&mut world, mob), "unique");
        assert_eq!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::Slimed)
                .unwrap()
                .state,
            "slimed"
        );
        remove_slime_stacks(&mut world, mob, 9.0);
        assert_eq!(world.get::<Slimed>(mob).unwrap().stacks, 1.0);
        remove_slime_stacks(&mut world, mob, 1.0);
        assert!(world.get::<Slimed>(mob).is_none());
        assert!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::Slimed)
                .is_none()
        );
        let weak = carbon(&mut world);
        world.get_mut::<Health>(weak).unwrap().brute = 95.0;
        assert!(!apply_slimed(&mut world, weak), "10 health or less");
    }

    #[test]
    fn saw_bleed_bursts_at_its_threshold_and_decays() {
        let mut world = world();
        let mob = carbon(&mut world);
        add_saw_bleed(&mut world, mob, SawBleedKind::Bloodletting, 6);
        let stacks = |w: &World| {
            w.get::<SawBleeds>(mob)
                .and_then(|b| b.0[1])
                .map(|s| s.stacks)
        };
        assert_eq!(stacks(&world), Some(6));
        advance(&mut world, 1.2);
        saw_bleed_tick(&mut world);
        assert_eq!(stacks(&world), Some(5));
        add_saw_bleed(&mut world, mob, SawBleedKind::Bloodletting, 2);
        assert_eq!(stacks(&world), None, "burst at 7");
        assert_eq!(world.get::<Health>(mob).unwrap().brute, 20.0);
    }

    #[test]
    fn terrified_builds_terror_and_ends_with_it() {
        let mut world = world();
        let mob = carbon(&mut world);
        apply_terrified(&mut world, mob);
        let fear = |w: &World| w.get::<crate::medical::fear::Fearful>(mob).cloned();
        assert_eq!(fear(&world).unwrap().buildup, TERROR_INITIAL);
        assert!(fear(&world).unwrap().terrified);
        apply_terrified(&mut world, mob);
        assert_eq!(fear(&world).unwrap().buildup, TERROR_INITIAL + TERROR_STACK);
        assert_eq!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::Terrified)
                .unwrap()
                .state,
            "terrified"
        );
        let mut f = fear(&world).unwrap();
        f.buildup = 0.0;
        world.entity_mut(mob).insert(f);
        let mut rng = CombatRng(1);
        crate::medical::fear::tick(&mut world, mob, 2.0, &mut rng);
        assert!(fear(&world).is_none(), "no terror, no source left");
        terrified_upkeep(&mut world);
        assert!(world.get::<Terrified>(mob).is_none());
        assert!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::Terrified)
                .is_none()
        );
    }

    #[test]
    fn crusher_mark_is_for_large_mobs_and_detonates_once() {
        let mut world = world();
        let small = carbon(&mut world);
        assert!(!apply_crusher_mark(&mut world, small, false));
        assert_eq!(detonate_crusher_mark(&mut world, small, None, false), None);
    }

    #[test]
    fn gravity_sickness_drains_stamina_after_two_minutes() {
        let mut world = world();
        let mob = carbon(&mut world);
        set_spacer(&mut world, mob, Spacer::Sickness { planetary: false });
        assert!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::GravitySickness)
                .is_some()
        );
        for _ in 0..119 {
            sickness_step(&mut world, mob);
        }
        let stamina = |w: &World| {
            w.get::<crate::status_effects::StatusEffects>(mob)
                .map_or(0.0, |e| e.stamina)
        };
        assert_eq!(stamina(&world), 0.0);
        sickness_step(&mut world, mob);
        assert_eq!(stamina(&world), 10.0, "5 a minute at 2 minutes");
        set_spacer(&mut world, mob, Spacer::Wellness);
        assert!(world.get::<GravitySickness>(mob).is_none());
        assert!(world.get::<GravityWellness>(mob).is_some());
    }
}
