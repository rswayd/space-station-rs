//! Gas chamber monitoring (server): tg `/obj/machinery/air_sensor`, `/obj/machinery/meter/
//! monitored` and `/obj/machinery/computer/atmos_control` (`code/game/machinery/computer/
//! atmos_computers/`).
//!
//! - A sensor belongs to a chamber (`chamber_id`) and at round start links the first injector
//!   (inlet) and vent (outlet) it sees within 4 tiles; a plain (not `monitored`) injector is set
//!   to the maximum rate, a plain vent becomes a full-pressure siphon off its area's alarms.
//!   Without power it reads nothing and shows `gsensor0`.
//! - A console lists its `atmos_chambers` and reads, per chamber, the closest sensor or
//!   monitored meter on its z-level: the gas, and, with `control`, the inlet's power and rate and
//!   the outlet's power and pressure bound. `reconnecting` consoles pick another station chamber
//!   from a list (not the distro or waste loops).
// shortcut: no multitool linking of sensors and ports (or air alarms), no taking a sensor off
// the wall; `oview(4)` is taken nearest first.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use ss13_atmos::pipenet::Pipenet;
use ss13_mapload::{DmAtmosDevice, DmName, IconIndex};
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::atmos_control::{
    AtmosControlData, AtmosControlStatic, AtmosControlUi, Chamber, PortInfo,
};
use ss13_tgui::interfaces::gas_analyzer::AtmosHandbook;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::devices::{
    ATMOS_PUMP_MAX_PRESSURE, EXTERNAL_BOUND, INTERNAL_BOUND, MAX_OUTPUT_PRESSURE,
    MAX_TRANSFER_RATE, OutletInjector, VentPump,
};
use super::meter::Meter;
use crate::obj_damage::is_under;
use crate::power::Unpowered;

const SENSOR: &str = "/obj/machinery/air_sensor";
const MONITORED_METER: &str = "/obj/machinery/meter/monitored";
pub const CONSOLE: &str = "/obj/machinery/computer/atmos_control";
const PLAIN_INJECTOR: &str = "/obj/machinery/atmospherics/components/unary/outlet_injector";
const PLAIN_VENT: &str = "/obj/machinery/atmospherics/components/unary/vent_pump";
const DISTRO: &str = "distro";
const WASTE: &str = "waste";
const SENSOR_RANGE: u32 = 4;

/// One chamber sensor (`air_sensor`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AirSensor {
    pub chamber: String,
    pub inlet: Option<Entity>,
    pub outlet: Option<Entity>,
    pub on: bool,
}

/// A monitored meter's chamber (the distro and waste loops).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct MonitoredMeter {
    pub chamber: String,
}

/// One `atmos_control` console and what its window shows.
#[derive(Component, Clone)]
pub struct AtmosControl {
    /// `atmos_chambers`: (id, name).
    pub chambers: Vec<(String, String)>,
    /// `connected_sensors`: the closest sensor or meter per chamber id.
    pub sensors: HashMap<String, Entity>,
    pub control: bool,
    pub reconnecting: bool,
    pub last_chamber: String,
    /// `GLOB.station_gas_chambers`: what a reconnect may pick.
    pub station_chambers: Vec<(String, String)>,
    pub handbook: Arc<AtmosHandbook>,
    pub data: AtmosControlData,
}

/// A reconnect prompt's console.
#[derive(Component, Clone, Copy, Debug)]
struct ReconnectFor(Entity);

pub fn plugin(app: &mut App) {
    app.register_ui::<ControlWindow>()
        .add_systems(
            Startup,
            init_control
                .after(super::init_atmos_machinery)
                .after(crate::atmos::load_handbook),
        )
        .add_systems(
            Update,
            (
                (control_acts, reconnect_answers)
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                (sensor_power, refresh_consoles).chain(),
            ),
        );
}

fn init_control(world: &mut World) {
    let mut q = world.query::<(Entity, &DmAtmosDevice, &TilePos)>();
    let found: Vec<(Entity, DmAtmosDevice, TilePos)> = q
        .iter(world)
        .filter(|(_, dm, _)| {
            is_under(&dm.path, SENSOR)
                || is_under(&dm.path, MONITORED_METER)
                || is_under(&dm.path, CONSOLE)
        })
        .map(|(e, dm, &p)| (e, dm.clone(), p))
        .collect();
    for (e, dm, pos) in &found {
        if is_under(&dm.path, SENSOR) {
            let (inlet, outlet) = link_ports(world, *pos);
            world.entity_mut(*e).insert(AirSensor {
                chamber: dm.chamber_id.clone(),
                inlet,
                outlet,
                on: true,
            });
        } else if is_under(&dm.path, MONITORED_METER) {
            world.entity_mut(*e).insert(MonitoredMeter {
                chamber: dm.chamber_id.clone(),
            });
        }
    }
    let handbook = world
        .get_resource::<crate::atmos::Handbook>()
        .map(|h| h.0.clone())
        .unwrap_or_default();
    for (e, dm, _) in found
        .iter()
        .filter(|(_, dm, _)| is_under(&dm.path, CONSOLE))
    {
        world.entity_mut(*e).insert(AtmosControl {
            chambers: dm.atmos_chambers.clone(),
            sensors: HashMap::new(),
            control: dm.control,
            reconnecting: dm.reconnecting,
            last_chamber: String::new(),
            station_chambers: dm.station_chambers.clone(),
            handbook: handbook.clone(),
            data: AtmosControlData::default(),
        });
        scan(world, *e);
    }
}

/// `post_machine_initialize`: the first injector and vent in `oview(4)`; plain ones are
/// configured for the chamber.
fn link_ports(world: &mut World, pos: TilePos) -> (Option<Entity>, Option<Entity>) {
    let mut q = world.query::<(
        Entity,
        &TilePos,
        &DmAtmosDevice,
        Has<OutletInjector>,
        Has<VentPump>,
    )>();
    let mut near: Vec<(u32, Entity, String, bool)> = q
        .iter(world)
        .filter(|(_, _, _, injector, vent)| *injector || *vent)
        .filter_map(|(e, p, dm, injector, _)| {
            let d = pos.chebyshev(*p)?;
            (d > 0 && d <= SENSOR_RANGE).then(|| (d, e, dm.path.clone(), injector))
        })
        .collect();
    near.sort_by_key(|&(d, e, ..)| (d, e));
    let (mut inlet, mut outlet) = (None, None);
    for (_, e, path, injector) in near {
        let Some(&target) = world.get::<TilePos>(e) else {
            continue;
        };
        if !crate::mobs::ai::behaviors::can_see(world, pos, target, SENSOR_RANGE) {
            continue;
        }
        if injector && inlet.is_none() {
            if path == PLAIN_INJECTOR
                && let Some(mut i) = world.get_mut::<OutletInjector>(e)
            {
                i.volume_rate = MAX_TRANSFER_RATE;
            }
            inlet = Some(e);
        } else if !injector && outlet.is_none() {
            if path == PLAIN_VENT
                && let Some(mut v) = world.get_mut::<VentPump>(e)
            {
                // A siphon at full pressure, off its area's alarms.
                v.area = None;
                v.releasing = false;
                v.pressure_checks = INTERNAL_BOUND;
                v.internal_pressure_bound = MAX_OUTPUT_PRESSURE;
                v.external_pressure_bound = 0.0;
            }
            outlet = Some(e);
        }
        if inlet.is_some() && outlet.is_some() {
            break;
        }
    }
    let _ = EXTERNAL_BOUND;
    (inlet, outlet)
}

/// `scan`: per chamber id, the closest sensor or monitored meter on the console's z-level.
fn scan(world: &mut World, console: Entity) {
    let Some(&pos) = world.get::<TilePos>(console) else {
        return;
    };
    let mut q = world.query::<(
        Entity,
        &TilePos,
        Option<&AirSensor>,
        Option<&MonitoredMeter>,
    )>();
    let mut closest: HashMap<String, (u32, Entity)> = HashMap::new();
    for (e, p, sensor, meter) in q.iter(world) {
        let chamber = match (sensor, meter) {
            (Some(s), _) => &s.chamber,
            (None, Some(m)) => &m.chamber,
            _ => continue,
        };
        if p.z != pos.z {
            continue;
        }
        let d = pos.chebyshev(*p).unwrap_or(u32::MAX);
        let entry = closest.entry(chamber.clone()).or_insert((d, e));
        if d < entry.0 {
            *entry = (d, e);
        }
    }
    if let Some(mut c) = world.get_mut::<AtmosControl>(console) {
        c.sensors = closest.into_iter().map(|(k, (_, e))| (k, e)).collect();
    }
}

/// `air_sensor/process`: on with power, `gsensor1`; off without, `gsensor0`.
fn sensor_power(
    icons: Option<Res<IconIndex>>,
    mut sensors: Query<(
        &mut AirSensor,
        &ss13_mapload::DmAtom,
        &mut Appearance,
        Has<Unpowered>,
    )>,
) {
    let Some(icons) = icons else { return };
    for (mut sensor, atom, mut appearance, unpowered) in &mut sensors {
        if sensor.on == unpowered {
            sensor.on = !unpowered;
            let state = if sensor.on { "gsensor1" } else { "gsensor0" };
            if let Some(id) = icons.get(&atom.icon, state) {
                appearance.icon_state = id;
            }
        }
    }
}

/// `return_air()` of a sensor (its tile, while on) or a monitored meter (its pipe).
fn sensor_gasmix(
    world: &World,
    sensor: Entity,
    name: &str,
) -> Option<ss13_tgui::interfaces::gas_analyzer::Gasmix> {
    let held = if let Some(s) = world.get::<AirSensor>(sensor) {
        if !s.on {
            return None;
        }
        let pos = world.get::<TilePos>(sensor)?;
        let turf = world
            .resource::<ss13_world::SpatialIndex>()
            .on_tile(*pos)
            .iter()
            .copied()
            .find(|&e| world.get::<ss13_mapload::Turf>(e).is_some())?;
        crate::atmos::analyzable(world, turf)?
    } else {
        let meter = world.get::<Meter>(sensor)?;
        let net = world.resource::<Pipenet>();
        let air = net.pipeline_air(meter.target?, 0)?;
        vec![crate::atmos::Held {
            gas: air.gas.clone(),
            volume: air.volume,
            turf: false,
        }]
    };
    crate::atmos::gasmix_named(name, sensor, held.first()?)
}

/// `ui_data` of every console, taken again each tick.
fn refresh_consoles(world: &mut World) {
    let mut q = world.query::<(Entity, &AtmosControl)>();
    let consoles: Vec<(Entity, Vec<(String, String)>, HashMap<String, Entity>)> = q
        .iter(world)
        .map(|(e, c)| (e, c.chambers.clone(), c.sensors.clone()))
        .collect();
    for (console, chambers, sensors) in consoles {
        let data = AtmosControlData {
            chambers: chambers
                .iter()
                .map(|(id, name)| {
                    let sensor = sensors
                        .get(id)
                        .copied()
                        .filter(|&s| world.get_entity(s).is_ok());
                    let gasmix = sensor.and_then(|s| sensor_gasmix(world, s, name));
                    let ports = sensor.and_then(|s| world.get::<AirSensor>(s));
                    let input_info = ports
                        .and_then(|p| p.inlet)
                        .and_then(|i| world.get::<OutletInjector>(i))
                        .map(|i| PortInfo {
                            active: i.on,
                            amount: i.volume_rate as f32,
                        });
                    let output_info = ports
                        .and_then(|p| p.outlet)
                        .and_then(|o| world.get::<VentPump>(o))
                        .map(|v| PortInfo {
                            active: v.on,
                            amount: v.internal_pressure_bound as f32,
                        });
                    Chamber {
                        id: id.clone(),
                        name: name.clone(),
                        gasmix,
                        input_info,
                        output_info,
                    }
                })
                .collect(),
        };
        if let Some(mut c) = world.get_mut::<AtmosControl>(console)
            && c.data != data
        {
            c.data = data;
        }
    }
}

/// A click on a powered console opens its window.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<AtmosControl>(target).is_none() {
        return false;
    }
    if world.get::<Unpowered>(target).is_none() {
        let title = world.get::<DmName>(target).map_or_else(String::new, |n| {
            crate::vending::plain_name(&n.name).to_owned()
        });
        world.write_message(OpenUi::new::<ControlWindow>(client, mob, target, title));
    }
    true
}

pub struct ControlWindow;

impl UiInterface for ControlWindow {
    const NAME: &'static str = AtmosControlUi::NAME;
    type Data = AtmosControlData;
    type StaticData = AtmosControlStatic;
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlAct {
    ToggleInput(String),
    ToggleOutput(String),
    AdjustInput(String, f64),
    AdjustOutput(String, f64),
    Reconnect,
}

impl ServerUi for ControlWindow {
    type Source = &'static AtmosControl;
    type UserParam = ();
    type Act = ControlAct;

    fn static_data(c: &AtmosControl) -> AtmosControlStatic {
        // `get_default_gas`: the first chamber when it is a gas id.
        let default_gas = c
            .chambers
            .first()
            .map(|(id, _)| id.clone())
            .filter(|id| ss13_atmos::GasId::parse(id).is_some());
        AtmosControlStatic {
            max_input: MAX_TRANSFER_RATE as f32,
            max_output: MAX_OUTPUT_PRESSURE as f32,
            control: c.control,
            reconnecting: c.reconnecting,
            default_gas,
            handbook: (*c.handbook).clone(),
        }
    }

    fn data(c: &AtmosControl, _: Entity, _: &()) -> AtmosControlData {
        c.data.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ControlAct, UiReject> {
        let chamber = || {
            params
                .get("chamber")
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams("chamber".into()))
        };
        Ok(match action {
            "toggle_input" => ControlAct::ToggleInput(chamber()?),
            "toggle_output" => ControlAct::ToggleOutput(chamber()?),
            "adjust_input" => ControlAct::AdjustInput(chamber()?, params.number("rate")?),
            "adjust_output" => ControlAct::AdjustOutput(chamber()?, params.number("rate")?),
            "reconnect" => ControlAct::Reconnect,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `ui_act`: only for consoles with `control` or `reconnecting`, on chambers they list.
fn control_acts(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<UiActed<ControlWindow>>>,
) {
    let acts: Vec<(Entity, Entity, Entity, ControlAct)> = cursor
        .read(world.resource::<bevy::ecs::message::Messages<UiActed<ControlWindow>>>())
        .map(|a| (a.source, a.client, a.actor, a.act.clone()))
        .collect();
    for (console, client, actor, act) in acts {
        let Some(c) = world.get::<AtmosControl>(console).cloned() else {
            continue;
        };
        if !(c.control || c.reconnecting) {
            continue;
        }
        let ports = |chamber: &str| {
            if !c.chambers.iter().any(|(id, _)| id == chamber) {
                return None;
            }
            let sensor = c.sensors.get(chamber)?;
            world.get::<AirSensor>(*sensor).cloned()
        };
        match act {
            ControlAct::ToggleInput(ch) => {
                if let Some(i) = ports(&ch).and_then(|p| p.inlet)
                    && let Some(mut inj) = world.get_mut::<OutletInjector>(i)
                {
                    inj.on = !inj.on;
                }
            }
            ControlAct::ToggleOutput(ch) => {
                if let Some(o) = ports(&ch).and_then(|p| p.outlet)
                    && let Some(mut vent) = world.get_mut::<VentPump>(o)
                {
                    vent.on = !vent.on;
                }
            }
            ControlAct::AdjustInput(ch, rate) => {
                if let Some(i) = ports(&ch).and_then(|p| p.inlet) {
                    let port_volume = world
                        .get::<super::AtmosDevice>(i)
                        .and_then(|d| world.resource::<Pipenet>().air(d.id, 0).map(|a| a.volume))
                        .unwrap_or(MAX_TRANSFER_RATE);
                    if let Some(mut inj) = world.get_mut::<OutletInjector>(i) {
                        let rate = rate.clamp(0.0, MAX_TRANSFER_RATE);
                        inj.volume_rate = rate.clamp(0.0, port_volume.min(MAX_TRANSFER_RATE));
                    }
                }
            }
            ControlAct::AdjustOutput(ch, rate) => {
                if let Some(o) = ports(&ch).and_then(|p| p.outlet)
                    && let Some(mut vent) = world.get_mut::<VentPump>(o)
                {
                    vent.internal_pressure_bound = rate.clamp(0.0, ATMOS_PUMP_MAX_PRESSURE);
                }
            }
            ControlAct::Reconnect => reconnect(world, console, client, actor),
        }
    }
}

/// `reconnect`: rescans, then asks which station chamber to show.
fn reconnect(world: &mut World, console: Entity, client: Entity, actor: Entity) {
    if !world
        .get::<AtmosControl>(console)
        .is_some_and(|c| c.reconnecting)
    {
        return;
    }
    scan(world, console);
    let Some(c) = world.get::<AtmosControl>(console) else {
        return;
    };
    let mut names: Vec<String> = c
        .sensors
        .keys()
        .filter(|id| *id != DISTRO && *id != WASTE)
        .filter_map(|id| c.station_chambers.iter().find(|(k, _)| k == id))
        .map(|(_, name)| name.clone())
        .collect();
    names.sort();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            actor,
            "Reconnect",
            "Select the device set",
            PromptKind::List {
                items: names,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(ReconnectFor(console));
}

fn reconnect_answers(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<PromptAnswered>>,
) {
    let answers: Vec<(Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<bevy::ecs::message::Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in answers {
        let Some(&ReconnectFor(console)) = world.get::<ReconnectFor>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(items) = world
            .get::<ss13_tgui::prompt::Prompt>(prompt)
            .and_then(|p| match &p.kind {
                PromptKind::List { items, .. } => Some(items.clone()),
                _ => None,
            })
        else {
            continue;
        };
        let Some(name) = items.get(i).cloned() else {
            continue;
        };
        let Some(mut c) = world.get_mut::<AtmosControl>(console) else {
            continue;
        };
        let Some(id) = c
            .station_chambers
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(k, _)| k.clone())
        else {
            continue;
        };
        let last = std::mem::take(&mut c.last_chamber);
        c.chambers.retain(|(k, _)| *k != last);
        c.chambers.push((id.clone(), name.clone()));
        c.last_chamber = id;
        let suffix = if c.control { " Control" } else { " Monitor" };
        if let Some(mut n) = world.get_mut::<DmName>(console) {
            n.name = format!("{name}{suffix}");
        }
    }
}
