//! Transport for [`SoundEvent`](ss13_shared::protocol::SoundEvent): one sound for one client,
//! attenuated by the server (tg `SEND_SOUND`). Over the reliable stream.

#[cfg(feature = "server")]
pub mod server {
    use bevy::prelude::*;
    use ss13_shared::protocol::SoundEvent;

    use crate::protocol::ServerMessage;
    use crate::server::{ClientLink, NetServerSystems};

    /// Write to send `sound` to one connected client.
    #[derive(Message, Clone, Debug)]
    pub struct SendSound {
        pub client: Entity,
        pub sound: SoundEvent,
    }

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<SendSound>()
            .add_systems(PostUpdate, send.in_set(NetServerSystems::Send));
    }

    fn send(mut outgoing: MessageReader<SendSound>, clients: Query<&ClientLink>) {
        for SendSound { client, sound } in outgoing.read() {
            if let Ok(link) = clients.get(*client) {
                link.send(&ServerMessage::Sound(sound.clone()));
            }
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::prelude::*;
    use ss13_shared::protocol::SoundEvent;

    /// Written by the client's receive system.
    #[derive(Message, Clone, Debug)]
    pub struct SoundReceived(pub SoundEvent);

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<SoundReceived>();
    }
}
