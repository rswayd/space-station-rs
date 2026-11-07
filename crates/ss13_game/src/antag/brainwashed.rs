//! Brainwashing after tg `code/modules/antagonists/brainwashing/brainwashing.dm` (`brainwash()`,
//! `/datum/antagonist/brainwashed`, `/datum/objective/brainwashing`) and the brainwashing
//! surgery (`code/modules/surgery/operations/operation_brainwash.dm`).
//!
//! - [`brainwash`]: directives become the victim's objectives (`completed = TRUE`); a victim
//!   already brainwashed gets them added and is greeted again. The victim gets the `brainwashed`
//!   antag HUD icon and the `AntagInfoBrainwashed` panel (reopened by the antag info button).
//! - **The surgery** (`crate::medical::surgery`, unlocked by the brainwashing disk): on an
//!   organic brain with the skin open, the organs cut and the bone sawed. `Op::Brainwash` first
//!   asks the surgeon for the directive ([`ask_directive`], `pre_preop`); the answer is checked
//!   again (the same tool in hand, next to the patient, able) before the operation starts. On
//!   success ([`surgical_brainwash`]) a patient without a player, or with a mindshield
//!   (`TRAIT_UNCONVERTABLE`), resists; otherwise the directive is implanted. `Op::BrainwashSleeper`
//!   implants a random sleeper agent directive. A failure deals 40 brain damage.
//! - Brain surgery and lobotomy cure it ([`remove_brainwashed`], `farewell`).
//! - [`roundend_report`]: each victim and its directives.
// shortcut: no deadchat broadcast, admin messages, April Fools line or stinger sound; the
// sleeper's conspiracies phobia is not ported (no phobias); no `mechanic` reprogramming of
// robotic brains; no admin "brainwash" verb; a body whose player is gone counts as mindless.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};

use super::objectives::RoundEnd;
use super::{AntagHudData, AntagHudUi, AntagInfoBrainwashedUi, BrotherInfo, ObjectiveInfo};
use crate::health::Stat;
use crate::inventory::Inventory;
use crate::medical::surgery::Op;
use crate::server::PlayerMob;
use crate::tools::{Actor, tell};
use crate::zone::TargetZone;

/// `name` ("\improper Brainwashed Victim").
pub const ANTAG_NAME: &str = "Жертва промывания мозгов";
/// `roundend_category`.
pub const ROUNDEND_CATEGORY: &str = "Жертвы с промытыми мозгами";
const HUD_STATE: &str = "brainwashed";
const ANTAG_HUD_DMI: &str = "icons/mob/huds/antag_hud.dmi";
const OBJECTIVE_NAME: &str = "Задача";
/// `MAX_MESSAGE_LEN`.
pub const MAX_DIRECTIVE_LEN: u32 = 1024;
/// `on_brainwash` without a directive.
const NO_DIRECTIVE: &str = "Oooo no objective set somehow report this to an admin";
/// `/datum/surgery_operation/organ/brainwash/sleeper` `possible_objectives`.
pub const SLEEPER_DIRECTIVES: [&str; 12] = [
    "Вы любите Синдикат.",
    "Не доверяйте Нанотрейзен.",
    "Капитан - унатх.",
    "Нанотрейзен не настоящий.",
    "Они подкладывают что-то в еду, чтобы вы забыли.",
    "Только вы настоящая личность на этой станции.",
    "На станции было бы намного лучше, если бы больше людей кричали. Кто-то должен что-то с этим \
     сделать.",
    "У здешних глав только дурные намерения по отношению к команде.",
    "Помогаете экипажу? Что они вообще когда-нибудь делали для вас?",
    "Твоя сумка стала легче на ощупь? Держу пари, те парни из службы безопасности что-то из нее \
     украли. Пойди и верни это.",
    "Командование некомпетентно, кто-то, обладающий РЕАЛЬНОЙ властью, должен взять это на себя.",
    "Киборги и ИИ преследуют вас. Что они планируют?",
];

/// On a brainwashed body (`/datum/antagonist/brainwashed`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Brainwashed {
    /// The player's key, for the round-end report.
    pub key: String,
    /// The directives (`/datum/objective/brainwashing` explanations).
    pub objectives: Vec<String>,
}

impl Brainwashed {
    fn info(&self) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: self
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.clone(),
                    complete: true,
                })
                .collect(),
        }
    }
}

/// On the surgeon: the directive asked for the brainwashing operation in progress
/// (`operation_args[OPERATION_OBJECTIVE]`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct BrainwashDirective(pub String);

/// On the directive prompt: the operation it starts.
#[derive(Component, Clone, Copy, Debug)]
struct DirectivePrompt {
    patient: Entity,
    zone: TargetZone,
    tool: Option<Entity>,
}

impl ServerUi for AntagInfoBrainwashedUi {
    type Source = &'static Brainwashed;
    type UserParam = ();
    type Act = ();

    fn static_data(victim: &Brainwashed) -> BrotherInfo {
        victim.info()
    }

    fn data(_: &Brainwashed, _: Entity, _: &()) {}
}

pub struct BrainwashedPlugin;

impl Plugin for BrainwashedPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AntagInfoBrainwashedUi>().add_systems(
            Update,
            (open_info, directive_answers.after(PromptSystems::Answer)).in_set(UiSystems::Handle),
        );
    }
}

use crate::tools::tell_mob as chat_to;

/// `/proc/brainwash(victim, directives)`. `false` when there is nothing to add or no body.
pub fn brainwash(world: &mut World, mob: Entity, directives: Vec<String>) -> bool {
    let directives: Vec<String> = directives
        .into_iter()
        .map(|d| d.trim().to_owned())
        .filter(|d| !d.is_empty())
        .collect();
    if directives.is_empty() || world.get_entity(mob).is_err() {
        return false;
    }
    let victim = match world.get::<Brainwashed>(mob).cloned() {
        Some(mut victim) => {
            victim.objectives.extend(directives.iter().cloned());
            victim
        }
        None => {
            let key = world
                .get::<PlayerMob>(mob)
                .and_then(|p| world.get::<ClientToken>(p.client))
                .map(|t| t.0.clone())
                .unwrap_or_default();
            let hud_icon = world
                .get_resource::<IconIndex>()
                .and_then(|i| i.get(ANTAG_HUD_DMI, HUD_STATE));
            // An antagonist already drawn with its own icon keeps it.
            if world.get::<AntagHudData>(mob).is_none() {
                world.entity_mut(mob).insert(AntagHudData {
                    hud_icon,
                    ..default()
                });
            }
            Brainwashed {
                key,
                objectives: directives.clone(),
            }
        }
    };
    world.entity_mut(mob).insert(victim.clone());
    greet(world, mob, &victim);
    info!("{mob} has been brainwashed with the following objectives: {directives:?}");
    true
}

/// `greet` + `announce_objectives` + the panel.
fn greet(world: &mut World, mob: Entity, victim: &Brainwashed) {
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    // shortcut: `span_big` is drawn bold (no big span in the chat).
    tell(world, client, Span::Bold, format!("Вы - {ANTAG_NAME}."));
    announce_objectives(world, client, &victim.objectives);
    world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    world.write_message(OpenUi::new::<AntagInfoBrainwashedUi>(
        client, mob, mob, ANTAG_NAME,
    ));
}

/// `/datum/mind/proc/announce_objectives`.
fn announce_objectives(world: &mut World, client: Entity, objectives: &[String]) {
    tell(world, client, Span::Notice, "Ваши текущие задачи:");
    for (i, objective) in objectives.iter().enumerate() {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                    ChatSegment::new(Span::Plain, format!(": {objective}")),
                ],
            },
        });
    }
}

/// `remove_antag_datum(/datum/antagonist/brainwashed)` (`farewell`). `false` if `mob` was not
/// brainwashed.
pub fn remove_brainwashed(world: &mut World, mob: Entity) -> bool {
    let Some(victim) = world.get::<Brainwashed>(mob).cloned() else {
        return false;
    };
    world.entity_mut(mob).remove::<Brainwashed>();
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ANTAG_HUD_DMI, HUD_STATE));
    if world
        .get::<AntagHudData>(mob)
        .is_some_and(|h| h.hud_icon == hud_icon && h.team.is_empty())
    {
        world.entity_mut(mob).remove::<AntagHudData>();
    }
    chat_to(
        world,
        mob,
        Span::Warning,
        "Ваш разум внезапно проясняется...",
    );
    chat_to(
        world,
        mob,
        Span::BoldWarning,
        "Вы чувствуете, как тяжесть Директив исчезает! Вам больше не нужно им подчиняться.",
    );
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        announce_objectives(world, client, &[]);
    }
    info!(
        "{mob} is no longer brainwashed with the objectives: {:?}",
        victim.objectives
    );
    true
}

/// `brainwash/pre_preop`: `tgui_input_text` for the directive; the operation starts on the answer
/// ([`directive_answers`]).
pub fn ask_directive(world: &mut World, actor: Actor, patient: Entity, zone: TargetZone) {
    let ask = Ask::new(
        actor.client,
        actor.mob,
        "Промывание мозгов",
        "Выберите цель, которая будет запечатлена в мозгу вашего пациента",
        PromptKind::Text {
            default: String::new(),
            max_length: MAX_DIRECTIVE_LEN,
            multiline: false,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(DirectivePrompt {
        patient,
        zone,
        tool: actor.item,
    });
}

/// The directive answered: checked again after the wait, then the operation starts with it.
fn directive_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, surgeon, answer) in pending {
        let Some(asked) = world.get::<DirectivePrompt>(prompt).copied() else {
            continue;
        };
        // `return !!operation_args[OPERATION_OBJECTIVE]`: no directive, no operation.
        let Some(PromptAnswer::Text(text)) = answer else {
            continue;
        };
        let directive: String = text
            .trim()
            .chars()
            .take(MAX_DIRECTIVE_LEN as usize)
            .collect();
        if directive.is_empty() || !still_able(world, client, surgeon, asked) {
            continue;
        }
        world
            .entity_mut(surgeon)
            .insert(BrainwashDirective(directive));
        let actor = Actor {
            client,
            mob: surgeon,
            item: asked.tool,
        };
        crate::medical::surgery::try_perform(
            world,
            actor,
            asked.patient,
            Op::Brainwash,
            asked.zone,
        );
    }
}

/// The surgeon is still the client's body, conscious and able, with the same tool in hand, next
/// to the patient.
fn still_able(world: &World, client: Entity, surgeon: Entity, asked: DirectivePrompt) -> bool {
    if world.get::<PlayerMob>(surgeon).map(|p| p.client) != Some(client)
        || world.get::<Stat>(surgeon) != Some(&Stat::Conscious)
        || crate::status_effects::incapacitation(world, surgeon).incapacitated
    {
        return false;
    }
    let held = world
        .get::<Inventory>(surgeon)
        .and_then(Inventory::active_item);
    let near = match (
        world.get::<TilePos>(surgeon),
        world.get::<TilePos>(asked.patient),
    ) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    held == asked.tool && near
}

/// `brainwash/on_success` + `on_brainwash` (`sleeper`: a random sleeper directive). `true` when
/// the patient was brainwashed.
pub fn surgical_brainwash(
    world: &mut World,
    surgeon: Entity,
    patient: Entity,
    sleeper: bool,
) -> bool {
    let asked = world
        .get::<BrainwashDirective>(surgeon)
        .map(|d| d.0.clone());
    if !sleeper {
        world.entity_mut(surgeon).remove::<BrainwashDirective>();
    }
    let target = crate::medical::name_of(world, patient);
    if world.get::<PlayerMob>(patient).is_none() {
        chat_to(
            world,
            surgeon,
            Span::Warning,
            format!("{target} не реагирует на промывание мозгов, как будто у него нет разума..."),
        );
        return false;
    }
    if crate::antag::revolution::server::mindshielded(world, patient) {
        chat_to(
            world,
            surgeon,
            Span::Warning,
            format!("{target}, похоже, не поддается промыванию мозгов..."),
        );
        return false;
    }
    let directive = if sleeper {
        let mut rng = crate::medical::fork_rng(world);
        SLEEPER_DIRECTIVES[rng.range(0, SLEEPER_DIRECTIVES.len() as u32 - 1) as usize].to_owned()
    } else {
        asked.unwrap_or_else(|| NO_DIRECTIVE.to_owned())
    };
    chat_to(
        world,
        patient,
        Span::Notice,
        format!("Новая мысль формируется в вашем разуме: «{directive}»"),
    );
    let surgeon_name = crate::medical::name_of(world, surgeon);
    info!(
        "{surgeon_name} ({surgeon}) surgically brainwashed {target} ({patient}) with the objective '{directive}'"
    );
    brainwash(world, patient, vec![directive])
}

/// `/datum/action/antag_info/Trigger` of a victim.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Brainwashed>(source).is_some() {
            world.write_message(OpenUi::new::<AntagInfoBrainwashedUi>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

/// Each victim and its directives (`/datum/objective/brainwashing` is always `completed`).
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Brainwashed)>();
    let mut victims: Vec<(Entity, Brainwashed)> =
        query.iter(world).map(|(e, v)| (e, v.clone())).collect();
    if victims.is_empty() {
        return Vec::new();
    }
    victims.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, victim) in victims {
        out.push(super::traitor::print_player(world, mob, &victim.key));
        for (i, objective) in victim.objectives.iter().enumerate() {
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {objective} ")),
                ChatSegment::new(Span::GreenText, "Успех!"),
            ]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brainwash_adds_directives_and_surgery_cures() {
        let mut world = World::new();
        let client = world.spawn_empty().id();
        let mob = world.spawn(PlayerMob { client }).id();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<OpenUi>>();
        assert!(!brainwash(&mut world, mob, vec!["  ".into()]));
        assert!(brainwash(&mut world, mob, vec![" Любите клоуна. ".into()]));
        assert!(brainwash(&mut world, mob, vec!["Не ешьте сыр.".into()]));
        let victim = world.get::<Brainwashed>(mob).unwrap();
        assert_eq!(victim.objectives, ["Любите клоуна.", "Не ешьте сыр."]);
        assert_eq!(victim.info().objectives[1].count, 2);
        assert!(world.get::<AntagHudData>(mob).is_some());
        assert_eq!(roundend_report(&mut world, RoundEnd::default()).len(), 4);
        assert!(remove_brainwashed(&mut world, mob));
        assert!(world.get::<Brainwashed>(mob).is_none());
        assert!(world.get::<AntagHudData>(mob).is_none());
        assert!(!remove_brainwashed(&mut world, mob));
    }
}
