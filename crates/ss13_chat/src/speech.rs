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

/// On a body without a tongue (tg: no `TRAIT_SPEAKS_CLEARLY` from one), kept by the game: its
/// speech loses its hard consonants and it gurgles ([`tongueless`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tongueless;

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

/// tg `get_message_mods`: up to 7 prefixes are stripped; a doubled first character ("...",
/// ";;") is never a prefix. `;` counts only above crit, but is stripped anyway; a second
/// department key after a known one is text.
pub fn parse_mods(message: &str, stat: SpeakerStat) -> (SayMods, String) {
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
}
