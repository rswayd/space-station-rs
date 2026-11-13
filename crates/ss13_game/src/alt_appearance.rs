//! tg `add_alt_appearance` / `remove_alt_appearance` (`code/game/atom/alternate_appearance.dm`,
//! `/datum/atom_hud/alternate_appearance/basic`) on `ss13_net`'s [`AltAppearances`]: an atom
//! looks different to some clients only. The seers are mobs (the client controlling one) and
//! [`SeeGroups`](ss13_net::server::SeeGroups) bits; only they are sent the alternate value.

use bevy::prelude::*;
pub use ss13_net::server::Seers;
use ss13_net::server::{AltAppearance, AltAppearances};
use ss13_shared::Appearance;

/// tg `add_alt_appearance(type, key, ...)`: `target` looks like `appearance` to `seers`. As tg,
/// nothing happens when `target` already has an alternate appearance under `key` (false).
pub fn add_alt_appearance(
    world: &mut World,
    target: Entity,
    key: impl Into<String>,
    seers: Seers,
    appearance: Appearance,
) -> bool {
    let key = key.into();
    let Ok(mut entity) = world.get_entity_mut(target) else {
        return false;
    };
    let mut alts = entity.get::<AltAppearances>().cloned().unwrap_or_default();
    if alts.0.iter().any(|a| a.key == key) {
        return false;
    }
    alts.0.push(AltAppearance {
        key,
        seers,
        appearance,
    });
    entity.insert(alts);
    true
}

/// tg `remove_alt_appearance(key)`.
pub fn remove_alt_appearance(world: &mut World, target: Entity, key: &str) {
    let Ok(mut entity) = world.get_entity_mut(target) else {
        return;
    };
    let Some(mut alts) = entity.get_mut::<AltAppearances>() else {
        return;
    };
    if !alts.0.iter().any(|a| a.key == key) {
        return;
    }
    alts.0.retain(|a| a.key != key);
    if alts.0.is_empty() {
        entity.remove::<AltAppearances>();
    }
}

/// The alternate appearance of `target` under `key`.
pub fn alt_appearance<'a>(
    world: &'a World,
    target: Entity,
    key: &str,
) -> Option<&'a AltAppearance> {
    world
        .get::<AltAppearances>(target)?
        .0
        .iter()
        .find(|a| a.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::{IconStateId, Layer};

    fn look(icon: u32) -> Appearance {
        Appearance {
            icon_state: IconStateId(icon),
            layer: Layer(4.0),
            plane: -4,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            mouse_opacity: Default::default(),
            glide: 0,
            overlays: Vec::new(),
        }
    }

    #[test]
    fn keys_add_once_and_remove_their_own_entry() {
        let mut world = World::new();
        let seer = world.spawn_empty().id();
        let target = world.spawn_empty().id();
        assert!(add_alt_appearance(
            &mut world,
            target,
            "a",
            Seers::mob(seer),
            look(1)
        ));
        assert!(!add_alt_appearance(
            &mut world,
            target,
            "a",
            Seers::mob(seer),
            look(2)
        ));
        assert!(add_alt_appearance(
            &mut world,
            target,
            "b",
            Seers::mob(seer),
            look(3)
        ));
        assert_eq!(
            alt_appearance(&world, target, "a").unwrap().appearance,
            look(1)
        );
        remove_alt_appearance(&mut world, target, "a");
        assert!(alt_appearance(&world, target, "a").is_none());
        assert_eq!(world.get::<AltAppearances>(target).unwrap().0.len(), 1);
        remove_alt_appearance(&mut world, target, "b");
        assert!(world.get::<AltAppearances>(target).is_none());
        // Gone targets are ignored.
        world.entity_mut(target).despawn();
        assert!(!add_alt_appearance(
            &mut world,
            target,
            "a",
            Seers::mob(seer),
            look(1)
        ));
        remove_alt_appearance(&mut world, target, "a");
    }
}
