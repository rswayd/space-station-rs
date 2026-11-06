//! The shuttle remote (server), after tg `/obj/item/shuttle_remote`
//! (`code/modules/shuttle/misc/shuttle_remote.dm`).
//!
//! - Linking: a remote mapped on a shuttle console's tile links to it; otherwise one used on a
//!   console that may be remote controlled (the white ship's, [`ShuttleConsole`]) and has no
//!   remote yet links ("linked", a beep); a linked remote or an occupied console refuses.
//! - Use in hand: with the shuttle at its home dock (or its `<id>_custom` dock) the remote asks
//!   to send it to the away dock, at the away dock to call it home (or to the custom dock when
//!   the console has one). Every check (`can_use`: conscious, holding it, not on a transit level,
//!   the console there and unlocked, the shuttle idle, both docks free) runs when used and again
//!   after the answer, with the destination's dock free; the console then sends the shuttle
//!   ([`crate::shuttle_nav::send_shuttle`]) and keeps the destination.
//! - Alt+right click: picks the home and away docks among the console's other destinations.
// shortcut: the prompt names the shuttle by its mobile port's name, not its area
// (`get_area_name`); a dock is "occupied" when another shuttle is docked there (no `canDock`
// size checks).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::health::Stat;
use crate::inventory::Inventory;
use crate::shuttle::{MobileDock, StationaryDock};
use crate::shuttle_nav::{
    Flight, NavShuttle, ShuttleConsole, flight, get_shuttle, send_shuttle, valid_destinations,
};
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

const REMOTE: &str = "/obj/item/shuttle_remote";
const BEEP: &str = "sound/machines/beep/beep.ogg";

/// `/obj/item/shuttle_remote`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ShuttleRemote {
    /// tg `computer_ref`.
    pub console: Option<Entity>,
    pub home_id: String,
    pub away_id: String,
    pub may_change_docks: bool,
}

impl Default for ShuttleRemote {
    fn default() -> Self {
        Self {
            console: None,
            home_id: "whiteship_home".into(),
            away_id: "whiteship_lavaland".into(),
            may_change_docks: true,
        }
    }
}

/// What a remote's prompt asks.
#[derive(Component, Clone, Debug)]
enum RemoteAsk {
    /// The alert's destinations by button (`None`: "No").
    Send {
        remote: Entity,
        buttons: Vec<Option<String>>,
    },
    /// The dock list (ids by item); `home` picks the home dock, then the away one is asked.
    Docks {
        remote: Entity,
        home: bool,
        ids: Vec<String>,
    },
}

pub struct ShuttleRemotePlugin;

impl Plugin for ShuttleRemotePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `attack_self`.
        app.add_click_handler(ClickPhase::AttackSelf, 3189, |w, c| {
            w.get::<ShuttleRemote>(c.target).is_some() && {
                use_remote(w, c.client, c.mob, c.target);
                true
            }
        });
        // `interact_with_atom` on a shuttle console.
        app.add_click_handler(ClickPhase::HeldOnTarget, 345, |w, c| {
            !c.modifiers.right
                && c.held
                    .is_some_and(|h| link(w, c.client, c.mob_pos, h, c.target))
        });
        app.add_observer(add_remote)
            .add_systems(
                ss13_world::MapInitLate,
                link_mapped.after(crate::shuttle_nav::setup_consoles),
            )
            .add_systems(
                Update,
                answers
                    .after(ss13_tgui::prompt::PromptSystems::Answer)
                    .in_set(ss13_tgui::server::UiSystems::Handle),
            );
    }
}

fn add_remote(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Ok(item) = items.get(add.entity)
        && is_under(&item.path, REMOTE)
    {
        commands
            .entity(add.entity)
            .try_insert_if_new(ShuttleRemote::default());
    }
}

/// `Initialize(mapload)`: a remote on a shuttle console's tile links to it.
fn link_mapped(world: &mut World) {
    let mut remotes = world
        .query_filtered::<(Entity, &TilePos), (With<ShuttleRemote>, With<ss13_world::Mapped>)>();
    let found: Vec<(Entity, TilePos)> = remotes.iter(world).map(|(e, p)| (e, *p)).collect();
    for (remote, pos) in found {
        let mut consoles = world.query::<(Entity, &TilePos, &ShuttleConsole)>();
        let Some(console) = consoles
            .iter(world)
            .find(|(_, p, _)| **p == pos)
            .map(|(e, ..)| e)
        else {
            continue;
        };
        if let Some(mut c) = world.get_mut::<ShuttleConsole>(console) {
            c.remote = Some(remote);
        }
        if let Some(mut r) = world.get_mut::<ShuttleRemote>(remote) {
            r.console = Some(console);
        }
    }
}

/// tg `computer_ref?.resolve()`: the linked console while it exists.
fn console_of(world: &World, remote: Entity) -> Option<Entity> {
    world
        .get::<ShuttleRemote>(remote)?
        .console
        .filter(|&c| world.get::<ShuttleConsole>(c).is_some())
}

/// tg `remote_ref`: the console's remote while it exists and is still linked to it.
fn remote_of(world: &World, console: Entity) -> Option<Entity> {
    world
        .get::<ShuttleConsole>(console)?
        .remote
        .filter(|&r| world.get::<ShuttleRemote>(r).and_then(|r| r.console) == Some(console))
}

/// `interact_with_atom`. True when it handled the click.
fn link(world: &mut World, client: Entity, pos: TilePos, held: Entity, target: Entity) -> bool {
    if world.get::<ShuttleRemote>(held).is_none() {
        return false;
    }
    let Some(console) = world.get::<ShuttleConsole>(target) else {
        return false;
    };
    let may = console.may_be_remote_controlled;
    if console_of(world, held).is_some() {
        tell(world, client, Span::Warning, "already linked!");
        return true;
    }
    if remote_of(world, target).is_some() || !may {
        tell(world, client, Span::Warning, "occupied signal!");
        return true;
    }
    if let Some(mut c) = world.get_mut::<ShuttleConsole>(target) {
        c.remote = Some(held);
    }
    if let Some(mut r) = world.get_mut::<ShuttleRemote>(held) {
        r.console = Some(target);
    }
    playsound(world, PlaySound::new(pos, BEEP, 30.0, true));
    tell(world, client, Span::Notice, "linked");
    true
}

/// The linked console's shuttle port.
fn port_of(world: &mut World, console: Entity) -> Option<Entity> {
    let id = world.get::<ShuttleConsole>(console)?.shuttle_id.clone()?;
    get_shuttle(world, &id)
}

/// `SSshuttle.getDock(id)`.
fn dock(world: &mut World, id: &str) -> Option<Entity> {
    let mut q = world.query::<(Entity, &StationaryDock)>();
    q.iter(world).find(|(_, d)| d.id == id).map(|(e, _)| e)
}

/// `canDock(dock) == SHUTTLE_CAN_DOCK`: the dock exists and no other shuttle sits on it.
fn can_dock(world: &mut World, port: Entity, id: &str) -> bool {
    let Some(dock) = dock(world, id) else {
        return false;
    };
    let mut q = world.query::<(Entity, &MobileDock)>();
    !q.iter(world)
        .any(|(e, m)| e != port && m.docked == Some(dock))
}

/// `is_reserved_level`: a shuttle's transit level.
fn on_transit_level(world: &mut World, z: u16) -> bool {
    let mut q = world.query::<&NavShuttle>();
    let transits: Vec<Entity> = q.iter(world).map(|n| n.transit).collect();
    transits
        .into_iter()
        .any(|t| world.get::<TilePos>(t).is_some_and(|p| p.z == z))
}

/// `can_use(user)` with its balloon alert; the console and the shuttle port when usable.
fn can_use(
    world: &mut World,
    client: Entity,
    mob: Entity,
    remote: Entity,
) -> Option<(Entity, Entity)> {
    let holding = world
        .get::<Inventory>(mob)
        .is_some_and(|inv| inv.hands.contains(&Some(remote)));
    if world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || crate::status_effects::incapacitation(world, mob).incapacitated
        || !holding
    {
        return None;
    }
    let warn = |world: &mut World, text: &str| {
        tell(world, client, Span::Warning, text);
        None
    };
    if let Some(pos) = world.get::<TilePos>(mob).copied()
        && on_transit_level(world, pos.z)
    {
        return warn(world, "can't use here!");
    }
    let Some(console) = console_of(world, remote) else {
        return warn(world, "no nav computer!");
    };
    if world
        .get::<ShuttleConsole>(console)
        .is_some_and(|c| c.locked)
    {
        return warn(world, "nav computer locked!");
    }
    let Some(port) = port_of(world, console) else {
        return warn(world, "no nav computer!");
    };
    if flight(world, port) != Flight::Idle {
        return warn(world, "engines recharging!");
    }
    let r = world.get::<ShuttleRemote>(remote)?.clone();
    if !can_dock(world, port, &r.home_id) {
        return warn(world, "home dock occupied!");
    }
    if !can_dock(world, port, &r.away_id) {
        return warn(world, "away dock occupied!");
    }
    Some((console, port))
}

fn shuttle_name(world: &World, port: Entity) -> String {
    world
        .get::<ss13_mapload::DmName>(port)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_else(|| "shuttle".into())
}

/// `attack_self`: asks where to send the shuttle.
fn use_remote(world: &mut World, client: Entity, mob: Entity, remote: Entity) {
    let Some((console, port)) = can_use(world, client, mob, remote) else {
        return;
    };
    let Some(r) = world.get::<ShuttleRemote>(remote).cloned() else {
        return;
    };
    let Some(shuttle_id) = world
        .get::<ShuttleConsole>(console)
        .and_then(|c| c.shuttle_id.clone())
    else {
        return;
    };
    let custom = format!("{shuttle_id}_custom");
    let (Some(home), Some(away)) = (dock(world, &r.home_id), dock(world, &r.away_id)) else {
        return;
    };
    let docked = world.get::<MobileDock>(port).and_then(|m| m.docked);
    let docked_id = docked.and_then(|d| world.get::<StationaryDock>(d).map(|s| s.id.clone()));
    let dock_name = |world: &World, d: Entity| {
        world
            .get::<StationaryDock>(d)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    };
    let name = shuttle_name(world, port);
    let (title, mut text, mut buttons) =
        if docked == Some(home) || docked_id.as_ref() == Some(&custom) {
            (
                "Send Off Shuttle?",
                format!(
                    "Are you sure you want to send off {name} to {}?",
                    dock_name(&*world, away)
                ),
                vec![Some(r.away_id.clone()), None],
            )
        } else if docked == Some(away) {
            (
                "Call Shuttle?",
                format!(
                    "Are you sure you want to call {name} to {}?",
                    dock_name(&*world, home)
                ),
                vec![Some(r.home_id.clone()), None],
            )
        } else {
            return;
        };
    if docked == Some(away) {
        let console_data = world.get::<ShuttleConsole>(console).cloned();
        if let Some(c) = console_data
            && valid_destinations(world, &c, port)
                .iter()
                .any(|(_, id, _)| *id == custom)
        {
            text.push_str("\n\nCustom location loaded, try to dock?");
            buttons.push(Some(custom));
        }
    }
    let labels: Vec<String> = ["Yes", "No", "Send to custom"]
        .iter()
        .take(buttons.len())
        .map(|s| (*s).to_owned())
        .collect();
    let ask = Ask::new(
        client,
        mob,
        title,
        text,
        PromptKind::Alert {
            buttons: labels,
            autofocus: true,
        },
    );
    let prompt = ask_world(world, ask);
    world
        .entity_mut(prompt)
        .insert(RemoteAsk::Send { remote, buttons });
}

/// `click_alt_secondary`: picks the home and away docks. True when it handled the click.
pub fn click_alt_secondary(world: &mut World, client: Entity, mob: Entity, remote: Entity) -> bool {
    let Some(r) = world.get::<ShuttleRemote>(remote).cloned() else {
        return false;
    };
    let Some(console) = console_of(world, remote).filter(|_| r.may_change_docks) else {
        return false;
    };
    ask_docks(world, client, mob, remote, console, true);
    true
}

fn ask_docks(
    world: &mut World,
    client: Entity,
    mob: Entity,
    remote: Entity,
    console: Entity,
    home: bool,
) {
    let (Some(r), Some(c)) = (
        world.get::<ShuttleRemote>(remote).cloned(),
        world.get::<ShuttleConsole>(console).cloned(),
    ) else {
        return;
    };
    let Some(port) = port_of(world, console) else {
        tell(world, client, Span::Warning, "no valid destinations!");
        return;
    };
    let custom = c
        .shuttle_id
        .as_ref()
        .map(|id| format!("{id}_custom"))
        .unwrap_or_default();
    let (ids, names): (Vec<String>, Vec<String>) = valid_destinations(world, &c, port)
        .into_iter()
        .filter(|(_, id, _)| *id != r.home_id && *id != r.away_id && *id != custom)
        .map(|(_, id, name)| (id, name))
        .unzip();
    if ids.is_empty() {
        tell(world, client, Span::Warning, "no valid destinations!");
        return;
    }
    let (title, message) = if home {
        (
            "Choose Home Dock",
            "choose which dock to designate as the shuttle's home point...",
        )
    } else {
        (
            "Choose Away Dock",
            "choose which dock to designate as the shuttle's away point...",
        )
    };
    let ask = Ask::new(
        client,
        mob,
        title,
        message,
        PromptKind::List {
            items: names,
            default: None,
        },
    );
    let prompt = ask_world(world, ask);
    world
        .entity_mut(prompt)
        .insert(RemoteAsk::Docks { remote, home, ids });
}

fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
        return;
    };
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(messages)
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in pending {
        let Some(ask) = world.get::<RemoteAsk>(prompt).cloned() else {
            continue;
        };
        match ask {
            RemoteAsk::Send { remote, buttons } => {
                let Some(PromptAnswer::Button(i)) = answer else {
                    continue;
                };
                let Some(Some(dest)) = buttons.get(i).cloned() else {
                    continue;
                };
                // Checked again after the wait.
                let Some((console, port)) = can_use(world, client, mob, remote) else {
                    continue;
                };
                if !can_dock(world, port, &dest) {
                    tell(world, client, Span::Warning, "destination occupied!");
                    continue;
                }
                // `transit_shuttle`.
                if let Err(e) = send_shuttle(world, console, mob, &dest) {
                    tell(world, client, Span::Warning, e.text());
                }
                if let Some(mut c) = world.get_mut::<ShuttleConsole>(console) {
                    c.destination = Some(dest);
                }
            }
            RemoteAsk::Docks { remote, home, ids } => {
                if let Some(PromptAnswer::Item(i)) = answer
                    && let Some(id) = ids.get(i).cloned()
                    && can_use(world, client, mob, remote).is_some()
                    && let Some(mut r) = world.get_mut::<ShuttleRemote>(remote)
                {
                    if home {
                        r.home_id = id;
                    } else {
                        r.away_id = id;
                    }
                }
                // tg asks for the away dock whatever the home answer was.
                if home && let Some(console) = console_of(world, remote) {
                    ask_docks(world, client, mob, remote, console, false);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_remote_flies_the_white_ship_between_home_and_lavaland() {
        let r = ShuttleRemote::default();
        assert_eq!(r.home_id, "whiteship_home");
        assert_eq!(r.away_id, "whiteship_lavaland");
        assert!(r.may_change_docks && r.console.is_none());
    }

    #[test]
    fn can_dock_refuses_a_missing_or_taken_dock() {
        let mut world = World::new();
        let size = crate::shuttle::PortSize::default();
        let free = world
            .spawn(StationaryDock {
                id: "free".into(),
                name: "Free".into(),
                dir: ss13_shared::Dir::South,
                size,
                area: None,
            })
            .id();
        let taken = world
            .spawn(StationaryDock {
                id: "taken".into(),
                name: "Taken".into(),
                dir: ss13_shared::Dir::South,
                size,
                area: None,
            })
            .id();
        let mobile = |docked| MobileDock {
            id: "m".into(),
            dir: ss13_shared::Dir::South,
            size,
            areas: Vec::new(),
            underlying: Default::default(),
            docked,
        };
        let ours = world.spawn(mobile(Some(free))).id();
        world.spawn(mobile(Some(taken)));
        assert!(can_dock(&mut world, ours, "free"));
        assert!(!can_dock(&mut world, ours, "taken"));
        assert!(!can_dock(&mut world, ours, "missing"));
    }
}
