//! Transport for [`SoundEvent`](ss13_shared::protocol::SoundEvent): one sound for one client,
//! attenuated by the server (tg `SEND_SOUND`), and for
//! [`ChannelSound`](ss13_shared::protocol::ChannelSound): a sound on a numbered channel. Over the
//! reliable stream.

#[cfg(feature = "server")]
pub mod server {
    use bevy::prelude::*;
    use ss13_shared::protocol::{ChannelSound, SoundEvent};

    use crate::protocol::ServerMessage;
    use crate::server::{ClientLink, NetServerSystems};

    /// Write to send `sound` to one connected client.
    #[derive(Message, Clone, Debug)]
    pub struct SendSound {
        pub client: Entity,
        pub sound: SoundEvent,
    }

    /// Write to play or stop a channel sound on one connected client.
    #[derive(Message, Clone, Debug)]
    pub struct SendChannelSound {
        pub client: Entity,
        pub sound: ChannelSound,
    }

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<SendSound>()
            .add_message::<SendChannelSound>()
            .add_systems(
                PostUpdate,
                (send, send_channel).in_set(NetServerSystems::Send),
            );
    }

    fn send(mut outgoing: MessageReader<SendSound>, clients: Query<&ClientLink>) {
        for SendSound { client, sound } in outgoing.read() {
            if let Ok(link) = clients.get(*client) {
                link.send(&ServerMessage::Sound(sound.clone()));
            }
        }
    }

    fn send_channel(mut outgoing: MessageReader<SendChannelSound>, clients: Query<&ClientLink>) {
        for SendChannelSound { client, sound } in outgoing.read() {
            if let Ok(link) = clients.get(*client) {
                link.send(&ServerMessage::ChannelSound(sound.clone()));
            }
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::prelude::*;
    use ss13_shared::protocol::{ChannelSound, SoundEvent};

    /// Written by the client's receive system.
    #[derive(Message, Clone, Debug)]
    pub struct SoundReceived(pub SoundEvent);

    /// Written by the client's receive system.
    #[derive(Message, Clone, Debug)]
    pub struct ChannelSoundReceived(pub ChannelSound);

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<SoundReceived>()
            .add_message::<ChannelSoundReceived>();
    }
}
