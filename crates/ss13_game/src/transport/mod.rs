//! Linear transports (server): tg `SStransport` (`code/controllers/subsystem/transport.dm`),
//! `/datum/transport_controller/linear` (`code/modules/transport/linear_controller.dm`) and its
//! modules `/obj/structure/transport/linear` (`transport_module.dm`). Trams are [`tram`],
//! elevators [`elevator`].
//!
//! - At map init the modules flood-fill (cardinal neighbours and the modules straight above and
//!   below with the same `transport_id`) into one [`Transport`] each (`link_transport_modules`);
//!   an id landmark on a module gives the transport its `specific_transport_id` and is removed.
//! - A move ([`move_transport`], tg `travel` + `group_move`) shifts every module tile one step
//!   and carries what stands on them: not turfs, landmarks hidden as `INVISIBILITY_ABSTRACT`,
//!   things hidden under the floor, tram rails, cleanable decals or ghosts
//!   (`add_item_on_transport`). What is on the tiles it enters is hit first ([`collide`]):
//!   going down walls are devastated and the living crushed (gibbed with `violent_landing`, else
//!   paralyzed 30 s with 120 brute over the body), going up walls are devastated, going
//!   sideways walls are devastated, mineral walls drilled, structures smashed (anchored) or
//!   thrown, machines on the floor and higher destroyed and the living rammed
//!   (`7..21` or with 15% `29` times `maxHealth / 100` times the lethality, x2 to the head, x3 to
//!   the chest, x0.5 to each limb) and thrown off at 45 degrees.
// shortcut: modules stay one entity per tile (tg merges a tram's into one multi-tile object); no
// glide sizes, no admin reset of the contents, no spoiler beheading, no collision counter
// (persistence); thrown structures take no damage; the floor-plane layer rule of structures is
// the plain `LOW_OBJ_LAYER` one.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::undertile::HiddenUnderTile;
use ss13_mapload::{DmAtom, DmObj, DmTransport, DmTurfBuild, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::{Mapped, SpatialIndex};

use crate::health::{DamageType, Health, Stat, apply_damage_zone};
use crate::multiz::ZLevels;
use crate::obj_damage::Anchored;
use crate::zone::BodyZone;

pub mod elevator;
pub mod tram;

pub const MODULE: &str = "/obj/structure/transport/linear";
pub const ID_LANDMARK: &str = "/obj/effect/landmark/transport/transport_id";
/// `TRANSPORT_TYPE_ELEVATOR`, `TRANSPORT_TYPE_TRAM`.
pub const TYPE_ELEVATOR: &str = "icts_elev";
pub const TYPE_TRAM: &str = "icts_tram";

/// `controller_status` bits (`__DEFINES/transport.dm`).
pub const SYSTEM_FAULT: u32 = 1 << 0;
pub const COMM_ERROR: u32 = 1 << 1;
pub const EMERGENCY_STOP: u32 = 1 << 2;
pub const PRE_DEPARTURE: u32 = 1 << 3;
pub const DOORS_READY: u32 = 1 << 4;
pub const CONTROLS_LOCKED: u32 = 1 << 5;
pub const BYPASS_SENSORS: u32 = 1 << 6;

const INVISIBILITY_ABSTRACT: u8 = 101;
/// `LOW_OBJ_LAYER`.
const LOW_OBJ_LAYER: f32 = 2.5;
/// `add_item_on_transport`'s blacklist.
const NOT_CARRIED: [&str; 4] = [
    "/obj/structure/fluff/tram_rail",
    "/obj/effect/decal/cleanable",
    MODULE,
    "/mob/eye",
];
/// `ignored_smashthroughs`.
const SMASH_IGNORED: [&str; 4] = [
    "/obj/machinery/power/supermatter_crystal",
    "/obj/structure/holosign",
    "/obj/machinery/field",
    "/obj/structure/fluff/tram_rail",
];
const WALL: &str = "/turf/closed/wall";
const IMPACT_SOUND: &str = "sound/effects/meteorimpact.ogg";

/// A module tile of a [`Transport`].
#[derive(Component, Clone, Copy, Debug)]
pub struct TransportModule {
    pub transport: Entity,
}

/// tg `/datum/transport_controller/linear`, on its own entity.
#[derive(Component, Clone, Debug, Default)]
pub struct Transport {
    /// `transport_id`: [`TYPE_ELEVATOR`], [`TYPE_TRAM`].
    pub transport_id: String,
    /// `specific_transport_id`: what buttons, panels, consoles and doors link by.
    pub specific_id: String,
    pub modules: Vec<Entity>,
    /// `controller_status` bits.
    pub status: u32,
    /// The modules' `elevator_vertical_speed`, seconds.
    pub vertical_secs: f64,
    pub violent_landing: bool,
    /// The modules' `collision_lethality`.
    pub lethality: f32,
}

impl Transport {
    pub fn locked(&self) -> bool {
        self.status & CONTROLS_LOCKED != 0
    }
}

/// One move: a step on the level, or a level up or down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Flat(i32, i32),
    Up,
    Down,
}

pub struct TransportPlugin;

impl Plugin for TransportPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            ss13_world::MapInitLate,
            link_transports.after(crate::multiz::init_multiz),
        )
        .add_plugins((elevator::ElevatorPlugin, tram::TramPlugin));
    }
}

fn path_of(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmTransport>(e).map(|t| t.path.as_str())
}

/// The transport with `specific_id` and `transport_id`.
pub fn find(world: &mut World, transport_id: &str, specific_id: &str) -> Option<Entity> {
    let mut q = world.query::<(Entity, &Transport)>();
    q.iter(world)
        .find(|(_, t)| t.transport_id == transport_id && t.specific_id == specific_id)
        .map(|(e, _)| e)
}

/// tg `New` -> `link_transport_modules` and `check_for_landmarks` for every mapped module.
pub fn link_transports(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmTransport, &TilePos), With<Mapped>>();
    let modules: Vec<(Entity, DmTransport, TilePos)> = q
        .iter(world)
        .filter(|(_, t, _)| is_under(&t.path, MODULE))
        .map(|(e, t, p)| (e, t.clone(), *p))
        .collect();
    let by_pos: HashMap<(TilePos, String), Entity> = modules
        .iter()
        .map(|(e, t, p)| ((*p, t.transport_id.clone()), *e))
        .collect();
    let levels = world.resource::<ZLevels>().clone();
    let mut done: HashSet<Entity> = HashSet::new();
    let mut made = 0;
    for (start, dm, _) in &modules {
        if done.contains(start) {
            continue;
        }
        // `link_transport_modules`: a flood fill over `module_adjacency`.
        let mut group = vec![*start];
        done.insert(*start);
        let mut i = 0;
        while i < group.len() {
            let pos = *world
                .get::<TilePos>(group[i])
                .expect("modules are on tiles");
            i += 1;
            let mut next = vec![
                pos.step(Dir::North),
                pos.step(Dir::South),
                pos.step(Dir::East),
                pos.step(Dir::West),
            ];
            next.extend(levels.step(pos, true));
            next.extend(levels.step(pos, false));
            for p in next {
                if let Some(&n) = by_pos.get(&(p, dm.transport_id.clone()))
                    && done.insert(n)
                {
                    group.push(n);
                }
            }
        }
        let tiles: Vec<TilePos> = group
            .iter()
            .filter_map(|&m| world.get::<TilePos>(m).copied())
            .collect();
        // `check_for_landmarks`: an id landmark on the transport names it.
        let mut specific_id = String::new();
        for &p in &tiles {
            let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(p).to_vec();
            for e in here {
                if path_of(world, e).is_some_and(|p| is_under(p, ID_LANDMARK)) {
                    let id = world
                        .get::<DmTransport>(e)
                        .map(|t| t.specific_transport_id.clone())
                        .unwrap_or_default();
                    if !id.is_empty() {
                        specific_id = id;
                    }
                    world.despawn(e);
                }
            }
        }
        group.sort_by_key(|&m| world.get::<TilePos>(m).map_or(0, |p| p.z));
        let transport = world
            .spawn(Transport {
                transport_id: dm.transport_id.clone(),
                specific_id,
                modules: group.clone(),
                status: 0,
                vertical_secs: f64::from(dm.elevator_vertical_speed) / 10.0,
                violent_landing: dm.violent_landing,
                lethality: 1.0,
            })
            .id();
        for m in group {
            world.entity_mut(m).insert(TransportModule { transport });
        }
        made += 1;
    }
    if made > 0 {
        info!("transports: {made} linked from {} modules", modules.len());
    }
}

/// The tiles of the transport's modules.
pub fn tiles(world: &World, transport: &Transport) -> Vec<TilePos> {
    transport
        .modules
        .iter()
        .filter_map(|&m| world.get::<TilePos>(m).copied())
        .collect()
}

/// tg `get_zs_we_are_on`.
pub fn zs(world: &World, transport: &Transport) -> Vec<u16> {
    let mut out: Vec<u16> = tiles(world, transport).iter().map(|p| p.z).collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn dest(world: &World, pos: TilePos, step: Step) -> Option<TilePos> {
    match step {
        Step::Flat(dx, dy) => {
            let to = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            world.resource::<SpatialIndex>().in_bounds(to).then_some(to)
        }
        Step::Up => world.resource::<ZLevels>().step(pos, true),
        Step::Down => world.resource::<ZLevels>().step(pos, false),
    }
}

/// tg `Check_lift_move`: every module has a level that way; going up into open space only,
/// going down only from open space.
pub fn check_lift_move(world: &World, transport: &Transport, up: bool) -> bool {
    tiles(world, transport).into_iter().all(|pos| {
        let step = if up { Step::Up } else { Step::Down };
        let Some(to) = dest(world, pos, step) else {
            return false;
        };
        if up {
            crate::multiz::is_openspace(world, to)
        } else {
            crate::multiz::is_openspace(world, pos)
        }
    })
}

/// What a module carries off its tile (`add_item_on_transport`).
fn carried(world: &World, e: Entity) -> bool {
    if world.get::<Turf>(e).is_some()
        || world.get::<TransportModule>(e).is_some()
        || world.get::<HiddenUnderTile>(e).is_some()
        || world.get::<crate::ghost::Ghost>(e).is_some()
        || world
            .get::<DmAtom>(e)
            .is_some_and(|a| a.invisibility >= INVISIBILITY_ABSTRACT)
    {
        return false;
    }
    if let Some(obj) = world.get::<DmObj>(e) {
        return !NOT_CARRIED.iter().any(|r| is_under(&obj.path, r));
    }
    world.get::<Health>(e).is_some() || world.get::<crate::doors::DoorFiller>(e).is_some()
}

/// tg `travel(direction)` of every module, then `group_move`: one step of the whole transport.
/// False when a module has nowhere to go.
pub fn move_transport(world: &mut World, transport_e: Entity, step: Step) -> bool {
    let Some(transport) = world.get::<Transport>(transport_e).cloned() else {
        return false;
    };
    let old = tiles(world, &transport);
    let mut moves: HashMap<TilePos, TilePos> = HashMap::new();
    for &p in &old {
        let Some(to) = dest(world, p, step) else {
            return false;
        };
        moves.insert(p, to);
    }
    let old_set: HashSet<TilePos> = old.iter().copied().collect();
    let entering: Vec<TilePos> = moves
        .values()
        .copied()
        .filter(|p| !old_set.contains(p))
        .collect();
    let riders: Vec<(Entity, TilePos)> = old
        .iter()
        .flat_map(|&p| {
            world
                .resource::<SpatialIndex>()
                .on_tile(p)
                .iter()
                .map(move |&e| (e, p))
                .collect::<Vec<_>>()
        })
        .filter(|&(e, _)| carried(world, e))
        .collect();
    collide(world, &transport, step, &entering);
    for &m in &transport.modules {
        if let Some(&p) = world.get::<TilePos>(m)
            && let Some(&to) = moves.get(&p)
        {
            world.entity_mut(m).insert(to);
        }
    }
    for (e, p) in riders {
        // A rider gone or moved off by the hit (a thrown passenger) stays where it is.
        if world.get::<TilePos>(e) != Some(&p) {
            continue;
        }
        if let Some(&to) = moves.get(&p) {
            world.entity_mut(e).insert(to);
            if let Some(&crate::doors::DoorFiller(door)) = world.get::<crate::doors::DoorFiller>(e)
                && let Some(mut d) = world.get_mut::<crate::doors::Door>(door)
            {
                d.filler_moved(to);
            }
        }
    }
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        for p in old_set.iter().chain(moves.values()) {
            atmos.blocker_changed(*p, false);
        }
    }
    true
}

/// tg `travel`: what the transport does to the tiles it enters.
pub fn collide(world: &mut World, transport: &Transport, step: Step, entering: &[TilePos]) {
    for &pos in entering {
        if let Some(turf) = crate::construction::turf_at(world, pos) {
            let path = world.get::<DmTurfBuild>(turf).map(|b| b.path.clone());
            if path.as_deref().is_some_and(|p| is_under(p, WALL)) {
                crate::construction::devastate_wall(world, turf, pos);
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, IMPACT_SOUND, 100.0, true),
                );
            } else if matches!(step, Step::Flat(..))
                && world.get::<ss13_mapload::mining::DmMineral>(turf).is_some()
            {
                crate::mining::rocks::gets_drilled(world, pos, None);
            }
        }
        let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
        for e in here {
            if world.get_entity(e).is_err() {
                continue;
            }
            match step {
                Step::Down => crush(world, transport, e),
                Step::Up => {}
                Step::Flat(dx, dy) => ram(world, transport, e, pos, (dx, dy)),
            }
        }
    }
}

/// Going down onto a living thing.
fn crush(world: &mut World, transport: &Transport, e: Entity) {
    if world.get::<Health>(e).is_none() || world.get::<crate::ghost::Ghost>(e).is_some() {
        return;
    }
    crate::tools::tell_mob(world, e, Span::Danger, "Вас раздавило лифтом!");
    if transport.violent_landing {
        crate::health::gib(world, e);
        return;
    }
    crate::status_effects::apply(world, e, crate::status_effects::Incap::Paralyze, 30.0);
    hurt(
        world,
        e,
        &[
            (BodyZone::Chest, 30.0),
            (BodyZone::Head, 20.0),
            (BodyZone::LLeg, 15.0),
            (BodyZone::RLeg, 15.0),
            (BodyZone::LArm, 15.0),
            (BodyZone::RArm, 15.0),
        ],
    );
}

fn hurt(world: &mut World, e: Entity, zones: &[(BodyZone, f32)]) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(e).cloned(),
        world.get::<Stat>(e).copied(),
    ) else {
        return;
    };
    for &(zone, amount) in zones {
        apply_damage_zone(&mut health, &mut stat, zone, DamageType::Brute, amount);
    }
    world.entity_mut(e).insert((health, stat));
}

/// A far tile from `pos` along `(dx, dy)` turned 45 degrees either way
/// (`get_edge_target_turf(src, turn(dir, pick(45, -45)))`).
fn throw_target(world: &mut World, pos: TilePos, (dx, dy): (i32, i32)) -> TilePos {
    let left = world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(50.0);
    // Turning a cardinal by 45 degrees adds the perpendicular component.
    let (tx, ty) = match (dx, dy, left) {
        (0, _, true) => (-dy, dy),
        (0, _, false) => (dy, dy),
        (_, 0, true) => (dx, dx),
        (_, 0, false) => (dx, -dx),
        _ => (dx, dy),
    };
    TilePos::new(pos.x + tx * 50, pos.y + ty * 50, pos.z)
}

/// Going sideways into something (tg `travel`, the horizontal branch).
fn ram(world: &mut World, transport: &Transport, e: Entity, pos: TilePos, dir: (i32, i32)) {
    let path = world.get::<DmObj>(e).map(|o| o.path.clone());
    if path
        .as_deref()
        .is_some_and(|p| SMASH_IGNORED.iter().any(|r| is_under(p, r)))
    {
        return;
    }
    let layer = world.get::<DmAtom>(e).map_or(0.0, |a| a.layer);
    let lethality = transport.lethality;
    if let Some(path) = path {
        if world.get::<HiddenUnderTile>(e).is_some() || world.get::<DmAtom>(e).is_none() {
            return;
        }
        if is_under(&path, "/obj/structure") && layer > LOW_OBJ_LAYER {
            if world.get::<Anchored>(e).is_some_and(|a| a.0) {
                let text = format!("{} пробивает {}!", name(world, None), name(world, Some(e)));
                let chat = crate::pull::visible_message(
                    world,
                    pos,
                    ss13_shared::chat::ChatChannel::Combat,
                    (Span::Danger, text),
                    &[],
                );
                for message in chat {
                    world.write_message(message);
                }
                crate::obj_damage::deconstruct(world, e, false);
            } else {
                let to = throw_target(world, pos, dir);
                crate::throw::throw_at(world, e, to, (200.0 * lethality) as u32, 4.0 * lethality);
            }
        } else if is_under(&path, "/obj/machinery") && layer >= LOW_OBJ_LAYER {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, "sound/effects/bang.ogg", 50.0, true),
            );
            world.despawn(e);
        }
        return;
    }
    let Some(max) = world.get::<Health>(e).map(|h| h.max) else {
        return;
    };
    if world.get::<crate::ghost::Ghost>(e).is_some() {
        return;
    }
    crate::tools::tell_mob(world, e, Span::Danger, "В вас врезается трамвай!");
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/splat.ogg", 50.0, true),
    );
    let multiplier = max * 0.01;
    let base = {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        if rng.chance(15.0) {
            29.0
        } else {
            rng.range(7, 21) as f32
        }
    };
    let damage = base * lethality * multiplier;
    hurt(
        world,
        e,
        &[
            (BodyZone::Head, 2.0 * damage),
            (BodyZone::Chest, 3.0 * damage),
            (BodyZone::LLeg, 0.5 * damage),
            (BodyZone::RLeg, 0.5 * damage),
            (BodyZone::LArm, 0.5 * damage),
            (BodyZone::RArm, 0.5 * damage),
        ],
    );
    let to = throw_target(world, pos, dir);
    crate::throw::throw_at(world, e, to, (200.0 * lethality) as u32, 4.0 * lethality);
}

fn name(world: &World, e: Option<Entity>) -> String {
    match e {
        None => "Трамвай".to_owned(),
        Some(e) => world
            .get::<ss13_mapload::DmName>(e)
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rams_throw_off_at_45_degrees() {
        let mut world = World::new();
        world.insert_resource(crate::combat::CombatRng::default());
        let pos = TilePos::new(100, 100, 1);
        for _ in 0..8 {
            let t = throw_target(&mut world, pos, (1, 0));
            assert_eq!(t.x - pos.x, 50);
            assert_eq!((t.y - pos.y).abs(), 50);
            let t = throw_target(&mut world, pos, (0, -1));
            assert_eq!(t.y - pos.y, -50);
            assert_eq!((t.x - pos.x).abs(), 50);
        }
    }
}
