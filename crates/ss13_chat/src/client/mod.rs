//! Client half: the message store ([`ChatLog`]) fed from the server, and the docked panel.
//!
//! [`ChatClientPlugin`] is the data path only (headless-safe). [`ChatPanelPlugin`] adds the UI and
//! needs `ss13_ui_kit::UiKitPlugin` with a windowed or headless-rendering app.

pub mod donor;
pub mod history;
pub mod log;
pub mod panel;
pub mod prefs;
pub mod settings_ui;
pub mod stat_dock;
pub mod style;
mod wrap;

use bevy::prelude::*;
use ss13_net::chat::client::ChatReceived;
use ss13_net::client::{NetClientState, NetClientSystems, NoticeReceived};
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};

pub use log::{ChatEntry, ChatLog};
pub use panel::{ChatLine, ChatPanel, ChatView};
pub use prefs::{ChannelSet, ChatPrefs};
pub use settings_ui::ChatExported;

pub struct ChatClientPlugin;

impl Plugin for ChatClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatLog>().add_systems(
            PreUpdate,
            (receive, connection_notices).after(NetClientSystems::Receive),
        );
    }
}

pub struct ChatPanelPlugin;

impl Plugin for ChatPanelPlugin {
    fn build(&self, app: &mut App) {
        panel::plugin(app);
        app.add_systems(Update, donor::animate);
    }
}

fn system(text: impl Into<String>) -> ChatMessage {
    ChatMessage::simple(ChatChannel::System, Span::BoldNotice, text)
}

fn receive(
    mut chat: MessageReader<ChatReceived>,
    mut notices: MessageReader<NoticeReceived>,
    time: Res<Time<Real>>,
    mut log: ResMut<ChatLog>,
) {
    let now = time.elapsed_secs_f64();
    for NoticeReceived(notice) in notices.read() {
        log.push(system(notice.text.clone()), now);
    }
    for ChatReceived(message) in chat.read() {
        log.push(message.clone(), now);
    }
}

/// tgui-panel's "Reconnected" marker, as system lines.
fn connection_notices(
    state: Res<NetClientState>,
    mut was_connected: Local<bool>,
    time: Res<Time<Real>>,
    mut log: ResMut<ChatLog>,
) {
    if !state.is_changed() {
        return;
    }
    let now = time.elapsed_secs_f64();
    match &*state {
        NetClientState::Connected if !*was_connected => {
            *was_connected = true;
            log.push(system("Подключено к серверу."), now);
        }
        NetClientState::Disconnected { reason } if *was_connected => {
            *was_connected = false;
            log.push(system(format!("Соединение потеряно: {reason}")), now);
        }
        _ => {}
    }
}
