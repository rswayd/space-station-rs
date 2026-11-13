//! Emotes, after tg `/mob/proc/emote` and `/datum/emote` (`emotes.dm`, `living/emote.dm`,
//! `carbon/emote.dm`, `human/emote.dm`, `silicon/ai/emote.dm`, `silicon/robot/emote.dm`,
//! `brain/emote.dm`, `carbon/alien/emote.dm`, `basic/slime/emote.dm`, `gorilla_emotes.dm`) with
//! BandaStation's messages (`ru_emotes.toml`), cooldown (3 s per emote) and species emotes
//! (`modular_bandastation/emote_panel`: the species' own emotes and their tongues' sounds).
//!
//! `*key` in say runs the emote `key`; `*key text` uses the emote's `message_param` with `%t` =
//! text, or replaces the message with the text when the emote has none (as tg does); `*me text`
//! is the custom emote (tg `me` verb). Several emotes may share a key (`sad`, `whistle`): the
//! first one the mob may run wins, as tg's `GLOB.emote_list[act]` loop.

use bevy::prelude::Component;

use crate::speech::SpeakerStat;

/// tg `EMOTE_CANUSE_*` and `emote_type` bits this table needs.
pub const SOFTCRIT: u8 = 1;
pub const HARDCRIT: u8 = 2;
pub const UNCONSCIOUS: u8 = 4;
pub const REQUIRE_HANDS: u8 = 8;
/// `EMOTE_AUDIBLE`: its sound is not made by a mute (`should_play_sound`).
pub const AUDIBLE: u8 = 16;

/// tg `/datum/emote/proc/cooldown` with BandaStation's edit, seconds.
pub const EMOTE_COOLDOWN_SECS: f64 = 3.0;
/// BandaStation's `manual_general_emote_audio_cooldown`: after an emote with a sound, no other
/// emote with a sound runs for 3 s (`run_emote` returns before the message).
pub const EMOTE_AUDIO_COOLDOWN_SECS: f64 = 3.0;
/// `sound_volume`.
pub const EMOTE_VOLUME: f32 = 50.0;

/// What kind of mob runs an emote: the tg types `mob_type_allowed_typecache` and
/// `select_message_type` look at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EmoteMob {
    /// `/mob/living/carbon/human` (monkeys included, as in tg).
    #[default]
    Human,
    /// `isalienadult`.
    AlienAdult,
    /// `islarva`.
    AlienLarva,
    /// `iscyborg`.
    Cyborg,
    /// `isAI` (the client talks through the eye).
    Ai,
    /// `/mob/living/brain`.
    Brain,
    /// `/mob/living/basic/slime`.
    Slime,
    /// Any other `isanimal_or_basicmob`.
    Basic,
    /// `/mob/living/basic/gorilla`.
    Gorilla,
}

impl EmoteMob {
    pub const fn bit(self) -> u16 {
        1 << self as u16
    }
}

pub const HUMAN: u16 = EmoteMob::Human.bit();
pub const ALIEN: u16 = EmoteMob::AlienAdult.bit() | EmoteMob::AlienLarva.bit();
/// `/mob/living/carbon`.
pub const CARBON: u16 = HUMAN | ALIEN;
/// `/mob`.
pub const ANY: u16 = (1 << 9) - 1;
/// `/datum/emote/living`: `/mob/living` without `/mob/living/brain`.
pub const LIVING: u16 = ANY & !EmoteMob::Brain.bit();
/// `isanimal_or_basicmob`.
const ANIMAL: u16 = EmoteMob::Slime.bit() | EmoteMob::Basic.bit() | EmoteMob::Gorilla.bit();

/// The BandaStation species whose emotes or tongue sounds differ (`species_type_whitelist`,
/// `/obj/item/organ/tongue/<species>/emote_sounds`), subtypes included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EmoteSpecies {
    #[default]
    Other,
    Vulpkanin,
    Tajaran,
    Skrell,
    /// `/datum/species/lizard` and its subtypes (forked tongue).
    Lizard,
    Moth,
    Pod,
    /// `/datum/species/jelly` and its subtypes (slime people, luminescents, stargazers).
    Jelly,
}

/// On a body: who runs its emotes, kept by the game. Absent = a human of no listed species.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmoteUser {
    pub mob: EmoteMob,
    pub species: EmoteSpecies,
    /// `physique == FEMALE` (gendered emote sounds).
    pub female: bool,
    /// `TRAIT_SILICON_EMOTES_ALLOWED`: silicons, drones, bots, robotic tongues, processors.
    pub silicon_emotes: bool,
    /// `MOB_ROBOTIC` (`message_robot` of a bot or drone).
    pub robotic: bool,
    /// A brain mob inside a brain processor (`/datum/emote/brain/can_run_emote`).
    pub in_processor: bool,
    /// A tail with `WAG_ABLE` (`ORGAN_SLOT_EXTERNAL_TAIL`): whether it is wagging.
    pub tail: Option<bool>,
}

/// The sound an emote makes: one of `files` (files or SFX keys) is played.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmoteSound {
    pub files: &'static [&'static str],
    pub vary: bool,
    pub volume: f32,
    /// `sound_wall_ignore`.
    pub ignore_walls: bool,
    /// `EMOTE_AUDIBLE`: a mute or tongueless mob makes no sound.
    pub audible: bool,
}

/// One emote datum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emote {
    pub key: &'static str,
    pub message: &'static str,
    /// `message_param`, `%t` is the parameter.
    pub message_param: Option<&'static str>,
    pub flags: u8,
    /// `mob_type_allowed_typecache` without `mob_type_blacklist_typecache`, [`EmoteMob`] bits.
    pub mobs: u16,
    /// `trait_required = TRAIT_SILICON_EMOTES_ALLOWED`.
    pub silicon: bool,
    /// BandaStation `species_type_whitelist_typecache`.
    pub species: Option<EmoteSpecies>,
    pub message_robot: &'static str,
    pub message_ai: &'static str,
    pub message_alien: &'static str,
    pub message_larva: &'static str,
    pub message_animal: &'static str,
    /// `sound` / `get_sound`: one is picked; empty = silent.
    pub sounds: &'static [&'static str],
    pub vary: bool,
    pub volume: f32,
    pub ignore_walls: bool,
    pub cooldown: f64,
}

const BASE: Emote = Emote {
    key: "",
    message: "",
    message_param: None,
    flags: 0,
    mobs: LIVING,
    silicon: false,
    species: None,
    message_robot: "",
    message_ai: "",
    message_alien: "",
    message_larva: "",
    message_animal: "",
    sounds: &[],
    vary: false,
    volume: EMOTE_VOLUME,
    ignore_walls: false,
    cooldown: EMOTE_COOLDOWN_SECS,
};

const fn emote(key: &'static str, message: &'static str, flags: u8) -> Emote {
    Emote {
        key,
        message,
        flags,
        ..BASE
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
        ..BASE
    }
}

/// The emote limited to these mobs.
const fn only(mobs: u16, base: Emote) -> Emote {
    Emote { mobs, ..base }
}

/// `/datum/emote/silicon`: any mob with `TRAIT_SILICON_EMOTES_ALLOWED`, audible.
const fn silicon(
    key: &'static str,
    message: &'static str,
    param: Option<&'static str>,
    sounds: &'static [&'static str],
) -> Emote {
    Emote {
        key,
        message,
        message_param: param,
        flags: AUDIBLE,
        mobs: ANY,
        silicon: true,
        sounds,
        ..BASE
    }
}

/// `/datum/emote/ai/emotion_display`: no message, the AI's status displays change.
const fn ai_display(key: &'static str) -> Emote {
    Emote {
        key,
        mobs: EmoteMob::Ai.bit(),
        ..BASE
    }
}

/// `/datum/emote/slime/mood`: no message, the slime's face changes.
const fn slime_mood(key: &'static str) -> Emote {
    Emote {
        key,
        mobs: EmoteMob::Slime.bit(),
        ..BASE
    }
}

/// `/datum/emote/brain`: only in a brain processor.
const fn brain(key: &'static str, message: &'static str, flags: u8) -> Emote {
    Emote {
        key,
        message,
        flags,
        mobs: EmoteMob::Brain.bit(),
        ..BASE
    }
}

/// A BandaStation species emote (`/datum/emote/living/carbon/human/<species>`).
const fn species(
    species: EmoteSpecies,
    key: &'static str,
    message: &'static str,
    param: &'static str,
    sounds: &'static [&'static str],
    vary: bool,
    cooldown: f64,
) -> Emote {
    Emote {
        key,
        message,
        message_param: Some(param),
        flags: AUDIBLE,
        mobs: HUMAN,
        species: Some(species),
        sounds,
        vary,
        cooldown,
        ..BASE
    }
}

/// Files of BandaStation's `modular_bandastation/emote_panel/audio`.
macro_rules! bp {
    ($($file:literal),+ $(,)?) => {
        &[$(concat!("modular_bandastation/emote_panel/audio/", $file)),+]
    };
}

/// The emote table, in tg's `GLOB.emote_list` order where keys repeat.
// shortcut: the common emotes carry no sounds, overlays, mime variants or `taunt`/`collapse`
// effects ("Эмоции живых и людей"); species tongues give theirs ([`tongue_sounds`]).
pub const EMOTES: &[Emote] = &[
    with_param("bow", "кланяется.", "кланяется %t.", REQUIRE_HANDS),
    emote("blush", "краснеет.", 0),
    emote("choke", "давится!", 0),
    emote("chuckle", "усмехается.", 0),
    only(CARBON, emote("clap", "хлопает.", REQUIRE_HANDS)),
    emote("cough", "кашляет!", AUDIBLE),
    emote("cross", "скрещивает свои руки.", REQUIRE_HANDS),
    only(CARBON, emote("cry", "плачет.", SOFTCRIT | AUDIBLE)),
    emote("dance", "радостно танцует.", REQUIRE_HANDS),
    Emote {
        message_robot: "на мгновение вздрагивает и замирает, глаза медленно темнеют.",
        message_ai: "скрипит, экран мерцает, пока системы медленно выключаются.",
        message_alien: "издает ослабевающий гортанный крик и падает на пол...",
        message_larva: "с тошнотворным шипением выдыхает воздух и падает на пол...",
        message_animal: "перестает двигаться...",
        vary: true,
        volume: 200.0,
        ignore_walls: true,
        ..emote(
            "deathgasp",
            "цепенеет и расслабляется, взгляд становится пустым и безжизненным...",
            SOFTCRIT | HARDCRIT | UNCONSCIOUS | AUDIBLE,
        )
    },
    only(HUMAN, emote("eyebrow", "приподнимает бровь.", 0)),
    emote("frown", "хмурится.", 0),
    emote(
        "gasp",
        "задыхается!",
        SOFTCRIT | HARDCRIT | UNCONSCIOUS | AUDIBLE,
    ),
    emote("giggle", "хихикает.", AUDIBLE),
    emote("grin", "ухмыляется.", 0),
    emote("groan", "болезненно стонет!", AUDIBLE),
    emote("laugh", "смеется.", AUDIBLE),
    with_param("nod", "кивает.", "кивает %t.", 0),
    only(
        HUMAN,
        with_param("salute", "салютует.", "салютует %t.", REQUIRE_HANDS),
    ),
    emote("scream", "кричит!", AUDIBLE),
    emote("shake", "качает своей головой.", 0),
    emote("shiver", "дрожит.", 0),
    only(HUMAN, emote("shrug", "пожимает плечами.", 0)),
    // `/datum/emote/living/carbon/human/wag`: only with a wag-able tail; the game toggles the
    // wagging on [`crate::server::Emoted`] (`ss13_game::medical::organs`).
    only(HUMAN, emote("wag", "начинает вилять хвостом.", 0)),
    emote("sigh", "вздыхает.", AUDIBLE),
    emote("smile", "улыбается.", 0),
    emote("sneeze", "чихает.", AUDIBLE),
    // `/datum/emote/living/surrender`: the game paralyzes the mob for 20 s
    // (`ss13_game::buffs`, on [`crate::server::Emoted`]).
    emote(
        "surrender",
        "ложит руки за голову и падает на землю, они сдаются!",
        0,
    ),
    only(
        HUMAN,
        emote("thumbsdown", "опускает большой палец вниз.", REQUIRE_HANDS),
    ),
    only(
        HUMAN,
        emote("thumbsup", "поднимает большой палец.", REQUIRE_HANDS),
    ),
    emote("wave", "машет рукой.", 0),
    only(CARBON, emote("whistle", "свистит.", AUDIBLE)),
    // `/datum/emote/living/carbon/crack` (tg also wants two usable hands).
    Emote {
        sounds: &["sound/mobs/humanoids/human/knuckle_crack/knuckles.ogg"],
        cooldown: 6.0,
        ..only(CARBON, emote("crack", "хрустит пальцами.", REQUIRE_HANDS))
    },
    // Sounds by mob type ([`human_sounds`]); an adult alien hisses with `SFX_HISS`.
    Emote {
        sounds: &["hiss"],
        vary: true,
        ..only(CARBON, emote("hiss", "шипит!", AUDIBLE))
    },
    only(CARBON, emote("moan", "стонет!", AUDIBLE)),
    Emote {
        vary: true,
        ..emote("sniff", "шмыгает носом.", AUDIBLE)
    },
    emote("snore", "храпит.", UNCONSCIOUS | AUDIBLE),
    only(
        CARBON,
        with_param(
            "snap",
            "щелкает пальцами.",
            "щелкает пальцами в сторону %t.",
            REQUIRE_HANDS | AUDIBLE,
        ),
    ),
    // `/datum/emote/living/carbon/human/screech`: the scream's sound, 10 s between them
    // (`manual_specific_emote_audio_cooldown`).
    Emote {
        cooldown: 10.0,
        ..only(HUMAN, emote("screech", "визжит!", AUDIBLE))
    },
    // `/datum/emote/jump`: a hop (`jump_animation`, `ss13_game::speech`) with a thud; not
    // `EMOTE_AUDIBLE`, so a mute jumps loudly too.
    Emote {
        sounds: &["sound/items/weapons/thudswoosh.ogg"],
        ..emote("jump", "прыгает!", 0)
    },
    // `/datum/emote/living/moth/*`: `TRAIT_MOTH_EMOTES_ALLOWED` (a moth tongue).
    Emote {
        species: Some(EmoteSpecies::Moth),
        sounds: &["sound/mobs/humanoids/moth/moth_chitter.ogg"],
        ..only(HUMAN, emote("chitter", "стрекочет!", AUDIBLE))
    },
    Emote {
        species: Some(EmoteSpecies::Moth),
        sounds: &["sound/mobs/humanoids/moth/moth_squeak.ogg"],
        ..only(HUMAN, emote("squeak", "пищит!", AUDIBLE))
    },
    // `/datum/emote/living/carbon/circle`, `slap`, `shoesteal`, `noogie`, `hand` and
    // `/datum/emote/living/kiss`: no message, the game puts the hand gesture in a free hand
    // (`ss13_game::devices::hand_items`, on `Emoted`).
    only(CARBON, emote("circle", "", REQUIRE_HANDS)),
    only(CARBON, emote("slap", "", REQUIRE_HANDS)),
    only(CARBON, emote("shoesteal", "", REQUIRE_HANDS)),
    only(CARBON, emote("noogie", "", REQUIRE_HANDS)),
    only(CARBON, emote("hand", "", REQUIRE_HANDS)),
    emote("kiss", "", 0),
    only(CARBON, emote("wink", "подмигивает.", 0)),
    Emote {
        // BandaStation's "сочувственно зевнуть." is an infinitive slip; the verb form here.
        message_robot: "сочувственно зевает.",
        message_ai: "сочувственно зевает.",
        ..emote("yawn", "зевает.", AUDIBLE)
    },
    // `/datum/emote/ai/emotion_display/*` ([`ai_emotion`]).
    ai_display("blank"),
    ai_display("veryhappy"),
    ai_display("happy"),
    ai_display("neutral"),
    ai_display("unsure"),
    ai_display("confused"),
    ai_display("sad"),
    ai_display("bsod"),
    ai_display("dead"),
    ai_display("download"),
    ai_display("trollface"),
    ai_display("awesome"),
    ai_display("dorfy"),
    ai_display("thinking"),
    ai_display("facepalm"),
    ai_display("friendcomputer"),
    ai_display("blueglow"),
    ai_display("redglow"),
    // `/datum/emote/brain/*`.
    brain("alarm", "издаёт звук тревоги.", AUDIBLE),
    brain("alert", "издаёт тревожный звук.", AUDIBLE),
    brain("flash", "моргает лампочками.", 0),
    brain("notice", "проигрывает громкий звук.", AUDIBLE),
    brain("whistle", "свистит.", AUDIBLE),
    // `/datum/emote/living/alien/*`.
    only(
        ALIEN,
        emote("gnarl", "оскаливается и показывает зубы...", 0),
    ),
    Emote {
        message_alien: "рычит.",
        message_larva: "тихо рычит.",
        // `sounds_by_mobtype`: only adults hiss ([`sound_of`]).
        sounds: &["sound/mobs/non-humanoids/hiss/hiss5.ogg"],
        vary: true,
        ..only(ALIEN, emote("roar", "", AUDIBLE))
    },
    // `/datum/emote/gorilla/ooga` (BandaStation leaves it untranslated).
    Emote {
        sounds: &["sound/mobs/non-humanoids/gorilla/gorilla.ogg"],
        ..only(
            EmoteMob::Gorilla.bit(),
            with_param("ooga", "oogas.", "oogas at %t.", AUDIBLE),
        )
    },
    // `/datum/emote/silicon/*`.
    silicon("boop", "бупает.", None, &[]),
    silicon(
        "beep",
        "бипает.",
        Some("бипает в сторону %t."),
        &["sound/machines/beep/twobeep.ogg"],
    ),
    silicon(
        "buzz",
        "жужжит.",
        Some("жужжит на %t."),
        &["sound/machines/buzz/buzz-sigh.ogg"],
    ),
    silicon(
        "buzz2",
        "жужжит дважды.",
        None,
        &["sound/machines/buzz/buzz-two.ogg"],
    ),
    silicon("chime", "звенит.", None, &["sound/machines/chime.ogg"]),
    Emote {
        vary: true,
        ..silicon("honk", "гудит.", None, &["sound/items/bikehorn.ogg"])
    },
    silicon(
        "ping",
        "сигналит.",
        Some("сигналит в сторону %t."),
        &["sound/machines/ping.ogg"],
    ),
    silicon(
        "sad",
        "проигрывает звук грустного тромбона...",
        None,
        &["sound/misc/sadtrombone.ogg"],
    ),
    silicon(
        "warn",
        "издаёт тревогу!",
        None,
        &["sound/machines/warning-buzzer.ogg"],
    ),
    silicon(
        "slowclap",
        "активирует процессор медленных хлопков.",
        None,
        &["sound/machines/slowclap.ogg"],
    ),
    // `/datum/emote/slime/*` ([`slime_mood_key`]).
    only(
        EmoteMob::Slime.bit(),
        emote("bounce", "подпрыгивает на месте.", 0),
    ),
    only(EmoteMob::Slime.bit(), emote("jiggle", "трясется!", 0)),
    only(
        EmoteMob::Slime.bit(),
        emote("light", "на мгновение светится.", 0),
    ),
    only(EmoteMob::Slime.bit(), emote("vibrate", "вибрирует!", 0)),
    slime_mood("moodnone"),
    slime_mood("moodsneaky"),
    slime_mood("moodsmile"),
    slime_mood("moodcat"),
    slime_mood("moodpout"),
    slime_mood("moodsad"),
    slime_mood("moodangry"),
    // BandaStation `vulpkanin.dm`.
    species(
        EmoteSpecies::Vulpkanin,
        "howl",
        "воет.",
        "воет на %t.",
        bp!("vulpkanin/howl.ogg"),
        false,
        6.0,
    ),
    species(
        EmoteSpecies::Vulpkanin,
        "growl",
        "рычит.",
        "рычит на %t.",
        bp!(
            "vulpkanin/growl1.ogg",
            "vulpkanin/growl2.ogg",
            "vulpkanin/growl3.ogg"
        ),
        false,
        2.0,
    ),
    species(
        EmoteSpecies::Vulpkanin,
        "purr",
        "урчит.",
        "урчит на %t.",
        bp!("vulpkanin/purr.ogg"),
        true,
        2.0,
    ),
    species(
        EmoteSpecies::Vulpkanin,
        "bark",
        "гавкает.",
        "гавкает на %t.",
        bp!("vulpkanin/bark.ogg"),
        true,
        2.0,
    ),
    species(
        EmoteSpecies::Vulpkanin,
        "wbark",
        "дважды гавкает.",
        "дважды гавкает на %t.",
        bp!("vulpkanin/wbark.ogg"),
        true,
        2.0,
    ),
    // BandaStation `tajaran.dm`.
    species(
        EmoteSpecies::Tajaran,
        "meow_t",
        "мяукает.",
        "мяукает на %t.",
        bp!("tajaran/tajaran_meow.ogg"),
        true,
        4.0,
    ),
    species(
        EmoteSpecies::Tajaran,
        "mow",
        "раздражённо мяукает.",
        "раздражённо мяукает на %t.",
        bp!("tajaran/tajaran_annoyed_meow.ogg"),
        true,
        4.0,
    ),
    species(
        EmoteSpecies::Tajaran,
        "purr_t",
        "мурчит.",
        "мурчит на %t.",
        bp!("tajaran/tajaran_purr.ogg"),
        false,
        4.0,
    ),
    species(
        EmoteSpecies::Tajaran,
        "pur",
        "кратко мурчит.",
        "кратко мурчит на %t.",
        bp!("tajaran/tajaran_purr_short.ogg"),
        false,
        4.0,
    ),
    species(
        EmoteSpecies::Tajaran,
        "purrr",
        "длительно мурчит.",
        "длительно мурчит на %t.",
        bp!("tajaran/tajaran_purr_long.ogg"),
        false,
        4.0,
    ),
    species(
        EmoteSpecies::Tajaran,
        "hiss_t",
        "шипит.",
        "шипит на %t.",
        bp!("tajaran/tajaran_hiss.ogg"),
        false,
        4.0,
    ),
    // BandaStation `skrell.dm`.
    species(
        EmoteSpecies::Skrell,
        "warble",
        "трелит.",
        "трелит на %t.",
        bp!("skrell/warble_1.ogg", "skrell/warble_2.ogg"),
        true,
        EMOTE_COOLDOWN_SECS,
    ),
    species(
        EmoteSpecies::Skrell,
        "melodicwarble",
        "мелодично трелит.",
        "мелодично трелит на %t.",
        bp!("skrell/melodicwarble.ogg"),
        true,
        EMOTE_COOLDOWN_SECS,
    ),
    species(
        EmoteSpecies::Skrell,
        "croak",
        "квакает.",
        "квакает на %t.",
        bp!(
            "skrell/croak_1.ogg",
            "skrell/croak_2.ogg",
            "skrell/croak_3.ogg"
        ),
        true,
        EMOTE_COOLDOWN_SECS,
    ),
    species(
        EmoteSpecies::Skrell,
        "croak_anger",
        "гневно квакает!",
        "гневно квакает на %t.",
        bp!("skrell/anger_1.ogg", "skrell/anger_2.ogg"),
        true,
        EMOTE_COOLDOWN_SECS,
    ),
];

/// The first emote with this key (code-run emotes, where the mob type does not matter).
pub fn find(key: &str) -> Option<&'static Emote> {
    EMOTES.iter().find(|e| e.key == key)
}

/// tg `can_run_emote` without the stat checks: the mob type, the trait, the species.
pub fn allowed(emote: &Emote, user: &EmoteUser) -> bool {
    emote.mobs & user.mob.bit() != 0
        && (!emote.silicon || user.silicon_emotes)
        && emote.species.is_none_or(|s| s == user.species)
        && (user.mob != EmoteMob::Brain || user.in_processor)
        && (emote.key != "wag" || user.tail.is_some())
}

/// tg `select_message_type` (no mime: the vow does not reach emotes yet).
fn select_message(emote: &Emote, user: &EmoteUser) -> &'static str {
    // BandaStation's `wag/select_message_type`: stopping is its own line.
    if emote.key == "wag" && user.tail == Some(true) {
        return "перестаёт вилять хвостом.";
    }
    let pick = |m: &'static str| (!m.is_empty()).then_some(m);
    let special = match user.mob {
        EmoteMob::AlienAdult => pick(emote.message_alien),
        EmoteMob::AlienLarva => pick(emote.message_larva),
        EmoteMob::Ai => pick(emote.message_ai),
        _ => None,
    }
    .or_else(|| {
        (user.mob == EmoteMob::Cyborg || user.robotic)
            .then(|| pick(emote.message_robot))
            .flatten()
    })
    .or_else(|| {
        (ANIMAL & user.mob.bit() != 0)
            .then(|| pick(emote.message_animal))
            .flatten()
    });
    special.unwrap_or(emote.message)
}

/// The emotes that only put a hand gesture (`/obj/item/hand_item`) in a free hand.
pub const HAND_GESTURES: [&str; 6] = ["circle", "slap", "shoesteal", "noogie", "hand", "kiss"];

/// tg `/datum/emote/ai/emotion_display`'s `emotion` (`AI_EMOTION_*`) of an AI emote key.
pub fn ai_emotion(key: &str) -> Option<&'static str> {
    Some(match key {
        "blank" => "Blank",
        "veryhappy" => "Very Happy",
        "happy" => "Happy",
        "neutral" => "Neutral",
        "unsure" => "Unsure",
        "confused" => "Confused",
        "sad" => "Sad",
        "bsod" => "BSOD",
        "dead" => "Dead",
        "download" => "Download",
        "trollface" => "Problems?",
        "awesome" => "Awesome",
        "dorfy" => "Dorfy",
        "thinking" => "Thinking",
        "facepalm" => "Facepalm",
        "friendcomputer" => "Friend Computer",
        "blueglow" => "Blue Glow",
        "redglow" => "Red Glow",
        _ => return None,
    })
}

/// tg `/datum/emote/slime/mood`'s `mood_key` (`moodnone`: none).
pub fn slime_mood_key(key: &str) -> Option<Option<&'static str>> {
    Some(match key {
        "moodnone" => None,
        "moodsneaky" => Some("mischievous"),
        "moodsmile" => Some(":3"),
        "moodcat" => Some(":33"),
        "moodpout" => Some("pout"),
        "moodsad" => Some("sad"),
        "moodangry" => Some("angry"),
        _ => return None,
    })
}

const COUGH_F: &[&str] = &[
    "sound/mobs/humanoids/human/cough/female_cough1.ogg",
    "sound/mobs/humanoids/human/cough/female_cough2.ogg",
    "sound/mobs/humanoids/human/cough/female_cough3.ogg",
    "sound/mobs/humanoids/human/cough/female_cough4.ogg",
    "sound/mobs/humanoids/human/cough/female_cough5.ogg",
    "sound/mobs/humanoids/human/cough/female_cough6.ogg",
];
const COUGH_M: &[&str] = &[
    "sound/mobs/humanoids/human/cough/male_cough1.ogg",
    "sound/mobs/humanoids/human/cough/male_cough2.ogg",
    "sound/mobs/humanoids/human/cough/male_cough3.ogg",
    "sound/mobs/humanoids/human/cough/male_cough4.ogg",
    "sound/mobs/humanoids/human/cough/male_cough5.ogg",
    "sound/mobs/humanoids/human/cough/male_cough6.ogg",
];
const CRY_F: &[&str] = &[
    "sound/mobs/humanoids/human/cry/female_cry1.ogg",
    "sound/mobs/humanoids/human/cry/female_cry2.ogg",
    "modular_bandastation/emote_panel/audio/human/female/cry_female_1.ogg",
    "modular_bandastation/emote_panel/audio/human/female/cry_female_2.ogg",
    "modular_bandastation/emote_panel/audio/human/female/cry_female_3.ogg",
];
const CRY_M: &[&str] = &[
    "sound/mobs/humanoids/human/cry/male_cry1.ogg",
    "sound/mobs/humanoids/human/cry/male_cry2.ogg",
    "sound/mobs/humanoids/human/cry/male_cry3.ogg",
    "modular_bandastation/emote_panel/audio/human/male/cry_male_1.ogg",
    "modular_bandastation/emote_panel/audio/human/male/cry_male_2.ogg",
];
const LAUGH_F: &[&str] = &[
    "sound/mobs/humanoids/human/laugh/womanlaugh.ogg",
    "modular_bandastation/emote_panel/audio/human/female/laugh_female_1.ogg",
    "modular_bandastation/emote_panel/audio/human/female/laugh_female_2.ogg",
    "modular_bandastation/emote_panel/audio/human/female/laugh_female_3.ogg",
];
const LAUGH_M: &[&str] = &[
    "sound/mobs/humanoids/human/laugh/manlaugh1.ogg",
    "sound/mobs/humanoids/human/laugh/manlaugh2.ogg",
    "modular_bandastation/emote_panel/audio/human/male/laugh_male_1.ogg",
    "modular_bandastation/emote_panel/audio/human/male/laugh_male_2.ogg",
];
/// `SFX_FEMALE_SIGH`, `SFX_MALE_SIGH`.
const SIGH_F: &[&str] = &["female_sigh"];
const SIGH_M: &[&str] = &["male_sigh"];
const SNEEZE_F: &[&str] = &["sound/mobs/humanoids/human/sneeze/female_sneeze1.ogg"];
const SNEEZE_M: &[&str] = &["sound/mobs/humanoids/human/sneeze/male_sneeze1.ogg"];
const TAJARAN_SNEEZE_F: &[&str] = bp!(
    "tajaran/tajaran_sneeze_female1.ogg",
    "tajaran/tajaran_sneeze_female2.ogg"
);
const TAJARAN_SNEEZE_M: &[&str] = bp!("tajaran/tajaran_sneeze_male.ogg");
const SKRELL_LAUGH_F: &[&str] = bp!(
    "skrell/laugh_female_1.ogg",
    "skrell/laugh_female_2.ogg",
    "skrell/laugh_female_3.ogg"
);
const SKRELL_LAUGH_M: &[&str] = bp!(
    "skrell/laugh_male_1.ogg",
    "skrell/laugh_male_2.ogg",
    "skrell/laugh_male_3.ogg"
);
const SIGH_HUMAN_F: &[&str] = &[
    "sound/mobs/humanoids/human/sigh/female_sigh1.ogg",
    "sound/mobs/humanoids/human/sigh/female_sigh2.ogg",
    "sound/mobs/humanoids/human/sigh/female_sigh3.ogg",
];
const SIGH_HUMAN_M: &[&str] = &[
    "sound/mobs/humanoids/human/sigh/male_sigh1.ogg",
    "sound/mobs/humanoids/human/sigh/male_sigh2.ogg",
    "sound/mobs/humanoids/human/sigh/male_sigh3.ogg",
];
const SCREAM_F: &[&str] = &[
    "sound/mobs/humanoids/human/scream/femalescream_1.ogg",
    "sound/mobs/humanoids/human/scream/femalescream_2.ogg",
    "sound/mobs/humanoids/human/scream/femalescream_3.ogg",
    "sound/mobs/humanoids/human/scream/femalescream_4.ogg",
    "sound/mobs/humanoids/human/scream/femalescream_5.ogg",
];
const SCREAM_M: &[&str] = &[
    "sound/mobs/humanoids/human/scream/malescream_1.ogg",
    "sound/mobs/humanoids/human/scream/malescream_2.ogg",
    "sound/mobs/humanoids/human/scream/malescream_3.ogg",
    "sound/mobs/humanoids/human/scream/malescream_4.ogg",
    "sound/mobs/humanoids/human/scream/malescream_5.ogg",
    "sound/mobs/humanoids/human/scream/malescream_6.ogg",
];

/// BandaStation `/obj/item/organ/tongue/<species>/emote_sounds`: the species tongue's sound
/// for an emote key, which takes precedence over the emote's own (`EMOTE_SOUND_TONGUE`).
// shortcut: by species, not by the tongue organ in the mouth (a transplanted tongue keeps the
// species' sounds).
pub fn tongue_sounds(
    species: EmoteSpecies,
    female: bool,
    key: &str,
) -> Option<&'static [&'static str]> {
    use EmoteSpecies::*;
    let gendered = |f: &'static [&'static str], m: &'static [&'static str]| {
        if female { f } else { m }
    };
    let files: &'static [&'static str] = match (species, key) {
        (Other, _) => return None,
        (Vulpkanin | Tajaran | Pod | Skrell, "cough") => gendered(COUGH_F, COUGH_M),
        (Jelly, "cough") => bp!("slime/jelly_squish.ogg"),
        (Moth, "cough") => bp!("moth/moth_cough.ogg"),
        (_, "cry") => gendered(CRY_F, CRY_M),
        (Vulpkanin | Tajaran, "sneeze") => gendered(TAJARAN_SNEEZE_F, TAJARAN_SNEEZE_M),
        (Jelly | Pod | Skrell, "sneeze") => gendered(SNEEZE_F, SNEEZE_M),
        (Moth, "sneeze") => bp!("moth/moth_sneeze.ogg"),
        (Vulpkanin | Tajaran | Jelly | Pod, "laugh") => gendered(LAUGH_F, LAUGH_M),
        (Skrell, "laugh") => gendered(SKRELL_LAUGH_F, SKRELL_LAUGH_M),
        (Lizard, "laugh") => &["sound/mobs/humanoids/lizard/lizard_laugh1.ogg"],
        (Moth, "laugh") => &["sound/mobs/humanoids/moth/moth_laugh1.ogg"],
        (Vulpkanin | Jelly | Pod | Skrell, "sigh") => gendered(SIGH_F, SIGH_M),
        (Tajaran, "sigh") => gendered(SIGH_HUMAN_F, SIGH_HUMAN_M),
        (Tajaran, "scream") => bp!("tajaran/tajaran_scream.ogg"),
        (Jelly, "scream") => bp!("slime/jelly_scream.ogg"),
        (Lizard, "scream") => &[
            "sound/mobs/humanoids/lizard/lizard_scream_1.ogg",
            "sound/mobs/humanoids/lizard/lizard_scream_2.ogg",
            "sound/mobs/humanoids/lizard/lizard_scream_3.ogg",
        ],
        (Moth, "scream") => &["sound/mobs/humanoids/moth/scream_moth.ogg"],
        (Skrell, "scream") => gendered(SCREAM_F, SCREAM_M),
        (Skrell, "giggle") => gendered(
            bp!("skrell/giggle_female_1.ogg"),
            bp!("skrell/giggle_male_1.ogg"),
        ),
        (Moth, "giggle") => &["sound/mobs/humanoids/moth/moth_chitter.ogg"],
        (Lizard, "deathgasp") => &["sound/mobs/humanoids/lizard/deathsound.ogg"],
        (Moth, "deathgasp") => &["sound/mobs/humanoids/moth/moth_death.ogg"],
        _ => return None,
    };
    Some(files)
}

/// tg `sounds_by_mobtype` of `/mob/living/carbon/human` (by gender), with BandaStation's
/// additions (`modular_bandastation/emote_panel/code/human.dm`): what a human without a species
/// tongue sound makes. Sigh and snore are the `SFX_*` keys.
pub fn human_sounds(female: bool, key: &str) -> Option<&'static [&'static str]> {
    let g = |f: &'static [&'static str], m: &'static [&'static str]| if female { f } else { m };
    Some(match key {
        "gasp" => g(
            &[
                "sound/mobs/humanoids/human/gasp/gasp_female1.ogg",
                "sound/mobs/humanoids/human/gasp/gasp_female2.ogg",
                "sound/mobs/humanoids/human/gasp/gasp_female3.ogg",
                "modular_bandastation/emote_panel/audio/human/female/gasp_female_1.ogg",
                "modular_bandastation/emote_panel/audio/human/female/gasp_female_2.ogg",
                "modular_bandastation/emote_panel/audio/human/female/gasp_female_3.ogg",
                "modular_bandastation/emote_panel/audio/human/female/gasp_female_4.ogg",
            ],
            &[
                "sound/mobs/humanoids/human/gasp/gasp_male1.ogg",
                "sound/mobs/humanoids/human/gasp/gasp_male2.ogg",
                "modular_bandastation/emote_panel/audio/human/male/gasp_male_1.ogg",
                "modular_bandastation/emote_panel/audio/human/male/gasp_male_2.ogg",
                "modular_bandastation/emote_panel/audio/human/male/gasp_male_3.ogg",
                "modular_bandastation/emote_panel/audio/human/male/gasp_male_4.ogg",
                "modular_bandastation/emote_panel/audio/human/male/gasp_male_5.ogg",
            ],
        ),
        "laugh" => g(LAUGH_F, LAUGH_M),
        "sneeze" => g(SNEEZE_F, SNEEZE_M),
        "cough" => g(COUGH_F, COUGH_M),
        "scream" => g(SCREAM_F, SCREAM_M),
        "sigh" => g(SIGH_F, SIGH_M),
        "cry" => g(CRY_F, CRY_M),
        "giggle" => g(
            &[
                "modular_bandastation/emote_panel/audio/human/female/giggle_female_1.ogg",
                "modular_bandastation/emote_panel/audio/human/female/giggle_female_2.ogg",
                "modular_bandastation/emote_panel/audio/human/female/giggle_female_3.ogg",
                "modular_bandastation/emote_panel/audio/human/female/giggle_female_4.ogg",
            ],
            &[
                "modular_bandastation/emote_panel/audio/human/male/giggle_male_1.ogg",
                "modular_bandastation/emote_panel/audio/human/male/giggle_male_2.ogg",
            ],
        ),
        "yawn" => g(
            &[
                "modular_bandastation/emote_panel/audio/human/female/yawn_female_1.ogg",
                "modular_bandastation/emote_panel/audio/human/female/yawn_female_2.ogg",
                "modular_bandastation/emote_panel/audio/human/female/yawn_female_3.ogg",
            ],
            &[
                "modular_bandastation/emote_panel/audio/human/male/yawn_male_1.ogg",
                "modular_bandastation/emote_panel/audio/human/male/yawn_male_2.ogg",
            ],
        ),
        "choke" => g(
            &[
                "modular_bandastation/emote_panel/audio/human/female/choke_female_1.ogg",
                "modular_bandastation/emote_panel/audio/human/female/choke_female_2.ogg",
                "modular_bandastation/emote_panel/audio/human/female/choke_female_3.ogg",
            ],
            &[
                "modular_bandastation/emote_panel/audio/human/male/choke_male_1.ogg",
                "modular_bandastation/emote_panel/audio/human/male/choke_male_2.ogg",
                "modular_bandastation/emote_panel/audio/human/male/choke_male_3.ogg",
            ],
        ),
        "moan" => g(
            &[
                "modular_bandastation/emote_panel/audio/human/female/moan_female_1.ogg",
                "modular_bandastation/emote_panel/audio/human/female/moan_female_2.ogg",
                "modular_bandastation/emote_panel/audio/human/female/moan_female_3.ogg",
            ],
            &[
                "modular_bandastation/emote_panel/audio/human/male/moan_male_1.ogg",
                "modular_bandastation/emote_panel/audio/human/male/moan_male_2.ogg",
                "modular_bandastation/emote_panel/audio/human/male/moan_male_3.ogg",
            ],
        ),
        "sniff" => g(
            &["sound/mobs/humanoids/human/sniff/female_sniff.ogg"],
            &["sound/mobs/humanoids/human/sniff/male_sniff.ogg"],
        ),
        "sniffle" => g(
            &["modular_bandastation/emote_panel/audio/human/female/sniff_female.ogg"],
            &["modular_bandastation/emote_panel/audio/human/male/sniff_male.ogg"],
        ),
        "snore" => g(&["snore_female"], &["snore_male"]),
        "clap" => &[
            "sound/mobs/humanoids/human/clap/clap1.ogg",
            "sound/mobs/humanoids/human/clap/clap2.ogg",
            "sound/mobs/humanoids/human/clap/clap3.ogg",
            "sound/mobs/humanoids/human/clap/clap4.ogg",
        ],
        "snap" => &[
            "sound/mobs/humanoids/human/snap/fingersnap1.ogg",
            "sound/mobs/humanoids/human/snap/fingersnap2.ogg",
        ],
        "whistle" => &[
            "sound/mobs/humanoids/human/whistle/whistle1.ogg",
            "modular_bandastation/emote_panel/audio/whistle.ogg",
        ],
        "hiss" => &["sound/mobs/humanoids/human/hiss/human_hiss.ogg"],
        _ => return None,
    })
}

/// tg `get_sound`: the tongue's, then a human's `sounds_by_mobtype` ([`human_sounds`]), then
/// the emote's own (`sounds_by_mobtype` of `roar`: adults).
fn sound_of(emote: &Emote, user: &EmoteUser) -> Option<EmoteSound> {
    // `screech/get_sound` is the scream's.
    let key = if emote.key == "screech" {
        "scream"
    } else {
        emote.key
    };
    let files = (user.mob == EmoteMob::Human)
        .then(|| {
            tongue_sounds(user.species, user.female, key).or_else(|| human_sounds(user.female, key))
        })
        .flatten()
        .or_else(|| {
            let own = !emote.sounds.is_empty()
                && (emote.key != "roar" || user.mob == EmoteMob::AlienAdult);
            own.then_some(emote.sounds)
        })?;
    Some(EmoteSound {
        files,
        vary: emote.vary,
        volume: emote.volume,
        ignore_walls: emote.ignore_walls,
        audible: emote.flags & AUDIBLE != 0,
    })
}

/// What `*act params` does.
#[derive(Clone, Debug, PartialEq)]
pub enum EmoteOutcome {
    /// Shown to everyone around as "<name> <text>" (nothing when `text` is empty: an AI display
    /// or slime mood emote only has its effect).
    Run {
        key: &'static str,
        text: String,
        sound: Option<EmoteSound>,
    },
    /// Only the user reads it (tg `to_chat` warnings).
    Refused(String),
    /// tg: a silent refusal (cooldown, empty custom emote).
    Silent,
    /// `*help`: the list of keys.
    Help(String),
}

/// tg `/mob/proc/emote` -> `can_run_emote` -> `run_emote` for an intentional emote; `act` is
/// what followed `*`. `on_cooldown(key, cooldown)` says whether that emote is still cooling
/// down after its `cooldown` seconds.
pub fn run(
    act: &str,
    stat: SpeakerStat,
    user: &EmoteUser,
    mut on_cooldown: impl FnMut(&'static str, f64) -> bool,
) -> EmoteOutcome {
    let (act, params) = match act.split_once(' ') {
        Some((act, params)) => (act, params.trim()),
        None => (act, ""),
    };
    let act = act.to_lowercase();
    if act == "help" {
        let mut keys: Vec<&str> = EMOTES
            .iter()
            .filter(|e| allowed(e, user))
            .map(|e| e.key)
            .chain(["me"])
            .collect();
        keys.sort_unstable();
        keys.dedup();
        return EmoteOutcome::Help(format!(
            "Доступные эмоции, которые можно проиграть, сказав \"*emote\": {}.",
            keys.join(", ")
        ));
    }
    let emote = if act == "me" {
        // `/datum/emote/living/custom`: the text is the message.
        None
    } else {
        if !EMOTES.iter().any(|e| e.key == act) {
            return EmoteOutcome::Refused(format!(
                "Эмоция '{act}' не существует. Скажите '*help', чтобы получить список \
                 доступных эмоций."
            ));
        }
        match EMOTES.iter().find(|e| e.key == act && allowed(e, user)) {
            Some(e) => Some(e),
            None => {
                return EmoteOutcome::Refused(format!(
                    "Невозможно проиграть эмоцию '{act}'. Скажите '*help', чтобы получить \
                     список доступных эмоций."
                ));
            }
        }
    };
    let (key, cooldown, flags) = emote.map_or(("me", EMOTE_COOLDOWN_SECS, 0), |e| {
        (e.key, e.cooldown, e.flags)
    });
    if on_cooldown(key, cooldown) {
        return EmoteOutcome::Silent;
    }
    // `can_run_emote`; tg's warnings exist only in English.
    let refuse = |why: &str| EmoteOutcome::Refused(format!("You cannot {key} {why}!"));
    if stat >= SpeakerStat::Unconscious && flags & UNCONSCIOUS == 0 {
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
    let Some(emote) = emote else {
        return if params.is_empty() {
            EmoteOutcome::Silent
        } else {
            EmoteOutcome::Run {
                key,
                text: params.to_owned(),
                sound: None,
            }
        };
    };
    let text = match (params.is_empty(), emote.message_param) {
        (true, _) => select_message(emote, user).to_owned(),
        (false, Some(param)) => param.replace("%t", params),
        (false, None) => params.to_owned(),
    };
    // A plain emote without words does nothing; an effect-only one still runs (tg calls the
    // display / mood code after the parent's empty-message return).
    let effect_only =
        ai_emotion(key).is_some() || slime_mood_key(key).is_some() || HAND_GESTURES.contains(&key);
    if text.is_empty() && !effect_only {
        return EmoteOutcome::Silent;
    }
    EmoteOutcome::Run {
        key,
        text,
        sound: sound_of(emote, user),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: SpeakerStat = SpeakerStat::Conscious;
    const HUMAN_USER: EmoteUser = EmoteUser {
        mob: EmoteMob::Human,
        species: EmoteSpecies::Other,
        female: false,
        silicon_emotes: false,
        robotic: false,
        in_processor: false,
        tail: None,
    };

    fn user(mob: EmoteMob) -> EmoteUser {
        EmoteUser {
            mob,
            silicon_emotes: matches!(mob, EmoteMob::Cyborg | EmoteMob::Ai),
            ..HUMAN_USER
        }
    }

    fn run_as(act: &str, u: &EmoteUser) -> EmoteOutcome {
        run(act, C, u, |_, _| false)
    }

    fn ran(outcome: EmoteOutcome) -> String {
        match outcome {
            EmoteOutcome::Run { text, .. } => text,
            other => panic!("{other:?}"),
        }
    }

    fn sound(outcome: EmoteOutcome) -> Option<EmoteSound> {
        match outcome {
            EmoteOutcome::Run { sound, .. } => sound,
            other => panic!("{other:?}"),
        }
    }

    fn refused(outcome: &EmoteOutcome) -> bool {
        matches!(outcome, EmoteOutcome::Refused(_))
    }

    #[test]
    fn humans_gasp_and_scream_by_gender_and_a_species_tongue_comes_first() {
        let files = |act: &str, u: &EmoteUser| match run_as(act, u) {
            EmoteOutcome::Run { sound, .. } => sound.map(|s| s.files),
            other => panic!("{other:?}"),
        };
        let male = HUMAN_USER;
        let female = EmoteUser {
            female: true,
            ..HUMAN_USER
        };
        let gasp = files("gasp", &male).expect("a human gasps aloud");
        assert!(gasp.contains(&"sound/mobs/humanoids/human/gasp/gasp_male1.ogg"));
        let gasp_f = files("gasp", &female).unwrap();
        assert!(gasp_f.contains(&"sound/mobs/humanoids/human/gasp/gasp_female1.ogg"));
        assert!(files("scream", &male).unwrap().contains(&SCREAM_M[0]));
        let lizard = EmoteUser {
            species: EmoteSpecies::Lizard,
            ..HUMAN_USER
        };
        assert!(
            files("scream", &lizard).unwrap()[0].contains("lizard_scream"),
            "the tongue's sound wins"
        );
        assert_eq!(files("smile", &male), None);
    }

    #[test]
    fn emotes_follow_tg() {
        let h = &HUMAN_USER;
        assert_eq!(ran(run_as("scream", h)), "кричит!");
        assert_eq!(ran(run_as("SCREAM", h)), "кричит!");
        assert_eq!(ran(run_as("nod Бобу", h)), "кивает Бобу.");
        // Without `message_param` the parameter is the message.
        assert_eq!(ran(run_as("laugh над шуткой", h)), "над шуткой");
        assert_eq!(ran(run_as("me чешет затылок", h)), "чешет затылок");
        assert_eq!(run_as("me", h), EmoteOutcome::Silent);
        assert!(
            matches!(run_as("flarp", h), EmoteOutcome::Refused(m) if m.contains("'flarp' не существует"))
        );
        assert_eq!(run("wave", C, h, |_, _| true), EmoteOutcome::Silent);
        assert!(matches!(run_as("help", h), EmoteOutcome::Help(m) if m.contains("scream")));
    }

    #[test]
    fn wag_needs_a_tail_and_toggles_its_line() {
        assert!(refused(&run_as("wag", &HUMAN_USER)));
        let still = EmoteUser {
            tail: Some(false),
            ..HUMAN_USER
        };
        assert_eq!(ran(run_as("wag", &still)), "начинает вилять хвостом.");
        let wagging = EmoteUser {
            tail: Some(true),
            ..HUMAN_USER
        };
        assert_eq!(ran(run_as("wag", &wagging)), "перестаёт вилять хвостом.");
    }

    #[test]
    fn crit_allows_only_crit_emotes() {
        let h = &HUMAN_USER;
        let soft = SpeakerStat::SoftCrit;
        assert!(refused(&run("scream", soft, h, |_, _| false)));
        assert!(refused(&run("me ползёт", soft, h, |_, _| false)));
        assert!(refused(&run("wave", soft, h, |_, _| false)));
        assert_eq!(ran(run("cry", soft, h, |_, _| false)), "плачет.");
        assert_eq!(ran(run("gasp", soft, h, |_, _| false)), "задыхается!");
        let hard = SpeakerStat::HardCrit;
        assert!(refused(&run("cry", hard, h, |_, _| false)));
        assert_eq!(ran(run("gasp", hard, h, |_, _| false)), "задыхается!");
        assert!(refused(&run("gasp", SpeakerStat::Dead, h, |_, _| false)));
    }

    #[test]
    fn silicon_and_creature_emotes() {
        let borg = user(EmoteMob::Cyborg);
        let beep = run_as("beep", &borg);
        assert_eq!(
            sound(beep.clone()).map(|s| s.files),
            Some(&["sound/machines/beep/twobeep.ogg"][..])
        );
        assert_eq!(ran(beep), "бипает.");
        assert_eq!(ran(run_as("ping Бобу", &borg)), "сигналит в сторону Бобу.");
        // Silicon emotes need the trait; humans have none.
        assert!(refused(&run_as("beep", &HUMAN_USER)));
        let robotic_tongue = EmoteUser {
            silicon_emotes: true,
            ..HUMAN_USER
        };
        assert_eq!(ran(run_as("buzz", &robotic_tongue)), "жужжит.");
        // Living emotes reach silicons, with their own message; carbon ones do not.
        assert_eq!(ran(run_as("yawn", &borg)), "сочувственно зевает.");
        assert!(refused(&run_as("cry", &borg)));
        assert!(refused(&run_as("salute", &borg)));
        // `sad`: the AI's display first, a cyborg's trombone.
        let ai = user(EmoteMob::Ai);
        assert_eq!(
            run_as("sad", &ai),
            EmoteOutcome::Run {
                key: "sad",
                text: String::new(),
                sound: None
            }
        );
        assert_eq!(ai_emotion("trollface"), Some("Problems?"));
        assert_eq!(
            ran(run_as("sad", &borg)),
            "проигрывает звук грустного тромбона..."
        );
        assert!(refused(&run_as("happy", &borg)));
        // A brain emotes only from a processor; living emotes are not its own.
        let loose = user(EmoteMob::Brain);
        assert!(refused(&run_as("alarm", &loose)));
        let boxed = EmoteUser {
            in_processor: true,
            ..loose
        };
        assert_eq!(ran(run_as("alarm", &boxed)), "издаёт звук тревоги.");
        assert_eq!(ran(run_as("whistle", &boxed)), "свистит.");
        assert!(refused(&run_as("laugh", &boxed)));
        // Slimes, xenos, animals.
        let slime = user(EmoteMob::Slime);
        assert_eq!(ran(run_as("jiggle", &slime)), "трясется!");
        assert!(
            matches!(run_as("moodcat", &slime), EmoteOutcome::Run { text, .. } if text.is_empty())
        );
        assert_eq!(slime_mood_key("moodcat"), Some(Some(":33")));
        assert_eq!(ran(run_as("deathgasp", &slime)), "перестает двигаться...");
        assert!(refused(&run_as("jiggle", &HUMAN_USER)));
        let larva = user(EmoteMob::AlienLarva);
        let roar = run_as("roar", &larva);
        assert_eq!(sound(roar.clone()), None);
        assert_eq!(ran(roar), "тихо рычит.");
        assert!(sound(run_as("roar", &user(EmoteMob::AlienAdult))).is_some());
        assert_eq!(
            ran(run_as("gnarl", &larva)),
            "оскаливается и показывает зубы..."
        );
        assert_eq!(ran(run_as("ooga", &user(EmoteMob::Gorilla))), "oogas.");
        assert!(refused(&run_as("ooga", &user(EmoteMob::Basic))));
        // `*help` lists only what the mob may run.
        let EmoteOutcome::Help(help) = run_as("help", &borg) else {
            panic!("no help");
        };
        assert!(help.contains("beep") && !help.contains("jiggle") && !help.contains("salute"));
    }

    #[test]
    fn species_emotes_and_tongue_sounds() {
        let vulp = EmoteUser {
            species: EmoteSpecies::Vulpkanin,
            ..HUMAN_USER
        };
        let howl = run_as("howl луну", &vulp);
        assert_eq!(
            sound(howl.clone()).map(|s| s.files),
            Some(&["modular_bandastation/emote_panel/audio/vulpkanin/howl.ogg"][..])
        );
        assert_eq!(ran(howl), "воет на луну.");
        assert!(refused(&run_as("howl", &HUMAN_USER)));
        assert!(refused(&run_as("meow_t", &vulp)));
        // Per-emote cooldowns: the howl cools down for 6 s.
        let mut asked = None;
        run("howl", C, &vulp, |_, cooldown| {
            asked = Some(cooldown);
            false
        });
        assert_eq!(asked, Some(6.0));
        let tajaran = EmoteUser {
            species: EmoteSpecies::Tajaran,
            female: true,
            ..HUMAN_USER
        };
        assert_eq!(ran(run_as("mow", &tajaran)), "раздражённо мяукает.");
        assert_eq!(
            sound(run_as("sneeze", &tajaran)).map(|s| s.files),
            Some(TAJARAN_SNEEZE_F)
        );
        let lizard = EmoteUser {
            species: EmoteSpecies::Lizard,
            ..HUMAN_USER
        };
        assert_eq!(
            sound(run_as("scream", &lizard)).map(|s| s.files.len()),
            Some(3)
        );
        // A plain human screams with tg's human sounds (`sounds_by_mobtype`).
        assert_eq!(
            sound(run_as("scream", &HUMAN_USER)).map(|s| s.files),
            Some(SCREAM_M)
        );
        assert_eq!(
            tongue_sounds(EmoteSpecies::Moth, false, "deathgasp"),
            Some(&["sound/mobs/humanoids/moth/moth_death.ogg"][..])
        );
        let skrell = EmoteUser {
            species: EmoteSpecies::Skrell,
            ..HUMAN_USER
        };
        assert_eq!(ran(run_as("croak_anger", &skrell)), "гневно квакает!");
    }

    /// The hand gestures run without words for carbons only; the game makes the gesture.
    #[test]
    fn hand_gestures_run_without_words() {
        assert!(matches!(
            run_as("slap", &HUMAN_USER),
            EmoteOutcome::Run { key: "slap", text, .. } if text.is_empty()
        ));
        assert!(refused(&run_as("circle", &user(EmoteMob::Cyborg))));
    }
}
