//! Power monitoring consoles (server): tg `/obj/machinery/computer/monitor`
//! (`code/modules/power/monitor.dm`).
//!
//! A console reads the powernet of the cable under it, else of its area's APC (`search`). Every
//! machine tick, after the power tick (tg `process_late`), it draws its active load while
//! attached (idle and searching again while not) and every 5 s records the net's supply and
//! demand, keeping the last 60 (`record`). The `PowerMonitor` window shows the history and the
//! APCs on the net with their cell, load and channels.
// shortcut: the window's area list is the snapshot of the last machine tick (tg builds it in
// `ui_data`, at most ~2 s fresher). The PDA program is in `../comms/`.

use std::collections::VecDeque;

use bevy::prelude::*;
use ss13_mapload::{Areas, DmName, DmPower};
use ss13_shared::TilePos;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::power_monitor::{PowerMonitorArea, PowerMonitorData, PowerMonitorUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject, UiSystems, UiWindow};
use ss13_world::SpatialIndex;

use super::{Apc, AreaPower, Grid, MachineTicks, NetId, Unpowered};
use crate::obj_damage::is_under;

pub const MONITOR: &str = "/obj/machinery/computer/monitor";
/// `record_size`.
const RECORD_SIZE: usize = 60;
/// `record_interval`, seconds.
const RECORD_INTERVAL: f64 = 5.0;

/// One console: what it reads and what it recorded.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PowerMonitor {
    /// tg `attached_wire_ref`, `local_apc_ref`.
    pub cable: Option<Entity>,
    pub apc: Option<Entity>,
    /// `history["supply"]`, `history["demand"]`, watts.
    pub supply: VecDeque<f32>,
    pub demand: VecDeque<f32>,
    pub next_record: f64,
    /// `use_power == ACTIVE_POWER_USE`.
    pub active: bool,
    /// The last tick's `ui_data`: attached, the APCs on the net.
    pub attached: bool,
    pub areas: Vec<PowerMonitorArea>,
}

pub struct PowerMonitorPlugin;

impl Plugin for PowerMonitorPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.register_ui::<MonitorWindow>()
            .add_systems(ss13_world::MapInit, init_monitors.after(super::init_power))
            .add_systems(
                Update,
                (
                    process_monitors.after(super::PowerSystems),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
        // `ui_interact` of a powered console.
        app.add_click_handler(ClickPhase::EmptyHand, 1971, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
    }
}

fn init_monitors(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmPower), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, dm)| is_under(&dm.path, MONITOR))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// A console's state (map load and `crate::machine_frame`): tg `Initialize` -> `search()`.
pub fn setup(world: &mut World, e: Entity) {
    if !world
        .get::<DmPower>(e)
        .is_some_and(|d| is_under(&d.path, MONITOR))
    {
        return;
    }
    let mut monitor = PowerMonitor {
        active: true,
        ..default()
    };
    search(world, e, &mut monitor);
    world.entity_mut(e).insert(monitor);
}

/// tg `search()`: the cable on the console's tile, else its area's APC.
fn search(world: &World, e: Entity, monitor: &mut PowerMonitor) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let grid = world.resource::<Grid>();
    monitor.cable = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&c| grid.cable_net(c).is_some());
    if monitor.cable.is_some() {
        return;
    }
    monitor.apc = world
        .resource::<Areas>()
        .area_at(pos)
        .and_then(|a| world.get::<AreaPower>(a))
        .and_then(|a| a.apc);
}

/// tg `get_powernet()`.
fn powernet(world: &World, monitor: &PowerMonitor) -> Option<NetId> {
    let grid = world.resource::<Grid>();
    if let Some(cable) = monitor.cable.filter(|&c| grid.cable_net(c).is_some()) {
        return grid.cable_net(cable);
    }
    monitor
        .apc
        .filter(|&a| world.get::<Apc>(a).is_some())
        .and_then(|a| grid.node_net(a))
}

/// `ui_data`'s `areas`: the APCs on `net`.
fn areas_on(world: &mut World, net: NetId) -> Vec<PowerMonitorArea> {
    let mut q = world.query::<(Entity, &Apc)>();
    let grid = world.resource::<Grid>();
    let apcs: Vec<(Entity, Apc)> = q
        .iter(world)
        .filter(|(e, _)| grid.node_net(*e) == Some(net))
        .map(|(e, a)| (e, a.clone()))
        .collect();
    apcs.into_iter()
        .map(|(_, apc)| PowerMonitorArea {
            name: world.get::<DmName>(apc.area).map_or_else(String::new, |n| {
                crate::vending::plain_name(&n.name).to_owned()
            }),
            charge: apc.cell.as_ref().map_or(0.0, |c| c.percent() as f32),
            load: super::apc_ui::display_power(
                apc.lastused.iter().sum::<f64>() + apc.lastused_charge,
            ),
            charging: apc.charging,
            eqp: apc.channels[super::Channel::Equip as usize],
            lgt: apc.channels[super::Channel::Light as usize],
            env: apc.channels[super::Channel::Environ as usize],
        })
        .collect()
}

/// tg `record()`: one sample of the net's supply and demand every [`RECORD_INTERVAL`].
fn record(monitor: &mut PowerMonitor, now: f64, avail: f64, load: f64) {
    if now < monitor.next_record {
        return;
    }
    monitor.next_record = now + RECORD_INTERVAL;
    // `energy_to_power`: joules per machine tick as watts.
    monitor
        .supply
        .push_back((avail / super::MACHINE_TICK_SECS) as f32);
    monitor
        .demand
        .push_back((load / super::MACHINE_TICK_SECS) as f32);
    for list in [&mut monitor.supply, &mut monitor.demand] {
        while list.len() > RECORD_SIZE {
            list.pop_front();
        }
    }
}

/// tg `process_late()` of every console, once per machine tick.
// Cost: consoles (a handful) × APCs (~150) per 2 s tick.
fn process_monitors(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &PowerMonitor)>();
    let monitors: Vec<(Entity, PowerMonitor)> =
        q.iter(world).map(|(e, m)| (e, m.clone())).collect();
    for (e, mut monitor) in monitors {
        let net = powernet(world, &monitor);
        match net {
            None => {
                if monitor.active {
                    monitor.active = false;
                    super::update_use_power(world, e, false);
                }
                search(world, e, &mut monitor);
                monitor.attached = false;
                monitor.areas.clear();
            }
            Some(net) => {
                if !monitor.active {
                    monitor.active = true;
                    super::update_use_power(world, e, true);
                }
                let (avail, load) = world
                    .resource::<Grid>()
                    .net(net)
                    .map_or((0.0, 0.0), |n| (n.avail, n.load));
                record(&mut monitor, now, avail, load);
                monitor.attached = true;
                monitor.areas = areas_on(world, net);
            }
        }
        if world.get::<PowerMonitor>(e) != Some(&monitor) {
            world.entity_mut(e).insert(monitor);
        }
    }
}

/// A click on a console opens its window while powered (tg `ui_interact`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<PowerMonitor>(target).is_none() {
        return false;
    }
    open(world, client, mob, target);
    true
}

/// `ui_interact` (also a silicon's `attack_ai`, `crate::silicon::interact`).
pub fn open(world: &mut World, client: Entity, actor: Entity, console: Entity) {
    if world.get::<Unpowered>(console).is_some() {
        return;
    }
    let title = world.get::<DmName>(console).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    });
    world.write_message(OpenUi::new::<MonitorWindow>(client, actor, console, title));
}

/// tg `/atom/ui_status`: an unpowered computer is update-only.
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<PowerMonitor>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == MonitorWindow::NAME && unpowered.contains(window.source) {
            window.cap_status(ss13_tgui::UiStatus::Update);
        }
    }
}

/// The server side of the `PowerMonitor` interface (no acts: sorting is client-side).
pub struct MonitorWindow;

impl UiInterface for MonitorWindow {
    const NAME: &'static str = PowerMonitorUi::NAME;
    type Data = PowerMonitorData;
    type StaticData = ();
}

impl ServerUi for MonitorWindow {
    type Source = &'static PowerMonitor;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &PowerMonitor) {}

    fn data(m: &PowerMonitor, _: Entity, _: &()) -> PowerMonitorData {
        PowerMonitorData {
            stored: RECORD_SIZE as u32,
            interval: RECORD_INTERVAL as f32,
            attached: m.attached,
            history_supply: m.supply.iter().copied().collect(),
            history_demand: m.demand.iter().copied().collect(),
            areas: m.areas.clone(),
        }
    }

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_every_interval_and_keeps_the_last_sixty() {
        let mut m = PowerMonitor::default();
        record(&mut m, 0.0, 200_000.0, 100_000.0);
        assert_eq!(m.supply, [100_000.0]);
        assert_eq!(m.demand, [50_000.0]);
        // Within the interval: nothing.
        record(&mut m, 4.0, 1.0, 1.0);
        assert_eq!(m.supply.len(), 1);
        for i in 1..=70 {
            record(&mut m, 6.0 * f64::from(i), 2.0, 2.0);
        }
        assert_eq!(m.supply.len(), RECORD_SIZE);
        assert_eq!(m.demand.back(), Some(&1.0));
    }

    #[test]
    fn reads_the_cable_under_it_else_its_areas_apc() {
        let mut w = World::new();
        w.init_resource::<SpatialIndex>();
        let pos = TilePos::new(1, 1, 1);
        let area = w.spawn_empty().id();
        let apc = w.spawn(Apc::new(area, None)).id();
        w.entity_mut(area).insert(AreaPower {
            apc: Some(apc),
            ..default()
        });
        let mut areas = Areas::default();
        areas.add_area(area);
        assert!(areas.add_level(1, 10, 10, area));
        w.insert_resource(areas);
        let mut grid = Grid::default();
        grid.add_node(
            apc,
            TilePos::new(5, 5, 1),
            2,
            super::super::NodeKind::Machine,
        );
        let apc_cable = w.spawn_empty().id();
        let apc_net = grid.add_cable(apc_cable, TilePos::new(5, 5, 1), 2);
        w.insert_resource(grid);
        let console = w.spawn(pos).id();
        let mut m = PowerMonitor::default();
        search(&w, console, &mut m);
        assert_eq!((m.cable, m.apc), (None, Some(apc)));
        assert_eq!(powernet(&w, &m), Some(apc_net));
        assert_eq!(areas_on(&mut w, apc_net).len(), 1);
    }
}
