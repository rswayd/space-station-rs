//! The tgui windows of atmos machines (server halves) and the hand interactions that open them
//! or act on the machines: clicks with an empty hand (`attack_hand` -> `ui_interact`, valves'
//! `interact`), alt-clicks (`click_alt`), ctrl-clicks (`click_ctrl`), and items on machines
//! (wrenching portables to ports, tanks into portables, IDs on air alarms, welding vents).
//!
//! Every act is checked at the moment of effect: the window is interactive this tick (tgui
//! distance rules, power) and the act's values are clamped to tg's ranges; references to vents
//! and scrubbers must be in the alarm's area (tg `locate(ref) in area.air_vents`).

use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_atmos::gas::{ONE_ATMOSPHERE, T0C, T20C};
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::pipenet::air::Mix;
use ss13_atmos::{Atmos, GASES, GasId};
use ss13_chat::Span;
use ss13_mapload::DmName;
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
use ss13_tgui::UiInterface;
use ss13_tgui::UiStatus;
use ss13_tgui::interfaces::air_alarm::{
    AirAlarmData, AirAlarmUi, AlarmControls, EnvEntry, ModeEntry, ScrubberEntry, TlvEntry,
    VentEntry,
};
use ss13_tgui::interfaces::atmos::{
    AtmosFilterData, AtmosFilterUi, AtmosMixerData, AtmosMixerUi, AtmosPumpData, AtmosPumpUi,
    AtmosTempGateData, AtmosTempGateUi, AtmosTempPumpData, AtmosTempPumpUi, CanisterData,
    CanisterStatic, CanisterUi, GasToggle, HoldingTank, PipeScrubberData, PipeScrubberStatic,
    PipeScrubberUi, PortablePumpData, PortablePumpUi, PortableScrubberData, PortableScrubberUi,
};
use ss13_tgui::interfaces::thermo_machine::{ThermoMachineData, ThermoMachineUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

/// The server halves of the shared interfaces (the orphan rule keeps `ServerUi` impls on
/// local types).
macro_rules! window {
    ($(#[$doc:meta])* $name:ident, $ui:ty) => {
        $(#[$doc])*
        pub struct $name;

        impl UiInterface for $name {
            const NAME: &'static str = <$ui as UiInterface>::NAME;
            type Data = <$ui as UiInterface>::Data;
            type StaticData = <$ui as UiInterface>::StaticData;
        }
    };
}

window!(
    /// `AtmosPump`: pumps, passive gates, volume pumps, injectors.
    PumpWindow,
    AtmosPumpUi
);
window!(FilterWindow, AtmosFilterUi);
window!(
    /// `AtmosTempGate`: temperature gates.
    TempGateWindow,
    AtmosTempGateUi
);
window!(
    /// `AtmosTempPump`: temperature pumps.
    TempPumpWindow,
    AtmosTempPumpUi
);
window!(MixerWindow, AtmosMixerUi);
window!(CanisterWindow, CanisterUi);
window!(PortablePumpWindow, PortablePumpUi);
window!(PortableScrubberWindow, PortableScrubberUi);
window!(PipeScrubberWindow, PipeScrubberUi);
window!(AirAlarmWindow, AirAlarmUi);

use super::alarm::{AirAlarm, AlarmMode, TLV_IGNORE, select_mode};
use super::devices::{
    ATMOS_PUMP_MAX_PRESSURE, ConditionalGate, EXTERNAL_BOUND, GasFilter, GasMixer, GasPump,
    GateCondition, INTERNAL_BOUND, MAX_HEAT_TRANSFER_RATE, MAX_OUTPUT_PRESSURE, MAX_TRANSFER_RATE,
    OutletInjector, PassiveGate, TEMPERATURE_GATE_MAX, TEMPERATURE_GATE_MIN, TemperaturePump,
    ThermoMachine, Valve, VentPump, VentScrubber, VolumePump,
};
use super::portable::{
    CAN_DEFAULT_RELEASE_PRESSURE, CAN_MAX_RELEASE_PRESSURE, CAN_MIN_RELEASE_PRESSURE, Canister,
    PUMP_DEFAULT_PRESSURE, PUMP_MAX_PRESSURE, PUMP_MIN_PRESSURE, PortableAtmos, PortablePump,
    PortableScrubber, TANK_FRAGMENT_PRESSURE, TANK_LEAK_PRESSURE,
};
use super::{AreaAtmos, AtmosDevice};
use crate::click::CtrlClick;
use crate::internals::TankAir;
use crate::inventory::Inventory;
use crate::power::Unpowered;
use crate::tools::{Actor, ToolKind, tell, tool_of};

/// tg `turn` of a mixture into the windows' gas lists.
fn gas_toggles(mask: u32) -> Vec<GasToggle> {
    GASES
        .iter()
        .enumerate()
        .map(|(i, g)| GasToggle {
            gas_id: g.id.to_owned(),
            gas_name: g.name.to_owned(),
            enabled: mask & (1 << i) != 0,
        })
        .collect()
}

/// A number param or one of the named presets (`"max"`, `"min"`, `"reset"`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Amount {
    Value(f64),
    Max,
    Min,
    Reset,
}

fn amount(params: &UiParams, key: &str) -> Result<Amount, UiReject> {
    match params.get(key).map(str::trim) {
        Some("max") => Ok(Amount::Max),
        Some("min") => Ok(Amount::Min),
        Some("reset") => Ok(Amount::Reset),
        _ => params.number(key).map(Amount::Value),
    }
}

fn resolve(a: Amount, min: f64, max: f64, reset: f64) -> f64 {
    match a {
        Amount::Value(v) => v.clamp(min, max),
        Amount::Max => max,
        Amount::Min => min,
        Amount::Reset => reset,
    }
}

// --- AtmosPump: pumps, passive gates, volume pumps, injectors -----------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PumpAct {
    Power,
    Pressure(Amount),
    Rate(Amount),
}

impl ServerUi for PumpWindow {
    type Source = (
        Option<&'static GasPump>,
        Option<&'static PassiveGate>,
        Option<&'static VolumePump>,
        Option<&'static OutletInjector>,
        Option<&'static ConditionalGate>,
    );
    type UserParam = ();
    type Act = PumpAct;

    fn static_data(
        _: (
            Option<&GasPump>,
            Option<&PassiveGate>,
            Option<&VolumePump>,
            Option<&OutletInjector>,
            Option<&ConditionalGate>,
        ),
    ) {
    }

    fn data(
        (pump, gate, volume, injector, valve): (
            Option<&GasPump>,
            Option<&PassiveGate>,
            Option<&VolumePump>,
            Option<&OutletInjector>,
            Option<&ConditionalGate>,
        ),
        _: Entity,
        _: &(),
    ) -> AtmosPumpData {
        let round = |v: f64| v.round() as f32;
        if let Some(p) = pump {
            AtmosPumpData {
                on: p.on,
                pressure: round(p.target_pressure),
                max_pressure: round(MAX_OUTPUT_PRESSURE),
                ..default()
            }
        } else if let Some(g) = gate {
            AtmosPumpData {
                on: g.on,
                pressure: round(g.target_pressure),
                max_pressure: round(MAX_OUTPUT_PRESSURE),
                ..default()
            }
        } else if let Some(v) = volume {
            AtmosPumpData {
                on: v.on,
                rate: round(v.transfer_rate),
                max_rate: round(MAX_TRANSFER_RATE),
                ..default()
            }
        } else if let Some(i) = injector {
            AtmosPumpData {
                on: i.on,
                rate: round(i.volume_rate),
                max_rate: round(MAX_TRANSFER_RATE),
                ..default()
            }
        } else if let Some(ConditionalGate {
            on,
            condition: GateCondition::Pressure { target },
            ..
        }) = valve
        {
            AtmosPumpData {
                on: *on,
                pressure: round(*target),
                max_pressure: round(ONE_ATMOSPHERE * 100.0),
                ..default()
            }
        } else {
            AtmosPumpData::default()
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PumpAct, UiReject> {
        match action {
            "power" => Ok(PumpAct::Power),
            "pressure" => amount(params, "pressure").map(PumpAct::Pressure),
            "rate" => amount(params, "rate").map(PumpAct::Rate),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

type PumpQuery<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static mut GasPump>,
        Option<&'static mut PassiveGate>,
        Option<&'static mut VolumePump>,
        Option<&'static mut OutletInjector>,
        Option<&'static mut ConditionalGate>,
    ),
>;

fn apply_pump_acts(mut acts: MessageReader<UiActed<PumpWindow>>, mut machines: PumpQuery) {
    for acted in acts.read() {
        let Ok((pump, gate, volume, injector, valve)) = machines.get_mut(acted.source) else {
            continue;
        };
        // `pressure_valve/ui_act`: 0..100 atmospheres, "max" is 100 atmospheres.
        if let Some(mut v) = valve {
            let ConditionalGate {
                on,
                condition: GateCondition::Pressure { target },
                ..
            } = &mut *v
            else {
                continue;
            };
            match acted.act {
                PumpAct::Power => *on = !*on,
                PumpAct::Pressure(a) => {
                    let top = ONE_ATMOSPHERE * 100.0;
                    *target = resolve(a, 0.0, top, *target);
                }
                PumpAct::Rate(_) => {}
            }
            continue;
        }
        match acted.act {
            PumpAct::Power => {
                if let Some(mut p) = pump {
                    p.on = !p.on;
                } else if let Some(mut g) = gate {
                    g.on = !g.on;
                } else if let Some(mut v) = volume {
                    v.on = !v.on;
                } else if let Some(mut i) = injector {
                    i.on = !i.on;
                }
            }
            PumpAct::Pressure(a) => {
                if let Some(mut p) = pump {
                    p.target_pressure = resolve(a, 0.0, MAX_OUTPUT_PRESSURE, p.target_pressure);
                } else if let Some(mut g) = gate {
                    // tg clamps a passive gate to 100 atmospheres, "max" to MAX_OUTPUT_PRESSURE.
                    g.target_pressure = match a {
                        Amount::Max => MAX_OUTPUT_PRESSURE,
                        a => resolve(a, 0.0, ONE_ATMOSPHERE * 100.0, g.target_pressure),
                    };
                }
            }
            PumpAct::Rate(a) => {
                if let Some(mut v) = volume {
                    v.transfer_rate = resolve(a, 0.0, MAX_TRANSFER_RATE, v.transfer_rate);
                } else if let Some(mut i) = injector {
                    i.volume_rate = resolve(a, 0.0, MAX_TRANSFER_RATE, i.volume_rate);
                }
            }
        }
    }
}

// --- AtmosTempGate, AtmosTempPump ----------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TempAct {
    Power,
    /// `temperature` (gates) or `rate` (pumps).
    Set(Amount),
}

impl ServerUi for TempGateWindow {
    type Source = &'static ConditionalGate;
    type UserParam = ();
    type Act = TempAct;

    fn static_data(_: &ConditionalGate) {}

    fn data(gate: &ConditionalGate, _: Entity, _: &()) -> AtmosTempGateData {
        let target = match gate.condition {
            GateCondition::Temperature { target, .. } => target,
            GateCondition::Pressure { .. } => 0.0,
        };
        AtmosTempGateData {
            on: gate.on,
            temperature: target.round() as f32,
            min_temperature: TEMPERATURE_GATE_MIN.round() as f32,
            max_temperature: TEMPERATURE_GATE_MAX.round() as f32,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<TempAct, UiReject> {
        match action {
            "power" => Ok(TempAct::Power),
            "temperature" => amount(params, "temperature").map(TempAct::Set),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

impl ServerUi for TempPumpWindow {
    type Source = &'static TemperaturePump;
    type UserParam = ();
    type Act = TempAct;

    fn static_data(_: &TemperaturePump) {}

    fn data(pump: &TemperaturePump, _: Entity, _: &()) -> AtmosTempPumpData {
        AtmosTempPumpData {
            on: pump.on,
            rate: pump.heat_transfer_rate.round() as f32,
            max_heat_transfer_rate: MAX_HEAT_TRANSFER_RATE.round() as f32,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<TempAct, UiReject> {
        match action {
            "power" => Ok(TempAct::Power),
            "rate" => amount(params, "rate").map(TempAct::Set),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn apply_temp_gate_acts(
    mut acts: MessageReader<UiActed<TempGateWindow>>,
    mut gates: Query<&mut ConditionalGate>,
) {
    for acted in acts.read() {
        let Ok(mut gate) = gates.get_mut(acted.source) else {
            continue;
        };
        let ConditionalGate {
            on,
            condition: GateCondition::Temperature { target, .. },
            ..
        } = &mut *gate
        else {
            continue;
        };
        match acted.act {
            TempAct::Power => *on = !*on,
            TempAct::Set(a) => {
                *target = resolve(a, TEMPERATURE_GATE_MIN, TEMPERATURE_GATE_MAX, *target);
            }
        }
    }
}

fn apply_temp_pump_acts(
    mut acts: MessageReader<UiActed<TempPumpWindow>>,
    mut pumps: Query<&mut TemperaturePump>,
) {
    for acted in acts.read() {
        let Ok(mut pump) = pumps.get_mut(acted.source) else {
            continue;
        };
        match acted.act {
            TempAct::Power => pump.on = !pump.on,
            TempAct::Set(a) => {
                pump.heat_transfer_rate =
                    resolve(a, 0.0, MAX_HEAT_TRANSFER_RATE, pump.heat_transfer_rate);
            }
        }
    }
}

// --- AtmosFilter ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum FilterAct {
    Power,
    Rate(Amount),
    Toggle(GasId),
}

impl ServerUi for FilterWindow {
    type Source = &'static GasFilter;
    type UserParam = ();
    type Act = FilterAct;

    fn static_data(_: &GasFilter) {}

    fn data(f: &GasFilter, _: Entity, _: &()) -> AtmosFilterData {
        AtmosFilterData {
            on: f.on,
            rate: f.transfer_rate.round() as f32,
            max_rate: MAX_TRANSFER_RATE as f32,
            filter_types: gas_toggles(f.filter),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<FilterAct, UiReject> {
        match action {
            "power" => Ok(FilterAct::Power),
            "rate" => amount(params, "rate").map(FilterAct::Rate),
            "toggle_filter" => gas_param(params).map(FilterAct::Toggle),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn gas_param(params: &UiParams) -> Result<GasId, UiReject> {
    params
        .get("val")
        .and_then(GasId::parse)
        .ok_or_else(|| UiReject::BadParams("val: expected a gas id".into()))
}

fn apply_filter_acts(
    mut acts: MessageReader<UiActed<FilterWindow>>,
    mut filters: Query<&mut GasFilter>,
) {
    for acted in acts.read() {
        let Ok(mut f) = filters.get_mut(acted.source) else {
            continue;
        };
        match acted.act {
            FilterAct::Power => f.on = !f.on,
            FilterAct::Rate(a) => {
                f.transfer_rate = resolve(a, 0.0, MAX_TRANSFER_RATE, f.transfer_rate);
            }
            FilterAct::Toggle(g) => f.filter ^= 1 << g.0,
        }
    }
}

// --- AtmosMixer -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MixerAct {
    Power,
    Pressure(Amount),
    Node1(f64),
    Node2(f64),
}

impl ServerUi for MixerWindow {
    type Source = &'static GasMixer;
    type UserParam = ();
    type Act = MixerAct;

    fn static_data(_: &GasMixer) {}

    fn data(m: &GasMixer, _: Entity, _: &()) -> AtmosMixerData {
        AtmosMixerData {
            on: m.on,
            set_pressure: m.target_pressure.round() as f32,
            max_pressure: MAX_OUTPUT_PRESSURE as f32,
            node1_concentration: (m.node1_concentration * 100.0).round() as f32,
            node2_concentration: (m.node2_concentration * 100.0).round() as f32,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<MixerAct, UiReject> {
        match action {
            "power" => Ok(MixerAct::Power),
            "pressure" => amount(params, "pressure").map(MixerAct::Pressure),
            "node1" => params.number("concentration").map(MixerAct::Node1),
            "node2" => params.number("concentration").map(MixerAct::Node2),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn apply_mixer_acts(
    mut acts: MessageReader<UiActed<MixerWindow>>,
    mut mixers: Query<&mut GasMixer>,
) {
    for acted in acts.read() {
        let Ok(mut m) = mixers.get_mut(acted.source) else {
            continue;
        };
        // `adjust_node1_value`: tg takes the percentage unclamped; 0..100 keeps the mix sane.
        let set_node1 = |m: &mut GasMixer, percent: f64| {
            m.node1_concentration = percent.clamp(0.0, 100.0) / 100.0;
            m.node2_concentration = 1.0 - m.node1_concentration;
        };
        match acted.act {
            MixerAct::Power => m.on = !m.on,
            MixerAct::Pressure(a) => {
                m.target_pressure = resolve(a, 0.0, MAX_OUTPUT_PRESSURE, m.target_pressure);
            }
            MixerAct::Node1(v) => set_node1(&mut m, v),
            MixerAct::Node2(v) => set_node1(&mut m, 100.0 - v),
        }
    }
}

// --- ThermoMachine --------------------------------------------------------------------------

/// The game's side of `ThermoMachine` (the interface's own server half is a demo stub).
pub struct ThermoWindow;

impl UiInterface for ThermoWindow {
    const NAME: &'static str = ThermoMachineUi::NAME;
    type Data = ThermoMachineData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThermoAct {
    Power,
    Target(f64),
}

impl ServerUi for ThermoWindow {
    type Source = (&'static ThermoMachine, &'static AtmosDevice);
    type UserParam = Res<'static, Pipenet>;
    type Act = ThermoAct;

    fn static_data(_: (&ThermoMachine, &AtmosDevice)) {}

    fn data(
        (m, device): (&ThermoMachine, &AtmosDevice),
        _: Entity,
        net: &SystemParamItem<'_, '_, Res<'static, Pipenet>>,
    ) -> ThermoMachineData {
        let port = net.air(device.id, 0);
        ThermoMachineData {
            on: m.on,
            min: m.min_temperature as f32,
            max: m.max_temperature as f32,
            target: m.target_temperature as f32,
            initial: T20C as f32,
            temperature: port.map_or(0.0, |p| p.temperature()) as f32,
            pressure: port.map_or(0.0, |p| p.pressure()) as f32,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ThermoAct, UiReject> {
        match action {
            "power" => Ok(ThermoAct::Power),
            // DM also takes `adjust=<delta>` and `target=input`; this interface sends neither.
            "target" => params.number("target").map(ThermoAct::Target),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn apply_thermo_acts(
    mut acts: MessageReader<UiActed<ThermoWindow>>,
    mut machines: Query<&mut ThermoMachine>,
) {
    for acted in acts.read() {
        let Ok(mut m) = machines.get_mut(acted.source) else {
            continue;
        };
        match acted.act {
            ThermoAct::Power => m.on = !m.on,
            ThermoAct::Target(t) => {
                m.target_temperature = t.clamp(m.min_temperature, m.max_temperature);
            }
        }
    }
}

// --- Canister and portables -----------------------------------------------------------------

type PortableUser = (
    Res<'static, Pipenet>,
    Query<'static, 'static, (&'static TankAir, Option<&'static DmName>)>,
    Query<'static, 'static, &'static crate::mecha::core::ItemCell>,
);

fn holding_tank(
    portable: &PortableAtmos,
    tanks: &Query<(&TankAir, Option<&DmName>)>,
) -> Option<HoldingTank> {
    let (tank, name) = tanks.get(portable.holding?).ok()?;
    Some(HoldingTank {
        name: name.map_or_else(
            || "tank".to_owned(),
            |n| crate::vending::plain_name(&n.name).to_owned(),
        ),
        pressure: tank.pressure().round() as f32,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanisterAct {
    Pressure(Amount),
    Valve,
    Eject,
    Shielding,
    Suppression,
    Relabel,
}

impl ServerUi for CanisterWindow {
    type Source = (
        &'static Canister,
        &'static PortableAtmos,
        Option<&'static super::portable::NobCrystal>,
    );
    type UserParam = PortableUser;
    type Act = CanisterAct;

    fn static_data(
        (canister, ..): (
            &Canister,
            &PortableAtmos,
            Option<&super::portable::NobCrystal>,
        ),
    ) -> CanisterStatic {
        CanisterStatic {
            default_release_pressure: CAN_DEFAULT_RELEASE_PRESSURE.round() as f32,
            min_release_pressure: CAN_MIN_RELEASE_PRESSURE.round() as f32,
            max_release_pressure: CAN_MAX_RELEASE_PRESSURE.round() as f32,
            pressure_limit: canister.pressure_limit.round() as f32,
            holding_tank_leak_pressure: TANK_LEAK_PRESSURE.round() as f32,
            holding_tank_frag_pressure: TANK_FRAGMENT_PRESSURE.round() as f32,
        }
    }

    fn data(
        (canister, portable, crystal): (
            &Canister,
            &PortableAtmos,
            Option<&super::portable::NobCrystal>,
        ),
        _: Entity,
        (net, tanks, cells): &SystemParamItem<'_, '_, PortableUser>,
    ) -> CanisterData {
        CanisterData {
            port_connected: portable.port.is_some(),
            tank_pressure: portable.air(net).map_or(0.0, Mix::pressure).round() as f32,
            release_pressure: canister.release_pressure.round() as f32,
            valve_open: canister.valve_open,
            holding_tank: holding_tank(portable, tanks),
            shielding: canister.shielding,
            // `internal_cell.percent()`.
            cell_charge: canister
                .cell
                .and_then(|c| cells.get(c).ok())
                .map_or(0.0, |c| c.0.percent()) as f32,
            has_hypernob_crystal: crystal.is_some(),
            reaction_suppression_enabled: crystal.is_some_and(|c| c.suppress),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CanisterAct, UiReject> {
        match action {
            "pressure" => amount(params, "pressure").map(CanisterAct::Pressure),
            "valve" => Ok(CanisterAct::Valve),
            "eject" => Ok(CanisterAct::Eject),
            "shielding" => Ok(CanisterAct::Shielding),
            "reaction_suppression" => Ok(CanisterAct::Suppression),
            "relabel" => Ok(CanisterAct::Relabel),
            // `recolor`: not ported.
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn apply_canister_acts(mut acts: MessageReader<UiActed<CanisterWindow>>, mut commands: Commands) {
    for acted in acts.read() {
        let (source, client, actor, act) = (acted.source, acted.client, acted.actor, acted.act);
        commands.queue(move |world: &mut World| {
            let Some(mut canister) = world.get_mut::<Canister>(source) else {
                return;
            };
            match act {
                CanisterAct::Pressure(a) => {
                    canister.release_pressure = resolve(
                        a,
                        CAN_MIN_RELEASE_PRESSURE,
                        CAN_MAX_RELEASE_PRESSURE,
                        CAN_DEFAULT_RELEASE_PRESSURE,
                    );
                }
                CanisterAct::Valve => {
                    canister.valve_open = !canister.valve_open;
                    let text = if canister.valve_open {
                        "opened"
                    } else {
                        "closed"
                    };
                    info!("canister {source:?} valve {text} by {actor:?}");
                }
                CanisterAct::Eject => {
                    eject_tank(world, source, actor);
                }
                // `toggle_shielding`.
                CanisterAct::Shielding => {
                    canister.shielding = !canister.shielding;
                    let text = if canister.shielding { "on" } else { "off" };
                    info!("canister {source:?} shielding turned {text} by {actor:?}");
                }
                CanisterAct::Suppression => {
                    super::portable::toggle_reaction_suppression(world, source, actor);
                }
                CanisterAct::Relabel => {
                    super::canister::ask_relabel(world, client, actor, source);
                }
            }
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PortablePumpAct {
    Power,
    Direction,
    Pressure(Amount),
    Eject,
    Suppression,
}

impl ServerUi for PortablePumpWindow {
    type Source = (
        &'static PortablePump,
        &'static PortableAtmos,
        Option<&'static super::portable::NobCrystal>,
    );
    type UserParam = PortableUser;
    type Act = PortablePumpAct;

    fn static_data(
        _: (
            &PortablePump,
            &PortableAtmos,
            Option<&super::portable::NobCrystal>,
        ),
    ) {
    }

    fn data(
        (pump, portable, crystal): (
            &PortablePump,
            &PortableAtmos,
            Option<&super::portable::NobCrystal>,
        ),
        _: Entity,
        (net, tanks, _): &SystemParamItem<'_, '_, PortableUser>,
    ) -> PortablePumpData {
        PortablePumpData {
            on: pump.on,
            direction: pump.pump_in,
            connected: portable.port.is_some(),
            pressure: portable.air(net).map_or(0.0, Mix::pressure).round() as f32,
            target_pressure: pump.target_pressure.round() as f32,
            default_pressure: PUMP_DEFAULT_PRESSURE.round() as f32,
            min_pressure: PUMP_MIN_PRESSURE.round() as f32,
            max_pressure: PUMP_MAX_PRESSURE.round() as f32,
            holding: holding_tank(portable, tanks),
            has_hypernob_crystal: crystal.is_some(),
            reaction_suppression_enabled: crystal.is_some_and(|c| c.suppress),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PortablePumpAct, UiReject> {
        match action {
            "power" => Ok(PortablePumpAct::Power),
            "direction" => Ok(PortablePumpAct::Direction),
            "pressure" => amount(params, "pressure").map(PortablePumpAct::Pressure),
            "eject" => Ok(PortablePumpAct::Eject),
            "reaction_suppression" => Ok(PortablePumpAct::Suppression),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn apply_portable_pump_acts(
    mut acts: MessageReader<UiActed<PortablePumpWindow>>,
    mut commands: Commands,
) {
    for acted in acts.read() {
        let (source, actor, act) = (acted.source, acted.actor, acted.act);
        commands.queue(move |world: &mut World| {
            let Some(mut pump) = world.get_mut::<PortablePump>(source) else {
                return;
            };
            match act {
                PortablePumpAct::Power => pump.on = !pump.on,
                PortablePumpAct::Direction => pump.pump_in = !pump.pump_in,
                PortablePumpAct::Pressure(a) => {
                    pump.target_pressure = resolve(
                        a,
                        PUMP_MIN_PRESSURE,
                        PUMP_MAX_PRESSURE,
                        PUMP_DEFAULT_PRESSURE,
                    );
                }
                PortablePumpAct::Eject => {
                    eject_tank(world, source, actor);
                }
                PortablePumpAct::Suppression => {
                    super::portable::toggle_reaction_suppression(world, source, actor);
                }
            }
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PortableScrubberAct {
    Power,
    Toggle(GasId),
    Eject,
    Suppression,
}

impl ServerUi for PortableScrubberWindow {
    type Source = (
        &'static PortableScrubber,
        &'static PortableAtmos,
        Option<&'static super::portable::NobCrystal>,
    );
    type UserParam = PortableUser;
    type Act = PortableScrubberAct;

    fn static_data(
        _: (
            &PortableScrubber,
            &PortableAtmos,
            Option<&super::portable::NobCrystal>,
        ),
    ) {
    }

    fn data(
        (scrubber, portable, crystal): (
            &PortableScrubber,
            &PortableAtmos,
            Option<&super::portable::NobCrystal>,
        ),
        _: Entity,
        (net, tanks, _): &SystemParamItem<'_, '_, PortableUser>,
    ) -> PortableScrubberData {
        PortableScrubberData {
            on: scrubber.on,
            connected: portable.port.is_some(),
            pressure: portable.air(net).map_or(0.0, Mix::pressure).round() as f32,
            filter_types: gas_toggles(scrubber.filter),
            holding: holding_tank(portable, tanks),
            has_hypernob_crystal: crystal.is_some(),
            reaction_suppression_enabled: crystal.is_some_and(|c| c.suppress),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PortableScrubberAct, UiReject> {
        match action {
            "power" => Ok(PortableScrubberAct::Power),
            "toggle_filter" => gas_param(params).map(PortableScrubberAct::Toggle),
            "eject" => Ok(PortableScrubberAct::Eject),
            "reaction_suppression" => Ok(PortableScrubberAct::Suppression),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn apply_portable_scrubber_acts(
    mut acts: MessageReader<UiActed<PortableScrubberWindow>>,
    mut commands: Commands,
) {
    for acted in acts.read() {
        let (source, actor, act) = (acted.source, acted.actor, acted.act);
        commands.queue(move |world: &mut World| {
            let Some(mut scrubber) = world.get_mut::<PortableScrubber>(source) else {
                return;
            };
            match act {
                PortableScrubberAct::Power => scrubber.on = !scrubber.on,
                PortableScrubberAct::Toggle(g) => scrubber.filter ^= 1 << g.0,
                PortableScrubberAct::Eject => {
                    eject_tank(world, source, actor);
                }
                PortableScrubberAct::Suppression => {
                    super::portable::toggle_reaction_suppression(world, source, actor);
                }
            }
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PipeScrubberAct {
    Power,
    Direction,
    Toggle(GasId),
    Suppression,
}

impl ServerUi for PipeScrubberWindow {
    type Source = (
        &'static super::pipe_scrubber::PipeScrubber,
        &'static PortableAtmos,
        Option<&'static super::portable::NobCrystal>,
    );
    type UserParam = PortableUser;
    type Act = PipeScrubberAct;

    fn static_data(
        (scrubber, ..): (
            &super::pipe_scrubber::PipeScrubber,
            &PortableAtmos,
            Option<&super::portable::NobCrystal>,
        ),
    ) -> PipeScrubberStatic {
        PipeScrubberStatic {
            pressure_limit_pump: super::damage::PRESSURE_LIMIT as f32,
            pressure_limit_tank: scrubber.tank.pressure_limit as f32,
        }
    }

    fn data(
        (scrubber, portable, crystal): (
            &super::pipe_scrubber::PipeScrubber,
            &PortableAtmos,
            Option<&super::portable::NobCrystal>,
        ),
        _: Entity,
        (net, ..): &SystemParamItem<'_, '_, PortableUser>,
    ) -> PipeScrubberData {
        PipeScrubberData {
            on: scrubber.on,
            direction: scrubber.pump_in,
            connected: portable.port.is_some(),
            pressure_tank: scrubber.tank.air.pressure().round() as f32,
            pressure_pump: portable.air(net).map_or(0.0, Mix::pressure).round() as f32,
            // The button toggles the internal canister's suppression, so it shows that one
            // (tg shows the scrubber's own, which the button never changes).
            has_hypernob_crystal: crystal.is_some() || scrubber.tank.crystal,
            reaction_suppression_enabled: scrubber.tank.suppress,
            filter_types: gas_toggles(scrubber.filter),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PipeScrubberAct, UiReject> {
        match action {
            "power" => Ok(PipeScrubberAct::Power),
            "direction" => Ok(PipeScrubberAct::Direction),
            "toggle_filter" => gas_param(params).map(PipeScrubberAct::Toggle),
            "reaction_suppression" => Ok(PipeScrubberAct::Suppression),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn apply_pipe_scrubber_acts(
    mut acts: MessageReader<UiActed<PipeScrubberWindow>>,
    mut scrubbers: Query<&mut super::pipe_scrubber::PipeScrubber>,
) {
    for acted in acts.read() {
        let Ok(mut s) = scrubbers.get_mut(acted.source) else {
            continue;
        };
        match acted.act {
            PipeScrubberAct::Power => s.on = !s.on,
            PipeScrubberAct::Direction => s.pump_in = !s.pump_in,
            PipeScrubberAct::Toggle(g) => s.filter ^= 1 << g.0,
            // The internal canister's; refused without its crystal (tg logs an href exploit).
            PipeScrubberAct::Suppression => {
                if s.tank.crystal {
                    s.tank.suppress = !s.tank.suppress;
                    let text = if s.tank.suppress { "on" } else { "off" };
                    info!(
                        "{:?} turned {text} the reaction suppression of {:?}'s tank",
                        acted.actor, acted.source
                    );
                } else {
                    warn!(
                        "{:?} toggled reaction suppression on {:?} without a crystal",
                        acted.actor, acted.source
                    );
                }
            }
        }
    }
}

/// `replace_tank(user, close_valve)` without a new tank: the held tank to the user's hands
/// (or the floor); a pump or scrubber turns off, a canister closes its valve when it was
/// releasing into it (`unregister_holding`).
pub fn eject_tank(world: &mut World, portable: Entity, user: Entity) -> bool {
    let Some(tank) = world.get::<PortableAtmos>(portable).and_then(|p| p.holding) else {
        return false;
    };
    if let Some(mut p) = world.get_mut::<PortableAtmos>(portable) {
        p.holding = None;
    }
    if let Some(mut pump) = world.get_mut::<PortablePump>(portable) {
        pump.on = false;
    }
    if let Some(mut scrubber) = world.get_mut::<PortableScrubber>(portable) {
        scrubber.on = false;
    }
    if let Some(mut canister) = world.get_mut::<Canister>(portable) {
        canister.valve_open = false;
    }
    give_to_hands(world, user, tank, portable);
    true
}

/// tg `put_in_hands` when adjacent, else onto the machine's tile.
fn give_to_hands(world: &mut World, user: Entity, item: Entity, from: Entity) {
    let near = match (world.get::<TilePos>(user), world.get::<TilePos>(from)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    let hand = world.get::<Inventory>(user).and_then(|inv| {
        [inv.active, 1 - inv.active]
            .into_iter()
            .find(|&h| inv.hands[h].is_none())
    });
    match hand.filter(|_| near) {
        Some(hand) => {
            world
                .entity_mut(item)
                .remove::<(crate::storage::StoredIn, TilePos)>()
                .insert(crate::inventory::CarriedBy(user));
            if let Some(mut inv) = world.get_mut::<Inventory>(user) {
                inv.hands[hand] = Some(item);
            }
        }
        None => {
            if let Some(&pos) = world.get::<TilePos>(from) {
                crate::storage::take_out(world, item, pos);
            }
        }
    }
}

// --- AirAlarm -------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum AlarmAct {
    Power {
        r: Entity,
        on: bool,
    },
    Direction {
        r: Entity,
        releasing: bool,
    },
    /// `overclock`: a vent's `toggle_overclock`.
    Overclock {
        r: Entity,
    },
    InCheck {
        r: Entity,
        checks: u8,
    },
    ExCheck {
        r: Entity,
        checks: u8,
    },
    SetInternal {
        r: Entity,
        value: f64,
    },
    ResetInternal {
        r: Entity,
    },
    SetExternal {
        r: Entity,
        value: f64,
    },
    ResetExternal {
        r: Entity,
    },
    Scrubbing {
        r: Entity,
        on: bool,
    },
    Widenet {
        r: Entity,
        on: bool,
    },
    ToggleFilter {
        r: Entity,
        gas: GasId,
    },
    Mode(AlarmMode),
    SetThreshold {
        id: String,
        bits: u8,
        value: f64,
    },
    ResetThreshold {
        id: String,
        bits: u8,
    },
    Alarm,
    Reset,
    Lock,
    DisconnectSensor,
}

type AlarmUser = (
    Res<'static, Atmos>,
    Res<'static, AreaAtmos>,
    Query<
        'static,
        'static,
        (
            &'static VentPump,
            Option<&'static DmName>,
            Option<&'static crate::obj_damage::Integrity>,
            Option<&'static ss13_mapload::DmObj>,
        ),
    >,
    Query<'static, 'static, (&'static VentScrubber, Option<&'static DmName>)>,
    Query<'static, 'static, &'static DmName>,
    crate::silicon::interact::SiliconUsers<'static, 'static>,
    Query<
        'static,
        'static,
        (&'static TilePos, &'static super::control::AirSensor),
        Without<AirAlarm>,
    >,
);

/// tg `REF(vent)` stand-in: the entity bits.
fn entity_ref(e: Entity) -> String {
    e.to_bits().to_string()
}

fn ref_param(params: &UiParams) -> Result<Entity, UiReject> {
    params
        .get("ref")
        .and_then(|r| r.trim().parse::<u64>().ok())
        .and_then(Entity::try_from_bits)
        .ok_or_else(|| UiReject::BadParams("ref: expected a device".into()))
}

/// `!!params["val"]` of a tgui number (`Number(!power)`).
fn flag(params: &UiParams) -> Result<bool, UiReject> {
    params.number("val").map(|v| v != 0.0)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

impl ServerUi for AirAlarmWindow {
    type Source = (&'static AirAlarm, &'static TilePos);
    type UserParam = AlarmUser;
    type Act = AlarmAct;

    fn static_data(_: (&AirAlarm, &TilePos)) {}

    fn data(
        (alarm, &pos): (&AirAlarm, &TilePos),
        user: Entity,
        (atmos, area_atmos, vents, scrubbers, names, silicons, sensors): &SystemParamItem<
            '_,
            '_,
            AlarmUser,
        >,
    ) -> AirAlarmData {
        let silicon = silicons.contains(user);
        let mut env_data = Vec::new();
        if alarm.sensor.is_some() {
            env_data.push(EnvEntry {
                name: "Linked area".into(),
                value: alarm
                    .area
                    .and_then(|a| names.get(a).ok())
                    .map(|n| crate::vending::plain_name(&n.name).to_owned())
                    .unwrap_or_default(),
                danger: None,
            });
        }
        let sensed = super::alarm::sensed_tile(alarm, pos, sensors);
        if let Some(env) = sensed.and_then(|p| atmos.air(p)) {
            let total = env.total_moles();
            let (temp, pressure) = (env.temperature, env.pressure());
            env_data.push(EnvEntry {
                name: "Pressure".into(),
                value: format!("{} kPa", round2(pressure)),
                danger: Some(alarm.pressure.check(pressure)),
            });
            env_data.push(EnvEntry {
                name: "Temperature".into(),
                value: format!(
                    "{} Kelvin / {} Celcius",
                    round2(temp),
                    round2(round2(temp) - T0C)
                ),
                danger: Some(alarm.temperature.check(temp)),
            });
            if total > 0.0 {
                for (g, info) in GASES.iter().enumerate() {
                    let moles = env.moles[g];
                    if moles == 0.0 {
                        continue;
                    }
                    let portion = moles / total;
                    env_data.push(EnvEntry {
                        name: info.name.to_owned(),
                        value: format!(
                            "{} moles / {} % / {} kPa",
                            round2(moles),
                            round2(100.0 * portion),
                            round2(portion * pressure)
                        ),
                        danger: Some(alarm.gases[g].check(portion * pressure)),
                    });
                }
            }
        }
        let tlv = |id: &str, name: &str, unit: &str, t: &super::alarm::Tlv| TlvEntry {
            id: id.to_owned(),
            name: name.to_owned(),
            unit: unit.to_owned(),
            hazard_min: t.hazard_min as f32,
            warning_min: t.warning_min as f32,
            warning_max: t.warning_max as f32,
            hazard_max: t.hazard_max as f32,
        };
        let mut tlv_settings = vec![
            tlv("pressure", "Pressure", "kPa", &alarm.pressure),
            tlv("temperature", "Temperature", "K", &alarm.temperature),
        ];
        tlv_settings.extend(
            GASES
                .iter()
                .zip(&alarm.gases)
                .map(|(info, t)| tlv(info.id, info.name, "kPa", t)),
        );
        let controls = (!alarm.locked || silicon).then(|| {
            let devices = alarm.area.and_then(|a| area_atmos.0.get(&a));
            let area_name = alarm
                .area
                .and_then(|a| names.get(a).ok())
                .map(|n| crate::vending::plain_name(&n.name).to_owned())
                .unwrap_or_default();
            let long_name = |name: Option<&DmName>, i: usize| {
                let base = name.map_or("device", |n| crate::vending::plain_name(&n.name));
                format!("{area_name} {base} {}", i + 1)
            };
            AlarmControls {
                vents: devices
                    .into_iter()
                    .flat_map(|d| d.vents.iter())
                    .enumerate()
                    .filter_map(|(i, &e)| {
                        let (v, name, integrity, obj) = vents.get(e).ok()?;
                        let integrity = match (integrity, obj) {
                            (Some(i), Some(o)) if o.max_integrity > 0.0 => {
                                (f64::from(i.current / o.max_integrity) * 100.0).round() / 100.0
                            }
                            _ => 1.0,
                        };
                        Some(VentEntry {
                            ref_id: entity_ref(e),
                            long_name: long_name(name, i),
                            power: v.on,
                            overclock: v.overclocked,
                            integrity: integrity as f32,
                            checks: v.pressure_checks,
                            excheck: v.pressure_checks & EXTERNAL_BOUND != 0,
                            incheck: v.pressure_checks & INTERNAL_BOUND != 0,
                            direction: v.releasing,
                            external: v.external_pressure_bound as f32,
                            internal: v.internal_pressure_bound as f32,
                            extdefault: v.external_pressure_bound == ONE_ATMOSPHERE,
                            intdefault: v.internal_pressure_bound == 0.0,
                        })
                    })
                    .collect(),
                scrubbers: devices
                    .into_iter()
                    .flat_map(|d| d.scrubbers.iter())
                    .enumerate()
                    .filter_map(|(i, &e)| {
                        let (s, name) = scrubbers.get(e).ok()?;
                        Some(ScrubberEntry {
                            ref_id: entity_ref(e),
                            long_name: long_name(name, i),
                            power: s.on,
                            scrubbing: s.scrubbing,
                            widenet: s.widenet,
                            filter_types: gas_toggles(s.filter),
                        })
                    })
                    .collect(),
                selected_mode_path: alarm.mode.path().to_owned(),
                modes: AlarmMode::ALL
                    .into_iter()
                    .filter(|m| alarm.emagged || !m.info().4)
                    .map(|m| {
                        let (path, name, desc, danger, _) = m.info();
                        ModeEntry {
                            name: name.into(),
                            desc: desc.into(),
                            danger,
                            path: path.into(),
                        }
                    })
                    .collect(),
                panic_siphon_path: AlarmMode::PanicSiphon.path().to_owned(),
                filtering_path: AlarmMode::Filtering.path().to_owned(),
            }
        });
        AirAlarmData {
            locked: alarm.locked,
            silicon_user: silicon,
            emagged: alarm.emagged,
            danger_level: alarm.danger_level,
            atmos_alarm: alarm.area_danger,
            fire_alarm: false,
            sensor: alarm.sensor.is_some(),
            allow_link_change: alarm.allow_link_change,
            env_data,
            tlv_settings,
            controls,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<AlarmAct, UiReject> {
        let checks = || {
            params
                .number("val")
                .map(|v| (v.clamp(0.0, 3.0) as u8).min(EXTERNAL_BOUND | INTERNAL_BOUND))
        };
        Ok(match action {
            "power" => AlarmAct::Power {
                r: ref_param(params)?,
                on: flag(params)?,
            },
            "direction" => AlarmAct::Direction {
                r: ref_param(params)?,
                releasing: flag(params)?,
            },
            "overclock" => AlarmAct::Overclock {
                r: ref_param(params)?,
            },
            "incheck" => AlarmAct::InCheck {
                r: ref_param(params)?,
                checks: checks()?,
            },
            "excheck" => AlarmAct::ExCheck {
                r: ref_param(params)?,
                checks: checks()?,
            },
            "set_internal_pressure" => AlarmAct::SetInternal {
                r: ref_param(params)?,
                value: params.number("value")?,
            },
            "reset_internal_pressure" => AlarmAct::ResetInternal {
                r: ref_param(params)?,
            },
            "set_external_pressure" => AlarmAct::SetExternal {
                r: ref_param(params)?,
                value: params.number("value")?,
            },
            "reset_external_pressure" => AlarmAct::ResetExternal {
                r: ref_param(params)?,
            },
            "scrubbing" => AlarmAct::Scrubbing {
                r: ref_param(params)?,
                on: flag(params)?,
            },
            "widenet" => AlarmAct::Widenet {
                r: ref_param(params)?,
                on: flag(params)?,
            },
            "toggle_filter" => AlarmAct::ToggleFilter {
                r: ref_param(params)?,
                gas: gas_param(params)?,
            },
            "mode" => AlarmAct::Mode(
                params
                    .get("mode")
                    .and_then(AlarmMode::from_path)
                    .ok_or_else(|| UiReject::BadParams("mode: unknown".into()))?,
            ),
            "set_threshold" | "reset_threshold" => {
                let id = params
                    .get("threshold")
                    .filter(|t| {
                        matches!(*t, "pressure" | "temperature") || GasId::parse(t).is_some()
                    })
                    .ok_or_else(|| UiReject::BadParams("threshold: unknown".into()))?
                    .to_owned();
                let bits = (params.number("threshold_type")?.clamp(0.0, 15.0)) as u8;
                if action == "set_threshold" {
                    let value = params.number("value")?;
                    AlarmAct::SetThreshold { id, bits, value }
                } else {
                    AlarmAct::ResetThreshold { id, bits }
                }
            }
            "alarm" => AlarmAct::Alarm,
            "reset" => AlarmAct::Reset,
            "lock" => AlarmAct::Lock,
            "disconnect_sensor" => AlarmAct::DisconnectSensor,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn apply_alarm_acts(mut acts: MessageReader<UiActed<AirAlarmWindow>>, mut commands: Commands) {
    for acted in acts.read() {
        let (source, actor, client, act) =
            (acted.source, acted.actor, acted.client, acted.act.clone());
        commands.queue(move |world: &mut World| alarm_act(world, source, client, actor, act));
    }
}

/// `airalarm/ui_act` at the moment of effect.
fn alarm_act(world: &mut World, alarm: Entity, client: Entity, actor: Entity, act: AlarmAct) {
    let Some(state) = world.get::<AirAlarm>(alarm).cloned() else {
        return;
    };
    let silicon = crate::silicon::interact::has_silicon_access(world, actor);
    if matches!(act, AlarmAct::Lock) {
        toggle_lock(world, alarm, client, actor);
        return;
    }
    if state.locked && !silicon {
        return;
    }
    let devices = state
        .area
        .and_then(|a| world.resource::<AreaAtmos>().0.get(&a).cloned())
        .unwrap_or_default();
    let vent = |r: Entity| devices.vents.contains(&r).then_some(r);
    let scrubber = |r: Entity| devices.scrubbers.contains(&r).then_some(r);
    match act {
        AlarmAct::Power { r, on } => {
            if let Some(mut v) = vent(r).and_then(|r| world.get_mut::<VentPump>(r)) {
                v.on = on;
            } else if let Some(mut s) = scrubber(r).and_then(|r| world.get_mut::<VentScrubber>(r)) {
                s.on = on;
            }
        }
        AlarmAct::Overclock { r } => {
            if let Some(r) = vent(r).filter(|&r| world.get::<VentPump>(r).is_some()) {
                super::overclock::toggle_vent_overclock(world, r);
            }
        }
        AlarmAct::Direction { r, releasing } => {
            if let Some(mut v) = vent(r).and_then(|r| world.get_mut::<VentPump>(r)) {
                v.releasing = releasing;
            }
        }
        AlarmAct::InCheck { r, checks } | AlarmAct::ExCheck { r, checks } => {
            let bit = if matches!(act, AlarmAct::InCheck { .. }) {
                INTERNAL_BOUND
            } else {
                EXTERNAL_BOUND
            };
            if let Some(mut v) = vent(r).and_then(|r| world.get_mut::<VentPump>(r)) {
                v.pressure_checks = (checks ^ bit) & (EXTERNAL_BOUND | INTERNAL_BOUND);
            }
        }
        AlarmAct::SetInternal { r, value } => {
            if let Some(mut v) = vent(r).and_then(|r| world.get_mut::<VentPump>(r)) {
                v.internal_pressure_bound = value.clamp(0.0, ATMOS_PUMP_MAX_PRESSURE);
            }
        }
        AlarmAct::ResetInternal { r } => {
            if let Some(mut v) = vent(r).and_then(|r| world.get_mut::<VentPump>(r)) {
                v.internal_pressure_bound = 0.0;
            }
        }
        AlarmAct::SetExternal { r, value } => {
            if let Some(mut v) = vent(r).and_then(|r| world.get_mut::<VentPump>(r)) {
                v.external_pressure_bound = value.clamp(0.0, ATMOS_PUMP_MAX_PRESSURE);
            }
        }
        AlarmAct::ResetExternal { r } => {
            // tg resets to ATMOS_PUMP_MAX_PRESSURE.
            if let Some(mut v) = vent(r).and_then(|r| world.get_mut::<VentPump>(r)) {
                v.external_pressure_bound = ATMOS_PUMP_MAX_PRESSURE;
            }
        }
        AlarmAct::Scrubbing { r, on } => {
            if let Some(mut s) = scrubber(r).and_then(|r| world.get_mut::<VentScrubber>(r)) {
                s.scrubbing = on;
            }
        }
        AlarmAct::Widenet { r, on } => {
            if let Some(mut s) = scrubber(r).and_then(|r| world.get_mut::<VentScrubber>(r)) {
                s.widenet = on;
            }
        }
        AlarmAct::ToggleFilter { r, gas } => {
            if let Some(mut s) = scrubber(r).and_then(|r| world.get_mut::<VentScrubber>(r)) {
                s.filter ^= 1 << gas.0;
            }
        }
        AlarmAct::Mode(mode) => select_mode(world, alarm, mode),
        AlarmAct::SetThreshold { id, bits, value } => {
            if let Some(mut a) = world.get_mut::<AirAlarm>(alarm)
                && let Some(t) = a.tlv_mut(&id)
            {
                t.set(bits, if value < 0.0 { TLV_IGNORE } else { value });
                a.resample();
            }
        }
        AlarmAct::ResetThreshold { id, bits } => {
            if let Some(mut a) = world.get_mut::<AirAlarm>(alarm)
                && let Some(initial) = a.initial_tlv(&id)
                && let Some(t) = a.tlv_mut(&id)
            {
                t.reset(bits, &initial);
                a.resample();
            }
        }
        // shortcut: the manual area alarm has no alarm manager (consoles, firelocks) to tell;
        // it raises the alarm's own danger level as tg does.
        AlarmAct::Alarm => {
            if let Some(mut a) = world.get_mut::<AirAlarm>(alarm) {
                a.danger_level = super::alarm::ALERT_HAZARD;
            }
        }
        AlarmAct::Reset => {
            if let Some(mut a) = world.get_mut::<AirAlarm>(alarm) {
                a.danger_level = super::alarm::ALERT_NONE;
            }
        }
        AlarmAct::DisconnectSensor => {
            if state.allow_link_change && state.sensor.is_some() {
                super::alarm::disconnect_sensor(world, alarm);
            }
        }
        AlarmAct::Lock => {}
    }
}

/// `togglelock`: an ID with the alarm's access flips the lock.
fn toggle_lock(world: &mut World, alarm: Entity, client: Entity, mob: Entity) {
    if world.get::<Unpowered>(alarm).is_some() {
        tell(world, client, Span::Warning, "It does nothing!");
        return;
    }
    // `HAS_SILICON_ACCESS(user)`: flips it without an ID.
    if crate::silicon::interact::has_silicon_access(world, mob) {
        if let Some(mut a) = world.get_mut::<AirAlarm>(alarm) {
            a.locked = !a.locked;
        }
        return;
    }
    let allowed = world.get::<AirAlarm>(alarm).is_some_and(|a| {
        crate::inventory::id_allowed(world, mob, &a.req_access, &a.req_one_access)
    });
    let Some(mut a) = world.get_mut::<AirAlarm>(alarm) else {
        return;
    };
    if allowed {
        a.locked = !a.locked;
        let text = if a.locked {
            "You lock the air alarm interface."
        } else {
            "You unlock the air alarm interface."
        };
        tell(world, client, Span::Notice, text);
    } else {
        tell(world, client, Span::Danger, "Access denied.");
    }
}

// --- Status hooks, clicks -------------------------------------------------------------------

/// tg `can_interact` of an unpowered machine: its window only updates.
fn unpowered_windows(mut windows: Query<&mut UiWindow>, unpowered: Query<(), With<Unpowered>>) {
    const POWERED: [&str; 7] = [
        PumpWindow::NAME,
        TempGateWindow::NAME,
        TempPumpWindow::NAME,
        FilterWindow::NAME,
        MixerWindow::NAME,
        ThermoMachineUi::NAME,
        AirAlarmWindow::NAME,
    ];
    for mut window in &mut windows {
        if POWERED.contains(&window.interface) && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

fn name_of(world: &World, e: Entity) -> String {
    world.get::<DmName>(e).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    })
}

fn open<T: ServerUi>(world: &mut World, client: Entity, mob: Entity, target: Entity) {
    let title = name_of(world, target);
    world.write_message(OpenUi::new::<T>(client, mob, target, title));
}

/// A click with an empty active hand on an atmos machine. True when handled.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let Ok(e) = world.get_entity(target) else {
        return false;
    };
    let powered = !e.contains::<Unpowered>();
    if super::consoles::interact(world, client, mob, target)
        || crate::silicon::alarms::interact(world, client, mob, target)
    {
        return true;
    }
    if super::control::interact(world, client, mob, target)
        || super::dispenser::interact(world, client, mob, target)
        || super::electrolyzer::interact(world, client, mob, target, modifiers.alt)
        || (!modifiers.alt && super::crystallizer::interact(world, client, mob, target))
    {
        return true;
    }
    let Ok(e) = world.get_entity(target) else {
        return false;
    };
    if modifiers.alt {
        if crate::power::teg::rotate(world, client, target, modifiers.right) {
            return true;
        }
        return click_alt(world, client, mob, target);
    }
    // `pipe/attack_hand_secondary`: the fitting's next layer.
    if modifiers.right && e.contains::<super::fitting::PipeFitting>() {
        return super::fitting::next_layer(world, client, target);
    }
    // `meter/interact`: the reading, unless unpowered or broken.
    if e.contains::<super::meter::Meter>() {
        let broken = e
            .get::<crate::obj_damage::Integrity>()
            .is_some_and(|i| i.broken);
        if powered
            && !broken
            && let Some(status) = super::meter::status(world, target)
        {
            tell(world, client, Span::Notice, &status);
        }
        return true;
    }
    if let Some(alarm) = e.get::<AirAlarm>() {
        // `ui_status`: a shorted alarm's window closes (`UI_CLOSE`).
        let shorted = alarm.shorted;
        if modifiers.right {
            toggle_lock(world, target, client, mob);
        } else if powered && !shorted {
            open::<AirAlarmWindow>(world, client, mob, target);
        }
        return true;
    }
    if e.contains::<Valve>() {
        // `valve/interact`: a second to turn.
        let now = world.resource::<Time>().elapsed_secs_f64();
        if let Some(mut valve) = world.get_mut::<Valve>(target)
            && valve.switching_until.is_none()
        {
            valve.switching_until = Some(now + 1.0);
        }
        return true;
    }
    if e.contains::<GasPump>() || e.contains::<VolumePump>() || e.contains::<OutletInjector>() {
        if powered {
            open::<PumpWindow>(world, client, mob, target);
        }
        return true;
    }
    if e.contains::<PassiveGate>() {
        open::<PumpWindow>(world, client, mob, target);
        return true;
    }
    if let Some(gate) = e.get::<ConditionalGate>() {
        match gate.condition {
            // `NO_POWER_USE`.
            GateCondition::Pressure { .. } => open::<PumpWindow>(world, client, mob, target),
            GateCondition::Temperature { .. } if powered => {
                open::<TempGateWindow>(world, client, mob, target);
            }
            GateCondition::Temperature { .. } => {}
        }
        return true;
    }
    if e.contains::<TemperaturePump>() {
        if powered {
            open::<TempPumpWindow>(world, client, mob, target);
        }
        return true;
    }
    if e.contains::<GasFilter>() {
        if powered {
            open::<FilterWindow>(world, client, mob, target);
        }
        return true;
    }
    if e.contains::<GasMixer>() {
        if powered {
            open::<MixerWindow>(world, client, mob, target);
        }
        return true;
    }
    if let Some(m) = e.get::<ThermoMachine>() {
        // `ui_interact`: nothing with the panel open.
        if powered && !m.panel_open {
            open::<ThermoWindow>(world, client, mob, target);
        }
        return true;
    }
    if e.contains::<Canister>() {
        open::<CanisterWindow>(world, client, mob, target);
        return true;
    }
    if e.contains::<PortablePump>() {
        open::<PortablePumpWindow>(world, client, mob, target);
        return true;
    }
    if e.contains::<PortableScrubber>() {
        open::<PortableScrubberWindow>(world, client, mob, target);
        return true;
    }
    if e.contains::<super::pipe_scrubber::PipeScrubber>() {
        open::<PipeScrubberWindow>(world, client, mob, target);
        return true;
    }
    false
}

/// `click_alt`: portables give back their tank; pumps and filters go to their maximum;
/// thermomachines cycle T20C -> max -> min.
fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    // `gas_receiver/click_alt` replaces the tank's.
    if super::portable::toggle_receiver(world, client, target) {
        return true;
    }
    if world.get::<PortableAtmos>(target).is_some() {
        if eject_tank(world, target, mob) {
            tell(world, client, Span::Notice, "Вы вынимаете баллон.");
        }
        return true;
    }
    if let Some(mut p) = world.get_mut::<GasPump>(target) {
        p.target_pressure = MAX_OUTPUT_PRESSURE;
        return true;
    }
    if let Some(mut g) = world.get_mut::<PassiveGate>(target) {
        g.target_pressure = MAX_OUTPUT_PRESSURE;
        return true;
    }
    if let Some(mut v) = world.get_mut::<VolumePump>(target) {
        v.transfer_rate = MAX_TRANSFER_RATE;
        return true;
    }
    if let Some(mut g) = world.get_mut::<ConditionalGate>(target) {
        match &mut g.condition {
            GateCondition::Pressure { target } => *target = MAX_OUTPUT_PRESSURE,
            GateCondition::Temperature { target, .. } => *target = TEMPERATURE_GATE_MAX,
        }
        return true;
    }
    if let Some(mut p) = world.get_mut::<TemperaturePump>(target) {
        p.heat_transfer_rate = MAX_HEAT_TRANSFER_RATE;
        return true;
    }
    if let Some(mut f) = world.get_mut::<GasFilter>(target) {
        f.transfer_rate = MAX_TRANSFER_RATE;
        return true;
    }
    if let Some(mut m) = world.get_mut::<GasMixer>(target) {
        m.target_pressure = MAX_OUTPUT_PRESSURE;
        return true;
    }
    if let Some(mut i) = world.get_mut::<OutletInjector>(target) {
        i.volume_rate = MAX_TRANSFER_RATE;
        return true;
    }
    if world
        .get::<ThermoMachine>(target)
        .is_some_and(|t| t.panel_open)
    {
        tell(world, client, Span::Warning, "Сначала закройте панель!");
        return true;
    }
    if let Some(mut t) = world.get_mut::<ThermoMachine>(target) {
        t.target_temperature = if t.target_temperature == T20C {
            t.max_temperature
        } else if t.target_temperature == t.max_temperature {
            t.min_temperature
        } else {
            T20C
        };
        return true;
    }
    false
}

/// `click_ctrl`: an operational pump, gate, filter, mixer, injector or thermomachine turns on
/// or off.
fn ctrl_clicks(
    mut clicks: MessageReader<CtrlClick>,
    mut machines: Query<
        (
            Option<&mut GasPump>,
            Option<&mut PassiveGate>,
            Option<&mut VolumePump>,
            Option<&mut GasFilter>,
            Option<&mut GasMixer>,
            Option<&mut OutletInjector>,
            Option<&mut ThermoMachine>,
            Option<&mut ConditionalGate>,
            Option<&mut TemperaturePump>,
        ),
        Without<Unpowered>,
    >,
) {
    for click in clicks.read() {
        let Ok((pump, gate, volume, filter, mixer, injector, thermo, conditional, heat_pump)) =
            machines.get_mut(click.target)
        else {
            continue;
        };
        if let Some(mut m) = conditional {
            m.on = !m.on;
        } else if let Some(mut m) = heat_pump {
            m.on = !m.on;
        } else if let Some(mut m) = pump {
            m.on = !m.on;
        } else if let Some(mut m) = gate {
            m.on = !m.on;
        } else if let Some(mut m) = volume {
            m.on = !m.on;
        } else if let Some(mut m) = filter {
            m.on = !m.on;
        } else if let Some(mut m) = mixer {
            m.on = !m.on;
        } else if let Some(mut m) = injector {
            m.on = !m.on;
        } else if let Some(mut m) = thermo.filter(|m| !m.panel_open) {
            m.on = !m.on;
        }
    }
}

/// Timed atmos jobs ([`crate::construction::Job::Atmos`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtmosJob {
    /// `welder_act` of a vent or scrubber: weld or unweld it.
    Weld,
    /// Cutting a tank open and the steps of its frame.
    Frame(super::tank_frame::FrameJob),
    /// `tank/wrench_act`: open or close the port in this direction.
    TankPort(u8),
    /// `welder_act` of a canister or tank: one round of repair, repeated until whole.
    Repair,
    /// `vent_pump/screwdriver_act`: the fan back to full integrity.
    FanRepair,
    /// `default_unfasten_wrench`: (un)anchor a huge scrubber or a pipe dispenser that was
    /// `anchored` when the wrenching began.
    Unfasten { anchored: bool },
    /// Wrenching a pipe, device or meter off.
    Fitting(super::fitting::FittingJob),
    /// One layer of an RPD build.
    Rpd(super::rpd::RpdJob),
}

/// The end of an [`AtmosJob`] (tg `use_tool` succeeded).
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: AtmosJob) {
    match job {
        AtmosJob::Fitting(job) => super::fitting::complete(world, mob, action, job),
        AtmosJob::Rpd(job) => super::rpd::complete(world, mob, action, job),
        AtmosJob::Repair => {
            let target = action.target;
            let Some(max) = world
                .get::<ss13_mapload::DmObj>(target)
                .map(|o| o.max_integrity)
            else {
                return;
            };
            // Portables mend 25 a round, tanks a tenth of their integrity (`repair_damage`).
            let step = if world.get::<PortableAtmos>(target).is_some() {
                25.0
            } else {
                max / 10.0
            };
            let Some(mut integrity) = world.get_mut::<crate::obj_damage::Integrity>(target) else {
                return;
            };
            integrity.current = (integrity.current + step).min(max);
            if integrity.current < max {
                tell(
                    world,
                    action.client,
                    Span::Notice,
                    "Частично отремонтировано...",
                );
                let actor = Actor {
                    client: action.client,
                    mob,
                    item: action.item,
                };
                crate::tools::use_tool(
                    world,
                    actor,
                    target,
                    25.0,
                    0,
                    crate::construction::Job::Atmos(AtmosJob::Repair),
                );
            } else {
                tell(world, action.client, Span::Notice, "Отремонтировано.");
            }
        }
        AtmosJob::Unfasten { anchored: was } => {
            let target = action.target;
            let now = world
                .get::<crate::obj_damage::Anchored>(target)
                .is_some_and(|a| a.0);
            if now != was {
                return;
            }
            let what = unfasten_noun(world, target);
            if !was && crate::construction::blocked_for(world, target) {
                let text = format!("Вам не удалось прикрутить {what}.");
                tell(world, action.client, Span::Notice, text);
                return;
            }
            crate::obj_damage::set_anchored(world, target, !was);
            if let Some(mut s) = world.get_mut::<PortableScrubber>(target)
                && !s.movable
            {
                s.on = false;
            }
            let text = if was {
                format!("Вы откручиваете {what}.")
            } else {
                format!("Вы прикручиваете {what}.")
            };
            tell(world, action.client, Span::Notice, text);
            if let Some(&pos) = world.get::<TilePos>(target) {
                let sound = crate::sound::PlaySound::new(
                    pos,
                    ss13_mapload::sound::files::DECONSTRUCT,
                    50.0,
                    true,
                );
                crate::sound::playsound(world, sound);
            }
        }
        AtmosJob::Frame(job) => super::tank_frame::complete(world, action, job),
        AtmosJob::FanRepair => {
            super::overclock::repair_vent(world, action.client, action.target);
        }
        AtmosJob::TankPort(dir) => {
            super::tank::toggle_port(world, action.target, dir);
        }
        AtmosJob::Weld => {
            let target = action.target;
            if let Some(item) = action.item
                && !crate::tools::consume(world, item, action.amount)
            {
                return;
            }
            let welded = if let Some(mut v) = world.get_mut::<VentPump>(target) {
                v.welded = !v.welded;
                v.welded
            } else if let Some(mut s) = world.get_mut::<VentScrubber>(target) {
                s.welded = !s.welded;
                s.welded
            } else {
                return;
            };
            let text = if welded {
                "Вы завариваете решётку."
            } else {
                "Вы развариваете решётку."
            };
            tell(world, action.client, Span::Notice, text);
        }
    }
}

const HUGE_BLOCKED: &str = "Вам не удалось прикрутить большой воздушный скруббер.";

/// What a `default_unfasten_wrench` message names, in the accusative.
fn unfasten_noun(world: &World, target: Entity) -> &'static str {
    if world.get::<PortableScrubber>(target).is_some() {
        "большой воздушный скруббер"
    } else if world
        .get::<super::electrolyzer::Electrolyzer>(target)
        .is_some()
    {
        "электролизёр"
    } else {
        "раздатчик труб"
    }
}

/// An item on an atmos machine. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let Ok(e) = world.get_entity(target) else {
        return false;
    };
    let is_portable = e.contains::<PortableAtmos>();
    let is_alarm = e.contains::<AirAlarm>();
    let is_vent = e.contains::<VentPump>() || e.contains::<VentScrubber>();
    let is_tank = e.contains::<super::tank::StationaryTank>();
    let tool = tool_of(world, held).map(|t| t.0);
    if crate::power::teg::tool_act(world, client, target, tool) {
        return true;
    }
    if super::tank_frame::item_interaction(world, client, mob, held, target, modifiers) {
        return true;
    }
    if super::thermo::item_interaction(world, client, target, tool, modifiers.right) {
        return true;
    }
    if tool == Some(ToolKind::Multitool)
        && (super::portable::bluespace_multitool(world, client, held, target)
            || super::control::multitool_act(world, client, held, target, modifiers.right))
    {
        return true;
    }
    if tool == Some(ToolKind::Wirecutters)
        && super::portable::receiver_wirecutter(world, client, target)
    {
        return true;
    }
    // `temperature_gate/multitool_act`: flips which side of the target lets gas through.
    if tool == Some(ToolKind::Multitool)
        && let Some(mut gate) = world.get_mut::<ConditionalGate>(target)
        && let GateCondition::Temperature { inverted, .. } = &mut gate.condition
    {
        *inverted = !*inverted;
        let text = if *inverted {
            "Вы настраиваете датчики клапана пропускать газ, когда он горячее заданной температуры."
        } else {
            "Вы возвращаете датчики клапана к настройкам по умолчанию."
        };
        tell(world, client, Span::Notice, text);
        return true;
    }
    if tool == Some(ToolKind::Multitool)
        && super::overclock::multitool_volume_pump(world, client, target)
    {
        return true;
    }
    // `vent_pump/screwdriver_act`: up to 10 s, by how worn the fan is.
    if tool == Some(ToolKind::Screwdriver)
        && let Some(delay) = super::overclock::vent_repair_delay(world, target)
    {
        tell(
            world,
            client,
            Span::Notice,
            "Вы начинаете ремонт вентиляции...",
        );
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        crate::tools::use_tool(
            world,
            actor,
            target,
            delay,
            0,
            crate::construction::Job::Atmos(AtmosJob::FanRepair),
        );
        return true;
    }
    if is_alarm && crate::inventory::get_id(world, held).is_some() {
        toggle_lock(world, target, client, mob);
        return true;
    }
    // `tank/wrench_act`: three seconds, the port on the side the user stands.
    if tool == Some(ToolKind::Wrench) && is_tank {
        let dir = match (world.get::<TilePos>(target), world.get::<TilePos>(mob)) {
            (Some(t), Some(m)) => match (m.x - t.x, m.y - t.y) {
                (0, 1) => 1,
                (0, -1) => 2,
                (1, 0) => 4,
                (-1, 0) => 8,
                _ => 0,
            },
            _ => 0,
        };
        if dir != 0 {
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            crate::tools::use_tool(
                world,
                actor,
                target,
                30.0,
                0,
                crate::construction::Job::Atmos(AtmosJob::TankPort(dir)),
            );
        }
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if super::rpd::item_interaction(world, actor, held, target, modifiers)
        || super::dispenser::item_interaction(world, actor, held, target, tool)
        || super::electrolyzer::item_interaction(world, actor, held, target, tool)
        || super::crystallizer::item_interaction(world, client, target, tool)
    {
        return true;
    }
    // `wrench_act` of pipes, devices and meters (off) and of fittings (on).
    if super::fitting::item_interaction(
        world,
        Actor {
            client,
            mob,
            item: Some(held),
        },
        target,
        tool,
    ) {
        return true;
    }
    if super::canister::item_interaction(world, client, mob, held, target, tool)
        || super::portable::insert_nob_crystal(world, client, held, target)
    {
        return true;
    }
    // `welder_act`: a hot welder mends a damaged, unbroken canister or tank.
    if (is_portable || is_tank) && tool == Some(ToolKind::Welder) {
        let integrity = world.get::<crate::obj_damage::Integrity>(target).copied();
        let max = world
            .get::<ss13_mapload::DmObj>(target)
            .map_or(0.0, |o| o.max_integrity);
        if !integrity.is_some_and(|i| i.current < max && !i.broken) {
            return false;
        }
        if !crate::tools::tool_usable(world, held, 0, crate::tools::HIGH_TEMPERATURE_REQUIRED) {
            return true;
        }
        tell(world, client, Span::Notice, "Вы начинаете ремонт...");
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        crate::tools::use_tool(
            world,
            actor,
            target,
            25.0,
            0,
            crate::construction::Job::Atmos(AtmosJob::Repair),
        );
        return true;
    }
    if is_portable && tool == Some(ToolKind::Wrench) {
        wrench_portable(world, client, target);
        // `huge/wrench_act`: after the port, `default_unfasten_wrench` (2 s).
        if world
            .get::<PortableScrubber>(target)
            .is_some_and(|s| s.huge)
        {
            let anchored = world
                .get::<crate::obj_damage::Anchored>(target)
                .is_some_and(|a| a.0);
            if !anchored && crate::construction::blocked_for(world, target) {
                tell(world, client, Span::Notice, HUGE_BLOCKED);
                return true;
            }
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            if crate::tools::use_tool(
                world,
                actor,
                target,
                20.0,
                0,
                crate::construction::Job::Atmos(AtmosJob::Unfasten { anchored }),
            ) {
                let text = if anchored {
                    "Вы начинаете откреплять большой воздушный скруббер..."
                } else {
                    "Вы начинаете прикреплять большой воздушный скруббер..."
                };
                tell(world, client, Span::Notice, text);
            }
        }
        return true;
    }
    // `pipe_scrubber/replace_tank` refuses every tank.
    if is_portable
        && world.get::<TankAir>(held).is_some()
        && world
            .get::<super::pipe_scrubber::PipeScrubber>(target)
            .is_none()
    {
        insert_tank(world, client, mob, held, target);
        return true;
    }
    if is_vent && tool == Some(ToolKind::Welder) {
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        tell(
            world,
            client,
            Span::Notice,
            "Вы начинаете заваривать вентиляцию...",
        );
        crate::tools::use_tool(
            world,
            actor,
            target,
            20.0,
            1,
            crate::construction::Job::Atmos(AtmosJob::Weld),
        );
        return true;
    }
    false
}

/// `portable_atmospherics/wrench_act`: disconnect, or connect to the port on its tile.
fn wrench_portable(world: &mut World, client: Entity, portable: Entity) {
    if super::portable::disconnect(world, portable) {
        tell(
            world,
            client,
            Span::Notice,
            "Вы отсоединяете машину от порта.",
        );
        return;
    }
    let Some(&pos) = world.get::<TilePos>(portable) else {
        return;
    };
    let port = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<super::PortsConnector>(e).is_some());
    let Some(port) = port else {
        tell(world, client, Span::Notice, "Ничего не происходит.");
        return;
    };
    if super::portable::connect(world, portable, port) {
        tell(
            world,
            client,
            Span::Notice,
            "Вы присоединяете машину к порту.",
        );
    } else {
        tell(
            world,
            client,
            Span::Notice,
            "Не удалось присоединить машину к порту.",
        );
    }
}

/// `replace_tank(user, FALSE, new_tank)`: the tank goes in, a held one comes out.
fn insert_tank(world: &mut World, client: Entity, mob: Entity, tank: Entity, portable: Entity) {
    let old = world.get::<PortableAtmos>(portable).and_then(|p| p.holding);
    crate::storage::put_inside(world, tank, portable);
    if let Some(mut p) = world.get_mut::<PortableAtmos>(portable) {
        p.holding = Some(tank);
    }
    if let Some(old) = old {
        give_to_hands(world, mob, old, portable);
    }
    tell(world, client, Span::Notice, "Вы вставляете баллон.");
}

pub fn plugin(app: &mut App) {
    app.register_ui::<PumpWindow>()
        .register_ui::<FilterWindow>()
        .register_ui::<TempGateWindow>()
        .register_ui::<TempPumpWindow>()
        .register_ui::<MixerWindow>()
        .register_ui::<ThermoWindow>()
        .register_ui::<CanisterWindow>()
        .register_ui::<PortablePumpWindow>()
        .register_ui::<PortableScrubberWindow>()
        .register_ui::<PipeScrubberWindow>()
        .register_ui::<AirAlarmWindow>()
        .register_ui::<super::dispenser::PipeDispenserWindow>()
        .register_ui::<super::rpd::RpdWindow>()
        .register_ui::<super::electrolyzer::ElectrolyzerWindow>()
        .register_ui::<super::crystallizer::CrystallizerWindow>()
        .add_observer(super::rpd::attach)
        .add_systems(
            Update,
            (
                (
                    apply_pump_acts,
                    apply_temp_gate_acts,
                    apply_temp_pump_acts,
                    apply_filter_acts,
                    apply_mixer_acts,
                    apply_thermo_acts,
                    apply_canister_acts,
                    apply_portable_pump_acts,
                    apply_portable_scrubber_acts,
                    apply_pipe_scrubber_acts,
                    apply_alarm_acts,
                    super::dispenser::dispenser_acts,
                    super::rpd::rpd_acts,
                    super::electrolyzer::electrolyzer_acts,
                    super::crystallizer::crystallizer_acts,
                )
                    .in_set(UiSystems::Handle),
                super::dispenser::init_dispensers,
                super::electrolyzer::init_electrolyzers,
                super::crystallizer::init_crystallizers,
                super::canister::relabel_answers
                    .in_set(UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
                (unpowered_windows, super::rpd::window_status).in_set(UiSystems::StatusHook),
                ctrl_clicks
                    .after(ss13_world::WorldSystems::Movement)
                    .after(crate::click::ResolveClicks),
                super::crystallizer::crystallizer_ctrl_clicks
                    .after(ss13_world::WorldSystems::Movement)
                    .after(crate::click::ResolveClicks),
            ),
        );
}
