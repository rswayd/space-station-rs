//! The R&D server control console (server), after tg `/obj/machinery/computer/rdservercontrol`
//! (`server_control.dm`) with tgui's `ServerControl` window, and the servers' status text
//! (`/obj/machinery/rnd/server/proc/get_status_text`).
//!
//! A click with an empty hand on a powered console opens the window: the techweb's servers with
//! their status, its R&D consoles with their lock, and the research log. Toggling a server
//! (`lockdown_server`: tg `toggle_disable`, the server stops giving points) or a console's lock
//! (`lock_console`) needs research director access (`req_access = list(ACCESS_RD)`).
//!
//! Servers and consoles are referenced by their index in entity order, the same in `ui_data`
//! and in the acts.
// shortcut: no emag (the access check always applies) or multitool relinking: the console is
// always linked to the station techweb.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{Areas, DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::WorldSystems;

use super::console::{RdConsole, id_access};
use super::points::ResearchServer;
use super::techweb::Techweb;
use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::emp::Emped;
use crate::inventory::{Inventory, check_access};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::tools::tell;

pub const SERVER_CONTROL_ROOT: &str = "/obj/machinery/computer/rdservercontrol";
/// `ACCESS_RD`.
const ACCESS_RD: &str = "rd";

/// An R&D server control console.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ServerControl;

pub struct ServerControlPlugin;

impl Plugin for ServerControlPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<ServerControlUi>()
            .add_systems(Startup, setup_server_controls)
            .add_systems(
                Update,
                (
                    open_server_control
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    server_control_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

fn setup_server_controls(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj)>();
    let found: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, SERVER_CONTROL_ROOT))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// A server control console's state (map load and `crate::machine_frame`).
pub(crate) fn setup(world: &mut World, e: Entity) {
    world.entity_mut(e).insert(ServerControl);
}

/// tg `get_status_text`.
pub fn status_text(server: &ResearchServer, unpowered: bool, emped: bool) -> &'static str {
    if emped {
        "O&F@I*$ - R3*&O$T R@U!R%D"
    } else if unpowered {
        "Offline - Server Unpowered"
    } else if server.research_disabled {
        "Offline - Server Control Disabled"
    } else {
        "Nominal"
    }
}

fn plain(name: &str) -> &str {
    crate::vending::plain_name(name)
}

fn open_server_control(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<Option<&DmName>, (With<ServerControl>, Without<Unpowered>)>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<ServerControlUi>(
                click.client,
                click.mob,
                click.target,
                name.map_or("", |n| plain(&n.name)),
            ));
        }
    }
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<ServerControl>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == ServerControlUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

type ServerControlParams = (
    Option<Res<'static, Techweb>>,
    Query<
        'static,
        'static,
        (
            Entity,
            &'static ResearchServer,
            Option<&'static DmName>,
            Has<Unpowered>,
            Has<Emped>,
        ),
    >,
    Query<
        'static,
        'static,
        (
            Entity,
            &'static RdConsole,
            Option<&'static DmName>,
            Option<&'static TilePos>,
        ),
    >,
    Option<Res<'static, Areas>>,
    Query<'static, 'static, &'static DmName>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerControlAct {
    LockdownServer(usize),
    LockConsole(usize),
}

impl ServerUi for ServerControlUi {
    type Source = &'static ServerControl;
    type UserParam = ServerControlParams;
    type Act = ServerControlAct;

    fn static_data(_: &ServerControl) {}

    fn data(
        _: &ServerControl,
        _: Entity,
        (web, servers, consoles, areas, names): &SystemParamItem<'_, '_, ServerControlParams>,
    ) -> ServerControlData {
        let Some(web) = web.as_deref() else {
            return ServerControlData::default();
        };
        let mut server_list: Vec<_> = servers.iter().collect();
        server_list.sort_by_key(|s| s.0);
        let mut console_list: Vec<_> = consoles.iter().collect();
        console_list.sort_by_key(|c| c.0);
        ServerControlData {
            server_connected: true,
            servers: server_list
                .into_iter()
                .map(|(_, server, name, unpowered, emped)| ServerInfo {
                    server_name: name.map_or_else(String::new, |n| plain(&n.name).to_owned()),
                    server_details: status_text(server, unpowered, emped).to_owned(),
                    server_disabled: server.research_disabled,
                })
                .collect(),
            consoles: console_list
                .into_iter()
                .map(|(_, console, name, pos)| ConsoleInfo {
                    console_name: name.map_or_else(String::new, |n| plain(&n.name).to_owned()),
                    console_location: pos
                        .and_then(|&p| areas.as_deref()?.area_at(p))
                        .and_then(|a| names.get(a).ok())
                        .map_or_else(String::new, |n| plain(&n.name).to_owned()),
                    console_locked: console.locked,
                })
                .collect(),
            logs: web
                .logs
                .iter()
                .map(|l| LogInfo {
                    node_name: l.node_name.clone(),
                    node_cost: l.node_cost,
                    node_researcher: l.node_researcher.clone(),
                    node_research_location: l.node_research_location.clone(),
                })
                .collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ServerControlAct, UiReject> {
        let index = |key: &str| {
            params
                .get(key)
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or_else(|| UiReject::BadParams(format!("{key}: expected an index")))
        };
        match action {
            LOCKDOWN_SERVER => Ok(ServerControlAct::LockdownServer(index(SELECTED_SERVER)?)),
            LOCK_CONSOLE => Ok(ServerControlAct::LockConsole(index(SELECTED_CONSOLE)?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// The `n`th entity with `T` in entity order (`locate(ref) in list`).
fn nth<T: Component>(world: &mut World, n: usize) -> Option<Entity> {
    let mut query = world.query_filtered::<Entity, With<T>>();
    let mut all: Vec<Entity> = query.iter(world).collect();
    all.sort();
    all.get(n).copied()
}

fn server_control_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ServerControlUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, ServerControlAct)> = acts
        .read(world.resource::<Messages<UiActed<ServerControlUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if world.get::<Unpowered>(console).is_some()
            || world.get::<ServerControl>(console).is_none()
        {
            continue;
        }
        // `allowed(usr)`.
        let (access, _) = id_access(world, actor);
        if !check_access(&[ACCESS_RD.to_owned()], &[], &access) {
            tell(world, client, Span::Warning, "в доступе отказано!");
            continue;
        }
        match act {
            ServerControlAct::LockdownServer(n) => {
                if let Some(server) = nth::<ResearchServer>(world, n)
                    && let Some(mut s) = world.get_mut::<ResearchServer>(server)
                {
                    s.research_disabled = !s.research_disabled;
                    let state = if s.research_disabled {
                        "shut off"
                    } else {
                        "turned on"
                    };
                    info!("research: {actor:?} {state} server {server:?}");
                }
            }
            ServerControlAct::LockConsole(n) => {
                if let Some(target) = nth::<RdConsole>(world, n)
                    && let Some(mut c) = world.get_mut::<RdConsole>(target)
                {
                    c.locked = !c.locked;
                }
            }
        }
    }
}
