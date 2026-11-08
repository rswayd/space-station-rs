//! What circuits touch in the world besides speech and radio: USB cables and ports (tg
//! `datums/components/usb_port.dm`, `wiremod/core/usb_cable.dm`) with the machines' USB
//! components (`teleporter.dm`, `launch_pad.dm`, `launchpad_control.dm`, `quantum_pad.dm`), and
//! the airlock shell's component (`wiremod/shell/airlock.dm`).
//!
//! - A USB cable used on a shell with a USB port (the bot, a modular console) takes that shell's
//!   circuit; used on a machine with a port (teleporter console, launchpad, launchpad console,
//!   quantum pad) within 2 tiles it goes inside the circuit, the machine's component joins the
//!   circuit and its window opens. A cable left too far from its circuit forgets it.
//! - The connection ends ("*snap*", the cable drops at the machine) when the machine or the
//!   circuit goes, when they are more than 2 tiles apart, when the circuit leaves its shell or
//!   when its USB component is ejected from the window.
//! - A machine component's inputs act on the machine at the moment of effect ([`device_input`]).
//!
//! Cost: every 0.5 s one pass over the USB cables holding a circuit and the circuits with a USB
//! connection (a handful); every circuit step one pass over the airlock shells.
// shortcut: range and liveness are polled (there are no move or despawn hooks to listen to), so a
// connection can outlive its range for up to 0.5 s; no cable beam visual; only the listed machines
// have ports (no emitter, thermomachine, pumps, valves, light switch and other USB ports).
// shortcut: airlock events are read from door state each step (`doors.rs` changes state in many
// places with no shared hook); an `Airlock Access Event` needs instant execution, not ported.

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::fidelity::is_under;

use super::circuit::{Kind, Value, boolean, signal};
use super::server::{IntegratedCircuit, Shell, ShellKind, open_window, with_circuit};
use crate::assemblies::server::{drop_out, insert};
use crate::balloon::{balloon_alert, balloon_alert_to_viewers};
use crate::doors::{Door, DoorState, with_door};
use crate::science::quantum_pad;
use crate::science::teleportation::{launchpad, teleporter};
use crate::tools::now;

pub const CABLE: &str = "/obj/item/usb_cable";
/// `USB_CABLE_MAX_RANGE`.
const MAX_RANGE: u32 = 2;
/// How often connections are checked, seconds.
const WATCH_SECS: f64 = 0.5;

/// tg `/obj/item/usb_cable`'s `attached_circuit`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UsbCable {
    pub circuit: Option<Entity>,
}

/// A machine's USB port in use, by that circuit (tg `usb_port`'s `attached_circuit`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbAttached(pub Entity);

/// A machine connected to a circuit.
#[derive(Clone, Debug, PartialEq)]
pub struct UsbLink {
    pub machine: Entity,
    pub cable: Entity,
    /// The machine's component in the circuit.
    pub node: u32,
    /// A teleporter console's target the circuit last heard of (`COMSIG_TELEPORTER_NEW_TARGET`);
    /// `None` before the first report.
    last_target: Option<Option<Entity>>,
}

/// The component a machine's USB port gives (`AddComponent(/datum/component/usb_port, ...)`).
fn port_kind(world: &World, e: Entity) -> Option<Kind> {
    let path = world.get::<DmObj>(e)?.path.as_str();
    if is_under(path, teleporter::CONSOLE) {
        Some(Kind::TeleporterConsole)
    } else if is_under(path, launchpad::CONSOLE) {
        Some(Kind::LaunchpadConsole)
    } else if is_under(path, launchpad::PAD) && !is_under(path, launchpad::BRIEFCASE_PAD) {
        Some(Kind::Launchpad)
    } else if is_under(path, quantum_pad::PAD) {
        Some(Kind::QuantumPad)
    } else {
        None
    }
}

fn alive(world: &World, e: Entity) -> bool {
    world.get_entity(e).is_ok()
}

/// `IN_GIVEN_RANGE(a, b, USB_CABLE_MAX_RANGE)`.
fn in_range(world: &World, a: Entity, b: Entity) -> bool {
    let (Some(a), Some(b)) = (
        crate::explosions::turf_of(world, a),
        crate::explosions::turf_of(world, b),
    ) else {
        return false;
    };
    a.chebyshev(b).is_some_and(|d| d <= MAX_RANGE)
}

fn sound(world: &mut World, e: Entity, file: &str) {
    if let Some(pos) = crate::explosions::turf_of(world, e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, 20.0, true));
    }
}

/// The cable's `pre_attack` on a circuit, a shell or a machine. True when handled.
pub fn cable_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<UsbCable>(held).is_none() {
        return false;
    }
    if world.get::<IntegratedCircuit>(target).is_some() {
        balloon_alert(
            world,
            mob,
            held,
            "circuit needs to be in a compatible shell",
        );
        return true;
    }
    if let Some(shell) = world.get::<Shell>(target).copied() {
        if !shell.kind.usb() {
            balloon_alert(world, mob, target, "this shell has no usb ports");
            return true;
        }
        let Some(circuit) = shell.circuit else {
            balloon_alert(world, mob, target, "no circuit inside");
            return true;
        };
        world.entity_mut(held).insert(UsbCable {
            circuit: Some(circuit),
        });
        balloon_alert(world, mob, held, "connected to circuit\nconnect to a port");
        sound(world, held, "sound/machines/pda_button/pda_button1.ogg");
        return true;
    }
    let Some(kind) = port_kind(world, target) else {
        return false;
    };
    if let Some(&UsbAttached(circuit)) = world.get::<UsbAttached>(target) {
        if alive(world, circuit) {
            balloon_alert(world, mob, target, "usb already connected");
            return true;
        }
        world.entity_mut(target).remove::<UsbAttached>();
    }
    let Some(circuit) = world
        .get::<UsbCable>(held)
        .and_then(|c| c.circuit)
        .filter(|&c| world.get::<IntegratedCircuit>(c).is_some())
    else {
        balloon_alert(world, mob, held, "connect to a shell first");
        return true;
    };
    if !in_range(world, circuit, target) {
        balloon_alert(world, mob, held, "too far away");
        return true;
    }
    attach(world, circuit, target, held, kind);
    open_window(world, client, mob, circuit);
    balloon_alert(world, mob, held, "connected to machine");
    sound(world, target, "sound/items/tools/screwdriver2.ogg");
    true
}

/// `on_atom_usb_cable_try_attach`: the cable goes in, the machine's component joins.
fn attach(world: &mut World, circuit: Entity, machine: Entity, cable: Entity, kind: Kind) {
    let Some(node) = with_circuit(world, circuit, |c, ctx| c.add_usb(kind, machine, ctx.rng))
    else {
        return;
    };
    let link = UsbLink {
        machine,
        cable,
        node,
        last_target: None,
    };
    if let Some(mut ic) = world.get_mut::<IntegratedCircuit>(circuit) {
        ic.usb.push(link);
    }
    world.entity_mut(machine).insert(UsbAttached(circuit));
    insert(world, cable, circuit);
    if kind == Kind::TeleporterConsole {
        report_target(world, circuit, machine);
    }
}

/// `detach`: the component leaves the circuit, the cable drops at the machine.
pub fn detach(world: &mut World, circuit: Entity, machine: Entity) {
    let Some(link) = world
        .get_mut::<IntegratedCircuit>(circuit)
        .and_then(|mut ic| {
            let i = ic.usb.iter().position(|l| l.machine == machine)?;
            Some(ic.usb.remove(i))
        })
    else {
        return;
    };
    with_circuit(world, circuit, |c, _| c.remove(link.node));
    if alive(world, machine) {
        world.entity_mut(machine).remove::<UsbAttached>();
    }
    if alive(world, link.cable) {
        let at = if alive(world, machine) {
            machine
        } else {
            circuit
        };
        drop_out(world, link.cable, at);
        balloon_alert_to_viewers(world, link.cable, "*snap*", None, &[]);
    }
}

/// Every connection of a circuit ends (it left its shell).
pub fn detach_all(world: &mut World, circuit: Entity) {
    let machines: Vec<Entity> = world
        .get::<IntegratedCircuit>(circuit)
        .map(|ic| ic.usb.iter().map(|l| l.machine).collect())
        .unwrap_or_default();
    for machine in machines {
        detach(world, circuit, machine);
    }
}

/// What tg does on `COMSIG_MOVABLE_MOVED` and qdel of the two ends, by polling.
pub(super) fn watch(world: &mut World, mut elapsed: Local<f64>) {
    *elapsed += world.resource::<Time>().delta_secs_f64();
    if *elapsed < WATCH_SECS {
        return;
    }
    *elapsed = 0.0;
    let cables: Vec<(Entity, Entity)> = world
        .query::<(Entity, &UsbCable)>()
        .iter(world)
        .filter_map(|(e, c)| c.circuit.map(|circuit| (e, circuit)))
        .collect();
    for (cable, circuit) in cables {
        // `check_in_range`; `on_circuit_qdeling`.
        if alive(world, circuit) && in_range(world, cable, circuit) {
            continue;
        }
        world.entity_mut(cable).insert(UsbCable::default());
        if alive(world, circuit) {
            balloon_alert_to_viewers(world, cable, "detached, too far away", None, &[]);
        }
    }
    let links: Vec<(Entity, Entity, Kind)> = world
        .query::<(Entity, &IntegratedCircuit)>()
        .iter(world)
        .flat_map(|(circuit, ic)| {
            ic.usb.iter().filter_map(move |l| {
                let i = ic.circuit.index_of(l.node)?;
                Some((circuit, l.machine, ic.circuit.nodes[i].kind))
            })
        })
        .collect();
    for (circuit, machine, kind) in links {
        if !alive(world, machine) || !in_range(world, circuit, machine) {
            detach(world, circuit, machine);
        } else if kind == Kind::TeleporterConsole {
            report_target(world, circuit, machine);
        }
    }
}

/// `on_teleporter_new_target`, `update_targets` on connecting: a changed target goes out on
/// `Current Target`, and the list on `Possible Targets`.
fn report_target(world: &mut World, circuit: Entity, machine: Entity) {
    let target = teleporter::console_state(world, machine).target;
    let Some(link) = world
        .get_mut::<IntegratedCircuit>(circuit)
        .and_then(|mut ic| {
            let l = ic.usb.iter_mut().find(|l| l.machine == machine)?;
            let first = l.last_target.is_none();
            let changed = l.last_target != Some(target);
            l.last_target = Some(target);
            changed.then_some((l.node, first))
        })
    else {
        return;
    };
    let (node, first) = link;
    let targets = teleporter::get_targets(world, machine);
    let name = target.map(|t| {
        targets.iter().find(|(_, e)| *e == t).map_or_else(
            || crate::assemblies::server::name_of(world, t),
            |(n, _)| n.clone(),
        )
    });
    let current = name.map_or(Value::Null, Value::Str);
    let mut outs = vec![(0, current)];
    if first {
        outs.push((1, target_list(&targets)));
    }
    set_outputs(world, circuit, node, outs);
}

fn target_list(targets: &[(String, Entity)]) -> Value {
    Value::List(targets.iter().map(|(n, _)| Value::Str(n.clone())).collect())
}

fn set_outputs(world: &mut World, circuit: Entity, node: u32, outs: Vec<(usize, Value)>) {
    with_circuit(world, circuit, |c, _| {
        for (port, value) in outs {
            c.set_output(node, port, value);
        }
    });
}

fn send_signal(world: &mut World, circuit: Entity, node: u32, port: usize) {
    let v = signal(now(world));
    set_outputs(world, circuit, node, vec![(port, v)]);
}

/// A machine or airlock component's `input_received` on `port`, at the moment of effect.
pub(super) fn device_input(
    world: &mut World,
    circuit: Entity,
    shell: Option<Entity>,
    node: u32,
    port: usize,
) {
    let Some((kind, machine, values)) = world
        .get::<IntegratedCircuit>(circuit)
        .and_then(|ic| ic.circuit.index_of(node).map(|i| &ic.circuit.nodes[i]))
        .map(|n| {
            let values: Vec<Value> = n.inputs.iter().map(|p| p.value.clone()).collect();
            (n.kind, n.usb.filter(|&m| alive(world, m)), values)
        })
    else {
        return;
    };
    match kind {
        Kind::Airlock => {
            if let Some(door) = shell.filter(|&s| world.get::<Door>(s).is_some()) {
                airlock_input(world, door, port, &values);
            }
        }
        Kind::TeleporterConsole => {
            if let Some(console) = machine {
                teleporter_input(world, circuit, node, console, port, &values);
            }
        }
        Kind::QuantumPad => {
            let Some(pad) = machine else {
                return;
            };
            let target = match values.first() {
                Some(Value::Entity(e)) => Some(*e),
                _ => None,
            };
            if !quantum_pad::circuit_teleport(world, pad, target) {
                send_signal(world, circuit, node, 0);
            }
        }
        Kind::Launchpad => launchpad_input(world, circuit, node, machine, port, &values),
        Kind::LaunchpadConsole => {
            let Some(console) = machine else {
                return;
            };
            let pads = launchpad::live_pads(world, console);
            if pads.is_empty() {
                pad_fail(world, circuit, node, "No launchpads connected!");
                return;
            }
            let id = values.first().and_then(Value::num).unwrap_or(0.0);
            if id == 0.0 {
                return;
            }
            let pad = (id >= 1.0 && id.is_finite())
                .then(|| pads.get(id as usize - 1).copied())
                .flatten();
            if pad.is_none() {
                pad_fail(world, circuit, node, "Invalid launchpad selected!");
                return;
            }
            // Past the `Launchpad ID` port the ports are the pad's.
            launchpad_input(
                world,
                circuit,
                node,
                pad,
                port.saturating_sub(1),
                &values[1..],
            );
        }
        _ => {}
    }
}

fn pad_fail(world: &mut World, circuit: Entity, node: u32, why: &str) {
    let v = signal(now(world));
    set_outputs(
        world,
        circuit,
        node,
        vec![(2, Value::Str(why.to_owned())), (3, v)],
    );
}

/// `bluespace_launchpad/input_received`: the offsets (ports 0, 1), `Send` (2), `Retrieve` (3).
fn launchpad_input(
    world: &mut World,
    circuit: Entity,
    node: u32,
    pad: Option<Entity>,
    port: usize,
    values: &[Value],
) {
    let Some(pad) = pad else {
        pad_fail(world, circuit, node, "Not connected!");
        return;
    };
    let offset = |i: usize| values.get(i).and_then(Value::num).filter(|n| n.is_finite());
    let (x, y) = (offset(0), offset(1));
    let range = f64::from(launchpad::range(world, pad));
    if x.is_some_and(|v| v.abs() > range) || y.is_some_and(|v| v.abs() > range) {
        pad_fail(world, circuit, node, "Out of range!");
        return;
    }
    launchpad::set_offset(
        world,
        pad,
        x.map(|v| v.round() as i32),
        y.map(|v| v.round() as i32),
    );
    if port < 2 {
        return;
    }
    if let Some(why) = launchpad::teleport_checks(world, pad) {
        pad_fail(world, circuit, node, why);
        return;
    }
    let triggered = |i: usize| port == i && values.get(i).is_some_and(Value::truthy);
    if triggered(2) {
        launchpad::doteleport(world, pad, true);
        send_signal(world, circuit, node, 0);
    } else if triggered(3) {
        launchpad::doteleport(world, pad, false);
        send_signal(world, circuit, node, 1);
    }
}

/// `teleporter_control_console/input_received`: `Set Target` (1), `Update Targets` (2).
fn teleporter_input(
    world: &mut World,
    circuit: Entity,
    node: u32,
    console: Entity,
    port: usize,
    values: &[Value],
) {
    match port {
        1 => {
            let name = values.first().map(Value::text).unwrap_or_default();
            if !teleporter::circuit_set_target(world, console, &name) {
                send_signal(world, circuit, node, 2);
            }
        }
        2 => {
            let list = target_list(&teleporter::get_targets(world, console));
            set_outputs(world, circuit, node, vec![(1, list)]);
        }
        _ => {}
    }
}

/// `airlock/input_received`: `Bolt`, `Unbolt`, `Open`, `Close`.
fn airlock_input(world: &mut World, door: Entity, port: usize, values: &[Value]) {
    if !values.get(port).is_some_and(Value::truthy) {
        return;
    }
    let closed = world
        .get::<Door>(door)
        .map(|d| d.state == DoorState::Closed);
    match (port, closed) {
        (0, _) => crate::wires::set_bolts(world, door, true),
        (1, _) => crate::wires::set_bolts(world, door, false),
        (2, Some(true)) => {
            with_door(world, door, |d, appearance, ctx| {
                let moved = ctx.open(d, appearance, false);
                ((), moved)
            });
        }
        (3, Some(false)) => {
            with_door(world, door, |d, appearance, ctx| {
                let moved = ctx.close(d, appearance, false);
                ((), moved)
            });
        }
        _ => {}
    }
}

/// What the airlock component last reported.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct AirlockSeen {
    open: bool,
    bolted: bool,
}

/// `on_airlock_open`, `on_airlock_closed`, `on_airlock_set_bolted`, read from the doors' state.
pub(super) fn airlock_states(world: &mut World) {
    let changed: Vec<(Entity, Entity, AirlockSeen, Option<AirlockSeen>)> = world
        .query::<(Entity, &Shell, &Door, Option<&AirlockSeen>)>()
        .iter(world)
        .filter_map(|(e, shell, door, seen)| {
            let circuit = shell.circuit.filter(|_| shell.kind == ShellKind::Airlock)?;
            let state = AirlockSeen {
                open: matches!(door.state, DoorState::Open | DoorState::Opening { .. }),
                bolted: door.locked,
            };
            (seen.copied() != Some(state)).then_some((e, circuit, state, seen.copied()))
        })
        .collect();
    for (door, circuit, state, before) in changed {
        world.entity_mut(door).insert(state);
        let t = now(world);
        with_circuit(world, circuit, |c, _| {
            let Some(id) = c
                .nodes
                .iter()
                .find(|n| n.kind == Kind::Airlock)
                .map(|n| n.id)
            else {
                return;
            };
            c.set_output(id, 0, boolean(state.open));
            c.set_output(id, 1, boolean(state.bolted));
            let Some(before) = before else {
                return;
            };
            if before.open != state.open {
                c.set_output(id, if state.open { 2 } else { 3 }, signal(t));
            }
            if before.bolted != state.bolted {
                c.set_output(id, if state.bolted { 4 } else { 5 }, signal(t));
            }
        });
    }
}

/// `usb_port/on_examine`.
pub(super) fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    use ss13_shared::chat::Span;
    if port_kind(world, target).is_none() {
        return;
    }
    let text = match world.get::<UsbAttached>(target) {
        None => "There is a USB port on the front.".to_owned(),
        Some(&UsbAttached(circuit)) => {
            let shell = world
                .get::<IntegratedCircuit>(circuit)
                .and_then(|ic| ic.shell);
            let who = crate::assemblies::server::name_of(world, shell.unwrap_or(circuit));
            format!("{who} is connected to it by a USB port.")
        }
    };
    lines.push((Span::Notice, text));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shells_with_usb_ports_follow_tg() {
        assert!(ShellKind::Bot.usb());
        assert!(ShellKind::Console.usb());
        assert!(!ShellKind::CompactRemote.usb());
        assert!(!ShellKind::Airlock.usb());
    }
}
