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
    /// `heckacious/handle_speech`; also `TREAT_CAPITALIZE_MESSAGE = FALSE` ([`no_capitalize`]).
    Heckacious,
}

/// `heckacious/handle_caps`: the message keeps its case.
pub fn no_capitalize(accents: &[Accent]) -> bool {
    accents.contains(&Accent::Heckacious)
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

/// `strings/heckacious.json`.
const HECKACIOUS: Table = &[
    ("your", &["youre"]),
    ("fucking", &["banging"]),
    ("economy", &["econony"]),
    ("know", &["no"]),
    ("ing", &["in"]),
    ("they are", &["there"]),
    ("sometimes", &["some times"]),
    ("do", &["does"]),
    ("hug", &["bro hug bump"]),
    ("we're", &["where"]),
    ("game", &["big game"]),
    ("has", &["hass"]),
    ("downtown", &["down town"]),
    ("jimmy", &["geromy"]),
    ("jeremy", &["geromy"]),
    ("anywhere", &["anywear"]),
    ("smoking", &["toking up"]),
    ("lying", &["lyong"]),
    ("AH", &["AUGH"]),
    ("distraction", &["distaction"]),
    ("lie", &["ruse"]),
    ("angle", &["angel"]),
    ("drink", &["pour"]),
    ("the", &["thef"]),
    ("backward", &["back ward", "awayways"]),
    ("god damn", &["GOD DAMN"]),
    ("goddamn", &["GOD DAMN"]),
    ("trick", &["dunk"]),
    ("impossible", &["unreal"]),
    ("court", &["coart"]),
    ("holding", &["holdung"]),
    ("reverse", &["flip"]),
    ("inverse", &["flip"]),
    ("around", &["turn-ways"]),
    ("difference", &["differance"]),
    ("values", &["vaules"]),
    ("inside", &["insine"]),
    ("pipe", &["punpe"]),
    ("suspicious", &["plot thicken"]),
    ("awesome", &["hella"]),
    ("reference", &["refrance"]),
    ("amazing", &["fucking incredible"]),
    ("woman", &["wonan"]),
    ("god", &["gog"]),
    ("bible", &["bibble"]),
    ("jesus", &["jescus"]),
    ("christ", &["chris", "dick"]),
    ("sigh", &["sign"]),
    ("bathroom", &["banthroom"]),
    ("remember", &["remender"]),
    ("night", &["nite"]),
    ("nachos", &["nachoes"]),
    ("nacho", &["nancho"]),
    ("dorito", &["nancho"]),
    ("party", &["panty"]),
    ("dumbass", &["dumpass"]),
    ("they", &["their"]),
    ("okay", &["ohh kayy"]),
    ("stupid", &["stutid"]),
    ("reagent", &["regent"]),
    ("idiot", &["fuckass"]),
    ("awful", &["conksuck"]),
    ("moron", &["PIECE OF SHIT"]),
    ("more", &["wider"]),
    ("serious", &["keeping it real"]),
    ("extreme", &["x-treme"]),
    ("guaranteed", &["garganted"]),
    ("black", &["blapck"]),
    ("glow", &["glowns"]),
    ("deadly", &["deudly"]),
    ("flying", &["lifdoff"]),
    ("cloud", &["cloun"]),
    ("seeing", &["scoping"]),
    ("great", &["choice"]),
    ("look", &["lonk"]),
    ("battery", &["babbery"]),
    ("gay", &["homo"]),
    ("close", &["closte"]),
    ("wrong", &["wrog"]),
    ("who", &["whoof"]),
    ("new", &["newd"]),
    ("holy", &["hopy"]),
    ("um", &["unm"]),
    ("horse", &["hornse"]),
    ("this", &["thits"]),
    ("then", &["than"]),
    ("quick", &["soon"]),
    ("bro", &["brah", "bro"]),
    ("dude", &["duge", "bro", "brah"]),
    ("'", &["'", ""]),
    ("i'm", &["im", "i am"]),
    ("shit", &["balls warmed oveur", "shit"]),
    ("he's", &["hes", "he is", "he'ss"]),
];

const VOWELS: [&str; 5] = ["a", "e", "i", "o", "u"];
const CONSONANTS: [&str; 21] = [
    "b", "c", "d", "f", "g", "h", "j", "k", "l", "m", "n", "p", "q", "r", "s", "t", "v", "w", "x",
    "y", "z",
];

/// BYOND `rand(low, high)` over `roll`.
fn rand(low: u32, high: u32, roll: &mut impl FnMut() -> f32) -> u32 {
    let span = (high - low + 1) as f32;
    low + ((roll() / 100.0 * span) as u32).min(high - low)
}

/// `heckacious/handle_speech`: per word the table's swaps (as written, upper case and
/// capitalized), random caps, trailing dots and emphasis marks; a word the table left alone
/// gets a vowel and consonants swapped and some letters doubled.
// shortcut: BYOND `replacetext` of the vowel swap ignores case; here it matches as written.
fn heckacious(text: &str, roll: &mut impl FnMut() -> f32) -> String {
    let mut words = Vec::new();
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        let original = word.to_owned();
        let mut word = original.clone();
        let mut edited = false;
        for (key, values) in HECKACIOUS {
            let value = pick(values, roll);
            word = word.replace(&key.to_uppercase(), &value.to_uppercase());
            word = word.replace(
                &crate::speech::capitalize(key),
                &crate::speech::capitalize(value),
            );
            word = word.replace(key, value);
            if word != original {
                edited = true;
            }
        }
        if roll() < 10.0 {
            word = if roll() < 85.0 {
                word.to_uppercase()
            } else {
                word.to_lowercase()
            };
        }
        if roll() < 10.0 {
            for _ in 0..rand(2, 8, roll) {
                word.push('.');
            }
        }
        if roll() < 10.0 {
            let mark = pick(&["+", "_", "|"], roll);
            word = format!("{mark}{word}{mark}");
        }
        if !edited {
            if roll() < 65.0 {
                word = word.replace(pick(&VOWELS, roll), pick(&VOWELS, roll));
            }
            for _ in 0..rand(1, 2, roll) {
                word = word.replace(pick(&CONSONANTS, roll), pick(&CONSONANTS, roll));
            }
            let mut patched = String::with_capacity(word.len());
            for c in word.chars() {
                patched.push(c);
                if roll() >= 92.0 {
                    patched.push(c);
                }
            }
            word = patched;
        }
        words.push(word);
    }
    words.join(" ").trim().to_owned()
}

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
            Self::Heckacious => heckacious(text, roll),
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
        // Table swaps, no random marks at a high roll.
        assert_eq!(
            Accent::Heckacious.apply("the party", &mut || 99.0),
            "thef panty"
        );
        assert!(no_capitalize(&[Accent::Heckacious]));
    }
}
