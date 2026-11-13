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

/// What a body has that some emotes need ([`Emote::needs`], [`EmoteUser::has`]).
/// `ORGAN_SLOT_EYES` (`blink`, `blink_r`).
pub const HAS_EYES: u16 = 1;
/// `ORGAN_SLOT_TONGUE` (`tongue`).
pub const HAS_TONGUE: u16 = 2;
/// `/obj/item/clothing/glasses/regular` or `sunglasses` in the eye slot (`glasses`).
pub const HAS_GLASSES: u16 = 4;
/// `/datum/status_effect/grouped/bodypart_effect/ethereal_glow` (`glow`, `flare`, `flicker`).
pub const HAS_GLOW: u16 = 8;
/// `TRAIT_CAT_EMOTES_ALLOWED` (`meow`, `purr`).
pub const HAS_CAT: u16 = 16;
/// `TRAIT_SIMIAN` (the monkey `gnarl`, `roll`, `scratch`, `sign`).
pub const HAS_SIMIAN: u16 = 32;
/// `TRAIT_LESSER_HUMANOID` (`message_monkey`).
pub const HAS_LESSER: u16 = 64;
/// What a body without an [`EmoteUser`] has: a human's eyes and tongue.
pub const DEFAULT_HAS: u16 = HAS_EYES | HAS_TONGUE;

/// The BandaStation species whose emotes or tongue sounds differ (`species_type_whitelist`,
/// `/obj/item/organ/tongue/<species>/emote_sounds`, `get_evil_laugh_sound`), subtypes included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EmoteSpecies {
    /// `/datum/species/human` itself.
    #[default]
    Human,
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

/// On a body: who runs its emotes, kept by the game. Absent = a human with eyes and a tongue.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
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
    /// `HAS_*` bits.
    pub has: u16,
    /// `TRAIT_MIMING` (`message_mime`).
    pub mime: bool,
}

impl Default for EmoteUser {
    fn default() -> Self {
        Self {
            mob: EmoteMob::Human,
            species: EmoteSpecies::Human,
            female: false,
            silicon_emotes: false,
            robotic: false,
            in_processor: false,
            tail: None,
            has: DEFAULT_HAS,
            mime: false,
        }
    }
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
    /// `message_mime`: what a mime under the vow shows instead of `message`.
    pub message_mime: &'static str,
    /// `message_monkey` (`TRAIT_LESSER_HUMANOID`).
    pub message_monkey: &'static str,
    pub flags: u8,
    /// `HAS_*` bits the body must have (`trait_required`, `can_run_emote` overrides).
    pub needs: u16,
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
    message_mime: "",
    message_monkey: "",
    flags: 0,
    needs: 0,
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

/// The emote with its `message_mime`.
const fn mimed(message_mime: &'static str, base: Emote) -> Emote {
    Emote {
        message_mime,
        ..base
    }
}

/// The emote only for bodies with these `HAS_*` bits.
const fn needing(needs: u16, base: Emote) -> Emote {
    Emote { needs, ..base }
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

/// The emote table, in tg's `GLOB.emote_list` order where keys repeat. The game makes the
/// effects of `run_emote` overrides on [`crate::server::Emoted`] (`ss13_game::speech`).
// shortcut: no emote overlays (`tongue`, `blush`, `cry`, `sigh`, `glasses` visuals): the port has
// no timed bodypart overlays yet ("Эмоции живых и людей").
pub const EMOTES: &[Emote] = &[
    with_param("bow", "кланяется.", "кланяется %t.", REQUIRE_HANDS),
    emote("blush", "краснеет.", 0),
    mimed("бесшумно давится!", emote("choke", "давится!", AUDIBLE)),
    mimed(
        "изображает смешок.",
        emote("chuckle", "усмехается.", AUDIBLE),
    ),
    only(CARBON, emote("clap", "хлопает.", REQUIRE_HANDS)),
    mimed(
        "изображает преувеличенный кашель!",
        emote("cough", "кашляет!", AUDIBLE),
    ),
    emote("cross", "скрещивает свои руки.", REQUIRE_HANDS),
    only(
        CARBON,
        mimed(
            "бесшумно плачет.",
            emote("cry", "плачет.", SOFTCRIT | AUDIBLE),
        ),
    ),
    // BandaStation: 5 s, the dancer spins and jitters.
    Emote {
        cooldown: 5.0,
        ..emote("dance", "радостно танцует.", REQUIRE_HANDS)
    },
    Emote {
        message_robot: "на мгновение вздрагивает и замирает, глаза медленно темнеют.",
        message_ai: "скрипит, экран мерцает, пока системы медленно выключаются.",
        message_alien: "издает ослабевающий гортанный крик и падает на пол...",
        message_larva: "с тошнотворным шипением выдыхает воздух и падает на пол...",
        message_animal: "перестает двигаться...",
        message_monkey: "издает слабый визг, когда рушится и перестает двигаться...",
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
    mimed(
        "бесшумно задыхается!",
        emote(
            "gasp",
            "задыхается!",
            SOFTCRIT | HARDCRIT | UNCONSCIOUS | AUDIBLE,
        ),
    ),
    mimed("бесшумно хихикает!", emote("giggle", "хихикает.", AUDIBLE)),
    emote("grin", "ухмыляется.", 0),
    mimed(
        "изображает болезненный стон!",
        emote("groan", "болезненно стонет!", AUDIBLE),
    ),
    mimed("бесшумно смеется!", emote("laugh", "смеется.", AUDIBLE)),
    with_param("nod", "кивает.", "кивает %t.", 0),
    // Not `EMOTE_AUDIBLE`: a mute's salute clicks too.
    Emote {
        sounds: &["sound/mobs/humanoids/human/salute/salute.ogg"],
        ..only(
            HUMAN,
            with_param("salute", "салютует.", "салютует %t.", REQUIRE_HANDS),
        )
    },
    mimed("изображает крик!", emote("scream", "кричит!", AUDIBLE)),
    emote("shake", "качает своей головой.", 0),
    emote("shiver", "дрожит.", 0),
    only(HUMAN, emote("shrug", "пожимает плечами.", 0)),
    // `/datum/emote/living/carbon/human/wag`: only with a wag-able tail; the game toggles the
    // wagging on [`crate::server::Emoted`] (`ss13_game::medical::organs`).
    only(HUMAN, emote("wag", "начинает вилять хвостом.", 0)),
    mimed(
        "изображает преувеличенный бесшумный вздох.",
        emote("sigh", "вздыхает.", AUDIBLE),
    ),
    emote("smile", "улыбается.", 0),
    mimed(
        "изображает преувеличенный бесшумный чих.",
        emote("sneeze", "чихает.", AUDIBLE),
    ),
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
    only(
        CARBON,
        mimed("бесшумно свистит!", emote("whistle", "свистит.", AUDIBLE)),
    ),
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
    only(
        CARBON,
        mimed("кажется, стонет!", emote("moan", "стонет!", AUDIBLE)),
    ),
    Emote {
        vary: true,
        ..mimed(
            "бесшумно шмыгает носом.",
            emote("sniff", "шмыгает носом.", AUDIBLE),
        )
    },
    mimed(
        "бесшумно храпит.",
        emote("snore", "храпит.", UNCONSCIOUS | AUDIBLE),
    ),
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
        ..only(
            HUMAN,
            mimed("бесшумно визжит.", emote("screech", "визжит!", AUDIBLE)),
        )
    },
    // `/datum/emote/living/carbon/human/screech/roar`: the scream's sound too.
    Emote {
        cooldown: 10.0,
        ..only(
            HUMAN,
            mimed("изображает рык.", emote("roar", "рычит!", AUDIBLE)),
        )
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
        ..only(
            HUMAN,
            mimed(
                "бесшумно стрекочет?",
                emote("chitter", "стрекочет!", AUDIBLE),
            ),
        )
    },
    Emote {
        species: Some(EmoteSpecies::Moth),
        sounds: &["sound/mobs/humanoids/moth/moth_squeak.ogg"],
        ..only(
            HUMAN,
            mimed("бесшумно пищит?", emote("squeak", "пищит!", AUDIBLE)),
        )
    },
    // `/datum/emote/living/carbon/circle`, `slap`, `shoesteal`: no message, the game puts the
    // hand gesture in a free hand (`ss13_game::devices::hand_items`, on `Emoted`).
    only(CARBON, emote("circle", "", REQUIRE_HANDS)),
    only(CARBON, emote("slap", "", REQUIRE_HANDS)),
    only(CARBON, emote("shoesteal", "", REQUIRE_HANDS)),
    only(CARBON, emote("wink", "подмигивает.", 0)),
    Emote {
        // BandaStation's "сочувственно зевнуть." is an infinitive slip; the verb form here.
        message_robot: "сочувственно зевает.",
        message_ai: "сочувственно зевает.",
        ..mimed(
            "изображает преувеличенный бесшумный зевок.",
            emote("yawn", "зевает.", AUDIBLE),
        )
    },
    // `/datum/emote/living/*` without sounds of their own.
    // 1.6 s: the matrix taunt's projectile block lasts that long in tg.
    Emote {
        cooldown: 1.6,
        ..emote("taunt", "дразнится!", 0)
    },
    // `tongue/run_emote`: a human without a tongue is told so ([`run`]).
    emote("tongue", "высовывает язык.", 0),
    emote("tunesing", "подпевает мелодию.", 0),
    mimed("изображает отрыжку.", emote("burp", "рыгает.", AUDIBLE)),
    emote("collapse", "падает!", AUDIBLE),
    emote("drool", "пускает слюни.", 0),
    emote("faint", "теряет сознание.", 0),
    // Wings open and close in tg; no wing organs here, so the line only.
    emote("flap", "хлопает крыльями.", REQUIRE_HANDS),
    emote("aflap", "сердито хлопает крыльями!", REQUIRE_HANDS),
    mimed("бесшумно тужится.", emote("gag", "тужится.", AUDIBLE)),
    with_param("glare", "сверлит взглядом.", "сверлит взглядом %t.", 0),
    emote("grimace", "строит гримасу.", 0),
    with_param("look", "смотрит.", "смотрит на %t.", 0),
    // tg: no hands check here, `point/run_emote` points with a leg, a glance or the tongue.
    // shortcut: hands are blocked here only by crit, which refuses `point` anyway, so the
    // handless variants never come up; add them with a hands-blocked capability.
    Emote {
        cooldown: 1.0,
        ..with_param("point", "указывает.", "указывает на %t.", 0)
    },
    emote("wheeze", "хрипит!", AUDIBLE),
    mimed(
        "бесшумно надувает губы.",
        emote("pout", "надувает губы.", 0),
    ),
    emote("scowl", "сердито смотрит.", 0),
    emote("sit", "садится.", 0),
    emote("smug", "самодовольно улыбается.", 0),
    with_param("stare", "уставился.", "уставился на %t.", 0),
    emote("stretch", "протягивает свои руки.", 0),
    emote("sulk", "грустно обижается.", 0),
    emote("sway", "покачивается.", 0),
    emote("tilt", "наклоняет голову на бок.", 0),
    emote("tremble", "дрожит в страхе!", 0),
    emote("twitch", "сильно дёргается.", 0),
    emote("twitch_s", "дёргается.", 0),
    mimed("кажется, задет.", emote("whimper", "хнычет.", AUDIBLE)),
    emote("wsmile", "слабо улыбается.", 0),
    mimed(
        "бесшумно и неприятно булькает.",
        emote("gurgle", "издает неприятное бульканье.", AUDIBLE),
    ),
    emote("inhale", "делает вдох.", AUDIBLE),
    emote("exhale", "выдыхает.", AUDIBLE),
    mimed("делает грубый жест!", emote("swear", "ругается!", AUDIBLE)),
    // `/datum/emote/living/monkey/*`: `TRAIT_SIMIAN`.
    needing(
        HAS_SIMIAN,
        mimed(
            "бесшумно оскаливается, показывая зубы...",
            emote("gnarl", "оскаливается и показывает зубы...", 0),
        ),
    ),
    needing(HAS_SIMIAN, emote("roll", "катится.", REQUIRE_HANDS)),
    needing(HAS_SIMIAN, emote("scratch", "чешется.", REQUIRE_HANDS)),
    needing(
        HAS_SIMIAN,
        with_param("sign", "", "показывает число %t.", REQUIRE_HANDS),
    ),
    // `/datum/emote/living/cat/*`: `TRAIT_CAT_EMOTES_ALLOWED`.
    Emote {
        sounds: &["cat_meow"],
        vary: true,
        ..needing(
            HAS_CAT,
            mimed("бесшумно мяукает.", emote("meow", "мяукает!", AUDIBLE)),
        )
    },
    // BandaStation leaves "purrs." untranslated.
    Emote {
        sounds: &["cat_purr"],
        vary: true,
        ..needing(HAS_CAT, emote("purr", "мурлычет.", AUDIBLE))
    },
    // BandaStation `emote_panel/code/human.dm`: by species and physique ([`evil_laugh`]).
    Emote {
        vary: true,
        ..mimed(
            "бесшумно злорадно смеётся!",
            emote("laughevil", "злорадно смеётся.", AUDIBLE),
        )
    },
    // By gender, for any mob ([`human_sounds`] for humans); not `EMOTE_AUDIBLE`.
    Emote {
        sounds: &["modular_bandastation/emote_panel/audio/human/male/sniff_male.ogg"],
        ..mimed(
            "бесшумно нюхает.",
            with_param("sniffle", "нюхает.", "нюхает %t.", 0),
        )
    },
    // `/datum/emote/flip`, `spin`: no message, the game spins the mob; spamming them trips
    // or dizzies ([`EmoteOutcome::Spammed`]).
    only(ANY, emote("flip", "", REQUIRE_HANDS)),
    only(ANY, emote("backflip", "", REQUIRE_HANDS)),
    only(ANY, emote("spin", "", REQUIRE_HANDS)),
    // `/datum/emote/living/carbon/*`.
    only(
        CARBON,
        emote(
            "airguitar",
            "невероятно играет на воображаемой гитаре.",
            REQUIRE_HANDS,
        ),
    ),
    only(ALIEN, emote("roll", "катится.", REQUIRE_HANDS)),
    only(ALIEN, emote("scratch", "чешется.", REQUIRE_HANDS)),
    // `sign/select_param`: only a number is signed ([`run`]).
    only(
        ALIEN,
        with_param("sign", "", "показывает число %t.", REQUIRE_HANDS),
    ),
    only(
        HUMAN,
        with_param("signal", "", "показывает %t пальцев.", REQUIRE_HANDS),
    ),
    only(ALIEN, emote("tail", "машет хвостом.", 0)),
    // `/datum/emote/living/carbon/human/*`.
    only(
        HUMAN,
        with_param(
            "dap",
            "к сожалению, нет никого, кому можно дать пятюню, и даёт её самому себе. Стыдоба.",
            "даёт пятюню %t.",
            REQUIRE_HANDS,
        ),
    ),
    needing(
        HAS_GLASSES,
        only(HUMAN, emote("glasses", "приподнимает свои очки.", 0)),
    ),
    only(
        HUMAN,
        mimed("бесшумно ворчит!", emote("grumble", "ворчит!", AUDIBLE)),
    ),
    only(
        HUMAN,
        with_param(
            "handshake",
            "дает рукопожатие себе.",
            "дает рукопожатие %t.",
            REQUIRE_HANDS | AUDIBLE,
        ),
    ),
    only(
        HUMAN,
        with_param("hug", "обнимает себя.", "обнимает %t.", REQUIRE_HANDS),
    ),
    only(
        HUMAN,
        mimed("бесшумно бормочет!", emote("mumble", "бормочет!", AUDIBLE)),
    ),
    only(HUMAN, emote("pale", "на мгновение бледнеет.", 0)),
    only(HUMAN, emote("raise", "поднимает руку.", REQUIRE_HANDS)),
    only(
        HUMAN,
        emote("slit", "проводит пальцем поперёк горла.", REQUIRE_HANDS),
    ),
    only(HUMAN, emote("scratchhead", "чешет голову.", REQUIRE_HANDS)),
    only(HUMAN, emote("time", "проверяет время.", REQUIRE_HANDS)),
    only(HUMAN, emote("tap", "нетерпеливо постукивает ногой.", 0)),
    only(
        HUMAN,
        emote(
            "halt",
            "поднимает ладонь, призывая остановиться.",
            REQUIRE_HANDS,
        ),
    ),
    only(
        HUMAN,
        emote("shush", "подносит палец к губам.", REQUIRE_HANDS),
    ),
    only(
        HUMAN,
        emote(
            "listen",
            "складывает ладонь рупором у уха, прислушиваясь.",
            REQUIRE_HANDS,
        ),
    ),
    only(
        HUMAN,
        emote("think", "задумчиво почёсывает голову.", REQUIRE_HANDS),
    ),
    only(
        HUMAN,
        emote(
            "beckon",
            "машет рукой, подзывая подойти ближе.",
            REQUIRE_HANDS,
        ),
    ),
    only(
        HUMAN,
        emote("airquote", "делает воздушные кавычки.", REQUIRE_HANDS),
    ),
    only(
        HUMAN,
        emote("crazy", "крутит пальцем у виска.", REQUIRE_HANDS),
    ),
    only(HUMAN, emote("squint", "прищуривается.", 0)),
    only(HUMAN, emote("rub", "чешет подбородок.", REQUIRE_HANDS)),
    only(HUMAN, emote("clear", "прочищает горло.", 0)),
    // shortcut: no eyelids are drawn, so the eyes do not visibly close.
    needing(HAS_EYES, only(HUMAN, emote("blink", "моргает.", 0))),
    needing(
        HAS_EYES,
        only(HUMAN, emote("blink_r", "быстро моргает.", 0)),
    ),
    // Ethereal glow emotes ([`HAS_GLOW`]).
    // shortcut: the ethereal glow has no light in the port, so `handle_glow_emote` and
    // `start_flicker` change nothing; add them with the ethereal light.
    needing(HAS_GLOW, only(HUMAN, emote("glow", "ярко светится!", 0))),
    Emote {
        sounds: &["sound/mobs/humanoids/ethereal/ethereal_hiss.ogg"],
        ..needing(
            HAS_GLOW,
            only(HUMAN, emote("flare", "вспыхивает с яркой вспышкой!", 0)),
        )
    },
    Emote {
        sounds: &["sound/effects/sparks/sparks4.ogg"],
        ..needing(HAS_GLOW, only(HUMAN, emote("flicker", "мерцает.", 0)))
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
    mimed(
        "делает вид, что воет.",
        species(
            EmoteSpecies::Vulpkanin,
            "howl",
            "воет.",
            "воет на %t.",
            bp!("vulpkanin/howl.ogg"),
            false,
            6.0,
        ),
    ),
    mimed(
        "бесшумно рычит.",
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
    mimed(
        "бесшумно мяукает.",
        species(
            EmoteSpecies::Tajaran,
            "meow_t",
            "мяукает.",
            "мяукает на %t.",
            bp!("tajaran/tajaran_meow.ogg"),
            true,
            4.0,
        ),
    ),
    mimed(
        "бесшумно раздражённо мяукает.",
        species(
            EmoteSpecies::Tajaran,
            "mow",
            "раздражённо мяукает.",
            "раздражённо мяукает на %t.",
            bp!("tajaran/tajaran_annoyed_meow.ogg"),
            true,
            4.0,
        ),
    ),
    mimed(
        "бесшумно мурчит.",
        species(
            EmoteSpecies::Tajaran,
            "purr_t",
            "мурчит.",
            "мурчит на %t.",
            bp!("tajaran/tajaran_purr.ogg"),
            false,
            4.0,
        ),
    ),
    mimed(
        "бесшумно кратко мурчит.",
        species(
            EmoteSpecies::Tajaran,
            "pur",
            "кратко мурчит.",
            "кратко мурчит на %t.",
            bp!("tajaran/tajaran_purr_short.ogg"),
            false,
            4.0,
        ),
    ),
    mimed(
        "бесшумно длительно мурчит.",
        species(
            EmoteSpecies::Tajaran,
            "purrr",
            "длительно мурчит.",
            "длительно мурчит на %t.",
            bp!("tajaran/tajaran_purr_long.ogg"),
            false,
            4.0,
        ),
    ),
    mimed(
        "бесшумно шипит.",
        species(
            EmoteSpecies::Tajaran,
            "hiss_t",
            "шипит.",
            "шипит на %t.",
            bp!("tajaran/tajaran_hiss.ogg"),
            false,
            4.0,
        ),
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

/// tg `can_run_emote` without the stat checks: the mob type, the traits and organs, the
/// species.
pub fn allowed(emote: &Emote, user: &EmoteUser) -> bool {
    emote.mobs & user.mob.bit() != 0
        && emote.needs & !user.has == 0
        && (!emote.silicon || user.silicon_emotes)
        && emote.species.is_none_or(|s| s == user.species)
        && (user.mob != EmoteMob::Brain || user.in_processor)
        && (emote.key != "wag" || user.tail.is_some())
}

/// tg `select_message_type`.
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
        (user.has & HAS_LESSER != 0)
            .then(|| pick(emote.message_monkey))
            .flatten()
    })
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
    let own = if user.mime && !emote.message_mime.is_empty() {
        emote.message_mime
    } else {
        emote.message
    };
    special.unwrap_or(own)
}

/// BYOND `isnum(text2num(text))`: the text starts with a number.
fn starts_with_number(text: &str) -> bool {
    let digits = text
        .trim_start()
        .trim_start_matches(['+', '-'])
        .trim_start_matches('.');
    digits.starts_with(|c: char| c.is_ascii_digit())
}

/// The emotes that only put a hand gesture (`/obj/item/hand_item`) in a free hand.
pub const HAND_GESTURES: [&str; 3] = ["circle", "slap", "shoesteal"];

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
        (Human | Other, _) => return None,
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

const EVIL_LAUGH_F: &[&str] = bp!(
    "human/female/evil_laugh_female_1.ogg",
    "human/female/evil_laugh_female_2.ogg"
);
const EVIL_LAUGH_M: &[&str] = bp!(
    "human/male/evil_laugh_male_1.ogg",
    "human/male/evil_laugh_male_2.ogg"
);

/// BandaStation `/datum/species/proc/get_evil_laugh_sound` (`laughevil`): none for the
/// species that do not override it.
pub fn evil_laugh(species: EmoteSpecies, female: bool) -> Option<&'static [&'static str]> {
    use EmoteSpecies::*;
    match species {
        Human | Jelly | Pod | Skrell | Tajaran | Vulpkanin => {
            Some(if female { EVIL_LAUGH_F } else { EVIL_LAUGH_M })
        }
        Lizard => Some(&["sound/mobs/humanoids/lizard/lizard_laugh1.ogg"]),
        Moth => Some(&["sound/mobs/humanoids/moth/moth_laugh1.ogg"]),
        Other => None,
    }
}

/// tg `get_sound`: the tongue's, then a human's `sounds_by_mobtype` ([`human_sounds`]), then
/// the emote's own (`sounds_by_mobtype` of `roar`: adults).
fn sound_of(emote: &Emote, user: &EmoteUser) -> Option<EmoteSound> {
    let human = user.mob == EmoteMob::Human;
    // `screech/get_sound` is the scream's, and the human `roar` is a `screech`.
    let key = match emote.key {
        "screech" => "scream",
        "roar" if human => "scream",
        key => key,
    };
    let by_body = if key == "laughevil" {
        // Its `get_sound` replaces the tongue's and the mob type's.
        human
            .then(|| evil_laugh(user.species, user.female))
            .flatten()
    } else {
        human
            .then(|| {
                tongue_sounds(user.species, user.female, key)
                    .or_else(|| human_sounds(user.female, key))
            })
            .flatten()
    };
    let files = by_body.or_else(|| {
        let own =
            !emote.sounds.is_empty() && (emote.key != "roar" || user.mob == EmoteMob::AlienAdult);
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
    /// `flip/check_cooldown`, `spin/check_cooldown`: run again while cooling down, the emote
    /// does not run but the mob may fall over or get dizzy (the game decides).
    Spammed(&'static str),
}

/// The emotes whose cooldown check has an effect ([`EmoteOutcome::Spammed`]).
pub const SPAM_PENALTY: [&str; 3] = ["flip", "backflip", "spin"];

/// The emotes without a message that still run for their effect (tg calls the display, mood,
/// gesture or spin code after the parent's empty-message return).
fn effect_only(key: &str) -> bool {
    ai_emotion(key).is_some()
        || slime_mood_key(key).is_some()
        || HAND_GESTURES.contains(&key)
        || SPAM_PENALTY.contains(&key)
}

/// `can_run_emote`'s stat checks; tg's warnings exist only in English.
fn stat_refusal(key: &str, flags: u8, stat: SpeakerStat) -> Option<EmoteOutcome> {
    let refuse = |why: &str| Some(EmoteOutcome::Refused(format!("You cannot {key} {why}!")));
    if stat >= SpeakerStat::Unconscious && flags & UNCONSCIOUS == 0 {
        return refuse("while unconscious");
    }
    // Hands are blocked from soft crit on (`TRAIT_HANDS_BLOCKED`).
    if stat >= SpeakerStat::SoftCrit && flags & REQUIRE_HANDS != 0 {
        return Some(EmoteOutcome::Refused(format!(
            "You cannot use your hands to {key} right now!"
        )));
    }
    match stat {
        SpeakerStat::SoftCrit if flags & SOFTCRIT == 0 => refuse("while in a critical condition"),
        SpeakerStat::HardCrit if flags & HARDCRIT == 0 => refuse("while in a critical condition"),
        SpeakerStat::Dead => refuse("while dead"),
        _ => None,
    }
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
        // The penalty's own `can_run_emote` warns, as the emote would.
        return match stat_refusal(key, flags, stat) {
            Some(refusal) if SPAM_PENALTY.contains(&key) => refusal,
            None if SPAM_PENALTY.contains(&key) => EmoteOutcome::Spammed(key),
            _ => EmoteOutcome::Silent,
        };
    }
    if let Some(refusal) = stat_refusal(key, flags, stat) {
        return refusal;
    }
    if key == "tongue" && user.mob == EmoteMob::Human && user.has & HAS_TONGUE == 0 {
        return EmoteOutcome::Refused("You don't have a tongue!".to_owned());
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
        // `sign/select_param`: not a number signs nothing (its `message` is empty).
        (false, Some(_)) if matches!(key, "sign" | "signal") && !starts_with_number(params) => {
            emote.message.to_owned()
        }
        (false, Some(param)) => param.replace("%t", params),
        (false, None) => params.to_owned(),
    };
    // A plain emote without words does nothing; an effect-only one still runs.
    if text.is_empty() && !effect_only(key) {
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
        species: EmoteSpecies::Human,
        female: false,
        silicon_emotes: false,
        robotic: false,
        in_processor: false,
        tail: None,
        has: DEFAULT_HAS,
        mime: false,
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

    #[test]
    fn the_default_user_is_a_human_with_eyes_and_a_tongue() {
        assert_eq!(EmoteUser::default(), HUMAN_USER);
        assert_eq!(ran(run_as("blink", &EmoteUser::default())), "моргает.");
    }

    #[test]
    fn organ_clothing_and_trait_emotes_need_them() {
        let h = &HUMAN_USER;
        let without = |bits: u16| EmoteUser {
            has: h.has & !bits,
            ..HUMAN_USER
        };
        let with = |bits: u16| EmoteUser {
            has: h.has | bits,
            ..HUMAN_USER
        };
        // Eyes blink; no eyes, no blink.
        assert_eq!(ran(run_as("blink_r", h)), "быстро моргает.");
        assert!(refused(&run_as("blink", &without(HAS_EYES))));
        // A tongueless human is told so; a cyborg has no tongue check.
        assert_eq!(ran(run_as("tongue", h)), "высовывает язык.");
        assert_eq!(
            run_as("tongue", &without(HAS_TONGUE)),
            EmoteOutcome::Refused("You don't have a tongue!".to_owned())
        );
        assert_eq!(
            ran(run_as(
                "tongue",
                &EmoteUser {
                    has: 0,
                    ..user(EmoteMob::Cyborg)
                }
            )),
            "высовывает язык."
        );
        // Glasses, the ethereal glow, cat and simian traits.
        assert!(refused(&run_as("glasses", h)));
        assert_eq!(
            ran(run_as("glasses", &with(HAS_GLASSES))),
            "приподнимает свои очки."
        );
        assert!(refused(&run_as("flare", h)));
        let flare = run_as("flare", &with(HAS_GLOW));
        assert_eq!(
            sound(flare).map(|s| s.files),
            Some(&["sound/mobs/humanoids/ethereal/ethereal_hiss.ogg"][..])
        );
        assert!(refused(&run_as("meow", h)));
        let meow = run_as("meow", &with(HAS_CAT));
        assert_eq!(
            sound(meow.clone()).map(|s| s.files),
            Some(&["cat_meow"][..])
        );
        assert_eq!(ran(meow), "мяукает!");
        let cat = EmoteUser {
            has: HAS_CAT,
            ..user(EmoteMob::Basic)
        };
        assert_eq!(ran(run_as("purr", &cat)), "мурлычет.");
        // A vulpkanin without cat ears purrs its own way.
        let vulp = EmoteUser {
            species: EmoteSpecies::Vulpkanin,
            ..HUMAN_USER
        };
        assert_eq!(ran(run_as("purr", &vulp)), "урчит.");
        assert!(refused(&run_as("roll", h)));
        assert_eq!(ran(run_as("roll", &with(HAS_SIMIAN))), "катится.");
        let gorilla = EmoteUser {
            has: HAS_SIMIAN,
            ..user(EmoteMob::Gorilla)
        };
        assert_eq!(ran(run_as("scratch", &gorilla)), "чешется.");
        assert_eq!(
            ran(run_as("gnarl", &gorilla)),
            "оскаливается и показывает зубы..."
        );
    }

    #[test]
    fn creature_and_hand_sign_emotes() {
        let alien = user(EmoteMob::AlienAdult);
        assert_eq!(ran(run_as("tail", &alien)), "машет хвостом.");
        assert_eq!(ran(run_as("roll", &alien)), "катится.");
        assert!(refused(&run_as("tail", &HUMAN_USER)));
        // Only a number is signed.
        assert_eq!(ran(run_as("sign 3", &alien)), "показывает число 3.");
        assert_eq!(run_as("sign привет", &alien), EmoteOutcome::Silent);
        assert_eq!(run_as("sign", &alien), EmoteOutcome::Silent);
        assert_eq!(
            ran(run_as("signal 5", &HUMAN_USER)),
            "показывает 5 пальцев."
        );
        assert_eq!(run_as("signal пять", &HUMAN_USER), EmoteOutcome::Silent);
        assert!(refused(&run_as("signal 5", &alien)));
        assert!(starts_with_number(" -2.5 штуки") && !starts_with_number("x1"));
        // Hands and crit.
        let soft = SpeakerStat::SoftCrit;
        assert!(refused(&run("hug", soft, &HUMAN_USER, |_, _| false)));
        assert_eq!(ran(run_as("hug Боба", &HUMAN_USER)), "обнимает Боба.");
        assert_eq!(
            ran(run_as("flap", &user(EmoteMob::Cyborg))),
            "хлопает крыльями."
        );
    }

    #[test]
    fn mimes_and_monkeys_have_their_own_lines() {
        let mime = EmoteUser {
            mime: true,
            ..HUMAN_USER
        };
        assert_eq!(ran(run_as("scream", &mime)), "изображает крик!");
        assert_eq!(ran(run_as("swear", &mime)), "делает грубый жест!");
        // No mime line: the usual one.
        assert_eq!(ran(run_as("smile", &mime)), "улыбается.");
        // The mime's line gives way to the alien's.
        let mime_alien = EmoteUser {
            mime: true,
            ..user(EmoteMob::AlienAdult)
        };
        assert_eq!(
            ran(run_as("deathgasp", &mime_alien)),
            "издает ослабевающий гортанный крик и падает на пол..."
        );
        let monkey = EmoteUser {
            has: DEFAULT_HAS | HAS_SIMIAN | HAS_LESSER,
            ..HUMAN_USER
        };
        assert_eq!(
            ran(run_as("deathgasp", &monkey)),
            "издает слабый визг, когда рушится и перестает двигаться..."
        );
        let tajaran = EmoteUser {
            species: EmoteSpecies::Tajaran,
            mime: true,
            ..HUMAN_USER
        };
        assert_eq!(
            ran(run_as("mow", &tajaran)),
            "бесшумно раздражённо мяукает."
        );
    }

    #[test]
    fn evil_laughs_sniffles_and_roars_sound_right() {
        let files = |act: &str, u: &EmoteUser| sound(run_as(act, u)).map(|s| s.files);
        let female = EmoteUser {
            female: true,
            ..HUMAN_USER
        };
        assert_eq!(files("laughevil", &female), Some(EVIL_LAUGH_F));
        assert_eq!(files("laughevil", &HUMAN_USER), Some(EVIL_LAUGH_M));
        let lizard = EmoteUser {
            species: EmoteSpecies::Lizard,
            ..HUMAN_USER
        };
        assert_eq!(
            files("laughevil", &lizard),
            Some(&["sound/mobs/humanoids/lizard/lizard_laugh1.ogg"][..])
        );
        // A species without its own evil laugh, and a non-human, laugh silently.
        let other = EmoteUser {
            species: EmoteSpecies::Other,
            ..HUMAN_USER
        };
        assert_eq!(files("laughevil", &other), None);
        assert_eq!(files("laughevil", &user(EmoteMob::Cyborg)), None);
        assert_eq!(
            files("sniffle", &female),
            Some(&["modular_bandastation/emote_panel/audio/human/female/sniff_female.ogg"][..])
        );
        assert_eq!(ran(run_as("sniffle цветок", &HUMAN_USER)), "нюхает цветок.");
        assert!(files("sniffle", &user(EmoteMob::Basic)).is_some());
        // A human roar is a scream; the alien roar keeps its hiss.
        assert_eq!(files("roar", &HUMAN_USER), Some(SCREAM_M));
        assert_eq!(ran(run_as("roar", &HUMAN_USER)), "рычит!");
        assert_eq!(
            files("salute", &HUMAN_USER),
            Some(&["sound/mobs/humanoids/human/salute/salute.ogg"][..])
        );
    }

    #[test]
    fn flips_and_spins_run_for_their_effect_and_spamming_them_costs() {
        let h = &HUMAN_USER;
        for key in SPAM_PENALTY {
            assert!(matches!(
                run_as(key, h),
                EmoteOutcome::Run { key: k, text, .. } if k == key && text.is_empty()
            ));
            assert_eq!(run(key, C, h, |_, _| true), EmoteOutcome::Spammed(key));
        }
        // Spamming from crit only warns, as the emote would.
        assert!(refused(&run("flip", SpeakerStat::SoftCrit, h, |_, _| true)));
        // Other emotes just wait.
        assert_eq!(run("collapse", C, h, |_, _| true), EmoteOutcome::Silent);
        assert_eq!(ran(run_as("collapse", h)), "падает!");
        assert_eq!(ran(run_as("faint", h)), "теряет сознание.");
        // `taunt` and `point` cool down faster.
        let mut asked = Vec::new();
        for key in ["taunt", "point", "dance"] {
            run(key, C, h, |_, cooldown| {
                asked.push(cooldown);
                false
            });
        }
        assert_eq!(asked, vec![1.6, 1.0, 5.0]);
    }
}
