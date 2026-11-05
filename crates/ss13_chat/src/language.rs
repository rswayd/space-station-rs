//! Languages, after tg `code/modules/language` (`/datum/language`, `/datum/language_holder`,
//! `translate_language` in `game/say.dm`, `get_message_mods`' `,x` key) and BandaStation's
//! languages (Неорусский, канилунц, сик'таир, курволиус).
//!
//! - [`LanguageId`] names a language, [`LanguageId::def`] holds its tg vars (key, syllables,
//!   chances, priority, mutual understanding).
//! - [`LanguageHolder`] on a body lists what it understands and speaks, by source, what is
//!   blocked, its partial understandings and the selected language. A body without one is tg's
//!   `atom_basic` holder: Galactic Common understood and spoken.
//! - [`Tongue`] on a body limits what it can physically speak (`could_speak_language`); a
//!   [`crate::speech::Tongueless`] body speaks only `LANGUAGE_TONGUELESS_SPEECH` languages.
//! - A listener who does not understand hears [`scramble_paragraph`]: syllables of the language,
//!   with some words kept by partial understanding.
//!
//! tg caches scrambled words (the last 50) and sentences (the last 20) so a repeated word sounds
//! the same and every radio listener hears one scramble. Here the scramble is a pure function of
//! the round's [`ScrambleSeed`], the language and the word (or sentence and understanding): the
//! same word always scrambles the same way in a round, which is what the caches are for.
// shortcut: no "1000 most common words" table (an English list: Russian speech never hits it),
// so partial understanding of a word is the language's percentage - 5 (tg's value for a word
// outside the list); codespeak scrambles with Galactic Common's syllables instead of ion-law
// code phrases (no `ion_laws.json` strings); no random name generation, language icons (a text
// tag instead) or spans (the robot font of machine languages).

use bevy::prelude::*;

/// `LANGUAGE_TONGUELESS_SPEECH`: spoken without a tongue.
pub const TONGUELESS_SPEECH: u8 = 1 << 1;
/// `LANGUAGE_HIDE_ICON_IF_UNDERSTOOD`.
pub const HIDE_ICON_IF_UNDERSTOOD: u8 = 1 << 2;
/// `LANGUAGE_HIDE_ICON_IF_NOT_UNDERSTOOD`.
pub const HIDE_ICON_IF_NOT_UNDERSTOOD: u8 = 1 << 3;

/// `UNDERSTOOD_LANGUAGE`, `SPOKEN_LANGUAGE`.
pub const SPOKEN: u8 = 1 << 0;
pub const UNDERSTOOD: u8 = 1 << 1;
pub const ALL: u8 = SPOKEN | UNDERSTOOD;

/// Full understanding (`has_language`), as a percentage of mutual understanding.
pub const FULL: u8 = 100;

/// Every language (`GLOB.all_languages`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LanguageId {
    Common,
    Uncommon,
    Draconic,
    Moffic,
    Nekomimetic,
    Sylvan,
    Terrum,
    Calcic,
    Voltaic,
    Slime,
    Shadowtongue,
    Mushroom,
    Machine,
    Drone,
    Xenocommon,
    Narsie,
    Aphasia,
    Beachbum,
    Buzzwords,
    Carptongue,
    Codespeak,
    Monkey,
    Piratespeak,
    Spinwarder,
    Wormspeak,
    Canilunzt,
    Qurvolious,
    Siiktajr,
}

impl LanguageId {
    pub const ALL: [Self; 28] = [
        Self::Common,
        Self::Uncommon,
        Self::Draconic,
        Self::Moffic,
        Self::Nekomimetic,
        Self::Sylvan,
        Self::Terrum,
        Self::Calcic,
        Self::Voltaic,
        Self::Slime,
        Self::Shadowtongue,
        Self::Mushroom,
        Self::Machine,
        Self::Drone,
        Self::Xenocommon,
        Self::Narsie,
        Self::Aphasia,
        Self::Beachbum,
        Self::Buzzwords,
        Self::Carptongue,
        Self::Codespeak,
        Self::Monkey,
        Self::Piratespeak,
        Self::Spinwarder,
        Self::Wormspeak,
        Self::Canilunzt,
        Self::Qurvolious,
        Self::Siiktajr,
    ];

    pub fn def(self) -> &'static LanguageDef {
        defs::of(self)
    }

    /// The language a `,x` key names (`key`, case-sensitive as DM's `==`).
    pub fn from_key(key: char) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.def().key == Some(key))
    }

    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.def().path == path)
    }
}

/// Where a language's syllables come from (`syllables`, a `pick_weight_recursive` list).
#[derive(Clone, Copy, Debug)]
pub enum Syllables {
    /// No syllables: words are starred (`stars`).
    None,
    Flat(&'static [&'static str]),
    /// Equally weighted sublists (Galactic Common's Mandarin and English halves).
    Groups(&'static [&'static [&'static str]]),
    /// Nar'Sian: its own list plus every pair of `base_syllables` (`/datum/language/narsie/New`).
    Narsie,
}

/// The vars of a `/datum/language`.
#[derive(Clone, Copy, Debug)]
pub struct LanguageDef {
    pub id: LanguageId,
    pub path: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// `key`: `,x` speaks it; `None` only by selection.
    pub key: Option<char>,
    pub flags: u8,
    pub syllables: Syllables,
    pub special_characters: &'static [&'static str],
    pub sentence_chance: f32,
    pub between_word_sentence_chance: f32,
    pub space_chance: f32,
    pub between_word_space_chance: f32,
    pub additional_syllable_low: i32,
    pub additional_syllable_high: i32,
    pub default_priority: i32,
    /// `mutual_understanding`: languages a speaker of this one partly understands, percent.
    pub mutual_understanding: &'static [(LanguageId, u8)],
}

impl LanguageDef {
    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

/// The base tongue's `get_possible_languages()`.
const TONGUE_LANGUAGES: [LanguageId; 15] = {
    use LanguageId::*;
    [
        Common,
        Uncommon,
        Spinwarder,
        Draconic,
        Codespeak,
        Monkey,
        Narsie,
        Beachbum,
        Aphasia,
        Piratespeak,
        Moffic,
        Sylvan,
        Shadowtongue,
        Terrum,
        Nekomimetic,
    ]
};

/// The tongue of a body (`/obj/item/organ/tongue`), kept by the game: what it can speak.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tongue {
    /// The base list plus `languages_native`.
    Organic(&'static [LanguageId]),
    /// `/obj/item/organ/tongue/alien`.
    Alien,
    /// `/obj/item/organ/tongue/robot`: anything ("THE MAGIC OF ELECTRONICS").
    Robot,
}

impl Tongue {
    /// The tongue of an organ type (`languages_native` of tg's and BandaStation's tongues).
    pub fn of_path(path: &str) -> Self {
        use LanguageId::*;
        const NATIVE: [(&str, &[LanguageId]); 16] = [
            ("/obj/item/organ/tongue/lizard", &[Draconic]),
            ("/obj/item/organ/tongue/bone", &[Calcic]),
            ("/obj/item/organ/tongue/ethereal", &[Voltaic]),
            ("/obj/item/organ/tongue/cat", &[Nekomimetic]),
            ("/obj/item/organ/tongue/jelly", &[Slime]),
            ("/obj/item/organ/tongue/monkey", &[Monkey]),
            ("/obj/item/organ/tongue/moth", &[Moffic]),
            ("/obj/item/organ/tongue/mush", &[Mushroom]),
            ("/obj/item/organ/tongue/pod", &[Sylvan]),
            ("/obj/item/organ/tongue/golem", &[Terrum]),
            ("/obj/item/organ/tongue/shadow", &[Shadowtongue]),
            ("/obj/item/organ/tongue/fly", &[Buzzwords]),
            ("/obj/item/organ/tongue/fish", &[Carptongue]),
            ("/obj/item/organ/tongue/vulpkanin", &[Canilunzt]),
            ("/obj/item/organ/tongue/skrell", &[Qurvolious]),
            ("/obj/item/organ/tongue/tajaran", &[Siiktajr]),
        ];
        let under = |root: &str| {
            path.strip_prefix(root)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        };
        if under("/obj/item/organ/tongue/alien") {
            return Self::Alien;
        }
        if under("/obj/item/organ/tongue/robot") {
            return Self::Robot;
        }
        NATIVE
            .into_iter()
            .find(|(root, _)| under(root))
            .map_or(Self::Organic(&[]), |(_, native)| Self::Organic(native))
    }

    /// `could_speak_language` of the tongue.
    pub fn could_speak(self, language: LanguageId) -> bool {
        use LanguageId::*;
        match self {
            Self::Organic(native) => {
                TONGUE_LANGUAGES.contains(&language) || native.contains(&language)
            }
            Self::Alien => matches!(language, Xenocommon | Common | Uncommon | Draconic | Monkey),
            Self::Robot => true,
        }
    }
}

/// `/mob/living/carbon/could_speak_language`: the tongue decides; without one only the
/// tongueless languages. A body with neither (silicons, simple mobs) speaks anything
/// (`/atom/movable/could_speak_language`).
pub fn could_speak(tongue: Option<&Tongue>, tongueless: bool, language: LanguageId) -> bool {
    if tongueless {
        return language.def().has(TONGUELESS_SPEECH);
    }
    tongue.is_none_or(|t| t.could_speak(language))
}

/// A language source (`LANGUAGE_*` of `__DEFINES/language.dm`): what granted or blocked it, so
/// that taking one source away keeps the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LanguageSource {
    Atom,
    Mind,
    Species,
    Tongue,
    Aphasia,
    Absorb,
    Cultist,
    Quirk,
    /// A martial art's type (the sleeping carp's Carptongue).
    MartialArt,
}

/// `/datum/language_holder`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct LanguageHolder {
    understood: Vec<(LanguageId, LanguageSource)>,
    spoken: Vec<(LanguageId, LanguageSource)>,
    blocked_understanding: Vec<(LanguageId, LanguageSource)>,
    blocked_speaking: Vec<(LanguageId, LanguageSource)>,
    /// `grant_partial_language`: language, source, percent.
    partial: Vec<(LanguageId, LanguageSource, u8)>,
    /// `omnitongue`: the tongue does not limit speech.
    pub omnitongue: bool,
    /// `selected_language`.
    pub selected: Option<LanguageId>,
}

/// A `/datum/language_holder/*` preset (`GLOB.prototype_language_holders`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Preset {
    pub understood: &'static [LanguageId],
    pub spoken: &'static [LanguageId],
    pub blocked: &'static [LanguageId],
    /// `blocked_understanding` when it differs from `blocked_speaking` (blood worms).
    pub blocked_understanding: Option<&'static [LanguageId]>,
    pub selected: Option<LanguageId>,
}

impl Preset {
    /// The preset of a holder type, `None` for an unknown one.
    pub fn of_path(path: &str) -> Option<Self> {
        use LanguageId::*;
        let both = |l: &'static [LanguageId]| Preset {
            understood: l,
            spoken: l,
            ..Preset::default()
        };
        Some(match path.strip_prefix("/datum/language_holder")? {
            "/atom_basic" | "/human_basic" => both(&[Common]),
            "/empty" | "" => Preset::default(),
            "/alien" => Preset {
                blocked: &[Common],
                ..both(&[Xenocommon])
            },
            "/construct" => both(&[Common, Narsie]),
            "/drone" => Preset {
                blocked: &[Common],
                ..both(&[Drone])
            },
            "/drone/syndicate" => both(&[Drone]),
            "/jelly" => both(&[Common, Slime]),
            "/lightbringer" => Preset {
                blocked: &[Common],
                ..both(&[Slime])
            },
            "/lizard" => both(&[Common, Draconic]),
            // tg sets `selected_language` on the prototype; species languages are granted one by
            // one, so the mob still selects by priority.
            "/lizard/ash" => Preset {
                selected: Some(Draconic),
                ..both(&[Common, Draconic])
            },
            "/lizard/silver" => Preset {
                understood: &[Common, Uncommon, Draconic],
                spoken: &[Uncommon, Draconic],
                selected: Some(Uncommon),
                ..Preset::default()
            },
            "/lizard/hear_common" => Preset {
                understood: &[Common, Draconic],
                spoken: &[Draconic],
                selected: Some(Draconic),
                ..Preset::default()
            },
            "/monkey" => Preset {
                understood: &[Common, Monkey],
                spoken: &[Monkey],
                ..Preset::default()
            },
            "/mushroom" => both(&[Common, Mushroom]),
            "/slime" => Preset {
                understood: &[Common, Slime],
                spoken: &[Slime],
                ..Preset::default()
            },
            "/venus" => Preset {
                blocked: &[Common],
                ..both(&[Sylvan])
            },
            "/spider" => Preset {
                blocked: &[Common],
                ..both(&[Buzzwords])
            },
            "/blood_worm" => Preset {
                understood: &[Wormspeak, Common],
                spoken: &[Wormspeak],
                blocked: &[Common],
                blocked_understanding: Some(&[]),
                ..Preset::default()
            },
            "/synthetic" | "/synthetic/silicon" => both(&[Common, Uncommon, Machine]),
            "/moth" => both(&[Common, Moffic]),
            "/skeleton" => both(&[Common, Calcic]),
            "/ethereal" => both(&[Common, Voltaic]),
            "/cerulean" => both(&[Common, Carptongue]),
            "/golem" => both(&[Common, Terrum]),
            "/fly" => both(&[Common, Buzzwords]),
            "/plant" => both(&[Common, Sylvan]),
            "/felinid" => both(&[Common, Nekomimetic]),
            "/shadowpeople" => both(&[Common, Shadowtongue]),
            "/clown" => both(&[Common, Monkey]),
            "/syndicate" => both(&[Common, Codespeak]),
            "/beachbum" => Preset {
                selected: Some(Beachbum),
                ..both(&[Common, Beachbum])
            },
            "/spinwarder" => Preset {
                selected: Some(Spinwarder),
                ..both(&[Common, Spinwarder])
            },
            "/spinwarder_exclusive" => Preset {
                selected: Some(Spinwarder),
                ..both(&[Spinwarder])
            },
            "/speaking_machine" => both(&[
                Common,
                Uncommon,
                Machine,
                Draconic,
                Moffic,
                Calcic,
                Voltaic,
                Nekomimetic,
            ]),
            "/carp" => Preset {
                selected: Some(Carptongue),
                ..both(&[Carptongue])
            },
            "/carp/dragon" => Preset {
                understood: &[Carptongue, Draconic, Common],
                spoken: &[Carptongue, Draconic],
                selected: Some(Carptongue),
                ..Preset::default()
            },
            "/carp/hear_common" => Preset {
                understood: &[Carptongue, Common],
                spoken: &[Carptongue],
                selected: Some(Carptongue),
                ..Preset::default()
            },
            // BandaStation's species holders.
            "/vulpkanin" => both(&[Common, Canilunzt]),
            "/tajaran" => both(&[Common, Siiktajr]),
            "/skrell" => both(&[Common, Qurvolious]),
            _ => return None,
        })
    }

    fn blocked_understanding(&self) -> &'static [LanguageId] {
        self.blocked_understanding.unwrap_or(self.blocked)
    }
}

impl LanguageHolder {
    /// A new holder of a preset, every language from `source` (`new initial_language_holder`).
    pub fn from_preset(preset: &Preset, source: LanguageSource) -> Self {
        let mut holder = Self::default();
        holder.apply(preset, source);
        holder.selected = preset.selected;
        holder
    }

    /// tg `/datum/language_holder/universal`: every language from the mind, omnitongue.
    pub fn universal() -> Self {
        let mut holder = Self::default();
        holder.grant_all(LanguageSource::Mind);
        holder.omnitongue = true;
        holder
    }

    /// `on_species_gain`: the preset's languages and blocks, all from `source`.
    pub fn apply(&mut self, preset: &Preset, source: LanguageSource) {
        for &l in preset.understood {
            self.grant(l, UNDERSTOOD, source);
        }
        for &l in preset.spoken {
            self.grant(l, SPOKEN, source);
        }
        for &l in preset.blocked {
            self.block(l, SPOKEN, source);
        }
        for &l in preset.blocked_understanding() {
            self.block(l, UNDERSTOOD, source);
        }
    }

    /// `on_species_loss`: takes back what [`Self::apply`] gave from `source`.
    pub fn unapply(&mut self, preset: &Preset, source: LanguageSource) {
        for &l in preset.understood {
            self.remove(l, UNDERSTOOD, Some(source));
        }
        for &l in preset.spoken {
            self.remove(l, SPOKEN, Some(source));
        }
        for &l in preset.blocked {
            self.unblock(l, SPOKEN, Some(source));
        }
        for &l in preset.blocked_understanding() {
            self.unblock(l, UNDERSTOOD, Some(source));
        }
    }

    /// `grant_language`.
    pub fn grant(&mut self, language: LanguageId, flags: u8, source: LanguageSource) {
        let entry = (language, source);
        if flags & UNDERSTOOD != 0 && !self.understood.contains(&entry) {
            self.understood.push(entry);
        }
        if flags & SPOKEN != 0 && !self.spoken.contains(&entry) {
            self.spoken.push(entry);
        }
    }

    /// `grant_all_languages` (without omnitongue).
    pub fn grant_all(&mut self, source: LanguageSource) {
        for language in LanguageId::ALL {
            self.grant(language, ALL, source);
        }
    }

    /// `remove_language`; `source: None` is `LANGUAGE_ALL`.
    pub fn remove(&mut self, language: LanguageId, flags: u8, source: Option<LanguageSource>) {
        let keep = |&(l, s): &(LanguageId, LanguageSource)| {
            l != language || source.is_some_and(|source| s != source)
        };
        if flags & UNDERSTOOD != 0 {
            self.understood.retain(keep);
        }
        if flags & SPOKEN != 0 {
            self.spoken.retain(keep);
        }
    }

    /// `remove_all_languages(source)`.
    pub fn remove_all(&mut self, source: Option<LanguageSource>) {
        for language in LanguageId::ALL {
            self.remove(language, ALL, source);
        }
    }

    /// `add_blocked_language`.
    pub fn block(&mut self, language: LanguageId, flags: u8, source: LanguageSource) {
        let entry = (language, source);
        if flags & SPOKEN != 0 && !self.blocked_speaking.contains(&entry) {
            self.blocked_speaking.push(entry);
        }
        if flags & UNDERSTOOD != 0 && !self.blocked_understanding.contains(&entry) {
            self.blocked_understanding.push(entry);
        }
    }

    /// `remove_blocked_language`; `source: None` is `LANGUAGE_ALL`.
    pub fn unblock(&mut self, language: LanguageId, flags: u8, source: Option<LanguageSource>) {
        let keep = |&(l, s): &(LanguageId, LanguageSource)| {
            l != language || source.is_some_and(|source| s != source)
        };
        if flags & SPOKEN != 0 {
            self.blocked_speaking.retain(keep);
        }
        if flags & UNDERSTOOD != 0 {
            self.blocked_understanding.retain(keep);
        }
    }

    /// `grant_partial_language`.
    pub fn grant_partial(&mut self, language: LanguageId, amount: u8, source: LanguageSource) {
        self.partial
            .retain(|&(l, s, _)| !(l == language && s == source));
        self.partial.push((language, source, amount.min(FULL)));
    }

    /// `remove_partial_language`.
    pub fn remove_partial(&mut self, language: LanguageId, source: LanguageSource) {
        self.partial
            .retain(|&(l, s, _)| !(l == language && s == source));
    }

    fn blocked(list: &[(LanguageId, LanguageSource)], language: LanguageId) -> bool {
        list.iter().any(|&(l, _)| l == language)
    }

    /// Whether anything in the holder comes from `source`.
    pub fn has_source(&self, source: LanguageSource) -> bool {
        [
            &self.understood,
            &self.spoken,
            &self.blocked_understanding,
            &self.blocked_speaking,
        ]
        .iter()
        .any(|list| list.iter().any(|&(_, s)| s == source))
            || self.partial.iter().any(|&(_, s, _)| s == source)
    }

    /// `has_language(language, flags)`: listed for one of `flags` and not blocked for it.
    pub fn has(&self, language: LanguageId, flags: u8) -> bool {
        let listed =
            |list: &[(LanguageId, LanguageSource)]| list.iter().any(|&(l, _)| l == language);
        (flags & SPOKEN != 0
            && !Self::blocked(&self.blocked_speaking, language)
            && listed(&self.spoken))
            || (flags & UNDERSTOOD != 0
                && !Self::blocked(&self.blocked_understanding, language)
                && listed(&self.understood))
    }

    /// `best_mutual_languages[language]`: the best partial understanding, from a granted
    /// percentage or from an understood language's `mutual_understanding` (not while that
    /// language is blocked).
    pub fn partial_understanding(&self, language: LanguageId) -> u8 {
        let granted = self
            .partial
            .iter()
            .filter(|&&(l, ..)| l == language)
            .map(|&(.., amount)| amount);
        let mutual = self
            .understood
            .iter()
            .filter(|&&(l, _)| !Self::blocked(&self.blocked_understanding, l))
            .flat_map(|&(l, _)| l.def().mutual_understanding)
            .filter(|&&(l, _)| l == language)
            .map(|&(_, amount)| amount);
        granted.chain(mutual).max().unwrap_or(0)
    }

    /// How well a message in `language` is understood: [`FULL`] or the partial percentage.
    pub fn understanding(&self, language: LanguageId) -> u8 {
        if self.has(language, UNDERSTOOD) {
            FULL
        } else {
            self.partial_understanding(language)
        }
    }

    /// `can_speak_language` with the body's tongue.
    pub fn can_speak(
        &self,
        language: LanguageId,
        tongue: Option<&Tongue>,
        tongueless: bool,
    ) -> bool {
        (self.omnitongue || could_speak(tongue, tongueless, language)) && self.has(language, SPOKEN)
    }

    /// `get_selected_language`: the selected language while it can be spoken, otherwise the
    /// speakable one of highest `default_priority` (the first of equals).
    pub fn selected_language(
        &self,
        tongue: Option<&Tongue>,
        tongueless: bool,
    ) -> Option<LanguageId> {
        if let Some(selected) = self.selected
            && self.can_speak(selected, tongue, tongueless)
        {
            return Some(selected);
        }
        let mut best: Option<LanguageId> = None;
        for &(language, _) in &self.spoken {
            let priority = language.def().default_priority;
            if best.is_none_or(|b| priority > b.def().default_priority)
                && self.can_speak(language, tongue, tongueless)
            {
                best = Some(language);
            }
        }
        best
    }

    /// The languages understood (`get_understood_languages`), each once.
    pub fn understood_languages(&self) -> Vec<LanguageId> {
        let mut out: Vec<LanguageId> = Vec::new();
        for &(l, _) in &self.understood {
            if !out.contains(&l) && !Self::blocked(&self.blocked_understanding, l) {
                out.push(l);
            }
        }
        out
    }

    /// The languages spoken, each once, before tongue limits.
    pub fn spoken_languages(&self) -> Vec<LanguageId> {
        let mut out: Vec<LanguageId> = Vec::new();
        for &(l, _) in &self.spoken {
            if !out.contains(&l) && !Self::blocked(&self.blocked_speaking, l) {
                out.push(l);
            }
        }
        out
    }

    /// `copy_languages(from, source_override)` (changeling absorb): what `from` understands and
    /// speaks, under `source`.
    pub fn copy_from(&mut self, from: &Self, source: LanguageSource) {
        for &(l, _) in &from.understood {
            self.grant(l, UNDERSTOOD, source);
        }
        for &(l, _) in &from.spoken {
            self.grant(l, SPOKEN, source);
        }
    }
}

/// Changes the holder of `body`, giving it tg's `atom_basic` one first when it has none.
pub fn edit_holder(world: &mut World, body: Entity, edit: impl FnOnce(&mut LanguageHolder)) {
    let Ok(mut entity) = world.get_entity_mut(body) else {
        return;
    };
    let mut holder = entity.get::<LanguageHolder>().cloned().unwrap_or_else(|| {
        LanguageHolder::from_preset(
            &Preset::of_path("/datum/language_holder/atom_basic").unwrap_or_default(),
            LanguageSource::Atom,
        )
    });
    edit(&mut holder);
    entity.insert(holder);
}

/// tg `atom_basic`: what a body without a [`LanguageHolder`] knows.
pub fn basic_understanding(language: LanguageId) -> u8 {
    match language {
        LanguageId::Common => FULL,
        l => LanguageId::Common
            .def()
            .mutual_understanding
            .iter()
            .find(|&&(m, _)| m == l)
            .map_or(0, |&(_, amount)| amount),
    }
}

/// The language a body speaks by default: its holder's selection, or Galactic Common for a body
/// without one (when it can speak it).
pub fn speaking(
    holder: Option<&LanguageHolder>,
    tongue: Option<&Tongue>,
    tongueless: bool,
) -> Option<LanguageId> {
    match holder {
        Some(h) => h.selected_language(tongue, tongueless),
        None => could_speak(tongue, tongueless, LanguageId::Common).then_some(LanguageId::Common),
    }
}

/// Whether a body can speak `language` (`can_speak_language`).
pub fn can_speak(
    holder: Option<&LanguageHolder>,
    tongue: Option<&Tongue>,
    tongueless: bool,
    language: LanguageId,
) -> bool {
    match holder {
        Some(h) => h.can_speak(language, tongue, tongueless),
        None => language == LanguageId::Common && could_speak(tongue, tongueless, language),
    }
}

/// How well a body understands `language`; ghosts and the dead understand everything (tg
/// observers have the universal holder).
pub fn understanding(holder: Option<&LanguageHolder>, dead: bool, language: LanguageId) -> u8 {
    if dead {
        return FULL;
    }
    holder.map_or_else(
        || basic_understanding(language),
        |h| h.understanding(language),
    )
}

/// `display_icon_type`: whether a listener sees which language it is (tg's chat icon), and
/// whether as a partial understanding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageTag {
    None,
    Full,
    Partial,
}

pub fn tag(language: LanguageId, understanding: u8) -> LanguageTag {
    let def = language.def();
    if understanding >= FULL {
        if def.has(HIDE_ICON_IF_UNDERSTOOD) {
            return LanguageTag::None;
        }
    } else {
        if understanding > 0 {
            return LanguageTag::Partial;
        }
        if def.has(HIDE_ICON_IF_NOT_UNDERSTOOD) {
            return LanguageTag::None;
        }
    }
    LanguageTag::Full
}

/// The chat text of a [`LanguageTag`]: `(Draconic) ` before the speaker's name.
pub fn tag_text(language: LanguageId, tag: LanguageTag) -> Option<String> {
    match tag {
        LanguageTag::None => None,
        LanguageTag::Full => Some(format!("({}) ", language.def().name)),
        LanguageTag::Partial => Some(format!("({}?) ", language.def().name)),
    }
}

/// `translate_language` without a language: what is heard of a body that speaks none.
pub const STRANGE_SOUND: &str = "издаёт странный звук.";

/// The round's scramble seed (tg's word and sentence caches, see the module docs).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrambleSeed(pub u64);

/// What a listener with `understanding` hears of `text` in `language`.
pub fn translate(seed: u64, language: Option<LanguageId>, understanding: u8, text: &str) -> String {
    match language {
        None => STRANGE_SOUND.to_owned(),
        Some(_) if understanding >= FULL => text.to_owned(),
        Some(l) => scramble_paragraph(seed, l, text, understanding),
    }
}

// ---------------------------------------------------------------------------------------------
// Scrambling

fn hash(seed: u64, parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ seed;
    for part in parts {
        for &b in *part {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// splitmix64.
struct Roll(u64);

impl Roll {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// `prob(percent)`.
    fn prob(&mut self, percent: f32) -> bool {
        ((self.next() % 10_000) as f32 / 100.0) < percent
    }

    /// `rand(low, high)`, inclusive.
    fn range(&mut self, low: i32, high: i32) -> i32 {
        if high <= low {
            return low;
        }
        low + (self.next() % (high - low + 1) as u64) as i32
    }

    fn pick<'a>(&mut self, list: &[&'a str]) -> &'a str {
        list[(self.next() % list.len() as u64) as usize]
    }
}

/// Nar'Sian's syllables: its list plus each pair of `base_syllables` joined by `'` (80%), `-`
/// (5%) or nothing (15%), or nothing when the pair is over 8 bytes (DM `length`). tg rolls the
/// joins once a round; here once per process.
fn narsie_syllables() -> &'static [String] {
    static SYLLABLES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    SYLLABLES.get_or_init(|| {
        let mut out: Vec<String> = defs::NARSIE_SYLLABLES
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        for a in defs::NARSIE_BASE {
            for b in defs::NARSIE_BASE {
                if a == b {
                    continue;
                }
                if a.len() + b.len() > 8 {
                    out.push(format!("{a}{b}"));
                    continue;
                }
                let mut roll = Roll(hash(0, &[a.as_bytes(), b.as_bytes()]));
                if roll.prob(80.0) {
                    out.push(format!("{a}'{b}"));
                } else if roll.prob(25.0) {
                    out.push(format!("{a}-{b}"));
                } else {
                    out.push(format!("{a}{b}"));
                }
            }
        }
        out
    })
}

fn pick_syllable(syllables: Syllables, roll: &mut Roll) -> Option<&'static str> {
    match syllables {
        Syllables::None => None,
        Syllables::Flat(list) => Some(roll.pick(list)),
        Syllables::Groups(groups) => {
            let group = groups[(roll.next() % groups.len() as u64) as usize];
            Some(roll.pick(group))
        }
        Syllables::Narsie => {
            let list = narsie_syllables();
            Some(list[(roll.next() % list.len() as u64) as usize].as_str())
        }
    }
}

/// `is_uppercase`: no lower-case letter. tg's regex only knows `[a-z]`, so every Cyrillic word
/// is "upper case" there and every scrambled Russian word is shouted; this checks any script.
fn is_uppercase(text: &str) -> bool {
    !text.chars().any(char::is_lowercase)
}

/// `STRIPPED_PUNCTUATION` of `strip_outer_punctuation`.
fn is_stripped_punctuation(c: char) -> bool {
    "!?.~;:,|+_`-".contains(c)
}

/// `strip_outer_punctuation`: up to 3 punctuation characters off each end, trimmed.
pub fn strip_outer_punctuation(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let start = chars
        .iter()
        .take(3)
        .take_while(|&&c| is_stripped_punctuation(c))
        .count();
    let rest = &chars[start..];
    let tail = rest
        .iter()
        .rev()
        .take(3)
        .take_while(|&&c| is_stripped_punctuation(c))
        .count();
    rest[..rest.len() - tail]
        .iter()
        .collect::<String>()
        .trim()
        .to_owned()
}

/// `find_last_punctuation`: the last (up to 3) of `!?.~;:,-` before trailing emphasis and
/// spaces.
pub fn find_last_punctuation(text: &str) -> String {
    let trimmed = text.trim_end_matches(|c: char| c.is_whitespace() || "|+_".contains(c));
    let chars: Vec<char> = trimmed.chars().collect();
    let run = chars
        .iter()
        .rev()
        .take(3)
        .take_while(|&&c| "!?.~;:,-".contains(c))
        .count();
    chars[chars.len() - run..].iter().collect()
}

/// `scramble_word`.
fn scramble_word(seed: u64, language: LanguageId, input: &str) -> String {
    let def = language.def();
    let lower = input.to_lowercase();
    let lang = [language as u8];
    let mut roll = Roll(hash(seed, &[&lang[..], lower.as_bytes()]));
    let word = match def.syllables {
        Syllables::None => {
            crate::speech::stars(input, 25.0, || (roll.next() % 10_000) as f32 / 100.0)
        }
        syllables => {
            let size = (input.chars().count() as i32
                + roll.range(def.additional_syllable_low, def.additional_syllable_high))
            .max(1) as usize;
            let (mut add_space, mut add_period) = (false, false);
            let mut word = String::new();
            while word.chars().count() < size {
                if add_period {
                    word.push_str(". ");
                } else if add_space {
                    word.push(' ');
                } else if !word.is_empty() && roll.prob(1.0) && !def.special_characters.is_empty() {
                    word.push_str(roll.pick(def.special_characters));
                }
                let next = pick_syllable(syllables, &mut roll).unwrap_or_default();
                if add_period {
                    word.push_str(&crate::speech::capitalize(next));
                } else {
                    word.push_str(next);
                }
                add_period = roll.prob(def.sentence_chance);
                add_space = roll.prob(def.space_chance);
            }
            word
        }
    };
    if is_uppercase(input) && input.chars().count() >= 2 {
        word.to_uppercase()
    } else {
        word
    }
}

/// `scramble_sentence`.
fn scramble_sentence(seed: u64, language: LanguageId, input: &str, understanding: u8) -> String {
    let def = language.def();
    let (lang, level) = ([language as u8], [understanding]);
    let mut roll = Roll(hash(seed, &[&lang[..], input.as_bytes(), &level[..]]));
    let mut words: Vec<(String, bool)> = Vec::new();
    for word in input.split(' ') {
        let base = strip_outer_punctuation(word);
        if understanding > 0 {
            // `commonness` of a word outside the common words list: -5%.
            let chance = f32::from(understanding) - 5.0;
            if roll.prob(chance) {
                words.push((word.to_owned(), false));
                continue;
            }
        }
        let scrambled = if language == LanguageId::Codespeak {
            scramble_word(seed, LanguageId::Common, &base)
        } else {
            scramble_word(seed, language, &base)
        };
        let translated = scrambled != base;
        words.push((scrambled, translated));
    }
    let Some(((first, _), rest)) = words.split_first() else {
        return String::new();
    };
    let mut sentence = crate::speech::capitalize(first);
    for (i, (word, translated)) in rest.iter().enumerate() {
        if !translated {
            sentence.push(' ');
            sentence.push_str(word);
            continue;
        }
        // `rest[i]` is `words[i + 1]`.
        let mut word = word.clone();
        if words[i].1 || roll.prob(def.between_word_space_chance) {
            sentence.push(' ');
        } else if roll.prob(def.between_word_sentence_chance) {
            sentence.push_str(". ");
            word = crate::speech::capitalize(&word);
        }
        sentence.push_str(&word);
    }
    if words.last().is_some_and(|w| w.1) {
        sentence.push_str(&find_last_punctuation(input));
    }
    sentence
}

/// `scramble_paragraph`: each sentence (up to and including `.`, `!` or `?`) scrambled, joined
/// by spaces; [`FULL`] understanding hears the text as is.
pub fn scramble_paragraph(
    seed: u64,
    language: LanguageId,
    text: &str,
    understanding: u8,
) -> String {
    if understanding >= FULL {
        return text.to_owned();
    }
    let mut sentences: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        let first = current.is_empty();
        current.push(c);
        if !first && matches!(c, '.' | '!' | '?') {
            sentences.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        sentences.push(current);
    }
    sentences
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| scramble_sentence(seed, language, s, understanding))
        .collect::<Vec<_>>()
        .join(" ")
}

mod defs;

#[cfg(test)]
mod tests {
    use super::*;
    use LanguageId::*;

    #[test]
    fn keys_are_unique_and_parse() {
        for l in LanguageId::ALL {
            assert_eq!(l.def().id, l, "{l:?}: def table order");
            if let Some(key) = l.def().key {
                assert_eq!(LanguageId::from_key(key), Some(l), "{key}");
            }
        }
        assert_eq!(LanguageId::from_key('o'), Some(Draconic));
        assert_eq!(LanguageId::from_key('O'), None);
        assert_eq!(LanguageId::from_key('7'), Some(Canilunzt));
        assert_eq!(
            LanguageId::from_path("/datum/language/spinwarder"),
            Some(Spinwarder)
        );
    }

    #[test]
    fn holder_follows_sources_blocks_and_priority() {
        let lizard = Preset::of_path("/datum/language_holder/lizard").unwrap();
        let mut holder = LanguageHolder::from_preset(&lizard, LanguageSource::Species);
        assert!(holder.has(Draconic, UNDERSTOOD) && holder.has(Common, SPOKEN));
        // Common (100) beats Draconic (90).
        assert_eq!(holder.selected_language(None, false), Some(Common));
        holder.selected = Some(Draconic);
        assert_eq!(holder.selected_language(None, false), Some(Draconic));
        // Without a tongue only the tongueless languages: Draconic has the flag, Moffic not.
        holder.grant(Moffic, ALL, LanguageSource::Mind);
        assert!(holder.can_speak(Draconic, None, true));
        assert!(!holder.can_speak(Moffic, None, true));
        // A human tongue cannot speak Voltaic.
        holder.grant(Voltaic, ALL, LanguageSource::Mind);
        let human = Tongue::of_path("/obj/item/organ/tongue");
        assert!(!holder.can_speak(Voltaic, Some(&human), false));
        let ethereal = Tongue::of_path("/obj/item/organ/tongue/ethereal/lustrous");
        assert!(holder.can_speak(Voltaic, Some(&ethereal), false));
        // Removing one source keeps the others.
        holder.grant(Draconic, ALL, LanguageSource::Mind);
        holder.unapply(&lizard, LanguageSource::Species);
        assert!(holder.has(Draconic, UNDERSTOOD) && !holder.has(Common, UNDERSTOOD));
        // Blocks win over grants.
        holder.block(Draconic, UNDERSTOOD, LanguageSource::Aphasia);
        assert!(!holder.has(Draconic, UNDERSTOOD) && holder.has(Draconic, SPOKEN));
        holder.unblock(Draconic, ALL, Some(LanguageSource::Aphasia));
        assert!(holder.has(Draconic, UNDERSTOOD));
    }

    #[test]
    fn mutual_understanding_comes_from_understood_languages() {
        let human = Preset::of_path("/datum/language_holder/human_basic").unwrap();
        let mut holder = LanguageHolder::from_preset(&human, LanguageSource::Species);
        assert_eq!(holder.understanding(Common), FULL);
        assert_eq!(holder.understanding(Beachbum), 33);
        assert_eq!(holder.understanding(Uncommon), 20);
        assert_eq!(holder.understanding(Draconic), 0);
        holder.grant_partial(Draconic, 66, LanguageSource::Atom);
        assert_eq!(holder.understanding(Draconic), 66);
        holder.block(Common, UNDERSTOOD, LanguageSource::Aphasia);
        assert_eq!(
            holder.understanding(Beachbum),
            0,
            "the source language is blocked"
        );
        assert_eq!(basic_understanding(Uncommon), 20);
        assert_eq!(understanding(None, true, Xenocommon), FULL);
    }

    #[test]
    fn tags_follow_display_icon_type() {
        assert_eq!(tag(Common, FULL), LanguageTag::None);
        assert_eq!(tag(Common, 0), LanguageTag::Full);
        assert_eq!(tag(Draconic, FULL), LanguageTag::Full);
        assert_eq!(tag(Uncommon, 20), LanguageTag::Partial);
        assert_eq!(tag(Codespeak, 0), LanguageTag::None);
        assert_eq!(
            tag_text(Draconic, LanguageTag::Full).unwrap(),
            "(Draconic) "
        );
    }

    #[test]
    fn punctuation_helpers_follow_tg() {
        assert_eq!(strip_outer_punctuation("...привет!!!"), "привет");
        assert_eq!(strip_outer_punctuation("ну,"), "ну");
        assert_eq!(find_last_punctuation("Что?!"), "?!");
        assert_eq!(find_last_punctuation("Стой!!!! "), "!!!");
        assert_eq!(find_last_punctuation("ну"), "");
    }

    #[test]
    fn scrambling_is_stable_and_replaces_words() {
        let text = "Привет, как дела? Всё хорошо.";
        let a = scramble_paragraph(7, Draconic, text, 0);
        let b = scramble_paragraph(7, Draconic, text, 0);
        assert_eq!(a, b, "one scramble per round seed");
        assert_ne!(a, text);
        assert!(!a.contains("Привет") && !a.contains("дела"));
        assert!(
            a.ends_with('.'),
            "punctuation of the last word is kept: {a}"
        );
        // The same word scrambles the same in another sentence (the word cache).
        let w1 = scramble_word(7, Draconic, "дела");
        assert_eq!(w1, scramble_word(7, Draconic, "Дела"));
        assert_eq!(scramble_paragraph(7, Draconic, text, FULL), text);
        // Words come from the syllables.
        let syllables = match Machine.def().syllables {
            Syllables::Flat(list) => list,
            _ => unreachable!(),
        };
        let beeps = scramble_word(1, Machine, "абв");
        assert!(syllables.iter().any(|s| beeps.starts_with(s)), "{beeps}");
        // An upper-case word is shouted (any script).
        let shout = scramble_word(3, Draconic, "СТОЙ");
        assert_eq!(shout, shout.to_uppercase());
        // No syllables: stars.
        assert!(!scramble_word(3, Codespeak, "секрет").is_empty());
        assert!(!narsie_syllables().is_empty());
    }

    #[test]
    fn no_language_is_a_strange_sound() {
        assert_eq!(translate(0, None, FULL, "привет"), STRANGE_SOUND);
        assert_eq!(translate(0, Some(Common), FULL, "привет"), "привет");
    }
}
