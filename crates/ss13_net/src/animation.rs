//! Transport for [`AnimationEvent`](ss13_shared::animation::AnimationEvent): a flick or transform
//! animation of one entity, to every client that has it replicated, by the id each knows it by.
//! Over the reliable stream, after the tick's replication batch, so a client always knows the
//! entity first. [`server::PlayAnimationFor`] sends one to a single client.

#[cfg(feature = "server")]
pub mod server {
    use bevy::prelude::*;
    use ss13_shared::animation::{Animation, AnimationEvent};

    use crate::protocol::ServerMessage;
    use crate::replication_server::Interest;
    use crate::server::ClientLink;

    /// Write to play `animation` on `entity` for the clients that see it now.
    #[derive(Message, Clone, Debug)]
    pub struct PlayAnimation {
        pub entity: Entity,
        pub animation: Animation,
    }

    /// Write to play `animation` on `entity` for one client only (tg `client.images` shown to
    /// one viewer, e.g. `balloon_alert`); nothing when that client does not see `entity` now.
    #[derive(Message, Clone, Debug)]
    pub struct PlayAnimationFor {
        pub client: Entity,
        pub entity: Entity,
        pub animation: Animation,
    }

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<PlayAnimation>()
            .add_message::<PlayAnimationFor>();
    }

    /// Cost: per animation, one chunk lookup and one small encode per client seeing it.
    pub(crate) fn send(
        mut outgoing: MessageReader<PlayAnimation>,
        mut personal: MessageReader<PlayAnimationFor>,
        interest: Res<Interest>,
        clients: Query<&ClientLink>,
        mut viewers: Local<Vec<(Entity, ss13_shared::protocol::NetId)>>,
    ) {
        for PlayAnimationFor {
            client,
            entity,
            animation,
        } in personal.read()
        {
            if let (Some(target), Ok(link)) =
                (interest.net_id(*client, *entity), clients.get(*client))
            {
                link.send(&ServerMessage::Animation(AnimationEvent {
                    target,
                    animation: animation.clone(),
                }));
            }
        }
        for PlayAnimation { entity, animation } in outgoing.read() {
            interest.viewers_of(*entity, &mut viewers);
            for &(client, target) in viewers.iter() {
                if let Ok(link) = clients.get(client) {
                    link.send(&ServerMessage::Animation(AnimationEvent {
                        target,
                        animation: animation.clone(),
                    }));
                }
            }
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::prelude::*;
    use ss13_shared::animation::AnimationEvent;

    /// Written by the client's receive system, after the replication it refers to is applied.
    #[derive(Message, Clone, Debug)]
    pub struct AnimationReceived(pub AnimationEvent);

    pub(crate) fn plugin(app: &mut App) {
        app.add_message::<AnimationReceived>();
    }
}
