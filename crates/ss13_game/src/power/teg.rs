//! The thermoelectric generator and its circulators (server): tg
//! `code/modules/power/thermoelectric_generator.dm` and
//! `code/modules/atmospherics/machinery/components/binary_devices/circulator.dm`.
//!
//! - A circulator is a binary pipe device across its `dir` (node 1, its output, at
//!   `turn(dir, 90)`; node 2, its input, at `turn(dir, 270)`; swapped when `flipped`): its ports
//!   are ordinary [`Pipenet`] port mixtures ([`crate::atmos_machinery::device_spec`]).
//! - Every SSair fire ([`process_tegs`], tg `process_atmos`) a wired generator with a hot and a
//!   cold circulator pulls half of each circulator's input-output pressure difference (at least
//!   10 kPa) through it, exchanges heat between the two parcels and banks 65% of the heat moved
//!   as `lastgen`; the parcels go on to the outputs.
//! - Every machine tick ([`machine_tick`], tg `process`) a tenth of `lastgen` goes into the
//!   generator's powernet.
//! - Tools are instant, as in tg: a screwdriver opens the panel of an anchored machine, a wrench
//!   with the panel open (un)anchors it, a multitool flips a circulator hot/cold or makes the
//!   generator look for its circulators again.
//!
//! Cost: O(generators) per SSair fire and machine tick, O(1) each; stations have none or one.

use bevy::prelude::*;
use ss13_atmos::GasMixture;
use ss13_atmos::gas::{ONE_ATMOSPHERE, R_IDEAL_GAS_EQUATION};
use ss13_atmos::pipenet::air::PipeAir;
use ss13_atmos::pipenet::{DeviceId, Pipenet};
use ss13_chat::Span;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtmosDevice, DmAtom, DmName};
use ss13_shared::appearance::Overlay;
use ss13_shared::{Appearance, Dir, EMISSIVE_PLANE, TilePos};
use ss13_tgui::interfaces::teg::{CirculatorData, ThermoElectricGeneratorData};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject, UiSystems};
use ss13_tgui::{UiInterface, interfaces::teg::ThermoElectricGeneratorUi};

use super::{Grid, NodeKind, Unpowered};
use crate::atmos_machinery::AtmosDevice;
use crate::obj_damage::{Anchored, Integrity, set_anchored};
use crate::tools::{ToolKind, tell};

pub const TEG: &str = "/obj/machinery/power/thermoelectric_generator";
pub const CIRCULATOR: &str = "/obj/machinery/atmospherics/components/binary/circulator";
/// `circulator/cold`: `mode = CIRCULATOR_COLD`.
// shortcut: the mode comes from the type, not a map edit of `mode`; no map sets one.
const COLD_CIRCULATOR: &str = "/obj/machinery/atmospherics/components/binary/circulator/cold";
/// `TEG_EFFICIENCY`.
pub const TEG_EFFICIENCY: f64 = 0.65;

/// `/obj/machinery/atmospherics/components/binary/circulator`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Circulator {
    /// `mode == CIRCULATOR_COLD`.
    pub cold: bool,
    pub panel_open: bool,
    pub last_pressure_delta: f64,
    pub generator: Option<Entity>,
    /// Its port mixtures while unanchored, out of the pipe graph (tg keeps `airs`).
    pub stored: Vec<PipeAir>,
}

impl Circulator {
    /// A map circulator of `path`.
    pub fn of(path: &str) -> Self {
        Self {
            cold: is_under(path, COLD_CIRCULATOR),
            ..default()
        }
    }
}

/// `/obj/machinery/power/thermoelectric_generator`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Teg {
    pub hot: Option<Entity>,
    pub cold: Option<Entity>,
    /// Joules banked, not yet put into the powernet.
    pub lastgen: f64,
    /// Joules put into the powernet last machine tick.
    pub lastgenlev: f64,
    /// `last_pressure_overlay`: whether the cold and the hot circulator moved gas.
    pub flowing: (bool, bool),
    pub panel_open: bool,
    pub layer: u8,
}

/// `circulator/set_init_directions` + `get_node_connects` for a cardinal `dir`.
pub fn circulator_nodes(dir: u8, flipped: bool) -> (u8, Vec<u8>) {
    let across = if matches!(dir, 1 | 2) { 4 | 8 } else { 1 | 2 };
    let cw = crate::atmos_machinery::turn_cw(dir);
    let ccw = crate::atmos_machinery::reverse_dir(cw);
    let nodes = if flipped {
        vec![cw, ccw]
    } else {
        vec![ccw, cw]
    };
    (across, nodes)
}

/// `circulator/return_transfer_air`: the parcel that crosses from the input (`airs[2]`) toward
/// the output (`airs[1]`), and `last_pressure_delta`.
pub fn return_transfer_air(net: &mut Pipenet, id: DeviceId) -> (Option<GasMixture>, f64) {
    let Some([output, input]) = net.airs_mut(id) else {
        return (None, 0.0);
    };
    let (output_pressure, input_pressure) = (output.pressure(), input.pressure());
    // At least 10 kPa to overcome the mechanism's friction.
    if output_pressure >= input_pressure - 10.0 || input.temperature() <= 0.0 {
        return (None, 0.0);
    }
    let pressure_delta = (input_pressure - output_pressure) / 2.0;
    let moles = pressure_delta * output.volume / (input.temperature() * R_IDEAL_GAS_EQUATION);
    let removed = input.remove(moles);
    net.update_parents(id);
    (removed, pressure_delta)
}

/// `process_atmos` heat exchange: the joules banked into `lastgen`; `hot` cools by what moved,
/// `cold` warms by the 35% not converted.
pub fn exchange(hot: &mut GasMixture, cold: &mut GasMixture) -> f64 {
    let (cold_capacity, hot_capacity) = (cold.heat_capacity_raw(), hot.heat_capacity_raw());
    let delta = hot.temperature - cold.temperature;
    if delta <= 0.0 || cold_capacity <= 0.0 || hot_capacity <= 0.0 {
        return 0.0;
    }
    let energy = delta * hot_capacity * cold_capacity / (hot_capacity + cold_capacity);
    let heat = energy * (1.0 - TEG_EFFICIENCY);
    hot.temperature -= energy / hot_capacity;
    cold.temperature += heat / cold_capacity;
    energy * TEG_EFFICIENCY
}

/// `thermoelectric_generator/process_atmos`, every SSair fire.
pub fn process_tegs(
    mut net: ResMut<Pipenet>,
    grid: Option<Res<Grid>>,
    mut tegs: Query<(Entity, &mut Teg)>,
    mut circulators: Query<(&AtmosDevice, &mut Circulator)>,
) {
    for (e, mut teg) in &mut tegs {
        let (Some(hot), Some(cold)) = (teg.hot, teg.cold) else {
            continue;
        };
        if grid.as_ref().is_none_or(|g| g.node_net(e).is_none()) {
            continue;
        }
        let Ok([(cold_device, mut cold_circ), (hot_device, mut hot_circ)]) =
            circulators.get_many_mut([cold, hot])
        else {
            continue;
        };
        let (cold_id, hot_id) = (cold_device.id, hot_device.id);
        let (mut cold_air, cold_delta) = return_transfer_air(&mut net, cold_id);
        let (mut hot_air, hot_delta) = return_transfer_air(&mut net, hot_id);
        cold_circ.last_pressure_delta = cold_delta;
        hot_circ.last_pressure_delta = hot_delta;
        if let (Some(c), Some(h)) = (cold_air.as_mut(), hot_air.as_mut()) {
            teg.lastgen += exchange(h, c);
        }
        for (air, id) in [(hot_air, hot_id), (cold_air, cold_id)] {
            if let (Some(air), Some(out)) = (air, net.air_mut(id, 0)) {
                out.merge(&air);
            }
        }
        teg.flowing = (cold_delta > 0.0, hot_delta > 0.0);
    }
}

/// `thermoelectric_generator/process`, every machine tick: a tenth of the bank (DM `round` is
/// `floor`) into the powernet.
pub(super) fn machine_tick(grid: &mut Grid, e: Entity, teg: &mut Teg) {
    let output = (teg.lastgen / 10.0).floor();
    if let Some(net) = grid.node_net(e).and_then(|n| grid.net_mut(n)) {
        net.newavail += output;
    }
    teg.lastgenlev = output;
    teg.lastgen -= output;
}

/// Generators from map load: wired where anchored (`connect_to_network`), then
/// `find_circulators`. After the pipe graph, whose circulators they look for.
fn init_tegs(world: &mut World) {
    let mut q = world.query::<(Entity, &super::DmPower, &TilePos)>();
    let found: Vec<(Entity, TilePos, u8)> = q
        .iter(world)
        .filter(|(_, dm, _)| is_under(&dm.path, TEG))
        .map(|(e, dm, &pos)| (e, pos, super::layer(dm)))
        .collect();
    for &(e, pos, layer) in &found {
        world.entity_mut(e).insert(Teg { layer, ..default() });
        if anchored(world, e)
            && let Some(mut grid) = world.get_resource_mut::<Grid>()
        {
            grid.add_node(e, pos, layer, NodeKind::Machine);
        }
    }
    for &(e, ..) in &found {
        find_circulators(world, e);
    }
    if !found.is_empty() {
        info!("thermoelectric generators: {}", found.len());
    }
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<Anchored>(e).is_some_and(|a| a.0)
}

fn dir_of(world: &World, e: Entity) -> u8 {
    world
        .get::<AtmosDevice>(e)
        .map(|d| d.dir)
        .or_else(|| world.get::<ss13_shared::Facing>(e).map(|f| f.0 as u8))
        .or_else(|| world.get::<DmAtom>(e).map(|a| a.dir))
        .unwrap_or(2)
}

/// `simple_rotation` (alt-click counterclockwise, right alt-click clockwise) of a loose
/// generator or circulator. True when the click was theirs.
pub fn rotate(world: &mut World, client: Entity, target: Entity, clockwise: bool) -> bool {
    if world.get::<Teg>(target).is_none() && world.get::<Circulator>(target).is_none() {
        return false;
    }
    if anchored(world, target) {
        tell(world, client, Span::Warning, "Сначала открутите его!");
        return true;
    }
    let cw = crate::atmos_machinery::turn_cw(dir_of(world, target));
    let dir = if clockwise {
        cw
    } else {
        crate::atmos_machinery::reverse_dir(cw)
    };
    if let Some(facing) = Dir::from_bits(dir) {
        world.entity_mut(target).insert(ss13_shared::Facing(facing));
    }
    true
}

/// `find_circulators`: on the sides across the generator's `dir`, a circulator facing it; the
/// first cold one becomes `cold_circ`, the first hot one `hot_circ`.
pub fn find_circulators(world: &mut World, teg: Entity) {
    null_circulators(world, teg);
    let Some(&pos) = world.get::<TilePos>(teg) else {
        return;
    };
    let sides = if dir_of(world, teg) & (1 | 2) != 0 {
        [(Dir::East, 8), (Dir::West, 4)]
    } else {
        [(Dir::North, 2), (Dir::South, 1)]
    };
    let mut valid = Vec::new();
    for (side, facing) in sides {
        let first = world
            .resource::<ss13_world::SpatialIndex>()
            .on_tile(pos.step(side))
            .iter()
            .copied()
            .find(|&c| world.get::<Circulator>(c).is_some());
        if let Some(c) = first.filter(|&c| dir_of(world, c) == facing) {
            valid.push(c);
        }
    }
    for c in valid {
        let cold = world.get::<Circulator>(c).is_some_and(|c| c.cold);
        let Some(state) = world.get::<Teg>(teg) else {
            return;
        };
        if (cold && state.cold.is_some()) || (!cold && state.hot.is_some()) {
            continue;
        }
        // A circulator another generator held lets go of it (tg leaves that pointer stale).
        disconnect_circulator(world, c);
        if let Some(mut t) = world.get_mut::<Teg>(teg) {
            if cold {
                t.cold = Some(c);
            } else {
                t.hot = Some(c);
            }
        }
        if let Some(mut circ) = world.get_mut::<Circulator>(c) {
            circ.generator = Some(teg);
        }
    }
}

/// `null_circulators`.
fn null_circulators(world: &mut World, teg: Entity) {
    let Some(mut t) = world.get_mut::<Teg>(teg) else {
        return;
    };
    let linked = [t.hot.take(), t.cold.take()];
    for c in linked.into_iter().flatten() {
        if let Some(mut circ) = world.get_mut::<Circulator>(c)
            && circ.generator == Some(teg)
        {
            circ.generator = None;
        }
    }
}

/// `circulator/disconnectFromGenerator`.
fn disconnect_circulator(world: &mut World, circulator: Entity) {
    let Some(teg) = world
        .get_mut::<Circulator>(circulator)
        .and_then(|mut c| c.generator.take())
    else {
        return;
    };
    if let Some(mut t) = world.get_mut::<Teg>(teg) {
        if t.cold == Some(circulator) {
            t.cold = None;
        }
        if t.hot == Some(circulator) {
            t.hot = None;
        }
    }
}

/// The circulator leaves the pipe graph (`node.disconnect`, `nullify_pipenet`), keeping its
/// port mixtures.
fn unplumb(world: &mut World, circulator: Entity) {
    let Some(device) = world.get::<AtmosDevice>(circulator).copied() else {
        return;
    };
    let stored = world.resource_scope(|_, mut net: Mut<Pipenet>| {
        let stored: Vec<PipeAir> = net
            .airs_mut(device.id)
            .map(|airs| {
                airs.iter_mut()
                    .map(|a| std::mem::replace(a, PipeAir::new(a.volume)))
                    .collect()
            })
            .unwrap_or_default();
        net.remove(device.id);
        stored
    });
    world.entity_mut(circulator).remove::<AtmosDevice>();
    if let Some(mut c) = world.get_mut::<Circulator>(circulator) {
        c.stored = stored;
    }
}

/// `set_init_directions` + `atmos_init` on its current tile: back into the pipe graph with the
/// mixtures it kept.
fn plumb(world: &mut World, circulator: Entity) {
    let (Some(&pos), Some(dm)) = (
        world.get::<TilePos>(circulator),
        world.get::<DmAtmosDevice>(circulator).cloned(),
    ) else {
        return;
    };
    let dir = dir_of(world, circulator);
    let Some((spec, dir)) = crate::atmos_machinery::device_spec(circulator, pos, &dm, dir) else {
        return;
    };
    let layer = spec.layer;
    let stored = world
        .get_mut::<Circulator>(circulator)
        .map(|mut c| std::mem::take(&mut c.stored))
        .unwrap_or_default();
    let id = world.resource_scope(|_, mut net: Mut<Pipenet>| {
        let id = net.add(spec);
        for (port, air) in stored.into_iter().enumerate() {
            if let Some(slot) = net.air_mut(id, port) {
                *slot = air;
            }
        }
        net.update_parents(id);
        id
    });
    world.entity_mut(circulator).insert((
        AtmosDevice { id, layer, dir },
        crate::atmos_machinery::looks::LookDirty,
    ));
}

/// Tools on a generator or a circulator (`wrench_act`, `screwdriver_act`, `multitool_act`).
/// True when handled.
pub fn tool_act(world: &mut World, client: Entity, target: Entity, tool: Option<ToolKind>) -> bool {
    let Some(tool) = tool else { return false };
    let teg = world.get::<Teg>(target).is_some();
    if !teg && world.get::<Circulator>(target).is_none() {
        return false;
    }
    let anchored_now = anchored(world, target);
    let panel_open = if teg {
        world.get::<Teg>(target).is_some_and(|t| t.panel_open)
    } else {
        world
            .get::<Circulator>(target)
            .is_some_and(|c| c.panel_open)
    };
    match tool {
        ToolKind::Screwdriver => {
            if !anchored_now {
                tell(world, client, Span::Warning, "Сначала закрепите его!");
                return true;
            }
            let open = !panel_open;
            if let Some(mut t) = world.get_mut::<Teg>(target) {
                t.panel_open = open;
            } else if let Some(mut c) = world.get_mut::<Circulator>(target) {
                c.panel_open = open;
            }
            let text = if open {
                "панель открыта"
            } else {
                "панель закрыта"
            };
            tell(world, client, Span::Notice, text);
        }
        ToolKind::Wrench => {
            if !panel_open {
                tell(world, client, Span::Warning, "Сначала откройте панель!");
                return true;
            }
            let now = !anchored_now;
            set_anchored(world, target, now);
            if teg {
                let (Some(&pos), Some(layer)) = (
                    world.get::<TilePos>(target),
                    world.get::<Teg>(target).map(|t| t.layer),
                ) else {
                    return true;
                };
                if now {
                    world
                        .resource_mut::<Grid>()
                        .add_node(target, pos, layer, NodeKind::Machine);
                } else {
                    // tg keeps an unanchored generator on its powernet; a loose machine is cut
                    // off like the emitter.
                    null_circulators(world, target);
                    world.resource_mut::<Grid>().remove_node(target);
                }
            } else {
                disconnect_circulator(world, target);
                if now {
                    plumb(world, target);
                } else {
                    unplumb(world, target);
                }
            }
            let text = if now {
                "закреплено"
            } else {
                "откреплено"
            };
            tell(world, client, Span::Notice, text);
        }
        ToolKind::Multitool if teg => {
            if !anchored_now {
                return false;
            }
            find_circulators(world, target);
            tell(world, client, Span::Notice, "циркуляторы обновлены");
        }
        ToolKind::Multitool => {
            disconnect_circulator(world, target);
            let Some(mut c) = world.get_mut::<Circulator>(target) else {
                return true;
            };
            c.cold = !c.cold;
            let text = if c.cold {
                "установлен на холодный"
            } else {
                "установлен на горячий"
            };
            tell(world, client, Span::Notice, text);
        }
        _ => return false,
    }
    true
}

pub struct TegWindow;

impl UiInterface for TegWindow {
    const NAME: &'static str = ThermoElectricGeneratorUi::NAME;
    type Data = ThermoElectricGeneratorData;
    type StaticData = ();
}

type TegUser = (
    Res<'static, Pipenet>,
    Option<Res<'static, Grid>>,
    Query<'static, 'static, &'static AtmosDevice>,
);

fn round1(v: f64) -> f32 {
    ((v * 10.0).round() / 10.0) as f32
}

/// One loop's readings: input `airs[2]`, output `airs[1]`.
fn loop_data(net: &Pipenet, id: DeviceId) -> Option<CirculatorData> {
    let (output, input) = (net.air(id, 0)?, net.air(id, 1)?);
    Some(CirculatorData {
        temperature_inlet: round1(input.temperature()),
        temperature_outlet: round1(output.temperature()),
        pressure_inlet: round1(input.pressure()),
        pressure_outlet: round1(output.pressure()),
    })
}

/// The window's `ui_data` (also what tests read).
pub fn ui_data(
    e: Entity,
    teg: &Teg,
    net: &Pipenet,
    grid: Option<&Grid>,
    devices: &Query<&AtmosDevice>,
) -> ThermoElectricGeneratorData {
    let error = |text: &str| ThermoElectricGeneratorData {
        error_message: Some(text.into()),
        ..default()
    };
    if grid.is_none_or(|g| g.node_net(e).is_none()) {
        return error("Unable to connect to the power network!");
    }
    let id = |c: Option<Entity>| c.and_then(|c| devices.get(c).ok()).map(|d| d.id);
    match (id(teg.cold), id(teg.hot)) {
        (None, None) => {
            error("Unable to locate any parts! Multitool the machine to sync to nearby parts.")
        }
        (None, _) => error("Unable to locate cold circulator!"),
        (_, None) => error("Unable to locate hot circulator!"),
        (Some(cold), Some(hot)) => ThermoElectricGeneratorData {
            error_message: None,
            // tg shows the joules of a machine tick as watts; the power is per second.
            last_power_output: (teg.lastgenlev / super::MACHINE_TICK_SECS) as f32,
            cold_data: loop_data(net, cold),
            hot_data: loop_data(net, hot),
        },
    }
}

impl ServerUi for TegWindow {
    type Source = (Entity, &'static Teg);
    type UserParam = TegUser;
    type Act = ();

    fn static_data(_: (Entity, &Teg)) {}

    fn data(
        (e, teg): (Entity, &Teg),
        _: Entity,
        (net, grid, devices): &bevy::ecs::system::SystemParamItem<'_, '_, TegUser>,
    ) -> ThermoElectricGeneratorData {
        ui_data(e, teg, net, grid.as_deref(), devices)
    }

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// An empty-hand click opens the window (`ui_interact`).
fn open_on_click(
    mut clicks: MessageReader<crate::click::ClickResolved>,
    tegs: Query<&DmName, With<Teg>>,
    inventories: Query<&crate::inventory::Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = tegs.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<TegWindow>(
                click.client,
                click.mob,
                click.target,
                crate::vending::plain_name(&name.name),
            ));
        }
    }
}

/// `thermoelectric_generator/update_overlays` and `circulator/update_overlays`.
fn teg_looks(
    icons: Option<Res<IconIndex>>,
    mut tegs: Query<(&Teg, &DmAtom, &mut Appearance, Option<&Integrity>)>,
    mut circulators: Query<
        (
            &Circulator,
            &DmAtom,
            &mut Appearance,
            Has<Unpowered>,
            Option<&Anchored>,
            Option<&Integrity>,
        ),
        Without<Teg>,
    >,
) {
    let Some(icons) = icons else { return };
    let overlay = |icon: &str, state: &str, emissive: bool| {
        icons.get(icon, state).map(|s| Overlay {
            plane: emissive.then_some(EMISSIVE_PLANE),
            ..Overlay::new(s)
        })
    };
    for (teg, atom, mut appearance, integrity) in &mut tegs {
        let mut overlays = Vec::new();
        if !integrity.is_some_and(|i| i.broken) {
            let level = ((teg.lastgenlev / 100_000.0).floor() as i64).min(11);
            if level > 0 {
                overlays.extend(overlay(&atom.icon, &format!("teg-op{level}"), false));
            }
            if teg.hot.is_some() && teg.cold.is_some() {
                let state = format!(
                    "teg-oc{}{}",
                    u8::from(teg.flowing.0),
                    u8::from(teg.flowing.1)
                );
                overlays.extend(overlay(&atom.icon, &state, false));
            }
        }
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
    for (circ, atom, mut appearance, unpowered, anchored, integrity) in &mut circulators {
        let icon = atom.icon.as_str();
        let mode = u8::from(circ.cold);
        let mut states: Vec<(String, bool)> = Vec::new();
        if anchored.is_some_and(|a| a.0) {
            states.push(("circ_anchor".into(), false));
        }
        states.push((format!("panel_{}", u8::from(circ.panel_open)), false));
        if unpowered || integrity.is_some_and(|i| i.broken) {
            states.push((format!("fan_{mode}"), false));
            states.push(("flow".into(), false));
            states.push(("display".into(), false));
        } else {
            states.push(("flow_on".into(), false));
            states.push(("flow_on-emissive".into(), true));
            states.push((format!("display_{mode}"), false));
            if circ.last_pressure_delta > 0.0 {
                let fast = u8::from(circ.last_pressure_delta > ONE_ATMOSPHERE);
                states.push((format!("fan_{mode}_{fast}"), false));
            } else {
                states.push((format!("fan_{mode}"), false));
            }
        }
        let overlays: Vec<Overlay> = states
            .iter()
            .filter_map(|(s, emissive)| overlay(icon, s, *emissive))
            .collect();
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
}

pub(super) struct TegPlugin;

impl Plugin for TegPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<TegWindow>()
            .add_systems(PostStartup, init_tegs)
            .add_systems(
                Update,
                (
                    open_on_click
                        .after(ss13_world::WorldSystems::Movement)
                        .after(crate::click::ResolveClicks)
                        .before(UiSystems::Open),
                    teg_looks
                        .run_if(ss13_atmos::air_tick_due)
                        .after(ss13_atmos::AtmosSystems::Machinery),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_atmos::GasId;
    use ss13_atmos::gas::T20C;

    #[test]
    fn circulator_nodes_follow_tg() {
        // Facing north: across east-west, output west (`turn(NORTH, 90)`), input east.
        assert_eq!(circulator_nodes(1, false), (4 | 8, vec![8, 4]));
        assert_eq!(circulator_nodes(1, true), (4 | 8, vec![4, 8]));
        // Facing east: output north, input south.
        assert_eq!(circulator_nodes(4, false), (1 | 2, vec![1, 2]));
    }

    #[test]
    fn exchange_banks_65_percent_of_the_heat_moved() {
        let mut hot = GasMixture::vacuum();
        hot.set(GasId::PLASMA, 10.0);
        hot.temperature = 1000.0;
        let mut cold = GasMixture::vacuum();
        cold.set(GasId::N2, 50.0);
        cold.temperature = 100.0;
        let (ch, cc) = (hot.heat_capacity_raw(), cold.heat_capacity_raw());
        let energy = 900.0 * ch * cc / (ch + cc);
        let banked = exchange(&mut hot, &mut cold);
        assert!((banked - energy * 0.65).abs() < 1e-6);
        assert!((hot.temperature - (1000.0 - energy / ch)).abs() < 1e-9);
        assert!((cold.temperature - (100.0 + energy * 0.35 / cc)).abs() < 1e-9);
        // Cold side hotter: nothing.
        assert_eq!(exchange(&mut cold.clone(), &mut hot.clone()), 0.0);
    }

    #[test]
    fn machine_tick_releases_a_tenth_of_the_bank() {
        let mut grid = Grid::default();
        let mut teg = Teg {
            lastgen: 12_345.0,
            ..default()
        };
        machine_tick(&mut grid, Entity::PLACEHOLDER, &mut teg);
        assert_eq!(teg.lastgenlev, 1234.0);
        assert_eq!(teg.lastgen, 11_111.0);
    }

    #[test]
    fn transfer_needs_10_kpa_and_moves_half_the_difference() {
        let mut net = Pipenet::default();
        let mut dm = DmAtmosDevice {
            path: CIRCULATOR.into(),
            piping_layer: 3.0,
            pipe_color: "#EEEEEE".into(),
            device_type: 2.0,
            ..default()
        };
        dm.pipe_flags = 0.0;
        let (spec, _) =
            crate::atmos_machinery::device_spec(Entity::PLACEHOLDER, TilePos::new(1, 1, 1), &dm, 1)
                .unwrap();
        assert_eq!(spec.node_dirs, vec![8, 4]);
        let id = net.add(spec);
        net.build();
        let fill = |net: &mut Pipenet, port: usize, kpa: f64| {
            let air = net.air_mut(id, port).unwrap();
            air.gas = GasMixture::vacuum();
            air.gas.temperature = T20C;
            air.gas
                .set(GasId::N2, kpa * air.volume / (R_IDEAL_GAS_EQUATION * T20C));
        };
        fill(&mut net, 0, 100.0);
        fill(&mut net, 1, 105.0);
        assert_eq!(return_transfer_air(&mut net, id), (None, 0.0));
        fill(&mut net, 1, 300.0);
        let (moved, delta) = return_transfer_air(&mut net, id);
        assert_eq!(delta, 100.0);
        let want = 100.0 * 200.0 / (T20C * R_IDEAL_GAS_EQUATION);
        assert!((moved.unwrap().total_moles() - want).abs() < 1e-3);
    }
}
