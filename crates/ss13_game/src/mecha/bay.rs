//! The mech bay (server), after tg `code/modules/vehicles/mecha/mech_bay.dm`:
//! `/obj/machinery/mech_bay_recharge_port` and `/obj/machinery/computer/mech_bay_power_console`
//! (tgui `MechBayPowerConsole`). The `recharge_floor` turf under the mech is decoration in tg.
//!
//! - The port charges the cell of a mech standing on the tile it faces (`recharging_turf`, one
//!   step in its `dir`) by `recharge_power` (5 tier 1 capacitors × 0.0125 ×
//!   `STANDARD_CELL_RATE`: 625 W) every machine tick, from the grid's surplus only
//!   (`charge_cell(grid_only = TRUE)`), and pays 1% of it plus its active load as heat loss. It
//!   works only while powered and linked to a console.
//! - The console links at startup (`reconnect`) to a port within one tile, else two tiles away
//!   in a cardinal direction, unless that port already has a console; "Sync" retries. A click
//!   with an empty hand on the powered console opens the window: the mech's integrity and its
//!   cell's charge. While a mech's cell is below full the screen shows `recharge_comp_on`.
//!
//! Cost: O(ports) per machine tick (0.5 Hz; MetaStation maps three) and one tile lookup each.
// shortcut: tier 1 parts only, no wrench/screwdriver/crowbar (turning, deconstruction).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmName};
use ss13_shared::{Appearance, Dir, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::mech_bay::{
    BayCell, BayMech, MechBayData, MechBayPowerConsoleUi, RechargePort,
};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use super::core::{ItemCell, Mech, MechaDb};
use crate::power::chargers::STANDARD_CELL_RATE;
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use ss13_mapload::DmPower;

pub const PORT: &str = "/obj/machinery/mech_bay_recharge_port";
pub const CONSOLE: &str = "/obj/machinery/computer/mech_bay_power_console";
/// `/obj/machinery/computer` icon.
const COMPUTER_DMI: &str = "icons/obj/machines/computer.dmi";
/// `RefreshParts` with the board's five tier 1 capacitors.
pub const RECHARGE_POWER: f64 = 5.0 * 0.0125 * STANDARD_CELL_RATE;

/// `/obj/machinery/mech_bay_recharge_port`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct MechBayPort {
    pub console: Option<Entity>,
    /// `recharging_mech_ref`.
    pub mech: Option<Entity>,
    /// `recharging_turf`: one step in its `dir`.
    pub turf: TilePos,
}

/// `/obj/machinery/computer/mech_bay_power_console`: `recharge_port`, and whether the screen
/// shows a charging mech.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct MechBayConsole {
    pub port: Option<Entity>,
    pub charging: bool,
}

pub struct MechBayPlugin;

impl Plugin for MechBayPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of the mech bay console: its window.
        app.add_click_handler(ClickPhase::EmptyHand, 2200, |w, c| {
            crate::mecha::bay::interact(w, c.client, c.mob, c.target)
        });
        app.register_ui::<MechBayWindow>()
            .add_systems(
                ss13_world::MapInit,
                init_bays.after(crate::power::init_power),
            )
            .add_systems(
                Update,
                (
                    console_acts.in_set(UiSystems::Handle),
                    (process_ports, console_looks)
                        .chain()
                        .after(crate::power::PowerSystems),
                ),
            );
    }
}

/// Ports with their charging tile, then consoles `reconnect()`.
fn init_bays(world: &mut World) {
    let mut q = world
        .query_filtered::<(Entity, &DmPower, &TilePos, Option<&DmAtom>), With<ss13_world::Mapped>>(
        );
    let found: Vec<(Entity, bool, TilePos, u8)> = q
        .iter(world)
        .filter_map(|(e, dm, &pos, atom)| {
            let dir = atom.map_or(4, |a| a.dir);
            if is_under(&dm.path, PORT) {
                Some((e, true, pos, dir))
            } else if is_under(&dm.path, CONSOLE) {
                Some((e, false, pos, dir))
            } else {
                None
            }
        })
        .collect();
    for &(e, port, pos, dir) in &found {
        if port {
            let turf = Dir::from_bits(dir).map_or(pos, |d| pos.step(d));
            world.entity_mut(e).insert(MechBayPort {
                console: None,
                mech: None,
                turf,
            });
        }
    }
    for &(e, port, ..) in &found {
        if !port {
            world.entity_mut(e).insert(MechBayConsole::default());
            reconnect(world, e);
        }
    }
}

fn port_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<MechBayPort>(e).is_some())
}

/// tg `mech_bay_power_console/reconnect`.
pub fn reconnect(world: &mut World, console: Entity) {
    if world
        .get::<MechBayConsole>(console)
        .is_none_or(|c| c.port.is_some())
    {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(console) else {
        return;
    };
    // `locate() in range(1, src)`: the 3x3 square, row by row.
    let mut port = None;
    'near: for dy in [1, 0, -1] {
        for dx in -1..=1 {
            if let Some(p) = port_on(world, TilePos::new(pos.x + dx, pos.y + dy, pos.z)) {
                port = Some(p);
                break 'near;
            }
        }
    }
    if port.is_none() {
        port = [Dir::North, Dir::South, Dir::East, Dir::West]
            .into_iter()
            .find_map(|d| port_on(world, pos.step(d).step(d)));
    }
    let Some(port) = port else {
        return;
    };
    let mut state = world.get_mut::<MechBayPort>(port).expect("found above");
    if state.console.is_none() {
        state.console = Some(console);
        world
            .get_mut::<MechBayConsole>(console)
            .expect("checked")
            .port = Some(port);
    }
}

/// One machine tick of every port (tg `mech_bay_recharge_port/process`).
fn process_ports(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    for _ in from..ticks {
        let mut q = world.query_filtered::<Entity, (With<MechBayPort>, Without<Unpowered>)>();
        let ports: Vec<Entity> = q.iter(world).collect();
        for port in ports {
            process_port(world, port, MACHINE_TICK_SECS);
        }
    }
}

fn mech_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Mech>(e).is_some())
}

pub fn process_port(world: &mut World, port: Entity, dt: f64) {
    let Some(mut state) = world.get::<MechBayPort>(port).copied() else {
        return;
    };
    if state.console.is_none() {
        return;
    }
    let mech = state
        .mech
        .filter(|&m| world.get::<Mech>(m).is_some())
        .or_else(|| mech_on(world, state.turf));
    state.mech = mech;
    if let Some(cell) = mech.and_then(|m| world.get::<Mech>(m)?.cell)
        && let Some(mut c) = world.get::<ItemCell>(cell).map(|c| c.0)
        && c.used_charge() > 0.0
    {
        let given = crate::power::charge_cell(world, port, RECHARGE_POWER * dt, &mut c, true);
        if given > 0.0 {
            let active = world
                .get::<DmPower>(port)
                .map_or(0.0, |d| d.active_power_usage);
            crate::power::machine_use_energy(world, port, (given + active) * 0.01, false);
        }
        world.entity_mut(cell).insert(ItemCell(c));
    }
    if mech.is_some_and(|m| world.get::<TilePos>(m) != Some(&state.turf)) {
        state.mech = None;
    }
    if world.get::<MechBayPort>(port) != Some(&state) {
        world.entity_mut(port).insert(state);
    }
}

/// tg `mech_bay_power_console/update_overlays`: `recharge_comp_on` while powered and the
/// port's mech has a cell below full.
fn console_looks(
    mut consoles: Query<(&mut MechBayConsole, &mut Appearance, Has<Unpowered>)>,
    ports: Query<&MechBayPort>,
    mechs: Query<&Mech>,
    cells: Query<&ItemCell>,
    icons: Option<Res<IconIndex>>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (mut console, mut appearance, unpowered) in &mut consoles {
        let charging = !unpowered
            && console
                .port
                .and_then(|p| ports.get(p).ok()?.mech)
                .and_then(|m| mechs.get(m).ok()?.cell)
                .and_then(|c| cells.get(c).ok())
                .is_some_and(|c| c.0.charge < c.0.maxcharge);
        if console.charging == charging {
            continue;
        }
        console.charging = charging;
        let Some(id) = icons.get(COMPUTER_DMI, "recharge_comp_on") else {
            continue;
        };
        if charging {
            appearance.overlays.push(Overlay::new(id));
        } else {
            appearance.overlays.retain(|o| o.icon_state != id);
        }
    }
}

/// The server half of `MechBayPowerConsole`.
pub struct MechBayWindow;

impl UiInterface for MechBayWindow {
    const NAME: &'static str = MechBayPowerConsoleUi::NAME;
    type Data = MechBayData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MechBayAct {
    Reconnect,
}

impl ServerUi for MechBayWindow {
    type Source = &'static MechBayConsole;
    type UserParam = (
        Query<'static, 'static, &'static MechBayPort>,
        Query<'static, 'static, (&'static Mech, Option<&'static DmName>)>,
        Query<'static, 'static, &'static ItemCell>,
        Option<Res<'static, MechaDb>>,
    );
    type Act = MechBayAct;

    fn static_data(_: &MechBayConsole) {}

    fn data(
        console: &MechBayConsole,
        _: Entity,
        (ports, mechs, cells, db): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> MechBayData {
        let Some(port) = console.port.and_then(|p| ports.get(p).ok()) else {
            return MechBayData::default();
        };
        let mech = port
            .mech
            .and_then(|m| mechs.get(m).ok())
            .map(|(mech, name)| {
                let maxhealth = db
                    .as_ref()
                    .and_then(|db| db.data.mechs.get(mech.def))
                    .map_or(0.0, |d| d.max_integrity);
                BayMech {
                    health: mech.integrity,
                    maxhealth,
                    cell: mech.cell.and_then(|c| cells.get(c).ok()).map(|c| BayCell {
                        charge: c.0.charge as f32,
                        maxcharge: c.0.maxcharge as f32,
                    }),
                    name: name
                        .map(|n| crate::vending::plain_name(&n.name).to_owned())
                        .unwrap_or_default(),
                }
            });
        MechBayData {
            recharge_port: Some(RechargePort { mech }),
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<MechBayAct, UiReject> {
        match action {
            "reconnect" => Ok(MechBayAct::Reconnect),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<MechBayWindow>>>) {
    let pending: Vec<Entity> = acts
        .read(world.resource::<Messages<UiActed<MechBayWindow>>>())
        .map(|a| a.source)
        .collect();
    for console in pending {
        reconnect(world, console);
    }
}

/// tg `attack_hand` -> `ui_interact` of a powered console. True when the click was its.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<MechBayConsole>(target).is_none() {
        return false;
    }
    open(world, client, mob, target);
    true
}

/// `ui_interact` (also for silicons): only while powered (tg `can_interact`: `is_operational`).
pub fn open(world: &mut World, client: Entity, actor: Entity, console: Entity) {
    if world.get::<Unpowered>(console).is_some() {
        return;
    }
    let title = world.get::<DmName>(console).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    });
    world.write_message(OpenUi::new::<MechBayWindow>(client, actor, console, title));
}
