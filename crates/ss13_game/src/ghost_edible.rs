//! Food ghosts eat (server), after tg `/datum/component/ghost_edible`
//! (`code/datums/components/food/ghost_edible.dm`), given to the booberry muffin and the ghost
//! burger.
//!
//! - A new one with reagents tells every ghost "<name> is edible by ghosts!".
//! - Once a second (tg `SSdcs`) each ghost orbiting it adds 20% to the chance of a bite; a bite
//!   plays the eating sound and takes 3 units (its `bite_consumption`) of its reagents. Eaten
//!   up, it "disappears completely!" and leaves ectoplasm.
// shortcut: no shrinking towards 60% of its size as it is eaten (the appearance has no scale),
// until `Appearance` carries a transform scale.

use bevy::prelude::*;
use ss13_mapload::{DmFood, DmItem};
use ss13_shared::chat::Span;

use crate::chem::{Reagents, name_of, tell_mob, visible};
use crate::combat::CombatRng;
use crate::food::Food;
use crate::ghost::{Ghost, Orbiting};
use crate::sound::{PlaySound, playsound};

/// The foods tg gives the component to (their `Initialize`), with subtypes.
pub const GHOST_FOODS: [&str; 2] = [
    "/obj/item/food/muffin/booberry",
    "/obj/item/food/burger/ghost",
];
/// tg `bite_chance` default, per orbiting ghost.
const BITE_CHANCE: f32 = 20.0;
/// tg `SSdcs` `wait`, seconds.
const PROCESS_SECS: f64 = 1.0;
const ECTOPLASM: &str = "/obj/item/ectoplasm";
const EAT_SOUND: &str = "sound/items/eatfood.ogg";

/// tg `/datum/component/ghost_edible`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GhostEdible {
    pub bite_consumption: f32,
}

pub struct GhostEdiblePlugin;

impl Plugin for GhostEdiblePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(attach).add_systems(Update, munch);
    }
}

fn is_ghost_food(path: &str) -> bool {
    GHOST_FOODS
        .iter()
        .any(|root| ss13_reagents::is_under(path, root))
}

/// tg `AddComponent(/datum/component/ghost_edible, bite_consumption)` in the food's
/// `Initialize`: refused without reagents (`COMPONENT_INCOMPATIBLE`).
fn attach(add: On<Add<Food>>, foods: Query<(&DmItem, &DmFood, &Reagents)>, mut commands: Commands) {
    let food = add.entity;
    let Ok((item, dm, reagents)) = foods.get(food) else {
        return;
    };
    if !is_ghost_food(&item.path) || reagents.0.total_volume <= 0.0 {
        return;
    }
    let bite_consumption = dm.bite_consumption;
    commands.queue(move |world: &mut World| {
        if world.get_entity(food).is_err() {
            return;
        }
        world
            .entity_mut(food)
            .insert(GhostEdible { bite_consumption });
        // `notify_ghosts(...)`, as a line to every ghost.
        let text = format!("{} is edible by ghosts!", name_of(world, food));
        for ghost in crate::ghost_poll::ghosts(world) {
            tell_mob(world, ghost, Span::Notice, &text);
        }
    });
}

/// tg `ghost_edible/process`.
fn munch(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + PROCESS_SECS;
    let mut orbits = world.query_filtered::<&Orbiting, With<Ghost>>();
    let targets: Vec<Entity> = orbits.iter(world).map(|o| o.target).collect();
    let mut edibles = world.query::<(Entity, &GhostEdible)>();
    let foods: Vec<(Entity, f32)> = edibles
        .iter(world)
        .map(|(e, g)| (e, g.bite_consumption))
        .collect();
    for (food, bite) in foods {
        let ghosts = targets.iter().filter(|&&t| t == food).count();
        let chance = (ghosts as f32 * BITE_CHANCE).min(100.0);
        if chance <= 0.0 || !world.resource_mut::<CombatRng>().chance(chance) {
            continue;
        }
        bite_once(world, food, bite);
    }
}

/// One ghost bite: the sound, `remove_all(bite_consumption)`, and the food gone when empty.
pub fn bite_once(world: &mut World, food: Entity, bite: f32) {
    let Some(pos) = crate::explosions::turf_of(world, food) else {
        return;
    };
    let volume = world.resource_mut::<CombatRng>().range(10, 50) as f32;
    playsound(world, PlaySound::new(pos, EAT_SOUND, volume, true));
    let left = {
        let Some(mut reagents) = world.get_mut::<Reagents>(food) else {
            return;
        };
        reagents.0.remove_all(bite);
        reagents.0.total_volume
    };
    if left > 0.0 {
        return;
    }
    let text = format!("{} disappears completely!", name_of(world, food));
    visible(world, food, &text, &text, Span::Notice);
    crate::construction::spawn_obj(world, ECTOPLASM, pos);
    world.despawn(food);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_ghost_foods_and_their_subtypes() {
        assert!(is_ghost_food("/obj/item/food/muffin/booberry"));
        assert!(is_ghost_food("/obj/item/food/burger/ghost"));
        assert!(!is_ghost_food("/obj/item/food/muffin"));
        assert!(!is_ghost_food("/obj/item/food/burger/plain"));
    }
}
