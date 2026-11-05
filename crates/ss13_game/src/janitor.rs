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
//! - A spray bottle clicked on an adjacent thing sprays `amount_per_transfer_from_this` onto
//!   its tile (`VAPOR`): space cleaner ≥ 1 u washes the turf and everything on it.
//! - `ClientIntent::ToggleMoveIntent` switches the mob between running and walking
//!   (`WALK_DELAY` 4 deciseconds per tile).
// shortcut: no cleaning skill (every mob has the `None` level's modifier 1), no bubbles overlay,
// no `TRAIT_CURRENTLY_CLEANING` (two mobs may scrub one tile at once), mobs are not washed
// (soap and mops skip them; items lose their traces, `crate::forensics`); no footprints, blood
// trails, flying hit splatters or bloody shoes; blood bloodiness does not decay while drying and dried blood keeps its name;
// no ice (wet floors below 0 °C just do not dry), lube, galoshes or noslip shoes, no sliding
// and no gravity check; the advanced mop does not self-wet; the cart holds no tools; sprays
// reach adjacent tiles only (`crate::click` resolves no ranged clicks) and have no puff; no
// sinks, so buckets are filled by pouring.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmCleanable, DmItem, DmObj, DmTurf, IconIndex, Turf};
use ss13_net::server::IntentReceived;
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::chat::Span;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_world::{Density, Moved, SpatialIndex, WorldSystems};

use crate::chem::{Chem, Reagents, TransferAmount};
use crate::combat::{CombatRng, NextMove};
use crate::construction::{Job, turf_at};
use crate::health::{BodyPosition, Health, HealthSystems, Stat};
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
/// `T0C`.
const T0C: f64 = 273.15;
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
const WATER: &str = "/datum/reagent/water";
const SPACE_CLEANER: &str = "/datum/reagent/space_cleaner";
/// `REAGENT_CLEANS`.
const REAGENT_CLEANS: u32 = 1 << 7;
/// `/obj/item/mop` `mopspeed`, deciseconds.
const MOPSPEED: f32 = 15.0;
/// `CLICK_CD_RANGE`, seconds.
const CLICK_CD_RANGE: f64 = 0.4;

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
    Mop { wash: bool },
    Soap,
}

#[derive(Resource, Default)]
struct JanitorClocks {
    wet: f64,
    dry: f64,
}

pub struct JanitorServerPlugin;

impl Plugin for JanitorServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JanitorClocks>()
            .add_observer(add_cleanable)
            .add_observer(add_item_parts)
            .add_observer(add_bucket_reagents)
            .add_systems(
                Update,
                (
                    toggle_move_intent.before(HealthSystems),
                    (
                        condensation,
                        slip_on_entered,
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
    let wiped = crate::forensics::server::clean_act(world, target, clean_types);
    match world.get::<Cleanable>(target) {
        Some(c) if c.clean_type & clean_types != 0 => {
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
    let mut cleaned = false;
    for e in on_tile {
        cleaned |= wash(world, e, clean_types);
    }
    cleaned
}

/// `reagents.expose(target, methods, volume_modifier)` for the reagents whose `expose_*` is
/// ported here (water and space cleaner on turfs and objects).
pub fn expose(world: &mut World, target: Entity, holder: &Holder, modifier: f32) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let turf = world
        .get::<Turf>(target)
        .and_then(|_| world.get::<TilePos>(target).copied());
    let living = world.get::<Health>(target).is_some();
    for r in &holder.reagents {
        let volume = r.volume * modifier;
        if chem.is_type(r.id, WATER) {
            if let Some(pos) = turf {
                // `/datum/reagent/water/expose_turf`.
                if volume >= 5.0 {
                    make_slippery(world, pos, TURF_WET_WATER, 10.0, (volume * 1.5).min(60.0));
                }
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
        }
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
/// `freeze_turf` makes it permafrost for 10 s.
// shortcut: freezing does not freeze the objects (`/datum/element/frozen`) or give cold mobs
// the freon status, and slimes are not watered; add with those mechanics.
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
        let Some(slip) = turf_at(world, m.to)
            .and_then(|t| world.get::<WetFloor>(t))
            .and_then(WetFloor::slip)
        else {
            continue;
        };
        // Active MOD magboots (`TRAIT_NO_SLIP_WATER`, `TRAIT_NO_SLIP_ICE`).
        if crate::modsuit::modules::resists_slip(world, m.entity, slip.1, slip.0) {
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
        do_after(world, actor, target, speed, 0, Job::Clean(CleanJob::Soap));
        return true;
    }
    if is_under(&path, SPRAY) && !modifiers.right {
        return spray(world, actor, target);
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
    do_after(
        world,
        actor,
        target,
        MOPSPEED,
        0,
        Job::Clean(CleanJob::Mop { wash }),
    );
    true
}

/// The cleaner's `clean()` after its do_after, then the mop's `apply_reagents` or the soap's
/// `decreaseUses`.
pub fn complete(world: &mut World, _mob: Entity, action: &DoAfter, job: CleanJob) {
    let target = action.target;
    let Some(item) = action.item.filter(|&i| world.get_entity(i).is_ok()) else {
        return;
    };
    match job {
        CleanJob::Mop { wash: do_wash } => {
            if do_wash {
                wash(world, target, CLEAN_SCRUB);
            }
            let Some(holder) = world.get::<Reagents>(item).map(|r| r.0.clone()) else {
                return;
            };
            if world.get_entity(target).is_ok() {
                // `reagents.expose(cleaned_turf, TOUCH, 10)`: "Needed for proper floor wetting."
                expose(world, target, &holder, 10.0);
            }
            if let Some(mut r) = world.get_mut::<Reagents>(item) {
                r.0.remove_all(1.0);
            }
        }
        CleanJob::Soap => {
            wash(world, target, CLEAN_SCRUB);
            let Some(mut soap) = world.get::<Soap>(item).copied() else {
                return;
            };
            soap.uses = soap.uses.saturating_sub(1);
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

/// `/obj/item/reagent_containers/spray/try_spray` + `spray` on an adjacent target: the puff
/// (`amount_per_transfer_from_this / range`, range 1) lands on the target's tile.
fn spray(world: &mut World, actor: Actor, target: Entity) -> bool {
    let spray = actor.item.expect("a held spray");
    let Some(mut holder) = world.get::<Reagents>(spray).map(|r| r.0.clone()) else {
        return false;
    };
    // A drainable, non-refillable target (a tank) refills the bottle with 50 u.
    if let Some(mut source) = world
        .get::<Reagents>(target)
        .map(|r| r.0.clone())
        .filter(|h| h.is_drainable() && !h.is_refillable())
    {
        let (src, dst) = (
            crate::chem::name_of(world, target),
            crate::chem::name_of(world, spray),
        );
        if source.total_volume == 0.0 {
            tell(world, actor.client, Span::Warning, format!("{src} пуст."));
        } else if holder.holder_full() {
            tell(world, actor.client, Span::Warning, format!("{dst} полон."));
        } else if let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) {
            let moved = source.trans_to(&chem, &mut holder, 50.0, 1.0, None, false, false);
            world.entity_mut(target).insert(Reagents(source));
            world.entity_mut(spray).insert(Reagents(holder));
            tell(
                world,
                actor.client,
                Span::Notice,
                format!(
                    "Вы наполняете {dst} {} ед. содержимого {src}.",
                    crate::chem::dm_num(moved)
                ),
            );
        }
        return true;
    }
    let amount = world.get::<TransferAmount>(spray).map_or(5.0, |t| t.0);
    if holder.total_volume < amount {
        tell(world, actor.client, Span::Warning, "Больше не осталось!");
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<NextMove>(actor.mob).is_some_and(|n| now < n.0) {
        return true;
    }
    let (Some(chem), Some(&pos)) = (
        world.get_resource::<Chem>().map(|c| c.0.clone()),
        world.get::<TilePos>(target),
    ) else {
        return false;
    };
    let mut puff = Holder::new(amount, 0, Owner::default());
    holder.trans_to(&chem, &mut puff, amount, 1.0, None, true, false);
    world.entity_mut(spray).insert(Reagents(holder));
    if let Some(mut next) = world.get_mut::<NextMove>(actor.mob) {
        next.0 = now + CLICK_CD_RANGE * 2.0;
    }
    // `spray_down_turf`: what is on the tile (not hidden below the floor), then the turf.
    let things: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world.get::<Turf>(e).is_none()
                && world.get::<ss13_mapload::HiddenUnderTile>(e).is_none()
        })
        .collect();
    for e in things {
        if world.get_entity(e).is_ok() {
            expose(world, e, &puff, 1.0);
        }
    }
    if let Some(turf) = turf_at(world, pos) {
        expose(world, turf, &puff, 1.0);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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
