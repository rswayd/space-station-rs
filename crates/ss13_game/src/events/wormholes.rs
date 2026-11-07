//! Wormholes (`code/modules/events/wormholes.dm`): 400 wormholes (`/obj/effect/portal/wormhole`)
//! on random station floors, all moved to new floors every 3 fires, gone when the event ends
//! (40-80 fires). Whatever unanchored walks into one (they are dense: `Bumped`) comes out next
//! to a random wormhole (`do_teleport(M, hard_target, 1)`).
//!
//! Cost: 400 entities moved every 6 s; a bump looks the wormholes up once.
// shortcut: thrown things and projectiles do not enter them (no `Bumped` from throws), mechs
// are not let through anchored.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmName, IconIndex};
use ss13_net::server::Networked;
use ss13_shared::appearance::{Layer, MouseOpacity};
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_world::{Bumped, Density, SpatialIndex};

use super::controller::{EventRng, EventState, RoundEvent};

pub const NAME: &str = "Wormholes";
/// `number_of_wormholes`, `shift_frequency`.
const NUMBER: usize = 400;
const SHIFT_FREQUENCY: u32 = 3;
const ICON: &str = "icons/obj/anomaly.dmi";
const ICON_STATE: &str = "anom";
/// `/obj/effect/portal`: `HIGH_OBJ_LAYER` on `GAME_PLANE`.
const HIGH_OBJ_LAYER: f32 = 3.5;
const GAME_PLANE: i16 = -4;
/// `SFX_PORTAL_CREATED`, `SFX_PORTAL_ENTER`; `SILENCED_SOUND_EXTRARANGE`,
/// `SHORT_RANGE_SOUND_EXTRARANGE`.
const SFX_CREATED: &str = "portal_created";
const SFX_ENTER: &str = "portal_enter";
const SILENCED_EXTRARANGE: f32 = -10.0;
const SHORT_RANGE_EXTRARANGE: f32 = -7.0;

/// `/obj/effect/portal/wormhole`.
#[derive(Component, Debug)]
pub struct Wormhole;

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Wormholes {
        holes: Vec::new(),
        turfs: Vec::new(),
    })
}

struct Wormholes {
    holes: Vec<Entity>,
    /// `pick_turfs`.
    turfs: Vec<TilePos>,
}

impl RoundEvent for Wormholes {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        let rng = &mut world.resource_mut::<EventRng>().0;
        ev.announce_when = rng.range(0, 20) as u32;
        ev.end_when = rng.range(40, 80) as u32;
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        self.turfs = super::station_tiles(world)
            .into_iter()
            .map(|(_, pos)| pos)
            .filter(|&pos| super::electrical_storm::is_floor(world, pos))
            .collect();
        for _ in 0..NUMBER {
            let Some(&pos) = world.resource_mut::<EventRng>().pick(&self.turfs) else {
                return;
            };
            self.holes.push(spawn_wormhole(world, pos));
            created_sound(world, pos);
        }
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        super::priority_announce_sound(
            world,
            "Обнаружена аномалия",
            "Зафиксированы пространственно-временные аномалии на борту станции. Дополнительная \
             информация отсутствует.",
            crate::announce::ANNOUNCER_SPANOMALIES,
        );
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        if !ev.active_for.is_multiple_of(SHIFT_FREQUENCY) {
            return;
        }
        for &hole in &self.holes {
            let Some(&pos) = world.resource_mut::<EventRng>().pick(&self.turfs) else {
                return;
            };
            if world.get_entity(hole).is_ok() {
                world.entity_mut(hole).insert(pos);
                created_sound(world, pos);
            }
        }
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        for hole in std::mem::take(&mut self.holes) {
            if world.get_entity(hole).is_ok() {
                world.despawn(hole);
            }
        }
    }
}

fn created_sound(world: &mut World, pos: TilePos) {
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, SFX_CREATED, 20.0, true).extrarange(SILENCED_EXTRARANGE),
    );
}

/// `new /obj/effect/portal/wormhole(pos)`.
pub fn spawn_wormhole(world: &mut World, pos: TilePos) -> Entity {
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ICON, ICON_STATE))
        .unwrap_or(IconStateId(u32::MAX));
    world
        .spawn((
            Name::new("/obj/effect/portal/wormhole"),
            DmName {
                name: "wormhole".to_owned(),
                desc: "It looks highly unstable; It could close at any moment.".to_owned(),
            },
            pos,
            Appearance {
                icon_state: icon,
                layer: Layer(HIGH_OBJ_LAYER),
                plane: GAME_PLANE,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                overlays: vec![],
                mouse_opacity: MouseOpacity::Icon,
            },
            Density,
            Networked,
            Wormhole,
        ))
        .id()
}

/// `/obj/effect/portal/Bumped` -> `wormhole/teleport(M)`.
pub(super) fn enter_wormholes(world: &mut World, mut bumps: Local<MessageCursor<Bumped>>) {
    let pending: Vec<Bumped> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    for Bumped {
        entity, obstacle, ..
    } in pending
    {
        if world.get::<Wormhole>(obstacle).is_some() {
            teleport(world, entity);
        }
    }
}

/// `wormhole/teleport(M)`: effects and anchored things stay; the rest goes next to a random
/// wormhole.
pub fn teleport(world: &mut World, mover: Entity) -> bool {
    let anchored = world.get::<crate::obj_damage::Anchored>(mover).map_or_else(
        || {
            world
                .get::<ss13_mapload::DmObj>(mover)
                .is_some_and(|o| o.anchored)
        },
        |a| a.0,
    );
    let Some(&start) = world.get::<TilePos>(mover) else {
        return false;
    };
    if anchored || world.get::<Wormhole>(mover).is_some() {
        return false;
    }
    let mut q = world.query_filtered::<&TilePos, With<Wormhole>>();
    let holes: Vec<TilePos> = q.iter(world).copied().collect();
    let Some(&target) = world.resource_mut::<EventRng>().pick(&holes) else {
        return false;
    };
    let Some(dest) = teleport_turf(world, target, 1) else {
        return false;
    };
    world.entity_mut(mover).insert(dest);
    for pos in [start, dest] {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SFX_ENTER, 50.0, true)
                .extrarange(SHORT_RANGE_EXTRARANGE),
        );
    }
    true
}

/// `do_teleport(center, precision)`'s landing: a random free tile within `precision` of
/// `center` (`get_teleport_turfs`: in bounds, nothing dense).
pub fn teleport_turf(world: &mut World, center: TilePos, precision: i32) -> Option<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let mut tiles = Vec::new();
    for dy in -precision..=precision {
        for dx in -precision..=precision {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            if index.in_bounds(pos) && index.dense_on_tile(pos).is_empty() {
                tiles.push(pos);
            }
        }
    }
    world.resource_mut::<EventRng>().pick(&tiles).copied()
}
