//! Parrots and Poly (server): speech learning, after tg `/datum/component/listen_and_repeat`,
//! `parrot_repeat_speech` (`parrot_ai/parroting_action.dm`) and `/mob/living/basic/parrot`
//! (`get_phrase`, `on_pet`, `consume_cracker`), with Poly's lines (`poly.dm`).
//!
//! A parrot remembers what it hears said aloud in view within 7 tiles (whispers only next to
//! it), up to 500 phrases, and now and then squawks one of them (`speech_probability_rate` 5,
//! Poly 6; a new phrase with `speech_shuffle_rate` 30%), at most once per 7.5 s. Petting it
//! makes it say its phrase (5 s apart). A cracker heals it by 10 and makes it chattier.
//!
//! What it hears is what it learns: speech in a language the parrot does not understand is
//! learned as the sounds it heard (tg keeps the speaker's raw words, so a parrot repeats
//! Draconic in Galactic Common, a translation it never could make).
// shortcut: no headset (Poly's radio lines are said aloud, the `:e` prefix cut off), no
// round-to-round memory (`data/npc_saves/Poly.json`) or Poly's ghost, no TTS voices, no perching,
// item stealing or hoarding; add them with persistence and the parrot's inventory.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::Spoke;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_shared::TilePos;

use super::ai::AiCtx;
use super::ai::behaviors::{DELAY_FAIL, DELAY_SUCCEED, Perform, can_see};
use crate::combat::CombatRng;
use crate::health::Stat;

/// `MAX_SPEECH_BUFFER_SIZE`.
pub const MAX_SPEECH_BUFFER: usize = 500;
/// `FORCED_SPEECH_COOLDOWN_DURATION`.
const FORCED_SPEECH_COOLDOWN: f64 = 5.0;
/// How far a parrot hears (`get_hearers_in_view` of a say).
const HEARING_RANGE: u32 = 7;
/// The `cooldown` decorator over `parrot_repeat_speech`.
pub const REPEAT_COOLDOWN: f64 = 7.5;
const PARROT: &str = "/mob/living/basic/parrot";
const POLY: &str = "/mob/living/basic/parrot/poly";

/// `get_static_list_of_phrases` of a parrot.
const DEFAULT_PHRASES: [&str; 4] = [
    "BAWWWWK george mellons griffing me!",
    "Cracker?",
    "Hello!",
    "Hi!",
];
/// Poly's lines (BandaStation translation) and the default-Poly line of `determine_special_poly`.
const POLY_PHRASES: [&str; 6] = [
    ":e Проверьте кристалл, долбоёбы!",
    ":e О БОЖЕ, ОН ВОТ-ВОТ РАССЛОИТСЯ, ВЫЗЫВАЙТЕ ШАТТЛ!",
    ":e КТО ЗАБРАЛ ЭТИ ЧЕРТОВЫ МОДСЬЮТЫ?",
    ":e Подключите соляры, бездельники!",
    "Поли хочет крекер!",
    "...живой?",
];

/// A parrot's `listen_and_repeat` buffer and speech blackboards.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Parrot {
    /// `speech_buffer`: unique phrases, oldest first.
    pub buffer: Vec<String>,
    /// `BB_PARROT_REPEAT_STRING`.
    pub phrase: Option<String>,
    /// `BB_PARROT_REPEAT_PROBABILITY`, `BB_PARROT_PHRASE_CHANGE_PROBABILITY`.
    pub speech_probability: f32,
    pub shuffle_probability: f32,
    /// `forced_speech_cooldown`.
    pub forced_until: f64,
}

impl Parrot {
    /// `listen_and_repeat/on_hear`: the phrase goes in (once), random ones out past the cap.
    pub fn hear(&mut self, text: &str, rng: &mut CombatRng) {
        let text = text.trim();
        if text.is_empty() || self.buffer.iter().any(|p| p == text) {
            return;
        }
        while self.buffer.len() >= MAX_SPEECH_BUFFER {
            let i = rng.range(0, self.buffer.len() as u32 - 1) as usize;
            self.buffer.swap_remove(i);
        }
        self.buffer.push(text.to_owned());
    }

    /// `set_new_blackboard_phrase`: `false` with nothing learned (`NO_NEW_PHRASE_AVAILABLE`).
    pub fn new_phrase(&mut self, rng: &mut CombatRng) -> bool {
        if self.buffer.is_empty() {
            self.phrase = None;
            return false;
        }
        let i = rng.range(0, self.buffer.len() as u32 - 1) as usize;
        self.phrase = Some(self.buffer[i].clone());
        true
    }
}

/// The parrot's `has_channel_prefix` cut without a headset: `:e text` -> `text`.
pub fn strip_radio_prefix(line: &str) -> &str {
    let mut chars = line.chars();
    match (chars.next(), chars.next()) {
        (Some(':' | '.' | '#'), Some(key)) if key.is_alphanumeric() => chars.as_str().trim_start(),
        _ => line,
    }
}

pub(super) fn build(app: &mut App) {
    app.add_systems(Update, listen.after(crate::health::HealthSystems));
}

pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    if !is_under(&def.path, PARROT) {
        return;
    }
    let poly = is_under(&def.path, POLY);
    let phrases: &[&str] = if poly {
        &POLY_PHRASES
    } else {
        &DEFAULT_PHRASES
    };
    world.entity_mut(mob).insert(Parrot {
        buffer: phrases.iter().map(|&p| p.to_owned()).collect(),
        phrase: None,
        speech_probability: if poly { 6.0 } else { 5.0 },
        shuffle_probability: 30.0,
        forced_until: 0.0,
    });
}

/// Parrots hearing speech (tg `COMSIG_MOVABLE_PRE_HEAR`).
// Cost: per said line, a pass over the (few) parrots.
fn listen(world: &mut World, mut cursor: Local<MessageCursor<Spoke>>) {
    if !world.contains_resource::<Messages<Spoke>>() {
        return;
    }
    let spoken: Vec<Spoke> = cursor
        .read(world.resource::<Messages<Spoke>>())
        .cloned()
        .collect();
    if spoken.is_empty() {
        return;
    }
    let parrots: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &TilePos), With<Parrot>>()
        .iter(world)
        .map(|(e, p)| (e, *p))
        .collect();
    let seed = world
        .get_resource::<ss13_chat::language::ScrambleSeed>()
        .map_or(0, |s| s.0);
    for said in spoken {
        for &(parrot, at) in &parrots {
            if said.speaker == parrot || world.get::<Stat>(parrot) == Some(&Stat::Dead) {
                continue;
            }
            let near = at.chebyshev(said.pos).is_some_and(|d| d <= 1);
            if (said.heard_whisper && !near) || !can_see(world, at, said.pos, HEARING_RANGE) {
                continue;
            }
            let understanding = ss13_chat::language::understanding(
                world.get::<ss13_chat::language::LanguageHolder>(parrot),
                false,
                said.language
                    .unwrap_or(ss13_chat::language::LanguageId::Common),
            );
            let heard =
                ss13_chat::language::translate(seed, said.language, understanding, &said.text);
            let mut rng = CombatRng(world.resource::<CombatRng>().0);
            if let Some(mut p) = world.get_mut::<Parrot>(parrot) {
                p.hear(&heard, &mut rng);
            }
            world.resource_mut::<CombatRng>().0 = rng.0;
        }
    }
}

/// tg `parrot_repeat_speech` `perform`.
pub fn repeat(ctx: &mut AiCtx) -> Perform {
    let Some(mut parrot) = ctx.world.get::<Parrot>(ctx.pawn).cloned() else {
        return DELAY_FAIL;
    };
    if (ctx.prob(parrot.shuffle_probability) || parrot.phrase.is_none())
        && !parrot.new_phrase(&mut ctx.rng)
    {
        ctx.world.entity_mut(ctx.pawn).insert(parrot);
        return DELAY_FAIL;
    }
    let speak = ctx.prob(parrot.speech_probability);
    let line = parrot.phrase.clone();
    ctx.world.entity_mut(ctx.pawn).insert(parrot);
    let Some(line) = line.filter(|_| speak) else {
        return DELAY_FAIL;
    };
    super::ai::behaviors::say(ctx, strip_radio_prefix(&line));
    DELAY_SUCCEED
}

/// `get_phrase` for a pet or a hit: a new phrase, 5 s apart, never for a player's parrot.
fn forced_phrase(world: &mut World, parrot: Entity) -> Option<String> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<crate::server::PlayerMob>(parrot).is_some()
        || world.get::<Stat>(parrot) != Some(&Stat::Conscious)
    {
        return None;
    }
    let mut p = world.get::<Parrot>(parrot)?.clone();
    if now < p.forced_until {
        return None;
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let found = p.new_phrase(&mut rng);
    world.resource_mut::<CombatRng>().0 = rng.0;
    p.forced_until = now + FORCED_SPEECH_COOLDOWN;
    let line = p.phrase.clone().filter(|_| found);
    world.entity_mut(parrot).insert(p);
    line
}

/// `on_pet` (`COMSIG_ANIMAL_PET`): the parrot says its phrase.
pub fn on_pet(world: &mut World, parrot: Entity) {
    if let Some(line) = forced_phrase(world, parrot) {
        say_aloud(world, parrot, strip_radio_prefix(&line));
    }
}

/// `consume_cracker`: heals 10 and makes the parrot chattier.
pub fn ate_cracker(world: &mut World, parrot: Entity) {
    if let Some(mut health) = world.get_mut::<crate::health::Health>(parrot) {
        health.adjust(crate::health::DamageType::Brute, -10.0);
    }
    if let Some(mut p) = world.get_mut::<Parrot>(parrot) {
        p.speech_probability *= 1.27;
        p.shuffle_probability += 10.0;
    }
}

/// A basic mob says `text` with its `speak_emote` verb (outside of its AI tick).
fn say_aloud(world: &mut World, mob: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = super::defense::name_of(world, mob);
    let verbs = world
        .get::<super::BasicMob>(mob)
        .map(|b| b.def.speak_emote.clone())
        .unwrap_or_default();
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let verb = (!verbs.is_empty())
        .then(|| verbs[rng.range(0, verbs.len() as u32 - 1) as usize].clone())
        .unwrap_or_else(|| "говорит".to_owned());
    world.resource_mut::<CombatRng>().0 = rng.0;
    let message = ss13_shared::chat::ChatMessage {
        channel: ss13_shared::chat::ChatChannel::Local,
        segments: vec![
            ss13_shared::chat::ChatSegment::new(ss13_shared::chat::Span::Name, name),
            ss13_shared::chat::ChatSegment::new(
                ss13_shared::chat::Span::Say,
                format!(" {verb}, \"{text}\""),
            ),
        ],
    };
    let clients =
        crate::obj_damage::clients_in_range(world, pos, crate::health::DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(ss13_net::chat::server::SendChat { clients, message });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parrot() -> Parrot {
        Parrot {
            buffer: Vec::new(),
            phrase: None,
            speech_probability: 5.0,
            shuffle_probability: 30.0,
            forced_until: 0.0,
        }
    }

    #[test]
    fn learns_unique_phrases_up_to_the_cap() {
        let mut rng = CombatRng(1);
        let mut p = parrot();
        p.hear("Привет", &mut rng);
        p.hear(" Привет ", &mut rng);
        assert_eq!(p.buffer, vec!["Привет".to_owned()]);
        for i in 0..(MAX_SPEECH_BUFFER + 20) {
            p.hear(&format!("фраза {i}"), &mut rng);
        }
        assert_eq!(p.buffer.len(), MAX_SPEECH_BUFFER);
        assert!(
            p.buffer
                .iter()
                .any(|l| l == &format!("фраза {}", MAX_SPEECH_BUFFER + 19))
        );
    }

    #[test]
    fn new_phrase_needs_a_buffer() {
        let mut rng = CombatRng(3);
        let mut p = parrot();
        assert!(!p.new_phrase(&mut rng));
        p.hear("Крекер?", &mut rng);
        assert!(p.new_phrase(&mut rng));
        assert_eq!(p.phrase.as_deref(), Some("Крекер?"));
    }

    #[test]
    fn radio_prefix_is_cut() {
        assert_eq!(
            strip_radio_prefix(":e Подключите соляры!"),
            "Подключите соляры!"
        );
        assert_eq!(
            strip_radio_prefix("Поли хочет крекер!"),
            "Поли хочет крекер!"
        );
        assert_eq!(strip_radio_prefix("...живой?"), "...живой?");
    }
}
