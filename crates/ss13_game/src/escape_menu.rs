//! The Escape menu, after tg `/datum/escape_menu` (`code/modules/escape_menu`, BandaStation's
//! `process` with the shift time), and BandaStation's Who window (`modular_bandastation/
//! tgui_who`, tgui `Who`).
//!
//! - Every client, in the lobby or in a body, has a menu host: an entity off the map
//!   ([`MenuHost`], tgui `always_state`) with the headless interface [`EscapeButtonUi`]. Esc
//!   (`crate::escape_menu_client`) sends its [`OPEN`] act.
//! - The menu ([`EscapeMenuUi`]) shows the station, the shift time ("Pre-Game" before the round)
//!   and how many players are online; [`WHO`] opens the Who window.
//! - Who ([`WhoUi`]) lists the keys of the clients online and the viewer's own key; it refreshes
//!   by itself (tgui autoupdate) and on [`UPDATE`].
// shortcut: the menu has no settings, leave-body, suicide or disconnect buttons, no admin list
// and no server (IC) time (the port has no station clock apart from the shift time); admin help
// is the `ss13_admin` F1 window. Who has no ping (the transport does not measure it per client)
// and no admin mode (status, roles, follow, smite: the admin panel's player list has them).

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// The client's menu host (no window).
pub struct EscapeButtonUi;

impl UiInterface for EscapeButtonUi {
    const NAME: &'static str = "EscapeMenuButton";
    type Data = ();
    type StaticData = ();
}

/// [`EscapeButtonUi`]'s act: Esc pressed.
pub const OPEN: &str = "open";

pub struct EscapeMenuUi;

impl UiInterface for EscapeMenuUi {
    const NAME: &'static str = "EscapeMenu";
    type Data = EscapeMenuData;
    type StaticData = ();
}

/// [`EscapeMenuUi`]'s act: open the Who window.
pub const WHO: &str = "who";

/// BandaStation's escape menu data without the admin list and the server time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscapeMenuData {
    pub station_name: String,
    /// `round_timestamp()`; `None`: "Pre-Game".
    pub shift_time: Option<String>,
    pub players: u32,
}

pub struct WhoUi;

impl UiInterface for WhoUi {
    const NAME: &'static str = "Who";
    type Data = WhoData;
    type StaticData = ();
}

/// [`WhoUi`]'s act: refresh (`update_static_data`).
pub const UPDATE: &str = "update";

/// `/datum/tgui_who/ui_data` + `ui_static_data` for a non-admin viewer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhoData {
    /// The viewer's key.
    pub user: String,
    /// Every client's key, sorted (tgui's default sort is by key).
    pub clients: Vec<String>,
}

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use std::collections::HashMap;

    use bevy::ecs::message::{MessageCursor, Messages};
    use bevy::prelude::*;
    use ss13_net::server::{ClientConnected, ClientDisconnected, ClientToken};
    use ss13_tgui::server::{
        AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems,
    };

    use super::*;
    use crate::lobby::GameState;

    /// How often the menu and Who data are rebuilt (tgui's autoupdate is 0.9 s).
    const REFRESH_SECS: f64 = 1.0;

    /// A client's menu host: the source of its menu windows, off the map.
    #[derive(Component, Clone, Debug)]
    pub struct MenuHost {
        pub client: Entity,
        pub menu: EscapeMenuData,
        pub who: WhoData,
    }

    /// Client -> its [`MenuHost`]; `fresh`: a host was added since the last refresh.
    #[derive(Resource, Default, Debug)]
    pub struct MenuHosts {
        pub hosts: HashMap<Entity, Entity>,
        fresh: bool,
    }

    pub struct EscapeMenuServerPlugin;

    impl Plugin for EscapeMenuServerPlugin {
        fn build(&self, app: &mut App) {
            app.register_ui::<EscapeButtonUi>()
                .register_ui::<EscapeMenuUi>()
                .register_ui::<WhoUi>()
                .init_resource::<MenuHosts>()
                .add_systems(Update, (hosts, refresh).chain().before(UiSystems::Open))
                .add_systems(Update, menu_acts.in_set(UiSystems::Handle));
        }
    }

    /// A client that passed the handshake gets its host; a gone client's host goes.
    fn hosts(
        world: &mut World,
        mut connected: Local<MessageCursor<ClientConnected>>,
        mut disconnected: Local<MessageCursor<ClientDisconnected>>,
    ) {
        let joined: Vec<Entity> = connected
            .read(world.resource::<Messages<ClientConnected>>())
            .map(|c| c.client)
            .collect();
        let left: Vec<Entity> = disconnected
            .read(world.resource::<Messages<ClientDisconnected>>())
            .map(|d| d.client)
            .collect();
        for client in left {
            if let Some(host) = world.resource_mut::<MenuHosts>().hosts.remove(&client) {
                world.try_despawn(host).ok();
            }
        }
        for client in joined {
            if world.get_entity(client).is_err()
                || world.resource::<MenuHosts>().hosts.contains_key(&client)
            {
                continue;
            }
            let host = world
                .spawn((
                    Name::new("escape menu host"),
                    MenuHost {
                        client,
                        menu: EscapeMenuData::default(),
                        who: WhoData::default(),
                    },
                    AlwaysState,
                ))
                .id();
            let mut hosts = world.resource_mut::<MenuHosts>();
            hosts.hosts.insert(client, host);
            hosts.fresh = true;
            // Actor = the client, as lobby windows: `crate::ghost::close_foreign_windows` closes a
            // window whose actor is neither the client nor its mob.
            world.write_message(OpenUi::new::<EscapeButtonUi>(client, client, host, ""));
        }
    }

    /// BandaStation's `process` of the menu (and Who's lists), once a second.
    // Cost: once a second, one pass over the clients for the keys and one over the hosts.
    fn refresh(world: &mut World, mut next: Local<f64>) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        // A new client's windows open with current data, not with the next second's.
        let fresh = std::mem::take(&mut world.resource_mut::<MenuHosts>().fresh);
        if now < *next && !fresh {
            return;
        }
        *next = now + REFRESH_SECS;
        let mut tokens = world.query::<(Entity, &ClientToken)>();
        let keys: HashMap<Entity, String> =
            tokens.iter(world).map(|(e, t)| (e, t.0.clone())).collect();
        let mut clients: Vec<String> = keys.values().cloned().collect();
        clients.sort_by_key(|k| k.to_lowercase());
        let started = world
            .get_resource::<crate::round::Ticker>()
            .is_some_and(|t| t.state() >= GameState::Playing);
        let menu = EscapeMenuData {
            station_name: crate::round::station_name(world),
            shift_time: started.then(|| crate::records::manifest::round_timestamp(world)),
            players: clients.len() as u32,
        };
        let mut hosts = world.query::<&mut MenuHost>();
        for mut host in hosts.iter_mut(world) {
            let who = WhoData {
                user: keys.get(&host.client).cloned().unwrap_or_default(),
                clients: clients.clone(),
            };
            if host.menu != menu {
                host.menu.clone_from(&menu);
            }
            if host.who != who {
                host.who = who;
            }
        }
    }

    impl ServerUi for EscapeButtonUi {
        type Source = &'static MenuHost;
        type UserParam = ();
        type Act = ();

        fn static_data(_: &MenuHost) {}

        fn data(_: &MenuHost, _: Entity, _: &()) {}

        fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
            if action == OPEN {
                Ok(())
            } else {
                Err(UiReject::UnknownAction)
            }
        }
    }

    impl ServerUi for EscapeMenuUi {
        type Source = &'static MenuHost;
        type UserParam = ();
        type Act = ();

        fn static_data(_: &MenuHost) {}

        fn data(host: &MenuHost, _: Entity, _: &()) -> EscapeMenuData {
            host.menu.clone()
        }

        fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
            if action == WHO {
                Ok(())
            } else {
                Err(UiReject::UnknownAction)
            }
        }
    }

    impl ServerUi for WhoUi {
        type Source = &'static MenuHost;
        type UserParam = ();
        type Act = ();

        fn static_data(_: &MenuHost) {}

        fn data(host: &MenuHost, _: Entity, _: &()) -> WhoData {
            host.who.clone()
        }

        /// An applied act sends the window again at once (`update_static_data`).
        fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
            if action == UPDATE {
                Ok(())
            } else {
                Err(UiReject::UnknownAction)
            }
        }
    }

    /// Esc opens the menu, its Who button the Who window; only on the client's own host.
    fn menu_acts(
        mut opens: MessageReader<UiActed<EscapeButtonUi>>,
        mut whos: MessageReader<UiActed<EscapeMenuUi>>,
        hosts: Query<&MenuHost>,
        mut open: MessageWriter<OpenUi>,
    ) {
        let own =
            |client: Entity, source: Entity| hosts.get(source).is_ok_and(|h| h.client == client);
        for act in opens.read() {
            if own(act.client, act.source) {
                open.write(OpenUi::new::<EscapeMenuUi>(
                    act.client, act.client, act.source, "Меню",
                ));
            }
        }
        for act in whos.read() {
            if own(act.client, act.source) {
                open.write(OpenUi::new::<WhoUi>(
                    act.client,
                    act.client,
                    act.source,
                    "Who's there?",
                ));
            }
        }
    }
}
