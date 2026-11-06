//! Emotes, after tg `/mob/proc/emote` and `/datum/emote` (`emotes.dm`, `living/emote.dm`,
//! `carbon/emote.dm`, `human/emote.dm`) with BandaStation's messages (`ru_emotes.toml`) and
//! cooldown (3 s per emote).
//!
//! `*key` in say runs the emote `key`; `*key text` uses the emote's `message_param` with `%t` =
//! text, or replaces the message with the text when the emote has none (as tg does); `*me text`
//! is the custom emote (tg `me` verb).

use crate::speech::SpeakerStat;

/// tg `EMOTE_CANUSE_*` and `emote_type` bits this table needs.
pub const SOFTCRIT: u8 = 1;
pub const HARDCRIT: u8 = 2;
pub const UNCONSCIOUS: u8 = 4;
pub const REQUIRE_HANDS: u8 = 8;

/// tg `/datum/emote/proc/cooldown` with BandaStation's edit, seconds.
pub const EMOTE_COOLDOWN_SECS: f64 = 3.0;

/// One emote datum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emote {
    pub key: &'static str,
    pub message: &'static str,
    /// `message_param`, `%t` is the parameter.
    pub message_param: Option<&'static str>,
    pub flags: u8,
}

const fn emote(key: &'static str, message: &'static str, flags: u8) -> Emote {
    Emote {
        key,
        message,
        message_param: None,
        flags,
    }
}

const fn with_param(
    key: &'static str,
    message: &'static str,
    param: &'static str,
    flags: u8,
) -> Emote {
    Emote {
        key,
        message,
        message_param: Some(param),
        flags,
    }
}

/// The common human emotes.
// shortcut: messages only (no sounds, overlays, mime/robot variants, `taunt`/`collapse` effects);
// add the rest of the list with sounds and species.
pub const EMOTES: &[Emote] = &[
    with_param("bow", "кланяется.", "кланяется %t.", REQUIRE_HANDS),
    emote("blush", "краснеет.", 0),
    emote("choke", "давится!", 0),
    emote("chuckle", "усмехается.", 0),
    emote("clap", "хлопает.", REQUIRE_HANDS),
    emote("cough", "кашляет!", 0),
    emote("cross", "скрещивает свои руки.", REQUIRE_HANDS),
    emote("cry", "плачет.", SOFTCRIT),
    emote("dance", "радостно танцует.", REQUIRE_HANDS),
    emote(
        "deathgasp",
        "цепенеет и расслабляется, взгляд становится пустым и безжизненным...",
        SOFTCRIT | HARDCRIT | UNCONSCIOUS,
    ),
    emote("eyebrow", "приподнимает бровь.", 0),
    emote("frown", "хмурится.", 0),
    emote("gasp", "задыхается!", SOFTCRIT | HARDCRIT | UNCONSCIOUS),
    emote("giggle", "хихикает.", 0),
    emote("grin", "ухмыляется.", 0),
    emote("groan", "болезненно стонет!", 0),
    emote("laugh", "смеется.", 0),
    with_param("nod", "кивает.", "кивает %t.", 0),
    with_param("salute", "салютует.", "салютует %t.", REQUIRE_HANDS),
    emote("scream", "кричит!", 0),
    emote("shake", "качает своей головой.", 0),
    emote("shiver", "дрожит.", 0),
    emote("shrug", "пожимает плечами.", 0),
    emote("sigh", "вздыхает.", 0),
    emote("smile", "улыбается.", 0),
    emote("sneeze", "чихает.", 0),
    // `/datum/emote/living/surrender`: the game paralyzes the mob for 20 s
    // (`ss13_game::buffs`, on [`crate::server::Emoted`]).
    emote(
        "surrender",
        "ложит руки за голову и падает на землю, они сдаются!",
        0,
    ),
    emote("thumbsdown", "опускает большой палец вниз.", REQUIRE_HANDS),
    emote("thumbsup", "поднимает большой палец.", REQUIRE_HANDS),
    emote("wave", "машет рукой.", 0),
    emote("whistle", "свистит.", 0),
    emote("wink", "подмигивает.", 0),
    emote("yawn", "зевает.", 0),
];

pub fn find(key: &str) -> Option<&'static Emote> {
    EMOTES.iter().find(|e| e.key == key)
}

/// What `*act params` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmoteOutcome {
    /// Shown to everyone around as "<name> <text>".
    Run { key: &'static str, text: String },
    /// Only the user reads it (tg `to_chat` warnings).
    Refused(String),
    /// tg: a silent refusal (cooldown, empty custom emote).
    Silent,
    /// `*help`: the list of keys.
    Help(String),
}

/// tg `/mob/proc/emote` -> `can_run_emote` -> `run_emote` for an intentional emote; `act` is
/// what followed `*`. `on_cooldown(key)` says whether that emote is still cooling down.
pub fn run(
    act: &str,
    stat: SpeakerStat,
    mut on_cooldown: impl FnMut(&'static str) -> bool,
) -> EmoteOutcome {
    let (act, params) = match act.split_once(' ') {
        Some((act, params)) => (act, params.trim()),
        None => (act, ""),
    };
    let act = act.to_lowercase();
    if act == "help" {
        let keys: Vec<&str> = EMOTES.iter().map(|e| e.key).chain(["me"]).collect();
        return EmoteOutcome::Help(format!(
            "Доступные эмоции, которые можно проиграть, сказав \"*emote\": {}",
            keys.join(", ")
        ));
    }
    let (key, message, message_param, flags) = if act == "me" {
        // `/datum/emote/living/custom`: the text is the message.
        ("me", "", None, 0)
    } else {
        match find(&act) {
            Some(e) => (e.key, e.message, e.message_param, e.flags),
            None => {
                return EmoteOutcome::Refused(format!(
                    "Эмоция '{act}' не существует. Скажите '*help', чтобы получить список \
                     доступных эмоций."
                ));
            }
        }
    };
    if on_cooldown(key) {
        return EmoteOutcome::Silent;
    }
    // `can_run_emote`; tg's warnings exist only in English.
    let refuse = |why: &str| EmoteOutcome::Refused(format!("You cannot {key} {why}!"));
    if stat >= SpeakerStat::HardCrit && flags & UNCONSCIOUS == 0 {
        return refuse("while unconscious");
    }
    // Hands are blocked from soft crit on (`TRAIT_HANDS_BLOCKED`).
    if stat >= SpeakerStat::SoftCrit && flags & REQUIRE_HANDS != 0 {
        return EmoteOutcome::Refused(format!("You cannot use your hands to {key} right now!"));
    }
    match stat {
        SpeakerStat::SoftCrit if flags & SOFTCRIT == 0 => {
            return refuse("while in a critical condition");
        }
        SpeakerStat::HardCrit if flags & HARDCRIT == 0 => {
            return refuse("while in a critical condition");
        }
        SpeakerStat::Dead => return refuse("while dead"),
        _ => {}
    }
    let text = match (params.is_empty(), message_param) {
        (true, _) => message.to_owned(),
        (false, Some(param)) => param.replace("%t", params),
        (false, None) => params.to_owned(),
    };
    if text.is_empty() {
        return EmoteOutcome::Silent;
    }
    EmoteOutcome::Run { key, text }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: SpeakerStat = SpeakerStat::Conscious;

    fn ran(outcome: EmoteOutcome) -> String {
        match outcome {
            EmoteOutcome::Run { text, .. } => text,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn emotes_follow_tg() {
        assert_eq!(ran(run("scream", C, |_| false)), "кричит!");
        assert_eq!(ran(run("SCREAM", C, |_| false)), "кричит!");
        assert_eq!(ran(run("nod Бобу", C, |_| false)), "кивает Бобу.");
        // Without `message_param` the parameter is the message.
        assert_eq!(ran(run("laugh над шуткой", C, |_| false)), "над шуткой");
        assert_eq!(ran(run("me чешет затылок", C, |_| false)), "чешет затылок");
        assert_eq!(run("me", C, |_| false), EmoteOutcome::Silent);
        assert!(
            matches!(run("flarp", C, |_| false), EmoteOutcome::Refused(m) if m.contains("'flarp' не существует"))
        );
        assert_eq!(run("wave", C, |_| true), EmoteOutcome::Silent);
        assert!(matches!(run("help", C, |_| false), EmoteOutcome::Help(m) if m.contains("scream")));
    }

    #[test]
    fn crit_allows_only_crit_emotes() {
        let soft = SpeakerStat::SoftCrit;
        assert!(matches!(
            run("scream", soft, |_| false),
            EmoteOutcome::Refused(_)
        ));
        assert!(matches!(
            run("me ползёт", soft, |_| false),
            EmoteOutcome::Refused(_)
        ));
        assert!(matches!(
            run("wave", soft, |_| false),
            EmoteOutcome::Refused(_)
        ));
        assert_eq!(ran(run("cry", soft, |_| false)), "плачет.");
        assert_eq!(ran(run("gasp", soft, |_| false)), "задыхается!");
        let hard = SpeakerStat::HardCrit;
        assert!(matches!(
            run("cry", hard, |_| false),
            EmoteOutcome::Refused(_)
        ));
        assert_eq!(ran(run("gasp", hard, |_| false)), "задыхается!");
        assert!(matches!(
            run("gasp", SpeakerStat::Dead, |_| false),
            EmoteOutcome::Refused(_)
        ));
    }
}
