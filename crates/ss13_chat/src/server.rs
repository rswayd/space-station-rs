//! Server half: `ClientIntent::Say` → validated, range-limited, server-built chat messages.
//!
//! Every say is hostile until checked, in this order: rate (counts every attempt, so rejected
//! ones cannot be used to flood either), a body with a position (the client's
//! [`ControlledEntity`]), then the text (control characters stripped, length, emptiness).
//! Hearers are the clients whose controlled entity stands within [`HEARING_RANGE`] of the
//! speaker on the same z-level, found through the spatial index, on a tile the speaker's tile
//! sees (tg `get_hearers_in_view`: BYOND `view()`, opaque tiles stop it, [`hearing_view`]); the
//! dead hear in range without sight (tg adds dead players within 7 tiles). The cost of one say
//! is the entities of a 15x15 box and the opacity of its 225 tiles, never the number of clients
//! or entities in the world. A body with [`MaskedSenses`] (the AI eye) hears only speakers on
//! tiles its mask shows, its cameras being its sight.
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
    ClientDisconnected, ClientToken, ControlledEntity, IntentReceived, Invisibility, MaskId,
    SeeInvisible, ViewMasks, VisibleToMobs,
};
use ss13_shared::chat::{
    ChatChannel, ChatMessage, ChatSegment, MAX_MESSAGE_LEN, MAX_NAME_LEN, Span,
};
use ss13_shared::line_of_sight::ViewGrid;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Optics, TilePos};
use ss13_world::{SpatialIndex, WorldSystems};

use crate::accents::TongueSpeech;
use crate::emoji::{Emojis, emoji_parse};
use crate::emote::{EMOTE_AUDIO_COOLDOWN_SECS, EmoteOutcome, EmoteSound, EmoteUser};
use crate::language::{self, LanguageHolder, LanguageId, ScrambleSeed, Tongue};
use crate::speech::{
    ANXIOUS_FILLERS, AlienHivemind, Anxiety, AnxiousSay, BinaryCapable, ChangelingHivemind,
    EarsWorn, HolopadSpeaker, Impediments, LastBreath, Muffled, RadioExtension, RadioTarget,
    SpeakerStat, SpeechEffects, Tongueless, VoiceSpan, WhisperHeard, capitalize, derpspeech,
    last_words, muffle, parse_mods_speaking, slur, stars, stutter, stutter_with, unintelligize,
    whisper_heard,
};
use crate::speech_filter::{NOTIFICATIONS as FILTER_NOTIFICATIONS, SpeechFilter, SpeechFiltered};

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

/// tg `TRAIT_SIXTHSENSE` on a living body (the death whispers trauma): it reads deadchat.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct SixthSense;

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

    /// Whether the body perceives through a view mask rather than its own eyes.
    pub fn masked(&self, body: Entity) -> bool {
        self.masked.contains(body)
    }
}

/// tg `get_hearers_in_view(range, source)`: BYOND `hearers(range, turf)`, the tiles the
/// speaker's tile sees, opaque ones (walls, closed doors) stopping it; a tile off the map is
/// opaque.
pub fn hearing_view(
    index: &SpatialIndex,
    optics: &Query<&Optics>,
    center: TilePos,
    range: u32,
) -> ViewGrid {
    let mut grid = ViewGrid::default();
    grid.compute(range.min(64) as i32, |dx, dy| {
        let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
        !index.in_bounds(pos)
            || index
                .on_tile(pos)
                .iter()
                .any(|&e| optics.get(e).is_ok_and(|o| o.opaque))
    });
    grid
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
    /// The language spoken; `None` when the speaker can speak none ([`language::STRANGE_SOUND`]).
    pub language: Option<LanguageId>,
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

/// An intentional emote a body ran (`*key`), for the game's emote effects (tg `run_emote`
/// overrides, e.g. `surrender`, the AI's display) and its sound (`playsound` of `get_sound`,
/// already gated by the audio cooldown and muteness).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Emoted {
    pub speaker: Entity,
    pub key: &'static str,
    pub sound: Option<EmoteSound>,
}

/// A mob in hard crit whispered its last words (tg `succumbed` in `/mob/living/say`): the game
/// makes it succumb (`succumb(whispered = TRUE)`).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastWords {
    pub speaker: Entity,
}

/// A say the Social Anxiety quirk swallowed ([`Anxiety`]), for its game effects: the
/// withdrawal's silence, the once-a-life spaghetti.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnxiousSpoke {
    pub speaker: Entity,
    pub outcome: AnxiousSay,
    /// `prob(1)` of the dumb thing: a plate of spaghetti drops.
    pub spaghetti: bool,
}

/// What kind of message a runechat is ([`Overhead`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverheadKind {
    #[default]
    Say,
    /// `spans |= SPAN_ITALICS`.
    Whisper,
    /// `EMOTE_MESSAGE`.
    Emote,
    /// BandaStation's `spans = list("looc")`.
    Looc,
}

/// One client's overhead text of what it heard or saw (tg `create_chat_message`): the words as
/// that hearer makes them out. The game turns it into a runechat (`ss13_game::runechat`), or
/// not (an unconscious or blind hearer).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct Overhead {
    pub client: Entity,
    pub speaker: Entity,
    /// The name its color comes from (`get_voice`).
    pub name: String,
    pub text: String,
    pub kind: OverheadKind,
}

/// What one group of listeners hears of a message (tg `translate_language` per hearer): the
/// clients, the text, and the language tag before the speaker's name.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub clients: Vec<Entity>,
    pub text: String,
    pub tag: Option<String>,
}

/// Groups listeners `(client, understanding)` by what they hear of `text` in `spoken`: one
/// scramble per level of understanding.
pub fn split_heard(
    seed: u64,
    spoken: Option<LanguageId>,
    text: &str,
    listeners: impl IntoIterator<Item = (Entity, u8)>,
) -> Vec<Heard> {
    let mut groups: Vec<(u8, Heard)> = Vec::new();
    for (client, level) in listeners {
        // Without a language everyone hears the same strange sound.
        let level = if spoken.is_some() {
            level
        } else {
            language::FULL
        };
        if let Some((_, heard)) = groups.iter_mut().find(|(l, _)| *l == level) {
            if !heard.clients.contains(&client) {
                heard.clients.push(client);
            }
            continue;
        }
        groups.push((
            level,
            Heard {
                clients: vec![client],
                text: language::translate(seed, spoken, level, text),
                tag: spoken.and_then(|l| language::tag_text(l, language::tag(l, level))),
            },
        ));
    }
    groups.into_iter().map(|(_, heard)| heard).collect()
}

/// A chat line with the language tag in front (where tg draws the language icon).
pub fn tagged(mut message: ChatMessage, tag: Option<String>) -> ChatMessage {
    if let Some(tag) = tag {
        message
            .segments
            .insert(0, ChatSegment::new(Span::Plain, tag));
    }
    message
}

/// tg `/datum/component/codeword_hearing` on a hearer's body: each list of words (tg one regex
/// `(a|b|c)`, case-insensitive) is highlighted with its span in the speech it hears.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Codewords(pub Vec<(Vec<String>, Span)>);

impl Codewords {
    /// `replace_regex.Replace(message, "<span class='[span_class]'>$1</span>")`: every
    /// segment whose text is the heard `text` is split around the matches.
    pub fn highlight(&self, message: ChatMessage, text: &str) -> ChatMessage {
        let mut segments = Vec::with_capacity(message.segments.len());
        for segment in message.segments {
            if segment.text != text {
                segments.push(segment);
                continue;
            }
            let mut plain_from = 0;
            let mut at = 0;
            while at < text.len() {
                let found = self.0.iter().find_map(|(words, span)| {
                    words
                        .iter()
                        .find_map(|w| match_ci(&text[at..], w))
                        .map(|len| (len, *span))
                });
                if let Some((len, span)) = found {
                    if plain_from < at {
                        segments.push(ChatSegment::new(segment.span, &text[plain_from..at]));
                    }
                    segments.push(ChatSegment::new(span, &text[at..at + len]));
                    at += len;
                    plain_from = at;
                } else {
                    at += text[at..].chars().next().map_or(1, char::len_utf8);
                }
            }
            if plain_from < text.len() {
                segments.push(ChatSegment::new(segment.span, &text[plain_from..]));
            }
        }
        ChatMessage {
            channel: message.channel,
            segments,
        }
    }
}

/// The byte length of `word` at the start of `text`, compared ignoring case.
fn match_ci(text: &str, word: &str) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let mut chars = text.char_indices();
    for w in word.chars() {
        let (_, c) = chars.next()?;
        if !c.to_lowercase().eq(w.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or(text.len(), |(i, _)| i))
}

/// Writes heard speech `message` (holding the heard `text` of the `said` words) to `clients`;
/// a hearer whose body has [`Codewords`] and understood the words (tg `has_language`: the
/// heard text is the said one) gets its own copy with them highlighted.
pub fn write_heard<'a>(
    chat: &mut MessageWriter<SendChat>,
    mut clients: Vec<Entity>,
    message: ChatMessage,
    text: &str,
    said: &str,
    codewords_of: impl Fn(Entity) -> Option<&'a Codewords>,
) {
    if text == said {
        clients.retain(|&client| match codewords_of(client) {
            Some(words) => {
                chat.write(SendChat {
                    clients: vec![client],
                    message: words.highlight(message.clone(), text),
                });
                false
            }
            None => true,
        });
    }
    if !clients.is_empty() {
        chat.write(SendChat { clients, message });
    }
}

/// How listening clients understand a language: the [`LanguageHolder`] of the body each
/// controls (none: Galactic Common only; dead bodies and ghosts: everything).
#[derive(SystemParam)]
pub struct Understanding<'w, 's> {
    controls: Query<'w, 's, &'static ControlledEntity>,
    bodies: Query<
        'w,
        's,
        (
            Option<&'static LanguageHolder>,
            Option<&'static SpeakerStat>,
        ),
    >,
    seed: Option<Res<'w, ScrambleSeed>>,
}

impl Understanding<'_, '_> {
    pub fn seed(&self) -> u64 {
        self.seed.as_deref().map_or(0, |s| s.0)
    }

    pub fn of_body(&self, body: Entity, spoken: LanguageId) -> u8 {
        let (holder, stat) = self.bodies.get(body).unwrap_or((None, None));
        language::understanding(holder, stat == Some(&SpeakerStat::Dead), spoken)
    }

    pub fn of_client(&self, client: Entity, spoken: LanguageId) -> u8 {
        self.controls
            .get(client)
            .map_or(language::FULL, |c| self.of_body(c.0, spoken))
    }

    /// [`split_heard`] of these clients.
    pub fn split(
        &self,
        clients: impl IntoIterator<Item = Entity>,
        spoken: Option<LanguageId>,
        text: &str,
    ) -> Vec<Heard> {
        let levels = clients.into_iter().map(|c| {
            let level = spoken.map_or(language::FULL, |l| self.of_client(c, l));
            (c, level)
        });
        split_heard(self.seed(), spoken, text, levels)
    }
}

/// tg `verb_whisper` (BandaStation).
pub const WHISPER_VERB: &str = "шепчет";

/// tg `say_mod` of `MODE_WHISPER_CRIT`: `"[ru_say_verb(verb_whisper)] своим последним вздохом"`.
pub const LAST_WORDS_VERB: &str = "шепчет своим последним вздохом";

/// tg `emotes_used`: when each emote of this speaker was last used, `Time<Real>` seconds.
#[derive(Component, Default, Debug)]
struct EmotesUsed(HashMap<&'static str, f64>);

/// The [`EmotesUsed`] entry of the last emote sound (`MANUAL_GENERAL_EMOTE_AUDIO_COOLDOWN`); no
/// emote key has a space.
const EMOTE_AUDIO: &str = "audio cooldown";

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

/// tg `handle_spam_prevention`'s counter: say, OOC and LOOC share it.
#[derive(Resource, Default)]
pub struct SayRates(HashMap<Entity, SayRate>);

impl SayRates {
    /// `Err(true)` for the first refusal of a window, `Err(false)` for the rest.
    pub fn admit(&mut self, client: Entity, now: f64) -> Result<(), bool> {
        self.0.entry(client).or_default().admit(now)
    }
}

#[derive(Default)]
pub struct SayRate {
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
            .insert_resource(ScrambleSeed(SpeechRng::default().0))
            .add_message::<SayRejected>()
            .add_message::<Spoke>()
            .add_message::<LastWords>()
            .add_message::<Emoted>()
            .add_message::<AnxiousSpoke>()
            .add_message::<Overhead>()
            .add_message::<SpeechFiltered>()
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
/// spaces), BandaStation's IC filter (`sanitize(apply_ic_filter = TRUE)`,
/// [`ss13_shared::text::ic_filter`]), trimmed, at most [`MAX_MESSAGE_LEN`] characters, first
/// letter capitalized (tg `capitalize`).
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
    let filtered = ss13_shared::text::ic_filter(&cleaned);
    let text = filtered.trim();
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

/// [`whisper_message`] with another verb (the last words).
pub fn whisper_message_verb(name: &str, verb: &str, text: &str) -> ChatMessage {
    quote_message(name, verb, text, Span::Italics)
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

/// tg `say_dead`: `МЕРТВ: Имя говорит, "текст"`, with `emoji_parse` of the text.
pub fn deadchat_message(name: &str, text: &str, emojis: Option<&Emojis>) -> ChatMessage {
    let mut segments = vec![
        ChatSegment::new(Span::Emote, "МЕРТВ: "),
        ChatSegment::new(Span::Name, name),
        ChatSegment::new(Span::Emote, format!(" {}, \"", say_verb(text))),
    ];
    segments.extend(emoji_parse(text, Span::Emote, emojis));
    segments.push(ChatSegment::new(Span::Emote, "\""));
    ChatMessage {
        channel: ChatChannel::Local,
        segments,
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
    (
        Has<ChangelingHivemind>,
        Has<AlienHivemind>,
        Has<HolopadSpeaker>,
    ),
    Has<Tongueless>,
    Option<&'a Impediments>,
    Option<&'a SpeechEffects>,
    Option<&'a LanguageHolder>,
    Option<&'a Tongue>,
    Option<&'a EmoteUser>,
    Option<&'a Anxiety>,
);

fn handle_say(
    mut intents: MessageReader<IntentReceived>,
    controls: Query<&ControlledEntity>,
    muted: Query<(), With<Muted>>,
    bodies: Query<Body>,
    stats: Query<(Entity, &SpeakerStat)>,
    mut emotes_used: Query<&mut EmotesUsed>,
    (
        sight,
        senses,
        voices,
        seed,
        seen_by,
        sixth_sense,
        emojis,
        accents,
        muffled,
        codewords,
        tongue_speech,
        speech_filter,
        tokens,
        optics,
        breaths,
    ): (
        Query<&SeeInvisible>,
        Senses,
        Query<&VoiceSpan>,
        Option<Res<ScrambleSeed>>,
        Query<&VisibleToMobs>,
        Query<Entity, With<SixthSense>>,
        Option<Res<Emojis>>,
        Query<&crate::accents::Accents>,
        Query<(), With<Muffled>>,
        Query<&Codewords>,
        Query<&TongueSpeech>,
        Option<Res<SpeechFilter>>,
        Query<&ClientToken>,
        Query<&Optics>,
        Query<&LastBreath>,
    ),
    index: Res<SpatialIndex>,
    listeners: Res<Listeners>,
    mut rates: ResMut<SayRates>,
    mut rng: ResMut<SpeechRng>,
    time: Res<Time<Real>>,
    mut chat: MessageWriter<SendChat>,
    (mut rejected, mut emoted, mut anxious, mut overhead, mut filtered, mut succumbed): (
        MessageWriter<SayRejected>,
        MessageWriter<Emoted>,
        MessageWriter<AnxiousSpoke>,
        MessageWriter<Overhead>,
        MessageWriter<SpeechFiltered>,
        MessageWriter<LastWords>,
    ),
    mut spoke: MessageWriter<Spoke>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let seed = seed.as_deref().map_or(0, |s| s.0);
    for IntentReceived { client, intent } in intents.read() {
        let ClientIntent::Say(raw) = intent else {
            continue;
        };
        let client = *client;
        let checked = rates
            .admit(client, now)
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
                (hivemind, hive, holopad),
                tongueless,
                impediments,
                speech_effects,
                holder,
                tongue,
                emote_user,
                anxiety,
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
                .map(|(e, _)| e)
                .chain(sixth_sense.iter())
                .filter_map(|e| listeners.client_of(e))
                .collect();
            clients.sort_unstable();
            clients.dedup();
            if !clients.contains(&client) {
                clients.push(client);
            }
            info!(target: "say", "{client} {name} (dead): {text}");
            chat.write(SendChat {
                clients,
                message: deadchat_message(&name, &text, emojis.as_deref()),
            });
            continue;
        }
        // An invisible speaker is heard only by those who see it; an AI eye hears only what its
        // cameras see; the others hear what the speaker's tile sees.
        // shortcut: no `TRAIT_XRAY_HEARING`, no whispers projected through a turf in front.
        let invisibility = invisibility.map_or(0, |i| i.0);
        // (client, distance, body).
        let hearers_within = |range: u32| -> Vec<(Entity, u32, Entity)> {
            let mut view: Option<ViewGrid> = None;
            let mut hearers: Vec<(Entity, u32, Entity)> = index
                .in_radius(*pos, range)
                .filter(|&e| senses.perceives(e, *pos))
                .filter_map(|e| {
                    let client = listeners.client_of(e)?;
                    let (at, dead) = bodies
                        .get(e)
                        .map_or((*pos, false), |b| (*b.1, b.4 == Some(&SpeakerStat::Dead)));
                    if e != speaker && !dead && !senses.masked(e) {
                        let view =
                            view.get_or_insert_with(|| hearing_view(&index, &optics, *pos, range));
                        if !view.visible(at.x - pos.x, at.y - pos.y) {
                            return None;
                        }
                    }
                    let distance = at.chebyshev(*pos).unwrap_or(0);
                    Some((client, distance, e))
                })
                .filter(|&(c, _, body)| {
                    invisibility == 0
                        || sight.get(c).is_ok_and(|s| s.0 >= invisibility)
                        // The mobs it shows itself to (an imaginary friend's owner).
                        || seen_by.get(speaker).is_ok_and(|v| v.0.contains(&body))
                })
                .collect();
            // The speaker always hears itself, even standing off the indexed map.
            if !hearers.iter().any(|&(c, ..)| c == client) {
                hearers.push((client, 0, speaker));
            }
            hearers
        };
        if let Some(act) = text.strip_prefix('*') {
            let mut used = emotes_used.get_mut(speaker).ok();
            let user = emote_user.copied().unwrap_or_default();
            let outcome = crate::emote::run(act, stat, &user, |key, cooldown| {
                used.as_ref()
                    .and_then(|u| u.0.get(key))
                    .is_some_and(|&at| now < at + cooldown)
            });
            match outcome {
                // `muffles_speech/emote_override`: an audible emote without hands is a noise.
                EmoteOutcome::Run { key, .. }
                    if muffled.contains(speaker)
                        && crate::emote::find(key).is_some_and(|e| {
                            e.flags & crate::emote::AUDIBLE != 0
                                && e.flags & crate::emote::REQUIRE_HANDS == 0
                        }) =>
                {
                    let strength = ["сильный ", "слабый ", ""];
                    let pick = (rng.percent() as usize * strength.len()) / 100;
                    let text = format!("издаёт {}звук.", strength[pick.min(strength.len() - 1)]);
                    let clients: Vec<Entity> = hearers_within(HEARING_RANGE)
                        .into_iter()
                        .map(|(c, ..)| c)
                        .collect();
                    chat.write(SendChat {
                        clients,
                        message: emote_message(&name, &text),
                    });
                }
                EmoteOutcome::Run { key, text, sound } => {
                    // `should_play_sound`: a mute or tongueless mob makes no audible sound.
                    let silenced = impediments.is_some_and(|i| i.mute) || tongueless;
                    let sound = sound.filter(|s| !(s.audible && silenced));
                    let audio_busy = sound.is_some()
                        && used
                            .as_ref()
                            .and_then(|u| u.0.get(EMOTE_AUDIO))
                            .is_some_and(|&at| now < at + EMOTE_AUDIO_COOLDOWN_SECS);
                    match used.as_mut() {
                        Some(used) => {
                            used.0.insert(key, now);
                            if sound.is_some() && !audio_busy {
                                used.0.insert(EMOTE_AUDIO, now);
                            }
                        }
                        None => {
                            let mut first = HashMap::from_iter([(key, now)]);
                            if sound.is_some() {
                                first.insert(EMOTE_AUDIO, now);
                            }
                            commands.entity(speaker).try_insert(EmotesUsed(first));
                        }
                    }
                    // BandaStation: `run_emote` returns before the message while the general
                    // emote audio cooldown runs.
                    if audio_busy {
                        continue;
                    }
                    if !text.is_empty() {
                        info!(target: "say", "{client} {name} emote: {text}");
                        let clients: Vec<Entity> = hearers_within(HEARING_RANGE)
                            .into_iter()
                            .map(|(c, ..)| c)
                            .collect();
                        for &hearer in &clients {
                            overhead.write(Overhead {
                                client: hearer,
                                speaker,
                                name: name.clone(),
                                text: text.clone(),
                                kind: OverheadKind::Emote,
                            });
                        }
                        chat.write(SendChat {
                            clients,
                            message: emote_message(&name, &text),
                        });
                    }
                    emoted.write(Emoted {
                        speaker,
                        key,
                        sound,
                    });
                }
                EmoteOutcome::Refused(text) | EmoteOutcome::Help(text) => {
                    chat.write(notice(text));
                }
                EmoteOutcome::Silent => {}
            }
            continue;
        }
        let (mods, message) = parse_mods_speaking(&text, stat, |l| {
            language::can_speak(holder, tongue, tongueless, l)
        });
        // tg `language ||= message_mods[LANGUAGE_EXTENSION] || get_selected_language()`.
        let spoken_in = mods
            .language
            .or_else(|| language::speaking(holder, tongue, tongueless));
        // What each hearer makes of the words (`translate_language`), by their body.
        let understand = |body: Entity| -> u8 {
            let Some(l) = spoken_in else {
                return language::FULL;
            };
            match bodies.get(body) {
                Ok(b) => language::understanding(b.11, b.4 == Some(&SpeakerStat::Dead), l),
                Err(_) => language::basic_understanding(l),
            }
        };
        // tg: an admin verb; anyone else's message goes nowhere.
        if mods.radio == Some(RadioExtension::Deadmin) {
            continue;
        }
        // tg: an unconscious mob says nothing, but one in hard crit may whisper its last words.
        let dying = stat == SpeakerStat::HardCrit && mods.whisper;
        if message.is_empty() || (stat >= SpeakerStat::Unconscious && !dying) {
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
        let mut message = message;
        // The last words: as much as the health left above death lets out.
        if dying {
            let breath = breaths.get(speaker).map_or(0, |b| b.0);
            message = last_words(&message, breath, || rng.percent());
        }
        // `COMSIG_MOB_SAY`: BandaStation's speech filter. It sees the words as typed, before any
        // accent could hide them.
        if let Some(filter) = speech_filter.as_deref()
            && let Some(changed) =
                filter.filter(tokens.get(client).map_or("", |t| t.0.as_str()), &message)
        {
            let i = (rng.percent() as usize * FILTER_NOTIFICATIONS.len()) / 100;
            chat.write(SendChat {
                clients: vec![client],
                message: ChatMessage::simple(
                    ChatChannel::Warning,
                    Span::BoldWarning,
                    FILTER_NOTIFICATIONS[i.min(FILTER_NOTIFICATIONS.len() - 1)],
                ),
            });
            filtered.write(SpeechFiltered {
                client,
                speaker,
                original: std::mem::replace(&mut message, changed),
            });
        }
        // `COMSIG_MOB_SAY`: Social Anxiety may swallow the message.
        // shortcut: no `TRAIT_FEARLESS` or sign language exemption.
        if let Some(anxiety) = anxiety {
            let outcome = anxiety.roll(|| rng.percent());
            let mut tell = |span: Span, text: &str| {
                chat.write(SendChat {
                    clients: vec![client],
                    message: ChatMessage::simple(ChatChannel::Warning, span, text),
                });
            };
            match outcome {
                AnxiousSay::Speaks => {}
                AnxiousSay::DumbThing => {
                    tell(
                        Span::BoldWarning,
                        "Вы вспоминаете глупость, которую сказали давным-давно, и внутренне \
                         кричите.",
                    );
                    commands.entity(speaker).try_insert(Anxiety {
                        dumb_thing: false,
                        ..*anxiety
                    });
                }
                AnxiousSay::Skipped | AnxiousSay::Withdrew => {
                    tell(
                        Span::Warning,
                        "Вы думаете, что это не добавит ничего нового к разговору, и решаете не \
                         говорить об этом.",
                    );
                    if outcome == AnxiousSay::Withdrew {
                        tell(
                            Span::Danger,
                            "Вы уходите в себя. Вы действительно не настроены на разговор.",
                        );
                    }
                }
            }
            if outcome != AnxiousSay::Speaks {
                let spaghetti = outcome == AnxiousSay::DumbThing && rng.percent() < 1.0;
                anxious.write(AnxiousSpoke {
                    speaker,
                    outcome,
                    spaghetti,
                });
                let i = (rng.percent() as usize * ANXIOUS_FILLERS.len()) / 100;
                message = ANXIOUS_FILLERS[i.min(ANXIOUS_FILLERS.len() - 1)].to_owned();
            }
        }
        // `COMSIG_MOB_SAY`: a muzzle (`muffles_speech/muzzle_talk`).
        // shortcut: no `TRAIT_SIGN_LANG` exemption (no sign language).
        if muffled.contains(speaker) {
            message = muffle(&message, || rng.percent());
        }
        // `COMSIG_MOB_SAY`: the speech mutations (`crate::accents`).
        if let Ok(accents) = accents.get(speaker) {
            message = crate::accents::apply_all(&accents.0, &message, || rng.percent());
        }
        // `COMSIG_MOB_SAY`: the tongue, unless the language is native to it.
        if let Ok(&speech) = tongue_speech.get(speaker)
            && !spoken_in.is_some_and(|l| speech.native().contains(&l))
        {
            message = speech.apply(&message, &mut || rng.percent());
        }
        // tg `treat_message`: `TRAIT_UNINTELLIGIBLE_SPEECH`, then the speech status effects.
        if impediments.unintelligible {
            message = unintelligize(&message, || rng.percent());
        }
        let stuttering = impediments.stutter || speech_effects.stutter;
        if stuttering {
            message = stutter(&message, || rng.percent());
        }
        if let Some(anxiety) = anxiety {
            message = stutter_with(&message, &anxiety.stutter(), || rng.percent());
        }
        for effect in speech_effects.slurs.iter().flatten() {
            message = slur(&message, effect, || rng.percent());
        }
        if impediments.derp {
            message = derpspeech(&message, stuttering, || rng.percent());
        }
        // `heckacious/handle_caps`: `TREAT_CAPITALIZE_MESSAGE = FALSE`.
        let mut message = if accents
            .get(speaker)
            .is_ok_and(|a| crate::accents::no_capitalize(&a.0))
        {
            message
        } else {
            capitalize(&message)
        };
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
        let whisper = mods.whisper || stat == SpeakerStat::SoftCrit || impediments.force_whisper;
        let target = match mods.radio_target(ears_worn) {
            // tg checks the saymode of the key first: the AI's `:h` is its holopad.
            _ if holopad && mods.radio == Some(RadioExtension::Department) => {
                Some(RadioTarget::Holopad)
            }
            // tg `/datum/saymode/vocalcords/can_be_used_by`: `iscarbon` (aliens are basic mobs
            // here).
            Some(RadioTarget::VocalCords)
                if !emote_user.is_some_and(|u| u.mob == crate::emote::EmoteMob::Human) =>
            {
                None
            }
            // tg: without `binarycheck()` the binary saymode is not used: plain speech.
            Some(RadioTarget::Binary) if !binary => None,
            // tg `/datum/saymode/changeling/can_be_used_by`: the hivemind trait.
            Some(RadioTarget::Changeling) if !hivemind => None,
            // tg `/datum/saymode/xeno/can_be_used_by`: `hivecheck`.
            Some(RadioTarget::Hive) if !hive => None,
            target => target,
        };
        if matches!(
            target,
            Some(
                RadioTarget::Binary
                    | RadioTarget::Changeling
                    | RadioTarget::Hive
                    | RadioTarget::VocalCords
                    | RadioTarget::Holopad
            )
        ) {
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
                language: spoken_in,
            });
            continue;
        }
        let verb = if dying {
            LAST_WORDS_VERB
        } else if whisper {
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
            language: spoken_in,
        });
        let whisper = whisper || target.is_some();
        info!(target: "say", "{client} {name} at {pos:?}: {message}");
        let codewords_of = |client: Entity| {
            controls
                .get(client)
                .ok()
                .and_then(|body| codewords.get(body.0).ok())
        };
        if !whisper {
            let hearers = hearers_within(HEARING_RANGE)
                .into_iter()
                .map(|(c, _, body)| (c, understand(body)));
            let span = voices.get(speaker).map_or(Span::Say, |v| v.0);
            let verb = say_verb(&message);
            for heard in split_heard(seed, spoken_in, &message, hearers) {
                for &hearer in &heard.clients {
                    overhead.write(Overhead {
                        client: hearer,
                        speaker,
                        name: name.clone(),
                        text: heard.text.clone(),
                        kind: OverheadKind::Say,
                    });
                }
                write_heard(
                    &mut chat,
                    heard.clients,
                    tagged(quote_message(&name, verb, &heard.text, span), heard.tag),
                    &heard.text,
                    &message,
                    codewords_of,
                );
            }
            continue;
        }
        let whisper_verb = if dying { LAST_WORDS_VERB } else { WHISPER_VERB };
        let (mut full, mut starred, mut far) = (Vec::new(), Vec::new(), Vec::new());
        for (hearer, distance, body) in hearers_within(HEARING_RANGE) {
            match whisper_heard(distance) {
                WhisperHeard::Full => full.push((hearer, understand(body))),
                WhisperHeard::Starred => starred.push((hearer, understand(body))),
                WhisperHeard::TooFar => far.push(hearer),
            }
        }
        for heard in split_heard(seed, spoken_in, &message, full) {
            for &hearer in &heard.clients {
                overhead.write(Overhead {
                    client: hearer,
                    speaker,
                    name: name.clone(),
                    text: heard.text.clone(),
                    kind: OverheadKind::Whisper,
                });
            }
            write_heard(
                &mut chat,
                heard.clients,
                tagged(
                    whisper_message_verb(&name, whisper_verb, &heard.text),
                    heard.tag,
                ),
                &heard.text,
                &message,
                codewords_of,
            );
        }
        // Eavesdroppers hear the translated words starred (`/mob/living/Hear`).
        for heard in split_heard(seed, spoken_in, &message, starred) {
            let text = stars(&heard.text, 25.0, || rng.percent());
            for &hearer in &heard.clients {
                overhead.write(Overhead {
                    client: hearer,
                    speaker,
                    name: name.clone(),
                    text: text.clone(),
                    kind: OverheadKind::Whisper,
                });
            }
            chat.write(SendChat {
                clients: heard.clients,
                message: tagged(whisper_message_verb(&name, whisper_verb, &text), heard.tag),
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
        if dying {
            succumbed.write(LastWords { speaker });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadchat_parses_emojis() {
        let emojis = Emojis(
            [("ai".to_owned(), ss13_shared::IconStateId(5))]
                .into_iter()
                .collect(),
        );
        let line = deadchat_message("Боб", "привет :ai:", Some(&emojis));
        assert_eq!(line.plain_text(), "МЕРТВ: Боб говорит, \"привет :ai:\"");
        assert!(
            line.segments
                .iter()
                .any(|s| s.span == Span::Emoji(ss13_shared::IconStateId(5)) && s.text == ":ai:")
        );
    }

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
        // BandaStation's IC filter: no markup characters, emoji or brackets.
        assert_eq!(sanitize_say("<b>x</b>").unwrap(), "Bx/b");
        assert_eq!(sanitize_say("ёлки (палки) 😀").unwrap(), "Ёлки палки");
        assert_eq!(sanitize_say("😀"), Err(SayReject::Empty));
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
