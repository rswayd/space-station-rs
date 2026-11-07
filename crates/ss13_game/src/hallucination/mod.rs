//! Hallucinations (server), after tg `code/modules/hallucination/` (`/datum/hallucination` and
//! its subtypes), `code/__HELPERS/hallucinations.dm` (`cause_hallucination`,
//! `get_random_hallucination`, `hallucination_pulse`) and
//! `code/datums/status_effects/debuffs/hallucination.dm`.
//!
//! - [`Hallucinating`] is `/datum/status_effect/hallucination`: its seconds left
//!   ([`adjust_hallucinations`], [`adjust_hallucinations_up_to`], [`set_hallucinations`]). Every
//!   tick (2 s) past its cooldown it causes a random hallucination of the tier its remaining time
//!   allows ([`random_hallucination`]: weighted, the tier and every one below), then waits
//!   20-80 s (shorter for long durations, a quarter of that after a hallucination that did not
//!   start).
//! - [`SanityHallucinations`] is `/datum/status_effect/hallucination/sanity`: on while the
//!   mood's sanity is crazy or insane (`crate::mood`), up to uncommon hallucinations every 4-8
//!   (crazy) or 2-4 (insane) minutes.
//! - [`cause_hallucination`] starts one [`Kind`] (`kinds`): only the hallucinator sees and hears
//!   it. Sounds are `playsound_local`, messages go to its own chat, fake alerts and HUD states
//!   are its own HUD, images are [`HallucinationImage`] entities only its client replicates
//!   (`VisibleToMobs`), delusions are alternate appearances of other mobs only it gets
//!   ([`delusion`]).
//! - Reagents: mindbreaker, bath salts, viper spider toxin and BZ metabolites add duration;
//!   synaptizine, diphen-synaptizine and haloperidol take it away ([`reagents`]).
//! - The Mass Hallucination event is `crate::events::mass_hallucination`.
// shortcut: only humans hallucinate (tg: every living mob without robotic or spirit biotypes);
// no `TRAIT_HALLUCINATION_IMMUNE`, `TRAIT_MADNESS_IMMUNE` or `TRAIT_FEARLESS` (no sources),
// no advanced health scanner line (scanners are `SCANPOWER_BASIC`), no airlock-bump shocks
// (no `COMSIG_CARBON_BUMPED_AIRLOCK_OPEN`), no limb self-check damage, no perceptomatrix
// variant, no admin give-hallucination verbs and no investigate log (a debug log instead).
// Images follow their mob tile by tile (no glide).

pub mod delusion;
pub mod kinds;
mod strings;

use std::collections::HashSet;

use bevy::prelude::*;
use ss13_mapload::{IconIndex, Opaque};
use ss13_net::chat::server::SendChat;
use ss13_net::server::{Invisibility, Networked, SeeGroups, VisibleToMobs};
use ss13_net::sound::server::SendSound;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::SoundEvent;
use ss13_shared::{Appearance, Layer, MouseOpacity, TilePos};
use ss13_world::SpatialIndex;

use crate::breath::{AlertCategory, Alerts};
use crate::combat::CombatRng;
use crate::health::Stat;
use crate::hud::{DOLL_LIMBS, HealthReadout};
use crate::mood::{Mood, SANITY_LEVEL_CRAZY, SANITY_LEVEL_INSANE};

pub use kinds::{Delusion, Kind, Tier};

/// The status effect's `tick_interval`, seconds.
pub const TICK_SECS: f64 = 2.0;
/// `lower_tick_interval`, `upper_tick_interval`, seconds.
const LOWER_INTERVAL: f64 = 20.0;
const UPPER_INTERVAL: f64 = 80.0;
/// `ABOVE_MOB_LAYER` and `GAME_PLANE`.
pub const ABOVE_MOB_LAYER: f32 = 4.1;
pub const GAME_PLANE: i16 = -4;
/// An invisibility nobody's `see_invisible` reaches: only [`VisibleToMobs`] shows the image.
const IMAGE_INVISIBILITY: u8 = u8::MAX;

/// `/datum/status_effect/hallucination` on a mob.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Hallucinating {
    /// `duration`: seconds left.
    pub remaining: f32,
    /// `hallucination_cooldown`: game time of the next hallucination (0: the first tick).
    pub next_at: f64,
}

/// `/datum/status_effect/hallucination/sanity` on a mob whose sanity is crazy or insane.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SanityHallucinations {
    pub level: u8,
    pub next_at: f64,
}

/// `/datum/status_effect/grouped/screwy_hud/*` and the fake health doll: what the owner's health
/// HUD shows instead of the truth (`crate::health` applies it).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ScrewyHud {
    /// Sources of `fake_dead` (priority 100), `fake_crit` (90), `fake_healthy` (10).
    pub dead: u8,
    pub crit: u8,
    pub healthy: u8,
    /// Fake doll severities 1..=5 in [`DOLL_LIMBS`] order (0: the real one).
    pub doll: [u8; 6],
}

impl ScrewyHud {
    /// tg `on_health_hud_updated` (the highest priority wins; nothing on the dead) and the doll's
    /// `on_bodypart_hud_update`.
    pub fn apply(&self, readout: &mut HealthReadout, stat: Stat) {
        for (limb, &fake) in readout.limbs.iter_mut().zip(&self.doll) {
            if fake > 0 {
                *limb = fake;
            }
        }
        if stat == Stat::Dead {
            return;
        }
        // shortcut: the readout has no separate health icon, so a fake death also greys out
        // the doll (tg overrides only `health7`).
        if self.dead > 0 {
            readout.dead = true;
        } else if self.crit > 0 {
            readout.health = 0.0;
        } else if self.healthy > 0 {
            readout.health = readout.max;
        }
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// An image only `seer`'s client sees (tg `client.images` and `/obj/effect/client_image_holder`),
/// on its own tile or following `follow`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HallucinationImage {
    pub seer: Entity,
    pub follow: Option<Entity>,
}

/// tg `addtimer` of hallucinations: closures run at their game time.
#[derive(Resource, Default)]
pub struct HallucinationTimers(Vec<(f64, Timed)>);

type Timed = Box<dyn FnOnce(&mut World) + Send + Sync>;

pub struct HallucinationPlugin;

impl Plugin for HallucinationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HallucinationTimers>().add_systems(
            Update,
            (sync_sanity, process, run_timers, follow_images).chain(),
        );
    }
}

pub(crate) use crate::tools::now;

pub(crate) fn rng(world: &mut World) -> CombatRng {
    crate::medical::fork_rng(world)
}

/// Runs `f` in `secs` seconds (tg `addtimer(CALLBACK(...), delay)`).
pub fn later(world: &mut World, secs: f64, f: impl FnOnce(&mut World) + Send + Sync + 'static) {
    let at = now(world) + secs;
    world
        .get_resource_or_init::<HallucinationTimers>()
        .0
        .push((at, Box::new(f)));
}

/// Whether `mob` still exists (tg `QDELETED(hallucinator)`).
pub(crate) fn exists(world: &World, mob: Entity) -> bool {
    world.get_entity(mob).is_ok()
}

// --- The status effect --------------------------------------------------------------------

/// Whether `mob` can get the hallucination status effect (`barred_biotypes`).
pub fn can_hallucinate(world: &World, mob: Entity) -> bool {
    use crate::species::biotype::{ROBOTIC, SPIRIT};
    let Ok(entity) = world.get_entity(mob) else {
        return false;
    };
    entity.contains::<crate::human::HumanLook>()
        && entity.contains::<Stat>()
        && crate::species::def_of(world, mob).biotypes & (ROBOTIC | SPIRIT) == 0
}

/// Seconds of hallucination `mob` has left (tg `get_timed_status_effect_duration`).
pub fn remaining(world: &World, mob: Entity) -> f32 {
    world.get::<Hallucinating>(mob).map_or(0.0, |h| h.remaining)
}

/// tg `adjust_timed_status_effect(duration, effect, max_duration)` on the remaining seconds
/// (`None`: no effect): the new remaining time, `None` when it ends or never starts.
pub fn adjusted(current: Option<f32>, mut secs: f32, max: Option<f32>) -> Option<f32> {
    if let Some(max) = max {
        if max <= 0.0 {
            return current;
        }
        secs = secs.min(max);
    }
    match current {
        Some(left) => {
            let new = match max {
                Some(max) if secs > 0.0 => {
                    if left >= max {
                        return Some(left);
                    }
                    left + (max - left).min(secs)
                }
                _ => left + secs,
            };
            (new > 0.0).then_some(new)
        }
        None => (secs > 0.0).then_some(secs),
    }
}

/// tg `set_timed_status_effect(duration, effect, only_if_higher)`.
pub fn set_to(current: Option<f32>, secs: f32, only_if_higher: bool) -> Option<f32> {
    match current {
        Some(left) if only_if_higher && secs <= left => Some(left),
        _ => (secs > 0.0).then_some(secs),
    }
}

fn store(world: &mut World, mob: Entity, new: Option<f32>) {
    if !exists(world, mob) {
        return;
    }
    let has = world.get::<Hallucinating>(mob).is_some();
    match new {
        None if has => {
            world.entity_mut(mob).remove::<Hallucinating>();
        }
        Some(left) if has => {
            if let Some(mut h) = world.get_mut::<Hallucinating>(mob) {
                h.remaining = left;
            }
        }
        Some(left) if can_hallucinate(world, mob) => {
            world.entity_mut(mob).insert(Hallucinating {
                remaining: left,
                next_at: 0.0,
            });
        }
        _ => {}
    }
}

fn current(world: &World, mob: Entity) -> Option<f32> {
    world.get::<Hallucinating>(mob).map(|h| h.remaining)
}

/// tg `adjust_hallucinations(duration)`.
pub fn adjust_hallucinations(world: &mut World, mob: Entity, secs: f32) {
    let new = adjusted(current(world, mob), secs, None);
    store(world, mob, new);
}

/// tg `adjust_hallucinations_up_to(duration, up_to)`.
pub fn adjust_hallucinations_up_to(world: &mut World, mob: Entity, secs: f32, max: f32) {
    let new = adjusted(current(world, mob), secs, Some(max));
    store(world, mob, new);
}

/// tg `set_hallucinations(duration)` / `set_hallucinations_if_lower(duration)`.
pub fn set_hallucinations(world: &mut World, mob: Entity, secs: f32, only_if_higher: bool) {
    let new = set_to(current(world, mob), secs, only_if_higher);
    store(world, mob, new);
}

/// tg `hallucination_pulse(center, radius, duration, max_duration, messages)`: everyone in
/// range who can hallucinate gets `duration * sqrt(1 / dist)` more, up to `max`.
pub fn hallucination_pulse(
    world: &mut World,
    center: TilePos,
    radius: u32,
    duration: f32,
    max: f32,
    messages: &[&str],
) {
    let mobs: Vec<(Entity, u32)> = world
        .resource::<SpatialIndex>()
        .in_radius(center, radius)
        .filter_map(|e| {
            let pos = world.get::<TilePos>(e)?;
            Some((e, pos.chebyshev(center)?))
        })
        .filter(|&(e, _)| can_hallucinate(world, e))
        .collect();
    for (mob, dist) in mobs {
        let factor = (1.0 / dist.max(1) as f32).sqrt();
        adjust_hallucinations_up_to(world, mob, duration * factor, max);
        if !messages.is_empty() {
            let mut rng = rng(world);
            let i = rng.range(0, messages.len() as u32 - 1) as usize;
            tell(world, mob, Span::Plain, messages[i]);
        }
    }
}

/// The tier the effect's remaining seconds allow and the factor of the next cooldown
/// (`variable_tier`; `rare_roll` is its `prob(10)`).
// tg multiplies its own `lower_tick_interval` vars by 1.2 under 20 s, so the slowdown compounds
// every tick for the rest of the effect; the intent is this tick's cooldown, as the other
// branches do.
pub fn variable_tier(remaining: f32, rare_roll: bool) -> (Tier, f64) {
    if remaining <= 20.0 {
        (Tier::Common, 1.2)
    } else if remaining <= 60.0 {
        (
            if rare_roll {
                Tier::Rare
            } else {
                Tier::Uncommon
            },
            1.0,
        )
    } else if remaining <= 120.0 {
        (Tier::Rare, 0.75)
    } else {
        (Tier::VerySpecial, 0.5)
    }
}

/// tg `get_random_hallucination(tier, strict)`: weighted (`random_hallucination_weight * 10`)
/// among the tier's hallucinations and, unless `strict`, every lower tier's.
pub fn random_hallucination(rng: &mut CombatRng, tier: Tier, strict: bool) -> Option<Kind> {
    let pool: Vec<(Kind, u32)> = kinds::TABLE
        .iter()
        .filter(|d| d.tier == tier || (!strict && d.tier < tier))
        .map(|d| (d.kind, (d.weight * 10.0).round() as u32))
        .filter(|&(_, w)| w > 0)
        .collect();
    let total: u32 = pool.iter().map(|p| p.1).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.range(1, total);
    for (kind, weight) in pool {
        if roll <= weight {
            return Some(kind);
        }
        roll -= weight;
    }
    None
}

/// tg `cause_hallucination(type, source)`: false when it did not start.
pub fn cause_hallucination(world: &mut World, mob: Entity, kind: Kind, source: &str) -> bool {
    if !exists(world, mob) || world.get::<Stat>(mob).is_none() {
        return false;
    }
    let started = kinds::start(world, mob, kind);
    if started {
        debug!(
            "{mob:?} was afflicted with a hallucination of type {} by: {source}",
            kind.def().path
        );
    }
    started
}

/// One status effect tick (`tick`) past the cooldown: a hallucination, then the next cooldown
/// (`rand(lower, upper)` deciseconds, a quarter of it when nothing started).
fn hallucinate(world: &mut World, mob: Entity, tier: Tier, lower: f64, upper: f64) -> f64 {
    let mut rng = rng(world);
    let picked = random_hallucination(&mut rng, tier, false);
    let started = picked.is_some_and(|k| cause_hallucination(world, mob, k, "status effect"));
    let factor = if started { 1.0 } else { 0.25 };
    let (low, high) = (
        (lower * factor * 10.0) as u32,
        (upper * factor * 10.0) as u32,
    );
    now(world) + f64::from(rng.range(low, high.max(low))) / 10.0
}

/// The status effects' processing (`SSprocessing`, `tick_interval` 2 s).
// Cost: every 2 s one pass over the hallucinating mobs (a handful).
fn process(world: &mut World, mut last: Local<f64>) {
    let now = now(world);
    if now - *last < TICK_SECS {
        return;
    }
    let elapsed = (now - *last) as f32;
    *last = now;
    let mut query = world.query::<(Entity, &Hallucinating)>();
    let effects: Vec<(Entity, f32, f64)> = query
        .iter(world)
        .map(|(e, h)| (e, h.remaining, h.next_at))
        .collect();
    for (mob, left, next_at) in effects {
        let left = left - elapsed;
        if left <= 0.0 {
            world.entity_mut(mob).remove::<Hallucinating>();
            continue;
        }
        if let Some(mut h) = world.get_mut::<Hallucinating>(mob) {
            h.remaining = left;
        }
        if world.get::<Stat>(mob) == Some(&Stat::Dead) || now < next_at {
            continue;
        }
        let mut rng = rng(world);
        let (tier, factor) = variable_tier(left, rng.chance(10.0));
        let at = hallucinate(
            world,
            mob,
            tier,
            LOWER_INTERVAL * factor,
            UPPER_INTERVAL * factor,
        );
        if let Some(mut h) = world.get_mut::<Hallucinating>(mob) {
            h.next_at = at;
        }
    }
    let mut query = world.query::<(Entity, &SanityHallucinations)>();
    let sane: Vec<(Entity, u8, f64)> = query
        .iter(world)
        .map(|(e, s)| (e, s.level, s.next_at))
        .collect();
    for (mob, level, next_at) in sane {
        if world.get::<Stat>(mob) == Some(&Stat::Dead) || now < next_at {
            continue;
        }
        // `update_intervals`; `max_hallucination_tier = HALLUCINATION_TIER_UNCOMMON`.
        let (lower, upper) = if level == SANITY_LEVEL_INSANE {
            (120.0, 240.0)
        } else {
            (240.0, 480.0)
        };
        let at = hallucinate(world, mob, Tier::Uncommon, lower, upper);
        if let Some(mut s) = world.get_mut::<SanityHallucinations>(mob) {
            s.next_at = at;
        }
    }
}

/// tg `set_sanity`: crazy or insane applies the sanity hallucinations (a level change refreshes
/// their intervals), anything else removes them.
fn sync_sanity(
    moods: Query<(Entity, &Mood, Option<&SanityHallucinations>), Changed<Mood>>,
    mut commands: Commands,
) {
    for (mob, mood, effect) in &moods {
        let crazy = mood.sanity_level >= SANITY_LEVEL_CRAZY;
        match effect {
            Some(_) if !crazy => {
                commands.entity(mob).remove::<SanityHallucinations>();
            }
            Some(e) if e.level != mood.sanity_level => {
                commands.entity(mob).insert(SanityHallucinations {
                    level: mood.sanity_level,
                    next_at: e.next_at,
                });
            }
            None if crazy => {
                commands.entity(mob).insert(SanityHallucinations {
                    level: mood.sanity_level,
                    next_at: 0.0,
                });
            }
            _ => {}
        }
    }
}

fn run_timers(world: &mut World) {
    let now = now(world);
    let Some(mut timers) = world.get_resource_mut::<HallucinationTimers>() else {
        return;
    };
    if !timers.0.iter().any(|(at, _)| *at <= now) {
        return;
    }
    let all = std::mem::take(&mut timers.0);
    let (mut due, rest): (Vec<_>, Vec<_>) = all.into_iter().partition(|(at, _)| *at <= now);
    timers.0.extend(rest);
    due.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, f) in due {
        f(world);
    }
}

// --- Reagents -----------------------------------------------------------------------------

const MINDBREAKER: &str = "/datum/reagent/toxin/mindbreaker";

/// The hallucination reagents in `mob`'s blood for one metabolism tick of `secs`
/// (`on_mob_life`).
// shortcut: a metabolization ratio of 1; no overdoses (space drugs, earthsblood, psicodine).
pub fn reagents(world: &mut World, mob: Entity, secs: f32) {
    use crate::chem::blood_amount;
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    let has = |world: &World, path: &str| blood_amount(world, mob, path) > 0.0;
    // Mindbreaker soothes the hallucinations brain trauma instead of causing hallucinations.
    let soothed = crate::medical::traumas::rds_suppressed(world, mob)
        && crate::medical::traumas::has(world, mob, crate::medical::traumas::Kind::Hallucinations);
    if soothed {
        set_hallucinations(world, mob, 0.0, false);
    }
    // Mindbreaker (and its jellyfish variant), viper spider toxin, BZ metabolites, bath salts.
    let gain: f32 = [
        (MINDBREAKER, 5.0),
        ("/datum/reagent/toxin/mindbreaker/fish", 5.0),
        ("/datum/reagent/toxin/viperspider", 5.0),
        ("/datum/reagent/bz_metabolites", 12.5),
        ("/datum/reagent/drug/bath_salts", 10.0),
    ]
    .iter()
    .filter(|(path, _)| has(world, path) && !(soothed && path.starts_with(MINDBREAKER)))
    .map(|(_, rate)| rate * secs)
    .sum();
    if gain > 0.0 {
        adjust_hallucinations(world, mob, gain);
    }
    for synaptizine in [
        "/datum/reagent/medicine/synaptizine",
        "/datum/reagent/medicine/synaphydramine",
    ] {
        if has(world, synaptizine) {
            adjust_hallucinations(world, mob, -10.0 * secs);
        }
    }
    if has(world, "/datum/reagent/medicine/haloperidol") && remaining(world, mob) >= 10.0 {
        adjust_hallucinations(world, mob, -12.5 * secs);
    }
}

// --- What the hallucinator perceives -------------------------------------------------------

pub(crate) use crate::tools::client_of;

/// `to_chat(hallucinator, ...)`.
pub(crate) fn tell(world: &mut World, mob: Entity, span: Span, text: &str) {
    crate::chem::tell_mob(world, mob, span, text);
}

pub(crate) fn tell_message(world: &mut World, mob: Entity, message: ChatMessage) {
    if let Some(client) = client_of(world, mob) {
        world.write_message(SendChat {
            clients: vec![client],
            message,
        });
    }
}

pub(crate) fn line(channel: ChatChannel, span: Span, text: impl Into<String>) -> ChatMessage {
    ChatMessage {
        channel,
        segments: vec![ChatSegment::new(span, text)],
    }
}

pub(crate) fn pos_of(world: &World, mob: Entity) -> Option<TilePos> {
    crate::chem::map_pos(world, mob)
}

/// `hallucinator.playsound_local(source, sound, vol, vary)`: no distance falloff (no
/// `max_distance`), the air's pressure, stereo from the source; nothing for the deaf.
pub fn play_local(
    world: &mut World,
    mob: Entity,
    source: Option<TilePos>,
    sound: &str,
    volume: f32,
    vary: bool,
) {
    let Some(client) = client_of(world, mob) else {
        return;
    };
    if !crate::sound::can_hear(world, mob) {
        return;
    }
    let mut rng = rng(world);
    let file = match world.get_resource::<crate::sound::Sfx>() {
        Some(sfx) => crate::sound::pick_sfx(sfx, &mut rng, sound),
        None => sound.to_owned(),
    };
    let speed = if vary {
        rng.range(32000, 55000) as f32 / 44100.0
    } else {
        1.0
    };
    let at = pos_of(world, mob);
    let mut volume = volume;
    let mut offset = None;
    if let (Some(source), Some(at)) = (source, at)
        && let Some(distance) = source.chebyshev(at)
    {
        let pressure = |pos| match world.get_resource::<ss13_atmos::Atmos>() {
            Some(a) => a.pressure(pos),
            None => Some(crate::sound::ONE_ATMOSPHERE),
        };
        volume *= crate::sound::pressure_factor(pressure(at), pressure(source), distance as f32);
        offset = Some([(source.x - at.x) as i16, (source.y - at.y) as i16]);
    }
    if volume <= 0.0 {
        return;
    }
    world.write_message(SendSound {
        client,
        sound: SoundEvent {
            file,
            volume,
            speed,
            offset,
        },
    });
}

/// `random_far_turf()`: a tile 5-8 tiles away on one axis, up to 8 on the other; `None` off
/// the map (tg's `locate` gives null, and the sound plays without a source).
pub fn random_far_turf(world: &mut World, mob: Entity) -> Option<TilePos> {
    let at = pos_of(world, mob)?;
    let mut rng = rng(world);
    let first = [-8, -7, -6, -5, 5, 6, 7, 8][rng.range(0, 7) as usize];
    let second = rng.range(0, 16) as i32 - 8;
    let (dx, dy) = if rng.chance(50.0) {
        (first, second)
    } else {
        (second, first)
    };
    let pos = TilePos::new(at.x + dx, at.y + dy, at.z);
    world
        .resource::<SpatialIndex>()
        .in_bounds(pos)
        .then_some(pos)
}

/// BYOND `view(center, radius)` as tiles.
pub(crate) fn view_tiles(world: &World, center: TilePos, radius: u32) -> HashSet<TilePos> {
    let index = world.resource::<SpatialIndex>();
    crate::sound::hearing_view(
        center,
        radius as f32,
        |p| {
            index
                .on_tile(p)
                .iter()
                .any(|&e| world.get::<Opaque>(e).is_some())
        },
        |p| index.in_bounds(p),
    )
}

/// The humans in `view(mob)` other than `mob`, with their tiles.
pub(crate) fn humans_in_view(world: &World, mob: Entity, radius: u32) -> Vec<(Entity, TilePos)> {
    let Some(at) = pos_of(world, mob) else {
        return Vec::new();
    };
    let seen = view_tiles(world, at, radius);
    let mut out: Vec<(Entity, TilePos)> = world
        .resource::<SpatialIndex>()
        .in_radius(at, radius)
        .filter(|&e| e != mob && world.get::<crate::human::HumanLook>(e).is_some())
        .filter_map(|e| Some((e, *world.get::<TilePos>(e)?)))
        .filter(|(_, p)| seen.contains(p))
        .collect();
    out.sort_by_key(|(e, _)| *e);
    out
}

/// `throw_alert(category, type, severity, override = TRUE)` of a fake alert: its own category
/// beside the real ones.
pub(crate) fn throw_fake_alert(
    world: &mut World,
    mob: Entity,
    category: u8,
    state: &'static str,
    severity: u8,
) {
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        alerts.throw(AlertCategory::Hallucination(category), state, severity);
    }
}

pub(crate) fn clear_fake_alert(world: &mut World, mob: Entity, category: u8) {
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob)
        && alerts.get(AlertCategory::Hallucination(category)).is_some()
    {
        alerts.clear(AlertCategory::Hallucination(category));
    }
}

/// Changes the mob's [`ScrewyHud`] and refreshes its health HUD.
pub(crate) fn screwy(world: &mut World, mob: Entity, f: impl FnOnce(&mut ScrewyHud)) {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    let mut hud = entity.get::<ScrewyHud>().cloned().unwrap_or_default();
    f(&mut hud);
    if hud.is_empty() {
        entity.remove::<ScrewyHud>();
    } else {
        entity.insert(hud);
    }
    // `update_health_hud`: `crate::health` redraws on a health change.
    if let Some(mut health) = entity.get_mut::<crate::health::Health>() {
        health.set_changed();
    }
}

/// The doll index of a body zone.
pub(crate) fn doll_index(zone: crate::zone::BodyZone) -> usize {
    DOLL_LIMBS.iter().position(|&z| z == zone).unwrap_or(0)
}

// --- Images -------------------------------------------------------------------------------

/// Sets or clears `bits` of a client's [`SeeGroups`], keeping the other bits.
pub fn see_groups(world: &mut World, client: Entity, bits: u32, on: bool) {
    let Ok(mut entity) = world.get_entity_mut(client) else {
        return;
    };
    let old = entity.get::<SeeGroups>().map_or(0, |g| g.0);
    let new = if on { old | bits } else { old & !bits };
    if new == old {
        return;
    }
    if new == 0 {
        entity.remove::<SeeGroups>();
    } else {
        entity.insert(SeeGroups(new));
    }
}

/// What an image looks like: `(dmi, state)`, layer, color, pixel offset, rotation.
#[derive(Clone, Copy, Debug)]
pub struct ImageLook {
    pub icon: (&'static str, &'static str),
    pub layer: f32,
    pub color: Option<[u8; 4]>,
    pub pixel_offset: [i16; 2],
    pub rotation: u16,
}

impl ImageLook {
    pub const fn new(dmi: &'static str, state: &'static str, layer: f32) -> Self {
        Self {
            icon: (dmi, state),
            layer,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
        }
    }
}

/// Shows `look` to `seer` on `pos` (following `follow`, if any); `None` when the state is
/// missing or the seer has no client.
pub fn show_image(
    world: &mut World,
    seer: Entity,
    pos: TilePos,
    look: ImageLook,
    follow: Option<Entity>,
) -> Option<Entity> {
    client_of(world, seer)?;
    let state = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(look.icon.0, look.icon.1))?;
    let image = world
        .spawn((
            Name::new("hallucination"),
            pos,
            Appearance {
                icon_state: state,
                layer: Layer(look.layer),
                plane: GAME_PLANE,
                color: look.color,
                pixel_offset: look.pixel_offset,
                rotation: look.rotation,
                overlays: Vec::new(),
                mouse_opacity: MouseOpacity::Transparent,
            },
            Networked,
            Invisibility(IMAGE_INVISIBILITY),
            VisibleToMobs(vec![seer]),
            HallucinationImage { seer, follow },
        ))
        .id();
    Some(image)
}

/// Shows `look` over `target` (following it) to `seer`.
pub fn show_on(world: &mut World, seer: Entity, target: Entity, look: ImageLook) -> Option<Entity> {
    let pos = pos_of(world, target)?;
    show_image(world, seer, pos, look, Some(target))
}

/// Removes an image (`client.images -= image`).
pub fn hide_image(world: &mut World, image: Entity) {
    if world.get::<HallucinationImage>(image).is_some() {
        world.entity_mut(image).despawn();
    }
}

/// Hides `image` after `secs` (tg `QDEL_IN` of the hallucination owning it).
pub fn hide_later(world: &mut World, image: Option<Entity>, secs: f64) {
    if let Some(image) = image {
        later(world, secs, move |world| hide_image(world, image));
    }
}

/// Images move with what they follow; an image whose seer or target is gone goes too.
// Cost: one pass over the live images (a few per hallucinating player) per tick.
fn follow_images(world: &mut World) {
    let mut query = world.query::<(Entity, &HallucinationImage, &TilePos)>();
    let images: Vec<(Entity, HallucinationImage, TilePos)> =
        query.iter(world).map(|(e, i, p)| (e, *i, *p)).collect();
    for (image, data, pos) in images {
        if !exists(world, data.seer) {
            hide_image(world, image);
            continue;
        }
        let Some(target) = data.follow else {
            continue;
        };
        let at = if exists(world, target) {
            pos_of(world, target)
        } else {
            None
        };
        match at {
            Some(at) if at != pos => {
                world.entity_mut(image).insert(at);
            }
            Some(_) => {}
            None => hide_image(world, image),
        }
    }
}

// --- Strings ------------------------------------------------------------------------------

/// tg `pick_list_replacements(HALLUCINATION_FILE, key)`: a random line of `key`, each
/// `@pick(other)` replaced by a random line of `other` (one level, as `strings_subkey_lookup`).
pub fn pick_line(rng: &mut CombatRng, key: &str) -> String {
    let pick = |rng: &mut CombatRng, key: &str| -> String {
        let Some((_, lines)) = strings::HALLUCINATION.iter().find(|(k, _)| *k == key) else {
            return String::new();
        };
        if lines.is_empty() {
            return String::new();
        }
        lines[rng.range(0, lines.len() as u32 - 1) as usize].to_owned()
    };
    let line = pick(rng, key);
    let mut out = String::new();
    let mut rest = line.as_str();
    while let Some(start) = rest.find("@pick(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "@pick(".len()..];
        let Some(end) = after.find(')') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        out.push_str(&pick(rng, &after[..end]));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// tg `first_name(name)`: up to the first space.
pub fn first_name(name: &str) -> &str {
    name.split(' ').next().unwrap_or(name)
}

/// tg `last_name(name)`: after the last space (the whole name without one).
pub fn last_name(name: &str) -> &str {
    name.rsplit(' ').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::PlayerMob;

    #[test]
    fn adjusting_follows_tg_timed_status_effects() {
        assert_eq!(adjusted(None, 10.0, None), Some(10.0));
        assert_eq!(adjusted(None, -5.0, None), None);
        assert_eq!(adjusted(Some(10.0), -10.0, None), None);
        assert_eq!(adjusted(Some(10.0), 5.0, None), Some(15.0));
        // Up to a cap: never past it, nothing when already above.
        assert_eq!(adjusted(Some(95.0), 10.0, Some(100.0)), Some(100.0));
        assert_eq!(adjusted(Some(120.0), 10.0, Some(100.0)), Some(120.0));
        assert_eq!(adjusted(None, 300.0, Some(100.0)), Some(100.0));
        assert_eq!(set_to(Some(30.0), 10.0, true), Some(30.0));
        assert_eq!(set_to(Some(30.0), 10.0, false), Some(10.0));
        assert_eq!(set_to(Some(30.0), 0.0, false), None);
    }

    #[test]
    fn the_tier_follows_the_remaining_time() {
        assert_eq!(variable_tier(5.0, true), (Tier::Common, 1.2));
        assert_eq!(variable_tier(20.0, false), (Tier::Common, 1.2));
        assert_eq!(variable_tier(40.0, false), (Tier::Uncommon, 1.0));
        assert_eq!(variable_tier(40.0, true), (Tier::Rare, 1.0));
        assert_eq!(variable_tier(100.0, false), (Tier::Rare, 0.75));
        assert_eq!(variable_tier(500.0, false), (Tier::VerySpecial, 0.5));
    }

    #[test]
    fn random_picks_stay_within_the_tiers_and_skip_weightless_ones() {
        let mut rng = CombatRng(7);
        for _ in 0..500 {
            let kind = random_hallucination(&mut rng, Tier::Common, false).unwrap();
            let def = kind.def();
            assert_eq!(def.tier, Tier::Common, "{}", def.path);
            assert!(def.weight > 0.0, "{}", def.path);
        }
        let mut seen_lower = false;
        for _ in 0..500 {
            let def = random_hallucination(&mut rng, Tier::Rare, false)
                .unwrap()
                .def();
            assert!(def.tier <= Tier::Rare);
            seen_lower |= def.tier < Tier::Rare;
        }
        assert!(seen_lower);
        for _ in 0..200 {
            let def = random_hallucination(&mut rng, Tier::Rare, true)
                .unwrap()
                .def();
            assert_eq!(def.tier, Tier::Rare);
        }
    }

    #[test]
    fn lines_replace_their_subkeys() {
        let mut rng = CombatRng(3);
        for _ in 0..100 {
            let line = pick_line(&mut rng, "getout");
            assert!(!line.contains("@pick"), "{line}");
            let location = pick_line(&mut rng, "location");
            assert!(!location.contains('@'), "{location}");
        }
        assert_eq!(pick_line(&mut rng, "no such key"), "");
        assert_eq!(first_name("John Smith"), "John");
        assert_eq!(last_name("John Smith"), "Smith");
        assert_eq!(last_name("Pun"), "Pun");
    }

    #[derive(Resource, Default)]
    struct Log(Vec<u32>);

    #[test]
    fn timers_run_when_due_in_order() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.init_resource::<Log>();
        later(&mut world, 2.0, |w| w.resource_mut::<Log>().0.push(2));
        later(&mut world, 1.0, |w| {
            w.resource_mut::<Log>().0.push(1);
            later(w, 0.0, |w| w.resource_mut::<Log>().0.push(3));
        });
        run_timers(&mut world);
        assert!(world.resource::<Log>().0.is_empty());
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(3));
        run_timers(&mut world);
        assert_eq!(world.resource::<Log>().0, [1, 2]);
        run_timers(&mut world);
        assert_eq!(world.resource::<Log>().0, [1, 2, 3]);
    }

    #[test]
    fn images_are_seen_by_their_seer_only() {
        let mut world = World::new();
        let mut icons = IconIndex::default();
        icons
            .0
            .insert(("a.dmi".into(), "s".into()), ss13_shared::IconStateId(1));
        world.insert_resource(icons);
        let client = world.spawn_empty().id();
        let seer = world.spawn(PlayerMob { client }).id();
        let other_client = world.spawn_empty().id();
        let other = world
            .spawn(PlayerMob {
                client: other_client,
            })
            .id();
        let look = ImageLook::new("a.dmi", "s", ABOVE_MOB_LAYER);
        let pos = TilePos::new(1, 1, 1);
        // No limit on how many clients have images at once.
        let many: Vec<Entity> = (0..40)
            .map(|_| show_image(&mut world, seer, pos, look, None).unwrap())
            .collect();
        let c = show_image(&mut world, other, pos, look, None).unwrap();
        assert_eq!(
            world.get::<VisibleToMobs>(many[0]),
            Some(&VisibleToMobs(vec![seer]))
        );
        assert_eq!(
            world.get::<VisibleToMobs>(c),
            Some(&VisibleToMobs(vec![other]))
        );
        assert_eq!(
            world.get::<Invisibility>(many[0]),
            Some(&Invisibility(u8::MAX))
        );
        hide_image(&mut world, many[0]);
        assert!(world.get_entity(many[0]).is_err());
        // Not an image: left alone.
        hide_image(&mut world, seer);
        assert!(world.get_entity(seer).is_ok());
        // A missing icon state or a clientless seer shows nothing.
        let missing = ImageLook::new("a.dmi", "nope", ABOVE_MOB_LAYER);
        assert!(show_image(&mut world, seer, pos, missing, None).is_none());
        let clientless = world.spawn_empty().id();
        assert!(show_image(&mut world, clientless, pos, look, None).is_none());
    }

    #[test]
    fn screwy_huds_override_by_priority() {
        let mut readout = HealthReadout {
            health: 50.0,
            max: 100.0,
            brute: 50.0,
            burn: 0.0,
            tox: 0.0,
            oxy: 0.0,
            dead: false,
            limbs: [0; 6],
        };
        let mut hud = ScrewyHud {
            healthy: 1,
            crit: 1,
            ..default()
        };
        hud.doll[2] = 4;
        let mut shown = readout;
        hud.apply(&mut shown, Stat::Conscious);
        assert_eq!(shown.health_state(), 6);
        assert_eq!(shown.limbs[2], 4);
        hud.crit = 0;
        let mut shown = readout;
        hud.apply(&mut shown, Stat::Conscious);
        assert_eq!(shown.health_state(), 0);
        hud.dead = 1;
        hud.apply(&mut readout, Stat::Conscious);
        assert_eq!(readout.health_state(), 7);
    }
}
