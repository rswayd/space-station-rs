//! The client side the lobby and antagonist tests share: what a client received ([`Received`],
//! recorded by [`record`]) and the [`Client`] helpers on a test's own `Station` that read it.
//!
//! ```ignore
//! struct Station(testing::Station);
//! ss13_game::drive_station!(Station, 0, context = harness::chat_context);
//! impl Client for Station {}
//! ```

use std::ops::DerefMut;

use bevy::prelude::*;
use ss13_game::lobby::{LobbyData, LobbyUi};
use ss13_game::round::{RoundConfig, Ticker};
use ss13_game::server::Players;
use ss13_game::testing::{self, Drive};
use ss13_net::chat::client::ChatReceived;
use ss13_net::client::NoticeReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;

/// What a client received: windows opened (id, interface, static data), every data message per
/// window, the lobby, chat lines, closed windows and notices.
#[derive(Resource, Default)]
pub struct Received {
    pub windows: Vec<(u32, String, Vec<u8>)>,
    pub data: Vec<(u32, Vec<u8>)>,
    pub lobby: Option<(u32, LobbyData)>,
    pub chat: Vec<String>,
    pub closed: Vec<u32>,
    pub notices: Vec<String>,
}

pub fn record(
    mut ui: MessageReader<UiMessageReceived>,
    mut chat: MessageReader<ChatReceived>,
    mut notices: MessageReader<NoticeReceived>,
    mut received: ResMut<Received>,
) {
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                static_data,
                data,
                ..
            } => {
                if interface == LobbyUi::NAME {
                    received.lobby = Some((*window, ss13_tgui::decode(data).unwrap()));
                }
                received
                    .windows
                    .push((*window, interface.clone(), static_data.clone()));
                received.data.push((*window, data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some((id, lobby)) = &mut received.lobby
                    && id == window
                {
                    *lobby = ss13_tgui::decode(data).unwrap();
                }
                received.data.push((*window, data.clone()));
            }
            UiServerMessage::Update { .. } => {}
            UiServerMessage::Close { window } => {
                received.closed.push(*window);
                if received.lobby.as_ref().is_some_and(|(id, _)| id == window) {
                    received.lobby = None;
                }
            }
        }
    }
    for ChatReceived(message) in chat.read() {
        received.chat.push(message.plain_text());
    }
    for NoticeReceived(notice) in notices.read() {
        received.notices.push(notice.text.clone());
    }
}

/// Every client's chat, for [`Drive::context`].
pub fn chat_context(st: &testing::Station) -> String {
    let chat: Vec<_> = st
        .players
        .iter()
        .map(|p| &p.app.world().resource::<Received>().chat)
        .collect();
    format!("; chat {chat:?}")
}

/// Helpers of a test's `Station` whose clients record [`Received`].
pub trait Client: Drive + DerefMut<Target = testing::Station> {
    /// A new client, in the lobby once its handshake is accepted.
    fn connect(&mut self) -> usize {
        let token = format!("p{}", self.players.len());
        let i = testing::Station::connect(self, &token, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        self.run_until("in the lobby", |s| s.received(i).lobby.is_some());
        i
    }

    /// An admin's "start now": the fixture lobby counts down for 10 minutes of game time (the
    /// clients gather on real time, so a short countdown could run out before they are ready),
    /// and once they are, the ticker restarts with a half-second countdown.
    fn start_now(&mut self) {
        let world = self.server.world_mut();
        world.resource_mut::<RoundConfig>().lobby_secs = 0.5;
        world.insert_resource(Ticker::default());
    }

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn lobby_act(&mut self, p: usize, action: &str) {
        let window = self.received(p).lobby.as_ref().expect("in the lobby").0;
        self.act(p, window, action, &[]);
    }

    fn window(&self, p: usize, interface: &str) -> Option<u32> {
        self.received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i, _)| i == interface)
            .map(|(id, ..)| *id)
    }

    fn windows_of(&self, p: usize, interface: &str) -> usize {
        self.received(p)
            .windows
            .iter()
            .filter(|(_, i, _)| i == interface)
            .count()
    }

    fn wait_window(&mut self, p: usize, interface: &str) -> (u32, Vec<u8>) {
        self.run_until(&format!("{interface} opens"), |s| {
            s.received(p).windows.iter().any(|(_, i, _)| i == interface)
        });
        let (id, _, st) = self
            .received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i, _)| i == interface)
            .unwrap();
        (*id, st.clone())
    }

    fn latest_data<T: serde::de::DeserializeOwned>(&self, p: usize, window: u32) -> T {
        let (_, data) = self
            .received(p)
            .data
            .iter()
            .rev()
            .find(|(w, _)| *w == window)
            .unwrap();
        ss13_tgui::decode(data).unwrap()
    }

    fn wait_chat(&mut self, p: usize, part: &str) {
        self.run_until(&format!("p{p} reads {part:?}"), |s| {
            s.received(p).chat.iter().any(|n| n.contains(part))
        });
    }

    fn read(&self, p: usize, part: &str) -> bool {
        self.received(p).chat.iter().any(|n| n.contains(part))
    }

    fn mob(&self, p: usize) -> Entity {
        self.server
            .world()
            .resource::<Players>()
            .mob(self.players[p].link)
            .unwrap()
    }
}

/// For tests that only read chat: a client's chat lines.
pub mod chat {
    use bevy::prelude::*;
    use ss13_net::chat::client::ChatReceived;

    #[derive(Resource, Default)]
    pub struct Received(pub Vec<String>);

    pub fn record(mut chat: MessageReader<ChatReceived>, mut received: ResMut<Received>) {
        for ChatReceived(message) in chat.read() {
            received.0.push(message.plain_text());
        }
    }
}

/// For tests that read the windows a client has open now: each open window's interface and last
/// data, and its chat.
pub mod open_windows {
    use std::collections::HashMap;

    use bevy::prelude::*;
    use ss13_net::chat::client::ChatReceived;
    use ss13_net::ui::client::UiMessageReceived;
    use ss13_shared::ui::UiServerMessage;

    #[derive(Resource, Default)]
    pub struct Received {
        pub windows: HashMap<u32, (String, Vec<u8>)>,
        pub chat: Vec<String>,
    }

    pub fn record(
        mut chat: MessageReader<ChatReceived>,
        mut ui: MessageReader<UiMessageReceived>,
        mut received: ResMut<Received>,
    ) {
        for ChatReceived(message) in chat.read() {
            received.chat.push(message.plain_text());
        }
        for UiMessageReceived(message) in ui.read() {
            match message {
                UiServerMessage::Open {
                    window,
                    interface,
                    data,
                    ..
                } => {
                    received
                        .windows
                        .insert(*window, (interface.clone(), data.clone()));
                }
                UiServerMessage::Update {
                    window,
                    data: Some(data),
                    ..
                } => {
                    if let Some(w) = received.windows.get_mut(window) {
                        w.1 = data.clone();
                    }
                }
                UiServerMessage::Close { window } => {
                    received.windows.remove(window);
                }
                _ => {}
            }
        }
    }
}
