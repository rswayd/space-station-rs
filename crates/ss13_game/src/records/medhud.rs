//! The medical HUD's part of the examine block and its links (server), after tg
//! `/mob/living/carbon/get_medhud_examine_info` (`carbon/examine.dm`), the human override
//! (`human/examine.dm`: broken suit sensors) and the `hud=m` branch of
//! `/mob/living/carbon/human/Topic` (`human.dm`).
//!
//! The block ([`lines`], put together with the security part by
//! [`super::sechud::examine_messages`]): the visible cybernetic implants, the record's physical
//! and mental status as links ("[Запись отсутствует]" twice without a record), the evaluation
//! and quirks links, and the jumpsuit's broken sensors. A click is checked again on the server
//! (link at most a minute old, the user a human with a medical HUD, conscious, able to use it,
//! not the target); the evaluation needs nothing more, the status edits and the quirks need
//! `ACCESS_MEDICAL` on the ID in the ID slot. A status answer is applied after the same checks.
// shortcut: the quirk list shows the quirks' names (the ported quirks have no
// `medical_record_text` or `QUIRK_HIDE_FROM_SCAN`); every implant counts as visible (no
// `ORGAN_HIDDEN` implants are ported).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};

use super::hud::{perp_name, wearer_huds};
use super::manifest::{CrewRecord, Manifest};
use super::sechud::{LINK_SECS, can_use_hud, is_human, unconscious_or_crit};
use super::*;
use crate::examine::{ChatLinkClicked, ChatLinkSystems, chat_link};
use crate::tools::tell;
use crate::zone::BodyZone;

const TOPIC_PREFIX: &str = "medhud";
/// `ACCESS_MEDICAL`.
const ACCESS_MEDICAL: &str = "medical";
const NO_ACCESS: &str = "ОШИБКА: Недостаточно доступа.";
const TITLE: &str = "Медицинский HUD";

/// A medical HUD link's action (`href_list` keys of `hud=m`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MedAction {
    Physical,
    Mental,
    Evaluation,
    Quirk,
}

impl MedAction {
    const ALL: [(Self, &'static str); 4] = [
        (Self::Physical, "physical_status"),
        (Self::Mental, "mental_status"),
        (Self::Evaluation, "evaluation"),
        (Self::Quirk, "quirk"),
    ];

    fn key(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(a, _)| *a == self)
            .map(|(_, k)| *k)
            .expect("every action has a key")
    }
}

/// `medhud:<action>:<target bits>`.
pub fn topic(action: MedAction, target: Entity) -> String {
    format!("{TOPIC_PREFIX}:{}:{}", action.key(), target.to_bits())
}

pub fn parse_topic(topic: &str) -> Option<(MedAction, Entity)> {
    let mut parts = topic.split(':');
    if parts.next()? != TOPIC_PREFIX {
        return None;
    }
    let key = parts.next()?;
    let action = MedAction::ALL.iter().find(|(_, k)| *k == key)?.0;
    let target = Entity::try_from_bits(parts.next()?.parse().ok()?)?;
    parts.next().is_none().then_some((action, target))
}

/// A status prompt of the medical HUD; on the prompt entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MedHudPrompt {
    pub physical: bool,
    pub target: Entity,
    pub record: u32,
}

pub struct MedHudPlugin;

impl Plugin for MedHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                hud_links.after(ChatLinkSystems),
                hud_answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(PromptSystems::Answer),
            ),
        );
    }
}

/// `get_medhud_examine_info` of `target` (and its human override), its links made for
/// `client`.
pub fn lines(
    world: &mut World,
    client: Entity,
    target: Entity,
    record: Option<&CrewRecord>,
) -> Vec<Vec<ChatSegment>> {
    let mut out = Vec::new();
    let cybers = implant_names(world, target);
    if !cybers.is_empty() {
        out.push(vec![ChatSegment::new(
            Span::Notice,
            "Обнаружены кибернетические модификации:",
        )]);
        out.push(vec![ChatSegment::new(
            Span::Notice,
            english_list(&cybers, ", и"),
        )]);
    }
    let link = |world: &mut World, action: MedAction, text: String| {
        ChatSegment::new(
            Span::Link(chat_link(world, client, topic(action, target))),
            text,
        )
    };
    match record {
        Some(r) => {
            out.push(vec![link(
                world,
                MedAction::Physical,
                format!("[{}]", r.physical_status),
            )]);
            out.push(vec![link(
                world,
                MedAction::Mental,
                format!("[{}]", r.mental_status),
            )]);
        }
        None => {
            for _ in 0..2 {
                out.push(vec![ChatSegment::new(Span::Plain, "[Запись отсутствует]")]);
            }
        }
    }
    out.push(vec![link(
        world,
        MedAction::Evaluation,
        "[Медицинское обследование]".to_owned(),
    )]);
    out.push(vec![link(
        world,
        MedAction::Quirk,
        "[Показать черты]".to_owned(),
    )]);
    // `human/get_medhud_examine_info`: `get_sensor_text()` is silent unless the sensors broke.
    let broken = world
        .get::<crate::inventory::Inventory>(target)
        .and_then(|i| i.uniform)
        .and_then(|u| world.get::<crate::clothing::sensors::SuitSensor>(u))
        .is_some_and(|s| s.has == crate::clothing::sensors::BROKEN_SENSORS);
    if broken {
        out.push(vec![
            ChatSegment::new(Span::Plain, "Статус датчиков: "),
            ChatSegment::new(Span::Warning, "Не работают: Почините с помощью проводов"),
        ]);
    }
    out
}

/// The names of `mob`'s cybernetic implants.
fn implant_names(world: &World, mob: Entity) -> Vec<String> {
    let Some(organs) = world.get::<crate::medical::organs::Organs>(mob) else {
        return Vec::new();
    };
    organs
        .iter()
        .filter(|&e| world.get::<crate::medical::cyberimps::Implant>(e).is_some())
        .map(|e| crate::examine::name_of(world, e))
        .collect()
}

/// DM `english_list(list, and_text = and_text)`.
fn english_list(items: &[String], and_text: &str) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{}{and_text} {last}", rest.join(", ")),
    }
}

/// The checks every `hud=m` click starts with, now: `user` a conscious human with a medical
/// HUD that can use it, `target` another human.
fn hud_ok(world: &World, user: Entity, target: Entity) -> bool {
    is_human(world, user)
        && is_human(world, target)
        && user != target
        && wearer_huds(world, user).1
        && !unconscious_or_crit(world, user)
        && can_use_hud(world, user)
}

/// `wear_id.GetAccess()` has `ACCESS_MEDICAL`.
fn medical_access(world: &World, user: Entity) -> bool {
    world
        .get::<crate::inventory::Inventory>(user)
        .and_then(|i| i.id)
        .and_then(|id| crate::inventory::get_id(world, id))
        .and_then(|card| world.get::<crate::inventory::IdCard>(card))
        .is_some_and(|c| c.access.iter().any(|a| a == ACCESS_MEDICAL))
}

fn record_of(world: &World, target: Entity) -> Option<u32> {
    let name = perp_name(world, target)?;
    world.get_resource::<Manifest>()?.find(&name).map(|r| r.id)
}

fn hud_links(world: &mut World, mut cursor: Local<MessageCursor<ChatLinkClicked>>) {
    let Some(messages) = world.get_resource::<Messages<ChatLinkClicked>>() else {
        return;
    };
    let clicks: Vec<ChatLinkClicked> = cursor.read(messages).cloned().collect();
    for click in clicks {
        let Some((action, target)) = parse_topic(&click.topic) else {
            continue;
        };
        let Some(user) = world.resource::<crate::server::Players>().mob(click.client) else {
            continue;
        };
        clicked(world, click.client, user, action, target, click.age);
    }
}

/// A click on a medical HUD link by `user` (`client`'s mob), `age` seconds after the examine.
pub fn clicked(
    world: &mut World,
    client: Entity,
    user: Entity,
    action: MedAction,
    target: Entity,
    age: f64,
) {
    if world.get_entity(target).is_err() || !hud_ok(world, user, target) {
        return;
    }
    if age > LINK_SECS {
        tell(world, client, Span::Notice, "Уже поздно использовать это!");
        return;
    }
    if action == MedAction::Evaluation {
        for (span, text) in evaluation(world, target) {
            tell(world, client, span, text);
        }
        return;
    }
    if !medical_access(world, user) {
        tell(world, client, Span::Warning, NO_ACCESS);
        return;
    }
    let record = record_of(world, target);
    match action {
        MedAction::Physical | MedAction::Mental => {
            let Some(record) = record else {
                return;
            };
            let physical = action == MedAction::Physical;
            let current = world.resource::<Manifest>().get(record).map(|r| {
                if physical {
                    r.physical_status.clone()
                } else {
                    r.mental_status.clone()
                }
            });
            let (message, items) = if physical {
                (
                    "Укажите новый физический статус для этой личности.",
                    PHYSICAL_STATUSES,
                )
            } else {
                (
                    "Укажите новый ментальный статус для этой личности.",
                    MENTAL_STATUSES,
                )
            };
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    user,
                    TITLE,
                    message,
                    PromptKind::List {
                        items: items.map(str::to_owned).to_vec(),
                        default: current,
                    },
                ),
            );
            world.entity_mut(prompt).insert(MedHudPrompt {
                physical,
                target,
                record,
            });
        }
        MedAction::Quirk => {
            let names: Vec<&str> = world
                .get::<crate::quirks::Quirks>(target)
                .map(|q| q.0.iter().map(|q| q.def().name).collect())
                .unwrap_or_default();
            if names.is_empty() {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Физиологические черты не были обнаружены.",
                );
            } else {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Обнаружены физиологические черты:",
                );
                tell(world, client, Span::Notice, names.join(", "));
            }
        }
        MedAction::Evaluation => {}
    }
}

/// The `evaluation` report: brute and burn per bodypart, suffocation, poisoning.
pub fn evaluation(world: &World, target: Entity) -> Vec<(Span, String)> {
    let Some(health) = world.get::<crate::health::Health>(target) else {
        return Vec::new();
    };
    if health.brute <= 0.0 && health.burn <= 0.0 && health.oxy <= 0.0 && health.tox < 20.0 {
        return vec![(
            Span::Notice,
            "Не обнаружены внешние повреждения.".to_owned(),
        )];
    }
    let mut out = Vec::new();
    let parts = |out: &mut Vec<(Span, String)>, burn: bool| {
        for zone in BodyZone::SCAN_ORDER {
            if health.missing & (1 << zone.index()) != 0 {
                continue;
            }
            let part = health.parts[zone.index()];
            let dam = if burn { part.burn } else { part.brute };
            if dam <= 0.0 {
                continue;
            }
            let (span, status) = match (dam > 40.0, dam > 20.0, burn) {
                (true, _, false) => (Span::BoldWarning, "имеет серьезные повреждения!"),
                (true, _, true) => (Span::BoldWarning, "имеет серьезные ожоги!"),
                (false, true, false) => (Span::Danger, "имеет сильные повреждения."),
                (false, true, true) => (Span::Danger, "имеет сильные ожоги."),
                (false, false, false) => (Span::Notice, "имеет незначительные ушибы."),
                (false, false, true) => (Span::Notice, "имеет незначительные ожоги."),
            };
            out.push((
                span,
                format!("{} {status}", ss13_shared::capitalize(zone.ru().0)),
            ));
        }
    };
    if health.brute > 0.0 {
        out.push((Span::Bold, "Анализ ушибов на коже:".to_owned()));
        parts(&mut out, false);
    }
    if health.burn > 0.0 {
        out.push((Span::Bold, "Анализ ожогов на коже:".to_owned()));
        parts(&mut out, true);
    }
    if health.oxy > 0.0 {
        out.push((
            Span::Danger,
            "Пациент имеет признаки удушья, может быть потребована экстренная помощь.".to_owned(),
        ));
    }
    if health.tox > 20.0 {
        out.push((
            Span::Danger,
            "Собранные данные не согласуются с результатами анализа, возможная причина: \
             отравление."
                .to_owned(),
        ));
    }
    out
}

fn hud_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, user, answer) in answers {
        let Some(asked) = world.get::<MedHudPrompt>(prompt).copied() else {
            continue;
        };
        if let Some(PromptAnswer::Item(i)) = answer {
            answered(world, user, asked, i);
        }
    }
}

/// A status picked: applied when the user can still use the HUD on the target, still has
/// medical access and the target's record is still the one asked about.
pub fn answered(world: &mut World, user: Entity, asked: MedHudPrompt, item: usize) {
    if !hud_ok(world, user, asked.target)
        || !medical_access(world, user)
        || record_of(world, asked.target) != Some(asked.record)
    {
        return;
    }
    let statuses: &[&str] = if asked.physical {
        &PHYSICAL_STATUSES
    } else {
        &MENTAL_STATUSES
    };
    let Some(&status) = statuses.get(item) else {
        return;
    };
    let mut manifest = world.resource_mut::<Manifest>();
    let Some(r) = manifest.get_mut(asked.record) else {
        return;
    };
    if asked.physical {
        r.physical_status = status.to_owned();
    } else {
        r.mental_status = status.to_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_round_trip() {
        let e = Entity::from_bits(42);
        for (action, _) in MedAction::ALL {
            assert_eq!(parse_topic(&topic(action, e)), Some((action, e)));
        }
        assert_eq!(parse_topic("sechud:status:42"), None);
        assert_eq!(parse_topic("medhud:quirk:42:extra"), None);
    }

    /// The `evaluation` report follows tg's thresholds per bodypart and type.
    #[test]
    fn evaluation_reports_by_thresholds() {
        let mut world = World::new();
        let healthy = world.spawn(crate::health::Health::default()).id();
        assert_eq!(
            evaluation(&world, healthy),
            vec![(
                Span::Notice,
                "Не обнаружены внешние повреждения.".to_owned()
            )]
        );
        let mut health = crate::health::Health::default();
        health.parts[BodyZone::Head.index()].brute = 45.0;
        health.parts[BodyZone::LArm.index()].burn = 5.0;
        health.brute = 45.0;
        health.burn = 5.0;
        health.tox = 25.0;
        let hurt = world.spawn(health).id();
        let report = evaluation(&world, hurt);
        assert!(report.contains(&(
            Span::BoldWarning,
            "Голова имеет серьезные повреждения!".to_owned()
        )));
        assert!(report.contains(&(
            Span::Notice,
            "Левая рука имеет незначительные ожоги.".to_owned()
        )));
        assert!(report.iter().any(|(_, t)| t.contains("отравление")));
        assert!(!report.iter().any(|(_, t)| t.contains("удушья")));
    }

    #[test]
    fn implants_list_like_english_list() {
        let list = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(english_list(&list(&["a"]), ", и"), "a");
        assert_eq!(english_list(&list(&["a", "b"]), ", и"), "a, и b");
        assert_eq!(english_list(&list(&["a", "b", "c"]), ", и"), "a, b, и c");
    }
}
