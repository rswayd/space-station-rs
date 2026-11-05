//! Radiation (server), after tg `SSradiation` (`code/controllers/subsystem/radiation.dm`,
//! `code/__HELPERS/radiation.dm`, `code/__DEFINES/radiation.dm`), the `irradiated` and
//! `radiation_countdown` components, the `radiation_protected_clothing` element, the Geiger
//! counter (`code/game/objects/items/devices/geiger_counter.dm`) and uranium walls.
//!
//! - [`radiation_pulse`] queues a pulse; every 0.5 s ([`SS_WAIT`]) each queued pulse walks the
//!   square of `max_range` around its source. For every human and item on those tiles it
//!   multiplies 1 by the `rad_insulation` ([`DmRadInsulation`]) of every tile on the line to it
//!   (the tile and everything on it, the source's tile excluded) until it drops to the
//!   threshold. Above the threshold the target is irradiated with a chance that is the pulse's
//!   `chance` at half `max_range` and falls with distance and insulation. A pulse with a
//!   `minimum_exposure_time` first only warms the target up ([`RadiationCountdown`]): it must
//!   stay near radiation that long.
//! - Species radiation immunity (`TRAIT_RADIMMUNE`, `crate::species`) and, for humans, being
//!   covered head to toe by radiation-protected clothing (the radiation suit and hood, the
//!   hostile environment suit, cosmic and ash heretic robes, any MOD part of a suit with an
//!   activated radiation protection module) stop it.
//! - [`Irradiated`] humans take 10 toxin at once and 2 more every 25 s, get burn blisters every
//!   30-60 s, and after one/two minutes lose hair, mutate, vomit and collapse
//!   (`/datum/species/proc/handle_radiation`); it ends when their toxin damage is healed.
//!   Potassium iodide and pentetic acid in the blood (`TRAIT_HALT_RADIATION_EFFECTS`) and a
//!   radiation wash halt the effects. Uranium and polonium in the blood irradiate.
//! - The Geiger counter shows the perceived danger of pulses reaching it or its holder, and
//!   scans a human or item for contamination after 2 s.
//! - Uranium walls and false walls pulse (range 3, 3 s exposure) when touched, hit or bumped,
//!   and pass the pulse to adjacent uranium walls; gas reactions and the supermatter pulse
//!   too.
//!
//! Cost: a pulse visits `(2r + 1)^2` tiles and, per human or item there, a line of at most `r`
//! tiles (tile insulation cached per pulse): a range 8 supermatter pulse every SSair fire is
//! ~300 tiles. Irradiated mobs process every 2 s.
// shortcut: no green glow filter on irradiated atoms, no geiger clicking sound loop
// (`geiger_sound`), no MOD radiation counter panel, no `radioactive` element on uranium
// materials (statues, mineral doors), no `/datum/status_effect/radiation_immunity`, no
// `TRAIT_STASIS` halt, no showers (the wash hook is here); add with those mechanics.

use std::collections::{HashMap, HashSet};

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmClothing, DmItem, DmObj, DmRadInsulation, DmTurfBuild, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_world::{Bumped, SpatialIndex};

use crate::breath::{AlertCategory, Alerts};
use crate::click::{ClickResolved, RangedClick};
use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::human::HumanLook;
use crate::inventory::{ALL_SLOTS, Inventory};
use crate::sound::{PlaySound, playsound};
use crate::species::Trait;
use crate::zone::BodyZone;

/// `SSradiation.wait`.
pub const SS_WAIT: f64 = 0.5;
/// `RAD_*_INSULATION`.
pub const RAD_NO_INSULATION: f32 = 1.0;
pub const RAD_VERY_LIGHT_INSULATION: f32 = 0.9;
pub const RAD_LIGHT_INSULATION: f32 = 0.8;
pub const RAD_MEDIUM_INSULATION: f32 = 0.7;
pub const RAD_HEAVY_INSULATION: f32 = 0.6;
pub const RAD_EXTREME_INSULATION: f32 = 0.5;
pub const RAD_FULL_INSULATION: f32 = 0.0;
/// `DEFAULT_RADIATION_CHANCE`, `URANIUM_IRRADIATION_CHANCE`.
pub const DEFAULT_RADIATION_CHANCE: f32 = 10.0;
pub const URANIUM_IRRADIATION_CHANCE: f32 = DEFAULT_RADIATION_CHANCE;
/// `URANIUM_RADIATION_MINIMUM_EXPOSURE_TIME`, seconds.
pub const URANIUM_RADIATION_MINIMUM_EXPOSURE_TIME: f64 = 3.0;
/// `CLEAN_TYPE_RADIATION`.
pub const CLEAN_TYPE_RADIATION: u32 = 1 << 3;

/// `RADIATION_IMMEDIATE_TOX_DAMAGE`, `RADIATION_TOX_DAMAGE_PER_INTERVAL`,
/// `RADIATION_TOX_INTERVAL`.
const IMMEDIATE_TOX: f32 = 10.0;
const TOX_PER_INTERVAL: f32 = 2.0;
const TOX_INTERVAL: f64 = 25.0;
/// `RADIATION_BURN_SPLOTCH_DAMAGE`, `RADIATION_BURN_INTERVAL_MIN/MAX`.
const BURN_SPLOTCH: f32 = 11.0;
const BURN_INTERVAL: (u32, u32) = (30, 60);
/// `RADIATION_CLEAN_IMMUNITY_TIME`: `SSMACHINES_DT + 1 SECONDS`.
const CLEAN_IMMUNITY: f64 = 3.0;
/// `SSobj.wait`: the irradiated component's `process`.
const SSOBJ_DT: f64 = 2.0;
/// `RAD_MOB_*` thresholds (seconds since irradiation) and chances (percent per second).
const RAD_MOB_HAIRLOSS: f64 = 60.0;
const RAD_MOB_HAIRLOSS_PROB: f32 = 7.5;
const RAD_MOB_MUTATE: f64 = 120.0;
const RAD_MOB_MUTATE_PROB: f32 = 0.5;
const RAD_MOB_VOMIT: f64 = 120.0;
const RAD_MOB_VOMIT_PROB: f32 = 0.5;
const RAD_MOB_KNOCKDOWN: f64 = 120.0;
const RAD_MOB_KNOCKDOWN_PROB: f32 = 0.5;
/// `RAD_MOB_KNOCKDOWN_AMOUNT`: 3 deciseconds.
const RAD_MOB_KNOCKDOWN_AMOUNT: f32 = 0.3;
/// `go_bald` timer.
const BALD_DELAY: f64 = 5.0;
/// `radiation_countdown`'s `TIME_UNTIL_DELETION`.
const COUNTDOWN_LIFETIME: f64 = 10.0;
/// `TIME_WITHOUT_RADIATION_BEFORE_RESET`.
const GEIGER_RESET: f64 = 5.0;
/// The Geiger counter's scan timer (20 deciseconds).
const GEIGER_SCAN_DELAY: f64 = 2.0;
/// `EXTREME_RADIATION_CHANCE`, `MEDIUM_RADIATION_THRESHOLD_RANGE`.
const EXTREME_RADIATION_CHANCE: f32 = 30.0;
const MEDIUM_RADIATION_THRESHOLD_RANGE: f32 = 0.5;
/// Uranium walls: `last_event + 1.5 SECONDS`, `max_range = 3`.
const URANIUM_COOLDOWN: f64 = 1.5;
const URANIUM_RANGE: f32 = 3.0;

pub const GEIGER: &str = "/obj/item/geiger_counter";
const URANIUM_WALL: &str = "/turf/closed/wall/mineral/uranium";
const URANIUM_FALSEWALL: &str = "/obj/structure/falsewall/uranium";
const POTASS_IODIDE: &str = "/datum/reagent/medicine/potass_iodide";
const PEN_ACID: &str = "/datum/reagent/medicine/pen_acid";
const URANIUM: &str = "/datum/reagent/uranium";
const POLONIUM: &str = "/datum/reagent/toxin/polonium";

/// Types that add `/datum/element/radiation_protected_clothing` in `Initialize`.
const RAD_PROTECTED: [&str; 7] = [
    "/obj/item/clothing/head/utility/radiation",
    "/obj/item/clothing/suit/utility/radiation",
    "/obj/item/clothing/suit/hooded/hostile_environment",
    "/obj/item/clothing/head/hooded/hostile_environment",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/ash",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/cosmic",
    "/obj/item/clothing/head/hooded/cult_hoodie/eldritch/cosmic",
];

/// `PERCEIVED_RADIATION_DANGER_*`.
pub const DANGER_LOW: u8 = 1;
pub const DANGER_MEDIUM: u8 = 2;
pub const DANGER_HIGH: u8 = 3;
pub const DANGER_EXTREME: u8 = 4;

/// `/datum/radiation_pulse_information` waiting for the subsystem.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pulse {
    pub source: Entity,
    /// The source's tile when the pulse was sent (tg `RANGE_TURFS(max_range, source)`).
    pub origin: TilePos,
    pub max_range: f32,
    pub threshold: f32,
    /// Percent at half `max_range` without insulation.
    pub chance: f32,
    pub minimum_exposure_time: f64,
}

/// `SSradiation.processing`.
#[derive(Resource, Debug, Default)]
pub struct RadiationQueue {
    pub pulses: Vec<Pulse>,
    next_fire: f64,
    next_process: f64,
}

/// Radiation rolls, own stream.
#[derive(Resource, Debug, Default)]
pub struct RadiationRng(pub CombatRng);

/// `/datum/component/irradiated` (`TRAIT_IRRADIATED`) on a human or item.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Irradiated {
    /// `beginning_of_irradiation`, server seconds.
    pub since: f64,
    next_tox: f64,
    next_burn: f64,
    /// `clean_cooldown`: a radiation wash halts the effects until then.
    clean_until: f64,
}

/// `/datum/component/radiation_countdown`: pulses with a minimum exposure time irradiate only
/// after this long near any of them.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct RadiationCountdown {
    pub added: f64,
    pub minimum: f64,
    pub expires: f64,
}

/// `addtimer(go_bald, 5 SECONDS)`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GoingBald(pub f64);

/// A uranium wall's `last_event`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct UraniumLastPulse(pub f64);

/// `/obj/item/geiger_counter` state (added on first use).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Geiger {
    pub scanning: bool,
    /// `last_perceived_radiation_danger`.
    pub danger: Option<u8>,
    reset_at: f64,
    /// The pending `scan` (`TIMER_UNIQUE`): when, target, user.
    scan: Option<(f64, Entity, Entity)>,
}

pub struct RadiationPlugin;

impl Plugin for RadiationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RadiationQueue>()
            .init_resource::<RadiationRng>()
            .add_click_handler(ClickPhase::AttackSelf, 1750, |w, c| {
                c.held
                    .is_some_and(|item| geiger_attack_self(w, c.client, item))
            })
            .add_click_handler(ClickPhase::HeldOnTarget, 1650, |w, c| {
                c.held
                    .is_some_and(|held| geiger_interact(w, c.mob, held, c.target))
            })
            .add_click_handler(ClickPhase::OtherHand, 1250, |w, c| {
                // The other hand's item is a world target of the held counter too.
                c.held
                    .is_some_and(|held| geiger_interact(w, c.mob, held, c.target))
            })
            .add_systems(
                Update,
                (
                    uranium_touches,
                    geiger_ranged,
                    atmos_pulses,
                    fire_pulses,
                    process_radiation,
                )
                    .chain()
                    .after(crate::click::ResolveClicks),
            );
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn item_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

/// tg `ishuman`.
fn is_human(world: &World, e: Entity) -> bool {
    world.get::<HumanLook>(e).is_some()
}

/// `CAN_IRRADIATE`: humans and items.
pub fn can_irradiate(world: &World, e: Entity) -> bool {
    is_human(world, e) || world.get::<DmItem>(e).is_some()
}

/// `HAS_TRAIT(target, TRAIT_RADIMMUNE)`.
pub fn rad_immune(world: &World, e: Entity) -> bool {
    is_human(world, e) && crate::species::has_trait(world, e, Trait::RadImmune)
}

pub fn is_irradiated(world: &World, e: Entity) -> bool {
    world.get::<Irradiated>(e).is_some()
}

fn is_geiger(world: &World, e: Entity) -> bool {
    item_path(world, e).is_some_and(|p| is_under(p, GEIGER))
}

/// `TRAIT_BYPASS_EARLY_IRRADIATED_CHECK`: a scanning counter (`geiger_sound`).
fn bypasses_irradiated_check(world: &World, e: Entity) -> bool {
    world.get::<Geiger>(e).is_some_and(|g| g.scanning)
}

/// `SSradiation.can_irradiate_basic`.
pub fn can_irradiate_basic(world: &World, e: Entity) -> bool {
    can_irradiate(world, e)
        && !(is_irradiated(world, e) && !bypasses_irradiated_check(world, e))
        && !rad_immune(world, e)
}

/// `TRAIT_RADIATION_PROTECTED_CLOTHING` from the element.
fn rad_protected_type(world: &World, item: Entity) -> bool {
    item_path(world, item).is_some_and(|p| RAD_PROTECTED.iter().any(|r| is_under(p, r)))
}

/// The worn MOD control units with an activated radiation protection module, and their parts
/// (`rad_protection/on_part_activation`).
fn mod_protected(world: &World, inventory: &Inventory) -> Vec<Entity> {
    use crate::modsuit::modules::{Kind, ModModule};
    let mut out = Vec::new();
    for item in inventory.equipped() {
        let Some(suit) = world.get::<crate::modsuit::server::ModSuit>(item) else {
            continue;
        };
        let active = suit.modules.iter().any(|&m| {
            world
                .get::<ModModule>(m)
                .is_some_and(|s| s.kind == Kind::RadProtection && s.part_activated)
        });
        if active {
            out.push(item);
            out.extend(suit.parts.iter().map(|p| p.item));
        }
    }
    out
}

/// `SSradiation.wearing_rad_protected_clothing`: every bodypart is covered by
/// radiation-protected clothing.
pub fn wearing_rad_protected_clothing(world: &World, mob: Entity) -> bool {
    let (Some(inventory), Some(health)) = (world.get::<Inventory>(mob), world.get::<Health>(mob))
    else {
        return false;
    };
    let mods = mod_protected(world, inventory);
    let covered: Vec<u32> = inventory
        .equipped()
        .filter(|&i| rad_protected_type(world, i) || mods.contains(&i))
        .filter_map(|i| world.get::<DmClothing>(i).map(|c| c.body_parts_covered))
        .collect();
    BodyZone::ALL
        .into_iter()
        .filter(|&z| health.has_part(z))
        .all(|z| {
            let bit = crate::chem::body_part_bit(z);
            covered.iter().any(|c| c & bit != 0)
        })
}

/// tg `radiation_pulse(source, max_range, threshold, chance, minimum_exposure_time)`: queued
/// for the subsystem. False when the source is nowhere.
pub fn radiation_pulse(
    world: &mut World,
    source: Entity,
    max_range: f32,
    threshold: f32,
    chance: f32,
    minimum_exposure_time: f64,
) -> bool {
    let Some(origin) = crate::explosions::turf_of(world, source) else {
        return false;
    };
    world
        .get_resource_or_init::<RadiationQueue>()
        .pulses
        .push(Pulse {
            source,
            origin,
            max_range,
            threshold,
            chance,
            minimum_exposure_time,
        });
    true
}

/// `SSradiation.irradiate`: irradiates `target` unless it cannot be or is protected.
pub fn irradiate(world: &mut World, target: Entity) -> bool {
    if !can_irradiate_basic(world, target) {
        return false;
    }
    irradiate_after_basic_checks(world, target);
    true
}

fn irradiate_after_basic_checks(world: &mut World, target: Entity) -> bool {
    if is_human(world, target) && wearing_rad_protected_clothing(world, target) {
        return false;
    }
    add_irradiated(world, target);
    true
}

/// `AddComponent(/datum/component/irradiated)` (`COMPONENT_DUPE_UNIQUE`: once).
pub fn add_irradiated(world: &mut World, target: Entity) {
    if is_irradiated(world, target) || !can_irradiate(world, target) || rad_immune(world, target) {
        return;
    }
    let now = now(world);
    let burn_in = {
        let mut rng = world.resource_mut::<RadiationRng>();
        rng.0.range(BURN_INTERVAL.0, BURN_INTERVAL.1)
    };
    world.entity_mut(target).insert(Irradiated {
        since: now,
        next_tox: now + TOX_INTERVAL,
        next_burn: now + f64::from(burn_in),
        clean_until: 0.0,
    });
    if is_human(world, target) {
        damage(world, target, DamageType::Tox, IMMEDIATE_TOX);
        if let Some(mut alerts) = world.get_mut::<Alerts>(target) {
            alerts.throw(AlertCategory::Irradiated, "irradiated", 0);
        }
    }
}

/// The component's `Destroy`.
fn remove_irradiated(world: &mut World, target: Entity) {
    world.entity_mut(target).remove::<Irradiated>();
    if let Some(mut alerts) = world.get_mut::<Alerts>(target)
        && alerts.get(AlertCategory::Irradiated).is_some()
    {
        alerts.clear(AlertCategory::Irradiated);
    }
}

fn damage(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    else {
        return;
    };
    let mut new = stat;
    crate::health::apply_damage(&mut health, &mut new, kind, amount);
    world.entity_mut(mob).insert(health);
    if new != stat {
        world.entity_mut(mob).insert(new);
    }
}

/// `get_line(a, b)`, exactly as tg steps it.
pub fn get_line(a: TilePos, b: TilePos) -> Vec<TilePos> {
    let (mut cx, mut cy) = (a.x, a.y);
    let mut line = vec![a];
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (ax, ay) = (dx.abs(), dy.abs());
    let (sx, sy) = (dx.signum(), dy.signum());
    let (mut x, mut y) = (ax >> 1, ay >> 1);
    if ax >= ay {
        for _ in 0..ax {
            y += ay;
            if y >= ax {
                y -= ax;
                cy += sy;
            }
            cx += sx;
            line.push(TilePos::new(cx, cy, a.z));
        }
    } else {
        for _ in 0..ay {
            x += ax;
            if x >= ay {
                x -= ay;
                cx += sx;
            }
            cy += sy;
            line.push(TilePos::new(cx, cy, a.z));
        }
    }
    line
}

/// `get_perceived_radiation_danger`.
// tg returns MEDIUM when the insulation is at most half the threshold and LOW above that, the
// reverse of "out of the threshold, but by how much"; here the closer reading is the higher.
pub fn perceived_danger(threshold: f32, chance: f32, insulation: f32) -> u8 {
    if insulation > threshold {
        if chance >= EXTREME_RADIATION_CHANCE {
            DANGER_EXTREME
        } else {
            DANGER_HIGH
        }
    } else if threshold > 0.0 && insulation / threshold > MEDIUM_RADIATION_THRESHOLD_RANGE {
        DANGER_MEDIUM
    } else {
        DANGER_LOW
    }
}

/// The chance (percent) a target `distance` tiles away behind `insulation` is irradiated.
pub fn perceived_chance(pulse: &Pulse, distance: f32, insulation: f32) -> f32 {
    if pulse.chance >= 100.0 {
        return 100.0;
    }
    let intensity = -(1.0 - pulse.chance / 100.0).ln() * (1.0 + pulse.max_range / 2.0).powi(2);
    let mut perceived = intensity / (1.0 + distance).powi(2);
    perceived *= (insulation - pulse.threshold) / (1.0 - pulse.threshold);
    100.0 * (1.0 - (-perceived).exp())
}

/// `SSradiation.fire`: every queued pulse.
fn fire_pulses(world: &mut World) {
    let now = now(world);
    let mut queue = world.resource_mut::<RadiationQueue>();
    if queue.next_fire > now {
        return;
    }
    queue.next_fire = now + SS_WAIT;
    let pulses = std::mem::take(&mut queue.pulses);
    for p in pulses {
        if world.get_entity(p.source).is_err() {
            continue;
        }
        pulse(world, &p, now);
    }
}

/// `SSradiation.pulse`.
pub fn pulse(world: &mut World, p: &Pulse, now: f64) {
    let r = p.max_range.max(0.0).floor() as i32;
    let o = p.origin;
    let mut cached: HashMap<TilePos, f32> = HashMap::new();
    for y in o.y - r..=o.y + r {
        for x in o.x - r..=o.x + r {
            let tile = TilePos::new(x, y, o.z);
            let index = world.resource::<SpatialIndex>();
            if !index.in_bounds(tile) {
                continue;
            }
            let targets: Vec<Entity> = index.on_tile(tile).to_vec();
            for target in targets {
                if !can_irradiate_basic(world, target) {
                    continue;
                }
                let mut insulation = 1.0;
                for t in get_line(o, tile).into_iter().skip(1) {
                    let here = *cached.entry(t).or_insert_with(|| tile_insulation(world, t));
                    insulation *= here;
                    if insulation <= p.threshold {
                        break;
                    }
                }
                // `COMSIG_IN_RANGE_OF_IRRADIATION`.
                let danger = perceived_danger(p.threshold, p.chance, insulation);
                notify_geigers(world, target, danger, now);
                if is_irradiated(world, target) || insulation <= p.threshold {
                    continue;
                }
                let dx = (tile.x - o.x) as f32;
                let dy = (tile.y - o.y) as f32;
                let chance = perceived_chance(p, (dx * dx + dy * dy).sqrt(), insulation);
                // `COMSIG_IN_THRESHOLD_OF_IRRADIATION` -> `radiation_countdown`.
                let mut skip_exposure = false;
                if let Some(mut cd) = world.get_mut::<RadiationCountdown>(target) {
                    cd.minimum = cd.minimum.min(p.minimum_exposure_time);
                    cd.expires = now + COUNTDOWN_LIFETIME;
                    if now - cd.added < cd.minimum {
                        continue;
                    }
                    skip_exposure = true;
                }
                if p.minimum_exposure_time > 0.0 && !skip_exposure {
                    world.entity_mut(target).insert(RadiationCountdown {
                        added: now,
                        minimum: p.minimum_exposure_time,
                        expires: now + COUNTDOWN_LIFETIME,
                    });
                    crate::chem::tell_mob(
                        world,
                        target,
                        Span::Danger,
                        "Воздух вокруг кажется тёплым... пожалуй, стоит уйти отсюда.",
                    );
                    continue;
                }
                if !world.resource_mut::<RadiationRng>().0.chance(chance) {
                    continue;
                }
                if irradiate_after_basic_checks(world, target) {
                    info!(target: "radiation", "{target:?} was irradiated by {:?}", p.source);
                }
            }
        }
    }
}

/// A tile's `rad_insulation` times everything on it.
fn tile_insulation(world: &World, tile: TilePos) -> f32 {
    world
        .resource::<SpatialIndex>()
        .on_tile(tile)
        .iter()
        .map(|&e| {
            world
                .get::<DmRadInsulation>(e)
                .map_or(RAD_NO_INSULATION, |r| r.value)
        })
        .product()
}

/// The counter's `on_pre_potential_irradiation`: on the counter itself or its holder.
fn notify_geigers(world: &mut World, target: Entity, danger: u8, now: f64) {
    let mut counters = Vec::new();
    if is_geiger(world, target) {
        counters.push(target);
    }
    if let Some(inventory) = world.get::<Inventory>(target) {
        counters.extend(
            ALL_SLOTS
                .iter()
                .filter_map(|&s| inventory.slot(s))
                .filter(|&i| is_geiger(world, i)),
        );
    }
    for counter in counters {
        let mut g = geiger_mut(world, counter);
        g.danger = Some(danger);
        g.reset_at = now + GEIGER_RESET;
        let scanning = g.scanning;
        if scanning {
            refresh_geiger(world, counter);
        }
    }
}

fn geiger_mut(world: &mut World, counter: Entity) -> Mut<'_, Geiger> {
    if world.get::<Geiger>(counter).is_none() {
        world.entity_mut(counter).insert(Geiger::default());
    }
    world.get_mut::<Geiger>(counter).expect("inserted")
}

/// `update_icon_state`.
fn refresh_geiger(world: &mut World, counter: Entity) {
    let g = world.get::<Geiger>(counter).copied().unwrap_or_default();
    let state = if g.scanning {
        format!("geiger_on_{}", g.danger.unwrap_or(0) + 1)
    } else {
        "geiger_off".to_owned()
    };
    let Some(icon) = world.get::<DmAtom>(counter).map(|a| a.icon.clone()) else {
        return;
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, &state));
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<Appearance>(counter)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

/// `attack_self`: switches the counter on or off.
pub fn geiger_attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    if !is_geiger(world, item) {
        return false;
    }
    let mut g = geiger_mut(world, item);
    g.scanning = !g.scanning;
    let on = g.scanning;
    refresh_geiger(world, item);
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        if on {
            "включено"
        } else {
            "выключено"
        },
    );
    true
}

/// `interact_with_atom` -> `ranged_interact_with_atom`: a human or item is scanned in 2 s.
// shortcut: no `SHOULD_SKIP_INTERACTION` (tables in combat mode).
pub fn geiger_interact(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    if !is_geiger(world, held) || !can_irradiate(world, target) {
        return false;
    }
    let user = crate::medical::name_of(world, mob);
    let name = crate::medical::name_of(world, target);
    crate::chem::visible(
        world,
        mob,
        &format!("Вы сканируете уровень радиации {name}..."),
        &format!("{user} сканирует {name} счётчиком Гейгера."),
        Span::Notice,
    );
    let at = now(world) + GEIGER_SCAN_DELAY;
    let mut g = geiger_mut(world, held);
    if g.scan.is_none() {
        g.scan = Some((at, target, mob));
    }
    true
}

/// `scan`.
fn geiger_scan(world: &mut World, target: Entity, user: Entity) {
    if world.get_entity(target).is_err() {
        return;
    }
    let living = world.get::<Health>(target).is_some();
    let text = match world.get::<Irradiated>(target) {
        Some(irr) if living => {
            let ago = ((now(world) - irr.since) / 5.0).floor() * 5.0;
            let tox = world.get::<Health>(target).map_or(0.0, |h| h.tox);
            format!(
                "Субъект облучён. Заражение произошло примерно {} назад. Текущий уровень \
                 токсинов: {}.",
                crate::jobs::display_time_text(ago),
                crate::chem::dm_num(tox)
            )
        }
        Some(_) => "Цель облучена.".to_owned(),
        None if living => "Субъект не заражён радиацией.".to_owned(),
        None => "Цель не заражена радиацией.".to_owned(),
    };
    let span = if is_irradiated(world, target) {
        Span::Danger
    } else {
        Span::Notice
    };
    crate::chem::tell_mob(world, user, span, &text);
}

/// `click_alt`: clears the stored level. True when `item` is a counter.
pub fn geiger_alt_click(world: &mut World, client: Entity, item: Entity) -> bool {
    if !is_geiger(world, item) {
        return false;
    }
    if !geiger_mut(world, item).scanning {
        crate::tools::tell(
            world,
            client,
            Span::Warning,
            "Счётчик Гейгера должен быть включён, чтобы сбросить уровень радиации!",
        );
        return true;
    }
    geiger_mut(world, item).danger = None;
    refresh_geiger(world, item);
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        "Вы сбрасываете показания счётчика Гейгера.",
    );
    true
}

/// The counter's and the protected clothing's examine lines.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if rad_protected_type(world, target) {
        lines.push((
            Span::Notice,
            "Нашивка со знаком радиационной опасности говорит, что это защищает от радиации."
                .to_owned(),
        ));
    }
    let Some(g) = world.get::<Geiger>(target).filter(|g| g.scanning) else {
        return;
    };
    lines.push((
        Span::Info,
        "Alt-клик сбрасывает сохранённый уровень радиации.".to_owned(),
    ));
    lines.push(match g.danger {
        None => (Span::Notice, "Фоновый уровень радиации в норме.".to_owned()),
        Some(DANGER_LOW) => (
            Span::Warning,
            "Фоновый уровень радиации немного выше среднего.".to_owned(),
        ),
        Some(DANGER_MEDIUM) => (
            Span::Warning,
            "Фоновый уровень радиации выше среднего.".to_owned(),
        ),
        Some(DANGER_HIGH) => (
            Span::Danger,
            "Фоновый уровень радиации значительно выше среднего.".to_owned(),
        ),
        Some(_) => (
            Span::BoldWarning,
            "Фоновый уровень радиации достигает критического!".to_owned(),
        ),
    });
}

/// The irradiated component's health scan line (`COMSIG_LIVING_HEALTHSCAN`).
pub fn scan_line(world: &World, target: Entity) -> Option<&'static str> {
    is_irradiated(world, target).then_some("Субъект облучён.")
}

/// The component's `on_clean` (`COMSIG_COMPONENT_CLEAN_ACT`): a radiation wash removes it from
/// an item and halts a mob's effects for a moment.
pub fn clean_act(world: &mut World, target: Entity, clean_types: u32) -> bool {
    if clean_types & CLEAN_TYPE_RADIATION == 0 || !is_irradiated(world, target) {
        return false;
    }
    if world.get::<DmItem>(target).is_some() {
        remove_irradiated(world, target);
        return true;
    }
    let until = now(world) + CLEAN_IMMUNITY;
    if let Some(mut irr) = world.get_mut::<Irradiated>(target) {
        irr.clean_until = until;
    }
    false
}

/// `TRAIT_HALT_RADIATION_EFFECTS` (potassium iodide, pentetic acid) or a recent wash.
fn halted(world: &World, mob: Entity, irr: &Irradiated, now: f64) -> bool {
    use crate::chem::blood_amount;
    irr.clean_until > now
        || blood_amount(world, mob, POTASS_IODIDE) > 0.0
        || blood_amount(world, mob, PEN_ACID) > 0.0
}

/// The radiation side of `on_mob_life`, every `SSmobs` tick (`crate::chem`): potassium iodide
/// heals the irradiated, uranium and polonium irradiate.
// shortcut: `metabolization_ratio` is taken as 1 (a reagent at its own rate).
pub fn reagents(world: &mut World, mob: Entity, secs: f32) {
    use crate::chem::blood_amount;
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    let irradiated = is_irradiated(world, mob);
    if irradiated && blood_amount(world, mob, POTASS_IODIDE) > 0.0 {
        damage(world, mob, DamageType::Tox, -0.25 * secs);
    }
    // `/datum/reagent/uranium` (rad_power 1) and `toxin/polonium` (rad_power 3).
    for (path, rad_power) in [(URANIUM, 1.0f32), (POLONIUM, 3.0)] {
        let volume = blood_amount(world, mob, path);
        if volume <= 0.0 {
            continue;
        }
        if !is_irradiated(world, mob) && can_irradiate_basic(world, mob) {
            let chance = (volume / (20.0 - rad_power * 5.0)).min(rad_power);
            let roll = {
                let mut rng = world.resource_mut::<RadiationRng>();
                crate::medical::wounds::spt_prob(&mut rng.0, chance, secs)
            };
            // "ignore rad protection calculations bc it's inside of us".
            if roll {
                add_irradiated(world, mob);
            }
        } else if path == POLONIUM {
            damage(world, mob, DamageType::Tox, 4.0 * secs);
        }
        if path == URANIUM {
            // `tox_damage * seconds_per_tick * metabolization_rate` (0.25 * s * 0.2).
            damage(
                world,
                mob,
                DamageType::Tox,
                0.25 * secs * ss13_reagents::REAGENTS_METABOLISM,
            );
        }
    }
}

/// Timers of the components: tox and burns, hair, countdowns, counters' resets and scans.
fn process_radiation(world: &mut World) {
    let now = now(world);
    let due = {
        let mut queue = world.resource_mut::<RadiationQueue>();
        let due = queue.next_process <= now;
        if due {
            queue.next_process = now + SSOBJ_DT;
        }
        due
    };
    let mut q = world.query::<(Entity, &Irradiated)>();
    let irradiated: Vec<(Entity, Irradiated)> = q.iter(world).map(|(e, i)| (e, *i)).collect();
    for (mob, irr) in irradiated {
        if !is_human(world, mob) {
            continue;
        }
        if now >= irr.next_burn {
            let next = {
                let mut rng = world.resource_mut::<RadiationRng>();
                now + f64::from(rng.0.range(BURN_INTERVAL.0, BURN_INTERVAL.1))
            };
            if let Some(mut i) = world.get_mut::<Irradiated>(mob) {
                i.next_burn = next;
            }
            if !halted(world, mob, &irr, now) {
                burn_splotches(world, mob);
            }
        }
        if due {
            process_irradiated(world, mob, irr, now, SSOBJ_DT as f32);
        }
    }
    let mut q = world.query::<(Entity, &RadiationCountdown)>();
    let expired: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.expires <= now)
        .map(|(e, _)| e)
        .collect();
    for e in expired {
        world.entity_mut(e).remove::<RadiationCountdown>();
        if !is_irradiated(world, e) {
            crate::chem::tell_mob(world, e, Span::Notice, "Здесь воздух кажется безопаснее.");
        }
    }
    let mut q = world.query::<(Entity, &GoingBald)>();
    let bald: Vec<Entity> = q
        .iter(world)
        .filter(|(_, b)| b.0 <= now)
        .map(|(e, _)| e)
        .collect();
    for mob in bald {
        world.entity_mut(mob).remove::<GoingBald>();
        go_bald(world, mob);
    }
    let mut q = world.query::<(Entity, &Geiger)>();
    let counters: Vec<(Entity, Geiger)> = q.iter(world).map(|(e, g)| (e, *g)).collect();
    for (counter, g) in counters {
        if g.danger.is_some() && g.reset_at <= now {
            geiger_mut(world, counter).danger = None;
            if g.scanning {
                refresh_geiger(world, counter);
            }
        }
        if let Some((at, target, user)) = g.scan
            && at <= now
        {
            geiger_mut(world, counter).scan = None;
            geiger_scan(world, target, user);
        }
    }
}

/// The irradiated component's `process(seconds_per_tick)` on a human.
fn process_irradiated(world: &mut World, mob: Entity, irr: Irradiated, now: f64, secs: f32) {
    if rad_immune(world, mob) || world.get::<Health>(mob).is_none_or(|h| h.tox <= 0.0) {
        remove_irradiated(world, mob);
        return;
    }
    if halted(world, mob, &irr, now) {
        return;
    }
    if world.get::<Stat>(mob) != Some(&Stat::Dead) {
        handle_radiation(world, mob, now - irr.since, secs);
    }
    if now >= irr.next_tox {
        damage(world, mob, DamageType::Tox, TOX_PER_INTERVAL);
        if let Some(mut i) = world.get_mut::<Irradiated>(mob) {
            i.next_tox = now + TOX_INTERVAL;
        }
    }
}

/// `/datum/species/proc/handle_radiation`.
fn handle_radiation(world: &mut World, mob: Entity, since: f64, secs: f32) {
    use crate::medical::wounds::spt_prob;
    let roll = |world: &mut World, percent: f32| {
        let mut rng = world.resource_mut::<RadiationRng>();
        spt_prob(&mut rng.0, percent, secs)
    };
    if since > RAD_MOB_KNOCKDOWN && roll(world, RAD_MOB_KNOCKDOWN_PROB) {
        let paralyzed = world
            .get::<crate::status_effects::StatusEffects>(mob)
            .is_some_and(|s| s.paralyze > 0.0);
        if !paralyzed {
            crate::medical::diseases::emote(world, mob, "collapse");
        }
        crate::status_effects::apply(
            world,
            mob,
            crate::status_effects::Incap::Paralyze,
            RAD_MOB_KNOCKDOWN_AMOUNT,
        );
        crate::chem::tell_mob(world, mob, Span::Danger, "Вы чувствуете слабость.");
    }
    if since > RAD_MOB_VOMIT && roll(world, RAD_MOB_VOMIT_PROB) {
        // `VOMIT_CATEGORY_BLOOD`, `lost_nutrition = 10`.
        crate::medical::diseases::vomit(world, mob, 10.0, true, true, 1);
    }
    if since > RAD_MOB_MUTATE && roll(world, RAD_MOB_MUTATE_PROB) {
        crate::chem::tell_mob(world, mob, Span::Danger, "Вы мутируете!");
        crate::genetics::easy_random_mutate(
            world,
            mob,
            crate::genetics::NEGATIVE | crate::genetics::MINOR_NEGATIVE,
        );
        crate::medical::diseases::emote(world, mob, "gasp");
        crate::genetics::domutcheck(world, mob);
    }
    if since > RAD_MOB_HAIRLOSS && roll(world, RAD_MOB_HAIRLOSS_PROB) {
        let hair = world
            .get::<HumanLook>(mob)
            .is_some_and(|l| l.hair_style.is_some());
        let head = world
            .get::<Health>(mob)
            .is_some_and(|h| h.has_part(BodyZone::Head));
        if hair && head {
            crate::chem::tell_mob(
                world,
                mob,
                Span::Danger,
                "Ваши волосы начинают выпадать клочьями...",
            );
            if world.get::<GoingBald>(mob).is_none() {
                let at = now(world) + BALD_DELAY;
                world.entity_mut(mob).insert(GoingBald(at));
            }
        }
    }
}

/// `/datum/species/proc/go_bald`.
fn go_bald(world: &mut World, mob: Entity) {
    let Some(mut look) = world.get_mut::<HumanLook>(mob) else {
        return;
    };
    if look.hair_style.is_none() {
        return;
    }
    look.hair_style = None;
    crate::inventory::refresh_look(world, mob);
}

/// `give_burn_splotches`.
// shortcut: no extra line for the blind (`is_blind`).
fn burn_splotches(world: &mut World, mob: Entity) {
    let zone = {
        let mut rng = world.resource_mut::<RadiationRng>();
        crate::medical::random_valid_zone(&mut rng.0, BodyZone::Chest, 0.0)
    };
    let zone = if world.get::<Health>(mob).is_some_and(|h| h.has_part(zone)) {
        zone
    } else {
        BodyZone::Chest
    };
    let name = crate::medical::name_of(world, mob);
    let part = zone.ru().0;
    crate::chem::visible(
        world,
        mob,
        &format!("Ваша {part} неестественно пузырится и покрывается волдырями!"),
        &format!("{name}: {part} неестественно пузырится и покрывается волдырями!"),
        Span::BoldWarning,
    );
    crate::medical::hurt_limb(world, mob, zone, DamageType::Burn, BURN_SPLOTCH);
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        playsound(world, PlaySound::new(pos, "sizzle", 50.0, true));
    }
}

// ---------------------------------------------------------------------------------------------
// Sources.

fn is_uranium_wall(world: &World, e: Entity) -> bool {
    world
        .get::<DmTurfBuild>(e)
        .is_some_and(|t| is_under(&t.path, URANIUM_WALL))
        || world
            .get::<DmObj>(e)
            .is_some_and(|o| is_under(&o.path, URANIUM_FALSEWALL))
}

/// Uranium (false) walls' `radiate()` and `propagate_radiation_pulse()`: the touched wall and,
/// through `COMSIG_ATOM_PROPAGATE_RAD_PULSE`, every adjacent uranium wall off cooldown.
pub fn uranium_radiate(world: &mut World, wall: Entity) {
    let now = now(world);
    let mut seen = HashSet::new();
    let mut stack = vec![wall];
    while let Some(e) = stack.pop() {
        if !seen.insert(e) || !is_uranium_wall(world, e) {
            continue;
        }
        if world
            .get::<UraniumLastPulse>(e)
            .is_some_and(|l| now <= l.0 + URANIUM_COOLDOWN)
        {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(e) else {
            continue;
        };
        radiation_pulse(
            world,
            e,
            URANIUM_RANGE,
            RAD_LIGHT_INSULATION,
            URANIUM_IRRADIATION_CHANCE,
            URANIUM_RADIATION_MINIMUM_EXPOSURE_TIME,
        );
        world.entity_mut(e).insert(UraniumLastPulse(now));
        let index = world.resource::<SpatialIndex>();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let t = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                if (dx, dy) != (0, 0) && index.in_bounds(t) {
                    stack.extend(index.on_tile(t).iter().copied());
                }
            }
        }
    }
}

/// `attack_hand`, `attackby` and (walls) `Bumped` of uranium walls.
fn uranium_touches(
    world: &mut World,
    mut clicks: Local<MessageCursor<ClickResolved>>,
    mut bumps: Local<MessageCursor<Bumped>>,
) {
    let mut touched: Vec<Entity> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .map(|c| c.target)
        .collect();
    touched.extend(
        bumps
            .read(world.resource::<Messages<Bumped>>())
            .map(|b| b.obstacle)
            .filter(|&o| world.get::<DmTurfBuild>(o).is_some()),
    );
    for e in touched {
        if is_uranium_wall(world, e) {
            uranium_radiate(world, e);
        }
    }
}

/// `ranged_interact_with_atom` of a held counter.
fn geiger_ranged(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        if let Some(held) = world
            .get::<Inventory>(click.mob)
            .and_then(|inv| inv.active_item())
        {
            geiger_interact(world, click.mob, held, click.target);
        }
    }
}

/// The pulses of gas reactions (`crate::atmos`): from the reacting turf, default chance.
fn atmos_pulses(world: &mut World) {
    let Some(pulses) = world
        .get_resource_mut::<ss13_atmos::Atmos>()
        .map(|mut a| a.drain_radiation())
    else {
        return;
    };
    for (pos, r) in pulses {
        if !world.resource_mut::<RadiationRng>().0.chance(r.prob as f32) {
            continue;
        }
        let Some(turf) = crate::construction::turf_at(world, pos) else {
            continue;
        };
        radiation_pulse(
            world,
            turf,
            r.max_range as f32,
            r.threshold as f32,
            DEFAULT_RADIATION_CHANCE,
            0.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_line_steps_like_tg() {
        let a = TilePos::new(0, 0, 1);
        let line = get_line(a, TilePos::new(4, 2, 1));
        let xy: Vec<(i32, i32)> = line.iter().map(|p| (p.x, p.y)).collect();
        // x major: y advances at counter 1 (1+2 >= 4 at the second step) and the fourth.
        assert_eq!(xy, [(0, 0), (1, 0), (2, 1), (3, 1), (4, 2)]);
        assert_eq!(get_line(a, a), [a]);
        let down: Vec<(i32, i32)> = get_line(a, TilePos::new(0, -3, 1))
            .iter()
            .map(|p| (p.x, p.y))
            .collect();
        assert_eq!(down, [(0, 0), (0, -1), (0, -2), (0, -3)]);
    }

    #[test]
    fn chance_is_the_pulse_chance_at_half_range() {
        let p = Pulse {
            source: Entity::PLACEHOLDER,
            origin: TilePos::new(0, 0, 0),
            max_range: 8.0,
            threshold: 0.0,
            chance: 30.0,
            minimum_exposure_time: 0.0,
        };
        assert!((perceived_chance(&p, 4.0, 1.0) - 30.0).abs() < 1e-3);
        // Farther and behind a wall (0.7) it drops.
        assert!(perceived_chance(&p, 8.0, 1.0) < 30.0);
        assert!(perceived_chance(&p, 4.0, 0.7) < perceived_chance(&p, 4.0, 1.0));
        let sure = Pulse { chance: 100.0, ..p };
        assert_eq!(perceived_chance(&sure, 20.0, 0.1), 100.0);
    }

    #[test]
    fn perceived_danger_levels() {
        assert_eq!(perceived_danger(0.5, 30.0, 0.9), DANGER_EXTREME);
        assert_eq!(perceived_danger(0.5, 10.0, 0.9), DANGER_HIGH);
        assert_eq!(perceived_danger(0.5, 10.0, 0.4), DANGER_MEDIUM);
        assert_eq!(perceived_danger(0.5, 10.0, 0.2), DANGER_LOW);
        assert_eq!(perceived_danger(0.0, 10.0, 0.0), DANGER_LOW);
    }
}
