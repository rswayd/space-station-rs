//! Wire sets of machines beyond the airlock, APC, vendor and autolathe ([`crate::wires`]), after
//! tg `code/datums/wires/`: `airalarm.dm`, `firealarm.dm`, `atmosshieldgen.dm`, `emitter.dm`,
//! `shieldwallgen.dm`, `mass_driver.dm`, `tesla_coil.dm`.
//!
//! - Every such machine gets its [`Wires`] when its state component appears (map load or built
//!   from a frame), as [`WireHolder::Machine`] with its [`MachineWires`] kind.
//! - The panel: air and fire alarms, emitters and mass drivers toggle it with a screwdriver here
//!   (tg `screwdriver_act` / `default_deconstruction_screwdriver`); shield generators keep their
//!   frame's panel (`crate::machine_frame`).
//! - With the panel open an empty hand or a wire tool opens the `Wires` window first
//!   (`INTERACT_MACHINE_WIRES_IF_OPEN`, `is_wire_tool(tool) && panel_open`), before the
//!   machine's own click (pulling a fire alarm, the air alarm window).
//! - Each kind's `on_cut`/`on_pulse`/`get_status`/`reset` lives here; the window's status lines
//!   are refreshed into [`MachineWireStatus`] while a window is open.
// shortcut: the emitter's hack wire flips `mode`, which only matters with an integrated gun
// (`gun_properties`, not ported): no effect here. The AI wires only keep their state (AI
// control of alarms is the silicon group's).

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::UiStatus;
use ss13_tgui::server::UiWindow;

use crate::atmos_machinery::AirAlarm;
use crate::atmos_machinery::alarm::{ALERT_HAZARD, ALERT_NONE, AlarmMode, select_mode};
use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::construction::PanelOpen;
use crate::eng_machines::atmos_shield::AtmosShieldGen;
use crate::health::DEFAULT_MESSAGE_RANGE;
use crate::obj_damage::Anchored;
use crate::power::Unpowered;
use crate::power::accumulator::{Accumulator, AccumulatorKind};
use crate::power::emitter::Emitter;
use crate::power::shields::ShieldWallGen;
use crate::tools::{ToolKind, tell, tool_of};
use crate::wallmount::fire_alarm::{self, AreaFire, FireAlarm};
use crate::wallmount::machines::MassDriver;
use crate::wires::{
    self, WIRE_AI, WIRE_HACK, WIRE_IDSCAN, WIRE_SPEAKER, WIRE_ZAP, WireColors, WireHolder, WireSet,
    Wires, colors_for, with_duds,
};

pub const WIRE_ACTIVATE: &str = "Activate";
pub const WIRE_ALARM: &str = "Alarm";
pub const WIRE_FIRE_DETECT: &str = "Automatic Detection";
pub const WIRE_FIRE_RESET: &str = "Alarm Reset";
pub const WIRE_FIRE_TRIGGER: &str = "Alarm Trigger";
pub const WIRE_LAUNCH: &str = "Launch";
pub const WIRE_PANIC: &str = "Panic Siphon";
pub const WIRE_POWER: &str = "Power";
pub const WIRE_SAFETIES: &str = "Safeties";

/// `airalarm/on_pulse` timers: the power wire shorts for 2 minutes, the AI wire for 10 s.
const AIR_ALARM_SHORT_SECS: f64 = 120.0;
const AIR_ALARM_AI_SECS: f64 = 10.0;
/// `mass_driver/power` with the safeties pulsed, and its default.
const DRIVER_OVERDRIVE: f32 = 3.0;
const DRIVER_SAFE: f32 = 1.0;

/// The machine a [`WireHolder::Machine`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineWires {
    AirAlarm,
    FireAlarm,
    AtmosShieldGen,
    Emitter,
    ShieldWallGen,
    MassDriver,
    TeslaCoil,
    /// The kinds of [`crate::device_wires`].
    Microwave,
    Holopad,
    Fax,
    Rnd,
    Robot,
    Mecha,
    Mod,
    Radio,
    /// A circuit shell's wires from its wire bundle component (`crate::wiremod::server`).
    WireBundle,
    /// A gizmo's cryptic wires (`crate::science::gizmo`).
    Gizmo,
}

impl MachineWires {
    /// The set of `/datum/wires/<kind>`.
    pub fn set(self) -> WireSet {
        let (key, proper_name, list, duds): (_, _, &[&str], _) = match self {
            Self::AirAlarm => (
                "/obj/machinery/airalarm",
                "Air Alarm",
                &[
                    WIRE_POWER,
                    WIRE_IDSCAN,
                    WIRE_AI,
                    WIRE_PANIC,
                    WIRE_ALARM,
                    WIRE_SPEAKER,
                ],
                3,
            ),
            Self::FireAlarm => (
                "/obj/machinery/firealarm",
                "Fire Alarm",
                &[WIRE_FIRE_DETECT, WIRE_FIRE_RESET, WIRE_FIRE_TRIGGER],
                1,
            ),
            Self::AtmosShieldGen => (
                "/obj/machinery/atmos_shield_gen",
                "Atmos Shield Generator",
                &[WIRE_ACTIVATE],
                0,
            ),
            Self::Emitter => (
                "/obj/machinery/power/emitter",
                "Emitter",
                &[WIRE_ZAP, WIRE_HACK],
                0,
            ),
            Self::ShieldWallGen => (
                "/obj/machinery/power/shieldwallgen",
                "Shield Wall Generator",
                &[WIRE_ACTIVATE],
                0,
            ),
            Self::MassDriver => (
                "/obj/machinery/mass_driver",
                "Mass Driver",
                &[WIRE_LAUNCH, WIRE_SAFETIES],
                0,
            ),
            Self::TeslaCoil => (
                crate::power::accumulator::TESLA_COIL_ROOT,
                "Tesla Coil",
                &[WIRE_ZAP],
                0,
            ),
            _ => return crate::device_wires::set(self),
        };
        WireSet {
            key,
            proper_name,
            wires: with_duds(list, duds),
        }
    }

    /// `randomize = TRUE`: every instance its own colors.
    fn randomized(self) -> bool {
        matches!(
            self,
            Self::ShieldWallGen | Self::TeslaCoil | Self::Rnd | Self::Robot
        )
    }

    /// The panel toggles with a screwdriver here (no frame family owns it).
    fn own_panel(self) -> bool {
        // A frame's panel (`crate::machine_frame`), an R&D lathe's, the MOD's cover
        // (`crate::modsuit`); cyborgs, mechs and radios: `crate::device_wires::screwdriver`.
        !matches!(
            self,
            Self::AtmosShieldGen
                | Self::ShieldWallGen
                | Self::Microwave
                | Self::Rnd
                | Self::Mod
                | Self::Robot
                | Self::Mecha
                | Self::Radio
                | Self::WireBundle
                | Self::Gizmo
        )
    }
}

/// The holder's `get_status()` lines, refreshed while a `Wires` window shows them.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct MachineWireStatus(pub Vec<String>);

pub(crate) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            attach_wires,
            refresh_status.before(ss13_tgui::server::UiSystems::Handle),
            close_shorted_alarms.in_set(ss13_tgui::server::UiSystems::StatusHook),
        ),
    )
    .add_click_handler(ClickPhase::EmptyHand, 99, empty_hand)
    .add_click_handler(ClickPhase::HeldOnTarget, 289, held_on_target)
    .add_click_handler(ClickPhase::OtherHand, 89, held_on_target);
    crate::device_wires::build(app);
}

/// The kind's [`Wires`] for `e`, colors from the round's directory (or per instance).
pub fn wires_for(colors: &mut WireColors, kind: MachineWires, e: Entity) -> Wires {
    let set = kind.set();
    if kind.randomized() {
        return Wires {
            holder: WireHolder::Machine(kind),
            proper_name: set.proper_name,
            colors: colors_for(colors.seed ^ e.to_bits(), &set),
            cut: Vec::new(),
        };
    }
    colors.wires_for(WireHolder::Machine(kind), &set)
}

/// Inserts the kind's wires on `e`.
pub fn add(world: &mut World, e: Entity, kind: MachineWires) {
    let wires = wires_for(&mut world.get_resource_or_init::<WireColors>(), kind, e);
    world.entity_mut(e).insert(wires);
}

type NewMachines = (
    Without<Wires>,
    Or<(
        Added<AirAlarm>,
        Added<FireAlarm>,
        Added<AtmosShieldGen>,
        Added<Emitter>,
        Added<ShieldWallGen>,
        Added<MassDriver>,
        Added<Accumulator>,
    )>,
);

/// tg `set_wires(new /datum/wires/<kind>(src))` in `Initialize`.
fn attach_wires(
    mut colors: ResMut<WireColors>,
    new: Query<
        (
            Entity,
            Has<AirAlarm>,
            Has<FireAlarm>,
            Has<AtmosShieldGen>,
            Has<Emitter>,
            Has<ShieldWallGen>,
            Has<MassDriver>,
            Option<&Accumulator>,
        ),
        NewMachines,
    >,
    mut commands: Commands,
) {
    for (e, air, fire, atmos_shield, emitter, wall, driver, accumulator) in &new {
        let coil = accumulator.is_some_and(|a| matches!(a.kind, AccumulatorKind::TeslaCoil { .. }));
        let kind = if air {
            MachineWires::AirAlarm
        } else if fire {
            MachineWires::FireAlarm
        } else if atmos_shield {
            MachineWires::AtmosShieldGen
        } else if emitter {
            MachineWires::Emitter
        } else if wall {
            MachineWires::ShieldWallGen
        } else if driver {
            MachineWires::MassDriver
        } else if coil {
            MachineWires::TeslaCoil
        } else {
            continue;
        };
        commands.entity(e).insert(wires_for(&mut colors, kind, e));
    }
}

pub(crate) fn kind_of(world: &World, e: Entity) -> Option<MachineWires> {
    match world.get::<Wires>(e)?.holder {
        WireHolder::Machine(kind) => Some(kind),
        _ => None,
    }
}

pub(crate) fn panel_open(world: &World, e: Entity) -> bool {
    world.get::<PanelOpen>(e).is_some_and(|p| p.0)
}

/// The kind's `interactable(user)`: the panel is open.
// tg's shield generators and emitters do not check the panel in `interactable`, so their window
// would stay open with the panel shut; the panel check here is the intent, the bug not carried.
pub fn interactable(world: &World, e: Entity, kind: MachineWires) -> bool {
    crate::device_wires::interactable(world, e, kind).unwrap_or_else(|| panel_open(world, e))
}

/// `INTERACT_MACHINE_WIRES_IF_OPEN`: an empty hand on an open panel opens the wires.
fn empty_hand(world: &mut World, ctx: &ClickCtx) -> bool {
    let Some(kind) = kind_of(world, ctx.target) else {
        return false;
    };
    // Items are picked up by hand; a cyborg's wires take a wire tool.
    if !crate::device_wires::hand_opens(world, ctx.target, kind)
        || !interactable(world, ctx.target, kind)
    {
        return false;
    }
    wires::interact(world, ctx.client, ctx.mob, ctx.target);
    true
}

/// A screwdriver on the panel, a wire tool on an open panel.
fn held_on_target(world: &mut World, ctx: &ClickCtx) -> bool {
    let Some(held) = ctx.held else {
        return false;
    };
    let Some(kind) = kind_of(world, ctx.target) else {
        return false;
    };
    if let Some(done) = crate::device_wires::screwdriver(world, ctx, kind, held) {
        return done;
    }
    if kind.own_panel() && tool_of(world, held).map(|t| t.0) == Some(ToolKind::Screwdriver) {
        let open = !panel_open(world, ctx.target);
        world.entity_mut(ctx.target).insert(PanelOpen(open));
        let text = if open {
            "Вы открываете панель с проводами."
        } else {
            "Вы закрываете панель с проводами."
        };
        tell(world, ctx.client, Span::Notice, text);
        return true;
    }
    let wire_tool = matches!(
        tool_of(world, held).map(|t| t.0),
        Some(ToolKind::Wirecutters | ToolKind::Multitool)
    );
    // `wire_bundle/on_shell_secondary_interaction`: a right click (the left multitool opens the
    // circuit).
    if kind == MachineWires::WireBundle && !ctx.modifiers.right {
        return false;
    }
    if !wire_tool || !interactable(world, ctx.target, kind) {
        return false;
    }
    wires::interact(world, ctx.client, ctx.mob, ctx.target);
    true
}

/// Everyone within hearing of `e` (tg `visible_message` / `audible_message`).
pub(crate) fn announce(world: &mut World, e: Entity, text: &str) {
    let Some(pos) = crate::explosions::turf_of(world, e) else {
        return;
    };
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    if clients.is_empty() {
        return;
    }
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![ChatSegment::new(Span::Notice, text)],
        },
    });
}

fn air_alarm(world: &mut World, e: Entity, f: impl FnOnce(&mut AirAlarm)) {
    if let Some(mut a) = world.get_mut::<AirAlarm>(e) {
        f(&mut a);
    }
}

/// The kind's `on_cut(wire, mend, source)`.
pub fn on_cut(
    world: &mut World,
    e: Entity,
    kind: MachineWires,
    wire: &str,
    mend: bool,
    source: Option<Entity>,
) {
    match (kind, wire) {
        (MachineWires::AirAlarm, WIRE_POWER) => {
            if let Some(user) = source {
                wires::shock(world, e, user, 50.0);
            }
            air_alarm(world, e, |a| a.shorted = !mend);
        }
        (MachineWires::AirAlarm, WIRE_IDSCAN) if !mend => air_alarm(world, e, |a| a.locked = true),
        // tg sets `aidisabled = mend`, so cutting the wire would give the AI control back; the
        // intent (cut: disabled, as the pulse and `reset`) is ported, the bug not carried.
        (MachineWires::AirAlarm, WIRE_AI) => air_alarm(world, e, |a| a.aidisabled = !mend),
        (MachineWires::AirAlarm, WIRE_PANIC) => {
            if !mend && world.get::<AirAlarm>(e).is_some_and(|a| !a.shorted) {
                select_mode(world, e, AlarmMode::PanicSiphon);
            }
        }
        // `alarm_manager.send_alarm(ALARM_ATMOS)`: the area alarms until the air is rated again.
        (MachineWires::AirAlarm, WIRE_ALARM) => air_alarm(world, e, |a| {
            a.danger_level = ALERT_HAZARD;
        }),
        (MachineWires::AirAlarm, WIRE_SPEAKER) => {
            air_alarm(world, e, |a| a.speaker_enabled = mend);
        }
        (MachineWires::FireAlarm, WIRE_FIRE_DETECT) => {
            let Some(area) = world.get::<FireAlarm>(e).map(|a| a.area) else {
                return;
            };
            if let Some(mut a) = world.get_mut::<FireAlarm>(e) {
                a.can_toggle_detection = mend;
            }
            if mend {
                fire_alarm::set_fire_detect(world, e, true);
                return;
            }
            // Off only when every alarm of the area has its detection wire cut.
            let alarms = world
                .get::<AreaFire>(area)
                .map(|f| f.firealarms.clone())
                .unwrap_or_default();
            let all_cut = alarms.iter().all(|&a| {
                world
                    .get::<Wires>(a)
                    .is_some_and(|w| w.is_cut(WIRE_FIRE_DETECT))
            });
            if all_cut {
                fire_alarm::set_fire_detect(world, e, false);
            }
        }
        // Stuck: no `alarm()` or `reset()` from these while cut.
        (MachineWires::FireAlarm, WIRE_FIRE_TRIGGER) => {
            if let Some(mut a) = world.get_mut::<FireAlarm>(e) {
                a.can_trigger = mend;
            }
        }
        (MachineWires::FireAlarm, WIRE_FIRE_RESET) => {
            if let Some(mut a) = world.get_mut::<FireAlarm>(e) {
                a.can_reset = mend;
            }
        }
        (MachineWires::MassDriver, WIRE_SAFETIES) => {
            let overdriven = world
                .get::<MassDriver>(e)
                .is_some_and(|d| d.power > DRIVER_SAFE);
            if overdriven {
                if let Some(mut d) = world.get_mut::<MassDriver>(e) {
                    d.power = DRIVER_SAFE;
                }
                announce(world, e, "Тревожное жужжание массдрайвера стихает.");
            }
        }
        _ => crate::device_wires::on_cut(world, e, kind, wire, mend, source),
    }
}

/// The kind's `on_pulse(wire, user)`.
pub fn on_pulse(
    world: &mut World,
    e: Entity,
    kind: MachineWires,
    wire: &str,
    user: Option<Entity>,
) {
    match (kind, wire) {
        (MachineWires::AirAlarm, WIRE_POWER) => {
            air_alarm(world, e, |a| a.shorted = true);
            wires::schedule_reset(world, e, AIR_ALARM_SHORT_SECS, WIRE_POWER);
        }
        (MachineWires::AirAlarm, WIRE_IDSCAN) => air_alarm(world, e, |a| a.locked = !a.locked),
        (MachineWires::AirAlarm, WIRE_AI) => {
            air_alarm(world, e, |a| a.aidisabled = true);
            wires::schedule_reset(world, e, AIR_ALARM_AI_SECS, WIRE_AI);
        }
        (MachineWires::AirAlarm, WIRE_PANIC) => {
            let Some(a) = world.get::<AirAlarm>(e) else {
                return;
            };
            if !a.shorted {
                let mode = if a.mode == AlarmMode::Filtering {
                    AlarmMode::PanicSiphon
                } else {
                    AlarmMode::Filtering
                };
                select_mode(world, e, mode);
            }
        }
        // `alarm_manager.clear_alarm(ALARM_ATMOS)`.
        (MachineWires::AirAlarm, WIRE_ALARM) => air_alarm(world, e, |a| {
            a.danger_level = ALERT_NONE;
        }),
        (MachineWires::FireAlarm, WIRE_FIRE_DETECT) => fire_alarm::toggle_fire_detect(world, e),
        (MachineWires::FireAlarm, WIRE_FIRE_TRIGGER) => fire_alarm::alarm(world, e, None),
        (MachineWires::FireAlarm, WIRE_FIRE_RESET) => fire_alarm::reset(world, e, None),
        (MachineWires::AtmosShieldGen, WIRE_ACTIVATE) => {
            if world.get::<Anchored>(e).is_some_and(|a| a.0) {
                crate::eng_machines::atmos_shield::toggle(world, e);
            }
        }
        (MachineWires::Emitter, WIRE_ZAP) => {
            crate::power::emitter::fire_beam_pulse(world, e);
        }
        (MachineWires::ShieldWallGen, WIRE_ACTIVATE) => {
            crate::power::shields::wire_toggle(world, e);
        }
        (MachineWires::MassDriver, WIRE_LAUNCH) => {
            crate::wallmount::machines::drive(world, e);
            announce(world, e, "Механизм массдрайвера срабатывает.");
        }
        (MachineWires::TeslaCoil, WIRE_ZAP) => {
            tesla_coil_zap(world, e);
        }
        (MachineWires::MassDriver, WIRE_SAFETIES) => {
            if let Some(mut d) = world.get_mut::<MassDriver>(e) {
                d.power = DRIVER_OVERDRIVE;
            }
            announce(world, e, "Из массдрайвера доносится тревожное жужжание.");
        }
        _ => crate::device_wires::on_pulse(world, e, kind, wire, user),
    }
}

/// `tesla_coil/zap_cooldown`: 10 s.
const COIL_ZAP_COOLDOWN: f64 = 10.0;
/// `tesla_zap(zap_range = 10)`.
const COIL_ZAP_RANGE: u32 = 10;

/// `tesla_coil/last_zap`.
#[derive(Component, Clone, Copy, Debug)]
pub struct CoilLastZap(pub f64);

/// `tesla_coil/zap()`: off cooldown and on a powernet, a fifth of the net's power times the
/// coil's multiplier (at most the surplus) is drawn and arcs to the nearest mob.
pub fn tesla_coil_zap(world: &mut World, e: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let (Some(acc), Some(&pos)) = (
        world.get::<Accumulator>(e).copied(),
        world.get::<TilePos>(e),
    ) else {
        return false;
    };
    let AccumulatorKind::TeslaCoil {
        input_power_multiplier,
    } = acc.kind
    else {
        return false;
    };
    if world
        .get::<CoilLastZap>(e)
        .is_some_and(|z| z.0 + COIL_ZAP_COOLDOWN > now)
    {
        return false;
    }
    let power = {
        let mut grid = world.resource_mut::<crate::power::Grid>();
        let Some(net) = grid.node_net(e).and_then(|id| grid.net_mut(id)) else {
            return false;
        };
        let power = (net.avail * 0.2 * input_power_multiplier).min(net.surplus());
        net.load += power;
        power
    };
    // shortcut: no `lightningshock.ogg` (not among the converted sounds).
    world.entity_mut(e).insert(CoilLastZap(now));
    crate::spells::effects::tesla_zap(world, pos, COIL_ZAP_RANGE, power, &mut vec![e], 0);
    true
}

/// The kind's timed `reset(wire)`: back unless the wire is cut.
pub fn reset(world: &mut World, e: Entity, kind: MachineWires, wire: &str) {
    let cut = world.get::<Wires>(e).is_some_and(|w| w.is_cut(wire));
    if cut || kind != MachineWires::AirAlarm {
        return;
    }
    match wire {
        WIRE_POWER => air_alarm(world, e, |a| a.shorted = false),
        WIRE_AI => air_alarm(world, e, |a| a.aidisabled = false),
        _ => {}
    }
}

/// Whether the machine can shock (`shock()`: not without power).
pub fn can_shock(world: &World, e: Entity) -> bool {
    world.get::<Unpowered>(e).is_none()
}

pub(crate) fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// The kind's `get_status()`.
pub fn status(world: &World, e: Entity, kind: MachineWires) -> Vec<String> {
    match kind {
        MachineWires::AirAlarm => {
            let Some(a) = world.get::<AirAlarm>(e) else {
                return Vec::new();
            };
            vec![
                format!(
                    "The interface light is {}.",
                    if a.locked { "red" } else { "green" }
                ),
                format!(
                    "The short indicator is {}.",
                    if a.shorted { "lit" } else { "off" }
                ),
                format!("The AI connection light is {}.", on_off(!a.aidisabled)),
            ]
        }
        MachineWires::FireAlarm => {
            let detect = world
                .get::<FireAlarm>(e)
                .and_then(|a| world.get::<AreaFire>(a.area))
                .is_some_and(|f| f.fire_detect);
            vec![format!("The thermal sensor light is {}.", on_off(detect))]
        }
        _ => crate::device_wires::status(world, e, kind),
    }
}

/// [`MachineWireStatus`] of every machine with a `Wires` window open.
fn refresh_status(world: &mut World) {
    let mut windows = world.query::<&UiWindow>();
    let mut sources: Vec<Entity> = windows
        .iter(world)
        .filter(|w| w.interface == ss13_tgui::interfaces::wires::WiresUi::NAME)
        .map(|w| w.source)
        .collect();
    sources.sort_unstable();
    sources.dedup();
    for e in sources {
        let Some(kind) = kind_of(world, e) else {
            continue;
        };
        let lines = status(world, e, kind);
        if world.get::<MachineWireStatus>(e).map(|s| &s.0) != Some(&lines) {
            world.entity_mut(e).insert(MachineWireStatus(lines));
        }
    }
}

/// `airalarm/ui_status`: a shorted alarm's window closes.
fn close_shorted_alarms(mut windows: Query<&mut UiWindow>, alarms: Query<&AirAlarm>) {
    for mut window in &mut windows {
        if window.interface == ss13_tgui::interfaces::air_alarm::AirAlarmUi::NAME
            && alarms.get(window.source).is_ok_and(|a| a.shorted)
        {
            window.cap_status(UiStatus::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wires::{WireResets, cut, pulse};
    use ss13_mapload::DmAtmosDevice;

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(ss13_world::SpatialIndex::default());
        world.insert_resource(crate::combat::CombatRng(1));
        world.insert_resource({
            let mut colors = WireColors::default();
            colors.seed = 9;
            colors
        });
        world
    }

    #[test]
    fn sets_match_tg() {
        assert_eq!(MachineWires::AirAlarm.set().wires.len(), 9);
        assert_eq!(MachineWires::FireAlarm.set().wires.len(), 4);
        assert_eq!(MachineWires::AtmosShieldGen.set().wires, [WIRE_ACTIVATE]);
        assert_eq!(MachineWires::Emitter.set().wires, [WIRE_ZAP, WIRE_HACK]);
        assert_eq!(
            MachineWires::MassDriver.set().wires,
            [WIRE_LAUNCH, WIRE_SAFETIES]
        );
    }

    #[test]
    fn air_alarm_wires() {
        let mut world = world();
        let e = world
            .spawn((
                AirAlarm::new(None, &DmAtmosDevice::default()),
                TilePos::new(1, 1, 1),
            ))
            .id();
        add(&mut world, e, MachineWires::AirAlarm);
        assert_eq!(
            world.get::<Wires>(e).unwrap().holder,
            WireHolder::Machine(MachineWires::AirAlarm)
        );
        // Power: a pulse shorts it until the 2 minute reset; cut shorts it for good.
        pulse(&mut world, e, WIRE_POWER, None);
        assert!(world.get::<AirAlarm>(e).unwrap().shorted);
        assert_eq!(world.get::<WireResets>(e).unwrap().0.len(), 1);
        reset(&mut world, e, MachineWires::AirAlarm, WIRE_POWER);
        assert!(!world.get::<AirAlarm>(e).unwrap().shorted);
        cut(&mut world, e, WIRE_POWER, None);
        assert!(world.get::<AirAlarm>(e).unwrap().shorted);
        reset(&mut world, e, MachineWires::AirAlarm, WIRE_POWER);
        assert!(world.get::<AirAlarm>(e).unwrap().shorted, "stays while cut");
        cut(&mut world, e, WIRE_POWER, None);
        assert!(!world.get::<AirAlarm>(e).unwrap().shorted);
        // ID scan: a pulse flips the lock, a cut locks it.
        let locked = world.get::<AirAlarm>(e).unwrap().locked;
        pulse(&mut world, e, WIRE_IDSCAN, None);
        assert_eq!(world.get::<AirAlarm>(e).unwrap().locked, !locked);
        world.get_mut::<AirAlarm>(e).unwrap().locked = false;
        cut(&mut world, e, WIRE_IDSCAN, None);
        assert!(world.get::<AirAlarm>(e).unwrap().locked);
        // AI: cut disables, mend enables; speaker: cut mutes.
        cut(&mut world, e, WIRE_AI, None);
        assert!(world.get::<AirAlarm>(e).unwrap().aidisabled);
        cut(&mut world, e, WIRE_AI, None);
        assert!(!world.get::<AirAlarm>(e).unwrap().aidisabled);
        pulse(&mut world, e, WIRE_AI, None);
        assert!(world.get::<AirAlarm>(e).unwrap().aidisabled);
        cut(&mut world, e, WIRE_SPEAKER, None);
        assert!(!world.get::<AirAlarm>(e).unwrap().speaker_enabled);
        // Alarm: cut raises the hazard, pulse clears it.
        cut(&mut world, e, WIRE_ALARM, None);
        assert_eq!(world.get::<AirAlarm>(e).unwrap().danger_level, ALERT_HAZARD);
        cut(&mut world, e, WIRE_ALARM, None);
        pulse(&mut world, e, WIRE_ALARM, None);
        assert_eq!(world.get::<AirAlarm>(e).unwrap().danger_level, ALERT_NONE);
        // Status lines.
        let lines = status(&world, e, MachineWires::AirAlarm);
        assert_eq!(lines[0], "The interface light is red.");
        assert_eq!(lines[1], "The short indicator is off.");
        assert_eq!(lines[2], "The AI connection light is off.");
        // The window needs the panel open.
        let user = world.spawn_empty().id();
        assert!(!wires::interactable(&world, e, user));
        world.entity_mut(e).insert(PanelOpen(true));
        assert!(wires::interactable(&world, e, user));
    }

    #[test]
    fn fire_alarm_wires() {
        let mut world = world();
        let area = world
            .spawn(crate::power::AreaPower {
                requires_power: true,
                power: [true; 3],
                ..default()
            })
            .id();
        let e = world.spawn(TilePos::new(2, 2, 1)).id();
        let other = world.spawn(TilePos::new(4, 2, 1)).id();
        fire_alarm::add_alarm(&mut world, e, area);
        fire_alarm::add_alarm(&mut world, other, area);
        add(&mut world, e, MachineWires::FireAlarm);
        add(&mut world, other, MachineWires::FireAlarm);
        let detect = |w: &World| w.get::<AreaFire>(area).unwrap().fire_detect;
        // A pulse toggles the area's detection.
        pulse(&mut world, e, WIRE_FIRE_DETECT, None);
        assert!(!detect(&world));
        pulse(&mut world, e, WIRE_FIRE_DETECT, None);
        assert!(detect(&world));
        // Cutting one alarm's detection wire: on while another alarm keeps its wire.
        cut(&mut world, e, WIRE_FIRE_DETECT, None);
        assert!(detect(&world));
        // ... and the pulse no longer toggles it.
        assert!(!world.get::<FireAlarm>(e).unwrap().can_toggle_detection);
        cut(&mut world, other, WIRE_FIRE_DETECT, None);
        assert!(!detect(&world), "every alarm cut: detection off");
        cut(&mut world, e, WIRE_FIRE_DETECT, None);
        assert!(detect(&world), "mending restores it");
        assert_eq!(
            status(&world, e, MachineWires::FireAlarm),
            ["The thermal sensor light is on."]
        );
        // Trigger: a pulse rings it; cut, it can no longer be pulled.
        pulse(&mut world, e, WIRE_FIRE_TRIGGER, None);
        assert!(world.get::<FireAlarm>(e).unwrap().ringing.is_some());
        pulse(&mut world, e, WIRE_FIRE_RESET, None);
        assert!(world.get::<FireAlarm>(e).unwrap().ringing.is_none());
        cut(&mut world, e, WIRE_FIRE_TRIGGER, None);
        fire_alarm::alarm(&mut world, e, None);
        assert!(world.get::<FireAlarm>(e).unwrap().ringing.is_none());
        // Reset cut: stuck ringing.
        cut(&mut world, e, WIRE_FIRE_TRIGGER, None);
        fire_alarm::alarm(&mut world, e, None);
        cut(&mut world, e, WIRE_FIRE_RESET, None);
        fire_alarm::reset(&mut world, e, None);
        assert!(world.get::<FireAlarm>(e).unwrap().ringing.is_some());
    }

    #[test]
    fn atmos_shield_gen_activate_wire() {
        let mut world = world();
        let e = world
            .spawn((
                AtmosShieldGen::default(),
                Anchored(false),
                TilePos::new(1, 1, 1),
            ))
            .id();
        add(&mut world, e, MachineWires::AtmosShieldGen);
        pulse(&mut world, e, WIRE_ACTIVATE, None);
        assert_eq!(
            world.get::<AtmosShieldGen>(e).unwrap().on,
            crate::eng_machines::atmos_shield::GenState::Off,
            "not anchored: nothing"
        );
        world.entity_mut(e).insert(Anchored(true));
        pulse(&mut world, e, WIRE_ACTIVATE, None);
        assert_eq!(
            world.get::<AtmosShieldGen>(e).unwrap().on,
            crate::eng_machines::atmos_shield::GenState::WantPower
        );
    }

    #[test]
    fn engineering_machine_wires() {
        let mut world = world();
        world.insert_resource(crate::power::Grid::default());
        // Mass driver: the safeties pulse overdrives it, cutting them calms it; launch drives.
        let driver = world
            .spawn((
                MassDriver {
                    id: None,
                    power: DRIVER_SAFE,
                    drive_range: 50.0,
                    power_per_obj: 0.0,
                },
                Unpowered,
                TilePos::new(1, 1, 1),
            ))
            .id();
        add(&mut world, driver, MachineWires::MassDriver);
        pulse(&mut world, driver, WIRE_SAFETIES, None);
        assert_eq!(
            world.get::<MassDriver>(driver).unwrap().power,
            DRIVER_OVERDRIVE
        );
        cut(&mut world, driver, WIRE_SAFETIES, None);
        assert_eq!(world.get::<MassDriver>(driver).unwrap().power, DRIVER_SAFE);
        // A cut wire cannot be pulsed.
        pulse(&mut world, driver, WIRE_SAFETIES, None);
        assert_eq!(world.get::<MassDriver>(driver).unwrap().power, DRIVER_SAFE);
        // Unpowered, the launch wire does nothing (`drive()` is not operational).
        pulse(&mut world, driver, WIRE_LAUNCH, None);

        // Shield wall generator: one random wire per instance; off the net, the pulse does
        // nothing.
        let gen_e = world
            .spawn((
                ShieldWallGen::of_path("/obj/machinery/power/shieldwallgen/unlocked", 2),
                Anchored(true),
                TilePos::new(3, 1, 1),
            ))
            .id();
        add(&mut world, gen_e, MachineWires::ShieldWallGen);
        assert_eq!(world.get::<Wires>(gen_e).unwrap().colors.len(), 1);
        pulse(&mut world, gen_e, WIRE_ACTIVATE, None);
        assert!(!world.get::<ShieldWallGen>(gen_e).unwrap().on());

        // Emitter: off the net the zap wire fires nothing; the hack wire is inert.
        let emitter = world
            .spawn((
                Emitter::new(ss13_shared::Dir::North, true, 2),
                TilePos::new(5, 1, 1),
            ))
            .id();
        world.init_resource::<crate::power::emitter::PendingBeams>();
        add(&mut world, emitter, MachineWires::Emitter);
        pulse(&mut world, emitter, WIRE_ZAP, None);
        pulse(&mut world, emitter, WIRE_HACK, None);
        assert!(
            world
                .resource::<crate::power::emitter::PendingBeams>()
                .0
                .is_empty()
        );
        // Tesla coil: off the net the zap wire draws nothing and starts no cooldown.
        let coil = world
            .spawn((
                Accumulator::new(AccumulatorKind::TeslaCoil {
                    input_power_multiplier: 0.25,
                }),
                TilePos::new(7, 1, 1),
            ))
            .id();
        add(&mut world, coil, MachineWires::TeslaCoil);
        assert!(!tesla_coil_zap(&mut world, coil));
        assert!(world.get::<CoilLastZap>(coil).is_none());
        // Machines' wire windows need their panel.
        let user = world.spawn_empty().id();
        assert!(!wires::interactable(&world, emitter, user));
        world.entity_mut(emitter).insert(PanelOpen(true));
        assert!(wires::interactable(&world, emitter, user));
    }
}
