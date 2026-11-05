//! Electromagnetic pulses (server), after tg `empulse()` (`code/game/objects/empulse.dm`),
//! `/atom/proc/emp_act` (`atom_act.dm`), `/datum/element/empprotection` and the `emp_act`
//! overrides of the families the port has.
//!
//! - A source writes [`Empulse`] (a pulse around a tile) or [`EmpHit`] (one atom, e.g. an event
//!   hitting the telecomms). [`resolve`] turns them into one [`EmpAct`] per atom hit: every atom
//!   on the tiles within `light_range` (Chebyshev, tg `get_dist`) gets `EMP_HEAVY` closer than
//!   `heavy_range`, a coin flip between heavy and light at exactly `heavy_range`, `EMP_LIGHT`
//!   up to `light_range`. A pulse with `heavy_range > 1` draws `emp/pulse` on its epicenter.
//! - `emp_act` of an atom ([`emp_act`]): its protection (tg `COMSIG_ATOM_PRE_EMP_ACT`: the
//!   [`EmpProtection`] component, else the types that add `empprotection` in `Initialize`,
//!   [`type_protection`]), its wires pulsed unless `EMP_PROTECT_WIRES` (`wires.emp_pulse()`),
//!   the [`EmpAct`] for the families' handlers, then its contents unless
//!   `EMP_PROTECT_CONTENTS`: a living mob's worn and held items and organs, with its clothing's
//!   `emp_protection` added to the severity (`/mob/living/emp_act`, `get_emp_protection`); a
//!   storage item's contents (`/datum/storage/on_emp_act`); a closet's objects unless it is
//!   itself protected (`/obj/structure/closet/emp_act`). A mob inside a closet is not hit.
//! - Handlers are systems in [`EmpSystems::Effects`] reading [`EmpAct`] ([`emp_acts`]); each
//!   family's own effect lives in the submodules: [`power`] (cells, APCs, SMES, lights, the
//!   generic machine's energy use, canisters, mechs), [`machines`] (doors, cameras, R&D servers,
//!   status displays, closets, atmos machines, wall mounts, the chem dispenser, telecomms) and
//!   [`mobs`] (silicons, bots, organs, radios, guns, batons).
//! - [`Emped`] is tg `machine_stat & EMPED`: the machine is out until the time, then the
//!   handler that set it restores it.
// shortcut: severities are tg's numbers (1 heavy, 2 light, more through a mob's clothing);
// holograms do not fade (no holograms), no examine tags for protected atoms, no admin log of
// the source.

pub mod machines;
pub mod mobs;
pub mod power;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmItem, DmObj};
use ss13_net::server::Networked;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, Layer, MouseOpacity, TilePos};
use ss13_world::SpatialIndex;

use crate::closets::{Closet, InCloset};
use crate::health::Health;
use crate::inventory::{ALL_SLOTS, Inventory};
use crate::medical::organs::Organs;
use crate::storage::{Storage, Stored};

/// tg `EMP_HEAVY`, `EMP_LIGHT`.
pub const EMP_HEAVY: u8 = 1;
pub const EMP_LIGHT: u8 = 2;
/// tg `EMP_PROTECT_SELF`, `EMP_PROTECT_CONTENTS`, `EMP_PROTECT_WIRES`.
pub const PROTECT_SELF: u8 = 1 << 0;
pub const PROTECT_CONTENTS: u8 = 1 << 1;
pub const PROTECT_WIRES: u8 = 1 << 2;
pub const PROTECT_ALL: u8 = PROTECT_SELF | PROTECT_CONTENTS | PROTECT_WIRES;
/// `STANDARD_CELL_CHARGE`, `STANDARD_BATTERY_CHARGE`, joules.
pub const STANDARD_CELL_CHARGE: f64 = 10_000.0;
pub const STANDARD_BATTERY_CHARGE: f64 = 1_000_000.0;
/// `/obj/effect/temp_visual/emp` and `emp/pulse`: `icon`, `icon_state`, `duration`.
const EFFECTS_DMI: &str = "icons/effects/effects.dmi";
const SPARKS_STATE: &str = "empdisable";
const PULSE_STATE: &str = "emppulse";
const SPARKS_SECS: f64 = 1.0;
const PULSE_SECS: f64 = 0.8;
/// `/obj/effect/temp_visual`: `ABOVE_MOB_LAYER` on the game plane.
const ABOVE_MOB_LAYER: f32 = 4.1;
const GAME_PLANE: i16 = -4;

/// The types that add `/datum/element/empprotection` in `Initialize` (or on a part, as the
/// modsuit's shield): their subtypes inherit it. Most specific first.
const PROTECTED_TYPES: &[(&str, u8)] = &[
    (
        "/obj/item/stock_parts/power_store/cell/emproof",
        PROTECT_SELF,
    ),
    ("/obj/item/gun/energy/e_gun/advtaser/cyborg", PROTECT_ALL),
    ("/obj/item/gun/energy/disabler/cyborg", PROTECT_ALL),
    ("/obj/item/gun/energy/laser/cyborg", PROTECT_ALL),
    (
        "/obj/item/gun/energy/laser/thermal",
        PROTECT_SELF | PROTECT_CONTENTS,
    ),
    ("/obj/item/gun/energy/laser/instakill", PROTECT_ALL),
    (
        "/obj/item/gun/energy/laser/chameleon",
        PROTECT_SELF | PROTECT_CONTENTS,
    ),
    ("/obj/item/gun/energy/ionrifle", PROTECT_ALL),
    ("/obj/item/gun/energy/printer", PROTECT_ALL),
    ("/obj/item/gun/energy/recharge", PROTECT_ALL),
    ("/obj/item/gun/energy/pulse", PROTECT_ALL),
    ("/obj/item/organ/eyes/robotic/flashlight", PROTECT_ALL),
    ("/obj/item/organ/eyes/robotic/shield", PROTECT_ALL),
    ("/obj/item/flashlight/emp", PROTECT_SELF),
    ("/obj/item/grenade/chem_grenade", PROTECT_WIRES),
    ("/obj/item/broadcast_camera", PROTECT_ALL),
    ("/obj/machinery/door/airlock/cult", PROTECT_ALL),
    ("/obj/machinery/door/password", PROTECT_ALL),
    ("/obj/machinery/door/puzzle", PROTECT_ALL),
    (
        "/obj/machinery/field/generator",
        PROTECT_SELF | PROTECT_WIRES,
    ),
    (
        "/obj/machinery/porta_turret/syndicate",
        PROTECT_SELF | PROTECT_WIRES,
    ),
    (
        "/obj/machinery/porta_turret/centcom_shuttle",
        PROTECT_SELF | PROTECT_WIRES,
    ),
    ("/obj/structure/emergency_shield/regenerating", PROTECT_SELF),
    ("/obj/structure/emergency_shield/cult", PROTECT_SELF),
    ("/obj/structure/closet/stasis", PROTECT_ALL),
];

/// Clothing `emp_protection` (`EMP_PROTECTION_MODERATE` = 1): added to the severity of what a
/// wearer's contents get.
const PROTECTIVE_CLOTHING: &[(&str, u8)] = &[
    ("/obj/item/clothing/head/costume/foilhat", 1),
    ("/obj/item/clothing/suit/armor/reactive/tesla", 1),
    ("/obj/item/clothing/suit/armor/heavy/adamantine", 1),
];

/// tg `empulse(epicenter, heavy_range, light_range)`.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Empulse {
    pub epicenter: TilePos,
    pub heavy: i32,
    pub light: i32,
}

/// tg `target.emp_act(severity)` from a source (not a pulse).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct EmpHit {
    pub target: Entity,
    pub severity: u8,
}

/// One atom's `emp_act`: what its family's handler acts on. `protection` is what the atom's
/// protection returned (`EMP_PROTECT_*`).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct EmpAct {
    pub target: Entity,
    pub severity: u8,
    pub protection: u8,
}

impl EmpAct {
    /// Not `EMP_PROTECT_SELF`: the atom's own effect happens.
    pub fn hits_self(&self) -> bool {
        self.protection & PROTECT_SELF == 0
    }

    /// Not `EMP_PROTECT_CONTENTS`: what it holds is hit too.
    pub fn hits_contents(&self) -> bool {
        self.protection & PROTECT_CONTENTS == 0
    }

    /// `value / severity`, the usual scaling.
    pub fn by_severity(&self, value: f64) -> f64 {
        value / f64::from(self.severity.max(1))
    }
}

/// Runtime `/datum/element/empprotection` (added by implants, modules...), on top of
/// [`type_protection`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct EmpProtection(pub u8);

/// tg `machine_stat & EMPED`: out of order until `until` (game seconds).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Emped {
    pub until: f64,
}

/// EMP rolls (DM `prob`, `rand`, `pick`), seeded from the map seed
/// ([`crate::server::seed_rngs`]).
#[derive(Resource)]
pub struct EmpRng(pub Rng);

impl Default for EmpRng {
    fn default() -> Self {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self(Rng::new(seed))
    }
}

impl EmpRng {
    /// DM `prob(percent)`.
    pub fn prob(&mut self, percent: f64) -> bool {
        (self.0.next_u64() % 10_000) as f64 / 100.0 < percent
    }

    /// DM `rand(lo, hi)` on integers.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        self.0.range(lo, hi)
    }
}

/// `/obj/effect/temp_visual/emp` and `emp/pulse`: gone at `until`.
#[derive(Component, Clone, Copy, Debug)]
pub struct EmpVisual {
    pub until: f64,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EmpSystems {
    /// Pulses and hits become [`EmpAct`]s.
    Resolve,
    /// The families' handlers.
    Effects,
    /// What ran out of [`Emped`] comes back.
    Restore,
}

pub struct EmpPlugin;

impl Plugin for EmpPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EmpRng>()
            .add_message::<Empulse>()
            .add_message::<EmpHit>()
            .add_message::<EmpAct>()
            .configure_sets(
                Update,
                (
                    EmpSystems::Resolve.before(EmpSystems::Effects),
                    // Idle ticks cost one message check, not a pass of every handler.
                    EmpSystems::Effects.run_if(on_message::<EmpAct>),
                    EmpSystems::Restore.run_if(any_with_component::<Emped>),
                ),
            )
            .add_systems(
                Update,
                (
                    resolve.in_set(EmpSystems::Resolve),
                    expire_visuals.run_if(any_with_component::<EmpVisual>),
                ),
            )
            .add_plugins((
                power::EmpPowerPlugin,
                machines::EmpMachinesPlugin,
                mobs::EmpMobsPlugin,
            ));
    }
}

/// The [`EmpAct`]s a handler has not seen yet.
pub fn emp_acts(world: &World, cursor: &mut MessageCursor<EmpAct>) -> Vec<EmpAct> {
    cursor
        .read(world.resource::<Messages<EmpAct>>())
        .copied()
        .collect()
}

/// Turns this tick's pulses and hits into [`EmpAct`]s.
pub fn resolve(
    world: &mut World,
    mut pulses: Local<MessageCursor<Empulse>>,
    mut hits: Local<MessageCursor<EmpHit>>,
) {
    let pulses: Vec<Empulse> = pulses
        .read(world.resource::<Messages<Empulse>>())
        .copied()
        .collect();
    let hits: Vec<EmpHit> = hits
        .read(world.resource::<Messages<EmpHit>>())
        .copied()
        .collect();
    for pulse in pulses {
        empulse(world, pulse);
    }
    for hit in hits {
        emp_act(world, hit.target, hit.severity);
    }
}

/// tg `empulse()`: `emp_act` of every atom within `light` tiles of the epicenter.
pub fn empulse(world: &mut World, pulse: Empulse) {
    let Empulse {
        epicenter,
        heavy,
        mut light,
    } = pulse;
    if !world.resource::<SpatialIndex>().in_bounds(epicenter) {
        return;
    }
    if heavy > 1 {
        spawn_visual(world, epicenter, PULSE_STATE, PULSE_SECS);
    }
    light = light.max(heavy);
    if light < 0 {
        return;
    }
    let mut targets: Vec<(Entity, u8)> = Vec::new();
    for pos in spiral(epicenter, light as u32) {
        let distance = epicenter.chebyshev(pos).unwrap_or(0) as i32;
        let things: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| world.get::<InCloset>(e).is_none())
            .collect();
        for thing in things {
            let severity = if distance < heavy {
                EMP_HEAVY
            } else if distance == heavy {
                if world.resource_mut::<EmpRng>().prob(50.0) {
                    EMP_HEAVY
                } else {
                    EMP_LIGHT
                }
            } else {
                EMP_LIGHT
            };
            targets.push((thing, severity));
        }
    }
    for (thing, severity) in targets {
        emp_act(world, thing, severity);
    }
}

/// tg `spiral_range` turfs: the center, then each ring outwards.
fn spiral(center: TilePos, dist: u32) -> impl Iterator<Item = TilePos> {
    std::iter::once(center).chain((1..=dist as i32).flat_map(move |d| {
        let (cx, cy, z) = (center.x, center.y, center.z);
        let top = (cx - d + 1..=cx + d).map(move |x| TilePos::new(x, cy + d, z));
        let right = (cy - d..=cy + d - 1)
            .rev()
            .map(move |y| TilePos::new(cx + d, y, z));
        let bottom = (cx - d..=cx + d - 1)
            .rev()
            .map(move |x| TilePos::new(x, cy - d, z));
        let left = (cy - d + 1..=cy + d).map(move |y| TilePos::new(cx - d, y, z));
        top.chain(right).chain(bottom).chain(left)
    }))
}

/// tg `emp_act(severity)` of one atom and, unless it protects them, its contents.
pub fn emp_act(world: &mut World, target: Entity, severity: u8) {
    let mut queue = vec![(target, severity)];
    while let Some((e, severity)) = queue.pop() {
        if world.get_entity(e).is_err() {
            continue;
        }
        let protection = protection_of(world, e);
        if protection & PROTECT_WIRES == 0 && world.get::<crate::wires::Wires>(e).is_some() {
            crate::wires::emp_pulse(world, e);
        }
        world.write_message(EmpAct {
            target: e,
            severity,
            protection,
        });
        if protection & PROTECT_CONTENTS == 0 {
            queue.extend(contents(world, e, protection, severity));
        }
    }
}

/// What `emp_act` of `e` passes its pulse on to, with the severity they get.
fn contents(world: &World, e: Entity, protection: u8, severity: u8) -> Vec<(Entity, u8)> {
    if world.get::<Health>(e).is_some() {
        let severity = severity.saturating_add(mob_protection(world, e));
        let held = world
            .get::<Inventory>(e)
            .map(|i| {
                ALL_SLOTS
                    .iter()
                    .filter_map(|&s| i.slot(s))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let organs = world
            .get::<Organs>(e)
            .map(|o| o.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        return held
            .into_iter()
            .chain(organs)
            .map(|c| (c, severity))
            .collect();
    }
    let closet = world.get::<Closet>(e).is_some();
    if (world.get::<Storage>(e).is_some() || closet && protection & PROTECT_SELF == 0)
        && let Some(stored) = world.get::<Stored>(e)
    {
        return stored.items().iter().map(|&c| (c, severity)).collect();
    }
    Vec::new()
}

/// `COMSIG_ATOM_PRE_EMP_ACT`: the atom's `EMP_PROTECT_*` flags.
pub fn protection_of(world: &World, e: Entity) -> u8 {
    let own = world.get::<EmpProtection>(e).map_or(0, |p| p.0);
    own | path_of(world, e).map_or(0, |p| type_protection(&p))
}

/// The `empprotection` a type adds in `Initialize`.
pub fn type_protection(path: &str) -> u8 {
    PROTECTED_TYPES
        .iter()
        .find(|(t, _)| is_under(path, t))
        .map_or(0, |&(_, f)| f)
}

/// `get_emp_protection()` of a human: its clothing's `emp_protection`.
fn mob_protection(world: &World, mob: Entity) -> u8 {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return 0;
    };
    ALL_SLOTS
        .iter()
        .filter(|s| !matches!(s, InventorySlot::LeftHand | InventorySlot::RightHand))
        .filter_map(|&s| inventory.slot(s))
        .filter_map(|e| path_of(world, e))
        .map(|p| {
            PROTECTIVE_CLOTHING
                .iter()
                .find(|(t, _)| is_under(&p, t))
                .map_or(0, |&(_, n)| n)
        })
        .sum()
}

/// The DM type of an item or object.
pub fn path_of(world: &World, e: Entity) -> Option<String> {
    let entity = world.get_entity(e).ok()?;
    entity
        .get::<DmItem>()
        .map(|i| i.path.clone())
        .or_else(|| entity.get::<DmObj>().map(|o| o.path.clone()))
}

/// Marks `e` [`Emped`] until `now + secs` (a later pulse extends it).
pub fn set_emped(world: &mut World, e: Entity, secs: f64) -> f64 {
    let until = world.resource::<Time>().elapsed_secs_f64() + secs;
    let until = world
        .get::<Emped>(e)
        .map_or(until, |old| old.until.max(until));
    world.entity_mut(e).insert(Emped { until });
    until
}

/// Entities whose [`Emped`] ran out; their handler restores them and removes it.
pub fn emped_expired<F: bevy::ecs::query::QueryFilter>(world: &mut World) -> Vec<Entity> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query_filtered::<(Entity, &Emped), F>();
    query
        .iter(world)
        .filter(|(_, e)| now >= e.until)
        .map(|(e, _)| e)
        .collect()
}

/// `new /obj/effect/temp_visual/emp(loc)`: the sparks of an EMPed machine or bot.
pub fn sparks(world: &mut World, e: Entity) {
    if let Some(&pos) = world.get::<TilePos>(e) {
        spawn_visual(world, pos, SPARKS_STATE, SPARKS_SECS);
    }
}


/// tg `do_sparks(3, TRUE, source)`: the spark system of `crate::effects::particles`.
pub fn do_sparks(world: &mut World, pos: TilePos) {
    crate::effects::particles::do_sparks(world, 3, true, pos);
}

fn spawn_visual(world: &mut World, pos: TilePos, state: &str, secs: f64) {
    let Some(icon) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(EFFECTS_DMI, state))
    else {
        return;
    };
    let until = world.resource::<Time>().elapsed_secs_f64() + secs;
    world.spawn((
        Name::new(format!("emp {state}")),
        pos,
        Appearance {
            icon_state: icon,
            layer: Layer(ABOVE_MOB_LAYER),
            plane: GAME_PLANE,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            overlays: vec![],
            mouse_opacity: MouseOpacity::Transparent,
        },
        Networked,
        EmpVisual { until },
    ));
}

fn expire_visuals(time: Res<Time>, visuals: Query<(Entity, &EmpVisual)>, mut commands: Commands) {
    let now = time.elapsed_secs_f64();
    for (e, v) in &visuals {
        if now >= v.until {
            commands.entity(e).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiral_visits_each_tile_of_the_square_once_ring_by_ring() {
        let c = TilePos::new(10, 10, 1);
        let tiles: Vec<TilePos> = spiral(c, 2).collect();
        assert_eq!(tiles.len(), 25);
        let unique: std::collections::HashSet<_> = tiles.iter().copied().collect();
        assert_eq!(unique.len(), 25);
        assert_eq!(tiles[0], c);
        // Rings in order of distance.
        let dists: Vec<u32> = tiles.iter().map(|t| c.chebyshev(*t).unwrap()).collect();
        assert!(dists.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(spiral(c, 0).count(), 1);
    }

    #[test]
    fn protection_follows_initialize_types() {
        assert_eq!(
            type_protection("/obj/item/gun/energy/pulse/carbine"),
            PROTECT_ALL
        );
        assert_eq!(
            type_protection("/obj/item/stock_parts/power_store/cell/emproof"),
            PROTECT_SELF
        );
        assert_eq!(type_protection("/obj/item/gun/energy/laser"), 0);
        assert_eq!(
            type_protection("/obj/item/grenade/chem_grenade/cleaner"),
            PROTECT_WIRES
        );
    }
}
