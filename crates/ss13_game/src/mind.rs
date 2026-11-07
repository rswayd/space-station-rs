//! The mind (server), after tg `/datum/mind` (`code/datums/mind/_mind.dm`, `initialization.dm`).
//!
//! - [`MindData`] is the player's mind on the mob it belongs to (tg `mind.current`): the key, the
//!   name, the death counters and the time of the last death. A mob that gets a client without
//!   a mind gets a new one (tg `mind_initialize` from `Login`); a ghost has none (its mind stays
//!   with the body it left, tg `get_ghost`).
//! - [`transfer`] is tg `transfer_to` for the mind's data: [`MindData`] and what lives on the
//!   mind in tg (addictions, skills, the traitor and blood brother datums and their antag HUD)
//!   move from the old mob to the new one, and every objective that targeted the old mob now
//!   targets the new one (tg objectives target minds). The client follows in
//!   `crate::medical::brainmob::transfer_mind` (brain removal and insertion, bitrunning) and in
//!   the AI shell deploy (`crate::silicon::ai_shell`). A client that only takes another mob
//!   (tg `key = ...`: ghost possession, imaginary friend, split personality) takes no mind.
//! - [`get_ghost`] / [`grab_ghost`]: the ghost of a body, also a DNR one when forced (tg
//!   `even_if_they_cant_reenter`), back into its body.
//! - [`witnessed_death`] / [`experienced_death`]: every human death seen (also the own death)
//!   lowers the mind's `desensitized_level` by 0.025 down to 0.1, which scales the `saw_death`
//!   moodlet (`crate::mood::send_death_moodlets`).
// shortcut: no `/datum/memory` list, no mind languages, no `original_character`, no observer
// transfer and no `antag_hud` alt appearance re-creation (the antag HUD is a window here and
// reopens with the body's HUDs); the other antag datums (changeling, vampire, cult, heretic,
// revolution, malf) stay on their body: their code keeps per-body state, move them here when it
// follows the mind.

use bevy::prelude::*;
use ss13_net::server::ClientToken;

use crate::antag::AntagHudData;
use crate::antag::brother::{Brother, BrotherTeam};
use crate::antag::objectives::{Objective, ObjectiveKind};
use crate::antag::traitor::Traitor;
use crate::death::TimeOfDeath;
use crate::ghost::Ghost;
use crate::server::PlayerMob;

/// `DESENSITIZED_MINIMUM`, `DESENSITIZED_REDUCTION_PER_DEATH`, `DESENSITIZED_THRESHOLD`.
pub const DESENSITIZED_MINIMUM: f32 = 0.1;
pub const DESENSITIZED_REDUCTION_PER_DEATH: f32 = 0.025;
pub const DESENSITIZED_THRESHOLD: f32 = 0.5;

/// tg `/datum/mind`, on its `current` mob.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MindData {
    /// `key`: the player's handshake token.
    pub key: String,
    /// `name`: the mob's name when the mind was made.
    pub name: String,
    /// `desensitized_level`: the multiplier of death moodlets.
    pub desensitized_level: f32,
    /// `deaths_witnessed`.
    pub deaths_witnessed: u32,
    /// `last_death`: server seconds of the last death of the mind's mob (`set_death_time`).
    pub last_death: Option<f64>,
}

impl MindData {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            desensitized_level: 1.0,
            deaths_witnessed: 0,
            last_death: None,
        }
    }
}

/// On a ghost that may not re-enter its body (tg `can_reenter_corpse = FALSE`, DNR): the body
/// whose mind it is, for a forced [`grab_ghost`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DnrBody(pub Entity);

pub struct MindPlugin;

impl Plugin for MindPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (mind_initialize, set_death_time));
    }
}

/// tg `mind_initialize` from `Login`: a mob with a client and no mind gets one.
fn mind_initialize(
    mobs: Query<(Entity, &PlayerMob, Option<&Name>), (Added<PlayerMob>, Without<MindData>)>,
    ghosts: Query<(), With<Ghost>>,
    tokens: Query<&ClientToken>,
    mut commands: Commands,
) {
    for (mob, player, name) in &mobs {
        if ghosts.contains(mob) {
            continue;
        }
        let key = tokens
            .get(player.client)
            .map(|t| t.0.clone())
            .unwrap_or_default();
        let name = name.map(|n| n.to_string()).unwrap_or_default();
        commands
            .entity(mob)
            .try_insert_if_new(MindData::new(key, name));
    }
}

/// tg `set_death_time` on `COMSIG_LIVING_DEATH` of the mind's mob.
fn set_death_time(mut minds: Query<(&TimeOfDeath, &mut MindData), Added<TimeOfDeath>>) {
    for (died, mut mind) in &mut minds {
        mind.last_death = Some(died.0);
    }
}

fn move_component<T: Component>(world: &mut World, from: Entity, to: Entity) {
    let Ok(mut old) = world.get_entity_mut(from) else {
        return;
    };
    if let Some(c) = old.take::<T>()
        && let Ok(mut new) = world.get_entity_mut(to)
    {
        new.insert(c);
    }
}

fn retarget(objectives: &mut [Objective], from: Entity, to: Entity) {
    for o in objectives {
        if let ObjectiveKind::Assassinate { target } | ObjectiveKind::Maroon { target } =
            &mut o.kind
            && *target == Some(from)
        {
            *target = Some(to);
        }
    }
}

/// tg `transfer_to` for the mind's data: what the mind of `from` holds moves to `to` (whose own
/// mind, if any, is left without a body: tg `new_character.mind.set_current(null)`), and the
/// objectives targeting `from` target `to`. Nothing when `from` has no mind or is `to`.
pub fn transfer(world: &mut World, from: Entity, to: Entity) {
    if from == to || world.get::<MindData>(from).is_none() || world.get_entity(to).is_err() {
        return;
    }
    if let Ok(mut new) = world.get_entity_mut(to) {
        new.remove::<MindData>();
    }
    move_component::<MindData>(world, from, to);
    move_component::<crate::addictions::Addictions>(world, from, to);
    move_component::<crate::skills::Skills>(world, from, to);
    move_component::<Traitor>(world, from, to);
    move_component::<Brother>(world, from, to);
    move_component::<AntagHudData>(world, from, to);
    // Cost: one pass over the traitors and brother teams per transfer (rare).
    for mut traitor in world.query::<&mut Traitor>().iter_mut(world) {
        retarget(&mut traitor.objectives, from, to);
    }
    for mut brother in world.query::<&mut Brother>().iter_mut(world) {
        retarget(&mut brother.objectives, from, to);
    }
    for mut team in world.query::<&mut BrotherTeam>().iter_mut(world) {
        retarget(&mut team.objectives, from, to);
        for member in team.members.iter_mut().filter(|m| **m == from) {
            *member = to;
        }
    }
    info!("mind of {from} transferred to {to}");
}

/// tg `get_ghost(even_if_they_cant_reenter)`: the ghost that left `body`; with `force` also the
/// ghost that gave up on it (DNR).
pub fn get_ghost(world: &mut World, body: Entity, force: bool) -> Option<Entity> {
    if let Some(ghost) = crate::medical::brainmob::ghost_of(world, body) {
        return Some(ghost);
    }
    if !force {
        return None;
    }
    world
        .query::<(Entity, &DnrBody)>()
        .iter(world)
        .find(|(_, d)| d.0 == body)
        .map(|(e, _)| e)
}

/// tg `grab_ghost(force)`: the ghost of `body` (with `force` also a DNR one) re-enters it.
/// Returns the ghost that went back.
pub fn grab_ghost(world: &mut World, body: Entity, force: bool) -> Option<Entity> {
    let ghost = get_ghost(world, body, force)?;
    world.get::<PlayerMob>(ghost)?;
    if let Some(mut g) = world.get_mut::<Ghost>(ghost) {
        g.body = Some(body);
    }
    if let Ok(mut e) = world.get_entity_mut(ghost) {
        e.remove::<DnrBody>();
    }
    crate::ghost::return_to_body(world, body);
    world.get_entity(ghost).is_err().then_some(ghost)
}

/// tg `witnessed_death`: `viewer`'s mind saw the human `dead` die.
pub fn witnessed_death(world: &mut World, viewer: Entity, dead: Entity) {
    if world.get::<crate::human::HumanLook>(dead).is_none() {
        return;
    }
    let Some(mut mind) = world.get_mut::<MindData>(viewer) else {
        return;
    };
    mind.desensitized_level =
        (mind.desensitized_level - DESENSITIZED_REDUCTION_PER_DEATH).max(DESENSITIZED_MINIMUM);
    mind.deaths_witnessed += 1;
    // shortcut: no achievements, so no `desensitized` award.
}

/// tg `experienced_death`: the own death desensitizes too.
pub fn experienced_death(world: &mut World, mob: Entity) {
    witnessed_death(world, mob, mob);
}

/// The `saw_death` multiplier of `viewer`'s mind (`max(DESENSITIZED_MINIMUM, level || 1)`).
pub fn desensitized_level(world: &World, viewer: Entity) -> f32 {
    world
        .get::<MindData>(viewer)
        .map_or(1.0, |m| m.desensitized_level.max(DESENSITIZED_MINIMUM))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_moves_the_mind_and_retargets_objectives() {
        let mut world = World::new();
        let old = world.spawn(MindData::new("p1", "Bob")).id();
        let new = world.spawn(MindData::new("", "brainmob")).id();
        crate::addictions::add_points(
            &mut world,
            old,
            crate::addictions::Addiction::Nicotine,
            700.0,
        );
        let hunter = world
            .spawn(Traitor {
                key: "p2".into(),
                employer: String::new(),
                flavor: Default::default(),
                objectives: vec![Objective {
                    kind: ObjectiveKind::Assassinate { target: Some(old) },
                    explanation: String::new(),
                }],
                ending: ObjectiveKind::Escape,
                uplink: None,
                code: None,
                unlock_text: String::new(),
                memory: String::new(),
                failsafe_code: None,
                can_assign_self_objectives: false,
            })
            .id();
        transfer(&mut world, old, new);
        assert!(world.get::<MindData>(old).is_none());
        assert_eq!(world.get::<MindData>(new).unwrap().key, "p1");
        assert_eq!(
            world
                .get::<crate::addictions::Addictions>(new)
                .unwrap()
                .points(crate::addictions::Addiction::Nicotine),
            700.0
        );
        assert!(world.get::<crate::addictions::Addictions>(old).is_none());
        assert_eq!(
            world.get::<Traitor>(hunter).unwrap().objectives[0].kind,
            ObjectiveKind::Assassinate { target: Some(new) }
        );
        // No mind, nothing moves.
        let other = world.spawn_empty().id();
        transfer(&mut world, old, other);
        assert!(world.get::<MindData>(new).is_some());
    }

    fn human() -> crate::human::HumanLook {
        crate::human::player_body(Entity::PLACEHOLDER, None)
    }

    #[test]
    fn deaths_desensitize_down_to_the_minimum() {
        let mut world = World::new();
        let viewer = world.spawn((MindData::new("p1", "A"), human())).id();
        let dead = world.spawn(human()).id();
        let monkey = world.spawn_empty().id();
        witnessed_death(&mut world, viewer, monkey);
        assert_eq!(world.get::<MindData>(viewer).unwrap().deaths_witnessed, 0);
        for _ in 0..40 {
            witnessed_death(&mut world, viewer, dead);
        }
        experienced_death(&mut world, viewer);
        let mind = world.get::<MindData>(viewer).unwrap();
        assert_eq!(mind.deaths_witnessed, 41);
        assert_eq!(mind.desensitized_level, DESENSITIZED_MINIMUM);
        assert_eq!(desensitized_level(&world, dead), 1.0);
    }
}
