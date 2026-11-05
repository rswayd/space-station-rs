//! Electrical Storm (`code/modules/events/electrical_storm.dm`): every APC within 25 tiles of a
//! random station turf overloads its lighting, which breaks every light tube of its area
//! (`/obj/machinery/power/apc/proc/overload_lighting`).

use bevy::prelude::*;
use ss13_shared::TilePos;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::power::{Apc, AreaMembers};

pub const NAME: &str = "Electrical Storm";
/// `lightsoutRange`.
const LIGHTS_OUT_RANGE: u32 = 25;
/// `overload_lighting`: `cell.use(0.02 * STANDARD_BATTERY_CHARGE)` (1 MJ).
const OVERLOAD_JOULES: f64 = 0.02 * 1_000_000.0;

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ElectricalStorm)
}

struct ElectricalStorm;

impl RoundEvent for ElectricalStorm {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        super::priority_announce(
            world,
            "Электрическая буря",
            "На борту станции зафиксирована электрическая буря. Пожалуйста, устраните \
             потенциальные перегрузки электросетей.",
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        // `lightsoutAmount = 1`.
        if let Some(centre) = random_station_turf(world) {
            storm(world, centre);
        }
    }
}

/// The APCs within range of `centre` (tg `get_dist`) overload their lighting.
pub fn storm(world: &mut World, centre: TilePos) {
    let mut q = world.query::<(Entity, &TilePos, &Apc)>();
    let apcs: Vec<Entity> = q
        .iter(world)
        .filter(|(_, pos, _)| pos.chebyshev(centre).is_some_and(|d| d <= LIGHTS_OUT_RANGE))
        .map(|(e, ..)| e)
        .collect();
    for apc in apcs {
        overload_lighting(world, apc);
    }
}

/// tg `overload_lighting()` + `break_lights()` (also the malfunctioning AI's blackout,
/// `crate::silicon::malf_modules`).
pub(crate) fn overload_lighting(world: &mut World, apc: Entity) {
    let Some(mut state) = world.get_mut::<Apc>(apc) else {
        return;
    };
    if !state.operating || state.shorted {
        return;
    }
    let paid = state
        .cell
        .as_mut()
        .is_some_and(|c| c.take(OVERLOAD_JOULES, false) > 0.0);
    let area = state.area;
    if !paid {
        return;
    }
    let lights = world
        .get::<AreaMembers>(area)
        .map(|m| m.lights.clone())
        .unwrap_or_default();
    for light in lights {
        crate::power::break_light_tube(world, light);
    }
}

/// tg `get_safe_random_station_turf_equal_weight()`: a random `/area/station` area of a station
/// level, then a random open, unblocked turf in it.
// shortcut: "safe" is open and without dense things; no air or temperature check. One scan of
// the station levels per call (an event every few minutes): ~65k tiles on MetaStation.
pub fn random_station_turf(world: &mut World) -> Option<TilePos> {
    let levels = station_levels(world);
    let index = world.resource::<ss13_world::SpatialIndex>();
    let areas = world.resource::<ss13_mapload::Areas>();
    let mut by_area: Vec<(Entity, Vec<TilePos>)> = Vec::new();
    for z in levels {
        let Some((w, h)) = index.level_size(z) else {
            continue;
        };
        for y in 1..=h as i32 {
            for x in 1..=w as i32 {
                let pos = TilePos::new(x, y, z);
                let Some(area) = areas.area_at(pos) else {
                    continue;
                };
                let station = world
                    .get::<ss13_mapload::DmPower>(area)
                    .is_some_and(|p| crate::obj_damage::is_under(&p.path, "/area/station"));
                if !station || !index.dense_on_tile(pos).is_empty() || !is_floor(world, pos) {
                    continue;
                }
                match by_area.iter_mut().find(|(a, _)| *a == area) {
                    Some((_, tiles)) => tiles.push(pos),
                    None => by_area.push((area, vec![pos])),
                }
            }
        }
    }
    let mut rng = world.resource_mut::<EventRng>();
    let (_, tiles) = rng.pick(&by_area)?;
    rng.pick(tiles).copied()
}

pub(super) fn is_floor(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| crate::obj_damage::is_under(&p, "/turf/open/floor"))
}
