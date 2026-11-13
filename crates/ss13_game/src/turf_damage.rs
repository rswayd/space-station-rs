//! Burnt and broken floors (server): tg `/turf/open/burn_tile`, `break_tile` and the burnt or
//! broken overlay of `/turf/open/update_overlays` (a random state of `damaged_dmi`). The type
//! data is [`DmTurfDamage`]; heavy fires report their turfs ([`Atmos::drain_burnt_turfs`]).
// shortcut: turf types mapped already broken or burnt (two, off-station) are not drawn so, and
// smoothed turfs get the overlay without tg's large-smoothing offset; nothing repairs a floor
// yet (crowbarring the tile off removes the overlay with the turf).

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_mapload::IconIndex;
use ss13_mapload::turf_damage::{DmTurfDamage, TurfBurn};
use ss13_shared::{Appearance, Overlay, TilePos};

use crate::combat::CombatRng;
use crate::construction::{scrape_away, turf_at};

/// tg `broken` / `burnt` of a turf entity.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurfDamaged {
    pub broken: bool,
    pub burnt: bool,
}

/// `burn_tile()` of every turf a heavy fire reached this fire.
pub fn burn_turfs(world: &mut World) {
    let Some(tiles) = world
        .get_resource_mut::<Atmos>()
        .map(|mut a| a.drain_burnt_turfs())
    else {
        return;
    };
    for pos in tiles {
        burn_tile(world, pos);
    }
}

/// tg `burn_tile()`: true when the turf changed.
pub fn burn_tile(world: &mut World, pos: TilePos) -> bool {
    let Some(turf) = turf_at(world, pos) else {
        return false;
    };
    let Some(damage) = world.get::<DmTurfDamage>(turf).cloned() else {
        return false;
    };
    let state = world.get::<TurfDamaged>(turf).copied().unwrap_or_default();
    match damage.burn {
        TurfBurn::Never => false,
        // `/turf/open/floor/carpet/burn_tile`: burnt, then `make_plating()`.
        TurfBurn::Plating => scrape_away(world, pos, true).is_some(),
        TurfBurn::Overlay => {
            if damage.damaged_dmi.is_empty() || state.burnt {
                return false;
            }
            world.entity_mut(turf).insert(TurfDamaged {
                burnt: true,
                ..state
            });
            // A broken turf keeps showing broken (`update_overlays` checks `broken` first).
            if !state.broken {
                add_overlay(world, turf, &damage.damaged_dmi, damage.burn_choices());
            }
            true
        }
    }
}

/// tg `/turf/open/break_tile()` (`broken` and its overlay); the base `/turf` does nothing.
pub fn break_tile(world: &mut World, pos: TilePos) -> bool {
    let Some(turf) = turf_at(world, pos) else {
        return false;
    };
    // `/turf/open/floor/engine/break_tile`: unbreakable.
    if crate::construction::path_of(world, turf)
        .is_some_and(|p| crate::reinforced_floor::is_engine(&p))
    {
        return false;
    }
    let Some(damage) = world.get::<DmTurfDamage>(turf).cloned() else {
        return false;
    };
    let state = world.get::<TurfDamaged>(turf).copied().unwrap_or_default();
    if damage.damaged_dmi.is_empty() || state.broken {
        return false;
    }
    world.entity_mut(turf).insert(TurfDamaged {
        broken: true,
        ..state
    });
    // The broken look replaces a burnt one.
    remove_damage_overlays(world, turf, &damage);
    add_overlay(world, turf, &damage.damaged_dmi, &damage.broken_states);
    true
}

fn add_overlay(world: &mut World, turf: Entity, dmi: &str, states: &[String]) {
    if states.is_empty() {
        return;
    }
    let i = world
        .resource_mut::<CombatRng>()
        .range(0, states.len() as u32 - 1) as usize;
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|icons| icons.get(dmi, &states[i]))
    else {
        warn!("{dmi}:{} is not in the atlas", states[i]);
        return;
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(turf) {
        appearance.overlays.push(Overlay::new(id));
    }
}

fn remove_damage_overlays(world: &mut World, turf: Entity, damage: &DmTurfDamage) {
    let ids: Vec<_> = {
        let Some(icons) = world.get_resource::<IconIndex>() else {
            return;
        };
        damage
            .broken_states
            .iter()
            .chain(&damage.burnt_states)
            .filter_map(|s| icons.get(&damage.damaged_dmi, s))
            .collect()
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(turf) {
        appearance.overlays.retain(|o| !ids.contains(&o.icon_state));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ss13_mapload::Turf;
    use ss13_shared::{IconStateId, Layer, MouseOpacity};
    use ss13_world::{SpatialIndex, WorldPlugin};

    use super::*;

    const DMI: &str = "icons/turf/damaged.dmi";

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, WorldPlugin))
            .init_resource::<CombatRng>();
        let mut icons = HashMap::new();
        for (i, state) in ["damaged1", "floorscorched1"].into_iter().enumerate() {
            icons.insert(
                (DMI.to_owned(), state.to_owned()),
                IconStateId(i as u32 + 10),
            );
        }
        app.insert_resource(IconIndex(icons));
        app.world_mut()
            .resource_mut::<SpatialIndex>()
            .create_level(1, 3, 3)
            .unwrap();
        app
    }

    fn floor(world: &mut World, damage: DmTurfDamage) -> Entity {
        world
            .spawn((
                Turf,
                damage,
                Appearance {
                    icon_state: IconStateId(1),
                    layer: Layer(2.0),
                    plane: -6,
                    color: None,
                    pixel_offset: [0, 0],
                    rotation: 0,
                    mouse_opacity: MouseOpacity::default(),
                    glide: 0,
                    overlays: Vec::new(),
                },
                TilePos::new(2, 2, 1),
            ))
            .id()
    }

    fn iron() -> DmTurfDamage {
        DmTurfDamage {
            damaged_dmi: DMI.into(),
            broken_states: vec!["damaged1".into()],
            burnt_states: vec!["floorscorched1".into()],
            ..default()
        }
    }

    fn overlays(world: &World, e: Entity) -> Vec<u32> {
        world
            .get::<Appearance>(e)
            .unwrap()
            .overlays
            .iter()
            .map(|o| o.icon_state.0)
            .collect()
    }

    /// A burn draws one scorch mark; a second burn does nothing; breaking swaps it for the
    /// broken look; a turf without `damaged_dmi` or one that never burns stays as it is.
    #[test]
    fn burn_then_break() {
        let mut app = app();
        let world = app.world_mut();
        let pos = TilePos::new(2, 2, 1);
        let e = floor(world, iron());
        assert!(burn_tile(world, pos));
        assert_eq!(overlays(world, e), [11]);
        assert!(!burn_tile(world, pos));
        assert_eq!(overlays(world, e), [11]);
        assert!(break_tile(world, pos));
        assert_eq!(overlays(world, e), [10]);
        assert!(!burn_tile(world, pos), "already burnt");
        world.despawn(e);

        for damage in [
            DmTurfDamage::default(),
            DmTurfDamage {
                burn: TurfBurn::Never,
                ..iron()
            },
        ] {
            let e = floor(world, damage);
            assert!(!burn_tile(world, pos));
            assert!(overlays(world, e).is_empty());
            world.despawn(e);
        }
    }
}
