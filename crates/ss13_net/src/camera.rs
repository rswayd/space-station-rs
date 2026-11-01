//! Transport for [`CameraEffect`](ss13_shared::protocol::CameraEffect): server-driven moves of
//! one client's view (tg `animate(client, pixel_x, ...)`, e.g. `recoil_camera`). Over the
//! reliable stream; at most one per shot.

#[cfg(feature = "server")]
pub mod server {
    use bevy::prelude::*;
    use ss13_shared::protocol::CameraEffect;

    use crate::protocol::ServerMessage;
    use crate::server::{ClientLink, NetServerSystems};

    /// Write to send `effect` to one connected client.
    #[derive(Message, Clone, Copy, Debug)]
    pub struct SendCameraEffect {
        pub client: Entity,
        pub effect: CameraEffect,
    }

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<SendCameraEffect>()
            .add_systems(PostUpdate, send.in_set(NetServerSystems::Send));
    }

    fn send(mut outgoing: MessageReader<SendCameraEffect>, clients: Query<&ClientLink>) {
        for SendCameraEffect { client, effect } in outgoing.read() {
            if let Ok(link) = clients.get(*client) {
                link.send(&ServerMessage::Camera(*effect));
            }
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::prelude::*;
    use ss13_shared::protocol::CameraEffect;

    /// Written by the client's receive system.
    #[derive(Message, Clone, Copy, Debug)]
    pub struct CameraEffectReceived(pub CameraEffect);

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<CameraEffectReceived>();
    }
}
