//! The traitor (server) after tg `/datum/antagonist/traitor` (BandaStation strings).
//!
//! [`add_traitor`] is `add_antag_datum(/datum/antagonist/traitor)` on a body: `on_gain` gives
//! the uplink silently, forges the objectives and the ending objective, picks the employer, and
//! the base `on_gain` greets ("Вы - Предатель."), opens the info panel and grants the antag info
//! button and the antag HUD icon ([`AntagHudData`]). The info panel ([`AntagInfoTraitorUi`]) is
//! also the traitor's notes: objectives, the uplink code and how to unlock it. At round end
//! [`roundend_report`] is tg's `antag_report` section for traitors.
// shortcut: no codewords (`should_give_codewords`), custom objectives, moodlets or stinger sound;
// the clown keeps his clumsiness (no mutations here).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::antag::TraitorFlavor;
use ss13_mapload::spawner::Rng;
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::interfaces::ru_jobs::job_ru;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::dynamic::DynamicRng;
use super::objectives::{Objective, ObjectiveKind, RoundEnd, forge_traitor_objectives};
use super::uplink::{PurchaseLog, UplinkCatalog, give_uplink, open_uplink};
use super::{AntagHudData, AntagHudUi, AntagInfoTraitorUi, ObjectiveInfo, TraitorInfo};
use crate::health::Stat;
use crate::jobs::{Jobs, PlayerJob};
use crate::server::PlayerMob;

/// `name = "\proper Предатель"`.
pub const TRAITOR_NAME: &str = "Предатель";
/// `roundend_category`.
pub const ROUNDEND_CATEGORY: &str = "Предатели";
/// `roundend_report`'s objective lines: "Задача #<n>".
const OBJECTIVE_NAME: &str = "Задача";
/// `antag_hud_name` in `hud_icon`.
const ANTAG_HUD: (&str, &str) = ("icons/mob/huds/antag_hud.dmi", "traitor");

/// tg `GLOB.syndicate_employers`, `GLOB.nanotrasen_employers`, `GLOB.hijack_employers`,
/// `GLOB.normal_employers` (`code/__DEFINES/antagonists.dm`, BandaStation's edits).
const SYNDICATE_EMPLOYERS: [&str; 8] = [
    "Cybersun Industries",
    "Donk Corporation",
    "Gorlex Marauders",
    "MI13",
    "Tiger Cooperative Occultist",
    "Waffle Corporation Terrorist",
    "Waffle Corporation",
    "S.E.L.F. Activist",
];
const NANOTRASEN_EMPLOYERS: [&str; 2] = ["Corporate Climber", "Legal Trouble"];
const HIJACK_EMPLOYERS: [&str; 3] = [
    "Gone Postal",
    "Tiger Cooperative Occultist",
    "Waffle Corporation Terrorist",
];
const NORMAL_EMPLOYERS: [&str; 8] = [
    "Corporate Climber",
    "Cybersun Industries",
    "Donk Corporation",
    "Gorlex Marauders",
    "S.E.L.F. Activist",
    "Legal Trouble",
    "MI13",
    "Waffle Corporation",
];

/// The traitor antagonist datum, on the body it was given to (the mind here).
#[derive(Component, Clone, Debug)]
pub struct Traitor {
    /// The player's key (tg `owner.key`) for the report.
    pub key: String,
    pub employer: String,
    pub flavor: TraitorFlavor,
    pub objectives: Vec<Objective>,
    pub ending: ObjectiveKind,
    pub uplink: Option<Entity>,
    pub code: Option<String>,
    pub unlock_text: String,
    /// tg `antag_memory`: the uplink's `unlock_note`.
    pub memory: String,
}

pub struct TraitorPlugin;

impl Plugin for TraitorPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AntagInfoTraitorUi>()
            .add_systems(Update, open_info.in_set(UiSystems::Handle));
    }
}

/// `pick_employer`: 75% a syndicate employer, else a Nanotrasen one, among those that hire for
/// the ending objective.
pub fn pick_employer(hijacker: bool, rng: &mut Rng) -> &'static str {
    let syndicate = rng.prob(75.0);
    let possible: Vec<&str> = SYNDICATE_EMPLOYERS
        .iter()
        .chain(&NANOTRASEN_EMPLOYERS)
        .copied()
        .filter(|e| {
            if hijacker {
                !NORMAL_EMPLOYERS.contains(e)
            } else {
                !HIJACK_EMPLOYERS.contains(e)
            }
        })
        .filter(|e| {
            if syndicate {
                !NANOTRASEN_EMPLOYERS.contains(e)
            } else {
                !SYNDICATE_EMPLOYERS.contains(e)
            }
        })
        .collect();
    // An empty pick (a Nanotrasen hijacker) is null in tg; take the first hijack employer.
    possible
        .get(rng.range(0, possible.len() as i64 - 1) as usize)
        .copied()
        .unwrap_or(HIJACK_EMPLOYERS[0])
}

pub(super) fn chat(world: &mut World, client: Option<Entity>, span: Span, text: impl Into<String>) {
    if let Some(client) = client {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::System, span, text),
        });
    }
}

/// `add_antag_datum(/datum/antagonist/traitor)` on `mob`. `false` if it already is one.
pub fn add_traitor(world: &mut World, mob: Entity) -> bool {
    gain(world, mob, true)
}

/// `on_gain`; `show_panel: false` skips opening the info panel (it stays on the antag button).
fn gain(world: &mut World, mob: Entity, show_panel: bool) -> bool {
    if world.get::<Traitor>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    let mut rng = world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(mob.to_bits()), |r| r.0);
    let catalog_data = world
        .get_resource::<UplinkCatalog>()
        .map(|c| (c.data.phonetic_alphabet.clone(), c.data.steal_items.clone()))
        .unwrap_or_default();
    let (phonetic, steal_targets) = catalog_data;
    let given = give_uplink(world, mob, &phonetic, &mut rng);
    let (objectives, ending) = forge_traitor_objectives(world, mob, &steal_targets, &mut rng);
    let employer = pick_employer(ending == ObjectiveKind::Hijack, &mut rng);
    world.insert_resource(DynamicRng(rng));
    let flavor = world
        .get_resource::<UplinkCatalog>()
        .and_then(|c| {
            c.data
                .traitor_flavor
                .iter()
                .find(|(name, _)| name == employer)
                .map(|(_, f)| f.clone())
        })
        .unwrap_or_default();
    if let Some(given) = &given
        && let Some(mut uplink) = world.get_mut::<super::uplink::Uplink>(given.device)
    {
        uplink.primary_objectives = primary_objectives(&objectives, &phonetic);
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(ANTAG_HUD.0, ANTAG_HUD.1));
    world.entity_mut(mob).insert((
        Traitor {
            key,
            employer: employer.to_owned(),
            flavor,
            objectives,
            ending,
            uplink: given.as_ref().map(|g| g.device),
            code: given.as_ref().map(|g| g.code.clone()),
            unlock_text: given
                .as_ref()
                .map(|g| g.unlock_text.clone())
                .unwrap_or_default(),
            memory: given
                .as_ref()
                .map(|g| g.unlock_note.clone())
                .unwrap_or_default(),
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    if world.get::<PurchaseLog>(mob).is_none() {
        world.entity_mut(mob).insert(PurchaseLog::default());
    }
    // The base `on_gain` (`silent = FALSE`): `greet()`, the panel note, the panel itself.
    chat(world, client, Span::Bold, format!("Вы - {TRAITOR_NAME}."));
    chat(
        world,
        client,
        Span::BoldNotice,
        "Для большей информации - прочтите панель. Вы можете вернуться снова, нажав кнопку \
         сверху слева.",
    );
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        if show_panel {
            world.write_message(OpenUi::new::<AntagInfoTraitorUi>(
                client,
                mob,
                mob,
                TRAITOR_NAME,
            ));
        }
    }
    info!(
        "{mob} is a traitor of {employer}{}",
        if given.is_some() {
            ""
        } else {
            " without an uplink"
        }
    );
    true
}

/// `ui_data`'s `primary_objectives`: "DIRECTIVE <phonetic word>" names.
fn primary_objectives(objectives: &[Objective], phonetic: &[String]) -> Vec<(String, String)> {
    objectives
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let name = match phonetic.get(i) {
                Some(word) => format!("DIRECTIVE {}", word.to_uppercase()),
                None => format!("DIRECTIVE {}", i + 1),
            };
            (name, o.explanation.clone())
        })
        .collect()
}

/// Dev and admins: `SS13_DEV_TRAITOR` makes a joining body a traitor; `unlocked` also unlocks
/// and opens its uplink; `brother` makes it a blood brother instead, `vampire` a vampire,
/// `changeling` a changeling.
pub fn dev_traitor(world: &mut World, client: Entity, mob: Entity) {
    let dev = world
        .get_resource::<super::dynamic::DynamicConfig>()
        .map(|c| c.dev_traitor)
        .unwrap_or_default();
    if dev == super::dynamic::DevTraitor::Brother {
        super::brother::add_brother(world, mob, None);
        return;
    }
    if dev == super::dynamic::DevTraitor::Vampire {
        super::vampire::server::add_vampire(world, mob);
        return;
    }
    if dev == super::dynamic::DevTraitor::Changeling {
        super::changeling::server::add_changeling(world, mob);
        return;
    }
    if dev == super::dynamic::DevTraitor::Nukeop {
        super::nukeop::server::dev_nukeop(world, client, mob);
        return;
    }
    if dev == super::dynamic::DevTraitor::Cult {
        super::cult::server::add_cultist_equipped(world, mob);
        return;
    }
    if dev == super::dynamic::DevTraitor::Heretic {
        super::heretic::server::add_heretic(world, mob);
        return;
    }
    if dev == super::dynamic::DevTraitor::Revolution {
        let head = !world.contains_resource::<super::revolution::server::RevolutionHandler>();
        super::revolution::server::add_revolutionary(world, mob, head);
        return;
    }
    let unlocked = dev == super::dynamic::DevTraitor::Unlocked;
    // An opened uplink would sit under the info panel.
    if dev == super::dynamic::DevTraitor::Off || !gain(world, mob, !unlocked) {
        return;
    }
    if unlocked && let Some(device) = world.get::<Traitor>(mob).and_then(|t| t.uplink) {
        if let Some(mut uplink) = world.get_mut::<super::uplink::Uplink>(device) {
            uplink.locked = false;
        }
        open_uplink(world, client, mob, device);
    }
}

impl ServerUi for AntagInfoTraitorUi {
    type Source = &'static Traitor;
    type UserParam = ();
    type Act = ();

    fn static_data(traitor: &Traitor) -> TraitorInfo {
        TraitorInfo {
            has_codewords: false,
            theme: traitor.flavor.ui_theme.clone(),
            intro: traitor.flavor.introduction.clone(),
            allies: traitor.flavor.allies.clone(),
            goal: traitor.flavor.goal.clone(),
            code: traitor.code.clone(),
            has_uplink: traitor.uplink.is_some(),
            given_uplink: true,
            uplink_intro: traitor.flavor.uplink.clone(),
            uplink_unlock_info: traitor.unlock_text.clone(),
            objectives: traitor
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation.clone(),
                    complete: false,
                })
                .collect(),
        }
    }

    fn data(_: &Traitor, _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// `/datum/action/antag_info/Trigger`: the panel of the antagonist the button belongs to.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Traitor>(source).is_some() {
            world.write_message(OpenUi::new::<AntagInfoTraitorUi>(
                client,
                actor,
                source,
                TRAITOR_NAME,
            ));
        }
    }
}

/// tg `printplayer`: "<key> был <name>, <job>, и выжил/умер".
pub(super) fn print_player(world: &World, mob: Entity, key: &str) -> ChatMessage {
    let name = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let title = world
        .get::<PlayerJob>(mob)
        .and_then(|j| {
            Some(
                world
                    .get_resource::<Jobs>()?
                    .data
                    .jobs
                    .get(j.0)?
                    .title
                    .clone(),
            )
        })
        .unwrap_or_default();
    let mut segments = vec![
        ChatSegment::new(Span::Bold, key.to_owned()),
        ChatSegment::new(Span::Plain, " был "),
        ChatSegment::new(Span::Bold, name),
    ];
    if !title.is_empty() {
        let title = job_ru(&title).map_or(title.clone(), str::to_owned);
        segments.push(ChatSegment::new(Span::Plain, ", "));
        segments.push(ChatSegment::new(Span::Bold, format!("{title},")));
    }
    segments.push(ChatSegment::new(Span::Plain, " и "));
    segments.push(match world.get::<Stat>(mob) {
        Some(Stat::Dead) => ChatSegment::new(Span::RedText, "умер"),
        Some(_) => ChatSegment::new(Span::GreenText, "выжил"),
        None => ChatSegment::new(Span::RedText, "его тело было уничтожено!"),
    });
    ChatMessage {
        channel: ChatChannel::System,
        segments,
    }
}

/// tg `antag_report` for traitors: the header, then each traitor's `roundend_report`.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Traitor, Option<&PurchaseLog>)>();
    let mut traitors: Vec<(Entity, Traitor, PurchaseLog)> = query
        .iter(world)
        .map(|(e, t, log)| (e, t.clone(), log.cloned().unwrap_or_default()))
        .collect();
    if traitors.is_empty() {
        return Vec::new();
    }
    traitors.sort_by_key(|(e, ..)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, traitor, log) in traitors {
        out.push(print_player(world, mob, &traitor.key));
        let name = world
            .get::<Name>(mob)
            .map(|n| n.to_string())
            .unwrap_or_default();
        out.push(line(vec![
            ChatSegment::new(Span::Plain, format!("{name} ")),
            ChatSegment::new(Span::Bold, traitor.flavor.roundend_report.clone()),
        ]));
        let mut won = true;
        let mut objective_lines = Vec::new();
        for (i, objective) in traitor.objectives.iter().enumerate() {
            let done = objective.check_completion(world, &[mob], end);
            won &= done;
            objective_lines.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", objective.explanation)),
                if done {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Неудача.")
                },
            ]));
        }
        if traitor.uplink.is_some() {
            // `generate_render(FALSE)`: one `[name]` per entry, `x<n>` for repeats.
            let purchases: String = log
                .entries
                .iter()
                .map(|p| {
                    let times = if p.amount > 1 {
                        format!("x{}", p.amount)
                    } else {
                        String::new()
                    };
                    format!("[{}{times}]", p.name)
                })
                .collect();
            let mut text = format!("(использовал {} ТК) {purchases}", log.total_spent);
            if log.total_spent == 0 && won {
                // The `badass` icon of `icons/ui/antags/badass.dmi`.
                text.push_str(" [badass]");
            }
            out.push(line(vec![ChatSegment::new(Span::Plain, text)]));
        }
        out.extend(objective_lines);
        let role = TRAITOR_NAME.to_lowercase();
        out.push(line(vec![if won {
            ChatSegment::new(Span::GreenText, format!("{role} был успешен!"))
        } else {
            ChatSegment::new(Span::RedText, format!("{role} провалился!"))
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn employers_follow_the_ending_and_faction() {
        for seed in 0..100 {
            let mut rng = Rng::new(seed);
            let normal = pick_employer(false, &mut rng);
            assert!(!HIJACK_EMPLOYERS.contains(&normal), "{normal}");
            let hijack = pick_employer(true, &mut rng);
            assert!(!NORMAL_EMPLOYERS.contains(&hijack), "{hijack}");
        }
        let names = primary_objectives(
            &[Objective {
                kind: ObjectiveKind::Escape,
                explanation: "Сбегите".into(),
            }],
            &["Альфа".into()],
        );
        assert_eq!(
            names,
            [("DIRECTIVE АЛЬФА".to_owned(), "Сбегите".to_owned())]
        );
    }

    #[test]
    fn report_marks_objectives_green_or_red() {
        let mut world = World::new();
        let mob = world
            .spawn((Stat::Conscious, Name::new("Агент"), PurchaseLog::default()))
            .id();
        let victim = world.spawn(Stat::Conscious).id();
        let objectives = vec![
            Objective {
                kind: ObjectiveKind::Assassinate {
                    target: Some(victim),
                },
                explanation: "Убейте Боба.".into(),
            },
            Objective {
                kind: ObjectiveKind::Escape,
                explanation: "Сбегите.".into(),
            },
        ];
        world.entity_mut(mob).insert(Traitor {
            key: "p0".into(),
            employer: "MI13".into(),
            flavor: TraitorFlavor {
                roundend_report: "был агентом MI13.".into(),
                ..default()
            },
            objectives,
            ending: ObjectiveKind::Escape,
            uplink: Some(victim),
            code: None,
            unlock_text: String::new(),
            memory: String::new(),
        });
        world
            .get_mut::<PurchaseLog>(mob)
            .unwrap()
            .log("Full Syndicate Toolbox", 1);
        let report = roundend_report(&mut world, RoundEnd { force_ending: true });
        let text: Vec<String> = report.iter().map(ChatMessage::plain_text).collect();
        assert_eq!(text[0], "Предатели были:");
        assert_eq!(text[1], "p0 был Агент и выжил");
        assert_eq!(text[2], "Агент был агентом MI13.");
        assert_eq!(text[3], "(использовал 1 ТК) [Full Syndicate Toolbox]");
        assert_eq!(text[4], "Задача #1: Убейте Боба. Неудача.");
        assert_eq!(report[4].segments[2].span, Span::RedText);
        assert_eq!(text[5], "Задача #2: Сбегите. Успех!");
        assert_eq!(report[5].segments[2].span, Span::GreenText);
        assert_eq!(text[6], "предатель провалился!");
        world.entity_mut(victim).insert(Stat::Dead);
        let report = roundend_report(&mut world, RoundEnd { force_ending: true });
        assert_eq!(report[6].plain_text(), "предатель был успешен!");
    }
}
