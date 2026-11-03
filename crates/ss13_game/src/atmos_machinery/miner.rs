//! Gas miners: tg `/obj/machinery/atmospherics/miner` (`machinery/other/miner.dm`). Every
//! machine tick (2 s) an active miner checks its tile and, when fine, puts `spawn_mol` moles a
//! second of its gas at `spawn_temp` into the tile's air. A blocked, open, planetary or
//! over-pressured tile breaks it until the problem goes away; the reason shows on examine.
// shortcut: only `GASMINER_POWER_NONE` (every mapped miner): it needs its area's power or a
// powered cable node, here area power (`Unpowered`); no silicon debug message.

use bevy::prelude::*;
use ss13_atmos::gas::{CELL_VOLUME, R_IDEAL_GAS_EQUATION};
use ss13_atmos::{Atmos, GasId, GasMixture, TileKind, TurfAir};
use ss13_mapload::{DmAtmosDevice, IconIndex};
use ss13_shared::{Appearance, Overlay, TilePos};

use crate::obj_damage::is_under;
use crate::power::Unpowered;

pub const MINER: &str = "/obj/machinery/atmospherics/miner";
const DMI: &str = "icons/obj/machines/atmospherics/miners.dmi";
/// `SSmachines` wait.
pub const MINER_TICK_SECS: f64 = 2.0;

#[derive(Component, Clone, Debug, PartialEq)]
pub struct GasMiner {
    pub gas: Option<GasId>,
    pub temperature: f64,
    /// Moles per second.
    pub moles: f64,
    pub max_ext_kpa: f64,
    pub max_ext_mol: f64,
    pub overlay_color: [u8; 3],
    pub active: bool,
    /// `broken_message` while broken.
    pub broken: Option<&'static str>,
}

impl GasMiner {
    pub fn from_dm(dm: &DmAtmosDevice) -> Self {
        Self {
            gas: GasId::parse(&dm.spawn_id),
            temperature: dm.spawn_temp,
            moles: dm.spawn_mol,
            max_ext_kpa: dm.max_ext_kpa,
            // `INFINITY` (1e31) when unset.
            max_ext_mol: if dm.max_ext_mol > 0.0 {
                dm.max_ext_mol
            } else {
                1e31
            },
            overlay_color: super::parse_pipe_color(&dm.overlay_color),
            active: dm.active,
            broken: None,
        }
    }
}

/// What a miner's tile is like for `check_operation`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinerTile {
    pub kind: TileKind,
    pub planetary: bool,
    pub pressure: f64,
    pub moles: f64,
}

/// `check_operation`: `None` when it may mine, else the broken message.
pub fn check_operation(miner: &GasMiner, tile: MinerTile) -> Option<&'static str> {
    if tile.kind == TileKind::Blocked {
        return Some("VENT BLOCKED");
    }
    if tile.planetary {
        return Some("DEVICE NOT ENCLOSED IN A PRESSURIZED ENVIRONMENT");
    }
    if tile.kind == TileKind::Space {
        return Some("AIR VENTING TO SPACE");
    }
    let added = miner.moles * miner.temperature * R_IDEAL_GAS_EQUATION / CELL_VOLUME;
    if tile.pressure > miner.max_ext_kpa - added {
        return Some("EXTERNAL PRESSURE OVER THRESHOLD");
    }
    if tile.moles > miner.max_ext_mol {
        return Some("EXTERNAL AIR CONCENTRATION OVER THRESHOLD");
    }
    None
}

#[derive(Resource, Default)]
pub struct MinerClock(f64);

pub fn init_miners(world: &mut World) {
    let mut query = world.query::<(Entity, &DmAtmosDevice)>();
    let found: Vec<(Entity, GasMiner)> = query
        .iter(world)
        .filter(|(_, dm)| is_under(&dm.path, MINER))
        .map(|(e, dm)| (e, GasMiner::from_dm(dm)))
        .collect();
    for (e, miner) in found {
        world.entity_mut(e).insert(miner);
    }
}

/// `process(seconds_per_tick)` of every miner.
pub fn process_miners(
    time: Res<Time>,
    mut clock: ResMut<MinerClock>,
    mut atmos: ResMut<Atmos>,
    turfs: Query<&TurfAir>,
    index: Res<ss13_world::SpatialIndex>,
    mut miners: Query<(&mut GasMiner, &TilePos, Has<Unpowered>)>,
) {
    clock.0 += time.delta_secs_f64();
    if clock.0 < MINER_TICK_SECS {
        return;
    }
    clock.0 -= MINER_TICK_SECS;
    for (mut miner, &pos, unpowered) in &mut miners {
        if !miner.active {
            continue;
        }
        let turf = index.on_tile(pos).iter().find_map(|&e| turfs.get(e).ok());
        let air = atmos.air(pos);
        let tile = MinerTile {
            kind: turf.map_or(TileKind::Blocked, |t| t.kind),
            planetary: turf.is_some_and(|t| t.planetary),
            pressure: air.map_or(0.0, GasMixture::pressure),
            moles: air.map_or(0.0, GasMixture::total_moles),
        };
        let broken = check_operation(&miner, tile);
        if miner.broken != broken {
            miner.broken = broken;
        }
        let Some(gas) = miner.gas.filter(|_| broken.is_none() && !unpowered) else {
            continue;
        };
        let mut merger = GasMixture::vacuum();
        merger.set(gas, miner.moles * MINER_TICK_SECS);
        merger.temperature = miner.temperature;
        atmos.merge_air(pos, &merger);
    }
}

/// `update_overlays`: `broken`, or the colored `on` light while active.
pub fn miner_looks(
    icons: Option<Res<IconIndex>>,
    mut miners: Query<(&GasMiner, &mut Appearance), Changed<GasMiner>>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (miner, mut appearance) in &mut miners {
        appearance.overlays.clear();
        let overlay = if miner.broken.is_some() {
            icons.get(DMI, "broken").map(Overlay::new)
        } else if miner.active {
            let [r, g, b] = miner.overlay_color;
            icons.get(DMI, "on").map(|id| Overlay {
                color: Some([r, g, b, 255]),
                ..Overlay::new(id)
            })
        } else {
            None
        };
        appearance.overlays.extend(overlay);
    }
}

/// `examine`: the debug output of a broken miner.
pub fn examine_line(world: &World, e: Entity) -> Option<String> {
    let message = world.get::<GasMiner>(e)?.broken?;
    Some(format!("Его отладочный вывод печатает «{message}»."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn miner() -> GasMiner {
        GasMiner {
            gas: Some(GasId::O2),
            temperature: 293.15,
            moles: 5.0 * ss13_atmos::gas::MOLES_CELLSTANDARD,
            max_ext_kpa: 6500.0,
            max_ext_mol: 1e31,
            overlay_color: [255; 3],
            active: true,
            broken: None,
        }
    }

    /// `check_operation`'s order and the pressure margin of one tick's worth of gas.
    #[test]
    fn check_operation_follows_tg() {
        let m = miner();
        let tile = |kind, planetary, pressure| MinerTile {
            kind,
            planetary,
            pressure,
            moles: 100.0,
        };
        assert_eq!(
            check_operation(&m, tile(TileKind::Open, false, 101.0)),
            None
        );
        assert_eq!(
            check_operation(&m, tile(TileKind::Blocked, false, 0.0)),
            Some("VENT BLOCKED")
        );
        assert_eq!(
            check_operation(&m, tile(TileKind::Space, false, 0.0)),
            Some("AIR VENTING TO SPACE")
        );
        assert!(check_operation(&m, tile(TileKind::Open, true, 101.0)).is_some());
        // 519.5 mol at 293 K is ~506 kPa in a cell: it stops ~506 kPa under 6500.
        let margin = m.moles * m.temperature * R_IDEAL_GAS_EQUATION / CELL_VOLUME;
        assert_eq!(
            check_operation(&m, tile(TileKind::Open, false, 6500.0 - margin - 1.0)),
            None
        );
        assert!(check_operation(&m, tile(TileKind::Open, false, 6500.0 - margin + 1.0)).is_some());
    }
}
