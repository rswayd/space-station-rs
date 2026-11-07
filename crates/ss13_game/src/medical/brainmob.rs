//! The brain's mob (server), after tg `/mob/living/brain` (`brain.dm`, `life.dm`) and the brain
//! organ's `transfer_identity`, `on_mob_insert` and `on_mob_remove` (`brain_item.dm`).
//!
//! - A brain taken out of a body that has a mind (its client, or the ghost that left it) takes
//!   the mind along: a [`BrainMob`] in the brain gets the client, or becomes the ghost's body
//!   (tg `L.mind.transfer_to(brainmob)`). In a bare brain the brainmob is dead: a step ghosts it
//!   and what it says goes to deadchat.
//! - A brain with a brainmob put into a body hands the mind to that body: the body's own player
//!   ghosts first (tg `brain_owner.ghostize()`) and loses the body, the brainmob is gone.
//! - A brainmob has no tile of its own: every tick it stands on its container's turf
//!   ([`follow_containers`]), so it hears and speaks from there; it is never replicated (nobody
//!   sees or clicks it). When its container is deleted its player ghosts (tg `Destroy` ->
//!   `ghostize`).
//! - In a brain processor (`crate::silicon::mmi`) the brainmob is alive: it speaks, hears and
//!   talks on the processor's radio.
// shortcut: no DNA, suicide flag, timeofdeath, `emp_damage` gibberish, robot speech span or
// silicon emotes; the brain keeps its own name (tg "[name]'s brain"); a changeling's or a
// vampire-incompatible body does not refuse the mind.

use bevy::prelude::*;
use ss13_net::server::{ControlledEntity, SeeInvisible, Sight, Viewer};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::ghost::Ghost;
use crate::health::Stat;
use crate::hud::{HealthHudData, HudData};
use crate::inventory::Inventory;
use crate::server::{MoveQueue, PlayerMob, Players};

/// tg `/mob/living/brain`: the mob of a brain outside a body or of a brain processor.
#[derive(Component, Clone, Copy, Debug)]
pub struct BrainMob {
    /// The brain item or the brain processor it is in (tg `loc`, `container`).
    pub container: Entity,
}

/// On a brain item or a brain processor: its [`BrainMob`] (tg `brainmob`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Occupant(pub Entity);

/// The mind of a mob: its client, or the ghost that left it (tg `mind`, `get_ghost()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mind {
    Client(Entity),
    Ghost(Entity),
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        follow_containers
            .run_if(any_with_component::<BrainMob>)
            .before(crate::server::follow_mobs),
    );
}

/// The ghost that left `body` (tg `get_ghost()`).
pub fn ghost_of(world: &mut World, body: Entity) -> Option<Entity> {
    world
        .query::<(Entity, &Ghost)>()
        .iter(world)
        .find(|(_, g)| g.body == Some(body))
        .map(|(e, _)| e)
}

pub fn mind_of(world: &mut World, mob: Entity) -> Option<Mind> {
    if let Some(player) = world.get::<PlayerMob>(mob) {
        return Some(Mind::Client(player.client));
    }
    ghost_of(world, mob).map(Mind::Ghost)
}

/// tg `mind.transfer_to(to)`: the client of `from` now controls `to`, or the ghost that left
/// `from` now belongs to `to`. False when `from` has no mind.
pub fn transfer_mind(world: &mut World, from: Entity, to: Entity) -> bool {
    match mind_of(world, from) {
        Some(Mind::Client(client)) => {
            // The mind's data first, so the body's HUDs reopen with its antag HUD.
            crate::mind::transfer(world, from, to);
            let moved = possess(world, client, from, to);
            if !moved {
                crate::mind::transfer(world, to, from);
            }
            moved
        }
        Some(Mind::Ghost(ghost)) => {
            if let Some(mut g) = world.get_mut::<Ghost>(ghost) {
                g.body = Some(to);
            }
            crate::mind::transfer(world, from, to);
            true
        }
        None => false,
    }
}

/// `client` leaves `from` (which stays, without a client) and controls `to`; a body's HUD
/// windows open as when a ghost re-enters it. False when `to` is nowhere on the map.
pub(crate) fn possess(world: &mut World, client: Entity, from: Entity, to: Entity) -> bool {
    let Some(pos) = crate::explosions::turf_of(world, to) else {
        return false;
    };
    let queue = world
        .get::<MoveQueue>(from)
        .map(MoveQueue::continued)
        .unwrap_or_default();
    if let Ok(mut old) = world.get_entity_mut(from) {
        old.remove::<(PlayerMob, MoveQueue, HudData, HealthHudData)>();
    }
    let body = world.get::<Inventory>(to).is_some();
    world.entity_mut(to).insert((PlayerMob { client }, queue));
    if body {
        world
            .entity_mut(to)
            .insert((HudData::default(), HealthHudData::default()));
        if let Some(mut inventory) = world.get_mut::<Inventory>(to) {
            inventory.set_changed();
        }
    }
    if let Ok(mut c) = world.get_entity_mut(client) {
        c.remove::<(SeeInvisible, Sight)>()
            .insert((ControlledEntity(to), Viewer::new(pos)));
    }
    if let Some(mut players) = world.get_resource_mut::<Players>() {
        players.set_mob(client, to);
    }
    if body {
        crate::ghost::reopen_body_huds(world, client, to);
    }
    true
}

/// A new brainmob in `container`, on `pos`, named `name` (tg `new /mob/living/brain(src)`).
pub(crate) fn spawn_brainmob(
    world: &mut World,
    container: Entity,
    pos: TilePos,
    name: &str,
    stat: Stat,
) -> Entity {
    let mob = world
        .spawn((
            BrainMob { container },
            Name::new(name.to_owned()),
            stat,
            pos,
        ))
        .id();
    world.entity_mut(container).insert(Occupant(mob));
    mob
}

/// tg brain `on_mob_remove` -> `transfer_identity(owner)`: the removed brain takes the mind of
/// `mob` into a new brainmob (dead, tg's default `stat`). Nothing for a body without a mind,
/// or for a brain that already has a brainmob.
pub(crate) fn brain_removed(world: &mut World, brain: Entity, mob: Entity) {
    if world.get::<Occupant>(brain).is_some() || mind_of(world, mob).is_none() {
        return;
    }
    // The caller drops the brain where the body is.
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = super::name_of(world, mob);
    let brainmob = spawn_brainmob(world, brain, pos, &name, Stat::Dead);
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    transfer_mind(world, mob, brainmob);
    if let Some(client) = client {
        crate::tools::tell(
            world,
            client,
            Span::Notice,
            "Вы чувствуете лёгкую дезориентацию. Это нормально, когда вы всего лишь мозг.",
        );
    }
}

/// tg brain `on_mob_insert`: the brainmob's mind goes into `mob` (whose own player ghosts and
/// loses it, tg `mind.transfer_to` clearing the old mind's `current`); the brainmob is gone.
pub(crate) fn brain_inserted(world: &mut World, brain: Entity, mob: Entity) {
    let Some(Occupant(brainmob)) = world.get::<Occupant>(brain).copied() else {
        return;
    };
    world.entity_mut(brain).remove::<Occupant>();
    if world.get_entity(brainmob).is_err() {
        return;
    }
    if mind_of(world, brainmob).is_some() {
        if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
            crate::ghost::ghostize_client(world, client);
        }
        while let Some(ghost) = ghost_of(world, mob) {
            if let Some(mut g) = world.get_mut::<Ghost>(ghost) {
                g.body = None;
            }
        }
        transfer_mind(world, brainmob, mob);
    }
    despawn_brainmob(world, brainmob);
}

/// tg `/mob/living/brain/Destroy`: its player ghosts for good, then it is gone.
pub(crate) fn despawn_brainmob(world: &mut World, brainmob: Entity) {
    if let Some(client) = world.get::<PlayerMob>(brainmob).map(|p| p.client) {
        crate::ghost::ghostize_client(world, client);
    }
    while let Some(ghost) = ghost_of(world, brainmob) {
        if let Some(mut g) = world.get_mut::<Ghost>(ghost) {
            g.body = None;
        }
    }
    if let Some(container) = world.get::<BrainMob>(brainmob).map(|b| b.container)
        && world.get::<Occupant>(container) == Some(&Occupant(brainmob))
    {
        world.entity_mut(container).remove::<Occupant>();
    }
    world.try_despawn(brainmob).ok();
}

/// The brainmob stands on its container's turf (wherever that is: on the floor, in a hand, in a
/// bag); a deleted container takes it along.
// Cost: one pass over the brainmobs (a handful per round) per tick, a short parent walk each.
fn follow_containers(world: &mut World) {
    let mobs: Vec<(Entity, Entity)> = world
        .query::<(Entity, &BrainMob)>()
        .iter(world)
        .map(|(e, b)| (e, b.container))
        .collect();
    for (brainmob, container) in mobs {
        if world.get_entity(container).is_err() {
            despawn_brainmob(world, brainmob);
            continue;
        }
        let Some(pos) = crate::explosions::turf_of(world, container) else {
            continue;
        };
        if world.get::<TilePos>(brainmob) != Some(&pos) {
            world.entity_mut(brainmob).insert(pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::Health;
    use crate::medical::organs::{self, OrganHost, OrganOf, Slot, spawn_organ};

    fn human(world: &mut World, pos: TilePos) -> Entity {
        let mob = world
            .spawn((
                Health::default(),
                Stat::Conscious,
                OrganHost,
                pos,
                Name::new("Иван Иванов"),
            ))
            .id();
        for slot in Slot::ALL {
            let organ = spawn_organ(world, slot.spec().path).unwrap();
            world.entity_mut(organ).insert(OrganOf(mob));
        }
        mob
    }

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Players>();
        world
    }

    fn give_client(world: &mut World, mob: Entity) -> Entity {
        let client = world.spawn_empty().id();
        world
            .entity_mut(mob)
            .insert((PlayerMob { client }, MoveQueue::default()));
        world.resource_mut::<Players>().set_mob(client, mob);
        client
    }

    #[test]
    fn the_mind_leaves_with_the_brain_and_enters_the_next_body() {
        let mut world = world();
        let pos = TilePos::new(5, 5, 1);
        let ivan = human(&mut world, pos);
        let client = give_client(&mut world, ivan);
        let brain = organs::organ(&world, ivan, Slot::Brain).unwrap();
        organs::remove(&mut world, brain, false);
        let Occupant(brainmob) = *world.get::<Occupant>(brain).expect("a brainmob");
        assert_eq!(world.resource::<Players>().mob(client), Some(brainmob));
        assert!(world.get::<PlayerMob>(ivan).is_none());
        assert_eq!(world.get::<Stat>(brainmob), Some(&Stat::Dead));
        assert_eq!(world.get::<Stat>(ivan), Some(&Stat::Dead));
        assert_eq!(
            world.get::<Name>(brainmob).map(Name::as_str),
            Some("Иван Иванов")
        );
        // A special removal (an organ swap) leaves the mind where it is.
        let other = human(&mut world, pos);
        let other_client = give_client(&mut world, other);
        let other_brain = organs::organ(&world, other, Slot::Brain).unwrap();
        organs::remove(&mut world, other_brain, true);
        assert!(world.get::<Occupant>(other_brain).is_none());
        assert_eq!(world.resource::<Players>().mob(other_client), Some(other));
        // Into a body without a brain: the player controls it, the brainmob is gone.
        organs::insert(&mut world, brain, other);
        assert_eq!(world.resource::<Players>().mob(client), Some(other));
        assert_eq!(
            world.get::<PlayerMob>(other).map(|p| p.client),
            Some(client)
        );
        assert!(world.get_entity(brainmob).is_err());
        assert!(world.get::<Occupant>(brain).is_none());
    }

    #[test]
    fn a_ghosted_mind_follows_its_brain() {
        let mut world = world();
        let pos = TilePos::new(5, 5, 1);
        let ivan = human(&mut world, pos);
        let ghost = world.spawn(Ghost { body: Some(ivan) }).id();
        let brain = organs::organ(&world, ivan, Slot::Brain).unwrap();
        organs::remove(&mut world, brain, false);
        let Occupant(brainmob) = *world.get::<Occupant>(brain).unwrap();
        assert_eq!(world.get::<Ghost>(ghost).unwrap().body, Some(brainmob));
        // A body without a mind gives none: no brainmob.
        let empty = human(&mut world, pos);
        let empty_brain = organs::organ(&world, empty, Slot::Brain).unwrap();
        organs::remove(&mut world, empty_brain, false);
        assert!(world.get::<Occupant>(empty_brain).is_none());
        // The deleted brain lets the ghost go.
        world.despawn(brain);
        follow_containers(&mut world);
        assert!(world.get_entity(brainmob).is_err());
        assert_eq!(world.get::<Ghost>(ghost).unwrap().body, None);
    }

    #[test]
    fn the_brainmob_stands_where_its_brain_is() {
        let mut world = world();
        let ivan = human(&mut world, TilePos::new(5, 5, 1));
        give_client(&mut world, ivan);
        let brain = organs::organ(&world, ivan, Slot::Brain).unwrap();
        organs::remove(&mut world, brain, false);
        let Occupant(brainmob) = *world.get::<Occupant>(brain).unwrap();
        let carrier = world.spawn(TilePos::new(9, 3, 1)).id();
        world
            .entity_mut(brain)
            .insert(crate::inventory::CarriedBy(carrier));
        follow_containers(&mut world);
        assert_eq!(world.get::<TilePos>(brainmob), Some(&TilePos::new(9, 3, 1)));
    }
}
