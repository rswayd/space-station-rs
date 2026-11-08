//! Polling players for a role, after tg `SSpolling` (`code/controllers/subsystem/polling.dm`,
//! `code/datums/candidate_poll.dm`).
//!
//! [`poll_candidates`] (`poll_candidates`, `poll_ghost_candidates`) asks every eligible mob of a
//! group at once with a timed window and calls back when the time is up with whoever signed up
//! and is still there: a candidate whose player left, or who is no longer in that mob, is
//! dropped at the moment of the callback (`trim_candidates`), as is everyone when the poll's
//! target is gone (`poll_ghosts_for_target`). Nobody eligible calls back at once. tg's proc
//! sleeps until the end; here the caller's continuation is the callback.
//!
//! "Не в этом раунде" (`do_never_for_this_round`, `GLOB.poll_ignore`) bars the player's key from
//! the polls of that ignore category until the round ends.
// shortcut: a tgui alert window instead of tg's stacking screen alert (`poll_alert`: no stacks
// or candidate counter, no jump to the event, no chat line or prompt sound, an answer cannot be
// changed afterwards), no `amount_to_pick` (callers pick from the result, [`pick_candidate`]),
// no sign-up inheritance between identical polls, no `announce_chosen` and no job bans or
// `ghost_roles` preferences (the port has neither); add them with such preferences and bans.

use std::collections::{HashMap, HashSet};

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_net::server::ClientToken;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::UiSystems;

use crate::ghost::Ghost;
use crate::server::PlayerMob;
use crate::tools::tell;

/// tg `poll_time`'s default and the `from_ghosts` rulesets' one.
pub const DEFAULT_POLL_SECS: f64 = 30.0;
/// `POLL_IGNORE_BLOB`.
pub const POLL_IGNORE_BLOB: &str = "blob";

const SIGN_UP: usize = 0;
const NEVER: usize = 2;
/// With `start_signed_up`: "Участвовать", "Отписаться".
const OPT_OUT: usize = 1;

/// A player polled in a mob; it stands as long as that mob is still the player's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub client: Entity,
    pub mob: Entity,
}

/// What a poll asks (the arguments of tg's `poll_candidates`).
#[derive(Clone, Debug, Default)]
pub struct PollSpec {
    pub question: String,
    /// `role_name_text`: the window's title and the sign-up messages.
    pub role_name: String,
    pub secs: f64,
    /// `ignore_category`: offers "Не в этом раунде".
    pub ignore: Option<&'static str>,
    /// Everyone starts signed up and may opt out (`start_signed_up`).
    pub start_signed_up: bool,
    /// `checked_target` of `poll_ghosts_for_target`: gone at the end, nobody is returned.
    pub target: Option<Entity>,
}

type OnFinish = Box<dyn FnOnce(&mut World, Vec<Candidate>) + Send + Sync>;

/// `/datum/candidate_poll`.
struct Poll {
    id: u64,
    ends_at: f64,
    role_name: String,
    ignore: Option<&'static str>,
    start_signed_up: bool,
    target: Option<Entity>,
    signed_up: Vec<Candidate>,
    prompts: Vec<Entity>,
    on_finish: OnFinish,
}

/// `SSpolling` and `GLOB.poll_ignore`.
#[derive(Resource)]
pub struct Polls {
    active: Vec<Poll>,
    next_id: u64,
    /// Ignore category to the keys that chose "Не в этом раунде".
    ignored: HashMap<&'static str, HashSet<String>>,
    rng: Rng,
}

impl Default for Polls {
    fn default() -> Self {
        Self {
            active: Vec::new(),
            next_id: 0,
            ignored: HashMap::new(),
            rng: Rng::new(0x5EED_6057),
        }
    }
}

/// On a poll's prompt.
#[derive(Component, Clone, Copy, Debug)]
struct PollPrompt {
    poll: u64,
    candidate: Candidate,
}

pub struct GhostPollPlugin;

impl Plugin for GhostPollPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Polls>().add_systems(
            Update,
            (answers.after(PromptSystems::Answer), finish_due)
                .chain()
                .in_set(UiSystems::Handle),
        );
    }
}

/// `GLOB.player_list` ghosts, in entity order.
pub fn ghosts(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<Entity, (With<PlayerMob>, With<Ghost>)>();
    let mut out: Vec<Entity> = q.iter(world).collect();
    out.sort();
    out
}

fn key(world: &World, client: Entity) -> Option<String> {
    world.get::<ClientToken>(client).map(|t| t.0.clone())
}

fn ignored(world: &World, category: Option<&'static str>, client: Entity) -> bool {
    let (Some(category), Some(key)) = (category, key(world, client)) else {
        return false;
    };
    world
        .resource::<Polls>()
        .ignored
        .get(category)
        .is_some_and(|keys| keys.contains(&key))
}

/// `trim_candidates`: the mob is still the player's.
fn present(world: &World, c: Candidate) -> bool {
    world.get::<PlayerMob>(c.mob).map(|p| p.client) == Some(c.client)
}

/// Is a poll for `target` running (`currently_polling_targets`)?
pub fn is_polling_target(world: &World, target: Entity) -> bool {
    world
        .resource::<Polls>()
        .active
        .iter()
        .any(|p| p.target == Some(target))
}

/// tg `poll_candidates`: asks the eligible mobs of `group`. `on_finish` gets who signed up, at
/// the end of the poll or, with nobody to ask, at once (then this is `false`).
pub fn poll_candidates(
    world: &mut World,
    spec: PollSpec,
    group: Vec<Entity>,
    on_finish: impl FnOnce(&mut World, Vec<Candidate>) + Send + Sync + 'static,
) -> bool {
    let mut eligible = Vec::new();
    for mob in group {
        let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
            continue;
        };
        if !ignored(world, spec.ignore, client) {
            eligible.push(Candidate { client, mob });
        }
    }
    if eligible.is_empty() {
        on_finish(world, Vec::new());
        return false;
    }
    let id = {
        let mut polls = world.resource_mut::<Polls>();
        polls.next_id += 1;
        polls.next_id
    };
    let mut prompts = Vec::with_capacity(eligible.len());
    for &candidate in &eligible {
        let buttons: Vec<String> = if spec.start_signed_up {
            vec!["Участвовать".to_owned(), "Отписаться".to_owned()]
        } else {
            let mut buttons = vec!["Записаться".to_owned(), "Отказаться".to_owned()];
            if spec.ignore.is_some() {
                buttons.push("Не в этом раунде".to_owned());
            }
            buttons
        };
        let ask = Ask::new(
            candidate.client,
            candidate.mob,
            spec.role_name.clone(),
            spec.question.clone(),
            PromptKind::Alert {
                buttons,
                autofocus: false,
            },
        )
        .timeout(spec.secs);
        let prompt = ask_world(world, ask);
        world.entity_mut(prompt).insert(PollPrompt {
            poll: id,
            candidate,
        });
        prompts.push(prompt);
    }
    let ends_at = world.resource::<Time>().elapsed_secs_f64() + spec.secs;
    let signed_up = if spec.start_signed_up {
        eligible
    } else {
        Vec::new()
    };
    world.resource_mut::<Polls>().active.push(Poll {
        id,
        ends_at,
        role_name: spec.role_name,
        ignore: spec.ignore,
        start_signed_up: spec.start_signed_up,
        target: spec.target,
        signed_up,
        prompts,
        on_finish: Box::new(on_finish),
    });
    true
}

/// `pick_n_take` from a poll's result.
pub fn pick_candidate(world: &mut World, signed_up: &mut Vec<Candidate>) -> Option<Candidate> {
    if signed_up.is_empty() {
        return None;
    }
    let last = signed_up.len() as i64 - 1;
    let i = world.resource_mut::<Polls>().rng.range(0, last) as usize;
    Some(signed_up.remove(i))
}

/// Ends every running poll now (the tests' fast clock).
pub fn finish_all(world: &mut World) {
    let polls = std::mem::take(&mut world.resource_mut::<Polls>().active);
    for poll in polls {
        finish(world, poll);
    }
}

fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
        return;
    };
    let pending: Vec<(Entity, Option<PromptAnswer>)> = cursor
        .read(messages)
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        if let Some(PromptAnswer::Button(button)) = answer {
            apply_answer(world, prompt, button);
        }
    }
}

/// `sign_up`, `remove_candidate`, `do_never_for_this_round`. Not answering leaves things as
/// they are, as a tg alert nobody clicks.
fn apply_answer(world: &mut World, prompt: Entity, button: usize) {
    let Some(PollPrompt { poll, candidate }) = world.get::<PollPrompt>(prompt).copied() else {
        return;
    };
    if !present(world, candidate) {
        return;
    }
    let player = key(world, candidate.client);
    let mut polls = world.resource_mut::<Polls>();
    let polls = &mut *polls;
    let Some(entry) = polls.active.iter_mut().find(|p| p.id == poll) else {
        return;
    };
    let reply = if entry.start_signed_up {
        if button == OPT_OUT {
            entry.signed_up.retain(|c| *c != candidate);
            Some((Span::Danger, "Вы отказались от участия.".to_owned()))
        } else {
            None
        }
    } else {
        match button {
            SIGN_UP if !entry.signed_up.contains(&candidate) => {
                entry.signed_up.push(candidate);
                Some((
                    Span::Notice,
                    format!(
                        "Вы записались на роль {}! Кандидат будет выбран случайно.",
                        entry.role_name
                    ),
                ))
            }
            NEVER => {
                entry.signed_up.retain(|c| *c != candidate);
                if let (Some(category), Some(player)) = (entry.ignore, player) {
                    polls.ignored.entry(category).or_default().insert(player);
                }
                Some((Span::Danger, "Выбор учтён: не в этом раунде.".to_owned()))
            }
            _ => None,
        }
    };
    if let Some((span, text)) = reply {
        tell(world, candidate.client, span, text);
    }
}

fn finish_due(world: &mut World) {
    if world.resource::<Polls>().active.is_empty() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut polls = world.resource_mut::<Polls>();
    let (due, active): (Vec<Poll>, Vec<Poll>) = std::mem::take(&mut polls.active)
        .into_iter()
        .partition(|p| p.ends_at <= now);
    // A callback may start a new poll (the heavy midround after a failed light one).
    polls.active = active;
    for poll in due {
        finish(world, poll);
    }
}

/// `polling_finished`: revalidates who signed up, closes the windows, runs the continuation.
fn finish(world: &mut World, poll: Poll) {
    let Poll {
        target,
        mut signed_up,
        prompts,
        on_finish,
        ..
    } = poll;
    signed_up.retain(|&c| present(world, c));
    if target.is_some_and(|t| world.get_entity(t).is_err()) {
        signed_up.clear();
    }
    for prompt in prompts {
        world.try_despawn(prompt).ok();
    }
    on_finish(world, signed_up);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_net::chat::server::SendChat;
    use ss13_tgui::prompt::Prompt;
    use ss13_tgui::server::OpenUi;

    #[derive(Resource, Default)]
    struct Finished(Vec<Vec<Candidate>>);

    fn app() -> App {
        let mut app = App::new();
        app.add_message::<PromptAnswered>()
            .add_message::<OpenUi>()
            .add_message::<SendChat>()
            .insert_resource(Time::<()>::default())
            .init_resource::<Finished>()
            .add_plugins(GhostPollPlugin);
        app
    }

    /// A ghost with a player.
    fn ghost(app: &mut App, name: &str) -> Candidate {
        let world = app.world_mut();
        let client = world.spawn(ClientToken(name.to_owned())).id();
        let mob = world
            .spawn((Ghost { body: None }, PlayerMob { client }))
            .id();
        Candidate { client, mob }
    }

    fn spec(ignore: Option<&'static str>) -> PollSpec {
        PollSpec {
            question: "Вы хотите сыграть за блоба?".to_owned(),
            role_name: "блоб".to_owned(),
            secs: DEFAULT_POLL_SECS,
            ignore,
            ..default()
        }
    }

    fn poll(app: &mut App, spec: PollSpec, group: &[Candidate]) -> bool {
        let group = group.iter().map(|c| c.mob).collect();
        poll_candidates(app.world_mut(), spec, group, |world, signed_up| {
            world.resource_mut::<Finished>().0.push(signed_up);
        })
    }

    fn prompt_of(app: &mut App, c: Candidate) -> Entity {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &Prompt)>();
        q.iter(world)
            .find(|(_, p)| p.client == c.client)
            .map(|(e, _)| e)
            .expect("the candidate was asked")
    }

    fn answer(app: &mut App, c: Candidate, button: usize) {
        let prompt = prompt_of(app, c);
        app.world_mut().write_message(PromptAnswered {
            prompt,
            client: c.client,
            actor: c.mob,
            answer: Some(PromptAnswer::Button(button)),
        });
        app.update();
    }

    fn end_of_poll(app: &mut App) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f64(DEFAULT_POLL_SECS + 1.0));
        app.update();
    }

    fn finished(app: &App) -> &[Vec<Candidate>] {
        &app.world().resource::<Finished>().0
    }

    fn prompts(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query::<&PollPrompt>().iter(world).count()
    }

    #[test]
    fn signing_up_declining_and_never_this_round() {
        let mut app = app();
        let a = ghost(&mut app, "a");
        let b = ghost(&mut app, "b");
        let c = ghost(&mut app, "c");
        assert!(poll(&mut app, spec(Some(POLL_IGNORE_BLOB)), &[a, b, c]));
        answer(&mut app, a, SIGN_UP);
        answer(&mut app, b, 1);
        answer(&mut app, c, NEVER);
        assert!(finished(&app).is_empty(), "the poll waits for its time");
        end_of_poll(&mut app);
        assert_eq!(finished(&app), [vec![a]]);
        // The windows are closed.
        assert_eq!(prompts(&mut app), 0);
        // "Не в этом раунде": c is not asked by the next poll of the category, the others are.
        assert!(poll(&mut app, spec(Some(POLL_IGNORE_BLOB)), &[a, b, c]));
        assert_eq!(prompts(&mut app), 2);
        // Other categories still ask c.
        assert!(poll(&mut app, spec(Some("drone")), &[c]));
        assert_eq!(prompts(&mut app), 3);
    }

    #[test]
    fn a_timeout_without_candidates_finishes_with_nobody() {
        let mut app = app();
        let a = ghost(&mut app, "a");
        assert!(poll(&mut app, spec(None), &[a]));
        end_of_poll(&mut app);
        assert_eq!(finished(&app), [Vec::<Candidate>::new()]);
        assert!(app.world().resource::<Polls>().active.is_empty());
        assert_eq!(prompts(&mut app), 0);
        // It does not run again.
        end_of_poll(&mut app);
        assert_eq!(finished(&app).len(), 1);
    }

    #[test]
    fn who_is_gone_at_the_end_is_not_a_candidate() {
        let mut app = app();
        let stays = ghost(&mut app, "stays");
        let disconnected = ghost(&mut app, "left");
        let reentered = ghost(&mut app, "back");
        assert!(poll(
            &mut app,
            spec(None),
            &[stays, disconnected, reentered]
        ));
        answer(&mut app, stays, SIGN_UP);
        answer(&mut app, disconnected, SIGN_UP);
        answer(&mut app, reentered, SIGN_UP);
        // `handle_connections` removes a disconnected ghost; going back to the body leaves the
        // ghost without the player.
        app.world_mut().despawn(disconnected.mob);
        app.world_mut()
            .entity_mut(reentered.mob)
            .remove::<PlayerMob>();
        end_of_poll(&mut app);
        assert_eq!(finished(&app), [vec![stays]]);
    }

    #[test]
    fn nobody_eligible_calls_back_at_once() {
        let mut app = app();
        // A mob with no player.
        let empty = app.world_mut().spawn(Ghost { body: None }).id();
        let c = Candidate {
            client: Entity::PLACEHOLDER,
            mob: empty,
        };
        assert!(!poll(&mut app, spec(None), &[c]));
        assert_eq!(finished(&app), [Vec::<Candidate>::new()]);
        assert_eq!(prompts(&mut app), 0);
    }

    #[test]
    fn start_signed_up_keeps_who_does_not_opt_out() {
        let mut app = app();
        let a = ghost(&mut app, "a");
        let b = ghost(&mut app, "b");
        let c = ghost(&mut app, "c");
        let spec = PollSpec {
            start_signed_up: true,
            ..spec(None)
        };
        assert!(poll(&mut app, spec, &[a, b, c]));
        answer(&mut app, b, OPT_OUT);
        end_of_poll(&mut app);
        assert_eq!(finished(&app), [vec![a, c]]);
    }

    #[test]
    fn a_gone_target_returns_nobody() {
        let mut app = app();
        let a = ghost(&mut app, "a");
        let target = app.world_mut().spawn_empty().id();
        let spec = PollSpec {
            target: Some(target),
            ..spec(None)
        };
        assert!(poll(&mut app, spec, &[a]));
        assert!(is_polling_target(app.world(), target));
        answer(&mut app, a, SIGN_UP);
        app.world_mut().despawn(target);
        end_of_poll(&mut app);
        assert_eq!(finished(&app), [Vec::<Candidate>::new()]);
        assert!(!is_polling_target(app.world(), target));
    }
}
