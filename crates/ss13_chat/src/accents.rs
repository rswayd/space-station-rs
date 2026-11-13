//! Speech-changing genetic mutations (tg `code/datums/mutations/speech.dm`, the
//! `/datum/component/speechmod` of the Swedish, Chav, Elvis and Hulk mutations, the Medieval and
//! Pig Latin `COMSIG_MOB_SAY` handlers) and worn `speechmod`s (the French beret, the gondola and
//! luchador masks, the Italian moustache, the crab hats), applied by `crate::server` where tg
//! sends `COMSIG_MOB_SAY`. The game keeps [`Accents`] on the body (`ss13_game::genetics`,
//! `ss13_game::speechmods`).
// shortcut: `medieval_replacement.json` is ported in part (the common words, every starting).

use bevy::prelude::*;

mod tables;

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
    /// `/obj/item/clothing/head/beret/frenchberet`'s speechmod.
    French,
    /// `/obj/item/clothing/mask/fakemoustache/italian`'s speechmod (BandaStation's Russian table).
    Italian,
    /// `/obj/item/clothing/mask/gondola`'s speechmod.
    Spurdo,
    /// `/obj/item/clothing/mask/luchador`'s speechmod: upper case, sometimes ` OLE!`.
    Luchador,
    /// The crab hats' speechmod (`crustacean_replacement.json`).
    Crustacean,
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
                let (from, to) = (pick(&VOWELS, roll), pick(&VOWELS, roll));
                word = word.replace(from, to);
            }
            for _ in 0..rand(1, 2, roll) {
                let (from, to) = (pick(&CONSONANTS, roll), pick(&CONSONANTS, roll));
                word = word.replace(from, to);
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
            Self::French => speechmod(
                text,
                tables::FRENCH,
                &[" Honh honh honh!", " Honh!", " Zut Alors!"],
                3.0,
                roll,
            ),
            Self::Italian => speechmod(
                text,
                tables::ITALIAN_RU,
                &[
                    " Равиоли, равиоли, подскажи мне формуоли!",
                    " Мамма-мия!",
                    " Мамма-мия! Какая острая фрикаделька!",
                    " Ла ла ла ла ла фуникули+ фуникуля+!",
                ],
                3.0,
                roll,
            ),
            Self::Spurdo => speechmod(text, tables::SPURDO, &[], 0.0, roll),
            // `COMPONENT_UPPERCASE_SPEECH`.
            Self::Luchador => {
                speechmod(text, tables::LUCHADOR, &[" OLE!"], 25.0, roll).to_uppercase()
            }
            Self::Crustacean => speechmod(text, tables::CRUSTACEAN, &[], 0.0, roll),
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

/// The speech of a tongue that changes it (`modifies_speech`: its `speechmod` or
/// `modify_speech`), kept on the body by the game (`ss13_game::medical::organs`). It is left out
/// when the body speaks a language native to the tongue ([`TongueSpeech::native`],
/// `should_modify_speech`).
// shortcut: no bone or robot voice spans (`SPAN_SANS`, `SPAN_PAPYRUS`, `SPAN_ROBOT`: the chat has
// no such fonts); add them with a span list on speech.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TongueSpeech {
    /// `/obj/item/organ/tongue/lizard` (BandaStation: also Cyrillic `с` and `ч`).
    Lizard,
    /// `/obj/item/organ/tongue/fly` (BandaStation: also `з` and `с`).
    Fly,
    /// BandaStation `/obj/item/organ/tongue/tajaran`: rolled `r`.
    Tajaran,
    /// BandaStation `/obj/item/organ/tongue/vulpkanin`: rolled `r`.
    Vulpkanin,
    /// `/obj/item/organ/tongue/snail`.
    Snail,
    /// `/obj/item/organ/tongue/rat` with BandaStation's `autohiss` greeting.
    Rat,
    /// `/obj/item/organ/tongue/zombie`: known words become zombie words, the rest growls.
    Zombie,
    /// `/obj/item/organ/tongue/alien`: the words stay, the game plays a hiss
    /// (`ss13_game::speechmods`).
    Alien,
}

/// BandaStation's tajaran and vulpkanin `speech_replacements`.
const ROLLED: Table = &[
    ("r", &["rr", "rrr", "rrrr"]),
    ("R", &["RR", "RRR", "RRRR"]),
    ("р", &["рр", "ррр", "рррр"]),
    ("Р", &["РР", "РРР", "РРРР"]),
];

/// BYOND regex `\w`.
fn ascii_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `regex("<from>+", "g")` → `to`: every run of `from` becomes `to`.
fn runs(text: &str, from: char, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if c == from {
            if !in_run {
                out.push_str(to);
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// `regex(@"(\w)<x>", "g")` → `$1<to>`.
fn after_word(text: &str, x: char, to: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if ascii_word(chars[i]) && chars.get(i + 1) == Some(&x) {
            out.push(chars[i]);
            out.push_str(to);
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// `regex(@"\b<x>([\-|r|R]|\b)", "g")` → `<to>$1`.
fn word_start(text: &str, x: char, to: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let starts = chars[i] == x && (i == 0 || !ascii_word(chars[i - 1]));
        if starts {
            match chars.get(i + 1) {
                Some(&c) if matches!(c, '-' | '|' | 'r' | 'R') => {
                    out.push_str(to);
                    out.push(c);
                    i += 2;
                    continue;
                }
                Some(&c) if ascii_word(c) => {}
                _ => {
                    out.push_str(to);
                    i += 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

impl TongueSpeech {
    /// The speech of a tongue organ type, `None` for a tongue that leaves it alone.
    pub fn of_path(path: &str) -> Option<Self> {
        const TONGUES: [(&str, TongueSpeech); 8] = [
            ("/obj/item/organ/tongue/lizard", TongueSpeech::Lizard),
            ("/obj/item/organ/tongue/fly", TongueSpeech::Fly),
            ("/obj/item/organ/tongue/tajaran", TongueSpeech::Tajaran),
            ("/obj/item/organ/tongue/vulpkanin", TongueSpeech::Vulpkanin),
            ("/obj/item/organ/tongue/snail", TongueSpeech::Snail),
            ("/obj/item/organ/tongue/rat", TongueSpeech::Rat),
            ("/obj/item/organ/tongue/zombie", TongueSpeech::Zombie),
            ("/obj/item/organ/tongue/alien", TongueSpeech::Alien),
        ];
        TONGUES
            .into_iter()
            .find(|(root, _)| {
                path.strip_prefix(root)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            })
            .map(|(_, speech)| speech)
    }

    /// The tongue's `languages_native`.
    pub fn native(self) -> &'static [crate::language::LanguageId] {
        use crate::language::LanguageId::*;
        match self {
            Self::Lizard => &[Draconic],
            Self::Fly => &[Buzzwords],
            Self::Tajaran => &[Siiktajr],
            Self::Vulpkanin => &[Canilunzt],
            Self::Snail | Self::Rat | Self::Zombie | Self::Alien => &[],
        }
    }

    /// What the tongue makes of `text`; `roll` gives values in 0..100.
    pub fn apply(self, text: &str, roll: &mut impl FnMut() -> f32) -> String {
        if text.starts_with('*') {
            return text.to_owned();
        }
        match self {
            Self::Lizard => {
                let t = runs(text, 's', "sss");
                let t = runs(&t, 'S', "SSS");
                let t = after_word(&t, 'x', "kss");
                let t = after_word(&t, 'X', "KSSS");
                let t = word_start(&t, 'x', "ecks");
                let t = word_start(&t, 'X', "ECKS");
                let t = runs(&t, 'с', "ссс");
                let t = runs(&t, 'С', "ССС");
                let t = runs(&t, 'ч', "щ");
                runs(&t, 'Ч', "Щ").trim().to_owned()
            }
            Self::Fly => {
                let t = runs(text, 'z', "zzz");
                let t = runs(&t, 'Z', "ZZZ").replace('s', "z").replace('S', "Z");
                let t = runs(&t, 'з', "ззз");
                let t = runs(&t, 'З', "ЗЗЗ").replace('с', "з").replace('С', "З");
                t.trim().to_owned()
            }
            Self::Tajaran | Self::Vulpkanin => speechmod(text, ROLLED, &[], 0.0, roll),
            // tg's letter list misses `Q`: every Latin letter is drawn out.
            Self::Snail => text
                .chars()
                .flat_map(|c| {
                    let n = if c.is_ascii_alphabetic() { 3 } else { 1 };
                    std::iter::repeat_n(c, n)
                })
                .collect(),
            Self::Rat => {
                // shortcut: tg's `whimsy_check` is only its 1% roll (no April Fools, no
                // whimsical personality); add them when the chat knows holidays and personalities.
                let whimsy = roll() < 1.0;
                match text.to_lowercase().as_str() {
                    "привет" | "привет." => "Сыррретствую вас!".to_owned(),
                    "привет?" => "Мм... сыррретствую вас?".to_owned(),
                    "hi" | "hi." if whimsy => "Cheesed to meet you!".to_owned(),
                    "hi?" if whimsy => "Um... cheesed to meet you?".to_owned(),
                    _ => text.to_owned(),
                }
            }
            Self::Zombie => zombie(text),
            Self::Alien => text.to_owned(),
        }
    }
}

/// tg `plural_s`.
fn plural_s(word: &str) -> &'static str {
    if word.ends_with("ss") || word.ends_with("sh") || word.ends_with("ch") {
        return "es";
    }
    if word.ends_with(['s', 'x', 'z']) {
        "es"
    } else {
        "s"
    }
}

/// `english_to_zombie[word]`: `add_word_to_translations` files every word with `plural_s`,
/// `ing` and `ed` too, a later zombie word overwriting an earlier one.
fn zombie_word(word: &str) -> Option<&'static str> {
    tables::ZOMBIE.iter().rev().find_map(|&(zombie, english)| {
        english
            .iter()
            .any(|&e| {
                word.strip_prefix(e).is_some_and(|rest| {
                    rest.is_empty() || rest == plural_s(e) || rest == "ing" || rest == "ed"
                })
            })
            .then_some(zombie)
    })
}

/// `/obj/item/organ/tongue/zombie/modify_speech`: a word the table knows becomes its zombie
/// word; any other loses `eiou` to `r` and everything but `zhrgbmna.!?-` (so, as in
/// BandaStation, an unknown Cyrillic word is gone), and words left empty go with their spaces.
/// tg growls the whole message first and pairs the words back by index, which shifts the
/// translations once a word vanishes; here each word is done in place.
fn zombie(text: &str) -> String {
    let words: Vec<String> = text
        .split(' ')
        .filter_map(|word| {
            if let Some(zombie) = zombie_word(&word.to_lowercase()) {
                return Some(zombie.to_owned());
            }
            let growl: String = word
                .chars()
                .map(|c| {
                    if matches!(c.to_ascii_lowercase(), 'e' | 'i' | 'o' | 'u') {
                        'r'
                    } else {
                        c
                    }
                })
                .filter(|&c| {
                    matches!(
                        c.to_ascii_lowercase(),
                        'z' | 'h' | 'r' | 'g' | 'b' | 'm' | 'n' | 'a' | '.' | '!' | '?' | '-'
                    )
                })
                .collect();
            (!growl.is_empty()).then(|| zombie_growl(&growl))
        })
        .collect();
    ss13_shared::capitalize(&words.join(" "))
}

/// A word's `\lr\b` -> `rh`, then `\b[Aa]\b` -> `hra`.
fn zombie_growl(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let boundary_after = |i: usize| chars.get(i).is_none_or(|&c| !ascii_word(c));
    let mut rh = Vec::with_capacity(chars.len() + 1);
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_lowercase() && chars.get(i + 1) == Some(&'r') && boundary_after(i + 2)
        {
            rh.extend(['r', 'h']);
            i += 2;
        } else {
            rh.push(chars[i]);
            i += 1;
        }
    }
    let mut out = String::with_capacity(rh.len() + 2);
    for (i, &c) in rh.iter().enumerate() {
        let lone = matches!(c, 'a' | 'A')
            && (i == 0 || !ascii_word(rh[i - 1]))
            && rh.get(i + 1).is_none_or(|&n| !ascii_word(n));
        if lone {
            out.push_str("hra");
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tongues_change_speech() {
        let mut low = || 0.0;
        let lizard = TongueSpeech::Lizard;
        assert_eq!(lizard.apply("snakes", &mut low), "sssnakesss");
        assert_eq!(lizard.apply("SOS box", &mut low), "SSSOSSS bokss");
        assert_eq!(lizard.apply("x-ray xr x", &mut low), "ecks-ray ecksr ecks");
        assert_eq!(lizard.apply("xeno", &mut low), "xeno");
        assert_eq!(lizard.apply("Сосиска, чай", &mut low), "СССосссиссска, щай");
        assert_eq!(lizard.apply("*hiss", &mut low), "*hiss");
        assert_eq!(
            TongueSpeech::Fly.apply("buzz sss зса", &mut low),
            "buzzz zzz зззза"
        );
        // One pick per key for the whole message.
        assert_eq!(
            TongueSpeech::Tajaran.apply("Rarr, ррр", &mut low),
            "RRarrrr, рррррр"
        );
        assert_eq!(TongueSpeech::Snail.apply("qa б!", &mut low), "qqqaaa б!");
        let rat = TongueSpeech::Rat;
        assert_eq!(rat.apply("Привет.", &mut || 50.0), "Сыррретствую вас!");
        assert_eq!(
            rat.apply("привет?", &mut || 50.0),
            "Мм... сыррретствую вас?"
        );
        assert_eq!(rat.apply("hi", &mut || 50.0), "hi");
        assert_eq!(rat.apply("hi", &mut low), "Cheesed to meet you!");
        assert_eq!(
            TongueSpeech::of_path("/obj/item/organ/tongue/lizard/silver"),
            Some(TongueSpeech::Lizard)
        );
        assert_eq!(
            TongueSpeech::of_path("/obj/item/organ/tongue/lizardx"),
            None
        );
        assert_eq!(TongueSpeech::of_path("/obj/item/organ/tongue"), None);
    }

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

    #[test]
    fn worn_speechmods_rewrite_speech() {
        assert_eq!(
            Accent::Luchador.apply("hello friend", &mut || 0.0),
            "HOLA AMIGO OLE!"
        );
        assert_eq!(
            Accent::Italian.apply("привет друг", &mut || 99.0),
            "бонжорно амиго"
        );
        assert_eq!(Accent::Spurdo.apply("epic tick", &mut || 0.0), "ebin digg");
        assert_eq!(
            Accent::Crustacean.apply("no problem", &mut || 0.0),
            "no prawnblem"
        );
    }

    #[test]
    fn zombies_translate_or_growl() {
        let mut low = || 0.0;
        let zombie = TongueSpeech::Zombie;
        assert_eq!(zombie.apply("Helping hello", &mut low), "Hab hrh");
        // Russian words of BandaStation's table; an unknown Cyrillic word is gone.
        assert_eq!(
            zombie.apply("Помогите табуретка мозги bor a", &mut low),
            "Паамахиите маазгии brh hra"
        );
        assert_eq!(zombie.apply("табуретка", &mut low), "");
        assert_eq!(zombie.apply("*scream", &mut low), "*scream");
        assert_eq!(
            TongueSpeech::of_path("/obj/item/organ/tongue/alien"),
            Some(TongueSpeech::Alien)
        );
        assert_eq!(TongueSpeech::Alien.apply("Хсс", &mut low), "Хсс");
    }
}
