//! Imaginary friend (server), after tg `code/datums/brain_damage/imaginary_friend.dm`
//! (`/datum/brain_trauma/special/imaginary_friend`, `/mob/eye/imaginary_friend`) with
//! BandaStation's edits (the friend is made only once a ghost accepts; it walks like a person).
//!
//! - The trauma ([`crate::medical::traumas::Kind::ImaginaryFriend`]) takes only in a living body
//!   with a player. Its body gets [`FriendHost`] ([`sync`] from the traumas' `refresh`) and every
//!   ghost is polled (an alert, 20 s). Nobody: the trauma goes. The first ghost to accept
//!   becomes the friend ([`ImaginaryFriend`]): a mob nobody's `see_invisible` reaches, shown
//!   only to its owner and itself (`VisibleToMobs`, tg's `client.images`), looking like the
//!   owner. What it says and emotes is heard only by them (`ss13_chat` passes an invisible
//!   speaker's words to the mobs it shows itself to); it hears and sees what is around.
//! - It walks like a person (BandaStation: dense things block it) and is pulled back to its
//!   owner past 9 tiles or off the owner's z ([`watch`], tg `recall`). Its buttons: «Join»
//!   (back to the owner) and «Hide» (invisible to the owner, half-transparent to itself).
//! - The owner dies: the trauma goes (`on_death`). The friend's player leaves: a minute later
//!   a new poll (`reroll_friend`). The cure: the friend goes, its player ghosts.
// shortcut: the friend copies its owner's look (no BandaStation appearance choice — own
// character, random human, a copy of a nearby carbon — no flat-human-icon renderer), a hidden
// friend does not phase through walls, no speech bubbles, typing indicators or pointing arrow,
// ghosts do not overhear it, no `trapped_owner` variant, no TTS, no jobban check.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::server::{Invisibility, Networked, VisibleToMobs};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Facing, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::UiActed;

use super::organs::{self, Slot};
use super::traumas::{self, Kind};
use crate::actions::{ActionButton, ActionButtonsUi, set_buttons};
use crate::alt_appearance::{Seers, add_alt_appearance, remove_alt_appearance};
use crate::ghost::Ghost;
use crate::health::Stat;
use crate::server::PlayerMob;

/// `poll_time`.
pub const POLL_SECS: f64 = 20.0;
/// `distance_allowance` (`IMAGINARY_FRIEND_RANGE`).
pub const RANGE: u32 = 9;
/// `reroll_friend`'s timer.
pub const REROLL_SECS: f64 = 60.0;
/// BandaStation's `move_delay` of a visible friend: 2 deciseconds.
const MOVE_DELAY: f32 = 0.2;
/// An invisibility nobody's `see_invisible` reaches (`INVISIBILITY_MAXIMUM` and above).
const FRIEND_INVISIBILITY: u8 = u8::MAX;
/// The alternate appearances of a hidden friend.
const HIDE_KEY: &str = "imaginary_hide";
const HIDE_SELF_KEY: &str = "imaginary_hide_self";
/// The button ids.
pub const JOIN: &str = "imaginary_join";
pub const HIDE: &str = "imaginary_hide";
const ACTIONS_DMI: &str = "icons/mob/actions/actions_minor_antag.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";

/// On a body with the trauma.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FriendHost {
    /// tg `friend`: none until a ghost accepts.
    pub friend: Option<Entity>,
    /// Game time the ghost poll ends; `None` once the friend is made.
    pub poll_until: Option<f64>,
    /// Game time of the `reroll_friend` timer (the friend's player left).
    pub reroll_at: Option<f64>,
}

/// `/mob/eye/imaginary_friend`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImaginaryFriend {
    pub owner: Entity,
    pub hidden: bool,
}

/// On a poll prompt: the body whose friend it asks for.
#[derive(Component, Clone, Copy, Debug)]
struct FriendPoll {
    body: Entity,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            answers.after(ss13_tgui::prompt::PromptSystems::Answer),
            buttons.run_if(resource_exists::<Messages<UiActed<ActionButtonsUi>>>),
            watch.run_if(any_with_component::<FriendHost>),
        )
            .chain()
            .in_set(ss13_tgui::server::UiSystems::Handle),
    );
}

/// `on_gain` returning FALSE: a dead body or one without a player.
pub fn can_take(world: &World, mob: Entity) -> bool {
    super::split_personality::can_take(world, mob)
}

/// Sets up or tears down the friend of `mob` for whether its brain has the trauma.
pub fn sync(world: &mut World, mob: Entity, wanted: bool) {
    let has = world.get::<FriendHost>(mob).is_some();
    if wanted && !has && can_take(world, mob) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        world.entity_mut(mob).insert(FriendHost {
            friend: None,
            poll_until: Some(now + POLL_SECS),
            reroll_at: None,
        });
        poll(world, mob);
    } else if !wanted && has {
        teardown(world, mob);
    }
}

/// `get_ghost`: every ghost is asked.
fn poll(world: &mut World, mob: Entity) {
    let name = super::name_of(world, mob);
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
            "Воображаемый друг",
            format!("Do you want to play as {name}'s imaginary friend?"),
            PromptKind::Alert {
                buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                autofocus: true,
            },
        )
        .timeout(POLL_SECS);
        let prompt = ask_world(world, ask);
        world.entity_mut(prompt).insert(FriendPoll { body: mob });
    }
    info!("{mob} got an imaginary friend; polling ghosts");
}

/// `on_lose`: `QDEL_NULL(friend)`, its player ghosts.
fn teardown(world: &mut World, mob: Entity) {
    let Some(host) = world.get::<FriendHost>(mob).copied() else {
        return;
    };
    world.entity_mut(mob).remove::<FriendHost>();
    if let Some(friend) = host.friend {
        despawn_friend(world, friend);
    }
}

fn despawn_friend(world: &mut World, friend: Entity) {
    if world.get_entity(friend).is_err() {
        return;
    }
    if let Some(client) = world.get::<PlayerMob>(friend).map(|p| p.client)
        && let Some(ghost) = crate::ghost::ghostize_client(world, client)
        && let Some(mut g) = world.get_mut::<Ghost>(ghost)
    {
        // The friend is gone: nothing to go back to.
        g.body = None;
    }
    world.despawn(friend);
}

/// Takes the trauma out of `mob`'s brain (`qdel(src)`).
fn cure(world: &mut World, mob: Entity) {
    if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
        traumas::remove_kind(world, brain, Kind::ImaginaryFriend);
    }
    teardown(world, mob);
}

/// `add_friend(ghost)` -> `make_friend`, `PossessByPlayer`, `attach_to_owner`: `client` leaves
/// `ghost` for a new friend of `owner`. The friend, if made.
pub fn make_friend(
    world: &mut World,
    owner: Entity,
    client: Entity,
    ghost: Entity,
) -> Option<Entity> {
    let &pos = world.get::<TilePos>(owner)?;
    let facing = world.get::<Facing>(owner).copied().unwrap_or_default();
    let appearance = world.get::<Appearance>(owner).cloned();
    let languages = world
        .get::<ss13_chat::language::LanguageHolder>(owner)
        .cloned();
    let mut friend = world.spawn((
        ImaginaryFriend {
            owner,
            hidden: false,
        },
        Name::new("воображаемый друг"),
        pos,
        facing,
        Networked,
        Invisibility(FRIEND_INVISIBILITY),
        ss13_world::Mover::new(MOVE_DELAY),
    ));
    if let Some(appearance) = appearance {
        friend.insert(appearance);
    }
    // `get_selected_language()` of the owner: it speaks what its owner speaks.
    if let Some(languages) = languages {
        friend.insert(languages);
    }
    let friend = friend.id();
    world
        .entity_mut(friend)
        .insert(VisibleToMobs(vec![owner, friend]));
    if !super::brainmob::possess(world, client, ghost, friend) {
        world.despawn(friend);
        return None;
    }
    world.despawn(ghost);
    if let Some(mut host) = world.get_mut::<FriendHost>(owner) {
        host.friend = Some(friend);
        host.poll_until = None;
        host.reroll_at = None;
    }
    refresh_buttons(world, friend);
    let name = super::name_of(world, owner);
    // `greet()`.
    for line in [
        format!("<b>Вы - воображаемый друг {name}!</b>"),
        "Вы абсолютно преданы своему другу, несмотря ни на что.".to_owned(),
        format!(
            "Вы не можете напрямую влиять на окружающий вас мир, но вы можете видеть то, чего не может видеть {name}."
        ),
    ] {
        super::diseases::say_to(world, friend, Span::Notice, line);
    }
    info!("{client} became {owner}'s imaginary friend {friend}");
    Some(friend)
}

/// The friend's «Join» and «Hide»/«Show» buttons.
fn refresh_buttons(world: &mut World, friend: Entity) {
    let Some(state) = world.get::<ImaginaryFriend>(friend).copied() else {
        return;
    };
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let button = |world: &World, id: &str, name: &str, state: &str| ActionButton {
        id: id.to_owned(),
        name: name.to_owned(),
        icon: icon(world, ACTIONS_DMI, state),
        background: icon(world, BACKGROUNDS_DMI, "bg_revenant"),
        border: icon(world, BACKGROUNDS_DMI, "bg_revenant_border"),
        available: true,
        cooldown: 0.0,
        active: false,
        maptext: None,
    };
    let (hide_name, hide_state) = if state.hidden {
        ("Show", "unhide")
    } else {
        ("Hide", "hide")
    };
    let buttons = vec![
        (button(&*world, JOIN, "Join", "join"), 0.0),
        (button(&*world, HIDE, hide_name, hide_state), 0.0),
    ];
    set_buttons(world, friend, buttons);
}

/// `recall()`: to the owner's tile.
pub fn recall(world: &mut World, friend: Entity) {
    let Some(owner) = world.get::<ImaginaryFriend>(friend).map(|f| f.owner) else {
        return;
    };
    let Some(pos) = crate::explosions::turf_of(world, owner) else {
        return;
    };
    if world.get::<TilePos>(friend) != Some(&pos) {
        world.entity_mut(friend).insert(pos);
    }
}

/// `/datum/action/innate/imaginary_hide/Activate`: hidden from the owner, half-transparent to
/// itself (tg `current_image.alpha = 150`, not added to the group's images).
pub fn toggle_hide(world: &mut World, friend: Entity) {
    let Some(mut state) = world.get::<ImaginaryFriend>(friend).copied() else {
        return;
    };
    state.hidden = !state.hidden;
    world.entity_mut(friend).insert(state);
    remove_alt_appearance(world, friend, HIDE_KEY);
    remove_alt_appearance(world, friend, HIDE_SELF_KEY);
    if state.hidden {
        if let Some(look) = world.get::<Appearance>(friend).cloned() {
            let tint = |alpha: u8| Appearance {
                color: Some([255, 255, 255, alpha]),
                ..look.clone()
            };
            add_alt_appearance(world, friend, HIDE_KEY, Seers::mob(state.owner), tint(0));
            add_alt_appearance(world, friend, HIDE_SELF_KEY, Seers::mob(friend), tint(150));
        }
        super::diseases::say_to(
            world,
            friend,
            Span::Notice,
            "Вы прячетесь. Теперь вы бестелесны и можете проходить сквозь стены.",
        );
    } else {
        super::diseases::say_to(
            world,
            friend,
            Span::Notice,
            "Вы появляетесь на виду и теперь двигаетесь как обычный человек.",
        );
    }
    refresh_buttons(world, friend);
}

/// A click on a friend's button, checked at the moment of effect: the client controls the
/// friend.
fn buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act == JOIN || a.act == HIDE)
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<ImaginaryFriend>(actor).is_none()
        {
            continue;
        }
        if id == JOIN {
            recall(world, actor);
        } else {
            toggle_hide(world, actor);
        }
    }
}

/// The ended poll, the dead owner, the friend too far or without its player (`on_life`).
// Cost: one pass over the hosts (a handful a round at most) per tick.
fn watch(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let hosts: Vec<(Entity, FriendHost)> = world
        .query::<(Entity, &FriendHost)>()
        .iter(world)
        .map(|(e, h)| (e, *h))
        .collect();
    for (mob, host) in hosts {
        // `on_death`: the friend goes down with the ship.
        if world.get::<Stat>(mob).is_none_or(|&s| s == Stat::Dead) {
            cure(world, mob);
            continue;
        }
        let Some(friend) = host.friend else {
            if host.poll_until.is_some_and(|until| now >= until) {
                // `add_friend(null)`.
                cure(world, mob);
            }
            continue;
        };
        if world.get_entity(friend).is_err() {
            // `if(!friend) qdel(src)`.
            cure(world, mob);
            continue;
        }
        let far = match (
            crate::explosions::turf_of(world, mob),
            world.get::<TilePos>(friend),
        ) {
            (Some(at), Some(&pos)) => at.chebyshev(pos).is_none_or(|d| d > RANGE),
            _ => false,
        };
        if far {
            recall(world, friend);
        }
        if world.get::<PlayerMob>(friend).is_some() {
            if host.reroll_at.is_some()
                && let Some(mut h) = world.get_mut::<FriendHost>(mob)
            {
                h.reroll_at = None;
            }
            continue;
        }
        match host.reroll_at {
            None => {
                if let Some(mut h) = world.get_mut::<FriendHost>(mob) {
                    h.reroll_at = Some(now + REROLL_SECS);
                }
            }
            // `reroll_friend`: a brand new friend.
            Some(at) if now >= at => {
                despawn_friend(world, friend);
                world.entity_mut(mob).insert(FriendHost {
                    friend: None,
                    poll_until: Some(now + POLL_SECS),
                    reroll_at: None,
                });
                poll(world, mob);
            }
            Some(_) => {}
        }
    }
}

/// The first ghost to accept becomes the friend.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
        return;
    };
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(messages)
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, ghost, answer) in pending {
        let Some(FriendPoll { body }) = world.get::<FriendPoll>(prompt).copied() else {
            continue;
        };
        if answer != Some(PromptAnswer::Button(0)) {
            continue;
        }
        // Checked again after the wait: the poll still open, still a ghost of this client.
        let open = world
            .get::<FriendHost>(body)
            .is_some_and(|h| h.friend.is_none() && h.poll_until.is_some());
        let still_ghost = world.get::<Ghost>(ghost).is_some()
            && world.get::<PlayerMob>(ghost).map(|p| p.client) == Some(client);
        if !open || !still_ghost || world.get::<Stat>(body) == Some(&Stat::Dead) {
            continue;
        }
        make_friend(world, body, client, ghost);
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

    fn human(world: &mut World) -> Entity {
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
        mob
    }

    fn ghost(world: &mut World) -> (Entity, Entity) {
        let client = world.spawn_empty().id();
        let ghost = world
            .spawn((
                Ghost { body: None },
                PlayerMob { client },
                MoveQueue::default(),
                TilePos::new(10, 10, 1),
            ))
            .id();
        world.resource_mut::<Players>().set_mob(client, ghost);
        (ghost, client)
    }

    #[test]
    fn a_ghost_becomes_a_friend_only_its_owner_sees() {
        let mut world = world();
        let mob = human(&mut world);
        let brain = organs::organ(&world, mob, Slot::Brain).unwrap();
        assert!(traumas::gain(
            &mut world,
            brain,
            Kind::ImaginaryFriend,
            None
        ));
        let host = *world.get::<FriendHost>(mob).expect("polling");
        assert_eq!(host.friend, None);
        let (ghost, client) = ghost(&mut world);
        let friend = make_friend(&mut world, mob, client, ghost).expect("friend");
        assert_eq!(world.resource::<Players>().mob(client), Some(friend));
        assert!(world.get_entity(ghost).is_err());
        assert_eq!(
            world.get::<VisibleToMobs>(friend),
            Some(&VisibleToMobs(vec![mob, friend]))
        );
        assert_eq!(
            world.get::<Invisibility>(friend),
            Some(&Invisibility(FRIEND_INVISIBILITY))
        );
        assert_eq!(world.get::<FriendHost>(mob).unwrap().friend, Some(friend));
        // Too far: pulled back to the owner.
        world.entity_mut(friend).insert(TilePos::new(30, 3, 1));
        watch(&mut world);
        assert_eq!(world.get::<TilePos>(friend), Some(&TilePos::new(3, 3, 1)));
        // Hidden from the owner, half-transparent to itself.
        toggle_hide(&mut world, friend);
        assert!(world.get::<ImaginaryFriend>(friend).unwrap().hidden);
        // The cure: the friend is gone, its player a ghost again.
        traumas::cure_all(&mut world, brain, traumas::Resilience::Magic);
        assert!(world.get_entity(friend).is_err());
        assert!(world.get::<FriendHost>(mob).is_none());
        let now = world.resource::<Players>().mob(client).unwrap();
        assert!(world.get::<Ghost>(now).is_some());
    }

    #[test]
    fn nobody_answers_or_the_owner_dies_and_the_trauma_goes() {
        let mut world = world();
        let mob = human(&mut world);
        let brain = organs::organ(&world, mob, Slot::Brain).unwrap();
        assert!(traumas::gain(
            &mut world,
            brain,
            Kind::ImaginaryFriend,
            None
        ));
        world.get_mut::<FriendHost>(mob).unwrap().poll_until = Some(-1.0);
        watch(&mut world);
        assert!(!traumas::has(&world, mob, Kind::ImaginaryFriend));
        assert!(world.get::<FriendHost>(mob).is_none());
        // No player: no friend at all.
        world.entity_mut(mob).remove::<PlayerMob>();
        assert!(!traumas::gain(
            &mut world,
            brain,
            Kind::ImaginaryFriend,
            None
        ));
    }
}
