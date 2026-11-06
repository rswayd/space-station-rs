//! Stray Cargo Pod (`code/modules/events/stray_cargo.dm`, BandaStation
//! `modular_bandastation/balance/code/events.dm`): 20 to 40 fires after the announcement chance
//! (75%), a plain supply pod with a random non-special supply pack's crate, unlocked, lands on
//! a free tile of a random station area away from engineering, the AI chambers and shuttles,
//! and explodes on landing (`crate::cargo::supplypod::launch_stray`).
// shortcut: no admin setup (aim, chosen pack), no ghost alert; goodies (crateless packs) are
// left out instead of coming in an empty briefcase; areas without a free tile are left out of
// the pick instead of crashing the event. "Stray Syndicate Cargo Pod" is not ported (no
// uplink item `surplus` in the catalog for its `fill()`).

use bevy::prelude::*;
use ss13_mapload::cargo::order_flags;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::cargo::supply::SupplyPacks;
use crate::cargo::supplypod::{PodCargo, launch_stray};

pub const NAME: &str = "Stray Cargo Pod";

/// `safe_area_types`: places that should not explode.
const SAFE_AREAS: [&str; 4] = [
    "/area/station/ai/satellite/chamber",
    "/area/station/ai/upload/chamber",
    "/area/station/engineering",
    "/area/shuttle",
];
/// `unsafe_area_subtypes`: what of them should after all.
const UNSAFE_SUBTYPE: &str = "/area/station/engineering/break_room";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(StrayCargo { area: None })
}

struct StrayCargo {
    /// `impact_area` and its free tiles.
    area: Option<(Entity, Vec<TilePos>)>,
}

impl RoundEvent for StrayCargo {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.announce_chance = 75.0;
        ev.start_when = world.resource_mut::<EventRng>().0.range(20, 40) as u32;
        self.area = find_event_area(world);
        if self.area.is_none() {
            warn!("Stray Cargo Pod: no valid area for the pod");
            ev.killed = true;
        }
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        let area = if fake {
            find_event_area(world).map(|(a, _)| a)
        } else {
            self.area.as_ref().map(|(a, _)| *a)
        };
        let name = area
            .map(|a| crate::shuttle::area_name(world, a))
            .unwrap_or_default();
        let text = format!(
            "Бродячая грузовая капсула обнаружена на сканерах дальнего действия. Ожидаемое место \
             падения: {name}."
        );
        super::priority_announce(world, "Предупреждение о столкновении", &text);
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let Some((_, tiles)) = self.area.as_ref() else {
            return;
        };
        let packs = stray_packs(world);
        let (landing, pack) = {
            let mut rng = world.resource_mut::<EventRng>();
            (rng.pick(tiles).copied(), rng.pick(&packs).copied())
        };
        let (Some(landing), Some(pack)) = (landing, pack) else {
            return;
        };
        launch_stray(world, landing, PodCargo::Pack(pack));
    }
}

/// `stray_spawnable_supply_packs`: every pack but `ORDER_SPECIAL` ones, that a crate can hold
/// here (a crate type, contents, no own `fill()`, no mobs).
pub fn stray_packs(world: &World) -> Vec<usize> {
    let Some(packs) = world.get_resource::<SupplyPacks>() else {
        return Vec::new();
    };
    packs
        .packs
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            !p.has(order_flags::SPECIAL)
                && !p.custom_fill
                && p.crate_type.is_some()
                && !p.contains.is_empty()
                && !p.contains.iter().any(|(t, _)| is_under(t, "/mob"))
        })
        .map(|(i, _)| i)
        .collect()
}

/// `find_event_area`: a station area outside [`SAFE_AREAS`] (but [`UNSAFE_SUBTYPE`]), with its
/// non-dense tiles (`get_landing_zone`).
fn find_event_area(world: &mut World) -> Option<(Entity, Vec<TilePos>)> {
    let mut tiles: std::collections::HashMap<Entity, Vec<TilePos>> = default();
    for (area, pos) in super::station_tiles(world) {
        let allowed = super::area_type(world, area).is_some_and(allowed_area);
        if allowed && !dense_turf(world, pos) {
            tiles.entry(area).or_default().push(pos);
        }
    }
    let mut by_area: Vec<(Entity, Vec<TilePos>)> = tiles.into_iter().collect();
    by_area.sort_by_key(|(a, _)| *a);
    let i = {
        let rng = &mut world.resource_mut::<EventRng>().0;
        if by_area.is_empty() {
            return None;
        }
        rng.range(0, by_area.len() as i64 - 1) as usize
    };
    Some(by_area.swap_remove(i))
}

/// The area type is in `allowed_areas`.
pub fn allowed_area(path: &str) -> bool {
    is_under(path, "/area/station")
        && (is_under(path, UNSAFE_SUBTYPE) || !SAFE_AREAS.iter().any(|s| is_under(path, s)))
}

/// `T.density` of the turf at `pos` (walls).
fn dense_turf(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .is_none_or(|t| world.get::<ss13_world::Density>(t).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engineering_and_ai_chambers_are_safe() {
        assert!(allowed_area("/area/station/hallway/primary/central"));
        assert!(allowed_area("/area/station/engineering/break_room"));
        assert!(!allowed_area("/area/station/engineering/main"));
        assert!(!allowed_area("/area/station/ai/upload/chamber"));
        assert!(!allowed_area("/area/shuttle/arrival"));
        assert!(!allowed_area("/area/space"));
    }
}
