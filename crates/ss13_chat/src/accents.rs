//! Speech-changing genetic mutations (tg `code/datums/mutations/speech.dm`, the
//! `/datum/component/speechmod` of the Swedish, Chav, Elvis and Hulk mutations, the Medieval and
//! Pig Latin `COMSIG_MOB_SAY` handlers), applied by `crate::server` where tg sends
//! `COMSIG_MOB_SAY`. The game keeps [`Accents`] on the body (`ss13_game::genetics`).
// shortcut: `medieval_replacement.json` is ported in part (the common words, every starting).

use bevy::prelude::*;

/// The Hulk's `speechmod` replacements.
const HULK: Table = &[(".", &["!"])];

/// One accent of a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Accent {
    Swedish,
    Chav,
    Elvis,
    Medieval,
    PigLatin,
    /// The Hulk's speechmod: `.` becomes `!`, `!!` at the end, all upper case.
    Hulk,
}

/// The accents of a body, in the order they were gained.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Accents(pub Vec<Accent>);

type Table = &'static [(&'static str, &'static [&'static str])];

/// `swedish/language_mutilation`.
const SWEDISH: Table = &[
    ("w", &["v"]),
    ("j", &["y"]),
    ("bo", &["bjo"]),
    ("a", &["å", "ä", "æ", "a"]),
    ("o", &["ö", "ø", "o"]),
];

/// `strings/chav_replacement.json`.
const CHAV: Table = &[
    ("fucking", &["bloody"]),
    ("dick", &["knob"]),
    ("dickhead", &["bellend"]),
    ("coffee", &["tea"]),
    ("pussy", &["fanny"]),
    ("my", &["me"]),
    ("stupid", &["daft"]),
    ("looking at", &["gawpin' at"]),
    ("head", &["dome"]),
    ("bar", &["pub"]),
    ("water", &["wa'er"]),
    ("music", &["tunes"]),
    ("dissapointed", &["gutted"]),
    ("vacuum", &["hoover"]),
    ("sus", &["dodgy"]),
    ("sleep", &["kip"]),
    ("tired", &["knackered"]),
    ("fries", &["chips"]),
    ("chips", &["crisps"]),
    ("what", &["wot"]),
    ("robust", &["chin"]),
    ("windows", &["windies"]),
    ("window", &["windy"]),
    ("break", &["do"]),
    ("your", &["yer"]),
    ("no", &["naw"]),
    ("mom", &["mum"]),
    ("hair", &["mop"]),
    ("nothing", &["bugger all"]),
    ("food", &["scran"]),
    ("security", &["coppers", "bobbies"]),
    ("friend", &["mate", "bruv", "lad"]),
    ("great", &["bangin'", "sound"]),
    ("man", &["mate", "mans", "bloke"]),
    ("isnt", &["innit", "aint it"]),
    ("dumb", &["daft", "stupid"]),
    ("idiot", &["wanker", "tosser", "prick"]),
    ("little", &["tad", "wee"]),
    ("credits", &["pounds", "quid"]),
    ("drunk", &["shitfaced", "pissed"]),
    ("tough", &["big"]),
];

/// `strings/elvis_replacement.json`.
const ELVIS: Table = &[
    ("i'm not ", &["I ain't "]),
    (" girl ", &[" honey ", " baby ", " baby doll "]),
    (
        " man ",
        &[" son ", " buddy ", " brother ", " pal ", " friendo "],
    ),
    (" out of ", &[" outta "]),
    (" thank you ", &[" thank you, thank you very much "]),
    (" thanks ", &[" thank you, thank you very much "]),
    (" what are you", &[" whatcha "]),
    (" yes", &[" sure ", " yea "]),
    (" muh valids ", &[" my kicks "]),
];

/// `medieval_replacement.json` `startings`.
const MEDIEVAL_STARTINGS: &[&str] = &[
    "Ah,",
    "Alas,",
    "Avast,",
    "By my faith,",
    "By my trowth,",
    "By my word,",
    "Come,",
    "Forsooth, I say,",
    "Forsooth, say I,",
    "Forsooth, sayeth I,",
    "Forsooth,",
    "Hark,",
    "Harketh,",
    "Hear me,",
    "Heigh-ho,",
    "I pray you,",
    "I say,",
    "I sayeth,",
    "I warrant",
    "If it pleases you,",
    "In short,",
    "In sooth,",
    "In truth,",
    "Kind sire,",
    "Listen thee,",
    "Listen,",
    "Now hear me,",
    "Perchance,",
    "Pray pardon,",
    "Pray tell,",
    "Pray,",
    "Prithee,",
    "Quoth I,",
    "S'wounds,",
    "Sire,",
    "Surely",
    "There is much in what you say, and yet,",
    "What hey,",
    "What ho,",
    "Z'wounds,",
    "Zounds,",
];

/// `medieval_replacement.json` `medieval` (the start of it).
const MEDIEVAL: Table = &[
    ("in the", &["i' the"]),
    ("it is", &["'tis"]),
    ("it was", &["'twas"]),
    ("it were", &["'twere"]),
    ("it will", &["'twould"]),
    ("it's", &["'tis"]),
    ("over there", &["yonder"]),
    ("shall not", &["shan't"]),
    ("there", &["thither"]),
    ("will not", &["shan't"]),
    (
        "thank you",
        &[
            "grammercy to you",
            "kindly thanks to you",
            "many good thanks to you",
            "thankee",
        ],
    ),
    (
        "thanks",
        &[
            "grammercy to you",
            "kindly thanks to you",
            "many good thanks to you",
            "thankee",
        ],
    ),
    ("you", &["thou", "thee", "ye"]),
    ("your", &["thine", "thy", "thyne"]),
    ("are", &["art"]),
    (
        "killed",
        &[
            "bested",
            "brung low",
            "conquered",
            "fleeced",
            "foiled",
            "humbled",
            "slain",
            "subjugated",
            "vanquished",
        ],
    ),
    (
        "kill",
        &[
            "best",
            "bring low",
            "conquer",
            "fleece",
            "foil",
            "humble",
            "slay",
            "subjugate",
            "vanquish",
        ],
    ),
];

/// BYOND `pick` over `roll` (a value in 0..100).
fn pick<'a>(list: &[&'a str], roll: &mut impl FnMut() -> f32) -> &'a str {
    let i = (roll() / 100.0 * list.len() as f32) as usize;
    list[i.min(list.len() - 1)]
}

/// `speechmod/handle_speech`: every key replaced (`replacetextEx`, case-sensitive) by one
/// picked value, trimmed, then `end_string` with `end_chance` percent.
fn speechmod(
    text: &str,
    table: Table,
    end: &[&str],
    end_chance: f32,
    roll: &mut impl FnMut() -> f32,
) -> String {
    let mut message = text.to_owned();
    for (key, values) in table {
        let value = pick(values, roll);
        message = message.replace(key, value);
    }
    let mut message = message.trim().to_owned();
    if !end.is_empty() && roll() < end_chance {
        message.push_str(pick(end, roll));
    }
    message.trim().to_owned()
}

/// `replacetextEx(message, regex("\b<key>\b", "ig"), value)`: whole-word, case-insensitive.
fn replace_words(text: &str, key: &str, value: &str) -> String {
    let lower = text.to_lowercase();
    // Case folding may change byte lengths outside ASCII: fall back to the text as is.
    if lower.len() != text.len() || key.is_empty() {
        return text.to_owned();
    }
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(key) {
        let start = at + found;
        let end = start + key.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        out.push_str(&text[at..start]);
        if is_word(before) || is_word(after) {
            out.push_str(&text[start..end]);
        } else {
            out.push_str(value);
        }
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// `medieval/handle_speech`.
fn medieval(text: &str, roll: &mut impl FnMut() -> f32) -> String {
    let mut message = format!(" {text} ");
    for (key, values) in MEDIEVAL {
        let value = pick(values, roll);
        message = replace_words(&message, key, value);
    }
    let starting = pick(MEDIEVAL_STARTINGS, roll);
    format!("{starting} {}", message.trim())
}

/// `VOWELS` of `piglatin_word`.
fn vowel(c: Option<char>) -> bool {
    c.is_some_and(|c| "aeiouаеёиоуыэюя".contains(c))
}

/// `piglatin_word`.
fn piglatin_word(word: &str, roll: &mut impl FnMut() -> f32) -> String {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() <= 1 {
        return word.to_owned();
    }
    if vowel(chars.first().copied()) {
        return format!("{word}{}", pick(&["yay", "way", "hay"], roll));
    }
    let skip = if vowel(chars.get(1).copied()) { 1 } else { 2 };
    let head: String = chars[..skip.min(chars.len())].iter().collect();
    let tail: String = chars[skip.min(chars.len())..].iter().collect();
    format!("{tail}{head}ay")
}

/// `piglatin_sentence`.
pub fn piglatin(text: &str, roll: &mut impl FnMut() -> f32) -> String {
    let text = text.to_lowercase();
    let body = text.trim_end_matches(['!', '?', '.', '-']);
    let punctuation = &text[body.len()..];
    let words: Vec<String> = body.split(' ').map(|w| piglatin_word(w, roll)).collect();
    format!(
        "{}{punctuation}",
        crate::speech::capitalize(&words.join(" "))
    )
}

impl Accent {
    /// What this accent makes of `text`; `roll` gives values in 0..100.
    pub fn apply(self, text: &str, roll: &mut impl FnMut() -> f32) -> String {
        match self {
            Self::Swedish => speechmod(text, SWEDISH, &["", ", bork", ", bork, bork"], 30.0, roll),
            Self::Chav => speechmod(text, CHAV, &[", mate"], 30.0, roll),
            Self::Elvis => speechmod(text, ELVIS, &[], 0.0, roll),
            Self::Medieval => medieval(text, roll),
            Self::PigLatin => piglatin(text, roll),
            // `COMPONENT_UPPERCASE_SPEECH`.
            Self::Hulk => speechmod(text, HULK, &["!!"], 100.0, roll).to_uppercase(),
        }
    }
}

/// Every accent of the body in turn (`COMSIG_MOB_SAY` handlers); emotes are left alone.
pub fn apply_all(accents: &[Accent], text: &str, mut roll: impl FnMut() -> f32) -> String {
    if text.starts_with('*') {
        return text.to_owned();
    }
    accents
        .iter()
        .fold(text.to_owned(), |text, a| a.apply(&text, &mut roll))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_rewrite_speech() {
        let mut low = || 0.0;
        assert_eq!(
            Accent::Swedish.apply("bow wow", &mut low),
            "bjöv vöv".to_owned()
        );
        let mut high = || 99.0;
        assert_eq!(Accent::Chav.apply("my friend", &mut high), "me lad");
        assert_eq!(
            Accent::Hulk.apply("hulk smash.", &mut high),
            "HULK SMASH!!!"
        );
        assert_eq!(piglatin("hello there!", &mut low), "Ellohay erethay!");
        let medieval = Accent::Medieval.apply("are you there", &mut low);
        assert_eq!(medieval, "Ah, art thou thither");
        // Whole words only.
        assert_eq!(replace_words(" yourself ", "you", "thou"), " yourself ");
        assert_eq!(apply_all(&[Accent::Hulk], "*scream", || 0.0), "*scream");
    }
}
