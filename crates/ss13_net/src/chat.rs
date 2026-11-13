//! Transport for `ss13_shared::chat`. Chat logic lives in `ss13_chat`; this module only moves
//! server-built [`ChatMessage`](ss13_shared::chat::ChatMessage)s over the reliable stream.
//! Speech goes client → server as [`ClientIntent::Say`](ss13_shared::protocol::ClientIntent::Say).

#[cfg(feature = "server")]
pub mod server {
    use bevy::prelude::*;
    use ss13_shared::chat::ChatMessage;

    use crate::protocol::ServerMessage;
    use crate::server::{ClientLink, NetServerSystems, encode_message};

    /// Write to send one message to several connected clients (the hearers of one say).
    #[derive(Message, Clone, Debug)]
    pub struct SendChat {
        pub clients: Vec<Entity>,
        pub message: ChatMessage,
    }

    /// Write to close one client's say line (tgui-say `close` after a forced say).
    #[derive(Message, Clone, Copy, Debug)]
    pub struct SendCloseSay {
        pub client: Entity,
    }

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<SendChat>()
            .add_message::<SendCloseSay>()
            .add_systems(
                PostUpdate,
                (send, send_close_say).in_set(NetServerSystems::Send),
            );
    }

    fn send_close_say(mut outgoing: MessageReader<SendCloseSay>, clients: Query<&ClientLink>) {
        for SendCloseSay { client } in outgoing.read() {
            if let Ok(link) = clients.get(*client) {
                link.send(&ServerMessage::CloseSay);
            }
        }
    }

    fn send(mut outgoing: MessageReader<SendChat>, clients: Query<&ClientLink>) {
        for SendChat {
            clients: to,
            message,
        } in outgoing.read()
        {
            // Encoded once for every hearer.
            let Some(frame) = encode_message(&ServerMessage::Chat(message.clone())) else {
                continue;
            };
            for client in to {
                match clients.get(*client) {
                    Ok(link) => link.send_frame(frame.clone()),
                    Err(_) => debug!("chat to {client} dropped: not a connected client"),
                }
            }
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::prelude::*;
    use ss13_shared::chat::ChatMessage;

    /// Written by the client's receive system.
    #[derive(Message, Clone, Debug)]
    pub struct ChatReceived(pub ChatMessage);

    /// The server forced the say line's text out: the line closes.
    #[derive(Message, Clone, Copy, Debug)]
    pub struct CloseSayReceived;

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<ChatReceived>()
            .add_message::<CloseSayReceived>();
    }
}
