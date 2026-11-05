//! Server half: `ClientIntent::Say` → validated, range-limited, server-built chat messages.
//!
//! Every say is hostile until checked, in this order: rate (counts every attempt, so rejected
//! ones cannot be used to flood either), a body with a position (the client's
//! [`ControlledEntity`]), then the text (control characters stripped, length, emptiness).
//! Hearers are the clients whose controlled entity stands within [`HEARING_RANGE`] of the
//! speaker on the same z-level, found through the spatial index: the cost of one say is the
//! number of entities in a 15x15 box, never the number of clients or entities in the world.
//! A body with [`MaskedSenses`] (the AI eye) hears only speakers on tiles its mask shows.
//!
//! Then, as tg `/mob/living/say`: a dead speaker ([`SpeakerStat::Dead`], kept by the game) talks
//! in deadchat (every dead or ghost client); `*` runs an emote ([`crate::emote`]); prefixes are
//! parsed ([`crate::speech`]); a hard crit mob says nothing, a soft crit one whispers. Every
//! spoken message is also published as [`Spoke`]: the game's radios talk it into the radio
//! (tg `radio()`, the `;`, `:x` and `:i` targets) and relay it (broadcasting radios nearby);
//! a message that went to the radio is whispered locally (`REDUCE_RANGE`). A whisper is heard in
//! full at 1 tile, starred at 2, and seen as "whispers something" up to 7.

use bevy::ecs::lifecycle::{Discard, Insert};
use bevy::ecs::system::SystemParam;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_net::server::{
    ClientDisconnected, ControlledEntity, IntentReceived, Invisibility, MaskId, SeeInvisible,
    ViewMasks,
};
use ss13_shared::TilePos;
use ss13_shared::chat::{
    ChatChannel, ChatMessage, ChatSegment, MAX_MESSAGE_LEN, MAX_NAME_LEN, Span,
};
use ss13_shared::protocol::ClientIntent;
use ss13_world::{SpatialIndex, WorldSystems};

use crate::emote::{EMOTE_COOLDOWN_SECS, EmoteOutcome};
use crate::speech::{
    BinaryCapable, ChangelingHivemind, EarsWorn, Impediments, RadioExtension, RadioTarget,
    SpeakerStat, SpeechEffects, Tongueless, VoiceSpan, WhisperHeard, capitalize, derpspeech,
    parse_mods, slur, stars, stutter, unintelligize, whisper_heard,
};

/// tg `get_hearers_in_view(world.view)`: 7 tiles, Chebyshev, like BYOND `view()`.
pub const HEARING_RANGE: u32 = 7;

/// tg `handle_spam_prevention`: messages are counted in 5 s windows and the
/// `SPAM_TRIGGER_AUTOMUTE`th (10th) one is suppressed, so 9 pass per window.
pub const SAY_WINDOW_SECS: f64 = 5.0;
pub const SAY_MAX_PER_WINDOW: u32 = 9;

/// Name shown for a body without a `Name` (tg `Unknown`, BandaStation translation).
pub const UNKNOWN_NAME: &str = "Неизвестный";

pub struct ChatServerPlugin;

/// The say handler: [`Spoke`] is written here.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SaySystems;

/// Why a say was dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SayReject {
    /// The client controls no entity, or that entity has no `TilePos`.
    NoBody,
    RateLimited,
    TooLong,
    /// Nothing left after stripping control characters and whitespace.
    Empty,
    /// The client is [`Muted`].
    Muted,
}

/// On a connection entity: an admin muted the client (tg `MUTE_ALL`): no IC speech, emotes or
/// deadchat (`ss13_admin` also refuses its adminhelps).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Muted;

/// A say was dropped; the client got a warning unless the reason is [`SayReject::Empty`] or a
/// repeated [`SayReject::RateLimited`].
#[derive(Message, Clone, Debug, PartialEq)]
pub struct SayRejected {
    pub client: Entity,
    pub reason: SayReject,
}

/// On a body that perceives the world only where a view mask shows it: the AI eye, which sees
/// through the cameras (tg `/mob/eye/camera/ai/Hear` passes speech only on `checkCameraVis`). It
/// hears speech and sees messages only from tiles of the mask, so nothing said or done under the
/// static reaches its client ([`Senses`]).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaskedSenses(pub MaskId);

/// Whether a listening body perceives what happens on a tile ([`MaskedSenses`]).
#[derive(SystemParam)]
pub struct Senses<'w, 's> {
    masks: Option<Res<'w, ViewMasks>>,
    masked: Query<'w, 's, &'static MaskedSenses>,
}

impl Senses<'_, '_> {
    pub fn perceives(&self, body: Entity, at: TilePos) -> bool {
        perceives(self.masks.as_deref(), self.masked.get(body).ok(), at)
    }
}

/// [`Senses::perceives`] from the world.
pub fn perceives_in(world: &World, body: Entity, at: TilePos) -> bool {
    perceives(
        world.get_resource::<ViewMasks>(),
        world.get::<MaskedSenses>(body),
        at,
    )
}

fn perceives(masks: Option<&ViewMasks>, sense: Option<&MaskedSenses>, at: TilePos) -> bool {
    sense.is_none_or(|s| masks.is_some_and(|m| m.visible(s.0, at)))
}

/// Controlled entity → the client controlling it: turns "entities in range" into "clients that
/// hear". Kept by observers on [`ControlledEntity`], so it is exact at every say.
// shortcut: one client per body; two clients controlling one entity (aghost-like) need a Vec.
#[derive(Resource, Default, Debug)]
pub struct Listeners(HashMap<Entity, Entity>);

impl Listeners {
    pub fn client_of(&self, body: Entity) -> Option<Entity> {
        self.0.get(&body).copied()
    }
}

/// Words a living speaker said aloud (tg `send_speech` after `radio()`), for radios: the target
/// talks into its devices (`talk_into`), broadcasting radios in range relay what they heard
/// (`/obj/item/radio/Hear`).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct Spoke {
    pub client: Entity,
    pub speaker: Entity,
    pub pos: TilePos,
    pub name: String,
    /// The final text (capitalized, with sing notes).
    pub text: String,
    /// tg `SAY_MOD_VERB` as `radio()` gets it: before the radio shrank the speech.
    pub verb: &'static str,
    /// `WHISPER_MODE` as `radio()` gets it (radios star whispers).
    pub whisper: bool,
    pub radio: Option<RadioTarget>,
    /// `WHISPER_MODE` of the words in the air (after `REDUCE_RANGE`), what relaying radios hear.
    pub heard_whisper: bool,
    /// `stars(text)`: what a radio transmits of a whisper (radios don't pick up whispers well);
    /// empty when nothing is whispered.
    pub starred: String,
}

impl Spoke {
    /// The verb relaying radios hear (`say_mod` again after `REDUCE_RANGE`).
    pub fn heard_verb(&self) -> &'static str {
        if self.heard_whisper && !self.whisper {
            WHISPER_VERB
        } else {
            self.verb
        }
    }
}

/// tg `verb_whisper` (BandaStation).
pub const WHISPER_VERB: &str = "шепчет";

/// tg `emotes_used`: when each emote of this speaker was last used, `Time<Real>` seconds.
#[derive(Component, Default, Debug)]
struct EmotesUsed(HashMap<&'static str, f64>);

/// `prob`/`rand` for speech (stars, sing notes): splitmix64, seeded from the clock; the game
/// reseeds it from the map seed (`ss13_game::server::seed_rngs`).
#[derive(Resource)]
pub struct SpeechRng(pub u64);

impl Default for SpeechRng {
    fn default() -> Self {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self(seed)
    }
}

impl SpeechRng {
    /// A value in `0..100`.
    fn percent(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) % 10_000) as f32 / 100.0
    }
}

#[derive(Resource, Default)]
struct SayRates(HashMap<Entity, SayRate>);

#[derive(Default)]
struct SayRate {
    window_start: f64,
    count: u32,
}

impl SayRate {
    /// `Err(true)` for the first refusal of a window, `Err(false)` for the rest.
    fn admit(&mut self, now: f64) -> Result<(), bool> {
        if self.count == 0 || now >= self.window_start + SAY_WINDOW_SECS {
            self.window_start = now;
            self.count = 0;
        }
        self.count += 1;
        if self.count <= SAY_MAX_PER_WINDOW {
            Ok(())
        } else {
            Err(self.count == SAY_MAX_PER_WINDOW + 1)
        }
    }
}

impl Plugin for ChatServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Listeners>()
            .init_resource::<SayRates>()
            .init_resource::<SpeechRng>()
            .add_message::<SayRejected>()
            .add_message::<Spoke>()
            .add_observer(on_control_inserted)
            .add_observer(on_control_discarded)
            .add_systems(
                Update,
                (
                    handle_say.after(WorldSystems::Movement).in_set(SaySystems),
                    forget_rates,
                ),
            );
    }
}

fn on_control_inserted(
    insert: On<Insert<ControlledEntity>>,
    controls: Query<&ControlledEntity>,
    mut listeners: ResMut<Listeners>,
) {
    if let Ok(control) = controls.get(insert.entity) {
        listeners.0.insert(control.0, insert.entity);
    }
}

/// Runs before the value is replaced or removed, and on despawn of the connection entity.
fn on_control_discarded(
    discard: On<Discard<ControlledEntity>>,
    controls: Query<&ControlledEntity>,
    mut listeners: ResMut<Listeners>,
) {
    if let Ok(control) = controls.get(discard.entity)
        && listeners.client_of(control.0) == Some(discard.entity)
    {
        listeners.0.remove(&control.0);
    }
}

fn forget_rates(mut gone: MessageReader<ClientDisconnected>, mut rates: ResMut<SayRates>) {
    for ClientDisconnected { client, .. } in gone.read() {
        rates.0.remove(client);
    }
}

/// Bidirectional formatting controls (U+200E/F, U+202A..E, U+2066..9) reorder the text around
/// them: in a name or a quote they let a player make the line read as something else.
fn is_stripped(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Player text → the text spoken: control characters stripped (tab and line breaks become
/// spaces), trimmed, at most
/// [`MAX_MESSAGE_LEN`] characters, first letter capitalized (tg `capitalize`).
pub fn sanitize_say(raw: &str) -> Result<String, SayReject> {
    // UTF-8 is at most 4 bytes per character: longer input cannot fit, and is not scanned.
    if raw.len() > MAX_MESSAGE_LEN * 4 {
        return Err(SayReject::TooLong);
    }
    // Tabs and line breaks separate words; other controls vanish.
    let cleaned: String = raw
        .chars()
        .filter_map(|c| match c {
            '\t' | '\n' | '\r' => Some(' '),
            c if is_stripped(c) => None,
            c => Some(c),
        })
        .collect();
    let text = cleaned.trim();
    if text.is_empty() {
        return Err(SayReject::Empty);
    }
    if text.chars().count() > MAX_MESSAGE_LEN {
        return Err(SayReject::TooLong);
    }
    let mut chars = text.chars();
    Ok(chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default())
}

/// tg `say_mod` with BandaStation's verb translations.
fn say_verb(text: &str) -> &'static str {
    if text.ends_with("!!") {
        "кричит"
    } else if text.ends_with('?') {
        "спрашивает"
    } else if text.ends_with('!') {
        "восклицает"
    } else {
        "говорит"
    }
}

/// `Имя говорит, "текст"`, as tg's `compose_message` lays it out. The player's text is one
/// segment, never parsed: anything markup-like in it is shown as typed.
pub fn say_message(name: &str, text: &str) -> ChatMessage {
    quote_message(name, say_verb(text), text, Span::Say)
}

/// [`say_message`] of a speaker with its own `verb_say`, `verb_ask`, `verb_exclaim` and
/// `verb_yell` (tg `say_mod`), e.g. a bot's "сообщает".
pub fn say_message_verbs(name: &str, text: &str, verbs: [&str; 4]) -> ChatMessage {
    let [say, ask, exclaim, yell] = verbs;
    let verb = if text.ends_with("!!") {
        yell
    } else if text.ends_with('?') {
        ask
    } else if text.ends_with('!') {
        exclaim
    } else {
        say
    };
    quote_message(name, verb, text, Span::Say)
}

/// `Имя шепчет, "текст"` in italics (tg `verb_whisper`, `SPAN_ITALICS`).
pub fn whisper_message(name: &str, text: &str) -> ChatMessage {
    quote_message(name, WHISPER_VERB, text, Span::Italics)
}

fn quote_message(name: &str, verb: &str, text: &str, span: Span) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Local,
        segments: vec![
            ChatSegment::new(Span::Name, name),
            ChatSegment::new(span, format!(" {verb}, \"")),
            ChatSegment::new(span, text),
            ChatSegment::new(span, "\""),
        ],
    }
}

/// tg `span_emote("<b>[user]</b> [msg]")`.
pub fn emote_message(name: &str, text: &str) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Local,
        segments: vec![
            ChatSegment::new(Span::Name, name),
            ChatSegment::new(Span::Emote, format!(" {text}")),
        ],
    }
}

/// tg `say_dead`: `МЕРТВ: Имя говорит, "текст"`.
pub fn deadchat_message(name: &str, text: &str) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Local,
        segments: vec![
            ChatSegment::new(Span::Emote, "МЕРТВ: "),
            ChatSegment::new(Span::Name, name),
            ChatSegment::new(Span::Emote, format!(" {}, \"", say_verb(text))),
            ChatSegment::new(Span::Emote, text),
            ChatSegment::new(Span::Emote, "\""),
        ],
    }
}

fn speaker_name(name: Option<&Name>) -> String {
    let name: String = name
        .map(|n| n.as_str())
        .unwrap_or_default()
        .chars()
        .filter(|&c| !is_stripped(c))
        .take(MAX_NAME_LEN)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        UNKNOWN_NAME.into()
    } else {
        name.into()
    }
}

fn warning(text: impl Into<String>) -> ChatMessage {
    ChatMessage::simple(ChatChannel::Warning, Span::Warning, text)
}

type Body<'a> = (
    Entity,
    &'a TilePos,
    Option<&'a Name>,
    Option<&'a Invisibility>,
    Option<&'a SpeakerStat>,
    Has<EarsWorn>,
    Has<BinaryCapable>,
    Has<ChangelingHivemind>,
    Has<Tongueless>,
    Option<&'a Impediments>,
    Option<&'a SpeechEffects>,
);

fn handle_say(
    mut intents: MessageReader<IntentReceived>,
    controls: Query<&ControlledEntity>,
    muted: Query<(), With<Muted>>,
    bodies: Query<Body>,
    stats: Query<(Entity, &SpeakerStat)>,
    mut emotes_used: Query<&mut EmotesUsed>,
    (sight, senses, voices): (Query<&SeeInvisible>, Senses, Query<&VoiceSpan>),
    index: Res<SpatialIndex>,
    listeners: Res<Listeners>,
    mut rates: ResMut<SayRates>,
    mut rng: ResMut<SpeechRng>,
    time: Res<Time<Real>>,
    mut chat: MessageWriter<SendChat>,
    mut rejected: MessageWriter<SayRejected>,
    mut spoke: MessageWriter<Spoke>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for IntentReceived { client, intent } in intents.read() {
        let ClientIntent::Say(raw) = intent else {
            continue;
        };
        let client = *client;
        let checked = rates
            .0
            .entry(client)
            .or_default()
            .admit(now)
            .map_err(|first| (SayReject::RateLimited, first))
            .and_then(|()| {
                if muted.contains(client) {
                    Err((SayReject::Muted, true))
                } else {
                    Ok(())
                }
            })
            .and_then(|()| {
                controls
                    .get(client)
                    .ok()
                    .and_then(|control| bodies.get(control.0).ok())
                    .ok_or((SayReject::NoBody, true))
            })
            .and_then(|body| {
                sanitize_say(raw)
                    .map(|text| (body, text))
                    .map_err(|r| (r, true))
            });
        let (
            (
                speaker,
                pos,
                name,
                invisibility,
                stat,
                ears_worn,
                binary,
                hivemind,
                tongueless,
                impediments,
                speech_effects,
            ),
            text,
        ) = match checked {
            Ok(said) => said,
            Err((reason, notify)) => {
                let feedback = match reason {
                    _ if !notify => None,
                    SayReject::NoBody => Some("Вам нечем говорить.".to_string()),
                    SayReject::RateLimited => {
                        Some("Вы отправляете сообщения слишком часто.".to_string())
                    }
                    SayReject::TooLong => Some(format!(
                        "Сообщение слишком длинное: больше {MAX_MESSAGE_LEN} символов."
                    )),
                    SayReject::Empty => None,
                    SayReject::Muted => {
                        Some("Вы не можете разговаривать в IC-чате (мут).".to_string())
                    }
                };
                if let Some(text) = feedback {
                    chat.write(SendChat {
                        clients: vec![client],
                        message: warning(text),
                    });
                }
                debug!("say from {client} rejected: {reason:?}");
                rejected.write(SayRejected { client, reason });
                continue;
            }
        };
        let name = speaker_name(name);
        let stat = stat.copied().unwrap_or_default();
        let notice = |text: String| SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::Local, Span::Notice, text),
        };
        // tg `say_dead`: every dead mob and ghost reads it, wherever it is.
        if stat == SpeakerStat::Dead {
            let mut clients: Vec<Entity> = stats
                .iter()
                .filter(|(_, s)| **s == SpeakerStat::Dead)
                .filter_map(|(e, _)| listeners.client_of(e))
                .collect();
            if !clients.contains(&client) {
                clients.push(client);
            }
            info!(target: "say", "{client} {name} (dead): {text}");
            chat.write(SendChat {
                clients,
                message: deadchat_message(&name, &text),
            });
            continue;
        }
        // shortcut: range only, no line of sight (tg hearing uses `view`, which opaque turfs
        // block); add an occlusion check once walls carry opacity in the index.
        // An invisible speaker is heard only by those who see it; an AI eye hears only what its
        // cameras see.
        let invisibility = invisibility.map_or(0, |i| i.0);
        let hearers_within = |range: u32| -> Vec<(Entity, u32)> {
            let mut hearers: Vec<(Entity, u32)> = index
                .in_radius(*pos, range)
                .filter(|&e| senses.perceives(e, *pos))
                .filter_map(|e| {
                    let client = listeners.client_of(e)?;
                    let distance = bodies
                        .get(e)
                        .ok()
                        .and_then(|b| b.1.chebyshev(*pos))
                        .unwrap_or(0);
                    Some((client, distance))
                })
                .filter(|&(c, _)| {
                    invisibility == 0 || sight.get(c).is_ok_and(|s| s.0 >= invisibility)
                })
                .collect();
            // The speaker always hears itself, even standing off the indexed map.
            if !hearers.iter().any(|&(c, _)| c == client) {
                hearers.push((client, 0));
            }
            hearers
        };
        if let Some(act) = text.strip_prefix('*') {
            let mut used = emotes_used.get_mut(speaker).ok();
            let outcome = crate::emote::run(act, stat, |key| {
                used.as_ref()
                    .and_then(|u| u.0.get(key))
                    .is_some_and(|&at| now < at + EMOTE_COOLDOWN_SECS)
            });
            match outcome {
                EmoteOutcome::Run { key, text } => {
                    match used.as_mut() {
                        Some(used) => {
                            used.0.insert(key, now);
                        }
                        None => {
                            commands
                                .entity(speaker)
                                .try_insert(EmotesUsed(HashMap::from_iter([(key, now)])));
                        }
                    }
                    info!(target: "say", "{client} {name} emote: {text}");
                    let clients = hearers_within(HEARING_RANGE)
                        .into_iter()
                        .map(|(c, _)| c)
                        .collect();
                    chat.write(SendChat {
                        clients,
                        message: emote_message(&name, &text),
                    });
                }
                EmoteOutcome::Refused(text) | EmoteOutcome::Help(text) => {
                    chat.write(notice(text));
                }
                EmoteOutcome::Silent => {}
            }
            continue;
        }
        let (mods, message) = parse_mods(&text, stat);
        // tg: an admin verb; anyone else's message goes nowhere.
        if mods.radio == Some(RadioExtension::Deadmin) {
            continue;
        }
        // tg: an unconscious mob says nothing.
        // shortcut: no last words (a hard crit whisper that succumbs); add with `succumb`.
        if message.is_empty() || stat >= SpeakerStat::HardCrit {
            continue;
        }
        let impediments = impediments.copied().unwrap_or_default();
        let speech_effects = speech_effects.copied().unwrap_or_default();
        // tg `try_speak` -> `can_speak()` (`TRAIT_MUTE`, also from `silenced`).
        if impediments.mute || speech_effects.silenced {
            chat.write(SendChat {
                clients: vec![client],
                message: warning("Вы осознаете, что не можете говорить!"),
            });
            continue;
        }
        // tg `treat_message`: `TRAIT_UNINTELLIGIBLE_SPEECH`, then the speech status effects.
        let mut message = message;
        if impediments.unintelligible {
            message = unintelligize(&message, || rng.percent());
        }
        let stuttering = impediments.stutter || speech_effects.stutter;
        if stuttering {
            message = stutter(&message, || rng.percent());
        }
        for effect in speech_effects.slurs.iter().flatten() {
            message = slur(&message, effect, || rng.percent());
        }
        if impediments.derp {
            message = derpspeech(&message, stuttering, || rng.percent());
        }
        let mut message = capitalize(&message);
        if tongueless {
            const SOUNDS: [(&str, &str); 3] = [("aa", "AA"), ("oo", "OO"), ("'", "'")];
            let (lower, upper) = SOUNDS[(rng.percent() as usize * SOUNDS.len()) / 100];
            message = crate::speech::tongueless(&message, lower, upper);
        }
        // `get_default_say_verb` of a human without a tongue.
        let say_verb = |text: &str| match say_verb(text) {
            "говорит" if tongueless => "булькает",
            verb => verb,
        };
        if mods.sing {
            let notes = ['\u{2669}', '\u{266A}', '\u{266B}'];
            let note = notes[(rng.percent() as usize * notes.len()) / 100];
            message = format!("{note} {message} {note}");
        }
        // `TRAIT_FORCE_WHISPER` in soft crit; anything that went to the radio shrinks to a
        // whisper (`REDUCE_RANGE`), even without a headset.
        let whisper = mods.whisper || stat == SpeakerStat::SoftCrit;
        let target = match mods.radio_target(ears_worn) {
            // tg: without `binarycheck()` the binary saymode is not used: plain speech.
            Some(RadioTarget::Binary) if !binary => None,
            // tg `/datum/saymode/changeling/can_be_used_by`: the hivemind trait.
            Some(RadioTarget::Changeling) if !hivemind => None,
            target => target,
        };
        if matches!(target, Some(RadioTarget::Binary | RadioTarget::Changeling)) {
            let verb = say_verb(&message);
            spoke.write(Spoke {
                client,
                speaker,
                pos: *pos,
                name,
                text: message,
                verb,
                whisper: false,
                radio: target,
                heard_whisper: false,
                starred: String::new(),
            });
            continue;
        }
        let verb = if whisper {
            WHISPER_VERB
        } else {
            say_verb(&message)
        };
        spoke.write(Spoke {
            client,
            speaker,
            pos: *pos,
            name: name.clone(),
            text: message.clone(),
            verb,
            whisper,
            radio: target,
            heard_whisper: whisper || target.is_some(),
            starred: if whisper || target.is_some() {
                stars(&message, 25.0, || rng.percent())
            } else {
                String::new()
            },
        });
        let whisper = whisper || target.is_some();
        info!(target: "say", "{client} {name} at {pos:?}: {message}");
        if !whisper {
            let clients = hearers_within(HEARING_RANGE)
                .into_iter()
                .map(|(c, _)| c)
                .collect();
            let span = voices.get(speaker).map_or(Span::Say, |v| v.0);
            chat.write(SendChat {
                clients,
                message: quote_message(&name, say_verb(&message), &message, span),
            });
            continue;
        }
        let (mut full, mut starred, mut far) = (Vec::new(), Vec::new(), Vec::new());
        for (hearer, distance) in hearers_within(HEARING_RANGE) {
            match whisper_heard(distance) {
                WhisperHeard::Full => full.push(hearer),
                WhisperHeard::Starred => starred.push(hearer),
                WhisperHeard::TooFar => far.push(hearer),
            }
        }
        chat.write(SendChat {
            clients: full,
            message: whisper_message(&name, &message),
        });
        if !starred.is_empty() {
            let text = stars(&message, 25.0, || rng.percent());
            chat.write(SendChat {
                clients: starred,
                message: whisper_message(&name, &text),
            });
        }
        if !far.is_empty() {
            chat.write(SendChat {
                clients: far,
                message: ChatMessage {
                    channel: ChatChannel::Local,
                    segments: vec![
                        ChatSegment::new(Span::Name, name.clone()),
                        ChatSegment::new(
                            Span::Plain,
                            " шепчет что-то, но вы слишком далеко, чтобы услышать его.",
                        ),
                    ],
                },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_controls_checks_length_and_capitalizes() {
        assert_eq!(sanitize_say("  при\u{7}вет\nмир  ").unwrap(), "Привет мир");
        assert_eq!(sanitize_say("a\u{202E}b\u{2066}c").unwrap(), "Abc");
        assert_eq!(sanitize_say("\u{0}\u{1b}\t "), Err(SayReject::Empty));
        let max = "я".repeat(MAX_MESSAGE_LEN);
        assert_eq!(sanitize_say(&max).unwrap().chars().count(), MAX_MESSAGE_LEN);
        assert_eq!(sanitize_say(&format!("{max}я")), Err(SayReject::TooLong));
        assert_eq!(
            sanitize_say(&"\u{0}".repeat(MAX_MESSAGE_LEN * 4 + 1)),
            Err(SayReject::TooLong)
        );
        // Markup is plain text.
        assert_eq!(sanitize_say("<b>x</b>").unwrap(), "<b>x</b>");
    }

    #[test]
    fn verb_follows_punctuation() {
        let verb = |t: &str| say_message("Боб", t).segments[1].text.clone();
        assert_eq!(verb("Привет"), " говорит, \"");
        assert_eq!(verb("Кто здесь?"), " спрашивает, \"");
        assert_eq!(verb("Ура!"), " восклицает, \"");
        assert_eq!(verb("Пожар!!"), " кричит, \"");
    }

    #[test]
    fn rate_window_admits_nine_and_reports_first_refusal() {
        let mut rate = SayRate::default();
        for _ in 0..SAY_MAX_PER_WINDOW {
            assert_eq!(rate.admit(10.0), Ok(()));
        }
        assert_eq!(rate.admit(11.0), Err(true));
        assert_eq!(rate.admit(12.0), Err(false));
        assert_eq!(rate.admit(10.0 + SAY_WINDOW_SECS), Ok(()));
    }

    #[test]
    fn names_are_cleaned_and_bounded() {
        assert_eq!(speaker_name(None), UNKNOWN_NAME);
        assert_eq!(speaker_name(Some(&Name::new("\u{202E} "))), UNKNOWN_NAME);
        let long = Name::new("Ж".repeat(100));
        assert_eq!(speaker_name(Some(&long)).chars().count(), MAX_NAME_LEN);
    }
}
