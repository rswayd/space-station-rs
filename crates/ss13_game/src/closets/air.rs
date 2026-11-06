//! The own air of sealed closets (server), after tg `/obj/structure/closet` `sealed`,
//! `internal_air`, `return_air()` and `process_internal_air()` (`closets.dm`) and the freezers'
//! override (`closets/secure/freezer.dm`, `crates.dm` `crate/freezer`).
//!
//! - A sealed closet ([`ClosetAir`]: the freezer locker and the freezer crates) starts with a copy
//!   of its tile's air when mapped (`is_maploaded`), else empty.
//! - A mob inside a sealed closet breathes it, feels its pressure and temperature, and exhales
//!   into it (`crate::breath`, through [`holder`], [`remove_air`], [`assume_air`]).
//! - Every `SSobj` fire (2 s): open, the closet air equalizes with its tile's (`equalize`); a
//!   freezer first warms up to the tile's temperature (it never cools the room). Closed, a
//!   freezer's air cools by 4 K per second down to `T0C - 60`.
// shortcut: no environmental body bags' 50 litre air (the mixture model here is turf sized;
// they breathe the tile's), no critter crate tanks; add with non-turf volumes for closets.

use bevy::prelude::*;
use ss13_atmos::gas::{
    CELL_VOLUME, MINIMUM_TEMPERATURE_DELTA_TO_SUSPEND, R_IDEAL_GAS_EQUATION, T0C, TCMB,
};
use ss13_atmos::{Atmos, GasMixture};
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;

use super::{Closet, InCloset};

/// `SSobj` `wait`, the `seconds_per_tick` of `process`.
pub const SSOBJ_WAIT_SECS: f64 = 2.0;
/// Freezers' `cooling_rate_per_second` and `minimum_temperature`.
pub const COOLING_RATE_PER_SECOND: f64 = 4.0;
pub const MINIMUM_TEMPERATURE: f64 = T0C - 60.0;

/// Types with `sealed = TRUE`; all of them are freezers.
const SEALED: [&str; 2] = [
    "/obj/structure/closet/secure_closet/freezer",
    "/obj/structure/closet/crate/freezer",
];

/// tg `internal_air` of a sealed closet; `freezer`: the freezers' `process_internal_air`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ClosetAir {
    pub air: GasMixture,
    pub freezer: bool,
}

/// Whether closets of `path` are sealed.
pub fn sealed(path: &str) -> bool {
    SEALED.iter().any(|root| is_under(path, root))
}

/// tg `Initialize`: a copy of the tile's air for a mapped closet, else a fresh (empty) mixture.
/// A level without simulated air is standard station air.
pub fn initial_air(world: &World, pos: Option<TilePos>, mapped: bool) -> GasMixture {
    let Some(pos) = pos.filter(|_| mapped) else {
        return GasMixture::vacuum();
    };
    match world.get_resource::<Atmos>() {
        Some(atmos) if atmos.level(pos.z).is_some() => atmos.air(pos).cloned().unwrap_or_default(),
        _ => crate::breath::station_air(),
    }
}

/// The sealed closet `mob` is in, whose air it breathes (tg `loc.return_air()`).
pub fn holder(world: &World, mob: Entity) -> Option<Entity> {
    let closet = world.get::<InCloset>(mob)?.closet;
    world.get::<ClosetAir>(closet).map(|_| closet)
}

/// tg `remove_air(amount)` of the closet's air.
pub fn remove_air(world: &mut World, closet: Entity, moles: f64) -> Option<GasMixture> {
    world.get_mut::<ClosetAir>(closet)?.air.remove(moles)
}

/// tg `assume_air`: `giver` into the closet's air.
pub fn assume_air(world: &mut World, closet: Entity, giver: &GasMixture) {
    if let Some(mut air) = world.get_mut::<ClosetAir>(closet) {
        air.air.merge_raw(giver);
    }
}

/// tg `/datum/gas_mixture/equalize` of two mixtures of the same volume (the closet's air is a
/// copy of a turf's): one temperature by heat capacity, every gas halved. True when anything
/// moved.
pub fn equalize(ours: &mut GasMixture, turf: &mut GasMixture) -> bool {
    let mut moved = false;
    if (ours.temperature - turf.temperature).abs() > MINIMUM_TEMPERATURE_DELTA_TO_SUSPEND {
        let (self_heat, other_heat) = (ours.heat_capacity_raw(), turf.heat_capacity());
        if self_heat + other_heat > 0.0 {
            moved = true;
            let t = (ours.temperature * self_heat + turf.temperature * other_heat)
                / (self_heat + other_heat);
            ours.temperature = t;
            turf.temperature = t;
        }
    }
    let threshold = 0.1 / (R_IDEAL_GAS_EQUATION * ours.temperature.max(TCMB));
    for (m, o) in ours.moles.iter_mut().zip(turf.moles.iter_mut()) {
        if ((*m - *o) / CELL_VOLUME).abs() > threshold {
            moved = true;
            let half = (*m + *o) / 2.0;
            *m = half;
            *o = half;
        }
    }
    ours.garbage_collect();
    turf.garbage_collect();
    moved
}

/// One `process_internal_air(seconds_per_tick)` of a closet on `pos`.
pub fn process_one(world: &mut World, closet: Entity, pos: TilePos, spt: f64) {
    let Some(opened) = world.get::<Closet>(closet).map(|c| c.opened) else {
        return;
    };
    let Some(mut state) = world.get::<ClosetAir>(closet).cloned() else {
        return;
    };
    if opened {
        let simulated = world
            .get_resource::<Atmos>()
            .is_some_and(|a| a.level(pos.z).is_some());
        if simulated {
            let mut atmos = world.resource_mut::<Atmos>();
            let edited = atmos.modify_air(pos, |turf| {
                if state.freezer {
                    // The internal air won't cool down the external air when opened.
                    state.air.temperature = state.air.temperature.max(turf.temperature);
                }
                equalize(&mut state.air, turf);
            });
            // Space and walls: their air does not change (space stays a vacuum).
            if edited.is_none()
                && let Some(mut turf) = atmos.air(pos).cloned()
            {
                if state.freezer {
                    state.air.temperature = state.air.temperature.max(turf.temperature);
                }
                equalize(&mut state.air, &mut turf);
            }
        } else {
            let mut turf = crate::breath::station_air();
            if state.freezer {
                state.air.temperature = state.air.temperature.max(turf.temperature);
            }
            equalize(&mut state.air, &mut turf);
        }
    } else if state.freezer && state.air.temperature > MINIMUM_TEMPERATURE {
        let drop = (COOLING_RATE_PER_SECOND * spt).min(state.air.temperature - MINIMUM_TEMPERATURE);
        state.air.temperature -= drop;
    }
    if let Some(mut air) = world.get_mut::<ClosetAir>(closet) {
        air.set_if_neq(state);
    }
}

/// `SSobj` fires over the sealed closets.
// Cost: every 2 s one pass over the sealed closets (the freezers of a map, a few dozen).
pub(super) fn process_closet_air(world: &mut World, mut clock: Local<f64>) {
    *clock += world.resource::<Time>().delta_secs_f64();
    if *clock < SSOBJ_WAIT_SECS {
        return;
    }
    *clock -= SSOBJ_WAIT_SECS;
    let closets: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &TilePos), (With<ClosetAir>, With<Closet>)>()
        .iter(world)
        .map(|(e, p)| (e, *p))
        .collect();
    for (closet, pos) in closets {
        process_one(world, closet, pos, SSOBJ_WAIT_SECS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freezers_cool_closed_and_equalize_open() {
        let mut world = World::new();
        let pos = TilePos::new(1, 1, 1);
        let closet = world
            .spawn((
                Closet::from_dm(&Default::default(), &Default::default(), None),
                ClosetAir {
                    air: crate::breath::station_air(),
                    freezer: true,
                },
                pos,
            ))
            .id();
        // Closed: 4 K per second, down to the minimum.
        process_one(&mut world, closet, pos, 2.0);
        let t = world.get::<ClosetAir>(closet).unwrap().air.temperature;
        assert!((t - (293.15 - 8.0)).abs() < 1e-6, "{t}");
        for _ in 0..100 {
            process_one(&mut world, closet, pos, 2.0);
        }
        let t = world.get::<ClosetAir>(closet).unwrap().air.temperature;
        assert!((t - MINIMUM_TEMPERATURE).abs() < 1e-6, "{t}");
        // Open (no simulated level: station air around): back to room temperature.
        world.get_mut::<Closet>(closet).unwrap().opened = true;
        process_one(&mut world, closet, pos, 2.0);
        let t = world.get::<ClosetAir>(closet).unwrap().air.temperature;
        assert!((t - 293.15).abs() < 1e-6, "{t}");
    }

    #[test]
    fn mobs_inside_breathe_the_closet_air() {
        let mut world = World::new();
        let closet = world
            .spawn(ClosetAir {
                air: crate::breath::station_air(),
                freezer: false,
            })
            .id();
        let mob = world.spawn_empty().id();
        assert_eq!(holder(&world, mob), None);
        let full = world.get::<ClosetAir>(closet).unwrap().air.total_moles();
        let breath = remove_air(&mut world, closet, 1.0).unwrap();
        let left = world.get::<ClosetAir>(closet).unwrap().air.total_moles();
        assert!((full - left - 1.0).abs() < 1e-6);
        assume_air(&mut world, closet, &breath);
        let back = world.get::<ClosetAir>(closet).unwrap().air.total_moles();
        assert!((back - full).abs() < 1e-6);
        // Empty closet air vs a full turf: halves.
        let mut ours = GasMixture::vacuum();
        let mut turf = crate::breath::station_air();
        let total = turf.total_moles();
        assert!(equalize(&mut ours, &mut turf));
        assert!((ours.total_moles() - total / 2.0).abs() < 1e-6);
        assert!(sealed("/obj/structure/closet/crate/freezer/blood"));
        assert!(!sealed("/obj/structure/closet/crate"));
    }
}
