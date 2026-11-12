//! Portable SMES (server): tg `/obj/machinery/power/smes/connector` and `/obj/machinery/smesbank`
//! (`code/modules/power/smes_portable.dm`).
//!
//! - The connector is a SMES ([`Smes`]) with no batteries of its own: capacity 1 J, nothing to
//!   charge or give, and its window refuses to open ("нет банка энергии!") until a bank is
//!   connected. While one is, a wrench, screwdriver or crowbar on it is refused.
//! - The bank holds the charge in its batteries. Wrenched on a connector's tile (4 s, the
//!   connector's panel shut) it connects: anchored, and the connector's capacity and charge are
//!   the bank's. Wrenched again (8 s) it disconnects: the charge goes back into the bank, the
//!   connector stops and its windows close. A crowbar takes it apart only when disconnected.
//! - An empty hand on a connected bank opens its connector's window. The bank shows its
//!   connector's output, input and charge (`smes-op`, `smes-oc`, `smes-og`), the connector
//!   `bp-c` (charging), `bp-o` (charged) or `bp-d` (empty) while a bank is on it.
// shortcut: no EMP drain of a bank, no screentips, no `super/full` map variant (no map places
// either).

use bevy::prelude::*;
use ss13_mapload::DmAtom;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_world::SpatialIndex;

use super::Smes;
use super::smes::SmesJob;
use crate::construction::{Job, panel_open};
use crate::mecha::core::ItemCell;
use crate::stock_parts::ComponentParts;
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of, use_tool};

pub const CONNECTOR: &str = "/obj/machinery/power/smes/connector";
pub const BANK: &str = "/obj/machinery/smesbank";
/// `use_tool(src, user, 4 SECONDS)`, `8 SECONDS`: deciseconds.
const CONNECT_DS: f32 = 40.0;
const DISCONNECT_DS: f32 = 80.0;
/// `RefreshParts`: "No division by zero error".
pub const EMPTY_CAPACITY: f64 = 1.0;

/// A connector's bank (tg `connected_smes`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmesConnector {
    pub bank: Option<Entity>,
}

/// A bank: its connector (tg `connected_port`), its batteries' capacity and, while
/// disconnected, their charge (connected, the connector's [`Smes`] holds it).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SmesBank {
    pub port: Option<Entity>,
    pub capacity: f64,
    pub charge: f64,
}

/// The bank's own looks.
#[derive(Component, Clone, Debug, Default)]
struct BankSprites {
    base: Option<IconStateId>,
    open: Option<IconStateId>,
    op: [Option<IconStateId>; 2],
    oc: [Option<IconStateId>; 2],
    og: [Option<IconStateId>; 5],
}

pub struct SmesPortablePlugin;

impl Plugin for SmesPortablePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Before the panel screwdriver (`machine_wires`, 289) and `machine_frame` (1600):
        // the connector's refusals and the bank's wrench.
        app.add_click_handler(ClickPhase::HeldOnTarget, 281, |w, c| {
            c.held.is_some_and(|h| {
                item_interaction(
                    w,
                    Actor {
                        client: c.client,
                        mob: c.mob,
                        item: Some(h),
                    },
                    c.target,
                )
            })
        });
        // `smesbank/interact`: its connector's window.
        app.add_click_handler(ClickPhase::EmptyHand, 1961, |w, c| {
            let port = w.get::<SmesBank>(c.target).map(|b| b.port);
            match port {
                Some(Some(port)) => {
                    super::smes::interact(w, c.client, c.mob, port);
                    true
                }
                Some(None) => true,
                None => false,
            }
        });
        app.add_systems(
            Update,
            (
                bank_looks.after(super::PowerSystems),
                connector_windows.in_set(ss13_tgui::server::UiSystems::StatusHook),
            ),
        );
    }
}

/// A connector built or mapped (`crate::power::smes::setup`, map load): empty capacity.
pub fn setup_connector(world: &mut World, e: Entity) {
    if !world
        .get::<ss13_mapload::DmPower>(e)
        .is_some_and(|d| is_under(&d.path, CONNECTOR))
    {
        return;
    }
    world.entity_mut(e).insert(SmesConnector::default());
    if let Some(mut s) = world.get_mut::<Smes>(e) {
        s.capacity = EMPTY_CAPACITY;
        s.charge = s.charge.min(EMPTY_CAPACITY);
    }
}

/// Batteries of a machine with their charge state.
fn batteries(world: &World, e: Entity) -> Vec<(Entity, ItemCell)> {
    world
        .get::<ComponentParts>(e)
        .map(|parts| {
            parts
                .parts()
                .iter()
                .filter_map(|&p| world.get::<ItemCell>(p).map(|c| (p, *c)))
                .collect()
        })
        .unwrap_or_default()
}

/// A bank built from its board (`crate::machine_frame::setup_built`): its batteries' capacity
/// and charge, unanchored (`on_construction`), then connected to a connector on its tile.
pub fn setup_bank(world: &mut World, e: Entity) {
    let cells = batteries(world, e);
    let bank = SmesBank {
        port: None,
        capacity: cells.iter().map(|(_, c)| c.0.maxcharge).sum(),
        charge: cells.iter().map(|(_, c)| c.0.charge).sum(),
    };
    let sprites = sprites_for(world, e);
    world.entity_mut(e).insert((bank, sprites));
    crate::obj_damage::set_anchored(world, e, false);
}

fn sprites_for(world: &World, e: Entity) -> BankSprites {
    let (Some(index), Some(icon)) = (
        world.get_resource::<IconIndex>(),
        world.get::<DmAtom>(e).map(|a| a.icon.clone()),
    ) else {
        return BankSprites::default();
    };
    let get = |s: &str| index.get(&icon, s);
    BankSprites {
        base: get("port_smes"),
        open: get("port_smes-o"),
        op: [get("smes-op0"), get("smes-op1")],
        oc: [get("smes-oc0"), get("smes-oc1")],
        og: std::array::from_fn(|i| get(&format!("smes-og{}", i + 1))),
    }
}

/// The connector on `pos`.
fn connector_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<SmesConnector>(e).is_some())
}

/// `connect_port(possible_connector, user, check_only)`: the refusal, if any.
fn connect_refusal(world: &World, connector: Option<Entity>) -> Option<&'static str> {
    let Some(connector) = connector else {
        return Some("нет коннектора!");
    };
    if panel_open(world, connector) {
        return Some("закройте панель коннектора!");
    }
    if world
        .get::<SmesConnector>(connector)
        .is_some_and(|c| c.bank.is_some())
    {
        // tg's "Make sure not already connected to something else" (checked as the panel).
        return Some("коннектор занят!");
    }
    None
}

/// Connects `bank` to `connector`: `connect_smes(bank)`, anchored.
pub fn connect(world: &mut World, bank: Entity, connector: Entity) {
    let Some(state) = world.get::<SmesBank>(bank).copied() else {
        return;
    };
    if let Some(mut c) = world.get_mut::<SmesConnector>(connector) {
        c.bank = Some(bank);
    }
    if let Some(mut s) = world.get_mut::<Smes>(connector) {
        s.capacity = state.capacity.max(EMPTY_CAPACITY);
        s.charge = state.charge.min(s.capacity);
    }
    if let Some(mut b) = world.get_mut::<SmesBank>(bank) {
        b.port = Some(connector);
    }
    crate::obj_damage::set_anchored(world, bank, true);
}

/// `disconnect_port()` / `connect_smes(null)`: the charge back into the bank, the connector
/// stopped and empty (its windows close: [`connector_windows`]), the bank loose.
pub fn disconnect(world: &mut World, bank: Entity) {
    let Some(port) = world.get::<SmesBank>(bank).and_then(|b| b.port) else {
        return;
    };
    let charge = world.get::<Smes>(port).map_or(0.0, |s| s.charge);
    if let Some(mut b) = world.get_mut::<SmesBank>(bank) {
        b.port = None;
        b.charge = charge.min(b.capacity);
    }
    if let Some(mut c) = world.get_mut::<SmesConnector>(port) {
        c.bank = None;
    }
    if let Some(mut s) = world.get_mut::<Smes>(port) {
        s.inputting = false;
        s.outputting = false;
        s.capacity = EMPTY_CAPACITY;
        s.charge = 0.0;
    }
    crate::obj_damage::set_anchored(world, bank, false);
}

/// `connect_smes(null)`'s `SStgui.close_uis(src)` and `connector/ui_act` without a bank: the
/// connector's SMES window closes.
fn connector_windows(
    mut windows: Query<&mut ss13_tgui::server::UiWindow>,
    connectors: Query<&SmesConnector>,
) {
    for mut window in &mut windows {
        if window.interface == <super::smes::SmesWindow as ss13_tgui::UiInterface>::NAME
            && connectors
                .get(window.source)
                .is_ok_and(|c| c.bank.is_none())
        {
            window.cap_status(ss13_tgui::UiStatus::Close);
        }
    }
}

/// `connector/ui_interact` without a bank: refused. True when refused.
pub fn connector_refuses_ui(world: &mut World, mob: Entity, smes: Entity) -> bool {
    if world
        .get::<SmesConnector>(smes)
        .is_some_and(|c| c.bank.is_none())
    {
        crate::balloon::balloon_alert(world, mob, smes, "нет банка энергии!");
        return true;
    }
    false
}

/// The connector's `wrench_act`/`screwdriver_act`/`crowbar_act` refusal (`connector_free`) and
/// the bank's `wrench_act`. True when handled.
pub fn item_interaction(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let Some((kind, _)) = tool_of(world, item) else {
        return false;
    };
    if let Some(c) = world.get::<SmesConnector>(target).copied() {
        if c.bank.is_some()
            && matches!(
                kind,
                ToolKind::Wrench | ToolKind::Screwdriver | ToolKind::Crowbar
            )
        {
            crate::balloon::balloon_alert(world, actor.mob, target, "сначала отключите SMES!");
            return true;
        }
        return false;
    }
    let Some(bank) = world.get::<SmesBank>(target).copied() else {
        return false;
    };
    match kind {
        ToolKind::Wrench if bank.port.is_some() => {
            use_tool(
                world,
                actor,
                target,
                DISCONNECT_DS,
                0,
                Job::Smes(SmesJob::BankDisconnect),
            );
            true
        }
        ToolKind::Wrench => {
            let connector = world
                .get::<TilePos>(target)
                .and_then(|&p| connector_at(world, p));
            if let Some(text) = connect_refusal(world, connector) {
                crate::balloon::balloon_alert(world, actor.mob, target, text);
                return true;
            }
            if let Some(connector) = connector {
                use_tool(
                    world,
                    actor,
                    target,
                    CONNECT_DS,
                    0,
                    Job::Smes(SmesJob::BankConnect { connector }),
                );
            }
            true
        }
        // `crowbar_act`: not while connected.
        ToolKind::Crowbar if bank.port.is_some() => {
            crate::balloon::balloon_alert(
                world,
                actor.mob,
                target,
                "сначала отключите от коннектора!",
            );
            true
        }
        _ => false,
    }
}

/// The end of the bank's wrench, against the state now.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: SmesJob) {
    let bank = action.target;
    let Some(state) = world.get::<SmesBank>(bank).copied() else {
        return;
    };
    let (who, what) = (
        crate::security::name_of(world, mob),
        crate::security::name_of(world, bank),
    );
    match job {
        SmesJob::BankDisconnect => {
            let Some(port) = state.port else {
                return;
            };
            let port_name = crate::security::name_of(world, port);
            disconnect(world, bank);
            tell(
                world,
                action.client,
                Span::Notice,
                format!("Вы отсоединяете {what} от {port_name}."),
            );
            say_near(world, bank, format!("{who} отсоединяет {what}."));
        }
        SmesJob::BankConnect { connector } => {
            let here = world
                .get::<TilePos>(bank)
                .and_then(|&p| connector_at(world, p));
            if state.port.is_some() || here != Some(connector) {
                return;
            }
            if let Some(text) = connect_refusal(world, here) {
                crate::balloon::balloon_alert(world, mob, bank, text);
                return;
            }
            connect(world, bank, connector);
            let port_name = crate::security::name_of(world, connector);
            tell(
                world,
                action.client,
                Span::Notice,
                format!("Вы прикручиваете {what} к {port_name}."),
            );
            say_near(world, bank, format!("{who} подключает {what}."));
        }
        _ => {}
    }
}

fn say_near(world: &mut World, at: Entity, text: String) {
    let Some(&pos) = world.get::<TilePos>(at) else {
        return;
    };
    let lines = crate::pull::visible_message(
        world,
        pos,
        ss13_shared::chat::ChatChannel::Info,
        (Span::Plain, text),
        &[],
    );
    for line in lines {
        world.write_message(line);
    }
}

/// `crowbar_act` of a connected bank, for `crate::machine_frame`. True when refused.
pub fn crowbar_refused(world: &mut World, client: Entity, bank: Entity) -> bool {
    if world
        .get::<SmesBank>(bank)
        .is_some_and(|b| b.port.is_some())
    {
        tell(
            world,
            client,
            Span::Warning,
            "сначала отключите от коннектора!",
        );
        return true;
    }
    false
}

/// Taken apart or destroyed (`crate::machine_frame::on_deconstruction`, `Destroy`): a bank
/// disconnects and its charge goes back into its batteries; a connector lets its bank go.
pub fn on_deconstruction(world: &mut World, e: Entity) {
    if let Some(bank) = world.get::<SmesConnector>(e).and_then(|c| c.bank) {
        disconnect(world, bank);
    }
    if world.get::<SmesBank>(e).is_none() {
        return;
    }
    disconnect(world, e);
    let mut left = world.get::<SmesBank>(e).map_or(0.0, |b| b.charge);
    for (cell, mut state) in batteries(world, e) {
        let give = left.min(state.0.maxcharge).max(0.0);
        state.0.charge = give;
        left -= give;
        world.entity_mut(cell).insert(state);
    }
}

/// `smesbank/update_icon_state`, `update_overlays`: the connector's state on a connected bank.
#[allow(clippy::type_complexity)]
fn bank_looks(
    mut banks: Query<(
        &SmesBank,
        &BankSprites,
        Option<&crate::construction::PanelOpen>,
        &mut Appearance,
    )>,
    ports: Query<&Smes>,
) {
    for (bank, sprites, panel, mut appearance) in &mut banks {
        let open = panel.is_some_and(|p| p.0);
        if let Some(state) = if open { sprites.open } else { sprites.base }
            && appearance.icon_state != state
        {
            appearance.icon_state = state;
        }
        let mut overlays = Vec::new();
        if !open && let Some(s) = bank.port.and_then(|p| ports.get(p).ok()) {
            let level = s.charge_display();
            let states = [
                sprites.op[usize::from(s.outputting)],
                sprites.oc[usize::from(s.inputting)],
                (level > 0)
                    .then(|| sprites.og[usize::from(level - 1)])
                    .flatten(),
            ];
            overlays.extend(states.into_iter().flatten().map(Overlay::new));
        }
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smes() -> Smes {
        Smes {
            charge: 0.0,
            capacity: 0.0,
            input_level: 0.0,
            output_level: 0.0,
            input_level_max: 0.0,
            output_level_max: 0.0,
            input_attempt: true,
            output_attempt: true,
            inputting: true,
            outputting: true,
            input_available: 0.0,
            output_used: 0.0,
            terminal: None,
        }
    }

    #[test]
    fn the_charge_moves_with_the_bank() {
        let mut w = World::new();
        let connector = w.spawn((smes(), SmesConnector::default())).id();
        let bank = w
            .spawn(SmesBank {
                port: None,
                capacity: 1.0e7,
                charge: 4.0e6,
            })
            .id();
        connect(&mut w, bank, connector);
        assert_eq!(w.get::<SmesConnector>(connector).unwrap().bank, Some(bank));
        let s = w.get::<Smes>(connector).unwrap();
        assert_eq!((s.capacity, s.charge), (1.0e7, 4.0e6));
        assert_eq!(
            connect_refusal(&w, Some(connector)),
            Some("коннектор занят!")
        );
        // It charged while connected; unhooked, the bank keeps it and the connector is empty.
        w.get_mut::<Smes>(connector).unwrap().charge = 9.0e6;
        disconnect(&mut w, bank);
        assert_eq!(w.get::<SmesBank>(bank).unwrap().charge, 9.0e6);
        let s = w.get::<Smes>(connector).unwrap();
        assert_eq!((s.capacity, s.charge), (EMPTY_CAPACITY, 0.0));
        assert!(!s.inputting && !s.outputting);
        assert_eq!(connect_refusal(&w, Some(connector)), None);
        assert_eq!(connect_refusal(&w, None), Some("нет коннектора!"));
    }
}
