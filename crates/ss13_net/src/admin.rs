//! Transport for `ss13_shared::admin`: admin verbs and adminhelp tickets. Their logic (rights,
//! rate limit, tickets) lives in `ss13_admin`; this module only moves them over the reliable
//! stream.

#[cfg(feature = "server")]
pub mod server {
    use bevy::prelude::*;
    use ss13_shared::admin::{AdminRequest, AdminResponse};

    use crate::protocol::ServerMessage;
    use crate::server::{ClientLink, NetServerSystems};

    /// Untrusted, from any client: the handler checks the client's rights.
    #[derive(Message, Clone, Debug)]
    pub struct AdminRequestReceived {
        pub client: Entity,
        pub request: AdminRequest,
    }

    /// Write to answer one connected client.
    #[derive(Message, Clone, Debug)]
    pub struct SendAdminResponse {
        pub client: Entity,
        pub response: AdminResponse,
    }

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<AdminRequestReceived>()
            .add_message::<SendAdminResponse>()
            .add_systems(PostUpdate, send.in_set(NetServerSystems::Send));
    }

    fn send(mut outgoing: MessageReader<SendAdminResponse>, clients: Query<&ClientLink>) {
        for SendAdminResponse { client, response } in outgoing.read() {
            if let Ok(link) = clients.get(*client) {
                link.send(&ServerMessage::Admin(response.clone()));
            }
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::prelude::*;
    use ss13_shared::admin::{AdminRequest, AdminResponse};

    use crate::client::{ClientTransport, NetClientState, NetClientSystems};
    use crate::protocol::ClientMessage;

    /// Written by the client's receive system.
    #[derive(Message, Clone, Debug)]
    pub struct AdminResponseReceived(pub AdminResponse);

    /// Write to send; dropped unless connected.
    #[derive(Message, Clone, Debug)]
    pub struct SendAdminRequest(pub AdminRequest);

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<AdminResponseReceived>()
            .add_message::<SendAdminRequest>()
            .add_systems(PostUpdate, send.in_set(NetClientSystems::Send));
    }

    fn send(
        mut outgoing: MessageReader<SendAdminRequest>,
        transport: Res<ClientTransport>,
        state: Res<NetClientState>,
    ) {
        if *state != NetClientState::Connected {
            outgoing.clear();
            return;
        }
        for SendAdminRequest(request) in outgoing.read() {
            transport.send(&ClientMessage::Admin(request.clone()));
        }
    }
}
