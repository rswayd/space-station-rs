//! The pipe scrubber (`portable/pipe_scrubber.dm`): a portable machine whose 200 L buffer
//! ([`super::PortableAtmos`], wrenched to a port like any portable) is filtered into an internal
//! canister, or the canister pumped back into the buffer up to `PUMP_MAX_PRESSURE`.
//!
//! - `process_atmos`: the buffer over its limits damages the machine and nothing is pumped
//!   that fire; on, it scrubs (`direction` in) or dumps (out).
//! - The internal canister (`internal_tank`) is processed too: its gas reacts unless its
//!   suppression is on, and over its limits it takes damage; once broken
//!   (`integrity_failure`) the whole machine is deconstructed (`COMSIG_ATOM_BREAK`).
//! - Destroyed, both mixtures go onto the tile.
//! - No tank goes in (`replace_tank` refuses), alt-click does nothing.
// shortcut: the internal canister is a mixture with a canister's limits and integrity, not a
// canister object: it does not leak when damaged (`gas_leaker`), does not explode when it
// breaks (`volatile_gas_storage`) and a welder does not mend it (`internal_tank.welder_act`).

use bevy::prelude::*;
use ss13_atmos::GasMixture;
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::pipenet::air::PipeAir;
use ss13_atmos::reactions::{Exposures, Holder};
use ss13_shared::TilePos;

use super::PortableAtmos;
use super::damage::{PRESSURE_LIMIT, TEMP_LIMIT, atmos_damage};
use super::portable::PUMP_MAX_PRESSURE;
use crate::health::DamageType;

pub const PIPE_SCRUBBER: &str = "/obj/machinery/portable_atmospherics/pipe_scrubber";
/// The internal `/obj/machinery/portable_atmospherics/canister`: `volume`, `max_integrity`,
/// `integrity_failure`.
const TANK_VOLUME: f64 = 2000.0;
const TANK_INTEGRITY: f32 = 300.0;
const TANK_INTEGRITY_FAILURE: f32 = 0.4;

/// `internal_tank`: its gas, limits, integrity and crystal.
#[derive(Clone, Debug, PartialEq)]
pub struct InternalTank {
    pub air: PipeAir,
    /// `pressure_limit` (`* (1 + 0.2 * rand())` in its `Initialize`), `temp_limit`.
    pub pressure_limit: f64,
    pub temp_limit: f64,
    pub integrity: f32,
    /// `nob_crystal_inserted`, `suppress_reactions` of the canister.
    pub crystal: bool,
    pub suppress: bool,
}

impl InternalTank {
    pub fn new(quality: f64) -> Self {
        let mut air = PipeAir::new(TANK_VOLUME);
        air.gas.temperature = ss13_atmos::gas::T20C;
        Self {
            air,
            pressure_limit: PRESSURE_LIMIT * (1.0 + 0.2 * quality),
            temp_limit: TEMP_LIMIT,
            integrity: TANK_INTEGRITY,
            crystal: false,
            suppress: false,
        }
    }

    /// `atom_break` happens at `max_integrity * integrity_failure`.
    pub fn broken(&self) -> bool {
        self.integrity <= TANK_INTEGRITY * TANK_INTEGRITY_FAILURE
    }
}

/// `/obj/machinery/portable_atmospherics/pipe_scrubber`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PipeScrubber {
    pub on: bool,
    /// `direction`: `PUMP_IN` (buffer -> tank, scrubbing) or `PUMP_OUT`.
    pub pump_in: bool,
    pub volume_rate: f64,
    /// `scrubbing`, a gas mask.
    pub filter: u32,
    pub tank: InternalTank,
}

/// `scrub()`: `min(1, volume_rate / volume)` of the buffer is taken out, every scrubbed gas of
/// it goes into the tank and the rest back. Nothing while the tank is at `PUMP_MAX_PRESSURE`.
pub fn scrub(buffer: &mut PipeAir, tank: &mut PipeAir, filter: u32, volume_rate: f64) {
    if tank.pressure() >= PUMP_MAX_PRESSURE {
        return;
    }
    let moles = (volume_rate / buffer.volume).min(1.0) * buffer.total_moles();
    let Some(mut filtering) = buffer.remove(moles) else {
        return;
    };
    let mut filtered = GasMixture::vacuum();
    filtered.temperature = filtering.temperature;
    for (g, m) in filtering.moles.iter_mut().enumerate() {
        if filter & (1 << g) != 0 && *m != 0.0 {
            filtered.moles[g] = *m;
            *m = 0.0;
        }
    }
    filtering.garbage_collect();
    tank.merge(&filtered);
    buffer.merge(&filtering);
}

/// `process_atmos` of the pipe scrubbers and their internal canisters (the buffer's own
/// reactions run with the other portables).
pub fn process_pipe_scrubbers(world: &mut World) {
    let mut q = world.query::<(Entity, &PipeScrubber)>();
    let all: Vec<Entity> = q.iter(world).map(|(e, _)| e).collect();
    for e in all {
        // The buffer over its limits: damage, no pumping this fire.
        let state = buffer_state(world, e);
        if let Some(damage) =
            state.and_then(|(t, p)| atmos_damage(t, p, TEMP_LIMIT, PRESSURE_LIMIT))
        {
            crate::obj_damage::take_damage(world, e, damage, DamageType::Burn);
        } else {
            pump(world, e);
        }
        if world.get_entity(e).is_err() {
            continue;
        }
        // The internal canister's own `process_atmos`.
        let broke = {
            let Some(mut s) = world.get_mut::<PipeScrubber>(e) else {
                continue;
            };
            let tank = &mut s.tank;
            if let Some(damage) = atmos_damage(
                tank.air.temperature(),
                tank.air.pressure(),
                tank.temp_limit,
                tank.pressure_limit,
            ) {
                // `take_damage(..., BURN, 0)`: no armor flag.
                tank.integrity = (tank.integrity - damage).max(0.0);
            }
            if !tank.suppress && tank.air.total_moles() > 0.0 {
                let volume = tank.air.volume;
                tank.air
                    .gas
                    .react(Holder::Other, volume, &mut Exposures::default());
            }
            tank.broken()
        };
        if broke {
            crate::obj_damage::deconstruct(world, e, true);
        }
    }
}

fn buffer_state(world: &mut World, e: Entity) -> Option<(f64, f64)> {
    let portable = world.get::<PortableAtmos>(e)?;
    let air = portable.air(world.resource::<Pipenet>())?;
    Some((air.temperature(), air.pressure()))
}

fn pump(world: &mut World, e: Entity) {
    let Some(s) = world.get::<PipeScrubber>(e) else {
        return;
    };
    if !s.on {
        return;
    }
    let (pump_in, filter, rate) = (s.pump_in, s.filter, s.volume_rate);
    let mut tank = s.tank.air.clone();
    world.resource_scope(|world, mut net: Mut<Pipenet>| {
        let Some(mut portable) = world.get_mut::<PortableAtmos>(e) else {
            return;
        };
        let Some(buffer) = portable.bypass_change_detection().air_mut(&mut net) else {
            return;
        };
        if pump_in {
            scrub(buffer, &mut tank, filter, rate);
        } else {
            ss13_atmos::pipenet::air::pump_gas(&mut tank, buffer, PUMP_MAX_PRESSURE);
        }
        // The port's pipeline takes the change on its next reconcile.
        if let Some((_, id)) = portable.port {
            net.update_parents(id);
        }
    });
    if let Some(mut s) = world.get_mut::<PipeScrubber>(e) {
        s.tank.air = tank;
    }
}

/// `update_icon_state`: `pipe_scrubber_active` while on.
pub fn pipe_scrubber_looks(
    icons: Option<Res<ss13_mapload::IconIndex>>,
    mut q: Query<
        (
            &PipeScrubber,
            &ss13_mapload::DmAtom,
            &mut ss13_shared::Appearance,
        ),
        Changed<PipeScrubber>,
    >,
) {
    let Some(icons) = icons else {
        return;
    };
    for (s, atom, mut appearance) in &mut q {
        let state = if s.on {
            format!("{}_active", atom.icon_state)
        } else {
            atom.icon_state.clone()
        };
        if let Some(id) = icons.get(&atom.icon, &state)
            && appearance.icon_state != id
        {
            appearance.icon_state = id;
        }
    }
}

/// `atom_deconstruct`: the buffer and the internal canister's gas onto the tile. True for a
/// pipe scrubber.
pub fn deconstruct(world: &mut World, e: Entity) -> bool {
    if world.get::<PipeScrubber>(e).is_none() {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return true;
    };
    super::portable::disconnect(world, e);
    let buffer = world
        .get_mut::<PortableAtmos>(e)
        .and_then(|mut p| p.air.take());
    let tank = world
        .get_mut::<PipeScrubber>(e)
        .map(|mut s| std::mem::replace(&mut s.tank.air, PipeAir::new(TANK_VOLUME)));
    let mut atmos = world.resource_mut::<ss13_atmos::Atmos>();
    for air in [buffer, tank].into_iter().flatten() {
        atmos.merge_air(pos, &air.gas);
    }
    true
}
