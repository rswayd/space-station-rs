//! Transport for [`MoveAck`](ss13_shared::protocol::MoveAck): the server's answer to the own
//! mob's `ClientIntent::Move` steps, which client-side prediction reconciles against. Over the
//! reliable stream: the last ack before the mob stops must arrive, and acks are a few bytes at
//! most once per server tick per client.

#[cfg(feature = "server")]
pub mod server {
    use bevy::prelude::*;
    use ss13_shared::protocol::MoveAck;

    use crate::protocol::ServerMessage;
    use crate::server::{ClientLink, NetServerSystems};

    /// Write to send `ack` to one connected client.
    #[derive(Message, Clone, Copy, Debug)]
    pub struct SendMoveAck {
        pub client: Entity,
        pub ack: MoveAck,
    }

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<SendMoveAck>()
            .add_systems(PostUpdate, send.in_set(NetServerSystems::Send));
    }

    fn send(mut outgoing: MessageReader<SendMoveAck>, clients: Query<&ClientLink>) {
        for SendMoveAck { client, ack } in outgoing.read() {
            if let Ok(link) = clients.get(*client) {
                link.send(&ServerMessage::MoveAck(*ack));
            }
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::prelude::*;
    use ss13_shared::protocol::MoveAck;

    /// Written by the client's receive system.
    #[derive(Message, Clone, Copy, Debug)]
    pub struct MoveAckReceived(pub MoveAck);

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<MoveAckReceived>();
    }
}
