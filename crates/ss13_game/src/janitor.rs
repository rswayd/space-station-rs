//! Cleaning, blood on the floor, wet floors and slipping (server), after tg
//! `code/game/objects/effects/decals/cleanable.dm`, `cleanable/blood.dm`,
//! `mob/living/blood.dm` (`bleed`, `add_splatter_floor`), `blood_types.dm`
//! (`make_blood_splatter`), `datums/components/cleaner.dm`, `wet_floor.dm`, `slippery.dm`,
//! `turfs/open/_open.dm` (`handle_slip`, `MakeSlippery`), `items/mop.dm`,
//! `structures/janitor.dm`, `items/clown_items.dm` (soap), `reagent_containers/spray.dm` and the
//! `expose_turf`/`expose_obj` of water and space cleaner.
//!
//! - Every `/obj/effect/decal/cleanable` (map-placed or spawned) is [`Cleanable`]; `wash()` with
//!   its `clean_type` deletes it. A turf's wash washes the cleanables on it (`TRAIT_MOPABLE`),
//!   or everything with `all_contents` (space cleaner).
//! - A bleeding mob on a turf leaves blood: each `bleed(amount)` rolls
//!   `prob(sqrt(amount) × 90)`; up to 10 units is a drip (up to five drips on a tile, the sixth
//!   turns them into a pool), more is a pool, which grows by 50 bloodiness instead of stacking.
//!   Blood dries after 5 minutes.
//! - [`WetFloor`] on a turf keeps a time per wetness kind (`time_left_list`), set by
//!   `add_wet(min, add, max)`; every second (`SSwet_floors`) it dries by
//!   `(T − 0 °C) / 2 × (1 s / 1.5 s)` deciseconds (instantly above 100 °C).
//! - Entering a wet tile slips a living mob that is standing (water: unless it walks): 0.5 s
//!   paralyze (BandaStation), a knockdown of the wetness' intensity (water 6 s) and 3 s daze.
//! - A mop (15 u) is wetted with a right click on a mop bucket or janitorial cart (100 u, open
//!   container, filled by pouring); a click with it cleans the clicked thing after 1.5 s:
//!   washes it if it holds 1 u of a cleaning reagent (water, space cleaner), then exposes it to
//!   ten times its reagents (water ≥ 5 u wets the floor: `MakeSlippery(water, 10 s, 60 s)`)
//!   and loses 1 u. Soap cleans after its `cleanspeed` and loses a use.
//! - Spray bottles are `crate::sprays`; `expose_by` is `reagents.expose(target, methods, ...)`:
//!   what the reagents do to a turf, an object or a mob, and what soaks into the blood
//!   (`penetrates_skin`, less bio armour).
//! - `ClientIntent::ToggleMoveIntent` switches the mob between running and walking
//!   (`WALK_DELAY` 4 deciseconds per tile).
// The cleaning skill (`crate::skills`) shortens the do_after (`min(speed + offset, 1)`), soaks
// less from the mop and spares soap uses; a clean that washed gives experience.
// shortcut: no bubbles overlay,
// no `TRAIT_CURRENTLY_CLEANING` (two mobs may scrub one tile at once), mobs are not washed
// (soap and mops skip them; items lose their traces, `crate::forensics`); no footprints, blood
// trails, flying hit splatters or bloody shoes; blood bloodiness does not decay while drying and dried blood keeps its name;
// no ice (wet floors below 0 °C just do not dry), lube, galoshes or noslip shoes, no sliding
// and no gravity check; the advanced mop does not self-wet; the cart holds no tools; no
// sinks, so buckets are filled by pouring.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_atmos::gas::{T0C, T20C};
use ss13_mapload::{DmCleanable, DmItem, DmObj, DmTurf, IconIndex, Turf};
use ss13_net::server::IntentReceived;
use ss13_reagents::{Holder, Owner, flags, round_to};
use ss13_shared::chat::Span;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_world::{Density, Moved, SpatialIndex, WorldSystems};

use crate::chem::{Chem, Reagents, TransferAmount};
use crate::combat::CombatRng;
use crate::construction::{Job, turf_at};
use crate::health::{BodyPosition, DamageType, Health, HealthSystems, Stat};
use crate::obj_damage::is_under;
use crate::server::{PlayerMob, Players};
use crate::status_effects::Incap;
use crate::tools::{Actor, DoAfter, do_after, tell};

/// tg `WALK_DELAY 4` deciseconds (`config/game_options.txt`).
pub const WALK_DELAY_SECS: f32 = 0.4;

/// `CLEAN_TYPE_*` (`code/__DEFINES/cleaning.dm`).
pub const CLEAN_TYPE_BLOOD: u32 = 1 << 0;
pub const CLEAN_TYPE_DISEASE: u32 = 1 << 4;
pub const CLEAN_TYPE_ACID: u32 = 1 << 5;
pub const CLEAN_TYPE_LIGHT_DECAL: u32 = 1 << 6;
pub const CLEAN_TYPE_HARD_DECAL: u32 = 1 << 7;
pub const CLEAN_WASH: u32 =
    CLEAN_TYPE_BLOOD | CLEAN_TYPE_DISEASE | CLEAN_TYPE_ACID | CLEAN_TYPE_LIGHT_DECAL;
/// `CLEAN_SCRUB`: also fingerprints and fibers (`crate::forensics::server`).
pub const CLEAN_SCRUB: u32 = CLEAN_WASH
    | crate::forensics::server::CLEAN_TYPE_FINGERPRINTS
    | crate::forensics::server::CLEAN_TYPE_FIBERS
    | CLEAN_TYPE_HARD_DECAL;

/// `TURF_WET_*` (`code/__DEFINES/turfs.dm`), in tg's strength order.
pub const TURF_WET_WATER: u8 = 1 << 0;
pub const TURF_WET_PERMAFROST: u8 = 1 << 1;
pub const TURF_WET_ICE: u8 = 1 << 2;
pub const TURF_WET_LUBE: u8 = 1 << 3;
pub const TURF_WET_SUPERLUBE: u8 = 1 << 4;
/// `MAXIMUM_WET_TIME`, seconds.
pub const MAXIMUM_WET_TIME: f32 = 300.0;
/// `SSwet_floors`: `wait`, `temperature_coeff`, `time_ratio`.
const WET_FLOORS_WAIT: f64 = 1.0;
const WET_TEMPERATURE_COEFF: f64 = 2.0;
const WET_TIME_RATIO: f64 = 1.5;

/// `BLOOD_DRIP_RATE_MOD`.
const BLOOD_DRIP_RATE_MOD: f32 = 90.0;
/// `BLOOD_AMOUNT_PER_DECAL`.
pub const BLOOD_AMOUNT_PER_DECAL: f32 = 50.0;
/// `/obj/effect/decal/cleanable/blood` `drying_time`.
pub const BLOOD_DRYING_SECS: f64 = 300.0;
const BLOOD: &str = "/obj/effect/decal/cleanable/blood";
const BLOOD_DRIP: &str = "/obj/effect/decal/cleanable/blood/drip";
const BLOOD_DMI: &str = "icons/effects/blood.dmi";
const BLOOD_STATES: [&str; 7] = [
    "floor1", "floor2", "floor3", "floor4", "floor5", "floor6", "floor7",
];
const DRIP_STATES: [&str; 5] = ["drip1", "drip2", "drip3", "drip4", "drip5"];
/// `/obj/effect/decal/cleanable/blood/old` `color`: what dried blood looks like.
const DRIED_BLOOD_COLOR: [u8; 4] = [55, 4, 4, 255];
const WATER_DMI: &str = "icons/effects/water.dmi";

const MOP: &str = "/obj/item/mop";
const MOP_ADVANCED: &str = "/obj/item/mop/advanced";
const MOP_BUCKET: &str = "/obj/structure/mop_bucket";
const JANICART: &str = "/obj/structure/mop_bucket/janitorialcart";
const SOAP: &str = "/obj/item/soap";
const SPRAY: &str = "/obj/item/reagent_containers/spray";
const FUEL: &str = "/datum/reagent/fuel";
const URANIUM: &str = "/datum/reagent/uranium";
const CAPSAICIN: &str = "/datum/reagent/consumable/condensedcapsaicin";
const ACID: &str = "/datum/reagent/toxin/acid";
/// The exposure `methods`.
pub const TOUCH: u32 = 1 << 0;
pub const INGEST: u32 = 1 << 1;
pub const VAPOR: u32 = 1 << 2;
pub const PATCH: u32 = 1 << 3;
pub const INJECT: u32 = 1 << 4;
pub const INHALE: u32 = 1 << 6;
pub const SMOKE_MACHINE: u32 = TOUCH | INHALE;
const WATER: &str = "/datum/reagent/water";
const SPACE_CLEANER: &str = "/datum/reagent/space_cleaner";
const FIREFIGHTING_FOAM: &str = "/datum/reagent/firefighting_foam";
/// `/datum/reagent/water::cooling_temperature`.
pub const WATER_COOLING_TEMPERATURE: f32 = 2.0;
/// `REAGENT_CLEANS`.
const REAGENT_CLEANS: u32 = 1 << 7;
/// `/obj/item/mop` `mopspeed`, deciseconds.
const MOPSPEED: f32 = 15.0;

/// tg `move_intent == MOVE_INTENT_WALK`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Walking(pub bool);

/// A cleanable decal and what washes it off (`clean_type`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Cleanable {
    pub clean_type: u32,
}

/// Blood spilled during the round (`/obj/effect/decal/cleanable/blood` and its drip).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct BloodSplat {
    pub drip: bool,
    /// Drip states not used yet on this tile (`random_icon_states`).
    pub free_drips: Vec<&'static str>,
    pub bloodiness: f32,
    pub dry_at: f64,
    pub dried: bool,
}

/// `/datum/component/wet_floor`: seconds left per `TURF_WET_*` kind.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct WetFloor {
    pub time_left: Vec<(u8, f32)>,
}

impl WetFloor {
    /// tg `_do_add_wet`: refreshes a kind to `clamp(left + add, min, max)`, a new one starts at
    /// `min(min, max)`. Nothing without a positive minimum.
    pub fn add_wet(&mut self, kind: u8, min: f32, add: f32, max: f32) -> bool {
        if min <= 0.0 || kind == 0 {
            return false;
        }
        match self.time_left.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, left)) => *left = (*left + add).clamp(min, max),
            None => self.time_left.push((kind, min.min(max))),
        }
        true
    }

    /// tg `highest_strength`.
    pub fn highest(&self) -> u8 {
        self.time_left.iter().map(|&(k, _)| k).max().unwrap_or(0)
    }

    /// One `SSwet_floors` `process()` of `secs` at `temperature` (K): what dries off.
    pub fn process(&mut self, temperature: f64, secs: f64) {
        let decrease = if temperature < T0C {
            0.0
        } else if temperature <= T0C + 100.0 {
            // Deciseconds per tg tick, turned into seconds.
            ((temperature - T0C) / WET_TEMPERATURE_COEFF * (secs / WET_TIME_RATIO) / 10.0) as f32
        } else {
            f32::INFINITY
        };
        for (_, left) in &mut self.time_left {
            *left = (*left - decrease.max(0.0)).max(0.0);
        }
        self.time_left.retain(|&(_, left)| left > 0.0);
    }

    /// `update_flags`: the slip's knockdown (seconds) and whether walking avoids it.
    pub fn slip(&self) -> Option<(f32, bool)> {
        match self.highest() {
            0 => None,
            TURF_WET_WATER => Some((6.0, true)),
            TURF_WET_LUBE => Some((8.0, false)),
            _ => Some((12.0, false)),
        }
    }
}

/// `/obj/item/soap` `uses`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Soap {
    pub uses: u32,
}

/// What a finished cleaning do_after does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanJob {
    Mop {
        wash: bool,
    },
    Soap,
    /// The janitor's key finding the right key for a door (`attempt_open_door`).
    AccessKey,
}

#[derive(Resource, Default)]
struct JanitorClocks {
    wet: f64,
    dry: f64,
}

pub struct JanitorServerPlugin;

impl Plugin for JanitorServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg cleaner component of mops and soap, mop buckets, spray bottles.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3500, |w, c| {
            c.held.is_some_and(|h| {
                crate::janitor::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        // The push broom is braced (wielded) in hand.
        app.add_click_handler(ClickPhase::AttackSelf, 3190, |w, c| {
            crate::janitor::broom_attack_self(w, c.client, c.mob, c.target)
        });
        // tg washing machine `attack_hand` (door), `attack_hand_secondary` (start).
        app.add_click_handler(ClickPhase::EmptyHand, 3468, |w, c| {
            crate::janitor::washer_attack_hand(w, c.client, c.target, c.modifiers.right)
        });
        // tg washing machine `item_interaction`: a load goes in.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3510, |w, c| {
            c.held.is_some_and(|h| {
                crate::janitor::washer_item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // tg `door/item_interaction` with the janitor's key.
        app.add_click_handler(ClickPhase::HeldOnTarget, 320, |w, c| {
            c.held
                .is_some_and(|h| crate::janitor::key_on_door(w, c.client, c.mob, h, c.target))
        });
        app.add_observer(add_washer)
            .add_observer(add_access_key)
            .add_systems(
                Update,
                (
                    finish_washing.run_if(any_with_component::<WashingMachine>),
                    expire_key_access.run_if(any_with_component::<AccessKey>),
                ),
            );
        app.init_resource::<JanitorClocks>()
            .add_observer(add_cleanable)
            .add_observer(add_item_parts)
            .add_observer(add_bucket_reagents)
            .add_systems(
                Update,
                (
                    toggle_move_intent.before(HealthSystems),
                    sync_walk_barriers
                        .after(toggle_move_intent)
                        .before(WorldSystems::Movement),
                    (
                        condensation,
                        slip_on_entered,
                        sweep_on_moved,
                        wet_floors,
                        dry_blood,
                        bucket_overlays,
                    )
                        .chain()
                        .after(ss13_atmos::AtmosSystems::Fire)
                        .after(WorldSystems::Movement)
                        .before(crate::status_effects::StatusSystems),
                ),
            );
    }
}

fn add_cleanable(add: On<Add<DmCleanable>>, decals: Query<&DmCleanable>, mut commands: Commands) {
    let Ok(dm) = decals.get(add.entity) else {
        return;
    };
    let clean_type = if is_under(&dm.path, BLOOD) {
        CLEAN_TYPE_BLOOD
    } else if is_under(&dm.path, "/obj/effect/decal/cleanable/crayon") {
        CLEAN_TYPE_HARD_DECAL
    } else if is_under(&dm.path, crate::hazard_decals::FUEL_POOL) {
        CLEAN_TYPE_BLOOD
    } else {
        CLEAN_TYPE_LIGHT_DECAL
    };
    commands.entity(add.entity).insert(Cleanable { clean_type });
}

/// A mop's `create_reagents(max_reagent_volume)`, a soap's `uses`.
fn add_item_parts(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if is_under(&item.path, MOP) {
        let volume = if is_under(&item.path, MOP_ADVANCED) {
            10.0
        } else {
            15.0
        };
        commands.entity(add.entity).insert(Reagents(Holder::new(
            volume,
            0,
            Owner {
                path: item.path.clone(),
                living: false,
            },
        )));
    } else if is_under(&item.path, SOAP) {
        let uses = match item.path.as_str() {
            "/obj/item/soap/nanotrasen" | "/obj/item/soap/nanotrasen/cyborg" => 300,
            "/obj/item/soap/omega" => 800,
            "/obj/item/soap/drone" => u32::MAX,
            _ => 100,
        };
        commands.entity(add.entity).insert(Soap { uses });
    }
}

/// `/obj/structure/mop_bucket/Initialize`: `create_reagents(100, OPENCONTAINER)`.
fn add_bucket_reagents(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    if is_under(&obj.path, MOP_BUCKET) {
        commands.entity(add.entity).insert((
            Reagents(Holder::new(
                100.0,
                flags::OPENCONTAINER,
                Owner {
                    path: obj.path.clone(),
                    living: false,
                },
            )),
            TransferAmount(5.0),
        ));
    }
}

/// `/mob/living/proc/toggle_move_intent`.
fn toggle_move_intent(
    mut intents: MessageReader<IntentReceived>,
    players: Res<Players>,
    mut mobs: Query<(&PlayerMob, Option<&mut Walking>), With<Health>>,
    mut commands: Commands,
) {
    for intent in intents.read() {
        if !matches!(intent.intent, ClientIntent::ToggleMoveIntent) {
            continue;
        }
        let Some(mob) = players.mob(intent.client) else {
            continue;
        };
        match mobs.get_mut(mob) {
            Ok((player, Some(mut walking))) if player.client == intent.client => {
                walking.0 = !walking.0;
            }
            Ok((player, None)) if player.client == intent.client => {
                commands.entity(mob).insert(Walking(true));
            }
            _ => {}
        }
    }
}

/// Carbons whose walking or consciousness may have changed.
type WalkersChanged = Or<(
    Changed<Walking>,
    Changed<Stat>,
    Changed<crate::status_effects::StatusEffects>,
)>;

/// Holobarriers' `CanAllowThrough`: a carbon passes them walking or `IS_UNCONSCIOUS_OR_CRIT`
/// (`ss13_world::PassesWalkBarriers`).
// Cost: change detection only; the status-effect countdown marks a stunned mob changed each tick.
fn sync_walk_barriers(
    changed: Query<
        Entity,
        (
            With<Health>,
            With<crate::inventory::Inventory>,
            Without<crate::silicon::Silicon>,
            Without<crate::mobs::BasicMob>,
            WalkersChanged,
        ),
    >,
    mut cleared: RemovedComponents<crate::status_effects::StatusEffects>,
    mobs: Query<(
        Option<&Walking>,
        Option<&Stat>,
        Option<&crate::status_effects::StatusEffects>,
        Has<ss13_world::PassesWalkBarriers>,
    )>,
    carbons: Query<
        (),
        (
            With<Health>,
            With<crate::inventory::Inventory>,
            Without<crate::silicon::Silicon>,
            Without<crate::mobs::BasicMob>,
        ),
    >,
    mut commands: Commands,
) {
    let cleared: Vec<Entity> = cleared.read().filter(|&e| carbons.contains(e)).collect();
    for mob in changed.iter().chain(cleared) {
        let Ok((walking, stat, effects, has)) = mobs.get(mob) else {
            continue;
        };
        let passes = walking.is_some_and(|w| w.0)
            || stat.is_some_and(|s| *s != Stat::Conscious)
            || effects.is_some_and(|e| e.knocked_out());
        if passes && !has {
            commands.entity(mob).insert(ss13_world::PassesWalkBarriers);
        } else if !passes && has {
            commands
                .entity(mob)
                .remove::<ss13_world::PassesWalkBarriers>();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Washing.

/// tg `wash(clean_types)` of one atom: its forensic traces are wiped
/// (`COMSIG_COMPONENT_CLEAN_ACT`), a cleanable it applies to is deleted. True when something was cleaned.
pub fn wash(world: &mut World, target: Entity, clean_types: u32) -> bool {
    if world.get::<Turf>(target).is_some()
        && let Some(&pos) = world.get::<TilePos>(target)
    {
        return wash_turf(world, pos, clean_types, false);
    }
    let wiped = crate::forensics::server::clean_act(world, target, clean_types)
        | crate::radiation::clean_act(world, target, clean_types)
        // `/mob/living/carbon/human/wash`: an uncovered mouth loses its lipstick.
        | crate::service::barber::clean_lips(world, target)
        | crate::stickers::clean_act(world, target);
    match world.get::<Cleanable>(target) {
        Some(c)
            if c.clean_type & clean_types != 0
                && !crate::syndie_spraycan::clean_proof(world, target) =>
        {
            world.despawn(target);
            true
        }
        _ => wiped,
    }
}

/// `/turf/wash(clean_types, all_contents)`: the cleanables on it (`TRAIT_MOPABLE`), or
/// everything with `all_contents`.
pub fn wash_turf(world: &mut World, pos: TilePos, clean_types: u32, all_contents: bool) -> bool {
    let on_tile: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<Turf>(e).is_none())
        .filter(|&e| all_contents || world.get::<Cleanable>(e).is_some())
        .collect();
    // The turf's own stickers (`COMSIG_COMPONENT_CLEAN_ACT`).
    let mut cleaned = turf_at(world, pos).is_some_and(|t| crate::stickers::clean_act(world, t));
    for e in on_tile {
        cleaned |= wash(world, e, clean_types);
    }
    cleaned
}

/// `reagents.expose(target, TOUCH, volume_modifier)`.
pub fn expose(world: &mut World, target: Entity, holder: &Holder, modifier: f32) {
    expose_by(
        world,
        target,
        holder,
        modifier,
        TOUCH,
        WATER_COOLING_TEMPERATURE,
    );
}

/// `reagents.expose(target, VAPOR, volume_modifier)`.
pub fn expose_vapor(world: &mut World, target: Entity, holder: &Holder, modifier: f32) {
    expose_by(
        world,
        target,
        holder,
        modifier,
        VAPOR,
        WATER_COOLING_TEMPERATURE,
    );
}

/// `reagents.expose(target, methods)` of a transfer into a mob (`trans_to(methods)`: INGEST,
/// INJECT, PATCH).
pub fn expose_methods(world: &mut World, target: Entity, holder: &Holder, methods: u32) {
    expose_by(
        world,
        target,
        holder,
        1.0,
        methods,
        WATER_COOLING_TEMPERATURE,
    );
}

/// [`expose`] with water's `cooling_temperature` (extinguishers set their own).
pub fn expose_cooled(world: &mut World, target: Entity, holder: &Holder, modifier: f32, cool: f32) {
    expose_by(world, target, holder, modifier, TOUCH, cool);
}

/// `reagents.expose(target, methods, volume_modifier)` for the reagents whose `expose_*` is
/// ported here (water, firefighting foam, space cleaner, welding fuel, uranium, capsaicin,
/// romerol, acids swallowed or injected, the pyrotechnics in `crate::pyrotechnics`), and
/// `penetrates_skin` into the blood of a mob.
// shortcut: no `acid_act` (acid on skin, objects and turfs melts nothing).
pub fn expose_by(
    world: &mut World,
    target: Entity,
    holder: &Holder,
    modifier: f32,
    methods: u32,
    cool: f32,
) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let turf = world
        .get::<Turf>(target)
        .and_then(|_| world.get::<TilePos>(target).copied());
    let living =
        world.get::<Health>(target).is_some() && world.get::<crate::ghost::Ghost>(target).is_none();
    // `getarmor(null, BIO)`: the torso's (no zone-less armour here).
    let touch_protection = if living && methods & (VAPOR | TOUCH) != 0 {
        crate::armor::getarmor(world, target, crate::zone::BodyZone::Chest, "bio") * 0.01
    } else {
        0.0
    };
    for r in &holder.reagents {
        let volume = r.volume * modifier;
        if chem.is_type(r.id, WATER) {
            // `/datum/reagent/water/expose_obj`: a monkey cube expands.
            if crate::food::expand_cube(world, target) {
                return;
            }
            if let Some(pos) = turf {
                // `/datum/reagent/water/expose_turf`.
                douse_hotspot(world, pos, |t| {
                    let cool = f64::from(cool);
                    (t - cool * 1000.0).min(t / cool).max(T0C).min(t)
                });
                if volume >= 5.0 {
                    make_slippery(world, pos, TURF_WET_WATER, 10.0, (volume * 1.5).min(60.0));
                }
            } else if living {
                water_expose_mob(world, target, volume, methods);
            } else {
                // `/datum/reagent/water/expose_obj` -> `extinguish()` (fireplaces, bonfires);
                // hairless hide soaks into wet hide.
                if !crate::sheets::soak(world, target) {
                    crate::fireplace::extinguish(world, target);
                    crate::fire::extinguish_obj(world, target);
                }
            }
        } else if chem.is_type(r.id, crate::antag::zombie::ROMEROL) {
            // `can_infect` by touch: at least 1u.
            if living && volume >= 1.0 {
                crate::antag::zombie::infect(
                    world,
                    target,
                    crate::antag::zombie::INFECTION_NODAMAGE,
                );
            }
        } else if chem.is_type(r.id, FIREFIGHTING_FOAM) {
            // `/datum/reagent/firefighting_foam/expose_*`.
            if let Some(pos) = turf {
                douse_hotspot(
                    world,
                    pos,
                    |t| if t > T20C { (t / 2.0).max(T20C) } else { t },
                );
            } else if living {
                if methods & (TOUCH | VAPOR) != 0 {
                    crate::fire::extinguish_mob(world, target);
                }
            } else {
                crate::fireplace::extinguish(world, target);
                crate::fire::extinguish_obj(world, target);
            }
        } else if chem.is_type(r.id, SPACE_CLEANER) {
            match turf {
                // `/datum/reagent/space_cleaner/expose_turf`.
                Some(pos) if volume >= 1.0 => {
                    wash_turf(world, pos, CLEAN_WASH, true);
                }
                Some(_) => {}
                // `expose_obj`; mobs carry nothing to wash here.
                None if !living => {
                    wash(world, target, CLEAN_WASH);
                }
                None => {}
            }
        } else if chem.is_type(r.id, FUEL) {
            // `/datum/reagent/fuel/expose_turf`, `expose_mob`.
            if let Some(pos) = turf {
                if volume >= 5.0 && !crate::construction::is_space(world, pos) {
                    crate::hazard_decals::spawn_pool(world, pos, volume);
                }
            } else if living && methods & (VAPOR | TOUCH) != 0 {
                crate::fire::adjust_fire_stacks(world, target, volume / 10.0);
            }
        } else if chem.is_type(r.id, URANIUM) {
            // `/datum/reagent/uranium/expose_turf`.
            if let Some(pos) = turf
                && volume >= 3.0
                && !crate::construction::is_space(world, pos)
            {
                crate::hazard_decals::spawn_glow(world, pos);
            }
        } else if chem.is_type(r.id, CAPSAICIN) && living && methods & (VAPOR | TOUCH | INHALE) != 0
        {
            pepper_expose(world, target);
        } else if chem.is_type(r.id, ACID) && living {
            // `/datum/reagent/toxin/acid/expose_mob` (carbons): swallowed or breathed burns
            // `min(6 * toxpwr, volume * toxpwr)` brute, injected 1.5 times that.
            let toxpwr = chem.def(r.id).toxpwr;
            let burn = (6.0 * toxpwr).min(round_to(volume, 0.1) * toxpwr);
            let carbon = world.get::<crate::chem::MobReagents>(target).is_some();
            if carbon && methods & (INGEST | INHALE) != 0 {
                crate::mobs::defense::hurt(world, target, DamageType::Brute, burn, None);
            } else if carbon && methods & INJECT != 0 {
                crate::mobs::defense::hurt(world, target, DamageType::Brute, 1.5 * burn, None);
            }
        } else {
            if living {
                crate::medical::diseases::reagent_expose(
                    world,
                    target,
                    &chem.def(r.id).path,
                    volume,
                    methods,
                    touch_protection,
                );
            }
            crate::pyrotechnics::expose(
                world,
                &chem,
                target,
                r.id,
                volume,
                methods,
                touch_protection,
                turf,
                living,
            );
        }
        // `/datum/reagent/expose_mob`: `penetrates_skin & methods`.
        if living && chem.def(r.id).penetrates_skin & methods != 0 {
            let amount = (volume * (1.0 - touch_protection).clamp(0.0, 1.0) * 10.0).round() / 10.0;
            if amount >= 0.5
                && let Some(mut m) = world.get_mut::<crate::chem::MobReagents>(target)
            {
                m.blood.add_reagent(
                    &chem,
                    r.id,
                    amount,
                    holder.chem_temp,
                    None,
                    None,
                    false,
                    false,
                );
            }
        }
    }
}

/// `/datum/reagent/consumable/condensedcapsaicin/expose_mob` on an unprotected human.
// shortcut: no 10 s movement slowdown or `TRAIT_ANOSMIA`, no `pepperspray_protect` eyes (only
// `is_pepper_proof` clothing); add with the movespeed modifier registry. The blindness replaces
// a longer one (`set_temp_blindness_if_lower` has no getter here).
fn pepper_expose(world: &mut World, mob: Entity) {
    if world.get::<crate::human::HumanLook>(mob).is_none()
        || crate::clothing::cover::pepper_proof(world, mob).is_some()
    {
        return;
    }
    if world.resource_mut::<CombatRng>().chance(5.0)
        && let Some(emote) = ss13_chat::emote::find("scream")
    {
        crate::mobs::defense::emote(world, mob, emote.message);
    }
    if let Some(emote) = ss13_chat::emote::find("cry") {
        crate::mobs::defense::emote(world, mob, emote.message);
    }
    crate::vision::set_eye_blur_if_lower(world, mob, 10.0);
    crate::vision::cure_temp_blindness(world, mob);
    crate::vision::adjust_temp_blindness(world, mob, 6.0);
    crate::status_effects::set_timed(
        world,
        mob,
        crate::status_effects::Timed::Confusion,
        5.0,
        true,
    );
    crate::status_effects::knockdown(world, mob, 3.0, 0.0);
}

/// The fire on `pos` goes out and its air cools (not over space: `isspaceturf`).
fn douse_hotspot(world: &mut World, pos: TilePos, cooled: impl FnOnce(f64) -> f64) {
    if crate::construction::is_space(world, pos) {
        return;
    }
    let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() else {
        return;
    };
    if atmos.hotspot(pos).is_none() {
        return;
    }
    atmos.modify_air(pos, |air| air.temperature = cooled(air.temperature));
    atmos.extinguish_hotspot(pos);
}

/// `WATER_TO_WET_STACKS_FACTOR_TOUCH`, `WATER_TO_WET_STACKS_FACTOR_VAPOR`.
const WATER_TO_WET_STACKS_FACTOR_TOUCH: f32 = 0.5;
const WATER_TO_WET_STACKS_FACTOR_VAPOR: f32 = 0.1;

/// `/datum/reagent/water/expose_mob`: by touch the fire goes out and the mob gets wet, by vapor it
/// gets a little wet; either way it wakes up a little.
// shortcut: no felinid water hate.
fn water_expose_mob(world: &mut World, mob: Entity, volume: f32, methods: u32) {
    use crate::status_effects::{self, Incap, Timed};
    if methods & TOUCH != 0 {
        crate::fire::extinguish_mob(world, mob);
        crate::fire::adjust_wet_stacks(world, mob, volume * WATER_TO_WET_STACKS_FACTOR_TOUCH);
    }
    if methods & VAPOR != 0 {
        crate::fire::adjust_wet_stacks(world, mob, volume * WATER_TO_WET_STACKS_FACTOR_VAPOR);
    }
    if methods & (TOUCH | VAPOR) == 0 {
        return;
    }
    status_effects::adjust_timed(world, mob, Timed::Dizziness, -2.0, None);
    status_effects::adjust_timed(world, mob, Timed::Confusion, -2.0, None);
    status_effects::adjust_timed(world, mob, Timed::Drowsiness, -4.0, None);
    status_effects::adjust_timed(world, mob, Timed::Jitter, -4.0, None);
    status_effects::adjust(world, mob, Incap::Sleeping, -15.0);
    status_effects::adjust(world, mob, Incap::Unconscious, -8.0);
    // `adjust_drunk_effect(-0.25)`.
    if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) {
        reagents.drunk = (reagents.drunk - 0.25).max(0.0);
    }
}

/// `isnoslipturf` (`GLOB.turfs_no_slip_water`): vapor does not wet them.
const NO_SLIP_WATER_TURFS: [&str; 8] = [
    "/turf/open/misc/asteroid",
    "/turf/open/misc/dirt",
    "/turf/open/misc/grass",
    "/turf/open/misc/basalt",
    "/turf/open/misc/ashplanet",
    "/turf/open/misc/snow",
    "/turf/open/misc/sandy_dirt",
    "/turf/open/floor/noslip",
];

/// Water vapor condensing on turfs (tg `/datum/gas_reaction/standard/water_vapor`):
/// `water_vapor_gas_act` wets (10 s, +5 s) and washes a grounded, slippable turf;
/// `freeze_turf` makes it permafrost for 10 s and freezes its objects
/// (`crate::effects::frozen`).
// shortcut: slimes are not watered; add with them.
fn condensation(world: &mut World) {
    let Some(tiles) = world
        .get_resource_mut::<ss13_atmos::Atmos>()
        .map(|mut a| a.drain_condensed())
    else {
        return;
    };
    for (pos, kind) in tiles {
        match kind {
            ss13_atmos::reactions::Condensation::Freeze => {
                crate::effects::frozen::freeze_turf(world, pos);
                crate::freon::freeze_turf_mobs(world, pos);
                make_slippery(world, pos, TURF_WET_PERMAFROST, 10.0, 0.0);
            }
            ss13_atmos::reactions::Condensation::Wet => {
                let Some(turf) = turf_at(world, pos) else {
                    continue;
                };
                let groundless = world.get::<DmTurf>(turf).is_some_and(|t| t.groundless);
                let no_slip = crate::construction::path_of(world, turf)
                    .is_some_and(|p| NO_SLIP_WATER_TURFS.iter().any(|r| is_under(&p, r)));
                if !groundless && !no_slip {
                    make_slippery(world, pos, TURF_WET_WATER, 10.0, 5.0);
                    wash_turf(world, pos, CLEAN_WASH, true);
                }
            }
        }
    }
}

/// tg `/turf/open/MakeSlippery(wet, min, add, MAXIMUM_WET_TIME)` on an open, grounded turf.
pub fn make_slippery(world: &mut World, pos: TilePos, kind: u8, min: f32, add: f32) {
    let Some(turf) = turf_at(world, pos) else {
        return;
    };
    let entity = world.entity(turf);
    if entity.contains::<Density>()
        || entity
            .get::<DmTurf>()
            .is_some_and(|t| t.closed || t.groundless)
    {
        return;
    }
    let mut wet = entity.get::<WetFloor>().cloned().unwrap_or_default();
    if wet.add_wet(kind, min, add, MAXIMUM_WET_TIME) {
        world.entity_mut(turf).insert(wet);
        refresh_wet_overlay(world, turf);
    }
}

/// `update_overlay`: `wet_floor_static` while wet.
fn refresh_wet_overlay(world: &mut World, turf: Entity) {
    let Some(state) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(WATER_DMI, "wet_floor_static"))
    else {
        return;
    };
    let wet = world
        .get::<WetFloor>(turf)
        .is_some_and(|w| w.highest() != 0);
    if let Some(mut appearance) = world.get_mut::<Appearance>(turf) {
        let has = appearance.overlays.iter().any(|o| o.icon_state == state);
        if wet && !has {
            appearance.overlays.push(Overlay::new(state));
        } else if !wet && has {
            appearance.overlays.retain(|o| o.icon_state != state);
        }
    }
}

/// `SSwet_floors`: every second each wet turf dries by its air's temperature.
fn wet_floors(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let last = world.resource::<JanitorClocks>().wet;
    if now - last < WET_FLOORS_WAIT {
        return;
    }
    world.resource_mut::<JanitorClocks>().wet = now;
    let secs = if last == 0.0 {
        WET_FLOORS_WAIT
    } else {
        now - last
    };
    // Cost: one pass over the wet turfs (a few dozen after a janitor's round) per second.
    let mut query = world.query::<(Entity, &TilePos, &WetFloor)>();
    let wet: Vec<(Entity, TilePos, WetFloor)> = query
        .iter(world)
        .map(|(e, p, w)| (e, *p, w.clone()))
        .collect();
    for (turf, pos, mut floor) in wet {
        let temperature = world
            .get_resource::<ss13_atmos::Atmos>()
            .and_then(|a| a.temperature(pos))
            .unwrap_or(T0C + 20.0);
        floor.process(temperature, secs);
        if floor.time_left.is_empty() {
            world.entity_mut(turf).remove::<WetFloor>();
        } else {
            world.entity_mut(turf).insert(floor);
        }
        refresh_wet_overlay(world, turf);
    }
}

/// The slippery component of a wet turf (`Slip` on `COMSIG_ATOM_ENTERED`) -> `handle_slip`.
fn slip_on_entered(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    // Cost: one tile lookup per mob step (a few per player per second).
    for m in moved {
        if world.get::<Health>(m.entity).is_none() {
            continue;
        }
        let Some((slip, wet)) = turf_at(world, m.to)
            .and_then(|t| world.get::<WetFloor>(t))
            .and_then(|w| Some((w.slip()?, w.highest())))
        else {
            continue;
        };
        // Active MOD magboots (`TRAIT_NO_SLIP_WATER`, `TRAIT_NO_SLIP_ICE`), the MOD anti-slip
        // module, worn `clothing_traits` (`crate::clothing::traits`).
        if crate::modsuit::modules::resists_slip(world, m.entity, slip.1, slip.0)
            || crate::modsuit::modules::resists_wet_slip(world, m.entity, wet)
            || crate::clothing::traits::resists_wet_slip(world, m.entity, wet)
        {
            continue;
        }
        try_slip(world, m.entity, slip);
    }
}

/// `/turf/open/handle_slip` of a standing mob; true when it slipped.
pub fn try_slip(world: &mut World, mob: Entity, (knockdown, walking_safe): (f32, bool)) -> bool {
    if world.get::<Stat>(mob) == Some(&Stat::Dead)
        || world.get::<BodyPosition>(mob).is_some_and(|b| b.lying())
        || (walking_safe && world.get::<Walking>(mob).is_some_and(|w| w.0))
    {
        return false;
    }
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, Span::Notice, "Вы поскользнулись!");
    }
    // BandaStation `paralyze_time = 0.5 SECONDS`, tg `daze = 3 SECONDS`.
    crate::status_effects::apply(world, mob, Incap::Paralyze, 0.5);
    crate::status_effects::knockdown(world, mob, knockdown, 3.0);
    true
}

// ---------------------------------------------------------------------------------------------
// Blood.

/// The end of tg `bleed(amount)`: on a turf, `prob(sqrt(amount) × 90)` splatters.
pub fn bleed_splatter(world: &mut World, mob: Entity, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    // "Blood loss still happens in locker, floor stays clean."
    let in_something = world.get::<crate::closets::InCloset>(mob).is_some()
        || world.get::<crate::mecha::Pilot>(mob).is_some();
    let Some(&pos) = world.get::<TilePos>(mob).filter(|_| !in_something) else {
        return;
    };
    if !world
        .resource_mut::<CombatRng>()
        .chance(amount.sqrt() * BLOOD_DRIP_RATE_MOD)
    {
        return;
    }
    if let Some(splat) = add_splatter_floor(world, pos, amount <= 10.0) {
        crate::forensics::server::bloody_splatter(world, mob, splat);
    }
}

/// tg `/datum/blood_type/make_blood_splatter` (red human blood) on `pos`.
pub fn add_splatter_floor(world: &mut World, pos: TilePos, drip: bool) -> Option<Entity> {
    let turf = turf_at(world, pos)?;
    let entity = world.entity(turf);
    if entity.contains::<Density>()
        || entity
            .get::<DmTurf>()
            .is_some_and(|t| t.closed || t.groundless)
    {
        return None;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let splats: Vec<(Entity, BloodSplat)> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter_map(|&e| world.get::<BloodSplat>(e).map(|b| (e, b.clone())))
        .collect();
    let pool = splats
        .iter()
        .find(|(_, b)| !b.drip && !b.dried)
        .map(|(e, _)| *e);
    if drip {
        match splats.iter().find(|(_, b)| b.drip) {
            None => {
                if let Some(pool) = pool {
                    return Some(pool);
                }
                let state = pick(world, &DRIP_STATES);
                let drop = spawn_blood(world, BLOOD_DRIP, pos, state)?;
                world.entity_mut(drop).insert(BloodSplat {
                    drip: true,
                    free_drips: DRIP_STATES.into_iter().filter(|&s| s != state).collect(),
                    bloodiness: 0.0,
                    dry_at: now + BLOOD_DRYING_SECS,
                    dried: false,
                });
                return Some(drop);
            }
            Some((drop, b)) if !b.free_drips.is_empty() => {
                // Another drip on the same decal (tg: a `vis_contents` drip).
                let drop = *drop;
                let mut b = b.clone();
                let i = world
                    .resource_mut::<CombatRng>()
                    .range(0, b.free_drips.len() as u32 - 1) as usize;
                let state = b.free_drips.swap_remove(i);
                world.entity_mut(drop).insert(b);
                if let Some(id) = world
                    .get_resource::<IconIndex>()
                    .and_then(|ix| ix.get(BLOOD_DMI, state))
                    && let Some(mut appearance) = world.get_mut::<Appearance>(drop)
                {
                    appearance.overlays.push(Overlay::new(id));
                }
                return Some(drop);
            }
            // Five drips: replaced by a pool.
            Some((drop, _)) => world.despawn(*drop),
        };
    }
    if let Some(pool) = pool {
        if let Some(mut b) = world.get_mut::<BloodSplat>(pool) {
            b.bloodiness += BLOOD_AMOUNT_PER_DECAL;
        }
        return Some(pool);
    }
    let state = pick(world, &BLOOD_STATES);
    let splat = spawn_blood(world, BLOOD, pos, state)?;
    world.entity_mut(splat).insert(BloodSplat {
        drip: false,
        free_drips: Vec::new(),
        bloodiness: BLOOD_AMOUNT_PER_DECAL,
        dry_at: now + BLOOD_DRYING_SECS,
        dried: false,
    });
    Some(splat)
}

fn pick(world: &mut World, states: &[&'static str]) -> &'static str {
    let i = world
        .resource_mut::<CombatRng>()
        .range(0, states.len() as u32 - 1);
    states[i as usize]
}

/// `new path(turf)` with one of its `random_icon_states`.
fn spawn_blood(world: &mut World, path: &str, pos: TilePos, state: &str) -> Option<Entity> {
    let e = crate::construction::spawn_obj(world, path, pos)?;
    if let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|ix| ix.get(BLOOD_DMI, state))
        && let Some(mut appearance) = world.get_mut::<Appearance>(e)
    {
        appearance.icon_state = id;
    }
    Some(e)
}

/// `SSblood_drying`: blood dries after its `drying_time`.
fn dry_blood(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now - world.resource::<JanitorClocks>().dry < 1.0 {
        return;
    }
    world.resource_mut::<JanitorClocks>().dry = now;
    let mut query = world.query::<(Entity, &BloodSplat)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, b)| !b.dried && now >= b.dry_at)
        .map(|(e, _)| e)
        .collect();
    for e in due {
        if let Some(mut b) = world.get_mut::<BloodSplat>(e) {
            b.dried = true;
        }
        if let Some(mut appearance) = world.get_mut::<Appearance>(e) {
            appearance.color = Some(DRIED_BLOOD_COLOR);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tools.

fn item_path(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn obj_path(world: &World, e: Entity) -> String {
    world
        .get::<DmObj>(e)
        .map(|o| o.path.clone())
        .unwrap_or_default()
}

/// `water_icon` of a mop bucket or cart with reagents.
fn bucket_overlays(
    mut buckets: Query<(&DmObj, &Reagents, &mut Appearance), Changed<Reagents>>,
    icons: Option<Res<IconIndex>>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (obj, reagents, mut appearance) in &mut buckets {
        let state = if is_under(&obj.path, JANICART) {
            "cart_water"
        } else {
            "mopbucket_water"
        };
        let Some(id) = icons.get("icons/obj/service/janitor.dmi", state) else {
            continue;
        };
        let wanted = reagents.0.total_volume > 0.0;
        let has = appearance.overlays.iter().any(|o| o.icon_state == id);
        if wanted && !has {
            appearance.overlays.push(Overlay::new(id));
        } else if !wanted && has {
            appearance.overlays.retain(|o| o.icon_state != id);
        }
    }
}

/// tg `item_interaction(_secondary)` of a mop, soap or spray bottle in hand. True when the
/// click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let path = item_path(world, held);
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    // `SHOULD_SKIP_INTERACTION`: living mobs are attacked, not cleaned.
    let living = world.get::<Health>(target).is_some();
    if is_under(&path, MOP) {
        if is_under(&obj_path(world, target), MOP_BUCKET) {
            if modifiers.right {
                wet_mop(world, client, held, target);
                return true;
            }
            return false; // `clean_blacklist`
        }
        if living {
            return false;
        }
        return start_mop(world, actor, target);
    }
    if is_under(&path, SOAP) {
        if living {
            return false;
        }
        let speed = match path.as_str() {
            "/obj/item/soap/homemade" => 30.0,
            "/obj/item/soap/nanotrasen" | "/obj/item/soap/nanotrasen/cyborg" => 28.0,
            "/obj/item/soap/deluxe" => 20.0,
            "/obj/item/soap/syndie" | "/obj/item/soap/drone" => 5.0,
            "/obj/item/soap/omega" => 3.0,
            _ => 35.0,
        };
        // `AddComponent(/datum/component/cleaner, cleanspeed, 0.1)`: less scaling for soap.
        let speed = speed * (cleaning_speed(world, mob) + 0.1).min(1.0);
        do_after(world, actor, target, speed, 0, Job::Clean(CleanJob::Soap));
        return true;
    }
    if is_under(&path, PUSHBROOM) {
        // `interact_with_atom`: sweeps the clicked tile the way the user faces.
        if living {
            return false;
        }
        let (Some(&pos), Some(dir)) = (
            world.get::<TilePos>(target),
            world.get::<ss13_shared::Facing>(mob).map(|f| f.0),
        ) else {
            return false;
        };
        do_sweep(world, pos, dir);
        return true;
    }
    if is_under(&path, SPRAY) && !modifiers.right {
        return crate::sprays::interact(world, actor, target);
    }
    false
}

/// `/obj/structure/mop_bucket/item_interaction_secondary` with a mop.
fn wet_mop(world: &mut World, client: Entity, mop: Entity, bucket: Entity) {
    let (Some(mop_r), Some(bucket_r)) = (
        world.get::<Reagents>(mop).map(|r| r.0.clone()),
        world.get::<Reagents>(bucket).map(|r| r.0.clone()),
    ) else {
        return;
    };
    if mop_r.total_volume >= mop_r.maximum_volume {
        tell(world, client, Span::Warning, "Швабра уже пропитана!");
        return;
    }
    // `CART_HAS_MINIMUM_REAGENT_VOLUME`.
    if bucket_r.total_volume < 1.0 {
        tell(world, client, Span::Warning, "Ведро пусто!");
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let (mut from, mut to) = (bucket_r, mop_r);
    let max = to.maximum_volume;
    from.trans_to(&chem, &mut to, max, 1.0, None, false, false);
    world.entity_mut(bucket).insert(Reagents(from));
    world.entity_mut(mop).insert(Reagents(to));
    tell(world, client, Span::Notice, "Вы смачиваете швабру.");
}

/// `/obj/item/mop/should_clean`, then the cleaner's `do_after(mopspeed)`.
fn start_mop(world: &mut World, actor: Actor, target: Entity) -> bool {
    let mop = actor.item.expect("a held mop");
    let Some(holder) = world.get::<Reagents>(mop).map(|r| r.0.clone()) else {
        return false;
    };
    if holder.total_volume < 0.1 {
        tell(world, actor.client, Span::Warning, "Швабра сухая!");
        return true;
    }
    let wash = world.get_resource::<Chem>().is_some_and(|chem| {
        holder
            .reagents
            .iter()
            .any(|r| r.volume >= 1.0 && chem.0.def(r.id).chemical_flags & REAGENT_CLEANS != 0)
    });
    let speed = MOPSPEED * cleaning_speed(world, actor.mob).min(1.0);
    do_after(
        world,
        actor,
        target,
        speed,
        0,
        Job::Clean(CleanJob::Mop { wash }),
    );
    true
}

/// `get_skill_modifier(/datum/skill/cleaning, SKILL_SPEED_MODIFIER)`.
fn cleaning_speed(world: &World, mob: Entity) -> f32 {
    crate::skills::modifier(
        world,
        mob,
        crate::skills::Skill::Cleaning,
        crate::skills::Modifier::Speed,
    )
}

/// The cleanables `target` holds (`for(var/obj/effect/decal/cleanable/c in target)`): those on
/// a turf's tile.
fn decals_in(world: &World, target: Entity) -> Vec<String> {
    if world.get::<Turf>(target).is_none() {
        return Vec::new();
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return Vec::new();
    };
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&e| world.get::<Cleanable>(e).is_some())
        .filter_map(|&e| world.get::<DmCleanable>(e).map(|d| d.path.clone()))
        .collect()
}

/// The cleaner's `clean()` experience (`grant_xp` with `call_wash`).
fn clean_and_learn(world: &mut World, mob: Entity, target: Entity) {
    let decals = decals_in(world, target);
    let washed = wash(world, target, CLEAN_SCRUB);
    let xp = crate::skills::cleaning_experience(&decals, washed);
    crate::skills::adjust_experience(world, mob, crate::skills::Skill::Cleaning, xp, false);
}

/// The cleaner's `clean()` after its do_after, then the mop's `apply_reagents` or the soap's
/// `decreaseUses`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CleanJob) {
    let target = action.target;
    let Some(item) = action.item.filter(|&i| world.get_entity(i).is_ok()) else {
        return;
    };
    match job {
        CleanJob::AccessKey => key_opens_door(world, action.client, mob, item, target),
        CleanJob::Mop { wash: do_wash } => {
            if do_wash {
                clean_and_learn(world, mob, target);
            }
            let Some(holder) = world.get::<Reagents>(item).map(|r| r.0.clone()) else {
                return;
            };
            if world.get_entity(target).is_ok() {
                // `reagents.expose(cleaned_turf, TOUCH, 10)`: "Needed for proper floor wetting."
                expose(world, target, &holder, 10.0);
            }
            // `val2remove = round(cleaning speed modifier, 0.1)`.
            let used = (cleaning_speed(world, mob) * 10.0).round() / 10.0;
            if let Some(mut r) = world.get_mut::<Reagents>(item) {
                r.0.remove_all(used);
            }
        }
        CleanJob::Soap => {
            clean_and_learn(world, mob, target);
            let Some(mut soap) = world.get::<Soap>(item).copied() else {
                return;
            };
            // `decreaseUses`: `prob(skillcheck * 100)`.
            let skillcheck = cleaning_speed(world, mob);
            if skillcheck >= 1.0
                || world
                    .get_resource_or_init::<CombatRng>()
                    .chance(skillcheck * 100.0)
            {
                soap.uses = soap.uses.saturating_sub(1);
            }
            if soap.uses == 0 {
                let name = crate::chem::name_of(world, item);
                tell(
                    world,
                    action.client,
                    Span::Warning,
                    format!("{name} крошится на мелкие кусочки!"),
                );
                world.despawn(item);
            } else {
                world.entity_mut(item).insert(soap);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Washing machine.

const WASHING_MACHINE: &str = "/obj/machinery/washing_machine";
/// `max_wash_capacity` of the stock machine: two 50 u beakers, `floor(100 / 20)`.
const WASH_CAPACITY: usize = 5;
/// `time_to_wash = 20 SECONDS / servo.tier`.
const WASH_SECS: f64 = 20.0;

/// `/obj/machinery/washing_machine` state.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct WashingMachine {
    /// `state_open`.
    pub open: bool,
    /// `busy`: when the running cycle ends (game seconds).
    pub done_at: Option<f64>,
    /// `bloody_mess`.
    pub bloody: bool,
}

fn add_washer(
    add: On<Add<ss13_mapload::DmPower>>,
    machines: Query<&ss13_mapload::DmPower>,
    mut commands: Commands,
) {
    if machines
        .get(add.entity)
        .is_ok_and(|m| is_under(&m.path, WASHING_MACHINE))
    {
        commands.entity(add.entity).insert(WashingMachine {
            open: true,
            done_at: None,
            bloody: false,
        });
        let e = add.entity;
        commands.queue(move |world: &mut World| washer_look(world, e));
    }
}

fn washer_load(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<crate::storage::Stored>(e)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

/// `update_icon_state`: `wm_running_<blood>`, `wm_<open>_blood`, `wm_<open>_<full>`.
fn washer_look(world: &mut World, e: Entity) {
    let Some(m) = world.get::<WashingMachine>(e).copied() else {
        return;
    };
    let open = u8::from(m.open);
    let state = if m.done_at.is_some() {
        format!("wm_running_{}", u8::from(m.bloody))
    } else if m.bloody {
        format!("wm_{open}_blood")
    } else {
        format!("wm_{open}_{}", u8::from(!washer_load(world, e).is_empty()))
    };
    let Some(id) = world
        .get::<ss13_mapload::DmAtom>(e)
        .zip(world.get_resource::<IconIndex>())
        .and_then(|(atom, icons)| icons.get(&atom.icon, &state))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

/// `attack_hand` (the door; `open_machine` drops the load) and `attack_hand_secondary` (a
/// closed, clean machine starts its cycle). True when `target` is a washing machine.
// shortcut: no pets or aggressively grabbed humans stuffed in, so no gibbed pets or washed
// humans; add with mobs inside machines.
pub fn washer_attack_hand(world: &mut World, client: Entity, target: Entity, right: bool) -> bool {
    let Some(mut m) = world.get::<WashingMachine>(target).copied() else {
        return false;
    };
    let name = crate::chem::name_of(world, target);
    if m.done_at.is_some() {
        tell(world, client, Span::Warning, format!("{name} занята!"));
        return true;
    }
    if right {
        if m.open {
            tell(world, client, Span::Warning, "Сначала закройте дверцу!");
            return true;
        }
        if m.bloody {
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} нужно сначала отмыть!"),
            );
            return true;
        }
        let tier = crate::stock_parts::ratings(world, target)
            .tiers(crate::stock_parts::PartKind::Servo)
            .max()
            .unwrap_or(1)
            .max(1);
        let now = world.resource::<Time>().elapsed_secs_f64();
        m.done_at = Some(now + WASH_SECS / f64::from(tier));
    } else if m.open {
        m.open = false;
    } else {
        open_washer(world, target);
        m.open = true;
    }
    world.entity_mut(target).insert(m);
    washer_look(world, target);
    true
}

/// `open_machine`: the load drops onto the machine's tile.
fn open_washer(world: &mut World, e: Entity) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    for item in washer_load(world, e) {
        crate::storage::take_out(world, item, pos);
    }
}

/// `item_interaction` out of combat mode: into the open, clean, not full machine.
pub fn washer_item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(m) = world.get::<WashingMachine>(target).copied() else {
        return false;
    };
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    let name = crate::chem::name_of(world, target);
    if !m.open {
        tell(world, client, Span::Warning, "Сначала откройте дверцу!");
    } else if m.bloody {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} нужно сначала отмыть!"),
        );
    } else if washer_load(world, target).len() >= WASH_CAPACITY {
        tell(world, client, Span::Warning, "Стиральная машина полна!");
    } else if crate::inventory::no_drop(world, held) {
        tell(
            world,
            client,
            Span::Warning,
            "Предмет прилип к руке, его не положить в стиральную машину!",
        );
    } else {
        crate::storage::put_inside(world, held, target);
        washer_look(world, target);
    }
    true
}

/// `wash_cycle` when the timer runs out: every load item is washed (`wash(CLEAN_WASH)`).
// shortcut: no dyeing by a crayon or other `dye_color` item inside (`dye_registry` and the items'
// `dying_key` are not converted), no `machine_wash` specials but hides (the Ian costume), no
// shaking or walking around while running; add with the dye registry in the prototypes.
fn finish_washing(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &WashingMachine)>();
    let done: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.done_at.is_some_and(|t| now >= t))
        .map(|(e, _)| e)
        .collect();
    for e in done {
        for item in washer_load(world, e) {
            // `hairlesshide/machine_wash`: wet hide on the washer's tile.
            if !crate::sheets::soak(world, item) {
                wash(world, item, CLEAN_WASH);
            }
        }
        if let Some(mut m) = world.get_mut::<WashingMachine>(e) {
            m.done_at = None;
        }
        washer_look(world, e);
    }
}

// ---------------------------------------------------------------------------------------------
// Janitor's key.

const ACCESS_KEY: &str = "/obj/item/access_key";
/// `ACCESS_TIMER_LIMIT`.
pub const ACCESS_TIMER_SECS: f64 = 600.0;
/// `key_speed`, deciseconds.
const KEY_SPEED: f32 = 30.0;
const RATTLING_KEYS: &str = "sound/items/rattling_keys.ogg";
const DING: &str = "sound/machines/ding.ogg";
const SCANBUZZ: &str = "sound/machines/scanner/scanbuzz.ogg";

/// `/obj/item/access_key`: `department_access` and when it clears itself.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AccessKey {
    pub region: Option<(&'static str, f64)>,
}

fn add_access_key(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, ACCESS_KEY))
    {
        commands.entity(add.entity).insert(AccessKey::default());
    }
}

fn key_sound(world: &mut World, key: Entity, sound: &str) {
    if let Some(pos) = crate::explosions::turf_of(world, key) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 25.0, true));
    }
}

/// `COMSIG_ON_DEPARTMENT_ACCESS` -> `department_access_given` of every key: the region for 10
/// minutes (`TIMER_OVERRIDE`: a new grant restarts the timer).
pub fn grant_department_access(world: &mut World, region: &'static str) {
    let until = world.resource::<Time>().elapsed_secs_f64() + ACCESS_TIMER_SECS;
    let mut q = world.query_filtered::<Entity, With<AccessKey>>();
    let keys: Vec<Entity> = q.iter(world).collect();
    for key in keys {
        world.entity_mut(key).insert(AccessKey {
            region: Some((region, until)),
        });
        crate::atmos_machinery::machine_say(
            world,
            key,
            &format!("Доступ к зоне {region} предоставлен."),
        );
        key_sound(world, key, DING);
        info!("Access to the {region} department was given to the janitor key {key}");
    }
}

/// `clear_access` when the timer runs out.
fn expire_key_access(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &AccessKey)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, k)| k.region.is_some_and(|(_, at)| now >= at))
        .map(|(e, _)| e)
        .collect();
    for key in due {
        world.entity_mut(key).insert(AccessKey { region: None });
        crate::atmos_machinery::machine_say(world, key, "Доступ отозван, время вышло.");
        key_sound(world, key, SCANBUZZ);
    }
}

/// `door/item_interaction` with the key -> `attempt_open_door`: 3 s fumbling for the key.
/// True when `held` is the janitor's key and `target` a door.
pub fn key_on_door(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<AccessKey>(held).is_none() || world.get::<crate::doors::Door>(target).is_none() {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    // `DOING_INTERACTION_WITH_TARGET`: a do_after already running refuses the new one.
    if do_after(
        world,
        actor,
        target,
        KEY_SPEED,
        0,
        Job::Clean(CleanJob::AccessKey),
    ) {
        tell(world, client, Span::Notice, "Вы ищете нужный ключ...");
        key_sound(world, held, RATTLING_KEYS);
    }
    true
}

/// The region's accesses `key` opens with now (`accesses_by_region[department_access]`).
fn key_access(world: &World, key: Entity) -> Option<Vec<String>> {
    let (region, _) = world.get::<AccessKey>(key)?.region?;
    Some(crate::id_access::region_access_list(&[region]))
}

/// The end of `attempt_open_door`: the door's access checked against the key's region, then
/// `try_to_activate_door(access_bypass = TRUE)`.
fn key_opens_door(world: &mut World, client: Entity, mob: Entity, key: Entity, door: Entity) {
    let Some(&mob_pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let access = key_access(world, key);
    let allowed = world
        .get::<crate::doors::Door>(door)
        .is_some_and(|d| access.as_deref().is_some_and(|a| d.allows(Some(a))));
    if !allowed {
        tell(world, client, Span::Warning, "Нет доступа!");
        return;
    }
    crate::doors::with_door(world, door, |door, appearance, ctx| {
        (
            (),
            ctx.activate(door, appearance, false, (mob_pos, access.as_deref())),
        )
    });
}

// ---------------------------------------------------------------------------------------------
// Push broom.

const PUSHBROOM: &str = "/obj/item/pushbroom";
/// `BROOM_PUSH_LIMIT`.
const BROOM_PUSH_LIMIT: usize = 20;
const THUDSWOOSH: &str = "sound/items/weapons/thudswoosh.ogg";

/// A push broom braced in both hands (tg `two_handed` wielded, `on_wield`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BroomWielded;

/// tg `two_handed` `attack_self` of a push broom: wields it (the other hand must be free) or
/// lets go. True when `item` is a push broom.
// shortcut: no wielded force (12) or icon, no jousting or walking aid; add with two_handed.
pub fn broom_attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !is_under(&item_path(world, item), PUSHBROOM) {
        return false;
    }
    if world.get::<BroomWielded>(item).is_some() {
        world.entity_mut(item).remove::<BroomWielded>();
        tell(
            world,
            client,
            Span::Notice,
            "Вы перестаёте держать метлу обеими руками.",
        );
        return true;
    }
    if !other_hand_free(world, mob, item) {
        tell(
            world,
            client,
            Span::Warning,
            "Вторая рука должна быть свободна!",
        );
        return true;
    }
    world.entity_mut(item).insert(BroomWielded);
    tell(
        world,
        client,
        Span::Notice,
        "Вы упираете метлу в пол, приняв уверенную подметающую стойку.",
    );
    true
}

/// `item` is in one of `mob`'s hands and the other hand is empty.
fn other_hand_free(world: &World, mob: Entity, item: Entity) -> bool {
    world
        .get::<crate::inventory::Inventory>(mob)
        .is_some_and(|inv| inv.hands.contains(&Some(item)) && inv.hands.contains(&None))
}

/// The mob's wielded push broom, still held in both hands.
fn wielded_broom(world: &World, mob: Entity) -> Option<Entity> {
    let inv = world.get::<crate::inventory::Inventory>(mob)?;
    inv.hands
        .iter()
        .flatten()
        .copied()
        .find(|&h| world.get::<BroomWielded>(h).is_some())
        .filter(|&h| other_hand_free(world, mob, h))
}

/// `sweep` on `COMSIG_MOVABLE_PRE_MOVE` of a mob with a wielded broom: the items on the tile it
/// steps onto are pushed one tile further. Run after the step, so they leave the mob's new tile.
fn sweep_on_moved(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    // Cost: one inventory lookup per mob step.
    for m in moved {
        if world.get::<crate::inventory::Inventory>(m.entity).is_none() {
            continue;
        }
        if wielded_broom(world, m.entity).is_some() {
            do_sweep(world, m.to, m.dir);
        }
    }
}

/// `/proc/do_sweep`: up to `BROOM_PUSH_LIMIT` loose items of `pos` step `dir` (each `Move`
/// refused by what blocks it), with a swoosh when there was anything.
pub fn do_sweep(world: &mut World, pos: TilePos, dir: ss13_shared::Dir) -> usize {
    let items: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<DmItem>(e).is_some_and(|i| !i.anchored))
        .filter(|&e| {
            world
                .get::<crate::obj_damage::Anchored>(e)
                .is_none_or(|a| !a.0)
        })
        .take(BROOM_PUSH_LIMIT)
        .collect();
    if items.is_empty() {
        return 0;
    }
    let mut swept = 0;
    for item in items {
        if crate::pull::step_once(world, item, dir, None).is_ok() {
            swept += 1;
        }
    }
    let mut sound = crate::sound::PlaySound::new(pos, THUDSWOOSH, 30.0, true);
    sound.extrarange = -1.0;
    crate::sound::playsound(world, sound);
    swept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acid() -> ss13_reagents::ReagentDef {
        ss13_reagents::ReagentDef {
            path: ACID.into(),
            name: "Sulfuric Acid".into(),
            description: String::new(),
            color: "#00FF32".into(),
            taste_description: String::new(),
            taste_mult: 1.0,
            metabolization_rate: 0.2,
            overdose_threshold: 0.0,
            ph: 2.75,
            specific_heat: 200.0,
            chemical_flags: 0,
            purity: 1.0,
            creation_purity: 1.0,
            inverse_chem: None,
            inverse_chem_val: 0.25,
            purge_multiplier: 1.0,
            toxpwr: 1.0,
            health_required: 0.0,
            liver_tolerance_multiplier: 1.0,
            boozepwr: 0.0,
            brute_heal: 0.0,
            nutriment_factor: 0.0,
            penetrates_skin: 0,
            default_container: String::new(),
            addiction_types: Vec::new(),
        }
    }

    #[test]
    fn acid_burns_by_its_exposure_method() {
        let chem = ss13_reagents::Chemistry::new(ss13_reagents::ChemData {
            reagents: vec![acid()],
            ..Default::default()
        });
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(Chem(std::sync::Arc::new(chem.clone())));
        let mut cup = Holder::new(50.0, flags::NO_REACT, Owner::default());
        cup.add_reagent(&chem, 0, 10.0, 300.0, None, None, true, false);
        let brute = |world: &mut World, methods: u32| {
            let mob = world
                .spawn((
                    Health::default(),
                    Stat::Conscious,
                    crate::chem::MobReagents {
                        blood: crate::chem::mob_holder(),
                        stomach: crate::chem::mob_holder(),
                        drunk: 0.0,
                    },
                ))
                .id();
            expose_methods(world, mob, &cup, methods);
            world.get::<Health>(mob).unwrap().brute
        };
        // `min(6 * toxpwr, volume * toxpwr)`; injected 1.5 times; on the skin `acid_act`.
        assert!((brute(&mut world, INGEST) - 6.0).abs() < 1e-3);
        assert!((brute(&mut world, INJECT) - 9.0).abs() < 1e-3);
        assert_eq!(brute(&mut world, PATCH), 0.0);
    }

    #[test]
    fn add_wet_follows_do_add_wet() {
        let mut wet = WetFloor::default();
        assert!(!wet.add_wet(TURF_WET_WATER, 0.0, 60.0, MAXIMUM_WET_TIME));
        // A mop's water: 10 s minimum, 60 s added on the next mopping.
        assert!(wet.add_wet(TURF_WET_WATER, 10.0, 60.0, MAXIMUM_WET_TIME));
        assert_eq!(wet.time_left, vec![(TURF_WET_WATER, 10.0)]);
        wet.add_wet(TURF_WET_WATER, 10.0, 60.0, MAXIMUM_WET_TIME);
        assert_eq!(wet.time_left, vec![(TURF_WET_WATER, 70.0)]);
        for _ in 0..10 {
            wet.add_wet(TURF_WET_WATER, 10.0, 60.0, MAXIMUM_WET_TIME);
        }
        assert_eq!(wet.time_left, vec![(TURF_WET_WATER, MAXIMUM_WET_TIME)]);
        wet.add_wet(TURF_WET_LUBE, 5.0, 0.0, MAXIMUM_WET_TIME);
        assert_eq!(wet.highest(), TURF_WET_LUBE);
    }

    #[test]
    fn wet_floor_dries_by_temperature() {
        let mut wet = WetFloor::default();
        wet.add_wet(TURF_WET_WATER, 10.0, 0.0, MAXIMUM_WET_TIME);
        // 20 °C: (20 / 2) × (1 s / 1.5 s) = 6.67 ds per second, 10 s of water last 15 s.
        for _ in 0..14 {
            wet.process(T0C + 20.0, 1.0);
        }
        assert_eq!(wet.highest(), TURF_WET_WATER);
        // The 15th second leaves float dust at most, the 16th none.
        wet.process(T0C + 20.0, 1.0);
        assert!(
            wet.time_left.iter().all(|&(_, t)| t < 1e-3),
            "{:?}",
            wet.time_left
        );
        wet.process(T0C + 20.0, 1.0);
        assert!(wet.time_left.is_empty(), "{:?}", wet.time_left);
        // Freezing: nothing dries; boiling: everything at once.
        wet.add_wet(TURF_WET_WATER, 10.0, 0.0, MAXIMUM_WET_TIME);
        wet.process(T0C - 5.0, 100.0);
        assert_eq!(wet.highest(), TURF_WET_WATER);
        wet.process(T0C + 150.0, 1.0);
        assert_eq!(wet.highest(), 0);
    }

    /// A grant puts the region on the key for 10 minutes; the key opens a door of that region
    /// only.
    #[test]
    fn janitor_key_holds_a_region_for_ten_minutes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, expire_key_access);
        let world = app.world_mut();
        let key = world.spawn(AccessKey::default()).id();
        assert_eq!(key_access(world, key), None);
        grant_department_access(world, "Engineering");
        let access = key_access(world, key).unwrap();
        assert!(access.iter().any(|a| a == "engineering"), "{access:?}");
        assert!(crate::doors::test_airlock(&["engineering"]).allows(Some(&access)));
        assert!(!crate::doors::test_airlock(&["brig"]).allows(Some(&access)));
        let (_, until) = world.get::<AccessKey>(key).unwrap().region.unwrap();
        assert!(until >= ACCESS_TIMER_SECS);
        world.get_mut::<AccessKey>(key).unwrap().region = Some(("Engineering", 0.0));
        app.update();
        assert_eq!(app.world().get::<AccessKey>(key).unwrap().region, None);
    }

    /// Only a head's trim names a region to grant.
    #[test]
    fn heads_grant_their_region() {
        use crate::id_access::{CardAccess, Trim};
        use crate::security_level::keycard::head_region;
        let mut world = World::new();
        let card = world
            .spawn((
                crate::inventory::IdCard { access: Vec::new() },
                CardAccess {
                    trim: Some(Trim {
                        job: "Chief Engineer".into(),
                        ..default()
                    }),
                    wildcard_slots: Vec::new(),
                    registered_age: 30,
                },
            ))
            .id();
        let mob = world
            .spawn(crate::inventory::Inventory {
                id: Some(card),
                ..default()
            })
            .id();
        assert_eq!(head_region(&world, mob), Some("Engineering"));
        world
            .get_mut::<CardAccess>(card)
            .unwrap()
            .trim
            .as_mut()
            .unwrap()
            .job = "Janitor".into();
        assert_eq!(head_region(&world, mob), None);
    }

    /// A closed washing machine runs its cycle and washes the blood off its load; opening it
    /// drops the load.
    #[test]
    fn washing_machine_washes_its_load() {
        use crate::forensics::server::Forensics;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ss13_world::WorldPlugin));
        app.add_systems(Update, finish_washing);
        let world = app.world_mut();
        world
            .resource_mut::<SpatialIndex>()
            .create_level(1, 4, 4)
            .unwrap();
        let machine = world
            .spawn((
                WashingMachine {
                    open: true,
                    done_at: None,
                    bloody: false,
                },
                TilePos::new(1, 1, 1),
            ))
            .id();
        let shirt = world
            .spawn(Forensics {
                blood_dna: vec![("abc".into(), crate::medical::blood::BloodType::APlus)],
                ..default()
            })
            .id();
        crate::storage::put_inside(world, shirt, machine);
        let client = world.spawn_empty().id();
        // Shut the door, start the cycle.
        assert!(washer_attack_hand(world, client, machine, false));
        assert!(washer_attack_hand(world, client, machine, true));
        assert!(
            world
                .get::<WashingMachine>(machine)
                .unwrap()
                .done_at
                .is_some()
        );
        world.get_mut::<WashingMachine>(machine).unwrap().done_at = Some(0.0);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.get::<WashingMachine>(machine).unwrap().done_at, None);
        assert!(world.get::<Forensics>(shirt).unwrap().blood_dna.is_empty());
        assert!(washer_attack_hand(world, client, machine, false));
        assert_eq!(world.get::<TilePos>(shirt), Some(&TilePos::new(1, 1, 1)));
        assert!(washer_load(world, machine).is_empty());
    }

    /// The broom pushes loose items one tile on, not through walls and not anchored ones; it
    /// sweeps only braced with the other hand free.
    #[test]
    fn broom_sweeps_loose_items() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ss13_world::WorldPlugin));
        let world = app.world_mut();
        world
            .resource_mut::<SpatialIndex>()
            .create_level(1, 6, 6)
            .unwrap();
        let item = |world: &mut World, anchored: bool| {
            world
                .spawn((
                    DmItem {
                        path: "/obj/item/trash/candy".into(),
                        anchored,
                        ..default()
                    },
                    TilePos::new(2, 2, 1),
                ))
                .id()
        };
        let (loose, fixed) = (item(world, false), item(world, true));
        world.spawn((TilePos::new(2, 4, 1), Density));
        assert_eq!(
            do_sweep(world, TilePos::new(2, 2, 1), ss13_shared::Dir::North),
            1
        );
        assert_eq!(world.get::<TilePos>(loose), Some(&TilePos::new(2, 3, 1)));
        assert_eq!(world.get::<TilePos>(fixed), Some(&TilePos::new(2, 2, 1)));
        assert_eq!(
            do_sweep(world, TilePos::new(2, 3, 1), ss13_shared::Dir::North),
            0
        );
        assert_eq!(world.get::<TilePos>(loose), Some(&TilePos::new(2, 3, 1)));

        let broom = world.spawn(BroomWielded).id();
        let other = world.spawn_empty().id();
        let mob = world
            .spawn(crate::inventory::Inventory {
                hands: [Some(broom), None],
                ..default()
            })
            .id();
        assert_eq!(wielded_broom(world, mob), Some(broom));
        world
            .get_mut::<crate::inventory::Inventory>(mob)
            .unwrap()
            .hands[1] = Some(other);
        assert_eq!(wielded_broom(world, mob), None);
    }

    /// Walking or critical carbons pass holobarriers; running conscious ones do not.
    #[test]
    fn walkers_and_the_critical_pass_barriers() {
        let mut app = App::new();
        app.add_systems(Update, sync_walk_barriers);
        let mob = app
            .world_mut()
            .spawn((
                Health::default(),
                crate::inventory::Inventory::default(),
                Walking(false),
                Stat::Conscious,
            ))
            .id();
        let passes = |app: &App| {
            app.world()
                .get::<ss13_world::PassesWalkBarriers>(mob)
                .is_some()
        };
        app.update();
        assert!(!passes(&app));
        app.world_mut().entity_mut(mob).insert(Walking(true));
        app.update();
        assert!(passes(&app));
        app.world_mut().entity_mut(mob).insert(Walking(false));
        app.update();
        assert!(!passes(&app));
        app.world_mut().entity_mut(mob).insert(Stat::HardCrit);
        app.update();
        assert!(passes(&app));
    }

    #[test]
    fn slip_rules_by_wetness() {
        let mut wet = WetFloor::default();
        assert_eq!(wet.slip(), None);
        wet.add_wet(TURF_WET_WATER, 10.0, 0.0, MAXIMUM_WET_TIME);
        // Water: knockdown 60 ds, `NO_SLIP_WHEN_WALKING`.
        assert_eq!(wet.slip(), Some((6.0, true)));
        wet.add_wet(TURF_WET_LUBE, 10.0, 0.0, MAXIMUM_WET_TIME);
        assert_eq!(wet.slip(), Some((8.0, false)));
        wet.add_wet(TURF_WET_ICE, 10.0, 0.0, MAXIMUM_WET_TIME);
        assert_eq!(wet.slip(), Some((8.0, false)), "lube outranks ice");
        wet.add_wet(TURF_WET_SUPERLUBE, 10.0, 0.0, MAXIMUM_WET_TIME);
        assert_eq!(wet.slip(), Some((12.0, false)));
        let _ = TURF_WET_PERMAFROST;
    }
}
