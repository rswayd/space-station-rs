//! Portable atmospherics (`code/modules/atmospherics/machinery/portable/`): canisters,
//! portable pumps and scrubbers. Their gas is [`PortableAtmos::air`] while free; wrenched to a
//! connector port it moves into the [`Pipenet`] (`Pipenet::attach`) and the port's pipeline
//! reconciles with it, as tg `connect` does. A gas tank inserted into one (`holding`) is where
//! it releases, pumps or scrubs instead of the tile.

use bevy::prelude::*;
use ss13_atmos::gas::{CELL_VOLUME, MOLAR_ACCURACY, N2STANDARD, O2STANDARD, ONE_ATMOSPHERE, T20C};
use ss13_atmos::pipenet::air::{Mix, PipeAir, TurfMix, pump_moles, release_moles};
use ss13_atmos::pipenet::{DeviceId, Pipenet};
use ss13_atmos::reactions::{Exposures, Holder};
use ss13_atmos::{Atmos, GasId, GasMixture};
use ss13_mapload::DmAtmosDevice;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;

use super::devices::scrub_gases;
use crate::internals::TankAir;

pub const CANISTER: &str = "/obj/machinery/portable_atmospherics/canister";
const AIR_CANISTER: &str = "/obj/machinery/portable_atmospherics/canister/air";
const FUSION_TEST_CANISTER: &str = "/obj/machinery/portable_atmospherics/canister/fusion_test";
const PORTABLE_PUMP: &str = "/obj/machinery/portable_atmospherics/pump";
const PORTABLE_SCRUBBER: &str = "/obj/machinery/portable_atmospherics/scrubber";
const HUGE_SCRUBBER: &str = "/obj/machinery/portable_atmospherics/scrubber/huge";
const MOVABLE_HUGE_SCRUBBER: &str = "/obj/machinery/portable_atmospherics/scrubber/huge/movable";
const PORTABLE_ROOT: &str = "/obj/machinery/portable_atmospherics";

/// `CAN_DEFAULT_RELEASE_PRESSURE`, `CAN_MIN_RELEASE_PRESSURE`, `CAN_MAX_RELEASE_PRESSURE`.
pub const CAN_DEFAULT_RELEASE_PRESSURE: f64 = ONE_ATMOSPHERE;
pub const CAN_MIN_RELEASE_PRESSURE: f64 = ONE_ATMOSPHERE * 0.1;
pub const CAN_MAX_RELEASE_PRESSURE: f64 = ONE_ATMOSPHERE * 25.0;
/// `PUMP_DEFAULT_PRESSURE`, `PUMP_MIN_PRESSURE`, `PUMP_MAX_PRESSURE`.
pub const PUMP_DEFAULT_PRESSURE: f64 = ONE_ATMOSPHERE;
pub const PUMP_MIN_PRESSURE: f64 = PUMP_DEFAULT_PRESSURE / 10.0;
pub const PUMP_MAX_PRESSURE: f64 = PUMP_DEFAULT_PRESSURE * 25.0;
/// `TANK_LEAK_PRESSURE`, `TANK_FRAGMENT_PRESSURE`.
pub const TANK_LEAK_PRESSURE: f64 = 30.0 * ONE_ATMOSPHERE;
pub const TANK_FRAGMENT_PRESSURE: f64 = 40.0 * ONE_ATMOSPHERE;

/// A portable machine's gas and what it is attached to.
#[derive(Component, Clone, Debug)]
pub struct PortableAtmos {
    /// `air_contents` while not on a port; `None` while the port holds it.
    pub air: Option<PipeAir>,
    /// The connector it is wrenched to.
    pub port: Option<(Entity, DeviceId)>,
    /// The inserted gas tank (`holding`).
    pub holding: Option<Entity>,
}

impl PortableAtmos {
    /// `air_contents`.
    pub fn air<'a>(&'a self, net: &'a Pipenet) -> Option<&'a PipeAir> {
        match self.port {
            Some((_, id)) => net.attached(id),
            None => self.air.as_ref(),
        }
    }

    pub fn air_mut<'a>(&'a mut self, net: &'a mut Pipenet) -> Option<&'a mut PipeAir> {
        match self.port {
            Some((_, id)) => net.attached_mut(id),
            None => self.air.as_mut(),
        }
    }
}

/// `/obj/machinery/portable_atmospherics/canister`. Its cell and shielding live in
/// [`super::canister`].
// shortcut: no relabel, no recolor (the GAGS icon keeps the type's colors) and no
// hypernoblium crystal.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Canister {
    pub valve_open: bool,
    pub release_pressure: f64,
    /// `current_pressure_state` (the light overlay drawn).
    pub light: Option<&'static str>,
    /// `machine_stat & BROKEN` ([`super::damage::canister_break`]).
    pub broken: bool,
    /// `pressure_limit`, `temp_limit` (`take_atmos_damage`).
    pub pressure_limit: f64,
    pub temp_limit: f64,
    /// `shielding_powered`.
    pub shielding: bool,
    /// `internal_cell` (an item inside the canister).
    pub cell: Option<Entity>,
    pub panel_open: bool,
}

impl Canister {
    /// A fresh canister: valve shut, default release pressure, the default limits.
    pub fn new(valve_open: bool, release_pressure: f64) -> Self {
        Self {
            valve_open,
            release_pressure,
            light: None,
            broken: false,
            pressure_limit: super::damage::PRESSURE_LIMIT,
            temp_limit: super::damage::TEMP_LIMIT,
            shielding: false,
            cell: None,
            panel_open: false,
        }
    }
}

/// `nob_crystal_inserted`: a hypernoblium crystal inside; `suppress` is `suppress_reactions`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct NobCrystal {
    pub suppress: bool,
}

pub const NOB_CRYSTAL: &str = "/obj/item/hypernoblium_crystal";

/// `hypernoblium_crystal/interact_with_atom` on a portable machine: it goes in (`uses = 1`, the
/// crystal is spent). True when the click was the crystal's.
// shortcut: on clothing it does nothing yet (pressure-proofing, see `crystals`).
pub fn insert_nob_crystal(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    let is_crystal = world
        .get::<ss13_mapload::DmItem>(held)
        .is_some_and(|i| is_under(&i.path, NOB_CRYSTAL));
    if !is_crystal || world.get::<PortableAtmos>(target).is_none() {
        return false;
    }
    let name = world
        .get::<ss13_mapload::DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    if world.get::<NobCrystal>(target).is_some() {
        crate::tools::tell(
            world,
            client,
            ss13_chat::Span::Warning,
            format!("В {name} уже вставлен кристалл гипер-ноблия!"),
        );
        return true;
    }
    world.entity_mut(target).insert(NobCrystal::default());
    crate::tools::tell(
        world,
        client,
        ss13_chat::Span::Notice,
        format!("Вы вставляете кристалл гипер-ноблия в {name}."),
    );
    world.despawn(held);
    true
}

/// `portable_atmospherics/examine`: the crystal and its glow.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(ss13_chat::Span, String)>) {
    let Some(crystal) = world.get::<NobCrystal>(target) else {
        return;
    };
    lines.push((
        ss13_chat::Span::Plain,
        "Внутри находится кристалл гипер-ноблия, позволяющий подавлять реакции.".into(),
    ));
    if crystal.suppress {
        lines.push((
            ss13_chat::Span::Plain,
            "Кристалл гипер-ноблия внутри слабо светится голубым: реакции сейчас подавлены.".into(),
        ));
    }
}

/// The UI's `reaction_suppression`: flips it, refused without a crystal (tg logs a possible
/// href exploit).
pub fn toggle_reaction_suppression(world: &mut World, target: Entity, actor: Entity) {
    let Some(mut crystal) = world.get_mut::<NobCrystal>(target) else {
        warn!("{actor:?} toggled reaction suppression on {target:?} without a crystal");
        return;
    };
    crystal.suppress = !crystal.suppress;
    let text = if crystal.suppress { "on" } else { "off" };
    info!("{actor:?} turned {text} the reaction suppression of {target:?}");
}

/// `/obj/machinery/portable_atmospherics/pump`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PortablePump {
    pub on: bool,
    /// `direction`: `PUMP_IN` (true, tile/tank -> pump) or `PUMP_OUT`.
    pub pump_in: bool,
    pub target_pressure: f64,
}

/// `/obj/machinery/portable_atmospherics/scrubber`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PortableScrubber {
    pub on: bool,
    pub filter: u32,
    pub volume_rate: f64,
    pub overpressure_m: f64,
    /// `scrubber/huge`: off unless anchored or `movable`; scrubs the tiles around twice.
    pub huge: bool,
    pub movable: bool,
}

/// `create_gas()`: `filled` of `maximum_pressure` of the canister's gas (air: O2/N2 mix).
// shortcut: special mixes (`anesthetic_mix`, `fusion_test`) start with their `gas_type` only.
pub fn canister_gas(dm: &DmAtmosDevice) -> GasMixture {
    let mut gas = match GasMixture::parse(&dm.initial_gas_mix) {
        Ok(g) if !dm.initial_gas_mix.is_empty() => g,
        _ => {
            let mut g = GasMixture::vacuum();
            g.temperature = T20C;
            g
        }
    };
    if !dm.initial_gas_mix.is_empty() {
        return gas;
    }
    let temperature = gas.temperature;
    let moles = |fraction: f64| {
        fraction * dm.maximum_pressure * dm.filled * dm.volume
            / (ss13_atmos::gas::R_IDEAL_GAS_EQUATION * temperature)
    };
    if is_under(&dm.path, AIR_CANISTER) {
        gas.add(GasId::O2, moles(O2STANDARD));
        gas.add(GasId::N2, moles(N2STANDARD));
    } else if let Some(g) = GasId::parse(&dm.gas_type) {
        gas.add(g, moles(1.0));
    }
    gas
}

/// `get_pressure_state`: the indicator light by pressure.
pub fn pressure_light(pressure: f64) -> Option<&'static str> {
    if pressure >= 40.0 * ONE_ATMOSPHERE {
        Some("can-3")
    } else if pressure >= 10.0 * ONE_ATMOSPHERE {
        Some("can-2")
    } else if pressure >= 5.0 * ONE_ATMOSPHERE {
        Some("can-1")
    } else if pressure >= 10.0 {
        Some("can-0")
    } else {
        None
    }
}

/// Portable machines from the map: their gas and state.
pub fn init_portables(world: &mut World, found: &[(Entity, DmAtmosDevice, TilePos, u8)]) {
    let mut count = 0;
    let mut quality = world
        .get_resource::<crate::combat::CombatRng>()
        .map_or_else(
            || crate::combat::CombatRng(0),
            |r| crate::combat::CombatRng(r.0),
        );
    for (e, dm, _, _) in found {
        let path = dm.path.as_str();
        if !is_under(path, PORTABLE_ROOT) {
            continue;
        }
        let volume = dm.volume.max(1.0);
        let mut air = PipeAir::new(volume);
        air.gas.temperature = T20C;
        let mut entity = world.entity_mut(*e);
        if is_under(path, CANISTER) {
            air.gas = canister_gas(dm);
            let mut canister = Canister::new(
                dm.valve_open,
                if dm.release_pressure > 0.0 {
                    dm.release_pressure
                } else {
                    CAN_DEFAULT_RELEASE_PRESSURE
                },
            );
            canister.light = pressure_light(air.pressure());
            if is_under(path, FUSION_TEST_CANISTER) {
                (canister.temp_limit, canister.pressure_limit) = (1e12, 1e14);
            }
            // `Initialize`: `pressure_limit *= 1 + 0.2 * rand()`.
            canister.pressure_limit *= 1.0 + 0.2 * f64::from(quality.unit());
            entity.insert(canister);
        } else if is_under(path, PORTABLE_PUMP) {
            entity.insert(PortablePump {
                on: dm.on,
                pump_in: dm.direction,
                target_pressure: if dm.target_pressure > 0.0 {
                    dm.target_pressure
                } else {
                    PUMP_DEFAULT_PRESSURE
                },
            });
        } else if is_under(path, PORTABLE_SCRUBBER) {
            entity.insert(PortableScrubber {
                on: dm.on,
                filter: super::gas_mask(&dm.filter_types),
                volume_rate: dm.volume_rate,
                overpressure_m: if dm.overpressure_m > 0.0 {
                    dm.overpressure_m
                } else {
                    100.0
                },
                huge: is_under(path, HUGE_SCRUBBER),
                movable: is_under(path, MOVABLE_HUGE_SCRUBBER),
            });
        } else {
            // shortcut: pipe scrubbers and bluespace sinks hold gas but do nothing yet.
        }
        entity.insert(PortableAtmos {
            air: Some(air),
            port: None,
            holding: None,
        });
        count += 1;
    }
    if let Some(mut rng) = world.get_resource_mut::<crate::combat::CombatRng>() {
        rng.0 = quality.0;
    }
    world
        .resource_mut::<super::AtmosMachineryReport>()
        .portables = count;
}

/// A tank's gas as a transfer side, and back.
fn tank_air(tank: &TankAir) -> PipeAir {
    PipeAir::with_gas(tank.air.clone(), tank.volume)
}

type PortableQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static TilePos,
        &'static mut PortableAtmos,
        Option<&'static mut Canister>,
        Option<&'static PortablePump>,
        Option<&'static mut PortableScrubber>,
        (
            Option<&'static crate::obj_damage::Anchored>,
            Option<&'static NobCrystal>,
        ),
    ),
>;

/// `process_atmos` of portables: reactions, then the canister valve, the pump, the scrubber.
pub fn process_portables(
    mut net: ResMut<Pipenet>,
    mut atmos: ResMut<Atmos>,
    mut portables: PortableQuery,
    mut tanks: Query<&mut TankAir>,
) {
    for (_, &pos, mut portable, canister, pump, scrubber, (anchored, crystal)) in &mut portables {
        // `huge/process_atmos`: a loose, unmovable huge scrubber switches off.
        let mut scrubber = scrubber;
        if let Some(s) = scrubber.as_mut()
            && s.huge
            && s.on
            && !s.movable
            && !anchored.is_some_and(|a| a.0)
        {
            s.on = false;
        }
        let holding = portable.holding;
        // The gas is no part of the look: `Changed<PortableAtmos>` stays for the port and the
        // tank, or every portable's overlays would be rebuilt on every fire.
        let Some(air) = portable.bypass_change_detection().air_mut(&mut net) else {
            continue;
        };
        // `process_atmos`: `!suppress_reactions && air_contents.react(src)`.
        if air.total_moles() > 0.0 && !crystal.is_some_and(|c| c.suppress) {
            let volume = air.volume;
            air.gas
                .react(Holder::Other, volume, &mut Exposures::default());
        }
        let mut tank = holding.and_then(|t| tanks.get_mut(t).ok());
        if let Some(mut canister) = canister {
            if canister.valve_open {
                release(
                    air,
                    tank.as_deref_mut(),
                    &mut atmos,
                    pos,
                    canister.release_pressure,
                );
            }
            let light = pressure_light(air.pressure());
            if canister.light != light {
                canister.light = light;
            }
        } else if let Some(pump) = pump.filter(|p| p.on) {
            portable_pump(air, tank.as_deref_mut(), &mut atmos, pos, pump);
        } else if let Some(scrubber) = scrubber.filter(|s| s.on) {
            // `huge/process_atmos` scrubs the tiles around, then `..()` does the usual pass.
            if scrubber.huge && tank.is_none() {
                let around = atmos.adjacent_turfs(pos, true);
                scrub_tiles(air, &mut atmos, around, &scrubber);
            }
            portable_scrub(air, tank.as_deref_mut(), &mut atmos, pos, &scrubber);
        }
    }
}

/// The canister's `release_gas_to(holding air or the tile, release_pressure)`.
pub fn release(
    air: &mut PipeAir,
    tank: Option<&mut TankAir>,
    atmos: &mut Atmos,
    pos: TilePos,
    target: f64,
) -> bool {
    if let Some(tank) = tank {
        let mut out = tank_air(tank);
        let Some(moles) = release_moles(&*air, &out, target, 1.0, None) else {
            return false;
        };
        let Some(removed) = air.remove(moles) else {
            return false;
        };
        out.merge(&removed);
        tank.air = out.gas;
        return true;
    }
    let Some(env) = atmos.air(pos) else {
        return false;
    };
    let Some(moles) = release_moles(&*air, &TurfMix(env), target, 1.0, None) else {
        return false;
    };
    let Some(removed) = air.remove(moles) else {
        return false;
    };
    atmos.merge_air(pos, &removed);
    true
}

/// `portable_atmospherics/pump/process_atmos` once on: `pump_gas_to` in its direction.
fn portable_pump(
    air: &mut PipeAir,
    tank: Option<&mut TankAir>,
    atmos: &mut Atmos,
    pos: TilePos,
    pump: &PortablePump,
) -> bool {
    let target = pump.target_pressure;
    if let Some(tank) = tank {
        let mut other = tank_air(tank);
        let moved = if pump.pump_in {
            ss13_atmos::pipenet::air::pump_gas(&mut other, air, target)
        } else {
            ss13_atmos::pipenet::air::pump_gas(air, &mut other, target)
        };
        tank.air = other.gas;
        return moved;
    }
    let Some(env) = atmos.air(pos) else {
        return false;
    };
    if pump.pump_in {
        let moles = pump_moles(&TurfMix(env), &*air, target, None);
        match atmos.remove_air(pos, moles) {
            Some(removed) => {
                air.merge(&removed);
                true
            }
            None => false,
        }
    } else {
        let moles = pump_moles(&*air, &TurfMix(env), target, None);
        match air.remove(moles) {
            Some(removed) => atmos.merge_air(pos, &removed),
            None => false,
        }
    }
}

/// `portable_atmospherics/scrubber/process_atmos` once on: the tank, or the tile and its
/// neighbours.
fn portable_scrub(
    air: &mut PipeAir,
    tank: Option<&mut TankAir>,
    atmos: &mut Atmos,
    pos: TilePos,
    scrubber: &PortableScrubber,
) {
    if air.pressure() >= scrubber.overpressure_m * ONE_ATMOSPHERE {
        return;
    }
    let minimum = MOLAR_ACCURACY * 1000.0;
    if let Some(tank) = tank {
        let ratio = (scrubber.volume_rate / tank.volume).min(1.0);
        let taken = scrub_gases(&mut tank.air, scrubber.filter, ratio, minimum);
        tank.air.garbage_collect();
        air.merge(&taken);
        return;
    }
    let mut tiles = vec![pos];
    tiles.extend(atmos.adjacent_turfs(pos, true));
    scrub_tiles(air, atmos, tiles, scrubber);
}

/// `scrub(turf.return_air())` on each tile in turn.
fn scrub_tiles(
    air: &mut PipeAir,
    atmos: &mut Atmos,
    tiles: Vec<TilePos>,
    scrubber: &PortableScrubber,
) {
    let minimum = MOLAR_ACCURACY * 1000.0;
    let ratio = (scrubber.volume_rate / CELL_VOLUME).min(1.0);
    for tile in tiles {
        if air.pressure() >= scrubber.overpressure_m * ONE_ATMOSPHERE {
            return;
        }
        let has = atmos.air(tile).is_some_and(|env| {
            env.moles
                .iter()
                .enumerate()
                .any(|(g, m)| *m != 0.0 && scrubber.filter & (1 << g) != 0)
        });
        if !has {
            continue;
        }
        if let Some(taken) = atmos.modify_air(tile, |env| {
            scrub_gases(env, scrubber.filter, ratio, minimum)
        }) {
            air.merge(&taken);
        }
    }
}

/// `connect(new_port)`: the portable's gas joins the port's pipeline. False when the port is
/// taken or not on its tile.
pub fn connect(world: &mut World, portable: Entity, connector: Entity) -> bool {
    let (Some(&pos), Some(&cpos)) = (
        world.get::<TilePos>(portable),
        world.get::<TilePos>(connector),
    ) else {
        return false;
    };
    let Some(device) = world.get::<super::AtmosDevice>(connector).copied() else {
        return false;
    };
    let free = world
        .get::<super::PortsConnector>(connector)
        .is_some_and(|c| c.connected.is_none());
    let Some(state) = world.get::<PortableAtmos>(portable) else {
        return false;
    };
    if pos != cpos || !free || state.port.is_some() {
        return false;
    }
    let air = world
        .get_mut::<PortableAtmos>(portable)
        .and_then(|mut p| p.air.take())
        .unwrap_or_else(|| PipeAir::new(1.0));
    if let Err(air) = world.resource_mut::<Pipenet>().attach(device.id, air) {
        if let Some(mut p) = world.get_mut::<PortableAtmos>(portable) {
            p.air = Some(*air);
        }
        return false;
    }
    if let Some(mut p) = world.get_mut::<PortableAtmos>(portable) {
        p.port = Some((connector, device.id));
    }
    if let Some(mut c) = world.get_mut::<super::PortsConnector>(connector) {
        c.connected = Some(portable);
    }
    crate::obj_damage::set_anchored(world, portable, true);
    true
}

/// `disconnect()`: the portable takes its gas back from the port.
pub fn disconnect(world: &mut World, portable: Entity) -> bool {
    let Some((connector, id)) = world.get::<PortableAtmos>(portable).and_then(|p| p.port) else {
        return false;
    };
    let air = world
        .resource_mut::<Pipenet>()
        .detach(id)
        .unwrap_or_else(|| PipeAir::new(1.0));
    if let Some(mut p) = world.get_mut::<PortableAtmos>(portable) {
        p.port = None;
        p.air = Some(air);
    }
    if let Some(mut c) = world.get_mut::<super::PortsConnector>(connector) {
        c.connected = None;
    }
    crate::obj_damage::set_anchored(world, portable, false);
    true
}

/// The pressure of a portable's gas (kPa), wherever it is.
pub fn pressure(world: &World, portable: Entity) -> f64 {
    let net = world.resource::<Pipenet>();
    world
        .get::<PortableAtmos>(portable)
        .and_then(|p| p.air(net))
        .map_or(0.0, Mix::pressure)
}
