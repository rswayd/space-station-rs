//! Speech modifiers of tg `/mob/living/say` (`get_message_mods`, `living_say.dm`): the prefixes
//! `#` (whisper), `%` (sing), `;` (headset) and `:x`/`.x` (department radio keys, Russian
//! layout included), forced whispers in soft crit, the whisper's hearing falloff and deadchat.

use bevy::prelude::*;

/// The speaker's stat as speech sees it (tg `stat`), kept by the game on the body or ghost.
/// Absent = conscious (anything that is not a mob).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpeakerStat {
    #[default]
    Conscious,
    /// tg `SOFT_CRIT`: `TRAIT_FORCE_WHISPER`, no emotes without `EMOTE_CANUSE_SOFTCRIT`.
    SoftCrit,
    /// tg `HARD_CRIT`: unconscious, no speech.
    HardCrit,
    /// Dead bodies and ghosts: deadchat only.
    Dead,
}

/// On a body that wears something in its ears slot (tg `ears`), kept by the game: a department
/// key without anything there is ordinary speech (`/mob/living/carbon/human/radio`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EarsWorn;

/// On a body that can talk in binary (tg `binarycheck()`: silicons; humans with a binary
/// key are not modelled), kept by the game: `:b` goes to [`RadioTarget::Binary`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinaryCapable;

/// On a body linked to the changeling hivemind (tg `TRAIT_CHANGELING_HIVEMIND`), kept by the
/// game: `:g` goes to [`RadioTarget::Changeling`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangelingHivemind;

/// On a body whose spoken words are drawn in this span instead of `.say` (tg `SPEECH_SPANS`
/// a held megaphone adds), kept by the game.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceSpan(pub ss13_shared::chat::Span);

/// On a body without a tongue (tg: no `TRAIT_SPEAKS_CLEARLY` from one), kept by the game: its
/// speech loses its hard consonants and it gurgles ([`tongueless`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tongueless;

/// What impairs a body's speech (tg `TRAIT_MUTE`, `TRAIT_UNINTELLIGIBLE_SPEECH`, the `stutter`
/// and `derpspeech` status effects), kept by the game (brain traumas).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Impediments {
    /// `can_speak()` fails: no say, whisper or radio.
    pub mute: bool,
    pub stutter: bool,
    pub derp: bool,
    pub unintelligible: bool,
    /// `TRAIT_FORCE_WHISPER` (the Soft-Spoken quirk): every say is a whisper.
    pub force_whisper: bool,
}

/// Timed speech status effects (tg `/datum/status_effect/speech/*`, `silenced`), kept by the
/// game (`ss13_game::status_effects`); they add to [`Impediments`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SpeechEffects {
    pub stutter: bool,
    /// `/datum/status_effect/silenced`: `TRAIT_MUTE`.
    pub silenced: bool,
    /// The slurring effects that run (generic, drunk, cult, heretic), applied in this order.
    pub slurs: [Option<Slur>; 4],
}

/// The text file of a slurring effect (`strings/slurring_*_text.json`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlurTable {
    Drunk,
    Cult,
    Heretic,
}

struct SlurText {
    /// `characters.common`: looked up by the lower-case character.
    common: &'static [(&'static str, &'static str)],
    /// `characters.uncommon`: by the character as written.
    uncommon: &'static [(&'static str, &'static str)],
    string_replacements: &'static [&'static str],
    string_additions: &'static [&'static str],
}

impl SlurTable {
    fn text(self) -> &'static SlurText {
        const DRUNK: SlurText = SlurText {
            common: &[
                ("o", "u"),
                ("s", "ch"),
                ("a", "ah"),
                ("u", "oo"),
                ("c", "k"),
                ("о", "у"),
                ("с", "сш"),
                ("г", "х"),
                ("а", "ах"),
                ("э", "и"),
                ("ц", "сц"),
            ],
            uncommon: &[(" ", "...ммм-м-м..."), (".", "...эээ..."), (",", ", *ик*")],
            string_replacements: &[],
            string_additions: &[],
        };
        const CULT: SlurText = SlurText {
            common: &[
                ("o", "u"),
                ("t", "ch"),
                ("a", "ah"),
                ("u", "oo"),
                ("c", " NAR "),
                ("s", " SIE "),
                ("о", "у"),
                ("т", "чх"),
                ("а", "ах"),
                ("у", "оо"),
                ("ц", " НАР "),
                ("с", " СИ "),
            ],
            uncommon: &[
                ("N", "ОН ИДЁТ"),
                (" ", " нет надежды... "),
                ("Н", " ОН ИДЁТ... "),
            ],
            string_replacements: &["'", "фтх", "нхлу", "чр"],
            string_additions: &["агн"],
        };
        const HERETIC: SlurText = SlurText {
            common: &[
                ("o", "u"),
                ("t", "ch"),
                ("a", "ah"),
                ("c", "th"),
                ("i", "ks"),
                ("m", "nth"),
                ("о", "у"),
                ("т", "ч"),
                ("а", "ах"),
                ("с", "тч"),
                ("и", "кс"),
                ("м", "нтх"),
            ],
            uncommon: &[
                (" ", " нескончаемый... "),
                ("H", " РУКИ... "),
                ("h", " ЯРКИЙ "),
                ("s", " УТЕКАЕТ "),
                ("r", " РАЗЛОМ "),
                ("Р", " РУКИ... "),
                ("я", " ЯРКИЙ "),
                ("с", " УТЕКАЕТ "),
                ("п", " РАЗЛОМ "),
            ],
            string_replacements: &["'", "бр", "тх", "увидь", "етч"],
            string_additions: &["ах", "утх"],
        };
        match self {
            Self::Drunk => &DRUNK,
            Self::Cult => &CULT,
            Self::Heretic => &HERETIC,
        }
    }
}

/// One slurring effect: its table and chances, percent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slur {
    pub table: SlurTable,
    pub common: f32,
    pub uncommon: f32,
    pub replacement: f32,
    pub doubletext: f32,
}

impl Slur {
    /// `/datum/status_effect/speech/slurring/generic`.
    pub const GENERIC: Self = Self {
        table: SlurTable::Drunk,
        common: 33.0,
        uncommon: 0.0,
        replacement: 24.0,
        doubletext: 40.0,
    };
    /// `/datum/status_effect/speech/slurring/cult`.
    pub const CULT: Self = Self {
        table: SlurTable::Cult,
        common: 50.0,
        uncommon: 25.0,
        replacement: 33.0,
        doubletext: 0.0,
    };
    /// `/datum/status_effect/speech/slurring/heretic`.
    pub const HERETIC: Self = Self {
        table: SlurTable::Heretic,
        common: 50.0,
        uncommon: 20.0,
        replacement: 30.0,
        doubletext: 5.0,
    };

    /// `/datum/status_effect/speech/slurring/drunk/handle_message` at `drunk` drunkenness.
    pub fn drunk(drunk: f32) -> Self {
        Self {
            table: SlurTable::Drunk,
            common: (drunk * 0.8 - 16.0).clamp(4.0, 50.0),
            uncommon: (drunk * 0.3 - 10.0).clamp(0.0, 12.0),
            replacement: (drunk * 0.6 - 8.0).clamp(0.0, 12.0),
            doubletext: (drunk * 0.8 - 8.0).clamp(2.0, 50.0),
        }
    }
}

/// tg `/datum/status_effect/speech/slurring/apply_speech` on every word of `text` (`roll`
/// gives 0..100 per call).
pub fn slur(text: &str, slur: &Slur, mut roll: impl FnMut() -> f32) -> String {
    const NO_SLUR: &str = " \"'()[]{}.!?,:;_`~-";
    let table = slur.table.text();
    fn pick(list: &[&'static str], roll: &mut dyn FnMut() -> f32) -> &'static str {
        list[((roll() / 100.0 * list.len() as f32) as usize).min(list.len() - 1)]
    }
    // `replacement_dupe_check`: no replacement right after a replacement.
    let mut dupe = false;
    let mut words = Vec::new();
    for word in text.split(' ') {
        let mut out = String::with_capacity(word.len());
        for (index, c) in word.chars().enumerate() {
            let mut buf = [0u8; 4];
            let original: &str = c.encode_utf8(&mut buf);
            let lower: String = c.to_lowercase().collect();
            let allow = index != 0 && !NO_SLUR.contains(c);
            if roll() < slur.common
                && let Some((_, to)) = table.common.iter().find(|(from, _)| *from == lower)
            {
                out.push_str(to);
                dupe = false;
            } else if roll() < slur.uncommon
                && let Some((_, to)) = table.uncommon.iter().find(|(from, _)| *from == original)
            {
                out.push_str(to);
                dupe = false;
            } else if allow {
                if !dupe && roll() < slur.replacement {
                    let (replacements, additions) =
                        (table.string_replacements, table.string_additions);
                    // tg weighs `(replacements + additions) / replacements * 100`, at least 100,
                    // so a table with both never appends; the intent is the replacements' share.
                    let replace = match (replacements.is_empty(), additions.is_empty()) {
                        (false, false) => {
                            let share = replacements.len() as f32
                                / (replacements.len() + additions.len()) as f32;
                            roll() < share * 100.0
                        }
                        (false, true) => true,
                        _ => false,
                    };
                    if replace {
                        out.push_str(pick(replacements, &mut roll));
                    } else if !additions.is_empty() {
                        out.push_str(original);
                        out.push_str(pick(additions, &mut roll));
                    } else {
                        out.push_str(original);
                    }
                    dupe = true;
                } else if roll() < slur.doubletext {
                    let times = if roll() < 50.0 { 2 } else { 3 };
                    for _ in 0..times {
                        out.push_str(original);
                    }
                    dupe = false;
                } else {
                    out.push_str(original);
                }
            } else {
                out.push_str(original);
                // "Y'''e": no replacement after the first character or punctuation either.
                dupe = true;
            }
        }
        words.push(out);
    }
    words.join(" ")
}

/// tg `/datum/status_effect/speech/stutter/stutter_char` (`roll` gives 0..100 per call).
fn stutter_char(c: &str, roll: &mut impl FnMut() -> f32) -> String {
    let times = if roll() < 10.0 {
        4
    } else if roll() < 20.0 {
        3
    } else if roll() < 90.0 {
        2
    } else {
        1
    };
    vec![c; times].join("-")
}

/// tg `/datum/status_effect/speech/stutter/apply_speech` on one word: 75% of words repeat
/// their first sound (`stutter_regex`, then BandaStation's Cyrillic consonant regex).
fn stutter_word(word: &str, roll: &mut impl FnMut() -> f32) -> String {
    const LEADING: &str = " \t\"'()[]{}.!?,:;_`~-";
    const CYRILLIC: &str = "бвгджзйклмнпрстфхцчшщБВГДЖЗЙКЛМНПРСТФХЦЧШЩ";
    if roll() >= 75.0 {
        return word.to_owned();
    }
    let rest = word.trim_start_matches(|c| LEADING.contains(c));
    let lead = &word[..word.len() - rest.len()];
    let mut chars = rest.chars();
    let Some(first) = chars.next() else {
        return word.to_owned();
    };
    let sound_len = if (first.is_ascii_alphanumeric() || first == '_') && !first.is_ascii_digit() {
        // `([^aeoiuh\d]h|qu|[^\d])`, case-insensitive.
        let second = chars.next().map(|c| c.to_ascii_lowercase());
        let lower = first.to_ascii_lowercase();
        let digraph = (second == Some('h') && !"aeoiuh".contains(lower))
            || (lower == 'q' && second == Some('u'));
        if digraph { 2 } else { 1 }
    } else if CYRILLIC.contains(first) {
        1
    } else {
        return word.to_owned();
    };
    let split = rest
        .char_indices()
        .nth(sound_len)
        .map_or(rest.len(), |(i, _)| i);
    let (sound, tail) = rest.split_at(split);
    format!("{lead}{}{tail}", stutter_char(sound, roll))
}

/// The stutter of every word of `text`.
pub fn stutter(text: &str, mut roll: impl FnMut() -> f32) -> String {
    text.split(' ')
        .map(|w| stutter_word(w, &mut roll))
        .collect::<Vec<_>>()
        .join(" ")
}

/// tg `/datum/status_effect/speech/stutter/derpspeech/handle_message`: English simplifications,
/// half the time shouted with a stuttered exclamation, and 15% of the time stuttered unless
/// the speaker stutters anyway.
pub fn derpspeech(text: &str, stuttering: bool, mut roll: impl FnMut() -> f32) -> String {
    let mut message = text.to_owned();
    for (from, to) in [
        (" am ", " "),
        (" is ", " "),
        (" are ", " "),
        ("you", "u"),
        ("help", "halp"),
        ("grief", "grife"),
        ("space", "spess"),
        ("carp", "crap"),
        ("reason", "raisin"),
    ] {
        message = message.replace(from, to);
    }
    if roll() < 50.0 {
        let exclamation = ["!", "!!", "!!!"][(roll() as usize * 3) / 100];
        message = message.to_uppercase();
        message.push_str(&stutter_char(exclamation, &mut roll));
    }
    if !stuttering && roll() < 15.0 {
        message = stutter(&message, roll);
    }
    message
}

/// tg `unintelligize`: the words in a random order.
pub fn unintelligize(text: &str, mut roll: impl FnMut() -> f32) -> String {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    for i in (1..words.len()).rev() {
        let j = ((roll() / 100.0) * (i + 1) as f32) as usize;
        words.swap(i, j.min(i));
    }
    words.join(" ")
}

/// tg `/mob/living/carbon/human/say` without `TRAIT_SPEAKS_CLEARLY`: every run of `[gdntke]`
/// (and `[GDNTKE]`) becomes `replacement` ("aa", "oo" or "'", one pick per message).
// tg's pattern is Latin only, so Cyrillic speech passes unchanged, as on the original server.
pub fn tongueless(text: &str, lower: &str, upper: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run: Option<bool> = None;
    for c in text.chars() {
        let class = match c {
            'g' | 'd' | 'n' | 't' | 'k' | 'e' => Some(false),
            'G' | 'D' | 'N' | 'T' | 'K' | 'E' => Some(true),
            _ => None,
        };
        if class.is_some() && class == run {
            continue;
        }
        run = class;
        match class {
            Some(false) => out.push_str(lower),
            Some(true) => out.push_str(upper),
            None => out.push(c),
        }
    }
    out
}

/// tg `RADIO_EXTENSION`: what a `:x` / `.x` key names (`GLOB.department_radio_keys`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RadioExtension {
    /// `MODE_DEPARTMENT` (`:h`): the headset's first channel.
    Department,
    /// A `RADIO_CHANNEL_*` name.
    Channel(&'static str),
    /// `MODE_INTERCOM` (`:i`): every intercom within 1 tile.
    Intercom,
    /// `MODE_R_HAND`, `MODE_L_HAND`: the item in that hand.
    RightHand,
    LeftHand,
    /// `MODE_DEADMIN` (`:d`): an admin verb; anyone else's message is dropped.
    Deadmin,
    /// `MODE_KEY_BINARY` (`:b`, BandaStation `:и`): tg's binary saymode.
    Binary,
    /// `MODE_KEY_CHANGELING` (`:g`, BandaStation `:п`): tg's changeling saymode.
    Changeling,
}

/// Where a message goes besides the air (tg `radio()`), already resolved by priority: the hand
/// and intercom extensions, then `;`, then the department and channel keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RadioTarget {
    /// `MODE_HEADSET` (`;`): the headset's own frequency.
    Headset,
    Department,
    Channel(&'static str),
    Intercom,
    /// Held index: 0 left, 1 right.
    Hand(usize),
    /// tg `robot_talk` (the binary saymode): every binary-capable mob and ghost hears it, nothing
    /// is said aloud.
    Binary,
    /// tg's changeling saymode: every hivemind member and ghost reads it, nothing is said aloud.
    Changeling,
}

/// tg `MESSAGE_RANGE - WHISPER_RANGE` + `WHISPER_RANGE`: whispers are heard in full at 1 tile,
/// starred one tile farther (`EAVESDROP_RANGE`), seen as "whispers something" up to 7.
pub const WHISPER_RANGE: u32 = 1;
pub const EAVESDROP_RANGE: u32 = 1;

/// The modifiers found in front of a message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SayMods {
    pub whisper: bool,
    pub sing: bool,
    /// `MODE_HEADSET` (`;`).
    pub headset: bool,
    pub radio: Option<RadioExtension>,
    /// `LANGUAGE_EXTENSION` (`,x`): the language spoken instead of the selected one.
    pub language: Option<crate::language::LanguageId>,
}

impl SayMods {
    /// tg `/mob/living/carbon/human/radio` after `/mob/living/radio`: the target and whether the
    /// spoken words shrink to a whisper (`REDUCE_RANGE`). A channel key reaches the headset
    /// only when something is worn in the ears slot; otherwise it is ordinary speech.
    pub fn radio_target(&self, ears_worn: bool) -> Option<RadioTarget> {
        match (self.radio, self.headset) {
            // tg checks the saymode before the radio.
            (Some(RadioExtension::Binary), _) => Some(RadioTarget::Binary),
            (Some(RadioExtension::Changeling), _) => Some(RadioTarget::Changeling),
            (Some(RadioExtension::Intercom), _) => Some(RadioTarget::Intercom),
            (Some(RadioExtension::LeftHand), _) => Some(RadioTarget::Hand(0)),
            (Some(RadioExtension::RightHand), _) => Some(RadioTarget::Hand(1)),
            (_, true) => Some(RadioTarget::Headset),
            (Some(RadioExtension::Department), _) => Some(RadioTarget::Department),
            (Some(RadioExtension::Channel(name)), _) if ears_worn => {
                Some(RadioTarget::Channel(name))
            }
            _ => None,
        }
    }
}

/// `GLOB.department_radio_keys`, both keyboard layouts. The DM list repeats some keys and the
/// later entry wins: `l` is the justice channel (not the left hand), `p` and `з` entertainment
/// (not admin say); `в` maps to the text `"d"`, which is no mode.
fn radio_key(key: char) -> Option<RadioExtension> {
    use crate::radio::*;
    let channel = |name| Some(RadioExtension::Channel(name));
    match key.to_lowercase().next()? {
        'r' | 'к' => Some(RadioExtension::RightHand),
        'л' => Some(RadioExtension::LeftHand),
        'i' | 'ш' => Some(RadioExtension::Intercom),
        'h' | 'р' => Some(RadioExtension::Department),
        'c' | 'с' => channel(RADIO_CHANNEL_COMMAND),
        'n' | 'т' => channel(RADIO_CHANNEL_SCIENCE),
        'm' | 'ь' => channel(RADIO_CHANNEL_MEDICAL),
        'e' | 'у' => channel(RADIO_CHANNEL_ENGINEERING),
        's' | 'ы' => channel(RADIO_CHANNEL_SECURITY),
        'l' | 'д' => channel(RADIO_CHANNEL_JUSTICE),
        'u' | 'г' => channel(RADIO_CHANNEL_SUPPLY),
        'v' | 'м' => channel(RADIO_CHANNEL_SERVICE),
        't' | 'е' => channel(RADIO_CHANNEL_SYNDICATE),
        'z' => channel(RADIO_CHANNEL_UPLINK),
        'y' | 'н' => channel(RADIO_CHANNEL_CENTCOM),
        'o' | 'щ' => channel(RADIO_CHANNEL_AI_PRIVATE),
        'p' | 'з' => channel(RADIO_CHANNEL_ENTERTAINMENT),
        'd' => Some(RadioExtension::Deadmin),
        'b' | 'и' => Some(RadioExtension::Binary),
        'g' | 'п' => Some(RadioExtension::Changeling),
        _ => None,
    }
}

/// [`parse_mods_speaking`] of a speaker that can speak every language.
pub fn parse_mods(message: &str, stat: SpeakerStat) -> (SayMods, String) {
    parse_mods_speaking(message, stat, |_| true)
}

/// tg `get_message_mods`: up to 7 prefixes are stripped; a doubled first character ("...",
/// ";;") is never a prefix. `;` counts only above crit, but is stripped anyway; a second
/// department key after a known one is text. `,x` speaks the language of key `x` when
/// `can_speak` allows it; an unknown or unspeakable key ends the prefixes, left in the text.
pub fn parse_mods_speaking(
    message: &str,
    stat: SpeakerStat,
    can_speak: impl Fn(crate::language::LanguageId) -> bool,
) -> (SayMods, String) {
    let mut mods = SayMods::default();
    let mut message = message.to_owned();
    for _ in 0..7 {
        let mut chars = message.chars();
        let (Some(first), second) = (chars.next(), chars.next()) else {
            break;
        };
        if second == Some(first) {
            break;
        }
        let chop = match first {
            '#' if !mods.whisper => {
                mods.whisper = true;
                1
            }
            '%' if !mods.sing => {
                mods.sing = true;
                1
            }
            ';' if !mods.headset => {
                if stat < SpeakerStat::SoftCrit {
                    mods.headset = true;
                }
                1
            }
            ':' | '.' if message.chars().count() > 2 && mods.radio.is_none() => {
                mods.radio = radio_key(second.expect("more than two characters"));
                2
            }
            ',' if mods.language.is_none() => {
                // "No, you cannot speak in xenocommon just because you know the key".
                let Some(language) = second
                    .and_then(crate::language::LanguageId::from_key)
                    .filter(|&l| can_speak(l))
                else {
                    break;
                };
                mods.language = Some(language);
                2
            }
            _ => break,
        };
        message = message
            .chars()
            .skip(chop)
            .collect::<String>()
            .trim_start()
            .to_owned();
        if message.is_empty() {
            break;
        }
    }
    // `message_modes_stat_limits`: only the intercom works below conscious (up to hard crit).
    let limit = match mods.radio {
        Some(RadioExtension::Intercom) => SpeakerStat::HardCrit,
        _ => SpeakerStat::Conscious,
    };
    if stat > limit {
        mods.radio = None;
    }
    (mods, message)
}

/// tg `capitalize`.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// tg `stars`: each non-space character becomes `*` with `probability` percent, rolled by
/// `roll` (a value in 0..100 per character).
pub fn stars(text: &str, probability: f32, mut roll: impl FnMut() -> f32) -> String {
    text.chars()
        .map(|c| {
            if c != ' ' && roll() < probability {
                '*'
            } else {
                c
            }
        })
        .collect()
}

/// How a whisper reaches a hearer at `distance` (tg `/mob/living/Hear` with a whisper's
/// `message_range` of 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhisperHeard {
    Full,
    Starred,
    /// "X шепчет что-то, но вы слишком далеко...".
    TooFar,
}

pub fn whisper_heard(distance: u32) -> WhisperHeard {
    match distance.saturating_sub(WHISPER_RANGE) {
        0 => WhisperHeard::Full,
        d if d <= EAVESDROP_RANGE => WhisperHeard::Starred,
        _ => WhisperHeard::TooFar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tongueless_speech_follows_tg_regex() {
        assert_eq!(
            tongueless("Get the knife!", "aa", "AA"),
            "AAaa aahaa aaifaa!"
        );
        assert_eq!(tongueless("Тут нож", "'", "'"), "Тут нож");
    }

    #[test]
    fn stutter_repeats_the_first_sound() {
        // Every roll 50: each word stutters (< 75) twice (not < 10, not < 20, < 90).
        let half = || 50.0;
        assert_eq!(stutter("the quick dog", half), "th-the qu-quick d-dog");
        assert_eq!(stutter("\"Привет, это я", half), "\"П-Привет, это я");
        assert_eq!(stutter("42 ok", half), "42 o-ok");
        // A roll of 80 leaves words alone.
        assert_eq!(stutter("hello", || 80.0), "hello");
    }

    #[test]
    fn derpspeech_and_unintelligible_speech() {
        assert_eq!(derpspeech("you are in space", true, || 60.0), "u in spess");
        assert_eq!(derpspeech("help", true, || 0.0), "HALP!-!-!-!");
        let shuffled = unintelligize("один два три", || 0.0);
        let mut words: Vec<&str> = shuffled.split(' ').collect();
        assert_ne!(shuffled, "один два три");
        words.sort_unstable();
        assert_eq!(words, ["два", "один", "три"]);
    }
    use crate::radio::*;

    #[test]
    fn prefixes_follow_get_message_mods() {
        let c = SpeakerStat::Conscious;
        assert_eq!(
            parse_mods("#тихо", c),
            (
                SayMods {
                    whisper: true,
                    ..default()
                },
                "тихо".into()
            )
        );
        let (mods, text) = parse_mods("; всем привет", c);
        assert_eq!((mods.headset, text.as_str()), (true, "всем привет"));
        let ext = |m: &str| parse_mods(m, c).0.radio;
        let channel = RadioExtension::Channel;
        assert_eq!(ext(":h отдел"), Some(RadioExtension::Department));
        assert_eq!(ext(".р отдел"), Some(RadioExtension::Department));
        assert_eq!(parse_mods(":ы охрана", c).1, "охрана");
        // Every key of `GLOB.department_radio_keys`, both layouts, upper case too.
        for (keys, want) in [
            ("eEуУ", channel(RADIO_CHANNEL_ENGINEERING)),
            ("sы", channel(RADIO_CHANNEL_SECURITY)),
            ("cс", channel(RADIO_CHANNEL_COMMAND)),
            ("nт", channel(RADIO_CHANNEL_SCIENCE)),
            ("mь", channel(RADIO_CHANNEL_MEDICAL)),
            ("uг", channel(RADIO_CHANNEL_SUPPLY)),
            ("vм", channel(RADIO_CHANNEL_SERVICE)),
            ("lд", channel(RADIO_CHANNEL_JUSTICE)),
            ("tе", channel(RADIO_CHANNEL_SYNDICATE)),
            ("z", channel(RADIO_CHANNEL_UPLINK)),
            ("yн", channel(RADIO_CHANNEL_CENTCOM)),
            ("oщ", channel(RADIO_CHANNEL_AI_PRIVATE)),
            ("pз", channel(RADIO_CHANNEL_ENTERTAINMENT)),
            ("iш", RadioExtension::Intercom),
            ("rк", RadioExtension::RightHand),
            ("л", RadioExtension::LeftHand),
            ("d", RadioExtension::Deadmin),
            ("bи", RadioExtension::Binary),
        ] {
            for key in keys.chars() {
                assert_eq!(ext(&format!(":{key} x")), Some(want), "key {key}");
            }
        }
        // `в` is the text "d" in the list, `j` a mode living mobs ignore: stripped, no radio.
        assert_eq!(parse_mods(":в hm", c), (SayMods::default(), "hm".into()));
        assert_eq!(parse_mods(":j hm", c), (SayMods::default(), "hm".into()));
        // Unknown keys are still stripped; doubled characters are text.
        assert_eq!(parse_mods(":q hm", c).1, "hm");
        assert_eq!(parse_mods("...ну", c), (SayMods::default(), "...ну".into()));
        assert_eq!(parse_mods(";;", c), (SayMods::default(), ";;".into()));
        // A second key after a known one is text; after an unknown one it is parsed.
        let (mods, text) = parse_mods(":e :s x", c);
        assert_eq!(
            (mods.radio, text.as_str()),
            (Some(channel(RADIO_CHANNEL_ENGINEERING)), ":s x")
        );
        assert_eq!(ext(":q :s x"), Some(channel(RADIO_CHANNEL_SECURITY)));
        // Several prefixes.
        let (mods, text) = parse_mods("#;секрет", c);
        assert!(mods.whisper && mods.headset);
        assert_eq!(text, "секрет");
        // No radio from crit; the prefix is gone anyway. The intercom works in soft crit.
        let (mods, text) = parse_mods("; помогите", SpeakerStat::SoftCrit);
        assert_eq!((mods.headset, text.as_str()), (false, "помогите"));
        assert_eq!(parse_mods(":e x", SpeakerStat::SoftCrit).0.radio, None);
        assert_eq!(
            parse_mods(":i x", SpeakerStat::SoftCrit).0.radio,
            Some(RadioExtension::Intercom)
        );
    }

    #[test]
    fn language_key_follows_get_message_mods() {
        use crate::language::LanguageId;
        let c = SpeakerStat::Conscious;
        let draconic = |l: LanguageId| l == LanguageId::Draconic;
        let (mods, text) = parse_mods_speaking(",o шшш", c, draconic);
        assert_eq!(mods.language, Some(LanguageId::Draconic));
        assert_eq!(text, "шшш");
        // With a radio key, in any order.
        let (mods, text) = parse_mods_speaking(";,o привет", c, draconic);
        assert!(mods.headset);
        assert_eq!(
            (mods.language, text.as_str()),
            (Some(LanguageId::Draconic), "привет")
        );
        // A language the speaker cannot speak, or no such key: the text stays as typed.
        assert_eq!(
            parse_mods_speaking(",m ну", c, draconic),
            (SayMods::default(), ",m ну".into())
        );
        assert_eq!(
            parse_mods_speaking(",q ну", c, draconic),
            (SayMods::default(), ",q ну".into())
        );
        assert_eq!(parse_mods_speaking(",,", c, draconic).1, ",,");
    }

    #[test]
    fn radio_target_follows_human_radio() {
        let c = SpeakerStat::Conscious;
        let target = |m: &str, ears: bool| parse_mods(m, c).0.radio_target(ears);
        assert_eq!(target("; x", false), Some(RadioTarget::Headset));
        assert_eq!(target(":h x", false), Some(RadioTarget::Department));
        // A channel key needs something in the ears slot, else it is plain speech.
        assert_eq!(target(":e x", false), None);
        assert_eq!(
            target(":e x", true),
            Some(RadioTarget::Channel(RADIO_CHANNEL_ENGINEERING))
        );
        // Hands and intercom come first (`/mob/living/radio`), then `;`.
        assert_eq!(target(":i ;x", true), Some(RadioTarget::Intercom));
        assert_eq!(target(";:к x", true), Some(RadioTarget::Hand(1)));
        assert_eq!(target(":e ;x", true), Some(RadioTarget::Headset));
        assert_eq!(target("x", true), None);
    }

    #[test]
    fn whispers_fade_with_distance() {
        assert_eq!(whisper_heard(0), WhisperHeard::Full);
        assert_eq!(whisper_heard(1), WhisperHeard::Full);
        assert_eq!(whisper_heard(2), WhisperHeard::Starred);
        assert_eq!(whisper_heard(3), WhisperHeard::TooFar);
        assert_eq!(whisper_heard(7), WhisperHeard::TooFar);
        assert_eq!(stars("аб в", 100.0, || 0.0), "** *");
        assert_eq!(stars("аб в", 25.0, || 50.0), "аб в");
    }

    #[test]
    fn slurring_follows_tg_tables() {
        // Every roll 0: the common replacement of a known character wins.
        assert_eq!(slur("со", &Slur::CULT, || 0.0), " СИ у");
        // Every roll 99: nothing happens.
        assert_eq!(slur("привет всем", &Slur::CULT, || 99.0), "привет всем");
        // The first character is never replaced by a string or doubled, and blocks a string
        // replacement after it (`replacement_dupe_check`). Rolls: "в" common (no entry) 99,
        // uncommon 99; "ы" common 99, uncommon 99, doubletext 0, two copies 0.
        let mut rolls = [99.0, 99.0, 99.0, 99.0, 0.0, 0.0].into_iter();
        assert_eq!(slur("вы", &Slur::GENERIC, || rolls.next().unwrap()), "выы");
        // Drunk text has no string replacements: a replacement roll changes nothing. Rolls:
        // "а" common 0 ("ах"); "о" common 99, uncommon 99, replacement 0.
        let mut rolls = [0.0, 99.0, 99.0, 0.0].into_iter();
        assert_eq!(
            slur("ао", &Slur::drunk(60.0), || rolls.next().unwrap()),
            "ахо"
        );
    }
}
