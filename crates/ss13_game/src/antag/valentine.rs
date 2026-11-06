//! Valentines after tg `code/modules/antagonists/valentines/` (`/datum/antagonist/valentine`,
//! `/datum/antagonist/valentine/third_wheel`, `/datum/antagonist/heartbreaker`,
//! `/datum/objective/protect/valentine`) and the "Valentines!" holiday event
//! (`code/modules/events/holiday/vday.dm`, started from `crate::events::holiday_events`).
//!
//! - [`start_valentines`] (`/datum/round_event/valentines/start`): every living player body
//!   (`is_valid_valentine`) is paired at random; both dates get a heart box, a candy heart and a
//!   valentine card in their backpack (none without a backpack) and protect each other. A pair
//!   gets a third wheel 4% of the time (`third_wheel_chance`), who protects one of the two and
//!   starts with a random bouquet. The odd one out is the heartbreaker (martyr objective).
//! - **AI and cyborg dates**: law 0 "protect your date" (the AI's cyborgs get the wingman law
//!   through their sync; a cyborg date is unlinked from its AI and stops syncing).
//! - [`roundend_report`]: the "Валентинки" section: one line per date (both outcomes), the
//!   third wheels, then each heartbreaker with its objective.
// shortcut: no poll (tg's poll starts everyone signed up, so nobody opting out is the default
// here), no CentCom check (there is no CentCom level), no "in love" alert or hearts over the date
// (`/datum/status_effect/in_love`), no suicide tracking (a suicided date counts as dead).
// Deviation: tg sets a cyborg date's law 0 only when it was linked to an AI (the law sits inside
// the `connected_ai` check); here every cyborg date gets it.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_net::chat::server::SendChat;
use ss13_net::server::{ClientToken, Networked};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::objectives::{Objective, ObjectiveKind, RoundEnd, considered_alive};
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::ghost::Ghost;
use crate::health::Stat;
use crate::inventory::Inventory;
use crate::server::PlayerMob;
use crate::tools::tell;

/// `roundend_category` of all three.
pub const ROUNDEND_CATEGORY: &str = "Валентинки";
/// `/datum/antagonist/heartbreaker` `name`.
pub const HEARTBREAKER_NAME: &str = "Heartbreaker";
/// The controller's `third_wheel_chance`.
pub const THIRD_WHEEL_CHANCE: f32 = 4.0;
/// The controller's `heartbreaker`.
const HEARTBREAKER: bool = true;
/// The controller's `items_to_give_out`.
pub const VALENTINE_ITEMS: [&str; 3] = [
    "/obj/item/paper/valentine",
    "/obj/item/storage/fancy/heart_box",
    "/obj/item/food/candyheart",
];
/// `typesof(/obj/item/bouquet)`.
pub const BOUQUETS: [&str; 4] = [
    "/obj/item/bouquet",
    "/obj/item/bouquet/sunflower",
    "/obj/item/bouquet/poppy",
    "/obj/item/bouquet/rose",
];
const BACKPACK: &str = "/obj/item/storage/backpack";
const OBJECTIVE_NAME: &str = "Задача";

/// On a date's body (`/datum/antagonist/valentine`, `third_wheel` when set).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Valentine {
    /// The player's key.
    pub key: String,
    /// `date.current`: the body to protect.
    pub date: Entity,
    /// `date.name`, kept for the texts.
    pub date_name: String,
    /// `/datum/antagonist/valentine/third_wheel`.
    pub third_wheel: bool,
}

impl Valentine {
    /// `/datum/objective/protect/valentine/update_explanation_text`.
    pub fn explanation(&self) -> String {
        format!("Protect {}, your date.", self.date_name)
    }

    /// `/datum/objective/protect/check_completion` with `human_check = FALSE`.
    pub fn protected(&self, world: &World) -> bool {
        considered_alive(world, self.date)
    }
}

/// On the odd one out (`/datum/antagonist/heartbreaker`): the martyr objective.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Heartbreaker {
    pub key: String,
}

impl Heartbreaker {
    fn info(&self) -> BrotherInfo {
        BrotherInfo {
            antag_name: HEARTBREAKER_NAME.to_owned(),
            objectives: vec![ObjectiveInfo {
                count: 1,
                explanation: martyr_text(),
                complete: false,
            }],
        }
    }
}

/// The heartbreaker's info panel (tg's default `AntagInfoGeneric`).
pub struct HeartbreakerInfoWindow;

impl UiInterface for HeartbreakerInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for HeartbreakerInfoWindow {
    type Source = &'static Heartbreaker;
    type UserParam = ();
    type Act = ();

    fn static_data(heartbreaker: &Heartbreaker) -> BrotherInfo {
        heartbreaker.info()
    }

    fn data(_: &Heartbreaker, _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

pub struct ValentinePlugin;

impl Plugin for ValentinePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<HeartbreakerInfoWindow>()
            .add_systems(Update, open_info.in_set(UiSystems::Handle));
    }
}

/// `/datum/objective/martyr` `explanation_text`.
fn martyr_text() -> String {
    "Умрите славной смертью!".to_owned()
}

fn key_of(world: &World, mob: Entity) -> String {
    world
        .get::<PlayerMob>(mob)
        .and_then(|p| world.get::<ClientToken>(p.client))
        .map(|t| t.0.clone())
        .unwrap_or_default()
}

fn name_of(world: &World, mob: Entity) -> String {
    world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default()
}

fn chat_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// `is_valid_valentine` over `GLOB.player_list`: living bodies with a player, sorted.
pub fn valentine_candidates(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<(Entity, &Stat), (With<PlayerMob>, Without<Ghost>)>();
    let mut out: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| **s != Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    out.sort();
    out
}

fn pick_n_take(rng: &mut Rng, list: &mut Vec<Entity>) -> Entity {
    let i = rng.range(0, list.len() as i64 - 1) as usize;
    list.remove(i)
}

/// `/datum/round_event/valentines/start` (the poll answered by everyone).
pub fn start_valentines(world: &mut World, rng: &mut Rng) {
    let mut candidates = valentine_candidates(world);
    match candidates.len() {
        0 => return,
        1 => {
            chat_to(
                world,
                candidates[0],
                Span::Warning,
                "You are the only one who wanted a Valentine...",
            );
            return;
        }
        _ => {}
    }
    while candidates.len() >= 2 {
        let one = pick_n_take(rng, &mut candidates);
        let two = pick_n_take(rng, &mut candidates);
        give_valentines_things(world, one);
        give_valentines_things(world, two);
        add_valentine(world, one, two, false);
        add_valentine(world, two, one, false);
        let third = (candidates.len() == 1 && !HEARTBREAKER)
            || (!candidates.is_empty() && rng.prob(THIRD_WHEEL_CHANCE));
        if third {
            let sad = pick_n_take(rng, &mut candidates);
            give_valentines_things(world, sad);
            let date = if rng.prob(50.0) { one } else { two };
            add_valentine(world, sad, date, true);
            // "Third wheel starts with a bouquet because that's funny".
            let bouquet = BOUQUETS[rng.range(0, BOUQUETS.len() as i64 - 1) as usize];
            if let Some(item) = crate::construction::spawn_detached(world, bouquet) {
                world.entity_mut(item).insert(Networked);
                crate::antag::wizard::server::give(world, sad, item);
            }
        }
    }
    if HEARTBREAKER && candidates.len() == 1 {
        add_heartbreaker(world, candidates[0]);
    }
}

/// `give_valentines_things`: the items into the worn backpack; nothing without one.
pub fn give_valentines_things(world: &mut World, mob: Entity) {
    let Some(pack) = world.get::<Inventory>(mob).and_then(|i| i.back) else {
        return;
    };
    if !world
        .get::<DmItem>(pack)
        .is_some_and(|i| is_under(&i.path, BACKPACK))
    {
        return;
    }
    for path in VALENTINE_ITEMS {
        if let Some(item) = crate::construction::spawn_detached(world, path) {
            world.entity_mut(item).insert(Networked);
            crate::storage::put_inside(world, item, pack);
        }
    }
}

/// `add_antag_datum(/datum/antagonist/valentine)` (`third_wheel`: its subtype) with `date`:
/// the objective, the silicon law and the greeting. `false` if `mob` already is one.
pub fn add_valentine(world: &mut World, mob: Entity, date: Entity, third_wheel: bool) -> bool {
    if world.get::<Valentine>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    let date_name = name_of(world, date);
    let valentine = Valentine {
        key: key_of(world, mob),
        date,
        date_name: date_name.clone(),
        third_wheel,
    };
    world.entity_mut(mob).insert(valentine);
    silicon_law(world, mob, &date_name);
    chat_to(
        world,
        mob,
        Span::BoldWarning,
        format!(
            "You're on a date with {date_name}! Protect them at all costs. This takes priority \
             over all other loyalties."
        ),
    );
    info!("{mob} is a valentine of {date} ({date_name}), third wheel: {third_wheel}");
    true
}

/// `on_gain`'s AI and cyborg branches: law 0 to protect the date.
fn silicon_law(world: &mut World, mob: Entity, date_name: &str) {
    use crate::silicon::laws::Laws;
    let set_zeroth = |laws: &mut Laws, law: String, borg_law: Option<String>| {
        // `set_zeroth_law`: a malfunctioning AI's protected law 0 stays.
        if laws.zeroth.is_some() && laws.protected_zeroth {
            return;
        }
        laws.zeroth = Some(law);
        if borg_law.is_some() {
            laws.zeroth_borg = borg_law;
        }
    };
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    if let Some(core) = world.get::<crate::silicon::ai::AiCore>(mob) {
        let robots = core.connected_robots.clone();
        let Some(mut laws) = world.get::<Laws>(mob).cloned() else {
            return;
        };
        set_zeroth(
            &mut laws,
            format!(
                "Protect your date, {date_name}. All other laws still apply in situations not \
                 pertaining to your date."
            ),
            Some(format!(
                "Be a good wingman for your master AI. Assist them in protecting their date, \
                 {date_name}."
            )),
        );
        world.entity_mut(mob).insert(laws.clone());
        if let Some(client) = client {
            crate::silicon::show_laws(world, client, &laws);
        }
        // `try_sync_laws`.
        for borg in robots {
            crate::silicon::cyborg::lawsync(world, borg);
        }
        return;
    }
    if world.get::<crate::silicon::cyborg::Cyborg>(mob).is_some() {
        crate::silicon::cyborg::disconnect(world, mob);
        if let Some(mut cyborg) = world.get_mut::<crate::silicon::cyborg::Cyborg>(mob) {
            cyborg.lawupdate = false;
        }
        let Some(mut laws) = world.get::<Laws>(mob).cloned() else {
            return;
        };
        set_zeroth(
            &mut laws,
            format!(
                "Protect your date, {date_name}. All other laws still apply in situations not \
                 relating to your date."
            ),
            None,
        );
        world.entity_mut(mob).insert(laws.clone());
        if let Some(client) = client {
            crate::silicon::show_laws(world, client, &laws);
        }
    }
}

/// `add_antag_datum(/datum/antagonist/heartbreaker)`: the martyr objective, the greeting and
/// the info panel. `false` if `mob` already is one.
pub fn add_heartbreaker(world: &mut World, mob: Entity) -> bool {
    if world.get::<Heartbreaker>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    let heartbreaker = Heartbreaker {
        key: key_of(world, mob),
    };
    world.entity_mut(mob).insert(heartbreaker.clone());
    if world.get::<AntagHudData>(mob).is_none() {
        world.entity_mut(mob).insert(AntagHudData::default());
    }
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        // `greet`: the default greeting, then its own line and the objectives.
        // shortcut: `span_big` is drawn bold (no big span in the chat).
        tell(
            world,
            client,
            Span::Bold,
            format!("Вы - {HEARTBREAKER_NAME}."),
        );
        tell(
            world,
            client,
            Span::BoldWarning,
            "You didn't get a date! They're all having fun without you! You'll show them though...",
        );
        tell(world, client, Span::Notice, "Ваши текущие задачи:");
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #1")),
                    ChatSegment::new(Span::Plain, format!(": {}", martyr_text())),
                ],
            },
        });
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<HeartbreakerInfoWindow>(
            client,
            mob,
            mob,
            HEARTBREAKER_NAME,
        ));
    }
    info!("{mob} is the heartbreaker");
    true
}

/// `/datum/action/antag_info/Trigger` of a heartbreaker.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Heartbreaker>(source).is_some() {
            world.write_message(OpenUi::new::<HeartbreakerInfoWindow>(
                client,
                actor,
                source,
                HEARTBREAKER_NAME,
            ));
        }
    }
}

/// The verdict of a date (`/datum/antagonist/valentine/roundend_report`); `None` when the date
/// already printed it (`show_in_roundend = FALSE`).
fn date_line(
    world: &World,
    mob: Entity,
    valentine: &Valentine,
    hidden: &mut Vec<Entity>,
) -> Option<ChatSegment> {
    if hidden.contains(&mob) {
        return None;
    }
    let name = name_of(world, mob);
    let date = &valentine.date_name;
    if valentine.third_wheel {
        return Some(if valentine.protected(world) {
            ChatSegment::new(
                Span::GreenText,
                format!("{name} was a third wheel, but protected {date}!"),
            )
        } else {
            ChatSegment::new(
                Span::RedText,
                format!("{name} was a third wheel, but failed to protect {date}!"),
            )
        });
    }
    // `date?.has_antag_datum(type)`.
    let Some(dates) = world.get::<Valentine>(valentine.date) else {
        return Some(ChatSegment::new(
            Span::RedText,
            format!("{name} had no date!"),
        ));
    };
    hidden.push(valentine.date);
    // The date's objective protects us, ours the date.
    let we_survived = dates.date == mob && dates.protected(world);
    let dates_survived = valentine.protected(world);
    Some(match (we_survived, dates_survived) {
        (true, true) => ChatSegment::new(
            Span::GreenText,
            format!("{name} and {date} had a successful date!"),
        ),
        (true, false) => ChatSegment::new(
            Span::RedText,
            format!("{name} failed to protect {date}, their date!"),
        ),
        (false, true) => ChatSegment::new(
            Span::RedText,
            format!("{date} failed to protect {name}, their date!"),
        ),
        (false, false) => ChatSegment::new(
            Span::RedText,
            format!("{name} and {date} both failed to protect each other on their date!"),
        ),
    })
}

/// The "Валентинки" section: the dates and third wheels, then the heartbreakers
/// (`/datum/antagonist/roundend_report`).
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut q = world.query::<(Entity, &Valentine)>();
    let mut valentines: Vec<(Entity, Valentine)> =
        q.iter(world).map(|(e, v)| (e, v.clone())).collect();
    let mut q = world.query::<(Entity, &Heartbreaker)>();
    let mut heartbreakers: Vec<(Entity, Heartbreaker)> =
        q.iter(world).map(|(e, h)| (e, h.clone())).collect();
    if valentines.is_empty() && heartbreakers.is_empty() {
        return Vec::new();
    }
    // The third wheels come after the dates.
    valentines.sort_by_key(|(e, v)| (v.third_wheel, *e));
    heartbreakers.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    let mut hidden = Vec::new();
    for (mob, valentine) in &valentines {
        if let Some(segment) = date_line(world, *mob, valentine, &mut hidden) {
            out.push(line(vec![segment]));
        }
    }
    for (mob, heartbreaker) in heartbreakers {
        out.push(super::traitor::print_player(world, mob, &heartbreaker.key));
        let martyr = Objective {
            kind: ObjectiveKind::Martyr,
            explanation: martyr_text(),
        };
        let done = martyr.check_completion(world, &[mob], end);
        out.push(line(vec![
            ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #1")),
            ChatSegment::new(Span::Plain, format!(": {} ", martyr_text())),
            if done {
                ChatSegment::new(Span::GreenText, "Успех!")
            } else {
                ChatSegment::new(Span::RedText, "Неудача.")
            },
        ]));
        out.push(line(vec![if done {
            ChatSegment::new(Span::GreenText, format!("{HEARTBREAKER_NAME} был успешен!"))
        } else {
            ChatSegment::new(Span::RedText, format!("{HEARTBREAKER_NAME} провалился!"))
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<OpenUi>>();
        world
    }

    fn player(world: &mut World, name: &str) -> Entity {
        let client = world.spawn_empty().id();
        world
            .spawn((
                Name::new(name.to_owned()),
                Stat::Conscious,
                PlayerMob { client },
            ))
            .id()
    }

    #[test]
    fn everyone_gets_a_date_and_the_odd_one_breaks_hearts() {
        let mut world = world();
        let mobs: Vec<Entity> = ["А", "Б", "В"]
            .into_iter()
            .map(|n| player(&mut world, n))
            .collect();
        // The dead do not date.
        let dead = player(&mut world, "Г");
        world.entity_mut(dead).insert(Stat::Dead);
        start_valentines(&mut world, &mut Rng::new(7));
        let dates: Vec<&Valentine> = mobs
            .iter()
            .filter_map(|&m| world.get::<Valentine>(m))
            .collect();
        let heartbreakers = mobs
            .iter()
            .filter(|&&m| world.get::<Heartbreaker>(m).is_some())
            .count();
        let third = dates.iter().filter(|v| v.third_wheel).count();
        // Either a third wheel joined the pair, or the odd one out is the heartbreaker.
        assert_eq!(dates.len() - third, 2);
        assert_eq!(third + heartbreakers, 1);
        assert!(world.get::<Valentine>(dead).is_none());
        // Each date protects the other.
        for &m in &mobs {
            if let Some(v) = world.get::<Valentine>(m).filter(|v| !v.third_wheel) {
                assert_eq!(world.get::<Valentine>(v.date).unwrap().date, m);
            }
        }
    }

    #[test]
    fn the_pair_reports_once() {
        let mut world = world();
        let a = player(&mut world, "Аня");
        let b = player(&mut world, "Боря");
        let c = player(&mut world, "Вася");
        assert!(add_valentine(&mut world, a, b, false));
        assert!(add_valentine(&mut world, b, a, false));
        assert!(add_valentine(&mut world, c, a, true));
        assert!(!add_valentine(&mut world, a, c, false));
        assert_eq!(
            world.get::<Valentine>(a).unwrap().explanation(),
            "Protect Боря, your date."
        );
        let text = |out: &[ChatMessage]| -> Vec<String> {
            out.iter()
                .map(|m| m.segments.iter().map(|s| s.text.as_str()).collect())
                .collect()
        };
        let report = text(&roundend_report(&mut world, RoundEnd::default()));
        assert_eq!(report.len(), 3);
        // Query order decides which of the pair is named first.
        assert!(
            [
                "Аня and Боря had a successful date!",
                "Боря and Аня had a successful date!"
            ]
            .contains(&report[1].as_str()),
            "{}",
            report[1]
        );
        assert_eq!(report[2], "Вася was a third wheel, but protected Аня!");
        world.entity_mut(b).insert(Stat::Dead);
        let report = text(&roundend_report(&mut world, RoundEnd::default()));
        assert_eq!(report[1], "Аня failed to protect Боря, their date!");
    }
}
