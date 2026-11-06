//! Split personality (server), after tg `code/datums/brain_damage/split_personality.dm`
//! (`/datum/brain_trauma/severe/split_personality`, `/mob/living/split_personality`).
//!
//! - The trauma ([`crate::medical::traumas::Kind::SplitPersonality`]) takes only in a living
//!   body with a player. Its body gets [`SplitPersonality`] ([`sync`] from the traumas'
//!   `refresh`): two backseat mobs ([`Backseat`], brainmobs standing in the body: they see and
//!   hear from it, cannot speak) and a poll of every ghost (an alert, 20 s). The first ghost to
//!   accept becomes the stranger in its backseat; nobody: the trauma goes.
//! - Now and then (1.5% a second, the traumas' `on_life`) the personalities switch
//!   ([`switch`]): the one in control goes to its backseat, the other takes the body. Never to
//!   a backseat without a player.
//! - The body dies: the owner gets it back and the trauma goes. The one in control leaves the
//!   body (ghosts, logs out): the other takes it for good and the trauma goes. The cure (or the
//!   brain taken out) gives the owner the body back; the stranger ghosts.
// shortcut: no `personality_commune` action, no brainwashing or blackout variants, no
// computer id / IP swap, no `TRAIT_NO_SPLIT_PERSONALITY` or deathmatch check, no jobban check;
// the body keeps its antagonist and job components whoever controls it (tg keeps them on the
// mind); backseats can still emote.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use super::brainmob::{self, BrainMob};
use super::organs::{self, Slot};
use super::traumas::{self, Kind};
use crate::ghost::Ghost;
use crate::health::Stat;
use crate::server::PlayerMob;

/// `poll_time`.
pub const POLL_SECS: f64 = 20.0;
/// `poll_role`.
const POLL_ROLE: &str = "split personality";

/// On a body with the trauma: its two backseats and who is in control.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SplitPersonality {
    pub owner_seat: Entity,
    pub stranger_seat: Entity,
    /// `current_controller == STRANGER`.
    pub stranger_controls: bool,
    /// Game time the ghost poll ends.
    pub poll_until: f64,
    /// A ghost took the stranger's seat (tg `initialized`).
    pub initialized: bool,
}

/// `/mob/living/split_personality`: a backseat in `body`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backseat {
    pub body: Entity,
}

/// On a poll prompt: the body whose stranger it asks for.
#[derive(Component, Clone, Copy, Debug)]
struct SplitPoll {
    body: Entity,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            answers.after(ss13_tgui::prompt::PromptSystems::Answer),
            watch.run_if(any_with_component::<SplitPersonality>),
        )
            .chain()
            .in_set(ss13_tgui::server::UiSystems::Handle),
    );
}

/// Whether the trauma can take in `mob` now (`on_gain`): alive, with a player.
pub fn can_take(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob).is_some_and(|&s| s != Stat::Dead)
        && world.get::<PlayerMob>(mob).is_some()
}

/// Sets up or tears down the backseats of `mob` for whether its brain has the trauma.
pub fn sync(world: &mut World, mob: Entity, wanted: bool) {
    let has = world.get::<SplitPersonality>(mob).is_some();
    if wanted && !has && can_take(world, mob) {
        setup(world, mob);
    } else if !wanted && has {
        teardown(world, mob);
    }
}

/// `make_backseats` and `get_ghost`.
fn setup(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = super::name_of(world, mob);
    let mut seat = || {
        world
            .spawn((
                BrainMob { container: mob },
                Backseat { body: mob },
                Name::new(name.clone()),
                Stat::Conscious,
                pos,
                // `try_speak`: "You cannot speak, your other self is controlling your body!"
                ss13_chat::speech::Impediments {
                    mute: true,
                    ..default()
                },
            ))
            .id()
    };
    let (stranger_seat, owner_seat) = (seat(), seat());
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(SplitPersonality {
        owner_seat,
        stranger_seat,
        stranger_controls: false,
        poll_until: now + POLL_SECS,
        initialized: false,
    });
    let mut ghosts: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &PlayerMob), With<Ghost>>()
        .iter(world)
        .map(|(g, p)| (g, p.client))
        .collect();
    ghosts.sort();
    for (ghost, client) in ghosts {
        let ask = Ask::new(
            client,
            ghost,
            "Раздвоение личности",
            format!("Do you want to play as {name}'s {POLL_ROLE}?"),
            PromptKind::Alert {
                buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                autofocus: true,
            },
        )
        .timeout(POLL_SECS);
        let prompt = ask_world(world, ask);
        world.entity_mut(prompt).insert(SplitPoll { body: mob });
    }
    info!("{mob} got a split personality; polling ghosts");
}

/// `on_lose`: the owner back in control, the backseats gone (their players ghost).
fn teardown(world: &mut World, mob: Entity) {
    let Some(split) = world.get::<SplitPersonality>(mob).copied() else {
        return;
    };
    if split.stranger_controls {
        switch(world, mob, true);
    }
    if let Ok(mut entity) = world.get_entity_mut(mob) {
        entity.remove::<SplitPersonality>();
    }
    for seat in [split.owner_seat, split.stranger_seat] {
        if world.get_entity(seat).is_ok() {
            brainmob::despawn_brainmob(world, seat);
        }
    }
}

fn tell(world: &mut World, mob: Entity, span: Span, text: &str) {
    super::diseases::say_to(world, mob, span, text);
}

/// `switch_personalities(reset_to_owner)`: the one in control to its backseat, the other into
/// the body. Nothing when the other's backseat has no player. True if it switched.
pub fn switch(world: &mut World, mob: Entity, reset_to_owner: bool) -> bool {
    let Some(mut split) = world.get::<SplitPersonality>(mob).copied() else {
        return false;
    };
    let (current_backseat, new_backseat) = if split.stranger_controls || reset_to_owner {
        (split.owner_seat, split.stranger_seat)
    } else {
        (split.stranger_seat, split.owner_seat)
    };
    let Some(incoming) = world.get::<PlayerMob>(current_backseat).map(|p| p.client) else {
        return false;
    };
    info!("{incoming} assumed control of {mob} (split personality)");
    tell(
        world,
        mob,
        Span::Danger,
        "You feel your control being taken away... your other personality is in charge now!",
    );
    tell(
        world,
        current_backseat,
        Span::Danger,
        "You manage to take control of your body!",
    );
    // Body to backseat.
    if let Some(outgoing) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        brainmob::possess(world, outgoing, mob, new_backseat);
        if reset_to_owner {
            crate::ghost::ghostize_client(world, outgoing);
        }
    }
    // Backseat to body.
    brainmob::possess(world, incoming, current_backseat, mob);
    split.stranger_controls = !split.stranger_controls;
    world.entity_mut(mob).insert(split);
    true
}

/// The traumas' `on_life`: 1.5% a second to switch.
pub fn on_life(world: &mut World, mob: Entity, secs: f32, rng: &mut crate::combat::CombatRng) {
    if super::wounds::spt_prob(rng, 1.5, secs) {
        switch(world, mob, false);
    }
}

/// Takes the trauma out of `mob`'s brain (`qdel(src)`).
fn cure(world: &mut World, mob: Entity) {
    if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
        traumas::remove_kind(world, brain, Kind::SplitPersonality);
    }
    // A brain that is gone or moved: the backseats go anyway.
    teardown(world, mob);
}

/// The dead body, the ended poll and the controller leaving (`on_life`, the backseat `Life`).
// Cost: one pass over the split bodies (a handful a round at most) per tick.
fn watch(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let bodies: Vec<(Entity, SplitPersonality)> = world
        .query::<(Entity, &SplitPersonality)>()
        .iter(world)
        .map(|(e, s)| (e, *s))
        .collect();
    for (mob, split) in bodies {
        let seats_gone = world.get_entity(split.owner_seat).is_err()
            || world.get_entity(split.stranger_seat).is_err();
        if seats_gone || world.get::<Stat>(mob) == Some(&Stat::Dead) {
            // `if(owner.stat == DEAD)`: back to the owner, then gone.
            cure(world, mob);
            continue;
        }
        if !split.initialized {
            if now >= split.poll_until {
                // `schism(null)`.
                cure(world, mob);
            }
            continue;
        }
        if world.get::<PlayerMob>(mob).is_none() {
            // `if(!body.client && trauma.initialized)`: the other one stays for good.
            switch(world, mob, false);
            if let Some(mut s) = world.get_mut::<SplitPersonality>(mob) {
                // The one now in control keeps the body: no switch back on the cure.
                s.stranger_controls = false;
            }
            cure(world, mob);
        }
    }
}

/// `schism(ghost)`: the first ghost to accept becomes the stranger.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
        return;
    };
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(messages)
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, ghost, answer) in pending {
        let Some(SplitPoll { body }) = world.get::<SplitPoll>(prompt).copied() else {
            continue;
        };
        if answer != Some(PromptAnswer::Button(0)) {
            continue;
        }
        // Checked again after the wait: still a ghost, still the client's, the seat still free.
        let Some(split) = world.get::<SplitPersonality>(body).copied() else {
            continue;
        };
        let still_ghost = world.get::<Ghost>(ghost).is_some()
            && world.get::<PlayerMob>(ghost).map(|p| p.client) == Some(client);
        if split.initialized
            || !still_ghost
            || world.get::<PlayerMob>(split.stranger_seat).is_some()
            || world.get_entity(split.stranger_seat).is_err()
        {
            continue;
        }
        if !brainmob::possess(world, client, ghost, split.stranger_seat) {
            continue;
        }
        world.despawn(ghost);
        if let Some(mut s) = world.get_mut::<SplitPersonality>(body) {
            s.initialized = true;
        }
        info!("{client} became {body}'s split personality");
        // `Login()`.
        let seat = split.stranger_seat;
        tell(
            world,
            seat,
            Span::Notice,
            "As a split personality, you cannot do anything but observe. However, you will \
             eventually gain control of your body, switching places with the current \
             personality.",
        );
        tell(
            world,
            seat,
            Span::BoldWarning,
            "Do not commit suicide or put the body in a deadly position. Behave like you care \
             about it as much as the owner.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::Health;
    use crate::medical::organs::{OrganHost, OrganOf, spawn_organ};
    use crate::server::{MoveQueue, Players};

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Players>();
        world
    }

    fn human(world: &mut World) -> (Entity, Entity) {
        let mob = world
            .spawn((
                Health::default(),
                Stat::Conscious,
                OrganHost,
                TilePos::new(3, 3, 1),
                Name::new("Иван Иванов"),
            ))
            .id();
        for slot in Slot::ALL {
            let organ = spawn_organ(world, slot.spec().path).unwrap();
            world.entity_mut(organ).insert(OrganOf(mob));
        }
        let client = world.spawn_empty().id();
        world
            .entity_mut(mob)
            .insert((PlayerMob { client }, MoveQueue::default()));
        world.resource_mut::<Players>().set_mob(client, mob);
        (mob, client)
    }

    #[test]
    fn personalities_switch_and_the_cure_gives_the_body_back() {
        let mut world = world();
        let (mob, owner) = human(&mut world);
        let brain = organs::organ(&world, mob, Slot::Brain).unwrap();
        assert!(traumas::gain(
            &mut world,
            brain,
            Kind::SplitPersonality,
            None
        ));
        let split = *world.get::<SplitPersonality>(mob).expect("backseats");
        // No stranger yet: no switch.
        assert!(!switch(&mut world, mob, false));
        // A ghost takes the stranger's seat.
        let stranger = world.spawn_empty().id();
        world
            .entity_mut(split.stranger_seat)
            .insert((PlayerMob { client: stranger }, MoveQueue::default()));
        world
            .resource_mut::<Players>()
            .set_mob(stranger, split.stranger_seat);
        world.get_mut::<SplitPersonality>(mob).unwrap().initialized = true;
        assert!(switch(&mut world, mob, false));
        assert_eq!(world.resource::<Players>().mob(stranger), Some(mob));
        assert_eq!(
            world.resource::<Players>().mob(owner),
            Some(split.owner_seat)
        );
        assert!(
            world
                .get::<SplitPersonality>(mob)
                .unwrap()
                .stranger_controls
        );
        // Brain surgery: the owner is back, the backseats are gone.
        traumas::cure_all(&mut world, brain, traumas::Resilience::Surgery);
        assert_eq!(world.resource::<Players>().mob(owner), Some(mob));
        assert!(world.get::<SplitPersonality>(mob).is_none());
        assert!(world.get_entity(split.owner_seat).is_err());
        assert!(world.get_entity(split.stranger_seat).is_err());
    }

    #[test]
    fn no_player_no_split() {
        let mut world = world();
        let (mob, _) = human(&mut world);
        world.entity_mut(mob).remove::<PlayerMob>();
        let brain = organs::organ(&world, mob, Slot::Brain).unwrap();
        assert!(!traumas::gain(
            &mut world,
            brain,
            Kind::SplitPersonality,
            None
        ));
        assert!(world.get::<SplitPersonality>(mob).is_none());
    }
}
