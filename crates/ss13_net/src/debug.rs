//! Transport for `ss13_shared::debug`: the debug overlay's queries and dev tools. Their logic
//! (gate, rate limit, answers) lives in `ss13_debug`; this module only moves them over the
//! reliable stream.

#[cfg(feature = "server")]
pub mod server {
    use bevy::prelude::*;
    use ss13_shared::debug::{DebugRequest, DebugResponse};

    use crate::protocol::ServerMessage;
    use crate::server::{ClientLink, NetServerSystems};

    /// Untrusted, and arrives whatever the server's debug gate says: the handler decides.
    #[derive(Message, Clone, Debug)]
    pub struct DebugRequestReceived {
        pub client: Entity,
        pub request: DebugRequest,
    }

    /// Write to answer one connected client.
    #[derive(Message, Clone, Debug)]
    pub struct SendDebugResponse {
        pub client: Entity,
        pub response: DebugResponse,
    }

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<DebugRequestReceived>()
            .add_message::<SendDebugResponse>()
            .add_systems(PostUpdate, send.in_set(NetServerSystems::Send));
    }

    fn send(mut outgoing: MessageReader<SendDebugResponse>, clients: Query<&ClientLink>) {
        for SendDebugResponse { client, response } in outgoing.read() {
            if let Ok(link) = clients.get(*client) {
                link.send(&ServerMessage::Debug(response.clone()));
            }
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::prelude::*;
    use ss13_shared::debug::{DebugRequest, DebugResponse};

    use crate::client::{ClientTransport, NetClientState, NetClientSystems};
    use crate::protocol::ClientMessage;

    /// Written by the client's receive system.
    #[derive(Message, Clone, Debug)]
    pub struct DebugResponseReceived(pub DebugResponse);

    /// Write to send; dropped unless connected.
    #[derive(Message, Clone, Debug)]
    pub struct SendDebugRequest(pub DebugRequest);

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<DebugResponseReceived>()
            .add_message::<SendDebugRequest>()
            .add_systems(PostUpdate, send.in_set(NetClientSystems::Send));
    }

    fn send(
        mut outgoing: MessageReader<SendDebugRequest>,
        transport: Res<ClientTransport>,
        state: Res<NetClientState>,
    ) {
        if *state != NetClientState::Connected {
            outgoing.clear();
            return;
        }
        for SendDebugRequest(request) in outgoing.read() {
            transport.send(&ClientMessage::Debug(request.clone()));
        }
    }
}
