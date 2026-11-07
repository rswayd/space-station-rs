//! BandaStation `/datum/component/rip_and_tear`
//! (`modular_bandastation/~components/code/rip_and_tear_item.dm`) and tg
//! `/datum/component/torn_wall` (`code/datums/components/torn_wall.dm`): a sledgehammer held in
//! both hands tears at a wall it hits. Each `tear_time / 3` do_after (times the
//! `reinforced_multiplier` on a reinforced wall, which only the D-6 can tear) costs its stamina
//! and tears the wall one stage further; the third stage devastates it (`dismantle_wall(devastated
//! = TRUE)`). The hammer keeps going until the wall falls, the do_after is broken or the user is
//! too tired.
// shortcut: no crack overlay, examine line or welder repair of a torn wall; add with wall
// overlays.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::construction::Job;
use crate::tools::{Actor, DoAfter, tell};
use crate::weapons::stamina_cost::exhausted;
use crate::weapons::two_handed::is_wielded;

const WALL: &str = "/turf/closed/wall";
const R_WALL: &str = "/turf/closed/wall/r_wall";
/// `TORN_WALL_RUINED`.
const TORN_WALL_RUINED: u8 = 2;
const PRYING: &str = "sound/machines/airlock/airlock_alien_prying.ogg";
const IMPACT: &str = "sound/effects/meteorimpact.ogg";
const BANG: &str = "sound/effects/bang.ogg";

/// A sledgehammer's `AddComponent(/datum/component/rip_and_tear, ...)` (`require_wielded`
/// is TRUE for all; `stamina_cost_wielded` defaults to `stamina_cost`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tear {
    pub root: &'static str,
    pub stamina_cost: f32,
    /// `None`: reinforced walls are too strong.
    pub reinforced_multiplier: Option<f32>,
    /// Deciseconds.
    pub tear_time: f32,
}

pub const TEARS: [Tear; 3] = [
    Tear {
        root: "/obj/item/sledgehammer",
        stamina_cost: 40.0,
        reinforced_multiplier: None,
        tear_time: 60.0,
    },
    Tear {
        root: "/obj/item/sledgehammer/tactical",
        stamina_cost: 30.0,
        reinforced_multiplier: None,
        tear_time: 50.0,
    },
    Tear {
        root: "/obj/item/sledgehammer/syndie",
        stamina_cost: 20.0,
        reinforced_multiplier: Some(2.0),
        tear_time: 30.0,
    },
];

pub fn tear_of(path: &str) -> Option<&'static Tear> {
    TEARS
        .iter()
        .filter(|t| is_under(path, t.root))
        .max_by_key(|t| t.root.len())
}

/// tg `torn_wall`'s `current_stage`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TornWall(pub u8);

/// `rip_time`: deciseconds of one tearing step.
pub fn rip_time(tear: &Tear, reinforced: bool) -> f32 {
    let mult = if reinforced {
        tear.reinforced_multiplier.unwrap_or(1.0)
    } else {
        1.0
    };
    tear.tear_time * mult / 3.0
}

/// Why `validate_target` stops: `None` lets the click go on (not a wall).
fn validate(
    world: &World,
    item: Entity,
    tear: &Tear,
    wall_path: &str,
) -> Result<(), Option<&'static str>> {
    if !is_under(wall_path, WALL) {
        return Err(None);
    }
    if !is_wielded(world, item) {
        return Err(Some("нужно держать двумя руками!"));
    }
    if is_under(wall_path, R_WALL) && tear.reinforced_multiplier.is_none() {
        return Err(Some("слишком крепкая!"));
    }
    Ok(())
}

fn tear_and_path(world: &World, item: Entity, wall: Entity) -> Option<(Tear, String)> {
    let tear = *world.get::<DmItem>(item).and_then(|d| tear_of(&d.path))?;
    Some((tear, crate::construction::path_of(world, wall)?))
}

/// `on_pre_attack` of a sledgehammer on a wall. True when the click is taken.
pub fn pre_attack(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
    wall: Entity,
) -> bool {
    let Some((tear, path)) = tear_and_path(world, item, wall) else {
        return false;
    };
    match validate(world, item, &tear, &path) {
        Err(None) => false,
        Err(Some(text)) => {
            tell(world, client, Span::Warning, text);
            true
        }
        Ok(()) => {
            start(world, client, mob, item, wall);
            true
        }
    }
}

/// One `rip_and_tear` step: the checks, the message and sound, the do_after.
fn start(world: &mut World, client: Entity, mob: Entity, item: Entity, wall: Entity) {
    let Some((tear, path)) = tear_and_path(world, item, wall) else {
        return;
    };
    if world.get::<DoAfter>(mob).is_some() {
        tell(world, client, Span::Warning, "заняты!");
        return;
    }
    let loss = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .map_or(0.0, |s| s.stamina);
    if exhausted(loss, tear.stamina_cost) {
        tell(world, client, Span::Warning, "вы слишком устали!");
        return;
    }
    let name = |world: &World, e: Entity| {
        world
            .get::<DmName>(e)
            .map(|n| n.name.clone())
            .or_else(|| world.get::<Name>(e).map(|n| n.as_str().to_owned()))
            .unwrap_or_else(|| "что-то".to_owned())
    };
    let text = format!(
        "{} начинает выламывать {} при помощи {}!",
        name(world, mob),
        name(world, wall),
        name(world, item)
    );
    crate::chem::visible(world, mob, &text, &text, Span::Warning);
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, PRYING, 100.0, true),
        );
    }
    tell(world, client, Span::Notice, "выламываем...");
    let actor = Actor {
        client,
        mob,
        item: Some(item),
    };
    let time = rip_time(&tear, is_under(&path, R_WALL));
    crate::tools::do_after(world, actor, wall, time, 0, Job::TearWall);
}

/// The do_after done: checks again, tears the wall a stage, takes the stamina, goes on.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter) {
    let (client, wall) = (action.client, action.target);
    let Some(item) = action.item else {
        return;
    };
    let held = world
        .get::<crate::inventory::Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(item)));
    let Some((tear, path)) = tear_and_path(world, item, wall) else {
        return;
    };
    if !held || validate(world, item, &tear, &path).is_err() {
        return;
    }
    crate::animation::do_attack_animation(world, mob, wall);
    let pos = action.target_pos;
    let ruined = torn_wall(world, wall, pos);
    crate::status_effects::adjust_stamina(world, mob, tear.stamina_cost);
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, IMPACT, 100.0, true),
    );
    if !ruined {
        start(world, client, mob, item, wall);
    }
}

/// `AddComponent(/datum/component/torn_wall)`: the next stage; true when the wall fell.
pub fn torn_wall(world: &mut World, wall: Entity, pos: TilePos) -> bool {
    // The first tear adds the component at `TORN_WALL_INITIAL`, each next one `increase_stage`.
    let stage = world.get::<TornWall>(wall).map_or(0, |t| t.0 + 1);
    if stage >= TORN_WALL_RUINED {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, IMPACT, 100.0, true),
        );
        crate::construction::devastate_wall(world, wall, pos);
        return true;
    }
    world.entity_mut(wall).insert(TornWall(stage));
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, BANG, 50.0, true));
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hammers_tear_by_their_component() {
        let hammer = tear_of("/obj/item/sledgehammer").unwrap();
        assert_eq!(rip_time(hammer, false), 20.0);
        let syndie = tear_of("/obj/item/sledgehammer/syndie").unwrap();
        assert_eq!(rip_time(syndie, true), 20.0);
        assert_eq!(rip_time(syndie, false), 10.0);
        assert_eq!(
            tear_of("/obj/item/sledgehammer/tactical")
                .unwrap()
                .stamina_cost,
            30.0
        );
        assert!(tear_of("/obj/item/fireaxe").is_none());
    }

    /// Unwielded the hammer refuses; the plain one cannot tear a reinforced wall; a third
    /// tear ruins the wall.
    #[test]
    fn walls_tear_in_three_stages() {
        let mut world = World::new();
        world.init_resource::<Messages<crate::sound::PlaySound>>();
        let hammer = world.spawn_empty().id();
        let plain = tear_of("/obj/item/sledgehammer").unwrap();
        assert_eq!(
            validate(&world, hammer, plain, "/turf/open/floor"),
            Err(None)
        );
        assert_eq!(
            validate(&world, hammer, plain, WALL),
            Err(Some("нужно держать двумя руками!"))
        );
        let mob = world.spawn_empty().id();
        let offhand = world.spawn_empty().id();
        world
            .entity_mut(hammer)
            .insert(crate::weapons::two_handed::Wielded { mob, offhand });
        assert_eq!(validate(&world, hammer, plain, WALL), Ok(()));
        assert_eq!(
            validate(&world, hammer, plain, R_WALL),
            Err(Some("слишком крепкая!"))
        );
        let syndie = tear_of("/obj/item/sledgehammer/syndie").unwrap();
        assert_eq!(validate(&world, hammer, syndie, R_WALL), Ok(()));
        let pos = TilePos::new(1, 1, 1);
        let wall = world.spawn(pos).id();
        assert!(!torn_wall(&mut world, wall, pos));
        assert_eq!(world.get::<TornWall>(wall), Some(&TornWall(0)));
        assert!(!torn_wall(&mut world, wall, pos));
        assert_eq!(world.get::<TornWall>(wall), Some(&TornWall(1)));
        // Without its `DmTurfBuild` the test wall is not dismantled, but the stage is ruined.
        assert!(torn_wall(&mut world, wall, pos));
    }
}
