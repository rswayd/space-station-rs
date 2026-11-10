//! The security HUD's examine block and its links (server), after tg
//! `/mob/living/carbon/human/get_hud_examine_info` + `get_sechud_examine_info`
//! (`carbon/examine.dm`) and the `hud=s` branch of `/mob/living/carbon/human/Topic`
//! (`human.dm`).
//!
//! A human with a security HUD examining another human sees its rank, wanted status, note and
//! links (`crate::examine::chat_link`): the status (a list prompt; "Арест" adds a HUD crime),
//! the record (a chat block), a citation (reason, then sum; the fined PDA is alerted), a crime,
//! a note. Every click and every answer is checked again on the server: the link is at most a
//! minute old, the user still wears a security HUD, is conscious and not the target, can use
//! it, and the ID in its ID slot has security access; the target's record is found again by its
//! visible name.
// shortcut: the medical HUD's part of the block and links (status edits, evaluation, quirks)
// belong to the medical group; no record photo links (no photo window without an item); no
// observer HUDs; emagged HUD glasses ("@%&ERROR_%$*" as the author) are not modelled.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};

use super::hud::{perp_name, wearer_huds};
use super::manifest::{Crime, Manifest, alert_owner, round_timestamp};
use super::*;
use crate::examine::{ChatLinkClicked, ChatLinkSystems, chat_link};
use crate::health::Stat;
use crate::tools::tell;

/// The links answer for `examine_time + 1 MINUTES`.
const LINK_SECS: f64 = 60.0;
/// `MAX_MESSAGE_LEN`.
const MAX_MESSAGE_LEN: usize = 1024;
/// `tgui_input_number(..., 50, max_fine, 5)`.
const CITATION_DEFAULT: f64 = 50.0;
const CITATION_MIN: f64 = 5.0;
const TOPIC_PREFIX: &str = "sechud";
const NO_ACCESS: &str = "ОШИБКА: Недостаточно доступа.";

/// A security HUD link's action (`href_list` keys of `hud=s`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudAction {
    Status,
    View,
    AddCitation,
    AddCrime,
    AddNote,
}

impl HudAction {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Status, "status"),
        (Self::View, "view"),
        (Self::AddCitation, "add_citation"),
        (Self::AddCrime, "add_crime"),
        (Self::AddNote, "add_note"),
    ];

    fn key(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(a, _)| *a == self)
            .map(|(_, k)| *k)
            .expect("every action has a key")
    }
}

/// `sechud:<action>:<target bits>`.
pub fn topic(action: HudAction, target: Entity) -> String {
    format!("{TOPIC_PREFIX}:{}:{}", action.key(), target.to_bits())
}

pub fn parse_topic(topic: &str) -> Option<(HudAction, Entity)> {
    let mut parts = topic.split(':');
    if parts.next()? != TOPIC_PREFIX {
        return None;
    }
    let key = parts.next()?;
    let action = HudAction::ALL.iter().find(|(_, k)| *k == key)?.0;
    let target = Entity::try_from_bits(parts.next()?.parse().ok()?)?;
    parts.next().is_none().then_some((action, target))
}

/// What a HUD prompt was asked for; on the prompt entity.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct HudPrompt {
    pub action: HudAction,
    pub target: Entity,
    pub record: u32,
    /// The citation's reason, once given (the sum is asked next).
    pub reason: Option<String>,
}

pub struct SecHudPlugin;

impl Plugin for SecHudPlugin {
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

/// tg `canUseHUD` (`MOBILITY_USE`): not stunned, paralysed or knocked out.
fn can_use_hud(world: &World, mob: Entity) -> bool {
    !crate::status_effects::knocked_out(world, mob)
        && !world
            .get::<crate::status_effects::StatusEffects>(mob)
            .is_some_and(|s| s.incapacitated())
}

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn unconscious_or_crit(world: &World, mob: Entity) -> bool {
    world
        .get::<Stat>(mob)
        .is_some_and(|s| *s != Stat::Conscious)
        || crate::status_effects::knocked_out(world, mob)
}

fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::human::HumanLook>(mob).is_some()
}

/// `allowed_access`: the registered name of the ID in `user`'s ID slot when it has security
/// access (`get_authentification_name`).
pub fn allowed_access(world: &World, user: Entity) -> Option<String> {
    let card = world
        .get::<crate::inventory::Inventory>(user)?
        .id
        .and_then(|id| crate::inventory::get_id(world, id))?;
    let access = world.get::<crate::inventory::IdCard>(card)?;
    if !access
        .access
        .iter()
        .any(|a| a == crate::id_access::ACCESS_SECURITY)
    {
        return None;
    }
    Some(
        world
            .get::<crate::outfit::IdRegistration>(card)
            .map(|r| r.registered_name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Unknown".to_owned()),
    )
}

/// Why a HUD action does nothing: silently, or with a line to the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Silent,
    Told(&'static str),
}

/// The checks of `Topic` (`hud=s`) for `user` acting on `target`, now: the record and the
/// author name (`allowed_access`).
pub fn check(world: &World, user: Entity, target: Entity) -> Result<(u32, String), Refusal> {
    if !is_human(world, user) || !is_human(world, target) || !wearer_huds(world, user).0 {
        return Err(Refusal::Silent);
    }
    if unconscious_or_crit(world, user) || user == target || !can_use_hud(world, user) {
        return Err(Refusal::Silent);
    }
    let author = allowed_access(world, user).ok_or(Refusal::Told(NO_ACCESS))?;
    let name = perp_name(world, target)
        .ok_or(Refusal::Told("ОШИБКА: Невозможно идентифицировать цель."))?;
    let record = world
        .resource::<Manifest>()
        .find(&name)
        .map(|r| r.id)
        .ok_or(Refusal::Told(
            "ОШИБКА: Невозможно найти запись в базе данных по цели.",
        ))?;
    Ok((record, author))
}

/// `get_hud_examine_info` of `target` for `examiner` with a security HUD: the block, its links
/// made for `client`. Empty without a security HUD.
pub fn examine_messages(
    world: &mut World,
    client: Entity,
    examiner: Entity,
    target: Entity,
) -> Vec<ChatMessage> {
    if examiner == target
        || !is_human(world, target)
        || !wearer_huds(world, examiner).0
        || unconscious_or_crit(world, examiner)
    {
        return Vec::new();
    }
    let Some(name) = perp_name(world, target) else {
        return Vec::new();
    };
    let record = world
        .get_resource::<Manifest>()
        .and_then(|m| m.find(&name).cloned());
    let human = is_human(world, examiner);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::Info,
        segments,
    };
    let plain = |text: String| ChatSegment::new(Span::Plain, text);
    let link = |world: &mut World, action: HudAction, text: String| {
        ChatSegment::new(
            Span::Link(chat_link(world, client, topic(action, target))),
            text,
        )
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        "Анализ безопасности",
    )])];
    if let Some(r) = &record {
        out.push(line(vec![plain(format!("Должность: {}", r.rank))]));
    }
    let wanted = record
        .as_ref()
        .map_or(WANTED_NONE.to_owned(), |r| r.wanted_status.clone());
    let note = record
        .as_ref()
        .map(|r| r.security_note.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Пусто.".to_owned());
    out.push(line(if human {
        vec![
            plain("Криминальный статус: ".to_owned()),
            link(world, HudAction::Status, format!("[{wanted}]")),
        ]
    } else {
        vec![plain(format!("Криминальный статус: {wanted}"))]
    }));
    out.push(line(vec![plain(format!("Важные заметки: {note}"))]));
    out.push(line(vec![
        plain("Записи охраны: ".to_owned()),
        link(world, HudAction::View, "[Показать]".to_owned()),
    ]));
    if human {
        out.push(line(vec![
            link(world, HudAction::AddCitation, "[Добавить штраф]".to_owned()),
            link(
                world,
                HudAction::AddCrime,
                "[Добавить преступление]".to_owned(),
            ),
            link(
                world,
                HudAction::AddNote,
                "[Добавить примечание]".to_owned(),
            ),
        ]));
    }
    out
}

/// The `view` block of a record.
pub fn record_lines(record: &super::manifest::CrewRecord) -> Vec<String> {
    let mut lines = vec![
        format!("Имя: {}", record.name),
        format!("Криминальный статус: {}", record.wanted_status),
        format!("Штрафы: {}", record.citations.len()),
        format!(
            "Примечания: {}",
            if record.security_note.is_empty() {
                "Отсутствуют"
            } else {
                &record.security_note
            }
        ),
        format!("Уголовное дело: {} инцидентов", record.crimes.len()),
    ];
    for crime in &record.crimes {
        if !crime.valid {
            lines.push("-- РЕДАКТИРОВАНО --".to_owned());
            continue;
        }
        lines.push(format!("Преступление: {}", crime.name));
        lines.push(format!("Описание: {}", crime.details));
        lines.push(format!("{} добавляет в {}", crime.author, crime.time));
    }
    lines
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

/// A click on a HUD link by `user` (`client`'s mob), `age` seconds after the examine.
pub fn clicked(
    world: &mut World,
    client: Entity,
    user: Entity,
    action: HudAction,
    target: Entity,
    age: f64,
) {
    if world.get_entity(target).is_err() {
        return;
    }
    if age > LINK_SECS {
        tell(world, client, Span::Notice, "Уже поздно использовать это!");
        return;
    }
    let record = match check(world, user, target) {
        Ok((record, _)) => record,
        Err(Refusal::Told(text)) => {
            tell(world, client, Span::Warning, text);
            return;
        }
        Err(Refusal::Silent) => return,
    };
    let (title, message, kind) = match action {
        HudAction::View => {
            if let Some(r) = world.resource::<Manifest>().get(record).cloned() {
                for text in record_lines(&r) {
                    tell(world, client, Span::Info, text);
                }
            }
            return;
        }
        HudAction::Status => {
            let current = world
                .resource::<Manifest>()
                .get(record)
                .map(|r| r.wanted_status.clone());
            (
                "Охранный HUD",
                "Укажите новый криминальный статус для этой личности.",
                PromptKind::List {
                    items: WANTED_STATUSES.map(str::to_owned).to_vec(),
                    default: current,
                },
            )
        }
        HudAction::AddCitation => ("Охранный HUD", "Причина штрафа", text_kind(false)),
        HudAction::AddCrime => ("Охранный HUD", "Название преступления", text_kind(false)),
        HudAction::AddNote => ("Записи охраны", "Примечание для охраны", text_kind(true)),
    };
    let prompt = ask_world(world, Ask::new(client, user, title, message, kind));
    world.entity_mut(prompt).insert(HudPrompt {
        action,
        target,
        record,
        reason: None,
    });
}

fn text_kind(multiline: bool) -> PromptKind {
    PromptKind::Text {
        default: String::new(),
        max_length: MAX_MESSAGE_LEN as u32,
        multiline,
    }
}

fn hud_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, user, answer) in answers {
        let Some(asked) = world.get::<HudPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(answer) = answer else {
            continue;
        };
        answered(world, client, user, &asked, answer);
    }
}

/// The answer to a HUD prompt, applied after every check again (tg checks the record, the HUD,
/// `canUseHUD` and the captured access; here the whole `Topic` check runs anew).
pub fn answered(
    world: &mut World,
    client: Entity,
    user: Entity,
    asked: &HudPrompt,
    answer: PromptAnswer,
) {
    let Ok((record, author)) = check(world, user, asked.target) else {
        return;
    };
    if record != asked.record {
        return;
    }
    let user_name = crate::examine::name_of(world, user);
    let time = round_timestamp(world);
    match (asked.action, answer) {
        (HudAction::Status, PromptAnswer::Item(i)) => {
            let Some(&status) = WANTED_STATUSES.get(i) else {
                return;
            };
            let mut manifest = world.resource_mut::<Manifest>();
            let id = manifest.next_id();
            let Some(r) = manifest.get_mut(record) else {
                return;
            };
            if status == WANTED_ARREST {
                r.crimes.push(Crime {
                    id,
                    name: "Crime".to_owned(),
                    details: "Установлен с помощью охранного HUD.".to_owned(),
                    author: user_name,
                    author_mob: Some(user),
                    time,
                    valid: true,
                    voider: None,
                    fine: None,
                    paid: 0,
                });
            }
            r.wanted_status = status.to_owned();
        }
        (HudAction::AddCitation, PromptAnswer::Text(reason)) if asked.reason.is_none() => {
            let reason = super::console::sanitize(&reason, MAX_MESSAGE_LEN);
            if reason.is_empty() {
                return;
            }
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    user,
                    "Охранный HUD",
                    "Размер штрафа",
                    PromptKind::Number {
                        default: CITATION_DEFAULT,
                        min: CITATION_MIN,
                        max: f64::from(super::console::MAX_FINE),
                        round: true,
                    },
                ),
            );
            world.entity_mut(prompt).insert(HudPrompt {
                reason: Some(reason),
                ..asked.clone()
            });
        }
        (HudAction::AddCitation, PromptAnswer::Number(fine)) => {
            let Some(reason) = asked.reason.clone() else {
                return;
            };
            if fine.fract() != 0.0 || fine <= 0.0 || fine > f64::from(super::console::MAX_FINE) {
                return;
            }
            let fine = fine as u32;
            let mut manifest = world.resource_mut::<Manifest>();
            let id = manifest.next_id();
            let Some(r) = manifest.get_mut(record) else {
                return;
            };
            r.citations.push(Crime {
                id,
                name: reason.clone(),
                details: "Детали не указаны.".to_owned(),
                author,
                author_mob: Some(user),
                time,
                valid: true,
                voider: None,
                fine: Some(fine),
                paid: 0,
            });
            let name = r.name.clone();
            alert_owner(
                world,
                asked.target,
                &name,
                &format!(
                    "Вам был выставлен штраф в размере {fine}{} за '{reason}'. Штраф может быть \
                     оплачен у отдела безопасности.",
                    crate::cargo::MONEY_SYMBOL
                ),
            );
        }
        (HudAction::AddCrime, PromptAnswer::Text(name)) => {
            let name = super::console::sanitize(&name, MAX_MESSAGE_LEN);
            if name.is_empty() {
                return;
            }
            let mut manifest = world.resource_mut::<Manifest>();
            let id = manifest.next_id();
            let Some(r) = manifest.get_mut(record) else {
                return;
            };
            r.crimes.push(Crime {
                id,
                name,
                details: "Детали не указаны.".to_owned(),
                author,
                author_mob: Some(user),
                time,
                valid: true,
                voider: None,
                fine: None,
                paid: 0,
            });
            tell(
                world,
                client,
                Span::Notice,
                "Преступление успешно добавлено.",
            );
        }
        (HudAction::AddNote, PromptAnswer::Text(note)) => {
            let note = super::console::sanitize(&note, MAX_MESSAGE_LEN);
            if note.is_empty() {
                return;
            }
            if let Some(r) = world.resource_mut::<Manifest>().get_mut(record) {
                r.security_note = note;
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{IdCard, Inventory};
    use crate::records::manifest::Joiner;
    use ss13_mapload::DmItem;
    use ss13_net::chat::server::SendChat;

    struct Setup {
        world: World,
        officer: Entity,
        perp: Entity,
        record: u32,
    }

    fn setup(access: &[&str]) -> Setup {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<SendChat>>();
        let mut manifest = Manifest::default();
        let record = manifest.inject(Joiner {
            name: "Боб",
            assignment: "Ассистент",
            job: None,
            mob: Entity::PLACEHOLDER,
        });
        world.insert_resource(manifest);
        let glasses = world
            .spawn(DmItem {
                path: "/obj/item/clothing/glasses/hud/security/sunglasses".into(),
                ..default()
            })
            .id();
        let card = world
            .spawn((
                IdCard {
                    access: access.iter().map(|a| (*a).to_owned()).collect(),
                },
                crate::outfit::IdRegistration {
                    registered_name: "Алиса".into(),
                    assignment: "Офицер".into(),
                    job: "Офицер".into(),
                },
            ))
            .id();
        let officer = world
            .spawn((
                Name::new("Алиса"),
                crate::human::HumanLook::random_assistant(1),
                Inventory {
                    id: Some(card),
                    eyes: Some(glasses),
                    ..default()
                },
            ))
            .id();
        let perp = world
            .spawn((
                Name::new("Боб"),
                crate::human::HumanLook::random_assistant(2),
            ))
            .id();
        Setup {
            world,
            officer,
            perp,
            record,
        }
    }

    fn prompt(s: &Setup, action: HudAction) -> HudPrompt {
        HudPrompt {
            action,
            target: s.perp,
            record: s.record,
            reason: None,
        }
    }

    #[test]
    fn topics_round_trip_and_reject_junk() {
        let e = Entity::from_raw_u32(7).unwrap();
        assert_eq!(
            parse_topic(&topic(HudAction::AddCrime, e)),
            Some((HudAction::AddCrime, e))
        );
        assert_eq!(parse_topic("sechud:status"), None);
        assert_eq!(parse_topic("sechud:steal:1"), None);
        assert_eq!(parse_topic("other:status:1"), None);
    }

    #[test]
    fn checks_need_the_hud_access_and_a_record() {
        let mut s = setup(&["security"]);
        assert_eq!(
            check(&s.world, s.officer, s.perp),
            Ok((s.record, "Алиса".to_owned()))
        );
        // Not on oneself.
        assert_eq!(check(&s.world, s.officer, s.officer), Err(Refusal::Silent));
        // No security access on the worn ID.
        let t = setup(&["theatre"]);
        assert_eq!(
            check(&t.world, t.officer, t.perp),
            Err(Refusal::Told(NO_ACCESS))
        );
        // A target without a record.
        *s.world.get_mut::<Name>(s.perp).unwrap() = Name::new("Ева");
        assert!(matches!(
            check(&s.world, s.officer, s.perp),
            Err(Refusal::Told(_))
        ));
        // No HUD: nothing.
        s.world.get_mut::<Inventory>(s.officer).unwrap().eyes = None;
        assert_eq!(check(&s.world, s.officer, s.perp), Err(Refusal::Silent));
    }

    #[test]
    fn answers_edit_the_record_after_the_checks() {
        let mut s = setup(&["security"]);
        let client = Entity::PLACEHOLDER;
        let (officer, record) = (s.officer, s.record);
        let rec = |w: &World| w.resource::<Manifest>().get(record).unwrap().clone();
        // Arrest by HUD adds the HUD crime.
        let arrest = WANTED_STATUSES
            .iter()
            .position(|s| *s == WANTED_ARREST)
            .unwrap();
        let p = prompt(&s, HudAction::Status);
        answered(
            &mut s.world,
            client,
            officer,
            &p,
            PromptAnswer::Item(arrest),
        );
        let r = rec(&s.world);
        assert_eq!(r.wanted_status, WANTED_ARREST);
        assert_eq!(r.crimes.len(), 1);
        assert_eq!(r.crimes[0].author, "Алиса");
        // A note; a crime authored by the ID's name.
        let p = prompt(&s, HudAction::AddNote);
        answered(
            &mut s.world,
            client,
            officer,
            &p,
            PromptAnswer::Text("Опасен".into()),
        );
        let p = prompt(&s, HudAction::AddCrime);
        answered(
            &mut s.world,
            client,
            officer,
            &p,
            PromptAnswer::Text("Кража".into()),
        );
        let r = rec(&s.world);
        assert_eq!(r.security_note, "Опасен");
        assert_eq!(r.crimes[1].name, "Кража");
        // A citation needs its reason first and a sum in range.
        let p = prompt(&s, HudAction::AddCitation);
        answered(
            &mut s.world,
            client,
            officer,
            &p,
            PromptAnswer::Number(50.0),
        );
        let with_reason = HudPrompt {
            reason: Some("Мусор".into()),
            ..prompt(&s, HudAction::AddCitation)
        };
        answered(
            &mut s.world,
            client,
            officer,
            &with_reason,
            PromptAnswer::Number(f64::from(crate::records::console::MAX_FINE) + 1.0),
        );
        assert!(rec(&s.world).citations.is_empty());
        answered(
            &mut s.world,
            client,
            officer,
            &with_reason,
            PromptAnswer::Number(50.0),
        );
        assert_eq!(rec(&s.world).citations[0].fine, Some(50));
        // The HUD taken off between the question and the answer: nothing changes.
        s.world.get_mut::<Inventory>(officer).unwrap().eyes = None;
        let p = prompt(&s, HudAction::AddNote);
        answered(
            &mut s.world,
            client,
            officer,
            &p,
            PromptAnswer::Text("Поздно".into()),
        );
        assert_eq!(rec(&s.world).security_note, "Опасен");
    }
}
