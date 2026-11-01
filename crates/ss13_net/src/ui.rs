//! Transport for `ss13_shared::ui`: server-owned tgui windows. Window logic lives in `ss13_tgui`;
//! this module only moves its messages over the reliable stream. Acts travel as
//! [`ClientIntent::UiAct`](ss13_shared::protocol::ClientIntent::UiAct) through the intent path.

#[cfg(feature = "server")]
pub mod server {
    use bevy::prelude::*;
    use ss13_shared::ui::{UiClientMessage, UiServerMessage};

    use crate::protocol::ServerMessage;
    use crate::server::{ClientLink, NetServerSystems};

    /// Write to send a UI message to one connected client.
    #[derive(Message, Clone, Debug)]
    pub struct SendUi {
        pub client: Entity,
        pub message: UiServerMessage,
    }

    /// Untrusted: the window id may be foreign or stale. Written by the server's receive system.
    #[derive(Message, Clone, Debug)]
    pub struct UiMessageReceived {
        pub client: Entity,
        pub message: UiClientMessage,
    }

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<SendUi>()
            .add_message::<UiMessageReceived>()
            .add_systems(PostUpdate, send.in_set(NetServerSystems::Send));
    }

    fn send(mut outgoing: MessageReader<SendUi>, clients: Query<&ClientLink>) {
        for SendUi { client, message } in outgoing.read() {
            match clients.get(*client) {
                Ok(link) => {
                    link.send(&ServerMessage::Ui(message.clone()));
                }
                Err(_) => debug!("ui message to {client} dropped: not a connected client"),
            }
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::prelude::*;
    use ss13_shared::ui::{UiClientMessage, UiServerMessage};

    use crate::client::{ClientTransport, NetClientState, NetClientSystems};
    use crate::protocol::ClientMessage;

    /// Written by the client's receive system.
    #[derive(Message, Clone, Debug)]
    pub struct UiMessageReceived(pub UiServerMessage);

    /// Write to send; dropped unless connected.
    #[derive(Message, Clone, Debug)]
    pub struct SendUi(pub UiClientMessage);

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<UiMessageReceived>()
            .add_message::<SendUi>()
            .add_systems(PostUpdate, send.in_set(NetClientSystems::Send));
    }

    fn send(
        mut outgoing: MessageReader<SendUi>,
        transport: Res<ClientTransport>,
        state: Res<NetClientState>,
    ) {
        if *state != NetClientState::Connected {
            outgoing.clear();
            return;
        }
        for SendUi(message) in outgoing.read() {
            transport.send(&ClientMessage::Ui(message.clone()));
        }
    }
}
