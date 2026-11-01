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

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<SendChat>()
            .add_systems(PostUpdate, send.in_set(NetServerSystems::Send));
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

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<ChatReceived>();
    }
}
