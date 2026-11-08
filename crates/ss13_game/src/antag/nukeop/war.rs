//! The war declaration (server) after tg `/obj/item/nuclear_challenge` (+ `/clownops`) and the
//! infiltrator console's `launch_check` (`/obj/machinery/computer/shuttle/syndicate`).
//!
//! The leader uses the item in hand ([`use_challenge`]): `check_allowed` (not already deciding,
//! at least [`CHALLENGE_MIN_PLAYERS`] players, on the base level, within the first 5 minutes of
//! the round, the infiltrator never flown, no war yet), then "Объявить войну?" (Да/Нет), "Изменить
//! текст?" and the custom text, each answer re-checked ([`answers`]). The declaration
//! ([`war_was_declared`]) is a priority announcement with the alarm, BandaStation's Gamma code
//! 20 s later, the team's bonus telecrystals ([`distribute_tc`]: `players × 3.5` split between
//! the operatives, into their uplinks or as a telecrystal stack in hand), the shuttle refuel
//! delay raised to 25 minutes, research points for the station, and the item is gone. The
//! infiltrator then may not leave for 20 minutes ([`launch_check`]).
// shortcut: the shuttle dockers are not jammed (no navigation computers), the science channel
// announcement and Cayenne's share of the remainder are not modelled; no admin "force war"
// button (an admin panel would call [`war_was_declared`] directly).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_net::server::ClientToken;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use super::server::{Infiltrator, Nukeop, chat_to};
use crate::antag::uplink::{TELECRYSTAL, Uplink};
use crate::inventory::{CarriedBy, Inventory};

/// `/obj/item/nuclear_challenge`.
pub const CHALLENGE: &str = "/obj/item/nuclear_challenge";
/// `/obj/item/nuclear_challenge/clownops`: another alarm.
pub const CHALLENGE_CLOWN: &str = "/obj/item/nuclear_challenge/clownops";
/// tg `CHALLENGE_MIN_PLAYERS`.
pub const CHALLENGE_MIN_PLAYERS: u32 = 50;
/// tg `CHALLENGE_TIME_LIMIT`.
pub const CHALLENGE_TIME_LIMIT_SECS: f64 = 300.0;
/// tg `CHALLENGE_SHUTTLE_DELAY`: the emergency shuttle's refuel delay after a declaration.
pub const CHALLENGE_SHUTTLE_DELAY_SECS: f64 = 1500.0;
/// tg `SYNDICATE_CHALLENGE_TIMER`: the infiltrator stays home this long after a declaration.
pub const SYNDICATE_CHALLENGE_TIMER_SECS: f64 = 1200.0;
/// BandaStation `challenge_telecrystals_per_crewman`.
pub const TELECRYSTALS_PER_CREWMAN: f64 = 3.5;
/// BandaStation: the Gamma code this long after the declaration.
const GAMMA_DELAY_SECS: f64 = 20.0;
/// `TECHWEB_TIER_5_POINTS * 3`.
const RESEARCH_BONUS: f64 = 600.0;
const ALARM: &str = "sound/announcer/alarm/nuke_alarm.ogg";
const CLOWN_ALARM: &str = "sound/announcer/alarm/clownops.ogg";
const TITLE: &str = "Объявление войны";
const SENDER: &str = "Аванпост ядерных оперативников";

/// The infiltrator board's `challenge_start_time` and `moved`, and the pending Gamma code.
#[derive(Resource, Clone, Debug, Default)]
pub struct WarState {
    /// Game seconds of the declaration.
    pub declared_at: Option<f64>,
    /// The infiltrator has been sent somewhere.
    pub shuttle_moved: bool,
    gamma_at: Option<f64>,
}

/// `declaring_war`, on the item while its owner answers.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Declaring;

/// What a war declaration prompt asks, for the item.
#[derive(Component, Clone, Copy, Debug)]
enum WarQuestion {
    Sure(Entity),
    Customize(Entity),
    Text(Entity),
}

/// Why `check_allowed` refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarRefusal {
    Declaring,
    TooFewPlayers,
    NotOnBase,
    TooLate,
    ShuttleMoved,
    AlreadyDeclared,
}

impl WarRefusal {
    pub fn text(self) -> &'static str {
        match self {
            Self::Declaring => "Вы уже объявляете войну! Определитесь.",
            Self::TooFewPlayers => "Экипаж противника слишком мал, чтобы объявлять ему войну.",
            Self::NotOnBase => "Для этого вы должны находиться на своей базе.",
            Self::TooLate => {
                "Слишком поздно объявлять войну. Ваши покровители уже заняты другими планами. \
                 Придётся обходиться тем, что есть."
            }
            Self::ShuttleMoved => "Шаттл уже улетал! Вы потеряли право объявить войну.",
            Self::AlreadyDeclared => "Война уже объявлена!",
        }
    }
}

/// `check_allowed` over its inputs.
pub fn war_refusal(
    declaring: bool,
    players: u32,
    on_base: bool,
    round_secs: f64,
    state: &WarState,
) -> Option<WarRefusal> {
    if declaring {
        Some(WarRefusal::Declaring)
    } else if players < CHALLENGE_MIN_PLAYERS {
        Some(WarRefusal::TooFewPlayers)
    } else if !on_base {
        Some(WarRefusal::NotOnBase)
    } else if round_secs > CHALLENGE_TIME_LIMIT_SECS {
        Some(WarRefusal::TooLate)
    } else if state.shuttle_moved {
        Some(WarRefusal::ShuttleMoved)
    } else if state.declared_at.is_some() {
        Some(WarRefusal::AlreadyDeclared)
    } else {
        None
    }
}

/// `distribute_tc`'s share: `round(players × 3.5 / recipients)` (DM `round` floors).
pub fn tc_share(players: u32, recipients: u32) -> u32 {
    if recipients == 0 {
        return 0;
    }
    (f64::from(players) * TELECRYSTALS_PER_CREWMAN / f64::from(recipients)).floor() as u32
}

/// `GLOB.player_list.len`: every connected player.
fn player_count(world: &mut World) -> u32 {
    world
        .query_filtered::<(), With<ClientToken>>()
        .iter(world)
        .count() as u32
}

fn round_secs(world: &World) -> f64 {
    let now = world.resource::<Time>().elapsed_secs_f64();
    world
        .get_resource::<crate::round::Ticker>()
        .map_or(0.0, |t| t.round_duration(now))
}

/// `onSyndieBase`: on the level standing in for the base.
fn on_base(world: &World, user: Entity) -> bool {
    let level = world.get_resource::<Infiltrator>().map(|i| i.level_z);
    crate::modpc::pda::turf_of(world, user).is_some_and(|p| Some(p.z) == level)
}

/// `check_allowed(user)` against the world; tells the refusal.
fn check_allowed(world: &mut World, client: Entity, user: Entity, item: Entity) -> bool {
    let declaring = world.get::<Declaring>(item).is_some();
    let players = player_count(world);
    let base = on_base(world, user);
    let secs = round_secs(world);
    let state = world
        .get_resource::<WarState>()
        .cloned()
        .unwrap_or_default();
    match war_refusal(declaring, players, base, secs, &state) {
        Some(refusal) => {
            chat_to(world, Some(client), Span::BoldWarning, refusal.text());
            false
        }
        None => true,
    }
}

/// Whether `item` is a war declaration (either kind).
pub fn is_challenge(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, CHALLENGE))
}

fn ask(world: &mut World, client: Entity, user: Entity, item: Entity, q: WarQuestion) {
    let (title, message, kind) = match q {
        WarQuestion::Sure(_) => {
            let left = (CHALLENGE_TIME_LIMIT_SECS - round_secs(world)).max(0.0);
            let station = crate::round::station_name(world);
            (
                "Объявить войну?",
                format!(
                    "Тщательно посоветуйтесь с командой, прежде чем объявлять войну станции \
                     {station}. Вы уверены, что хотите предупредить экипаж противника? У вас \
                     есть {} на решение.",
                    crate::jobs::display_time_text(left)
                ),
                PromptKind::Alert {
                    buttons: vec!["Да".into(), "Нет".into()],
                    autofocus: true,
                },
            )
        }
        WarQuestion::Customize(_) => (
            "Изменить?",
            "Хотите изменить текст объявления?".to_owned(),
            PromptKind::Alert {
                buttons: vec!["Да".into(), "Нет".into()],
                autofocus: true,
            },
        ),
        WarQuestion::Text(_) => (
            "Объявление",
            "Введите текст объявления".to_owned(),
            PromptKind::Text {
                default: String::new(),
                max_length: ss13_shared::chat::MAX_MESSAGE_LEN as u32,
                multiline: true,
            },
        ),
    };
    world.entity_mut(item).insert(Declaring);
    let prompt = ask_world(world, Ask::new(client, user, title, message, kind));
    world.entity_mut(prompt).insert(q);
}

/// `attack_self`: the checks, then the first question.
pub fn use_challenge(world: &mut World, client: Entity, user: Entity, item: Entity) {
    if !check_allowed(world, client, user, item) {
        return;
    }
    ask(world, client, user, item, WarQuestion::Sure(item));
}

/// The default declaration.
fn default_memo(world: &World) -> String {
    format!(
        "Радикальная группировка Синдиката объявила о своём намерении полностью уничтожить \
         станцию {} с помощью ядерного устройства и бросает экипажу вызов попытаться их \
         остановить.",
        crate::round::station_name(world)
    )
}

/// The answers, each re-checked with `check_allowed` once the item is no longer deciding.
pub(super) fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, user, answer) in answers {
        let Some(q) = world.get::<WarQuestion>(prompt).copied() else {
            continue;
        };
        let item = match q {
            WarQuestion::Sure(i) | WarQuestion::Customize(i) | WarQuestion::Text(i) => i,
        };
        if world.get_entity(item).is_err() || !is_challenge(world, item) {
            continue;
        }
        world.entity_mut(item).remove::<Declaring>();
        if !check_allowed(world, client, user, item) {
            continue;
        }
        let yes = answer == Some(PromptAnswer::Button(0));
        match q {
            WarQuestion::Sure(_) if !yes => chat_to(
                world,
                Some(client),
                Span::Notice,
                "Если подумать, элемент неожиданности не так уж и плох.",
            ),
            WarQuestion::Sure(_) => ask(world, client, user, item, WarQuestion::Customize(item)),
            WarQuestion::Customize(_) if yes => {
                ask(world, client, user, item, WarQuestion::Text(item));
            }
            WarQuestion::Customize(_) => {
                let memo = default_memo(world);
                war_was_declared(world, Some(user), item, &memo);
            }
            WarQuestion::Text(_) => {
                if let Some(PromptAnswer::Text(memo)) = answer
                    && !memo.trim().is_empty()
                {
                    war_was_declared(world, Some(user), item, &memo);
                }
            }
        }
    }
}

/// `war_was_declared(user, memo)`; `user` is `None` for an admin's forced war.
pub fn war_was_declared(world: &mut World, user: Option<Entity>, item: Entity, memo: &str) {
    let clown = world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, CHALLENGE_CLOWN));
    crate::announce::Priority::new(memo, TITLE)
        .sender(SENDER)
        .sound(if clown { CLOWN_ALARM } else { ALARM })
        .important()
        .send(world);
    if let Some(user) = user {
        chat_to(
            world,
            crate::tools::client_of(world, user),
            Span::Plain,
            "Вы привлекли внимание могущественных сил внутри Синдиката. Ваша команда получила \
             бонусный набор телекристаллов. Если вы выполните задание, вас ждут великие дела.",
        );
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    {
        let mut state = world.get_resource_or_insert_with(WarState::default);
        state.declared_at = Some(now);
        state.gamma_at = Some(now + GAMMA_DELAY_SECS);
    }
    distribute_tc(world);
    if let Some(mut config) = world.get_resource_mut::<crate::shuttle::ShuttleConfig>() {
        config.refuel_secs = config.refuel_secs.max(CHALLENGE_SHUTTLE_DELAY_SECS);
    }
    if world.contains_resource::<crate::science::techweb::Techweb>() {
        world.resource_scope(|world, mut web: Mut<crate::science::techweb::Techweb>| {
            if let Some(db) = world.get_resource::<crate::science::techweb::ResearchDb>() {
                web.adjust_points(db, crate::science::techweb::POINT_GENERIC, RESEARCH_BONUS);
            }
        });
    }
    info!("war was declared by {user:?}: {memo}");
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
}

/// `distribute_tc`: the operatives' share into their uplinks; one without an uplink gets it as
/// telecrystals in hand (else at its feet).
pub fn distribute_tc(world: &mut World) {
    let mut ops = world.query_filtered::<Entity, With<Nukeop>>();
    let mut ops: Vec<Entity> = ops.iter(world).collect();
    ops.sort();
    let mut uplinks = world.query::<(Entity, &Uplink)>();
    let found: Vec<(Entity, Entity)> = uplinks
        .iter(world)
        .filter(|(_, u)| u.uplink_flag & ss13_mapload::antag::UPLINK_ALL_SYNDIE_OPS != 0)
        .map(|(e, u)| (u.owner, e))
        .collect();
    let players = player_count(world);
    let share = tc_share(players, ops.len() as u32);
    if share == 0 {
        return;
    }
    for op in ops {
        if let Some(&(_, uplink)) = found.iter().find(|(owner, _)| *owner == op) {
            if let Some(mut u) = world.get_mut::<Uplink>(uplink) {
                u.telecrystals += share;
            }
            continue;
        }
        let Some(pos) = crate::modpc::pda::turf_of(world, op) else {
            continue;
        };
        let Some(stack) = crate::construction::spawn_stack(world, TELECRYSTAL, pos, share) else {
            continue;
        };
        if let Some(inventory) = world.get::<Inventory>(op).cloned() {
            crate::storage::put_in_hands(world, op, &inventory, stack);
        }
        let place = if world.get::<CarriedBy>(stack).is_some() {
            "руки"
        } else {
            "ноги"
        };
        chat_to(
            world,
            crate::tools::client_of(world, op),
            Span::Warning,
            format!(
                "Ваш аплинк не найден, поэтому ваша доля бонусных телекристаллов команды \
                 телепортирована вам в {place}."
            ),
        );
    }
}

/// `launch_check` of the syndicate shuttle console: refused while the station prepares after a
/// declaration; otherwise the infiltrator counts as moved.
pub fn launch_check(world: &mut World) -> Result<(), &'static str> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut state = world.get_resource_or_insert_with(WarState::default);
    if state
        .declared_at
        .is_some_and(|at| now < at + SYNDICATE_CHALLENGE_TIMER_SECS)
    {
        return Err("Вы бросили вызов станции! Дайте им ещё немного времени на подготовку.");
    }
    state.shuttle_moved = true;
    Ok(())
}

/// The Gamma code 20 s after the declaration.
pub(super) fn tick_war(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due = world
        .get_resource::<WarState>()
        .and_then(|s| s.gamma_at)
        .is_some_and(|at| now >= at);
    if !due {
        return;
    }
    world.resource_mut::<WarState>().gamma_at = None;
    crate::security_level::set_level(world, crate::security_level::SecLevel::Gamma, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_allowed_follows_tg_order() {
        let state = WarState::default();
        assert_eq!(
            war_refusal(true, 10, false, 999.0, &state),
            Some(WarRefusal::Declaring)
        );
        assert_eq!(
            war_refusal(false, 49, true, 0.0, &state),
            Some(WarRefusal::TooFewPlayers)
        );
        assert_eq!(
            war_refusal(false, 50, false, 0.0, &state),
            Some(WarRefusal::NotOnBase)
        );
        assert_eq!(
            war_refusal(false, 50, true, 301.0, &state),
            Some(WarRefusal::TooLate)
        );
        assert_eq!(war_refusal(false, 50, true, 300.0, &state), None);
        let moved = WarState {
            shuttle_moved: true,
            ..default()
        };
        assert_eq!(
            war_refusal(false, 60, true, 10.0, &moved),
            Some(WarRefusal::ShuttleMoved)
        );
        let declared = WarState {
            declared_at: Some(5.0),
            ..default()
        };
        assert_eq!(
            war_refusal(false, 60, true, 10.0, &declared),
            Some(WarRefusal::AlreadyDeclared)
        );
    }

    #[test]
    fn bonus_telecrystals_are_split_and_floored() {
        // 50 players × 3.5 = 175 between 3 operatives: 58 each.
        assert_eq!(tc_share(50, 3), 58);
        assert_eq!(tc_share(60, 5), 42);
        assert_eq!(tc_share(60, 0), 0);
    }

    #[test]
    fn the_infiltrator_waits_twenty_minutes_after_a_declaration() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(WarState {
            declared_at: Some(0.0),
            ..default()
        });
        assert!(launch_check(&mut world).is_err());
        assert!(!world.resource::<WarState>().shuttle_moved);
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f64(
                SYNDICATE_CHALLENGE_TIMER_SECS,
            ));
        assert!(launch_check(&mut world).is_ok());
        assert!(world.resource::<WarState>().shuttle_moved);
        // Without a war the first launch only marks the shuttle as moved.
        let mut world = World::new();
        world.init_resource::<Time>();
        assert!(launch_check(&mut world).is_ok());
        assert!(world.resource::<WarState>().shuttle_moved);
    }
}
