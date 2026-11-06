//! Hot air damaging things (server): tg `/datum/element/atmos_sensitive` and
//! `SSair.process_atoms` (`code/datums/elements/atmos_sensitive.dm`).
//!
//! A processed turf whose air is hot ([`Atmos::drain_exposed_turfs`], tg `COMSIG_TURF_EXPOSE`)
//! starts every sensitive atom on it whose `should_atmos_process` holds; each fire every
//! started atom checks again against its turf's air and, while it holds, takes its
//! `atmos_expose`, else stops. Ported: windows, grilles, windoors, canisters, APCs, lights,
//! spider webs and turfs themselves (they burn and melt from their `heat_capacity` in K up; a
//! solid turf heated through superconduction is exposed at once, as tg's
//! `/turf/temperature_expose`).
// shortcut: an atom moved onto a hot turf starts on that turf's next processing, not at once
// (tg `react_to_move`), and only these kinds are sensitive; add the rest with their objects.

use bevy::prelude::*;
use ss13_atmos::{Atmos, TileKind, TurfAir};
use ss13_mapload::DmObj;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

use crate::combat::CombatRng;
use crate::construction::scrape_away;
use crate::health::DamageType;
use crate::obj_damage::{is_under, take_damage};
use crate::power::{LightFixture, break_light_tube};

const T0C: f64 = 273.15;
/// `/turf/open` air volume (`CELL_VOLUME`), what `air.return_volume()` gives a window.
const CELL_VOLUME: f64 = 2500.0;
/// `canister.dm` `TEMPERATURE_RESISTANCE`.
const CANISTER_TEMPERATURE_RESISTANCE: f64 = 1000.0 + T0C;

/// An atom with tg's `atmos_sensitive` element, by what it does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sensitive {
    /// `/obj/structure/window`: over `T0C + heat_resistance`, `round(volume / 100)` burn.
    Window { heat_resistance: f64 },
    /// `/obj/structure/grille`: over `T0C + 1500` while whole, 1 burn.
    Grille,
    /// `/obj/machinery/door/window`: over `T0C + 1600` (reinforced) or `+ 800`,
    /// `round(T / 200)` burn.
    Windoor { reinforced: bool },
    /// Over `TEMPERATURE_RESISTANCE`, 5 burn (no shielding is ported).
    Canister,
    /// `/obj/machinery/power/apc`: over 2000 K, `min(T / 100, 10)` burn.
    Apc,
    /// `/obj/machinery/light`: over 673 K, breaks with `prob(T - 673)`.
    Light,
    /// `/obj/structure/spider` (webs): over 350 K, 5 burn.
    SpiderWeb,
    /// A turf (`/turf/should_atmos_process`): from its `heat_capacity` in K, or while it is
    /// `to_be_destroyed`.
    Turf { heat_capacity: f64 },
}

/// tg `to_be_destroyed` and `max_fire_temperature_sustained` of a turf.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct TurfMelting {
    pub to_be_destroyed: bool,
    pub max_fire_temperature_sustained: f64,
}

impl Sensitive {
    /// The kind of `e`, if it is sensitive.
    pub fn of(world: &World, e: Entity) -> Option<Self> {
        if world.get::<LightFixture>(e).is_some() {
            return Some(Self::Light);
        }
        if let Some(air) = world.get::<TurfAir>(e) {
            return Some(Self::Turf {
                heat_capacity: air.thermal.heat_capacity,
            });
        }
        let obj = world.get::<DmObj>(e)?;
        let path = obj.path.as_str();
        Some(if is_under(path, "/obj/structure/window") {
            Self::Window {
                heat_resistance: f64::from(obj.heat_resistance),
            }
        } else if is_under(path, "/obj/structure/grille") {
            Self::Grille
        } else if is_under(path, "/obj/machinery/door/window") {
            Self::Windoor {
                reinforced: obj.reinf,
            }
        } else if is_under(path, "/obj/machinery/portable_atmospherics/canister") {
            Self::Canister
        } else if is_under(path, "/obj/machinery/power/apc") {
            Self::Apc
        } else if is_under(path, crate::effects::webs::SPIDER_STRUCTURE) {
            Self::SpiderWeb
        } else {
            return None;
        })
    }

    /// `should_atmos_process(air, exposed_temperature)`.
    pub fn should_process(self, world: &World, e: Entity, temperature: f64) -> bool {
        match self {
            Self::Window { heat_resistance } => temperature > T0C + heat_resistance,
            Self::Grille => {
                temperature > T0C + 1500.0 && !world.get::<DmObj>(e).is_some_and(|o| o.broken)
            }
            Self::Windoor { reinforced } => {
                temperature > T0C + if reinforced { 1600.0 } else { 800.0 }
            }
            Self::Canister => {
                temperature > CANISTER_TEMPERATURE_RESISTANCE
                    && !world
                        .get::<crate::atmos_machinery::portable::Canister>(e)
                        .is_some_and(|c| c.shielding)
            }
            Self::Apc => temperature > 2000.0,
            Self::Light => temperature > 673.0,
            Self::SpiderWeb => temperature > 350.0,
            Self::Turf { heat_capacity } => {
                temperature >= heat_capacity
                    || world
                        .get::<TurfMelting>(e)
                        .is_some_and(|m| m.to_be_destroyed)
            }
        }
    }

    /// `atmos_expose(air, exposed_temperature)`.
    pub fn expose(self, world: &mut World, e: Entity, temperature: f64) {
        let burn = |world: &mut World, amount: f64| {
            take_damage(world, e, amount as f32, DamageType::Burn);
        };
        match self {
            Self::Window { .. } => burn(world, (CELL_VOLUME / 100.0).floor()),
            Self::Grille => burn(world, 1.0),
            Self::Windoor { .. } => burn(world, (temperature / 200.0).floor()),
            Self::Canister => burn(world, 5.0),
            // `take_damage(5, BURN, 0, 0)`: no damage flag, so no melee armor.
            Self::SpiderWeb => {
                crate::obj_damage::take_damage_flagless(world, e, 5.0);
            }
            Self::Apc => burn(world, (temperature / 100.0).min(10.0)),
            Self::Light => {
                let chance = (temperature - 673.0).max(0.0) as f32;
                if world.resource_mut::<CombatRng>().chance(chance) {
                    break_light_tube(world, e);
                }
            }
            Self::Turf { heat_capacity } => turf_atmos_expose(world, e, heat_capacity, temperature),
        }
    }
}

/// `/turf/atmos_expose` and `burn_turf`: hot enough marks it, the hottest fire it sustains
/// ramps up by a quarter of its heat capacity a time, and each exposure it burns its floor and
/// melts (`ScrapeAway`, air kept) with `prob(max_fire_temperature_sustained / heat_capacity * 8)`.
fn turf_atmos_expose(world: &mut World, turf: Entity, heat_capacity: f64, temperature: f64) {
    let Some(&pos) = world.get::<TilePos>(turf) else {
        return;
    };
    let mut melting = world.get::<TurfMelting>(turf).copied().unwrap_or_default();
    if temperature >= heat_capacity {
        melting.to_be_destroyed = true;
    }
    if melting.to_be_destroyed && temperature >= melting.max_fire_temperature_sustained {
        melting.max_fire_temperature_sustained =
            temperature.min(melting.max_fire_temperature_sustained + heat_capacity / 4.0);
    }
    if !melting.to_be_destroyed {
        world.entity_mut(turf).insert(melting);
        return;
    }
    crate::turf_damage::burn_tile(world, pos);
    let chance = if heat_capacity > 0.0 {
        melting.max_fire_temperature_sustained / heat_capacity * 8.0
    } else {
        100.0
    };
    if world.resource_mut::<CombatRng>().chance(chance as f32) {
        // `Melt()`: the new turf starts over.
        scrape_away(world, pos, true);
        return;
    }
    melting.to_be_destroyed = false;
    if let Ok(mut entity) = world.get_entity_mut(turf) {
        entity.insert(melting);
    }
}

/// `SSair.atom_process`, in insertion order.
#[derive(Resource, Default, Debug)]
pub struct AtomProcess(pub Vec<Entity>);

/// `check_atmos_process` of the sensitive atoms on each hot turf, then `process_atoms`.
pub fn process_atoms(world: &mut World) {
    let Some(exposed) = world
        .get_resource_mut::<Atmos>()
        .map(|mut a| a.drain_exposed_turfs())
    else {
        return;
    };
    for (pos, temperature) in exposed {
        let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
        for e in on_tile {
            let Some(kind) = Sensitive::of(world, e) else {
                continue;
            };
            // A closed turf is exposed at once (`/turf/temperature_expose`), not processed.
            if world
                .get::<TurfAir>(e)
                .is_some_and(|a| a.kind == TileKind::Blocked)
            {
                if kind.should_process(world, e, temperature) {
                    kind.expose(world, e, temperature);
                }
                continue;
            }
            let listed = world.resource::<AtomProcess>().0.contains(&e);
            if kind.should_process(world, e, temperature) {
                if !listed {
                    world.resource_mut::<AtomProcess>().0.push(e);
                }
            } else if listed {
                world.resource_mut::<AtomProcess>().0.retain(|&x| x != e);
            }
        }
    }
    let run = world.resource::<AtomProcess>().0.clone();
    for &e in run.iter().rev() {
        let state = world
            .get_entity(e)
            .ok()
            .and_then(|entity| entity.get::<TilePos>().copied())
            .zip(Sensitive::of(world, e));
        let temperature = state.and_then(|(pos, _)| world.resource::<Atmos>().temperature(pos));
        match (state, temperature) {
            (Some((_, kind)), Some(t)) if kind.should_process(world, e, t) => {
                kind.expose(world, e, t);
            }
            // Gone, off an open turf (a wall, a locker) or cooled down: `atmos_end`.
            _ => world.resource_mut::<AtomProcess>().0.retain(|&x| x != e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tg's thresholds: strictly over for atoms, from `heat_capacity` up for turfs.
    #[test]
    fn thresholds_follow_tg() {
        let mut world = World::new();
        let e = world.spawn_empty().id();
        let cases = [
            (
                Sensitive::Window {
                    heat_resistance: 800.0,
                },
                1073.15,
            ),
            (Sensitive::Windoor { reinforced: false }, 1073.15),
            (Sensitive::Windoor { reinforced: true }, 1873.15),
            (Sensitive::Grille, 1773.15),
            (Sensitive::Canister, 1273.15),
            (Sensitive::Apc, 2000.0),
            (Sensitive::Light, 673.0),
        ];
        for (kind, at) in cases {
            assert!(!kind.should_process(&world, e, at), "{kind:?} at {at}");
            assert!(
                kind.should_process(&world, e, at + 0.01),
                "{kind:?} over {at}"
            );
        }
        let floor = Sensitive::Turf {
            heat_capacity: 20_000.0,
        };
        assert!(floor.should_process(&world, e, 20_000.0));
        assert!(!floor.should_process(&world, e, 19_999.0));
        world.entity_mut(e).insert(TurfMelting {
            to_be_destroyed: true,
            ..default()
        });
        assert!(
            floor.should_process(&world, e, 300.0),
            "marked turfs keep processing"
        );
        // Everything ported starts above what atmos reports from.
        assert!(
            cases
                .iter()
                .all(|&(_, at)| at > ss13_atmos::level::EXPOSE_REPORT_MIN)
        );
    }
}
