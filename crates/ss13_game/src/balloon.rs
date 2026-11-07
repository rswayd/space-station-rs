//! tg balloon alerts (`code/modules/balloon_alert/balloon_alert.dm`): a short text that floats up
//! from an atom for one viewer ([`balloon_alert`]) or for everyone who sees it
//! ([`balloon_alert_to_viewers`]). Sent as [`Animation::Balloon`] to the viewer's client only;
//! `crate::balloon_client` draws it.
// shortcut: no `runechat_prefs_check` fallback to a chat line (the port has no runechat
// preference yet), no `balloon_alert_to_hearers`; add them with the preference.

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use ss13_net::animation::server::PlayAnimationFor;
use ss13_shared::TilePos;
use ss13_shared::animation::{Animation, MAX_BALLOON_CHARS};

use crate::health::DEFAULT_MESSAGE_RANGE;

/// tg `balloon_alert(viewer, text)` on `source`: nothing when `viewer` has no client.
pub fn balloon_alert(world: &mut World, viewer: Entity, source: Entity, text: &str) {
    let Some(client) = client_of(world, viewer) else {
        return;
    };
    let Some(entity) = atom_on_turf(world, source) else {
        return;
    };
    if !world.contains_resource::<Messages<PlayAnimationFor>>() {
        return;
    }
    world.write_message(PlayAnimationFor {
        client,
        entity,
        animation: Animation::Balloon { text: clip(text) },
    });
}

/// tg `balloon_alert_to_viewers(message, self_message, vision_distance, ignored_mobs)`: every
/// mob with a client in [`DEFAULT_MESSAGE_RANGE`] of `source` that perceives its tile and is not
/// blind; `source` itself gets `self_message` when given.
pub fn balloon_alert_to_viewers(
    world: &mut World,
    source: Entity,
    message: &str,
    self_message: Option<&str>,
    ignored: &[Entity],
) {
    let Some(at) = crate::explosions::turf_of(world, source) else {
        return;
    };
    let viewers: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .in_radius(at, DEFAULT_MESSAGE_RANGE)
        .filter(|e| !ignored.contains(e))
        .filter(|&e| client_of(world, e).is_some())
        .filter(|&e| ss13_chat::server::perceives_in(world, e, at))
        .filter(|&e| !crate::vision::is_blind(world, e))
        .collect();
    for viewer in viewers {
        let text = match self_message {
            Some(own) if viewer == source => own,
            _ => message,
        };
        balloon_alert(world, viewer, source, text);
    }
}

/// tg `isturf(src) ? src : get_atom_on_turf(src)`: the balloon of a held or stored item rises
/// from whoever holds it.
pub fn atom_on_turf(world: &World, mut e: Entity) -> Option<Entity> {
    for _ in 0..8 {
        if world.get::<TilePos>(e).is_some() {
            return Some(e);
        }
        e = world
            .get::<crate::inventory::CarriedBy>(e)
            .map(|c| c.0)
            .or_else(|| world.get::<crate::storage::StoredIn>(e).map(|s| s.0))?;
    }
    None
}

fn client_of(world: &World, mob: Entity) -> Option<Entity> {
    world
        .get_resource::<ss13_chat::server::Listeners>()
        .and_then(|l| l.client_of(mob))
}

/// At most [`MAX_BALLOON_CHARS`] characters.
fn clip(text: &str) -> String {
    text.chars().take(MAX_BALLOON_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_texts_are_clipped_by_characters() {
        let long = "я".repeat(MAX_BALLOON_CHARS + 5);
        assert_eq!(clip(&long).chars().count(), MAX_BALLOON_CHARS);
        assert_eq!(clip("нет места!"), "нет места!");
    }

    #[test]
    fn a_held_item_balloons_from_its_holder() {
        let mut world = World::new();
        let mob = world.spawn(TilePos::new(1, 1, 1)).id();
        let item = world.spawn(crate::inventory::CarriedBy(mob)).id();
        let inner = world.spawn(crate::storage::StoredIn(item)).id();
        assert_eq!(atom_on_turf(&world, inner), Some(mob));
        assert_eq!(atom_on_turf(&world, mob), Some(mob));
    }
}
