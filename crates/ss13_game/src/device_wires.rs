//! The wire sets of other machines, cyborgs, mechs, MODsuits and radios (tg
//! `code/datums/wires/`: `microwave.dm`, `holopad.dm`, `fax.dm`, `r_n_d.dm`, `robot.dm`,
//! `mecha.dm`, `mod.dm`, `radio.dm`), on top of [`crate::machine_wires`]: the same
//! [`WireHolder::Machine`] holder, these kinds' sets, panels and effects.
//!
//! - Panels: a fax and a holopad take a screwdriver (`crate::machine_wires`); a microwave and an
//!   R&D lathe keep their frame's panel; a cyborg's wires are exposed by a screwdriver with its
//!   cover open and no cell (`wiresexposed`); a mech's panel (`PANEL_OPEN`) by a screwdriver
//!   while empty; a MOD's wires are reachable while its cover is open (`crate::modsuit`); a
//!   radio is unscrewed by a right-click or combat-mode screwdriver (`screwdriver_act_secondary`)
//!   and then shows its wires in hand (`interact`) or, an intercom, on a hand click.
//! - Per-holder wire state the holder types do not have lives in [`WireState`].
// shortcut: no holopad recordings (loop and replay only keep their state), no microwave
// vampire charging (the mode wire's swap ends in "needs upgrades"), no cyborg built-in camera
// or model reset, no mech internal damage, overclock, lights or `try_attack` (those wires keep
// their state for the status lights), no MOD malfunction effects beyond the window's flag
// (`interface_break` is not in the MODsuit window data), no fax jamming. Add with those systems.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::construction::PanelOpen;
use crate::kitchen::Microwave;
use crate::machine_wires::{
    MachineWires, WIRE_ACTIVATE, WIRE_LAUNCH, announce, on_off, panel_open,
};
use crate::mecha::core::Mech;
use crate::modsuit::server::ModSuit;
use crate::radio::{Radio, RadioKind};
use crate::science::lathe::Lathe;
use crate::service::Fax;
use crate::silicon::chassis::Chassis;
use crate::silicon::cyborg::Cyborg;
use crate::silicon::holopad::Holopad;
use crate::tools::{ToolKind, tell, tool_of};
use crate::wires::{
    self, Electrified, MACHINE_DEFAULT_ELECTRIFY_TIME, WIRE_AI, WIRE_DISABLE, WIRE_HACK,
    WIRE_IDSCAN, WIRE_INTERFACE, WIRE_LIGHT, WIRE_SHOCK, WIRE_THROW, WIRE_ZAP, WireColors, WireSet,
    Wires, with_duds,
};

pub const WIRE_CAMERA: &str = "Camera";
pub const WIRE_DISARM: &str = "Disarm";
pub const WIRE_LAWSYNC: &str = "AI Law Synchronization";
pub const WIRE_LOADCHECK: &str = "Load Check";
pub const WIRE_LOCKDOWN: &str = "Lockdown";
pub const WIRE_LOOP_MODE: &str = "Loop mode";
pub const WIRE_MODE_SELECT: &str = "Mode Select";
pub const WIRE_OVERCLOCK: &str = "Overclock";
pub const WIRE_REPLAY_MODE: &str = "Replay mode";
pub const WIRE_RESET_MODEL: &str = "Reset Model";
pub const WIRE_RX: &str = "Receive";
pub const WIRE_SIGNAL: &str = "Signal";
pub const WIRE_TX: &str = "Transmit";

/// `mecha_flags`: `PANEL_OPEN`, `HAS_LIGHTS`.
pub const MECH_PANEL_OPEN: u32 = 1 << 0;
pub const MECH_HAS_LIGHTS: u32 = 1 << 6;

/// The wire-driven vars of these holders that their own state does not carry.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireState {
    /// Holopad `loop_mode`, `replay_mode`.
    pub loop_mode: bool,
    pub replay_mode: bool,
    /// Fax `hurl_contents` (received items are thrown), `allow_exotic_faxes`.
    pub hurl: bool,
    pub exotic: bool,
    /// R&D machine `hacked`, `disabled` (no insertion: "belts disabled!").
    pub hacked: bool,
    pub disabled: bool,
    /// Mech `MECHA_INT_SHORT_CIRCUIT`, `overclock_mode`, `light_on`.
    pub short_circuit: bool,
    pub overclock: bool,
    pub lights: bool,
    /// MOD `malfunctioning`, `interface_break`.
    pub malfunctioning: bool,
    pub interface_break: bool,
}

pub(crate) fn build(app: &mut App) {
    app.add_systems(Update, attach_wires);
    app.add_click_handler(ClickPhase::AttackSelf, 89, attack_self);
}

/// The set of a kind of this module.
pub fn set(kind: MachineWires) -> WireSet {
    let (key, proper_name, list, duds): (_, _, &[&str], _) = match kind {
        MachineWires::Microwave => (
            crate::kitchen::MICROWAVE_ROOT,
            "Microwave",
            &[WIRE_ACTIVATE, WIRE_MODE_SELECT],
            0,
        ),
        MachineWires::Holopad => (
            "/obj/machinery/holopad",
            "holopad",
            &[WIRE_LOOP_MODE, WIRE_REPLAY_MODE],
            0,
        ),
        MachineWires::Fax => (
            "/obj/machinery/fax",
            "Fax Unit",
            &[WIRE_SHOCK, WIRE_SIGNAL, WIRE_THROW, WIRE_LOADCHECK],
            1,
        ),
        MachineWires::Rnd => (
            "/obj/machinery/rnd",
            "R&D Machinery",
            &[WIRE_HACK, WIRE_DISABLE, WIRE_SHOCK],
            5,
        ),
        MachineWires::Robot => (
            "/mob/living/silicon/robot",
            "Cyborg",
            &[
                WIRE_AI,
                WIRE_CAMERA,
                WIRE_LAWSYNC,
                WIRE_LOCKDOWN,
                WIRE_RESET_MODEL,
            ],
            2,
        ),
        // `WIRE_LIGHT` only with `HAS_LIGHTS` (taken out in `attach_wires`).
        MachineWires::Mecha => (
            "/obj/vehicle/sealed/mecha",
            "Mecha Control",
            &[
                WIRE_IDSCAN,
                WIRE_DISARM,
                WIRE_ZAP,
                WIRE_OVERCLOCK,
                WIRE_LAUNCH,
                WIRE_LIGHT,
            ],
            3,
        ),
        MachineWires::Mod => (
            "/obj/item/mod/control",
            "MOD control unit",
            &[WIRE_HACK, WIRE_DISABLE, WIRE_SHOCK, WIRE_INTERFACE],
            2,
        ),
        _ => (
            "/obj/item/radio",
            "Radio",
            &[WIRE_SIGNAL, WIRE_RX, WIRE_TX],
            0,
        ),
    };
    WireSet {
        key,
        proper_name,
        wires: with_duds(list, duds),
    }
}

type NewDevices = (
    Without<Wires>,
    Or<(
        Added<Microwave>,
        Added<Holopad>,
        Added<Fax>,
        Added<Lathe>,
        Added<Cyborg>,
        Added<Mech>,
        Added<ModSuit>,
        Added<Radio>,
    )>,
);

/// The kind of a holder of this module.
fn new_kind(world: &World, e: Entity) -> Option<MachineWires> {
    let entity = world.get_entity(e).ok()?;
    Some(if entity.contains::<Microwave>() {
        MachineWires::Microwave
    } else if entity.contains::<Holopad>() {
        MachineWires::Holopad
    } else if entity.contains::<Fax>() {
        MachineWires::Fax
    } else if entity.contains::<Lathe>() {
        MachineWires::Rnd
    } else if entity.contains::<Cyborg>() {
        MachineWires::Robot
    } else if entity.contains::<Mech>() {
        MachineWires::Mecha
    } else if entity.contains::<ModSuit>() {
        MachineWires::Mod
    } else if entity.contains::<Radio>() {
        MachineWires::Radio
    } else {
        return None;
    })
}

/// `set_wires(new /datum/wires/<kind>(src))`.
fn attach_wires(new: Query<Entity, NewDevices>, mut commands: Commands) {
    let found: Vec<Entity> = new.iter().collect();
    if found.is_empty() {
        return;
    }
    commands.queue(move |world: &mut World| {
        for e in found {
            if world.get::<Wires>(e).is_none()
                && let Some(kind) = new_kind(world, e)
            {
                add(world, e, kind);
            }
        }
    });
}

/// Inserts the kind's wires and [`WireState`] on `e`.
pub fn add(world: &mut World, e: Entity, kind: MachineWires) {
    let mut w =
        crate::machine_wires::wires_for(&mut world.get_resource_or_init::<WireColors>(), kind, e);
    if kind == MachineWires::Mecha
        && world
            .get::<Mech>(e)
            .is_some_and(|m| m.flags & MECH_HAS_LIGHTS == 0)
    {
        w.colors.retain(|(_, wire)| wire != WIRE_LIGHT);
    }
    // Radios mapped or built with their transmitter cut (`wires?.cut(WIRE_TX)`).
    if kind == MachineWires::Radio && world.get::<Radio>(e).is_some_and(|r| r.tx_cut) {
        w.cut.push(WIRE_TX.to_owned());
    }
    world.entity_mut(e).insert((w, WireState::default()));
}

fn state(world: &mut World, e: Entity, f: impl FnOnce(&mut WireState)) {
    if let Some(mut s) = world.get_mut::<WireState>(e) {
        f(&mut s);
    }
}

fn flag(world: &World, e: Entity, f: impl FnOnce(&WireState) -> bool) -> bool {
    world.get::<WireState>(e).is_some_and(f)
}

/// The kind's `interactable(user)`; `None`: the panel decides.
pub fn interactable(world: &World, e: Entity, kind: MachineWires) -> Option<bool> {
    Some(match kind {
        MachineWires::Robot => {
            panel_open(world, e) && world.get::<Chassis>(e).is_some_and(|c| c.opened)
        }
        MachineWires::Mecha => world
            .get::<Mech>(e)
            .is_some_and(|m| m.flags & MECH_PANEL_OPEN != 0),
        MachineWires::Mod => world.get::<ModSuit>(e).is_some_and(|s| s.open),
        // `can_access_wires`: a shell that is not a machine always (no shell locking).
        MachineWires::WireBundle => true,
        // `/obj/machinery/gizmo` starts and stays open (`panel_open = TRUE`); the handheld one
        // has no panel.
        MachineWires::Gizmo => true,
        _ => return None,
    })
}

/// Whether an empty hand on the open holder opens the wires: not on items (picked up), a
/// cyborg or a mech (their `attack_hand`); an intercom does.
pub fn hand_opens(world: &World, e: Entity, kind: MachineWires) -> bool {
    match kind {
        MachineWires::Robot
        | MachineWires::Mecha
        | MachineWires::Mod
        | MachineWires::WireBundle => false,
        // The handheld gizmo is picked up by hand.
        MachineWires::Gizmo => world.get::<ss13_mapload::DmItem>(e).is_none(),
        MachineWires::Radio => world
            .get::<Radio>(e)
            .is_some_and(|r| r.kind == RadioKind::Intercom),
        _ => true,
    }
}

/// The kinds' own screwdriver: `Some(handled)` for a cyborg, a mech and a radio.
pub fn screwdriver(
    world: &mut World,
    ctx: &ClickCtx,
    kind: MachineWires,
    held: Entity,
) -> Option<bool> {
    if tool_of(world, held).map(|t| t.0) != Some(ToolKind::Screwdriver) {
        return None;
    }
    let e = ctx.target;
    match kind {
        // `robot/screwdriver_act`: `if(opened && !cell)` the wires; with a cell the radio keys
        // (not ported).
        MachineWires::Robot => {
            let ready = world
                .get::<Chassis>(e)
                .is_some_and(|c| c.opened && c.cell.is_none());
            if !ready {
                return None;
            }
            let open = !panel_open(world, e);
            world.entity_mut(e).insert(PanelOpen(open));
            let text = if open {
                "Вы открываете доступ к проводке."
            } else {
                "Вы закрываете доступ к проводке."
            };
            tell(world, ctx.client, Span::Notice, text);
            Some(true)
        }
        // `mecha/screwdriver_act`: not with someone inside.
        MachineWires::Mecha => {
            let occupied = world.get::<Mech>(e).map(|m| m.occupant.is_some())?;
            if occupied {
                tell(world, ctx.client, Span::Warning, "панель заблокирована");
                return Some(true);
            }
            let open = {
                let mut m = world.get_mut::<Mech>(e)?;
                m.flags ^= MECH_PANEL_OPEN;
                m.flags & MECH_PANEL_OPEN != 0
            };
            let text = if open {
                "панель открыта"
            } else {
                "панель закрыта"
            };
            tell(world, ctx.client, Span::Notice, text);
            Some(true)
        }
        // `radio/screwdriver_act_secondary`, or a combat-mode left click (`item_interaction`).
        MachineWires::Radio => {
            let combat = world
                .get::<crate::combat::CombatMode>(ctx.mob)
                .is_some_and(|c| c.0);
            if !ctx.modifiers.right && !combat {
                return None;
            }
            let open = !panel_open(world, e);
            world.entity_mut(e).insert(PanelOpen(open));
            let text = if open {
                "Радио теперь можно модифицировать и подсоединять!"
            } else {
                "Радио больше нельзя модифицировать или подсоединять!"
            };
            tell(world, ctx.client, Span::Notice, text);
            Some(true)
        }
        _ => None,
    }
}

/// `radio/interact` in hand: an unscrewed radio shows its wires.
fn attack_self(world: &mut World, ctx: &ClickCtx) -> bool {
    if crate::machine_wires::kind_of(world, ctx.target) != Some(MachineWires::Radio)
        || !panel_open(world, ctx.target)
    {
        return false;
    }
    wires::interact(world, ctx.client, ctx.mob, ctx.target);
    true
}

/// `ui_act` of the MOD's and the mech's wires: an electrified MOD or a short-circuited mech
/// shocks the hand first. False: the act stops.
pub fn act_allowed(world: &mut World, e: Entity, kind: MachineWires, user: Entity) -> bool {
    let shocks = match kind {
        MachineWires::Mod => world.get::<Electrified>(e).is_some(),
        MachineWires::Mecha => flag(world, e, |s| s.short_circuit),
        _ => false,
    };
    !(shocks && wires::shock(world, e, user, 100.0))
}

fn set_electrified(world: &mut World, e: Entity, value: Option<Electrified>) {
    wires::set_electrified(world, e, value);
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
        (MachineWires::Holopad, WIRE_REPLAY_MODE) => state(world, e, |s| s.replay_mode = !mend),
        (MachineWires::Fax | MachineWires::Mod, WIRE_SHOCK) => {
            set_electrified(world, e, (!mend).then_some(Electrified::Permanent))
        }
        (MachineWires::Fax, WIRE_SIGNAL) => {
            if let Some(mut f) = world.get_mut::<Fax>(e) {
                f.visible = mend;
            }
        }
        (MachineWires::Fax, WIRE_THROW) => state(world, e, |s| s.hurl = !mend),
        (MachineWires::Fax, WIRE_LOADCHECK) => state(world, e, |s| s.exotic = !mend),
        (MachineWires::Rnd, WIRE_HACK) => state(world, e, |s| s.hacked = !mend),
        (MachineWires::Rnd, WIRE_DISABLE) => state(world, e, |s| s.disabled = !mend),
        (MachineWires::Robot, WIRE_AI) => {
            if !mend {
                crate::silicon::cyborg::disconnect(world, e);
            }
        }
        (MachineWires::Robot, WIRE_LAWSYNC) => {
            let emagged = world.get::<Chassis>(e).is_some_and(|c| c.emagged);
            if let Some(mut c) = world.get_mut::<Cyborg>(e)
                && (!mend || !emagged)
            {
                c.lawupdate = mend;
            }
        }
        (MachineWires::Robot, WIRE_LOCKDOWN) => {
            crate::silicon::chassis::set_lockdown(world, e, !mend);
        }
        (MachineWires::Mecha, WIRE_IDSCAN) => {
            if !mend && let Some(mut m) = world.get_mut::<Mech>(e) {
                m.flags &= !crate::mecha::core::ID_LOCK_ON;
            }
        }
        (MachineWires::Mecha, WIRE_DISARM) => set_mech_disarmed(world, e, !mend),
        (MachineWires::Mecha, WIRE_ZAP) => {
            state(world, e, |s| s.short_circuit = !mend);
            if !mend && let Some(user) = source {
                wires::shock(world, e, user, 50.0);
            }
        }
        (MachineWires::Mecha, WIRE_LIGHT) => state(world, e, |s| s.lights = !mend),
        (MachineWires::Mecha, WIRE_OVERCLOCK) => {
            if !mend {
                state(world, e, |s| s.overclock = false);
            }
        }
        (MachineWires::Mod, WIRE_HACK) => {
            if !mend && let Some(mut s) = world.get_mut::<ModSuit>(e) {
                s.req_access.clear();
            }
        }
        (MachineWires::Mod, WIRE_DISABLE) => state(world, e, |s| s.malfunctioning = !mend),
        (MachineWires::Mod, WIRE_INTERFACE) => state(world, e, |s| s.interface_break = !mend),
        // `radio/talk_into`: `if(wires.is_cut(WIRE_TX))` nothing is sent.
        (MachineWires::Radio, WIRE_TX) => {
            if let Some(mut r) = world.get_mut::<Radio>(e) {
                r.tx_cut = !mend;
            }
        }
        _ => {}
    }
}

/// `equipment_disabled` (`crate::emp::power::MechEquipmentDisabled`, shared with the EMP).
fn set_mech_disarmed(world: &mut World, e: Entity, disabled: bool) {
    if disabled {
        world
            .entity_mut(e)
            .insert(crate::emp::power::MechEquipmentDisabled);
    } else {
        world
            .entity_mut(e)
            .remove::<crate::emp::power::MechEquipmentDisabled>();
    }
}

/// tg `select_active_ai(user, z)`: a living AI with the fewest cyborgs.
fn pick_ai(world: &mut World) -> Option<Entity> {
    let mut ais = world.query::<(Entity, &crate::silicon::ai::AiCore, &crate::health::Stat)>();
    ais.iter(world)
        .filter(|(_, _, s)| **s != crate::health::Stat::Dead)
        .min_by_key(|(_, core, _)| core.connected_robots.len())
        .map(|(e, ..)| e)
}

/// The kind's `on_pulse(wire, user)`.
pub fn on_pulse(
    world: &mut World,
    e: Entity,
    kind: MachineWires,
    wire: &str,
    _user: Option<Entity>,
) {
    match (kind, wire) {
        (MachineWires::Microwave, WIRE_ACTIVATE) => crate::kitchen::wire_cook(world, e),
        (MachineWires::Holopad, WIRE_LOOP_MODE) => state(world, e, |s| s.loop_mode = !s.loop_mode),
        (MachineWires::Holopad, WIRE_REPLAY_MODE) => {
            state(world, e, |s| s.replay_mode = !s.replay_mode);
        }
        (MachineWires::Fax | MachineWires::Mod, WIRE_SHOCK) => set_electrified(
            world,
            e,
            Some(Electrified::Timed(MACHINE_DEFAULT_ELECTRIFY_TIME)),
        ),
        (MachineWires::Fax, WIRE_SIGNAL) => {
            if let Some(mut f) = world.get_mut::<Fax>(e) {
                f.visible = !f.visible;
            }
        }
        (MachineWires::Fax, WIRE_THROW) => state(world, e, |s| s.hurl = !s.hurl),
        (MachineWires::Fax, WIRE_LOADCHECK) => state(world, e, |s| s.exotic = !s.exotic),
        (MachineWires::Rnd, WIRE_HACK) => state(world, e, |s| s.hacked = !s.hacked),
        (MachineWires::Rnd, WIRE_DISABLE) => state(world, e, |s| s.disabled = !s.disabled),
        (MachineWires::Robot, WIRE_AI) => {
            if world.get::<Chassis>(e).is_some_and(|c| c.emagged) {
                return;
            }
            let new = pick_ai(world);
            let old = world.get::<Cyborg>(e).and_then(|c| c.connected_ai);
            if let Some(ai) = new.filter(|&a| Some(a) != old) {
                crate::silicon::cyborg::connect(world, e, ai);
                crate::silicon::chassis::show_laws(world, e);
            }
        }
        (MachineWires::Robot, WIRE_CAMERA) => {
            announce(world, e, "Объектив камеры киборга громко фокусируется.");
        }
        (MachineWires::Robot, WIRE_LAWSYNC) => {
            if world.get::<Cyborg>(e).is_some_and(|c| c.lawupdate) {
                announce(world, e, "Киборг мягко звенит.");
                crate::silicon::cyborg::lawsync(world, e);
                crate::silicon::chassis::show_laws(world, e);
            }
        }
        (MachineWires::Robot, WIRE_LOCKDOWN) => {
            let locked = world.get::<Chassis>(e).is_some_and(|c| c.lockcharge);
            crate::silicon::chassis::set_lockdown(world, e, !locked);
        }
        (MachineWires::Robot, WIRE_RESET_MODEL) => {
            if world.get::<Cyborg>(e).is_some_and(|c| c.model.is_some()) {
                announce(world, e, "Сервоприводы модуля киборга дёргаются.");
            }
        }
        (MachineWires::Mecha, WIRE_IDSCAN) => {
            if let Some(mut m) = world.get_mut::<Mech>(e) {
                m.flags ^= crate::mecha::core::ID_LOCK_ON;
            }
        }
        (MachineWires::Mecha, WIRE_DISARM) => set_mech_disarmed(world, e, true),
        (MachineWires::Mecha, WIRE_ZAP) => state(world, e, |s| s.short_circuit = !s.short_circuit),
        (MachineWires::Mecha, WIRE_LIGHT) => state(world, e, |s| s.lights = !s.lights),
        (MachineWires::Mecha, WIRE_OVERCLOCK) => state(world, e, |s| s.overclock = !s.overclock),
        (MachineWires::Mod, WIRE_HACK) => {
            if let Some(mut s) = world.get_mut::<ModSuit>(e) {
                s.locked = !s.locked;
            }
        }
        (MachineWires::Mod, WIRE_DISABLE) => state(world, e, |s| s.malfunctioning = true),
        (MachineWires::Mod, WIRE_INTERFACE) => {
            state(world, e, |s| s.interface_break = !s.interface_break);
        }
        (MachineWires::WireBundle, _) => crate::wiremod::server::bundle_pulsed(world, e, wire),
        (MachineWires::Gizmo, _) => crate::science::gizmo::pulsed(world, e, wire, _user),
        // `set_listening(!listening)`, then broadcasting follows listening.
        (MachineWires::Radio, WIRE_SIGNAL) => {
            if let Some(mut r) = world.get_mut::<Radio>(e) {
                r.listening = !r.listening;
                r.broadcasting = r.listening;
            }
        }
        (MachineWires::Radio, WIRE_RX) => {
            if let Some(mut r) = world.get_mut::<Radio>(e) {
                r.listening = !r.listening;
            }
        }
        (MachineWires::Radio, WIRE_TX) => {
            if let Some(mut r) = world.get_mut::<Radio>(e) {
                r.broadcasting = !r.broadcasting;
            }
        }
        _ => {}
    }
}

/// The kind's `get_status()`.
pub fn status(world: &World, e: Entity, kind: MachineWires) -> Vec<String> {
    let s = world.get::<WireState>(e).copied().unwrap_or_default();
    let electrified = world.get::<Electrified>(e).is_some();
    match kind {
        MachineWires::Holopad => vec![
            format!("The purple light is {}.", on_off(s.loop_mode)),
            format!("The red light is {}.", on_off(s.replay_mode)),
        ],
        MachineWires::Fax => {
            let spin = |b: bool| if b { "spinning rapidly" } else { "on" };
            vec![
                format!(
                    "A red light is {}.",
                    if electrified { "blinking" } else { "off" }
                ),
                format!(
                    "The network light is {}.",
                    on_off(world.get::<Fax>(e).is_some_and(|f| f.visible))
                ),
                format!("The output servo is {}.", spin(s.hurl)),
                format!("The input servo is {}.", spin(s.exotic)),
            ]
        }
        MachineWires::Rnd => vec![
            format!("The red light is {}.", on_off(!s.disabled)),
            format!("The blue light is {}.", on_off(!s.hacked)),
        ],
        MachineWires::Robot => {
            let cyborg = world.get::<Cyborg>(e);
            let ai = cyborg.and_then(|c| c.connected_ai).map_or_else(
                || "NULL".to_owned(),
                |a| crate::silicon::chassis::name_of(world, a),
            );
            let reset = world
                .get::<Wires>(e)
                .and_then(|w| w.color_of(WIRE_RESET_MODEL))
                .unwrap_or_default();
            vec![
                format!(
                    "The law sync module is {}.",
                    on_off(cyborg.is_some_and(|c| c.lawupdate))
                ),
                format!("The intelligence link display shows {ai}."),
                // No built-in camera (see the module shortcut).
                "The camera light is off.".to_owned(),
                format!(
                    "The lockdown indicator is {}.",
                    on_off(world.get::<Chassis>(e).is_some_and(|c| c.lockcharge))
                ),
                format!("There is a star symbol above the {reset} wire."),
            ]
        }
        MachineWires::Mecha => {
            let flags = world.get::<Mech>(e).map_or(0, |m| m.flags);
            let mut out = vec![
                format!("The orange light is {}.", on_off(s.short_circuit)),
                format!(
                    "The red light is {}.",
                    if s.overclock { "blinking" } else { "off" }
                ),
                format!(
                    "The green light is {}.",
                    on_off(flags & crate::mecha::core::ID_LOCK_ON != 0)
                ),
            ];
            if flags & MECH_HAS_LIGHTS != 0 {
                out.push(format!("The yellow light is {}.", on_off(s.lights)));
            }
            out.push(format!(
                "The blue light is {}.",
                on_off(
                    world
                        .get::<crate::emp::power::MechEquipmentDisabled>(e)
                        .is_some()
                )
            ));
            out
        }
        MachineWires::Mod => vec![
            format!("The orange light is {}.", on_off(electrified)),
            format!(
                "The red light is {}.",
                if s.malfunctioning { "off" } else { "blinking" }
            ),
            format!(
                "The green light is {}.",
                on_off(world.get::<ModSuit>(e).is_some_and(|m| m.locked))
            ),
            format!("The yellow light is {}.", on_off(!s.interface_break)),
        ],
        _ => Vec::new(),
    }
}

/// `rnd/is_insertion_ready`: "belts disabled!" while the disable wire holds.
pub fn rnd_disabled(world: &World, e: Entity) -> bool {
    flag(world, e, |s| s.disabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wires::{cut, pulse};

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(ss13_world::SpatialIndex::default());
        world.insert_resource(crate::combat::CombatRng(1));
        world.insert_resource({
            let mut colors = WireColors::default();
            colors.seed = 3;
            colors
        });
        world
    }

    fn holder(world: &mut World, kind: MachineWires) -> Entity {
        let e = world.spawn(ss13_shared::TilePos::new(1, 1, 1)).id();
        add(world, e, kind);
        e
    }

    fn st(world: &World, e: Entity) -> WireState {
        *world.get::<WireState>(e).unwrap()
    }

    #[test]
    fn sets_match_tg() {
        assert_eq!(set(MachineWires::Microwave).wires.len(), 2);
        assert_eq!(set(MachineWires::Fax).wires.len(), 5);
        assert_eq!(set(MachineWires::Rnd).wires.len(), 8);
        assert_eq!(set(MachineWires::Robot).wires.len(), 7);
        assert_eq!(set(MachineWires::Mecha).wires.len(), 9);
        assert_eq!(set(MachineWires::Mod).wires.len(), 6);
        assert_eq!(
            set(MachineWires::Radio).wires,
            [WIRE_SIGNAL, WIRE_RX, WIRE_TX]
        );
    }

    #[test]
    fn other_machine_wires() {
        let mut world = world();
        // Holopad: loop and replay toggle, cutting replay holds it on.
        let pad = holder(&mut world, MachineWires::Holopad);
        pulse(&mut world, pad, WIRE_LOOP_MODE, None);
        assert!(st(&world, pad).loop_mode);
        cut(&mut world, pad, WIRE_REPLAY_MODE, None);
        assert!(st(&world, pad).replay_mode);
        cut(&mut world, pad, WIRE_REPLAY_MODE, None);
        assert!(!st(&world, pad).replay_mode);
        // R&D: hack and disable follow the cut, flip on a pulse.
        let lathe = holder(&mut world, MachineWires::Rnd);
        cut(&mut world, lathe, WIRE_DISABLE, None);
        assert!(rnd_disabled(&world, lathe));
        cut(&mut world, lathe, WIRE_DISABLE, None);
        pulse(&mut world, lathe, WIRE_HACK, None);
        assert!(st(&world, lathe).hacked && !rnd_disabled(&world, lathe));
        assert_eq!(
            status(&world, lathe, MachineWires::Rnd),
            ["The red light is on.", "The blue light is off."]
        );
        // Fax: shock, network, servos.
        let fax = holder(&mut world, MachineWires::Fax);
        world.entity_mut(fax).insert(Fax {
            id: "1".into(),
            loaded: None,
            visible: true,
        });
        pulse(&mut world, fax, WIRE_SHOCK, None);
        assert_eq!(
            world.get::<Electrified>(fax),
            Some(&Electrified::Timed(MACHINE_DEFAULT_ELECTRIFY_TIME))
        );
        cut(&mut world, fax, WIRE_SHOCK, None);
        assert_eq!(world.get::<Electrified>(fax), Some(&Electrified::Permanent));
        cut(&mut world, fax, WIRE_SHOCK, None);
        assert!(world.get::<Electrified>(fax).is_none());
        cut(&mut world, fax, WIRE_SIGNAL, None);
        assert!(!world.get::<Fax>(fax).unwrap().visible);
        cut(&mut world, fax, WIRE_THROW, None);
        pulse(&mut world, fax, WIRE_LOADCHECK, None);
        assert!(st(&world, fax).hurl && st(&world, fax).exotic);
        assert_eq!(
            status(&world, fax, MachineWires::Fax)[1],
            "The network light is off."
        );
    }

    #[test]
    fn radio_mod_and_mech_wires() {
        let mut world = world();
        // Radio: pulses toggle listening/broadcasting, the TX cut mutes it.
        let radio = world
            .spawn(Radio::new(&ss13_mapload::DmRadio {
                path: "/obj/item/radio".into(),
                listening: true,
                broadcasting: false,
                ..default()
            }))
            .id();
        add(&mut world, radio, MachineWires::Radio);
        pulse(&mut world, radio, WIRE_SIGNAL, None);
        let r = world.get::<Radio>(radio).unwrap();
        assert!(!r.listening && !r.broadcasting);
        pulse(&mut world, radio, WIRE_TX, None);
        assert!(world.get::<Radio>(radio).unwrap().broadcasting);
        cut(&mut world, radio, WIRE_TX, None);
        assert!(world.get::<Radio>(radio).unwrap().tx_cut);
        cut(&mut world, radio, WIRE_TX, None);
        assert!(!world.get::<Radio>(radio).unwrap().tx_cut);
        // An unscrewed radio's wires are open; an item is not opened by hand (picked up).
        assert!(!wires::interactable(&world, radio, radio));
        world.entity_mut(radio).insert(PanelOpen(true));
        assert!(wires::interactable(&world, radio, radio));
        assert!(!hand_opens(&world, radio, MachineWires::Radio));

        // Mech disarm: the equipment control goes and comes back with the wire.
        let mech = holder(&mut world, MachineWires::Mecha);
        cut(&mut world, mech, WIRE_DISARM, None);
        assert!(
            world
                .get::<crate::emp::power::MechEquipmentDisabled>(mech)
                .is_some()
        );
        cut(&mut world, mech, WIRE_DISARM, None);
        assert!(
            world
                .get::<crate::emp::power::MechEquipmentDisabled>(mech)
                .is_none()
        );
        pulse(&mut world, mech, WIRE_ZAP, None);
        assert!(st(&world, mech).short_circuit);
        // MOD: disable and interface follow the cut.
        let suit = holder(&mut world, MachineWires::Mod);
        cut(&mut world, suit, WIRE_DISABLE, None);
        cut(&mut world, suit, WIRE_INTERFACE, None);
        assert!(st(&world, suit).malfunctioning && st(&world, suit).interface_break);
        pulse(&mut world, suit, WIRE_SHOCK, None);
        assert!(world.get::<Electrified>(suit).is_some());
        assert_eq!(
            status(&world, suit, MachineWires::Mod)[3],
            "The yellow light is off."
        );
    }
}
