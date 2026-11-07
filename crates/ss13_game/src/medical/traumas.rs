//! Brain traumas (server), after tg `code/datums/brain_damage` (`brain_trauma.dm`, `mild.dm`,
//! `severe.dm`), the brain's trauma procs (`mob/living/brain/brain_item.dm`: `can_gain_trauma`,
//! `brain_gain_trauma`, `gain_trauma_type`, `cure_trauma_type`, `cure_all_traumas`,
//! `roll_for_brain_trauma`) and the carbon wrappers (`carbon/status_procs.dm`).
//!
//! A trauma lives in the brain ([`Traumas`] on the brain organ) and goes with it: a removed
//! brain keeps them, the body loses their effects, an inserted brain brings them. Every hit of
//! brain damage above [`organs::BRAIN_DAMAGE_MILD`] rolls for a mild trauma (chance = the
//! damage, +1% per point past the threshold) and, from [`organs::BRAIN_DAMAGE_SEVERE`], for a
//! severe or (20%) special one; natural gains stop at the tier limits (3 basic, 2 surgery, ...).
//! A trauma's [`Resilience`] decides what cures it: neurine and brain surgery the basic ones,
//! brain surgery the surgery ones, lobotomy the deep-rooted ones, nothing normal the rest.
//!
//! What the body gets is derived from the traumas ([`refresh`]): speech impairments for
//! `ss13_chat` ([`ss13_chat::speech::Impediments`]), [`Pacifist`] for melee and guns, paralyzed
//! limbs ([`super::wounds::Limb::paralyzed`]: legs count as disabled, a paralyzed hand drops its
//! item and holds nothing). The rest acts every `Life()` ([`life`]).
//!
//! Ported traumas: mild — stuttering, dumbness, speech impediment, concussion, anosognosia
//! (`healthy`), muscle weakness, nervous cough, possessiveness, achromatopsia, hallucinations
//! (`crate::hallucination`, Gibbering; mindbreaker holds them off), phobias
//! ([`super::fear`]); severe — mute, paralysis, pacifism, kleptomania, aphasia
//! (`ss13_chat::language`), cortical blindness (both read by `crate::vision`), narcolepsy (and
//! its permanent kind), monophobia ([`super::fear`]), dyslexia ([`illiterate`], papers), split
//! personality ([`super::split_personality`]); special — imaginary friend
//! ([`super::imaginary_friend`]), obsession ([`super::obsession`], [`gain_obsessed`]); hypnosis
//! ([`gain_hypnosis`]); the other special and the magic ones in [`super::special_traumas`].
//! Random gains pick among all of tg's types
//! ([`TG_POOL`]): an unported pick gives nothing, so each ported trauma comes as often as in tg.
// shortcut: no muscle spasms (no `/datum/status_effect/spasms`), expressive aphasia (needs the
// common words list), mind echo (needs per-hearer speech rewriting), hypnotic stupor and
// triggers (no trance), discoordination (no `TRAIT_DISCOORDINATED_TOOL_USER` gates),
// achromatopsia's `TRAIT_COLORBLIND` (wire colours), psychotic brawling, primal instincts and
// fire-axe adoration; no forced brain damage lines of dumbness (`traumas.json` is not converted), no
// fake health HUD, mood events or `TRAIT_DUMB`; kleptomania ignores the damage cooldown; an
// inserted brain applies its traumas without their gain messages; no head-wound
// (`TRAUMA_RESILIENCE_WOUND`) traumas, no `TRAIT_SPECIAL_TRAUMA_BOOST`; no admin-only phobia
// subtypes or uncapped hallucinations. Add each with the system it needs.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmWeight};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::organs::{self, OrganOf, Slot};
use super::wounds::{Limbs, spt_prob};
use crate::combat::CombatRng;
use crate::health::{BodyPosition, Health, Stat};
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::server::PlayerMob;
use crate::status_effects::Incap;
use crate::tools::tell;
use crate::zone::BodyZone;

/// `TRAUMA_RESILIENCE_*`: what cures a trauma (a cure of a level takes every lower one too).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Resilience {
    /// Neurine, brain surgery.
    Basic = 1,
    /// Brain surgery.
    Surgery,
    Lobotomy,
    /// Healing the head wound.
    Wound,
    Magic,
    Absolute,
}

impl Resilience {
    /// `TRAUMA_LIMIT_*`: natural gains stop at this many of one tier.
    fn limit(self) -> usize {
        match self {
            Self::Basic | Self::Lobotomy | Self::Magic => 3,
            Self::Surgery | Self::Wound => 2,
            Self::Absolute => usize::MAX,
        }
    }

    /// The scanner's tier word (`get_status_appendix`).
    fn scan_word(self) -> &'static str {
        match self {
            Self::Basic => "Mild",
            Self::Surgery => "Severe",
            Self::Lobotomy => "Deep-rooted",
            Self::Wound => "Fracture-derived",
            Self::Magic => "Persistent",
            Self::Absolute => "Permanent",
        }
    }
}

/// `BRAIN_TRAUMA_MILD`/`SEVERE`/`SPECIAL`/`MAGIC`: the subtype trees random gains pick from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Mild,
    Severe,
    Special,
    Magic,
}

/// `/datum/brain_trauma/severe/paralysis` `paralysis_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Paralysis {
    Full,
    Left,
    Right,
    Arms,
    Legs,
    RArm,
    LArm,
    RLeg,
    LLeg,
}

impl Paralysis {
    pub const ALL: [Self; 9] = [
        Self::Full,
        Self::Left,
        Self::Right,
        Self::Arms,
        Self::Legs,
        Self::RArm,
        Self::LArm,
        Self::RLeg,
        Self::LLeg,
    ];

    /// The limbs of its `paralysis_traits`.
    pub fn zones(self) -> &'static [BodyZone] {
        use BodyZone::{LArm, LLeg, RArm, RLeg};
        match self {
            Self::Full => &[LArm, RArm, LLeg, RLeg],
            Self::Left => &[LArm, LLeg],
            Self::Right => &[RArm, RLeg],
            Self::Arms => &[LArm, RArm],
            Self::Legs => &[LLeg, RLeg],
            Self::RArm => &[RArm],
            Self::LArm => &[LArm],
            Self::RLeg => &[RLeg],
            Self::LLeg => &[LLeg],
        }
    }

    fn subject(self) -> &'static str {
        match self {
            Self::Full => "ваше тело",
            Self::Left => "левую сторону вашего тела",
            Self::Right => "правую сторону вашего тела",
            Self::Arms => "ваши руки",
            Self::Legs => "ваши ноги",
            Self::RArm => "вашу правую руку",
            Self::LArm => "вашу левую руку",
            Self::RLeg => "вашу правую ногу",
            Self::LLeg => "вашу левую ногу",
        }
    }
}

/// The ported trauma types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Stuttering,
    Dumbness,
    SpeechImpediment,
    Concussion,
    Healthy,
    MuscleWeakness,
    NervousCough,
    Possessive,
    Mute,
    Paralysis(Paralysis),
    Pacifism,
    Kleptomaniac,
    ColorBlindness,
    Blindness,
    Aphasia,
    /// `/datum/brain_trauma/mild/hallucinations`.
    Hallucinations,
    /// `/datum/brain_trauma/mild/phobia` of a phobia (an index into
    /// [`super::phobia_data::PHOBIAS`]).
    Phobia(usize),
    /// `/datum/brain_trauma/severe/narcolepsy` (`permanent`: its `/permanent` subtype).
    Narcolepsy {
        permanent: bool,
    },
    Monophobia,
    Dyslexia,
    /// `/datum/brain_trauma/severe/split_personality` ([`super::split_personality`]).
    SplitPersonality,
    /// `/datum/brain_trauma/special/imaginary_friend` ([`super::imaginary_friend`]).
    ImaginaryFriend,
    /// `/datum/brain_trauma/hypnosis`: the phrase is the trauma's ([`Trauma::phrase`],
    /// [`gain_hypnosis`]).
    Hypnosis,
    /// `/datum/brain_trauma/special/obsessed` ([`super::obsession`], [`gain_obsessed`]).
    Obsessed,
    // `special.dm` and `magic.dm` ([`super::special_traumas`]).
    Godwoken,
    BluespaceProphet,
    QuantumAlignment,
    Tenacity,
    DeathWhispers,
    ExistentialCrisis,
    /// `/datum/brain_trauma/special/beepsky` (never random, hidden from scanners).
    Beepsky,
    /// `/datum/brain_trauma/special/ptsd` (the veteran advisor's).
    Ptsd,
    Lumiphobia,
    Poltergeist,
    Antimagic,
    /// `/datum/brain_trauma/magic/stalker` (`cosmic`: the heretic's star gazer).
    Stalker {
        cosmic: bool,
    },
}

/// Quantum alignment's entangled target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Link {
    Entity(Entity),
    Turf(TilePos),
}

/// The state of the special and magic traumas ([`super::special_traumas`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extra {
    /// A cooldown's end (game time): portals, crises, snapbacks, hallucinations, warnings.
    pub next_at: f64,
    /// The end of an episode: death whispers, a crisis, a snapback under way.
    pub until: Option<f64>,
    /// A phantom chasing the owner: its tile and the image its owner sees.
    pub phantom: Option<(TilePos, Option<Entity>)>,
    pub link: Option<Link>,
    /// An existential crisis' veil.
    pub entity: Option<Entity>,
    /// The stalker is within 8 tiles.
    pub close: bool,
}

/// The static part of a trauma type (tg vars).
#[derive(Clone, Copy, Debug)]
pub struct Def {
    pub path: &'static str,
    pub name: &'static str,
    pub scan_desc: &'static str,
    pub gain: (Span, &'static str),
    pub lose: (Span, &'static str),
    pub category: Category,
    pub resilience: Resilience,
}

impl Kind {
    pub fn def(self) -> Def {
        use Span::{Danger, Notice, Warning};
        let mild = |path, name, scan_desc, gain, lose| Def {
            path,
            name,
            scan_desc,
            gain,
            lose,
            category: Category::Mild,
            resilience: Resilience::Basic,
        };
        let severe = |path, name, scan_desc, gain, lose| Def {
            path,
            name,
            scan_desc,
            gain,
            lose,
            category: Category::Severe,
            resilience: Resilience::Surgery,
        };
        let special = |path, name, scan_desc, gain, lose| Def {
            path,
            name,
            scan_desc,
            gain,
            lose,
            category: Category::Special,
            resilience: Resilience::Basic,
        };
        let magic = |path, name, scan_desc, gain, lose| Def {
            path,
            name,
            scan_desc,
            gain,
            lose,
            category: Category::Magic,
            resilience: Resilience::Lobotomy,
        };
        match self {
            Self::Stuttering => mild(
                "/datum/brain_trauma/mild/stuttering",
                "Заикание",
                "пониженная координация речи",
                (Warning, "Говорить четко становится все труднее."),
                (Notice, "Вы чувствуете, что контролируете свою речь."),
            ),
            Self::Dumbness => mild(
                "/datum/brain_trauma/mild/dumbness",
                "Отупение",
                "снижение мозговой активности",
                (Warning, "Вы чувствуете себя глупее."),
                (Notice, "Вы снова чувствуете себя умным."),
            ),
            Self::SpeechImpediment => mild(
                "/datum/brain_trauma/mild/speech_impediment",
                "Дефект речи",
                "коммуникативное расстройство",
                (
                    Danger,
                    "Кажется, вы не можете сформулировать ни одной связной мысли!",
                ),
                (Danger, "Ваш разум становится более ясным."),
            ),
            Self::Concussion => mild(
                "/datum/brain_trauma/mild/concussion",
                "Сотрясение",
                "сотрясение",
                (Warning, "У вас болит голова!"),
                // tg's line carries a stray Latin "T".
                (Notice, "Давление в вашей голове начинает ослабевать."),
            ),
            Self::Healthy => mild(
                "/datum/brain_trauma/mild/healthy",
                "Анозогнозия",
                "дефицит самосознания",
                (Notice, "Вы чувствуете себя прекрасно!"),
                (Warning, "Вы больше не чувствуете себя совершенно здоровым."),
            ),
            Self::MuscleWeakness => mild(
                "/datum/brain_trauma/mild/muscle_weakness",
                "Мышечная слабость",
                "слабый сигнал двигательного нерва",
                (Warning, "Ваши мышцы ощущаются странно слабыми."),
                (Notice, "Вы снова чувствуете контроль над своими мышцами."),
            ),
            Self::NervousCough => mild(
                "/datum/brain_trauma/mild/nervous_cough",
                "Нервный кашель",
                "нервный кашель",
                (Warning, "У вас постоянно чешется горло..."),
                (Notice, "Ваше горло перестает чесаться."),
            ),
            Self::Possessive => mild(
                "/datum/brain_trauma/mild/possessive",
                "Собственничество",
                "собственничество",
                (Warning, "Вы начинаете беспокоиться о своих вещах."),
                (Notice, "Вы меньше беспокоитесь о своих вещах."),
            ),
            Self::Mute => severe(
                "/datum/brain_trauma/severe/mute",
                "Немота",
                "обширное повреждение речевого центра мозга",
                (Warning, "Вы разучились говорить!"),
                (Notice, "Вы вдруг вспоминаете, как говорить."),
            ),
            // The lines name the paralysed part ([`gain_text`]).
            Self::Paralysis(_) => severe(
                "/datum/brain_trauma/severe/paralysis",
                "Паралич",
                "церебральный паралич",
                (Warning, ""),
                (Notice, ""),
            ),
            Self::Pacifism => severe(
                "/datum/brain_trauma/severe/pacifism",
                "Травматическое ненасилие",
                "синдром пацифизма",
                (Notice, "Вы чувствуете себя странно умиротворенным."),
                (
                    Notice,
                    "Вы больше не чувствуете себя обязанным не причинять вреда.",
                ),
            ),
            Self::ColorBlindness => mild(
                "/datum/brain_trauma/mild/color_blindness",
                "Ахроматопсия",
                "цветовая слепота",
                (Warning, "Кажется, что мир вокруг вас теряет свои краски."),
                (Notice, "Мир снова кажется ярким и красочным."),
            ),
            Self::Blindness => severe(
                "/datum/brain_trauma/severe/blindness",
                "Корковая слепота",
                "обширное повреждение затылочной доли мозга",
                (Warning, "Вы не можете видеть!"),
                (Notice, "Ваше зрение возвращается."),
            ),
            Self::Aphasia => severe(
                "/datum/brain_trauma/severe/aphasia",
                "Афазия",
                "обширное повреждение языкового центра мозга",
                (Warning, "У вас проблемы с формированием слов в голове..."),
                (Notice, "Вы вдруг вспоминаете, как работают языки."),
            ),
            Self::Hallucinations => mild(
                "/datum/brain_trauma/mild/hallucinations",
                "Галлюцинации",
                "шизофрения",
                (
                    Warning,
                    "Вы чувствуете, что ваша связь с реальностью ослабевает...",
                ),
                (Notice, "Вы чувствуете себя более приземленным."),
            ),
            // The lines name the phobia ([`gain_text`]).
            Self::Phobia(_) => mild(
                "/datum/brain_trauma/mild/phobia",
                "Фобия",
                "фобия",
                (Warning, ""),
                (Notice, ""),
            ),
            Self::Narcolepsy { permanent } => Def {
                path: if permanent {
                    "/datum/brain_trauma/severe/narcolepsy/permanent"
                } else {
                    "/datum/brain_trauma/severe/narcolepsy"
                },
                scan_desc: if permanent {
                    "хроническая нарколепсия"
                } else {
                    "травматическая нарколепсия"
                },
                ..severe(
                    "",
                    "Нарколепсия",
                    "",
                    (Warning, "У вас постоянное чувство сонливости..."),
                    (
                        Notice,
                        "Вы снова чувствуете себя бодрым и осознающим происходящее.",
                    ),
                )
            },
            Self::Monophobia => severe(
                "/datum/brain_trauma/severe/monophobia",
                "Монофобия",
                "монофобия",
                (Warning, "Вы чувствуете себя по-настоящему одиноким..."),
                (
                    Notice,
                    "Вам кажется, что вы могли бы чувствовать себя в безопасности в одиночестве.",
                ),
            ),
            Self::Dyslexia => severe(
                "/datum/brain_trauma/severe/dyslexia",
                "Дислексия",
                "дислексия",
                (Warning, "У вас проблемы с чтением или письмом..."),
                (Notice, "Вы вдруг вспоминаете, как читать и писать."),
            ),
            Self::SplitPersonality => severe(
                "/datum/brain_trauma/severe/split_personality",
                "Split Personality",
                "complete lobe separation",
                (Warning, "You feel like your mind was split in two."),
                (Notice, "You feel alone again."),
            ),
            Self::ImaginaryFriend => Def {
                category: Category::Special,
                resilience: Resilience::Basic,
                ..mild(
                    "/datum/brain_trauma/special/imaginary_friend",
                    "Воображаемый друг",
                    "частичная шизофрения",
                    (
                        Notice,
                        "По какой-то причине вы чувствуете себя в хорошей компании.",
                    ),
                    (Warning, "Вы снова чувствуете себя одиноким."),
                )
            },
            // The lines name the phrase ([`gain_hypnosis`], [`lose_text`]).
            Self::Hypnosis => Def {
                category: Category::Special,
                ..severe(
                    "/datum/brain_trauma/hypnosis",
                    "Гипноз",
                    "повторяющийся мыслительный паттерн",
                    (Notice, ""),
                    (Notice, ""),
                )
            },
            Self::Obsessed => Def {
                category: Category::Special,
                resilience: Resilience::Lobotomy,
                ..mild(
                    "/datum/brain_trauma/special/obsessed",
                    "Психотическая шизофрения",
                    "психотические шизофренические бредовые идеи",
                    (
                        Warning,
                        "Вы слышите отвратительный, скрипучий голос у себя в голове. Он требует от вас выполнения одного небольшого задания...",
                    ),
                    (Warning, "Голоса в вашей голове замолкают.."),
                )
            },
            Self::Godwoken => special(
                "/datum/brain_trauma/special/godwoken",
                "Синдром пробужденного Бога",
                "божественная иллюзия",
                (Notice, "Вы чувствуете высшую силу внутри своего разума..."),
                (
                    Warning,
                    "Божественное присутствие покидает вашу голову, потеряв интерес.",
                ),
            ),
            Self::BluespaceProphet => special(
                "/datum/brain_trauma/special/bluespace_prophet",
                "Блюспейс пророчество",
                "bluespace attunement",
                (
                    Notice,
                    "Вы чувствуете, как блюспейс пульсирует вокруг тебя...",
                ),
                (Warning, "Слабая пульсация блюспейса сменяется тишиной."),
            ),
            Self::QuantumAlignment => special(
                "/datum/brain_trauma/special/quantum_alignment",
                "Квантовое выравнивание",
                "квантовое выравнивание",
                (
                    Notice,
                    "Вы чувствуете слабую связь со всем, что вас окружает...",
                ),
                (
                    Warning,
                    "Вы больше не чувствуете связи со своим окружением.",
                ),
            ),
            Self::Tenacity => special(
                "/datum/brain_trauma/special/tenacity",
                "Упорство",
                "травматическая невропатия",
                (Warning, "Вы внезапно перестаете чувствовать боль."),
                (Warning, "Вы понимаете, что снова можете чувствовать боль."),
            ),
            Self::DeathWhispers => special(
                "/datum/brain_trauma/special/death_whispers",
                "Функциональный церебральный некроз",
                "хронический функциональный некроз",
                (Warning, "Вы чувствуете себя мертвым внутри."),
                (Notice, "Вы снова чувствуете себя живым."),
            ),
            Self::ExistentialCrisis => special(
                "/datum/brain_trauma/special/existential_crisis",
                "Экзистенциальный кризис",
                "экзистенциальный кризис",
                (Warning, "Вы чувствуете себя менее реальным."),
                (Notice, "Вы снова чувствуете себя более значимым."),
            ),
            Self::Beepsky => special(
                "/datum/brain_trauma/special/beepsky",
                "Преступник",
                "преступный склад ума",
                (Warning, "Правосудие придет за вами."),
                (
                    Notice,
                    "Вы были освобождены от ответственности за ваши преступления.",
                ),
            ),
            Self::Ptsd => Def {
                resilience: Resilience::Absolute,
                ..special(
                    "/datum/brain_trauma/special/ptsd",
                    "Боевое ПТСР",
                    "ПТСР",
                    (
                        Warning,
                        "Вы возвращаетесь в хаос прошлого! Взрывы! Стрельба! Эмоции покинули строй!",
                    ),
                    (
                        Notice,
                        "Вы чувствуете, как исчезают воспоминания о прошлом, как возвращаются ваши эмоции и проясняется разум.",
                    ),
                )
            },
            Self::Lumiphobia => magic(
                "/datum/brain_trauma/magic/lumiphobia",
                "Люмифобия",
                "светочувствительность",
                (Warning, "Вы чувствуете тягу к темноте."),
                (Notice, "Свет больше не беспокоит вас."),
            ),
            Self::Poltergeist => magic(
                "/datum/brain_trauma/magic/poltergeist",
                "Полтергейст",
                "паранормальная активность",
                (
                    Warning,
                    "Вы чувствуете ненавистное присутствие рядом с собой.",
                ),
                (
                    Notice,
                    "Вы чувствуете, как ненавистное присутствие исчезает.",
                ),
            ),
            Self::Antimagic => magic(
                "/datum/brain_trauma/magic/antimagic",
                "Атаумазия",
                "таумическая пустота",
                (Notice, "Вы понимаете, что магия не может быть реальной."),
                (Notice, "Вы понимаете, что магия может быть реальной."),
            ),
            Self::Stalker { cosmic } => Def {
                path: if cosmic {
                    "/datum/brain_trauma/magic/stalker/cosmic"
                } else {
                    "/datum/brain_trauma/magic/stalker"
                },
                ..magic(
                    "",
                    "Преследующий призрак",
                    "экстрасенсорная паранойя",
                    (
                        Warning,
                        "Вы чувствуете себя так, словно что-то хочет убить вас...",
                    ),
                    (
                        Notice,
                        "Вы больше не чувствуете, что кто-то смотрит вам в спину.",
                    ),
                )
            },
            Self::Kleptomaniac => severe(
                "/datum/brain_trauma/severe/kleptomaniac",
                "Клептомания",
                "клептомания",
                (
                    Warning,
                    "Вы чувствуете внезапное желание взять это. Конечно, никто не заметит.",
                ),
                (Notice, "Вы больше не чувствуете желания брать чужие вещи."),
            ),
        }
    }

    /// tg `istype(trauma, type)` between two of the same tree: paralysis types are one type.
    fn same_type(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }

    /// `scan_desc` (a phobia names what it fears).
    fn scan_desc(self) -> String {
        match self {
            Self::Phobia(i) => format!("фобия {}", phobia_label(i)),
            _ => self.def().scan_desc.to_owned(),
        }
    }

    fn gain_text(self) -> (Span, String) {
        match self {
            Self::Paralysis(p) => (
                Span::Warning,
                format!("Вы больше не чувствуете {}!", p.subject()),
            ),
            Self::Phobia(i) => (
                Span::Warning,
                format!(
                    "Вы начинаете находить {} очень нервирующим...",
                    phobia_label(i)
                ),
            ),
            _ => (self.def().gain.0, self.def().gain.1.to_owned()),
        }
    }

    fn lose_text(self) -> (Span, String) {
        match self {
            Self::Paralysis(p) => (
                Span::Notice,
                format!("Вы снова чувствуете {}!", p.subject()),
            ),
            Self::Phobia(i) => (
                Span::Notice,
                format!("Вы больше не боитесь {}.", phobia_label(i)),
            ),
            _ => (self.def().lose.0, self.def().lose.1.to_owned()),
        }
    }
}

/// The Russian `phobia_type` of phobia `i`.
fn phobia_label(i: usize) -> &'static str {
    super::phobia_data::PHOBIAS.get(i).map_or("", |p| p.label)
}

/// tg's randomly gainable trauma types (`subtypesof(category)` with `random_gain` and
/// `can_gain`, the abstract roots out): type, the ported kind (a paralysis of a placeholder
/// kind), category, `initial(resilience)`. Random gains pick among all of them, so each ported
/// trauma comes as often as in tg.
// shortcut: a pick of an unported type gives no trauma; add each type's kind as it is ported.
pub const TG_POOL: [(&str, Option<Kind>, Category, Resilience); 41] = {
    use Category::{Magic, Mild, Severe, Special};
    use Resilience::{Basic, Lobotomy, Surgery};
    [
        (
            "/datum/brain_trauma/mild/color_blindness",
            Some(Kind::ColorBlindness),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/concussion",
            Some(Kind::Concussion),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/dumbness",
            Some(Kind::Dumbness),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/expressive_aphasia",
            None,
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/hallucinations",
            Some(Kind::Hallucinations),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/healthy",
            Some(Kind::Healthy),
            Mild,
            Basic,
        ),
        ("/datum/brain_trauma/mild/mind_echo", None, Mild, Basic),
        ("/datum/brain_trauma/mild/muscle_spasms", None, Mild, Basic),
        (
            "/datum/brain_trauma/mild/muscle_weakness",
            Some(Kind::MuscleWeakness),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/nervous_cough",
            Some(Kind::NervousCough),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/phobia",
            Some(Kind::Phobia(0)),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/possessive",
            Some(Kind::Possessive),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/speech_impediment",
            Some(Kind::SpeechImpediment),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/stuttering",
            Some(Kind::Stuttering),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/severe/aphasia",
            Some(Kind::Aphasia),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/blindness",
            Some(Kind::Blindness),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/discoordination",
            None,
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/dyslexia",
            Some(Kind::Dyslexia),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/hypnotic_stupor",
            None,
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/kleptomaniac",
            Some(Kind::Kleptomaniac),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/monophobia",
            Some(Kind::Monophobia),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/mute",
            Some(Kind::Mute),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/narcolepsy",
            Some(Kind::Narcolepsy { permanent: false }),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/narcolepsy/permanent",
            Some(Kind::Narcolepsy { permanent: true }),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/pacifism",
            Some(Kind::Pacifism),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/paralysis",
            Some(Kind::Paralysis(Paralysis::Full)),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/split_personality",
            Some(Kind::SplitPersonality),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/special/bluespace_prophet",
            Some(Kind::BluespaceProphet),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/death_whispers",
            Some(Kind::DeathWhispers),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/existential_crisis",
            Some(Kind::ExistentialCrisis),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/godwoken",
            Some(Kind::Godwoken),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/imaginary_friend",
            Some(Kind::ImaginaryFriend),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/primal_instincts",
            None,
            Special,
            Surgery,
        ),
        (
            "/datum/brain_trauma/special/psychotic_brawling",
            None,
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/psychotic_brawling/bath_salts",
            None,
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/quantum_alignment",
            Some(Kind::QuantumAlignment),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/tenacity",
            Some(Kind::Tenacity),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/magic/antimagic",
            Some(Kind::Antimagic),
            Magic,
            Lobotomy,
        ),
        (
            "/datum/brain_trauma/magic/lumiphobia",
            Some(Kind::Lumiphobia),
            Magic,
            Lobotomy,
        ),
        (
            "/datum/brain_trauma/magic/poltergeist",
            Some(Kind::Poltergeist),
            Magic,
            Lobotomy,
        ),
        (
            "/datum/brain_trauma/magic/stalker",
            Some(Kind::Stalker { cosmic: false }),
            Magic,
            Lobotomy,
        ),
    ]
};

/// One trauma in a brain.
#[derive(Clone, Debug, PartialEq)]
pub struct Trauma {
    pub kind: Kind,
    pub resilience: Resilience,
    /// Possessiveness: the items held on to and when each is let go (game time).
    possessed: Vec<(Entity, f64)>,
    /// Kleptomania's `steal_cd` (game time).
    steal_at: f64,
    /// Hallucinations: held off by mindbreaker (`TRAIT_RDS_SUPPRESSED`), Gibbering taken away.
    suppressed: bool,
    /// Hypnosis: `hypnotic_phrase`.
    pub phrase: String,
    /// Obsession: the obsession and its counters.
    pub obsession: Option<super::obsession::Obsession>,
    /// The special and magic traumas' state.
    pub extra: Extra,
}

/// A brain's traumas (tg `traumas`), on the brain organ.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Traumas(pub Vec<Trauma>);

/// tg `TRAIT_PACIFISM` from a trauma: no harmful attacks or lethal shots.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Pacifist;

fn owner(world: &World, brain: Entity) -> Option<Entity> {
    world.get::<OrganOf>(brain).map(|o| o.0)
}

fn traumas(world: &World, brain: Entity) -> &[Trauma] {
    world.get::<Traumas>(brain).map_or(&[], |t| t.0.as_slice())
}

fn tell_owner(world: &mut World, brain: Entity, (span, text): (Span, String)) {
    if text.is_empty() {
        return;
    }
    if let Some(client) = owner(world, brain)
        .and_then(|mob| world.get::<PlayerMob>(mob))
        .map(|p| p.client)
    {
        tell(world, client, span, text);
    }
}

/// The traumas of `mob`'s brain.
pub fn of_mob(world: &World, mob: Entity) -> Vec<Trauma> {
    organs::organ(world, mob, Slot::Brain)
        .map(|brain| traumas(world, brain).to_vec())
        .unwrap_or_default()
}

/// tg `has_trauma_type(type, resilience)` on a mob.
pub fn has(world: &World, mob: Entity, kind: Kind) -> bool {
    of_mob(world, mob).iter().any(|t| t.kind.same_type(kind))
}

/// tg `can_gain_trauma(type, resilience, natural_gain)`: not twice of one type, and a natural
/// gain only below its tier's limit.
pub fn can_gain(
    traumas: &[Trauma],
    kind: Kind,
    resilience: Option<Resilience>,
    natural: bool,
) -> bool {
    let resilience = resilience.unwrap_or(kind.def().resilience);
    if traumas.iter().any(|t| t.kind.same_type(kind)) {
        return false;
    }
    let tier = traumas
        .iter()
        .filter(|t| t.resilience == resilience)
        .count();
    !natural || tier < resilience.limit()
}

/// tg `brain_gain_trauma(type, resilience)`: the trauma goes into `brain`, its owner (if any)
/// gets its effects and the gain line. False if it cannot be gained.
pub fn gain(world: &mut World, brain: Entity, kind: Kind, resilience: Option<Resilience>) -> bool {
    if !can_gain(traumas(world, brain), kind, resilience, false) {
        return false;
    }
    let resilience = resilience.unwrap_or(kind.def().resilience);
    // An imprinted cortex's `COMSIG_CARBON_BLOCK_TRAUMA`.
    if owner(world, brain).is_some_and(|mob| super::bioware::blocks_trauma(world, mob, resilience))
    {
        return false;
    }
    // `on_gain` returning FALSE: a split personality needs a living body with a player.
    if kind == Kind::SplitPersonality
        && owner(world, brain).is_some_and(|mob| !super::split_personality::can_take(world, mob))
    {
        return false;
    }
    // An imaginary friend: a living body with a player too.
    if kind == Kind::ImaginaryFriend
        && owner(world, brain).is_some_and(|mob| !super::imaginary_friend::can_take(world, mob))
    {
        return false;
    }
    // `/datum/brain_trauma/hypnosis/New(phrase)`: no phrase, no trauma ([`gain_hypnosis`]).
    if kind == Kind::Hypnosis {
        return false;
    }
    if kind == Kind::Obsessed {
        let mut rng = super::fork_rng(world);
        return gain_obsessed(world, brain, None, Some(resilience), &mut rng);
    }
    let mut trauma = Trauma {
        kind,
        resilience,
        possessed: Vec::new(),
        steal_at: 0.0,
        suppressed: false,
        phrase: String::new(),
        obsession: None,
        extra: Extra::default(),
    };
    if let Some(mob) = owner(world, brain) {
        let mut rng = super::fork_rng(world);
        super::special_traumas::on_gain(world, mob, &mut trauma, &mut rng);
    }
    let mut all = world.get::<Traumas>(brain).cloned().unwrap_or_default();
    all.0.push(trauma);
    world.entity_mut(brain).insert(all);
    if let Some(mob) = owner(world, brain) {
        info!("{mob} gained the brain trauma {}", kind.def().path);
        tell_owner(world, brain, kind.gain_text());
        refresh(world, mob);
    }
    true
}

/// tg `gain_trauma_type(category, resilience, natural_gain)`: a random gainable type of the
/// category from [`TG_POOL`] (a paralysis of a random kind). The gained type, if any: none
/// when the pick is not ported.
pub fn gain_type(
    world: &mut World,
    brain: Entity,
    category: Category,
    resilience: Option<Resilience>,
    natural: bool,
    rng: &mut CombatRng,
) -> Option<Kind> {
    let have = traumas(world, brain);
    let pool: Vec<Option<Kind>> = TG_POOL
        .into_iter()
        .filter(|&(_, kind, cat, initial)| {
            cat == category
                && match kind {
                    Some(kind) => can_gain(have, kind, resilience, natural),
                    // `can_gain_trauma` of a type that cannot be present: the tier limit only.
                    None => {
                        let tier = resilience.unwrap_or(initial);
                        !natural
                            || have.iter().filter(|t| t.resilience == tier).count() < tier.limit()
                    }
                }
        })
        .map(|(_, kind, _, _)| kind)
        .collect();
    if pool.is_empty() {
        return None;
    }
    let mut kind = pool[rng.range(0, pool.len() as u32 - 1) as usize]?;
    match kind {
        Kind::Paralysis(_) => kind = Kind::Paralysis(Paralysis::ALL[rng.range(0, 8) as usize]),
        // `pick(GLOB.phobia_types)`.
        Kind::Phobia(_) => {
            let random = &super::phobia_data::RANDOM;
            let key = random[rng.range(0, random.len() as u32 - 1) as usize];
            kind = Kind::Phobia(super::fear::phobia_index(key).unwrap_or(0));
        }
        _ => {}
    }
    gain(world, brain, kind, resilience).then_some(kind)
}

/// The `on_gain` lines of hypnosis, one picked.
const HYPNOSIS_LINES: [&str; 5] = [
    "В этом что-то есть... и, по какой-то причине, это кажется правильным. Вам кажется, что вы должны следовать этим словам.",
    "Эти слова продолжают звучать у вас в голове. Вы обнаруживаете, что совершенно очарованы ими.",
    "Вы чувствуете, как часть вашего сознания повторяет это снова и снова. Вам нужно следовать этим словам.",
    "Вы чувствуете, что ваши мысли сосредотачиваются на этой фразе... Кажется, вы не можете выбросить её из головы.",
    "Болит голова, но это все, о чем вы можете думать. Должно быть, это жизненно важно.",
];

/// `gain_trauma(/datum/brain_trauma/hypnosis, resilience, phrase)`: `brain` is hypnotized with
/// `phrase` (trimmed; nothing for an empty one, as `New` deletes itself). False if not gained.
// shortcut: no `/datum/antagonist/hypnotized` (its objective and antag HUD: the antagonists
// group), no hypnosis alert, no highlight of the phrase in what the owner hears (no
// per-hearer speech rewriting; `handle_hearing`).
pub fn gain_hypnosis(
    world: &mut World,
    brain: Entity,
    phrase: &str,
    resilience: Option<Resilience>,
    rng: &mut CombatRng,
) -> bool {
    let phrase = phrase.trim();
    let kind = Kind::Hypnosis;
    if phrase.is_empty() || !can_gain(traumas(world, brain), kind, resilience, false) {
        return false;
    }
    let resilience = resilience.unwrap_or(kind.def().resilience);
    if owner(world, brain).is_some_and(|mob| super::bioware::blocks_trauma(world, mob, resilience))
    {
        return false;
    }
    let mut all = world.get::<Traumas>(brain).cloned().unwrap_or_default();
    all.0.push(Trauma {
        kind,
        resilience,
        possessed: Vec::new(),
        steal_at: 0.0,
        suppressed: false,
        phrase: phrase.to_owned(),
        obsession: None,
        extra: Extra::default(),
    });
    world.entity_mut(brain).insert(all);
    let Some(mob) = owner(world, brain) else {
        return true;
    };
    info!("{mob} was hypnotized with the phrase '{phrase}'");
    let line = HYPNOSIS_LINES[rng.range(0, HYPNOSIS_LINES.len() as u32 - 1) as usize];
    tell_owner(world, brain, (Span::Hypnophrase, phrase.to_owned()));
    tell_owner(world, brain, (Span::Notice, line.to_owned()));
    tell_owner(
        world,
        brain,
        (
            Span::BoldWarning,
            "Вы были загипнотизированы этим предложением. Вы должны следовать этим словам. \
             Если это не четкий приказ, вы можете свободно интерпретировать, как именно действовать, \
             главное — вести себя так, будто эти слова являются для вас наивысшим приоритетом."
                .to_owned(),
        ),
    );
    refresh(world, mob);
    true
}

/// `gain_trauma(/datum/brain_trauma/special/obsessed, resilience, obsession)`: `brain` becomes
/// obsessed with `target`, or (none given) with [`super::obsession::find_obsession`]'s pick.
/// False if not gained (nobody to obsess over, as `on_gain` returning FALSE).
pub fn gain_obsessed(
    world: &mut World,
    brain: Entity,
    target: Option<Entity>,
    resilience: Option<Resilience>,
    rng: &mut CombatRng,
) -> bool {
    let kind = Kind::Obsessed;
    if !can_gain(traumas(world, brain), kind, resilience, false) {
        return false;
    }
    let resilience = resilience.unwrap_or(kind.def().resilience);
    let Some(mob) = owner(world, brain) else {
        return false;
    };
    if super::bioware::blocks_trauma(world, mob, resilience) {
        return false;
    }
    let Some(target) = target
        .filter(|&t| t != mob && world.get_entity(t).is_ok())
        .or_else(|| super::obsession::find_obsession(world, mob, &[], rng))
    else {
        return false;
    };
    let mut all = world.get::<Traumas>(brain).cloned().unwrap_or_default();
    all.0.push(Trauma {
        kind,
        resilience,
        possessed: Vec::new(),
        steal_at: 0.0,
        suppressed: false,
        phrase: String::new(),
        obsession: Some(super::obsession::Obsession::new(target)),
        extra: Extra::default(),
    });
    world.entity_mut(brain).insert(all);
    info!("{mob} has developed an obsession with {target}");
    tell_owner(world, brain, kind.gain_text());
    refresh(world, mob);
    true
}

/// `set_obsession(new_obsession)` (BandaStation): a new obsession for `mob`'s trauma, its
/// progress reset. False without the trauma.
pub fn set_obsession(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(brain) = organs::organ(world, mob, Slot::Brain) else {
        return false;
    };
    let Some(mut all) = world.get::<Traumas>(brain).cloned() else {
        return false;
    };
    let Some(trauma) = all.0.iter_mut().find(|t| t.kind == Kind::Obsessed) else {
        return false;
    };
    trauma.obsession = Some(super::obsession::Obsession::new(target));
    world.entity_mut(brain).insert(all);
    crate::mood::clear_event(world, mob, super::obsession::CATEGORY);
    true
}

/// The phrase `mob`'s brain is hypnotized with, if any.
pub fn hypnotic_phrase(world: &World, mob: Entity) -> Option<String> {
    of_mob(world, mob)
        .into_iter()
        .find(|t| t.kind == Kind::Hypnosis)
        .map(|t| t.phrase)
}

/// `/datum/brain_trauma/hypnosis/on_life`: 1% a second the phrase comes back, as a thought or
/// as a voice on the radio (a chat hallucination).
fn hypnosis_life(world: &mut World, mob: Entity, phrase: &str, rng: &mut CombatRng, secs: f32) {
    if !spt_prob(rng, 1.0, secs) {
        return;
    }
    if rng.chance(50.0) {
        say_to(
            world,
            mob,
            Span::Hypnophrase,
            format!("...{}...", phrase.to_lowercase()),
        );
    } else {
        crate::hallucination::kinds::chat(world, mob, true, Some(phrase.to_owned()));
    }
}

/// Takes the trauma at `index` out of `brain` (tg `qdel(trauma)` -> `on_lose`).
fn lose(world: &mut World, brain: Entity, index: usize) {
    let Some(mut all) = world.get::<Traumas>(brain).cloned() else {
        return;
    };
    let trauma = all.0.remove(index);
    world.entity_mut(brain).insert(all);
    for (item, _) in &trauma.possessed {
        release(world, *item);
    }
    // `/datum/brain_trauma/mild/hallucinations/on_lose`.
    if trauma.kind == Kind::Hallucinations
        && let Some(mob) = owner(world, brain)
    {
        crate::hallucination::set_hallucinations(world, mob, 0.0, false);
    }
    if let Some(mob) = owner(world, brain) {
        info!("{mob} lost the brain trauma {}", trauma.kind.def().path);
        if trauma.kind == Kind::Obsessed {
            crate::mood::clear_event(world, mob, super::obsession::CATEGORY);
        }
        super::special_traumas::on_lose(world, Some(mob), &trauma);
        if trauma.kind == Kind::Hypnosis {
            info!(
                "{mob} is no longer hypnotized with the phrase '{}'",
                trauma.phrase
            );
            tell_owner(
                world,
                brain,
                (
                    Span::Danger,
                    format!(
                        "Вы внезапно выходите из состояния гипноза. Фраза '{}' больше не кажется вам важной.",
                        trauma.phrase
                    ),
                ),
            );
        }
        tell_owner(world, brain, trauma.kind.lose_text());
        refresh(world, mob);
    }
}

/// `qdel` of `brain`'s trauma of `kind`'s type (whatever its resilience), if any.
pub fn remove_kind(world: &mut World, brain: Entity, kind: Kind) {
    if let Some(index) = traumas(world, brain)
        .iter()
        .position(|t| t.kind.same_type(kind))
    {
        lose(world, brain, index);
    }
}

/// tg `TRAIT_ILLITERATE`: the Illiterate quirk or dyslexia.
pub fn illiterate(world: &World, mob: Entity) -> bool {
    crate::quirks::has(world, mob, crate::quirks::Quirk::Illiterate)
        || has(world, mob, Kind::Dyslexia)
}

/// tg `cure_trauma_type(resilience)`: one random trauma of at most `resilience`.
pub fn cure_one(world: &mut World, brain: Entity, resilience: Resilience, rng: &mut CombatRng) {
    let curable: Vec<usize> = traumas(world, brain)
        .iter()
        .enumerate()
        .filter(|(_, t)| t.resilience <= resilience)
        .map(|(i, _)| i)
        .collect();
    if !curable.is_empty() {
        let index = curable[rng.range(0, curable.len() as u32 - 1) as usize];
        lose(world, brain, index);
    }
}

/// tg `cure_all_traumas(resilience)`: every trauma of at most `resilience`; how many.
pub fn cure_all(world: &mut World, brain: Entity, resilience: Resilience) -> usize {
    let mut cured = 0;
    while let Some(index) = traumas(world, brain)
        .iter()
        .position(|t| t.resilience <= resilience)
    {
        lose(world, brain, index);
        cured += 1;
    }
    cured
}

/// tg `roll_for_brain_trauma(delta)` after `delta` more brain damage brought it to `damage`
/// (above [`organs::BRAIN_DAMAGE_MILD`]).
pub fn roll(world: &mut World, brain: Entity, delta: f32, damage: f32) {
    let mut rng = super::fork_rng(world);
    let mild = organs::BRAIN_DAMAGE_MILD;
    if rng.chance(delta * (1.0 + ((damage - mild) / 100.0).max(0.0))) {
        gain_type(world, brain, Category::Mild, None, true, &mut rng);
    }
    let severe = organs::BRAIN_DAMAGE_SEVERE;
    if damage < severe {
        return;
    }
    if rng.chance(delta * (1.0 + ((damage - severe) / 100.0).max(0.0))) {
        let category = if rng.chance(20.0) {
            Category::Special
        } else {
            Category::Severe
        };
        gain_type(world, brain, category, None, true, &mut rng);
    }
}

/// `/datum/reagent/medicine/mannitol`, `/datum/reagent/medicine/neurine`.
pub const MANNITOL: &str = "/datum/reagent/medicine/mannitol";
pub const NEURINE: &str = "/datum/reagent/medicine/neurine";

/// The brain reagents in `mob`'s blood for one metabolism tick of `secs`: mannitol heals the
/// brain (`on_mob_life`, 1 a second), neurine cures a basic trauma now and then (8% a second)
/// and, in a corpse, heals the brain (`on_mob_dead`, 0.5 a second).
// shortcut: a metabolization ratio of 1 and full purity; no mannitol overdose tips, no
// neurine anticonvulsant, neurotoxin purge or `initial_bdamage` restore.
pub fn brain_reagents(world: &mut World, mob: Entity, secs: f32) {
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    if !dead && crate::chem::blood_amount(world, mob, MANNITOL) > 0.0 {
        organs::adjust_organ_loss(world, mob, Slot::Brain, -secs);
    }
    if crate::chem::blood_amount(world, mob, NEURINE) <= 0.0 {
        return;
    }
    if dead {
        organs::adjust_organ_loss(world, mob, Slot::Brain, -0.5 * secs);
        return;
    }
    let mut rng = super::fork_rng(world);
    if spt_prob(&mut rng, 8.0, secs)
        && let Some(brain) = organs::organ(world, mob, Slot::Brain)
    {
        cure_one(world, brain, Resilience::Basic, &mut rng);
    }
}

/// The scanner's `Mental trauma: ...` line of `mob`'s brain (`get_status_appendix`).
pub fn scan_line(world: &World, mob: Entity) -> Option<String> {
    let mut all = of_mob(world, mob);
    // `known_trauma = FALSE`: scanners do not show it.
    all.retain(|t| t.kind != Kind::Beepsky);
    if all.is_empty() {
        return None;
    }
    let parts: Vec<String> = all
        .iter()
        .map(|t| {
            format!(
                "{} {}",
                t.resilience.scan_word(),
                ss13_chat::speech::capitalize(&t.kind.scan_desc())
            )
        })
        .collect();
    // DM `english_list(list, and_text = ", and ")`.
    let text = match parts.as_slice() {
        [one] => one.clone(),
        [init @ .., last] => format!("{}, and {last}", init.join(", ")),
        [] => String::new(),
    };
    Some(format!("Mental trauma: {text}."))
}

/// `/datum/brain_trauma/severe/aphasia` `on_gain`/`on_lose` (`block`: every other language
/// blocked) and the hallucinations trauma's Gibbering (`gibbering`), from `LANGUAGE_APHASIA`.
fn set_aphasia(world: &mut World, mob: Entity, block: bool, gibbering: bool) {
    use ss13_chat::language::{ALL, LanguageHolder, LanguageId, LanguageSource, Preset};
    let had = world
        .get::<LanguageHolder>(mob)
        .is_some_and(|h| h.has_source(LanguageSource::Aphasia));
    if !had && !block && !gibbering {
        return;
    }
    let before = world.get::<LanguageHolder>(mob).cloned();
    let mut holder = before.clone().unwrap_or_else(|| {
        LanguageHolder::from_preset(
            &Preset::of_path("/datum/language_holder/atom_basic").unwrap_or_default(),
            LanguageSource::Atom,
        )
    });
    for language in LanguageId::ALL {
        if language == LanguageId::Aphasia {
            continue;
        }
        holder.unblock(language, ALL, Some(LanguageSource::Aphasia));
        if block {
            holder.block(language, ALL, LanguageSource::Aphasia);
        }
    }
    holder.remove(LanguageId::Aphasia, ALL, Some(LanguageSource::Aphasia));
    if block || gibbering {
        holder.grant(LanguageId::Aphasia, ALL, LanguageSource::Aphasia);
    }
    if before.as_ref() != Some(&holder) {
        world.entity_mut(mob).insert(holder);
    }
}

/// Applies the traumas of `mob`'s brain to `mob` (the traits of their `on_gain`/`on_lose`), with
/// the quirks that give the same traits.
pub fn refresh(world: &mut World, mob: Entity) {
    if world.get_entity(mob).is_err() {
        return;
    }
    let mut speech = ss13_chat::speech::Impediments::default();
    let mut pacifist = false;
    let mut paralyzed = [false; 6];
    let mut aphasia = false;
    let mut gibbering = false;
    let mut phobias = Vec::new();
    let mut monophobia = false;
    let mut split = false;
    let mut friend = false;
    let mut tenacity = false;
    for trauma in of_mob(world, mob) {
        match trauma.kind {
            Kind::Aphasia => aphasia = true,
            Kind::Hallucinations => gibbering |= !trauma.suppressed,
            Kind::Phobia(i) => phobias.push(i),
            Kind::Monophobia => monophobia = true,
            Kind::SplitPersonality => split = true,
            Kind::ImaginaryFriend => friend = true,
            Kind::Tenacity => tenacity = true,
            Kind::Stuttering => speech.stutter = true,
            Kind::Dumbness => speech.derp = true,
            Kind::SpeechImpediment => speech.unintelligible = true,
            Kind::Mute => speech.mute = true,
            Kind::Pacifism => pacifist = true,
            Kind::Paralysis(p) => {
                for zone in p.zones() {
                    paralyzed[zone.index()] = true;
                }
            }
            _ => {}
        }
    }
    // The Mute (`TRAIT_MUTE`), Soft-Spoken (`TRAIT_FORCE_WHISPER`) and Pacifist
    // (`TRAIT_PACIFISM`) quirks (`crate::quirks`).
    if let Some(quirks) = world.get::<crate::quirks::Quirks>(mob) {
        speech.mute |= quirks.has(crate::quirks::Quirk::Mute);
        speech.force_whisper |= quirks.has(crate::quirks::Quirk::SoftSpoken);
        pacifist |= quirks.has(crate::quirks::Quirk::Pacifist);
    }
    // `/datum/status_effect/pacify` (`crate::status_effects` calls this when it starts or ends).
    pacifist |= world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(crate::status_effects::StatusEffects::pacified);
    // The Hippocrates bust's oath (`crate::service`).
    pacifist |= crate::service::hippocratic_sworn(world, mob);
    // The mime's vow (`TRAIT_MIMING`, `can_speak(allow_mimes = FALSE)`).
    speech.mute |= world.get::<crate::service::MimeVow>(mob).is_some();
    // A stuck item, a slit throat (`crate::afflictions`).
    speech.mute |= crate::afflictions::mutes(world, mob);
    // The Unintelligible mutation (`crate::genetics`).
    speech.unintelligible |=
        crate::genetics::has_effect(world, mob, crate::genetics::Effect::Unintelligible);
    set_aphasia(world, mob, aphasia, gibbering);
    super::fear::set_sources(world, mob, &phobias, monophobia);
    super::split_personality::sync(world, mob, split);
    super::imaginary_friend::sync(world, mob, friend);
    // Tenacity's `TRAIT_NOSOFTCRIT` and `TRAIT_NOHARDCRIT`, with the species' own.
    let species_softcrit = crate::species::def_of(world, mob).body_mods().no_softcrit;
    if let Some(health) = world.get::<Health>(mob) {
        let (soft, hard) = (species_softcrit || tenacity, tenacity);
        if health.mods.no_softcrit != soft || health.mods.no_hardcrit != hard {
            let mut health = health.clone();
            health.mods.no_softcrit = soft;
            health.mods.no_hardcrit = hard;
            world.entity_mut(mob).insert(health);
        }
    }
    let mut entity = world.entity_mut(mob);
    if speech == ss13_chat::speech::Impediments::default() {
        entity.remove::<ss13_chat::speech::Impediments>();
    } else if entity.get::<ss13_chat::speech::Impediments>() != Some(&speech) {
        entity.insert(speech);
    }
    if pacifist {
        entity.insert(Pacifist);
    } else {
        entity.remove::<Pacifist>();
    }
    let Some(mut limbs) = world.get::<Limbs>(mob).cloned() else {
        return;
    };
    let mut newly = Vec::new();
    for zone in BodyZone::ALL {
        let limb = limbs.limb_mut(zone);
        if limb.paralyzed != paralyzed[zone.index()] {
            limb.paralyzed = paralyzed[zone.index()];
            if limb.paralyzed {
                newly.push(zone);
            }
        }
    }
    if world.get::<Limbs>(mob) != Some(&limbs) {
        world.entity_mut(mob).insert(limbs);
    }
    // `/obj/item/bodypart/arm/set_disabled`: the hand drops what it holds.
    for zone in newly {
        let hand = match zone {
            BodyZone::LArm => 0,
            BodyZone::RArm => 1,
            _ => continue,
        };
        drop_from_hand(world, mob, hand);
    }
}

/// `dropItemToGround` of what `mob` holds in `hand` (not a `TRAIT_NODROP` item). True if it fell.
fn drop_from_hand(world: &mut World, mob: Entity, hand: usize) -> bool {
    let (Some(item), Some(&pos)) = (
        world.get::<Inventory>(mob).and_then(|i| i.hands[hand]),
        world.get::<TilePos>(mob),
    ) else {
        return false;
    };
    if crate::inventory::no_drop(world, item) {
        return false;
    }
    crate::inventory::put_down(world, item, pos);
    true
}

/// Possessiveness lets go of everything it holds on to: the brain left the body or the body
/// died (tg's `relax` timers would still fire; here `on_life` stops).
pub fn let_go(world: &mut World, brain: Entity) {
    if !traumas(world, brain)
        .iter()
        .any(|t| !t.possessed.is_empty())
    {
        return;
    }
    let mut all = world.get::<Traumas>(brain).cloned().unwrap_or_default();
    for trauma in &mut all.0 {
        for (item, _) in std::mem::take(&mut trauma.possessed) {
            release(world, item);
        }
    }
    world.entity_mut(brain).insert(all);
}

/// Possessiveness lets go of `item` (`clear_trait`).
fn release(world: &mut World, item: Entity) {
    if let Ok(mut entity) = world.get_entity_mut(item) {
        entity.remove::<NoDrop>();
    }
}

/// tg `on_life` of every trauma in `mob`'s brain, one `SSmobs` fire of `secs` (a living mob).
pub fn life(world: &mut World, mob: Entity, secs: f32) {
    let Some(brain) = organs::organ(world, mob, Slot::Brain) else {
        return;
    };
    let count = traumas(world, brain).len();
    if count == 0 {
        // A fear without traumas (`crate::afflictions` terrified) still ticks.
        if world.get::<super::fear::Fearful>(mob).is_some() {
            let mut rng = super::fork_rng(world);
            super::fear::tick(world, mob, secs, &mut rng);
        }
        return;
    }
    let mut rng = super::fork_rng(world);
    for index in 0..count {
        // An effect may have cured or moved the brain.
        let Some(mut trauma) = traumas(world, brain).get(index).cloned() else {
            return;
        };
        if owner(world, brain) != Some(mob) || world.get::<Stat>(mob) == Some(&Stat::Dead) {
            return;
        }
        let suppressed = trauma.suppressed;
        on_life(world, mob, &mut trauma, secs, &mut rng);
        let changed = trauma.suppressed != suppressed;
        if let Some(mut all) = world.get_mut::<Traumas>(brain)
            && let Some(slot) = all.0.get_mut(index)
            && slot.kind == trauma.kind
            && *slot != trauma
        {
            *slot = trauma;
        }
        if changed {
            refresh(world, mob);
        }
    }
    if owner(world, brain) == Some(mob) {
        super::fear::tick(world, mob, secs, &mut rng);
    }
}

fn conscious(world: &World, mob: Entity) -> bool {
    match (world.get::<Health>(mob), world.get::<Stat>(mob)) {
        (Some(health), Some(&stat)) => !health.knocked_out(stat),
        _ => false,
    }
}

fn say_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

fn emote(world: &mut World, mob: Entity, key: &str) {
    if let Some(emote) = ss13_chat::emote::find(key) {
        crate::mobs::defense::emote(world, mob, emote.message);
    }
}

fn on_life(world: &mut World, mob: Entity, trauma: &mut Trauma, secs: f32, rng: &mut CombatRng) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    match trauma.kind {
        Kind::Dumbness => {
            if spt_prob(rng, 1.5, secs) {
                // `/datum/emote/living/drool` (BandaStation's line).
                crate::mobs::defense::emote(world, mob, "пускает слюни.");
            }
        }
        Kind::Concussion => {
            if !spt_prob(rng, 2.5, secs) {
                return;
            }
            match rng.range(1, 11) {
                1 => crate::medical::diseases::vomit(world, mob, 10.0, true, false, 1),
                2 | 3 => crate::status_effects::adjust_timed(
                    world,
                    mob,
                    crate::status_effects::Timed::Dizziness,
                    20.0,
                    None,
                ),
                6..=9 => crate::status_effects::adjust_timed(
                    world,
                    mob,
                    crate::status_effects::Timed::Slurring(
                        crate::status_effects::SlurKind::Generic,
                    ),
                    60.0,
                    None,
                ),
                11 => {
                    say_to(world, mob, Span::Warning, "Вы теряете сознание.");
                    crate::status_effects::apply(world, mob, Incap::Unconscious, 8.0);
                }
                4 | 5 => {
                    if let Some(mut effects) = crate::status_effects::effects_mut(world, mob) {
                        effects.confusion += 10.0;
                    }
                    crate::vision::set_eye_blur_if_lower(world, mob, 20.0);
                }
                10 => {
                    say_to(
                        world,
                        mob,
                        Span::Notice,
                        "На мгновение вы забываете, что делали.",
                    );
                    crate::status_effects::apply(world, mob, Incap::Stun, 2.0);
                }
                _ => {}
            }
        }
        Kind::Healthy => crate::status_effects::adjust_stamina(world, mob, -6.0 * secs),
        Kind::MuscleWeakness => {
            let running = world
                .get::<crate::janitor::Walking>(mob)
                .is_none_or(|w| !w.0);
            let fall_chance = if running { 3.0 } else { 1.0 };
            let standing = world.get::<BodyPosition>(mob).is_some_and(|b| !b.lying());
            let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
            if spt_prob(rng, 0.5 * fall_chance, secs) && standing {
                say_to(world, mob, Span::Warning, "У вас подкашивается нога!");
                crate::status_effects::apply(world, mob, Incap::Paralyze, 3.5);
            } else if let Some(item) = held {
                let w_class = world.get::<DmWeight>(item).map_or(3, |w| w.w_class);
                let active = world.get::<Inventory>(mob).map_or(0, |i| i.active);
                if spt_prob(rng, 0.5 * (1.0 + f32::from(w_class)), secs)
                    && drop_from_hand(world, mob, active)
                {
                    let name = super::name_of(world, item);
                    say_to(world, mob, Span::Warning, format!("Вы роняете {name}!"));
                }
            } else if spt_prob(rng, 1.5, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "Вы чувствуете внезапную слабость в мышцах!",
                );
                crate::status_effects::adjust_stamina(world, mob, 50.0);
            }
        }
        Kind::NervousCough => {
            if !spt_prob(rng, 6.0, secs) {
                return;
            }
            if rng.chance(5.0) {
                let line = if rng.chance(50.0) {
                    "У вас приступ кашля!"
                } else {
                    "Вы не можете перестать кашлять!"
                };
                say_to(world, mob, Span::Warning, line);
                crate::status_effects::apply(world, mob, Incap::Immobilize, 2.0);
                // shortcut: tg spaces these two by 0.6 s timers.
                emote(world, mob, "cough");
                emote(world, mob, "cough");
            }
            emote(world, mob, "cough");
        }
        Kind::Possessive => possessive(world, mob, trauma, now, rng, secs),
        Kind::Hallucinations => hallucinations(world, mob, trauma, secs),
        Kind::Narcolepsy { permanent } => narcolepsy(world, mob, permanent, rng, secs),
        Kind::SplitPersonality => super::split_personality::on_life(world, mob, secs, rng),
        Kind::Kleptomaniac => kleptomania(world, mob, trauma, now, rng, secs),
        Kind::Hypnosis => hypnosis_life(world, mob, &trauma.phrase, rng, secs),
        Kind::Obsessed => super::obsession::on_life(world, mob, trauma, secs),
        _ => super::special_traumas::on_life(world, mob, trauma, secs, rng),
    }
}

/// `/datum/brain_trauma/mild/hallucinations/on_life`: hallucinations up to a minute and
/// Gibbering, unless mindbreaker holds them off (`TRAIT_RDS_SUPPRESSED`).
fn hallucinations(world: &mut World, mob: Entity, trauma: &mut Trauma, secs: f32) {
    if crate::status_effects::knocked_out(world, mob) {
        return;
    }
    trauma.suppressed = rds_suppressed(world, mob);
    if trauma.suppressed {
        crate::hallucination::adjust_hallucinations(world, mob, -10.0 * secs);
        return;
    }
    crate::hallucination::adjust_hallucinations_up_to(world, mob, 5.0 * secs, 60.0);
}

/// `TRAIT_RDS_SUPPRESSED`: mindbreaker (or its jellyfish variant) in the blood.
pub fn rds_suppressed(world: &World, mob: Entity) -> bool {
    [
        "/datum/reagent/toxin/mindbreaker",
        "/datum/reagent/toxin/mindbreaker/fish",
    ]
    .iter()
    .any(|path| crate::chem::blood_amount(world, mob, path) > 0.0)
}

/// `/datum/brain_trauma/severe/narcolepsy/on_life`: drowsy first, then asleep; more often while
/// running, while drowsy; never with modafinil or synaptizine in the blood.
// shortcut: no `TRAIT_STIMULATED` (caffeine's halving), no source of it yet.
fn narcolepsy(world: &mut World, mob: Entity, permanent: bool, rng: &mut CombatRng, secs: f32) {
    let asleep = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(|e| e.sleeping > 0.0);
    if asleep {
        return;
    }
    let immune = [
        "/datum/reagent/medicine/modafinil",
        "/datum/reagent/medicine/synaptizine",
    ]
    .iter()
    .any(|path| crate::chem::blood_amount(world, mob, path) > 0.0);
    if immune {
        return;
    }
    // `sleep_chance`, `sleep_chance_running`, `sleep_chance_drowsy`, the sleep times.
    let (base, running_add, drowsy_add, sleep_min, sleep_max) = if permanent {
        (0.333, 0.333, 1.0, 20, 30)
    } else {
        (1.0, 2.0, 3.0, 6, 6)
    };
    use crate::status_effects::Timed;
    let drowsy = crate::status_effects::timed(world, mob, Timed::Drowsiness) > 0.0;
    let running = world
        .get::<crate::janitor::Walking>(mob)
        .is_none_or(|w| !w.0);
    let mut chance = base;
    if running {
        chance += running_add;
    }
    if drowsy {
        chance += drowsy_add;
    }
    if !spt_prob(rng, chance, secs) {
        return;
    }
    if !drowsy {
        say_to(world, mob, Span::Warning, "Вы чувствуете усталость...");
        let secs = rng.range(20, 30) as f32;
        crate::status_effects::adjust_timed(world, mob, Timed::Drowsiness, secs, None);
        if rng.chance(50.0) {
            emote(world, mob, "yawn");
        } else if rng.chance(33.0) {
            crate::mobs::defense::emote(world, mob, "трёт свои глаза.");
        }
    } else {
        say_to(world, mob, Span::Warning, "Вы засыпаете.");
        let secs = rng.range(sleep_min, sleep_max) as f32;
        crate::status_effects::apply(world, mob, Incap::Sleeping, secs);
        let asleep = world
            .get::<crate::status_effects::StatusEffects>(mob)
            .is_some_and(|e| e.sleeping > 0.0);
        if rng.chance(50.0) && asleep {
            crate::mobs::defense::emote(world, mob, "храпит.");
        }
    }
}

/// `/datum/brain_trauma/mild/possessive/on_life`, `relax`, `clear_trait`.
fn possessive(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    now: f64,
    rng: &mut CombatRng,
    secs: f32,
) {
    // `COMSIG_ITEM_DROPPED`/`COMSIG_MOVABLE_MOVED` (it left the hand some other way), `relax`.
    let mut kept = Vec::new();
    for (item, until) in std::mem::take(&mut trauma.possessed) {
        let held = world.get::<CarriedBy>(item).is_some_and(|c| c.0 == mob);
        if held && now < until {
            kept.push((item, until));
            continue;
        }
        if held {
            let name = super::name_of(world, item);
            say_to(
                world,
                mob,
                Span::Notice,
                format!("Вы чувствуете себя более комфортно, отпуская {name}."),
            );
        }
        release(world, item);
    }
    trauma.possessed = kept;
    if !spt_prob(rng, 5.0, secs) {
        return;
    }
    let hand = rng.range(0, 1) as usize;
    let Some(item) = world.get::<Inventory>(mob).and_then(|i| i.hands[hand]) else {
        return;
    };
    if crate::inventory::no_drop(world, item) {
        return;
    }
    world.entity_mut(item).insert(NoDrop);
    let until = now + f64::from(rng.range(30, 180));
    trauma.possessed.push((item, until));
    let name = super::name_of(world, item);
    say_to(
        world,
        mob,
        Span::Warning,
        format!("Вы чувствуете потребность держать {name} при себе..."),
    );
}

/// `WEIGHT_CLASS_BULKY`.
const WEIGHT_CLASS_BULKY: u8 = 4;

/// `/datum/brain_trauma/severe/kleptomaniac/on_life`: now and then an empty hand picks up a
/// small item next to the mob, without a message.
fn kleptomania(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    now: f64,
    rng: &mut CombatRng,
    secs: f32,
) {
    let hands = [0, 1].map(|h| super::bodyparts::has_hand(world, mob, h));
    if !hands.contains(&true) || !spt_prob(rng, 5.0, secs) || now < trauma.steal_at {
        return;
    }
    if !conscious(world, mob) || crate::status_effects::incapacitation(world, mob).hands_blocked {
        return;
    }
    let (Some(inventory), Some(&pos)) = (world.get::<Inventory>(mob), world.get::<TilePos>(mob))
    else {
        return;
    };
    // The active hand if it is free, else the other one.
    let active = inventory.active;
    let Some(hand) = [active, 1 - active]
        .into_iter()
        .find(|&h| hands[h] && inventory.hands[h].is_none())
    else {
        return;
    };
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut stealables: Vec<Entity> = index
        .in_radius(pos, 1)
        .filter(|&e| {
            world.get::<DmItem>(e).is_some_and(DmItem::pickable)
                && world.get::<CarriedBy>(e).is_none()
                && world
                    .get::<DmWeight>(e)
                    .is_none_or(|w| w.w_class < WEIGHT_CLASS_BULKY)
                && world
                    .get::<TilePos>(e)
                    .is_some_and(|&at| index.adjacent(pos, at, |x| x == e))
        })
        .collect();
    stealables.sort_unstable();
    if !stealables.is_empty() {
        let item = stealables[rng.range(0, stealables.len() as u32 - 1) as usize];
        info!("{mob} attempted to pick up {item} (kleptomania)");
        crate::inventory::pick_up(world, mob, item, hand);
    }
    trauma.steal_at = now + 8.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trauma(kind: Kind, resilience: Resilience) -> Trauma {
        Trauma {
            kind,
            resilience,
            possessed: Vec::new(),
            steal_at: 0.0,
            suppressed: false,
            phrase: String::new(),
            obsession: None,
            extra: Extra::default(),
        }
    }

    #[test]
    fn gains_follow_types_and_tier_limits() {
        let have = vec![
            trauma(Kind::Stuttering, Resilience::Basic),
            trauma(Kind::Dumbness, Resilience::Basic),
            trauma(Kind::Paralysis(Paralysis::Legs), Resilience::Surgery),
        ];
        // One of a type: paralysis of any kind counts as the same.
        assert!(!can_gain(&have, Kind::Stuttering, None, false));
        assert!(!can_gain(
            &have,
            Kind::Paralysis(Paralysis::LArm),
            None,
            false
        ));
        // Two basic of three: a natural one more fits, then the tier is full.
        assert!(can_gain(&have, Kind::Concussion, None, true));
        let mut full = have.clone();
        full.push(trauma(Kind::Concussion, Resilience::Basic));
        assert!(!can_gain(&full, Kind::Healthy, None, true));
        // Given ones ignore the limit; another tier counts on its own.
        assert!(can_gain(&full, Kind::Healthy, None, false));
        assert!(can_gain(
            &full,
            Kind::Healthy,
            Some(Resilience::Lobotomy),
            true
        ));
    }

    #[test]
    fn random_severe_gains_come_as_often_as_in_tg() {
        // tg has 13 randomly gainable severe types; paralysis is one of them.
        let severe = TG_POOL.iter().filter(|e| e.2 == Category::Severe).count();
        assert_eq!(severe, 13);
        let mut world = World::new();
        let brain = world.spawn_empty().id();
        let mut rng = CombatRng(11);
        let (rolls, mut paralysis, mut none) = (13_000, 0, 0);
        for _ in 0..rolls {
            match gain_type(&mut world, brain, Category::Severe, None, true, &mut rng) {
                Some(Kind::Paralysis(_)) => paralysis += 1,
                None => none += 1,
                Some(_) => {}
            }
            world.entity_mut(brain).remove::<Traumas>();
        }
        // 1/13 each (1000 expected); nothing from the 2 of 13 unported (discoordination,
        // hypnotic stupor). A bare brain takes a split personality (it needs a player only in a
        // body).
        assert!((850..1150).contains(&paralysis), "{paralysis}");
        assert!((1_700..2_300).contains(&none), "{none}");
    }

    #[test]
    fn categories_and_scan_words() {
        // The ported kinds sit in the pool under their own type, category and resilience.
        for (path, kind, category, resilience) in TG_POOL {
            if let Some(kind) = kind {
                let def = kind.def();
                assert_eq!(
                    (def.path, def.category, def.resilience),
                    (path, category, resilience)
                );
            }
        }
        assert_eq!(Resilience::Lobotomy.scan_word(), "Deep-rooted");
        assert!(Resilience::Basic < Resilience::Surgery);
        assert_eq!(
            Kind::Paralysis(Paralysis::Left).gain_text().1,
            "Вы больше не чувствуете левую сторону вашего тела!"
        );
    }
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use crate::health::Health;
    use crate::medical::organs::{OrganHost, insert, remove, spawn_organ};

    /// A conscious human with organs, limbs, hands and a tile.
    fn human(world: &mut World) -> Entity {
        world.init_resource::<Time>();
        let mob = world
            .spawn((
                Health::default(),
                Stat::Conscious,
                OrganHost,
                Limbs::default(),
                Inventory::default(),
                TilePos::new(1, 1, 1),
            ))
            .id();
        for slot in Slot::ALL {
            let organ = spawn_organ(world, slot.spec().path).unwrap();
            world.entity_mut(organ).insert(OrganOf(mob));
        }
        mob
    }

    fn brain(world: &World, mob: Entity) -> Entity {
        organs::organ(world, mob, Slot::Brain).unwrap()
    }

    #[test]
    fn traumas_follow_the_brain() {
        let mut world = World::new();
        let mob = human(&mut world);
        let b = brain(&world, mob);
        assert!(gain(&mut world, b, Kind::Mute, None));
        assert!(!gain(&mut world, b, Kind::Mute, None), "one of a type");
        assert!(gain(&mut world, b, Kind::Pacifism, None));
        let speech = |w: &World| w.get::<ss13_chat::speech::Impediments>(mob).copied();
        assert!(speech(&world).is_some_and(|s| s.mute && !s.stutter));
        assert!(world.get::<Pacifist>(mob).is_some());
        // Out of the body the effects go; back in, they come back.
        remove(&mut world, b, true);
        assert!(speech(&world).is_none() && world.get::<Pacifist>(mob).is_none());
        assert_eq!(world.get::<Traumas>(b).unwrap().0.len(), 2);
        insert(&mut world, b, mob);
        assert!(speech(&world).is_some_and(|s| s.mute));
        assert!(has(&world, mob, Kind::Pacifism));
        assert_eq!(
            scan_line(&world, mob).unwrap(),
            "Mental trauma: Severe Обширное повреждение речевого центра мозга, and Severe \
             Синдром пацифизма."
        );
    }

    #[test]
    fn obsession_counts_time_away_and_goes_with_lobotomy() {
        let mut world = World::new();
        let mob = human(&mut world);
        let b = brain(&world, mob);
        let mut rng = CombatRng(5);
        // Nobody to obsess over: no trauma.
        assert!(!gain(&mut world, b, Kind::Obsessed, None));
        let target = world.spawn((Stat::Conscious, TilePos::new(20, 20, 1))).id();
        assert!(
            !gain_obsessed(&mut world, b, Some(mob), None, &mut rng),
            "not oneself"
        );
        assert!(gain_obsessed(&mut world, b, Some(target), None, &mut rng));
        let state = |w: &World| of_mob(w, mob)[0].obsession.unwrap();
        assert_eq!(state(&world).target, target);
        // Out of view: the time away adds up, the time near resets.
        life(&mut world, mob, 2.0);
        life(&mut world, mob, 2.0);
        let s = state(&world);
        assert!(!s.viewing);
        assert!((s.away - 4.0).abs() < 1e-4 && s.creeping == 0.0);
        // A new obsession starts over.
        let other = world.spawn((Stat::Conscious, TilePos::new(2, 2, 1))).id();
        assert!(set_obsession(&mut world, mob, other));
        assert_eq!(state(&world).target, other);
        assert_eq!(state(&world).away, 0.0);
        // Brain surgery does not take it; a lobotomy does.
        assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 0);
        assert_eq!(cure_all(&mut world, b, Resilience::Lobotomy), 1);
        assert!(!has(&world, mob, Kind::Obsessed));
    }

    #[test]
    fn special_and_magic_traumas_apply_their_traits() {
        let mut world = World::new();
        let mob = human(&mut world);
        let b = brain(&world, mob);
        // Tenacity: conscious down to death.
        assert!(gain(&mut world, b, Kind::Tenacity, None));
        let health = world.get::<Health>(mob).unwrap().clone();
        assert!(health.mods.no_softcrit && health.mods.no_hardcrit);
        let mut hurt = health.clone();
        hurt.adjust(crate::health::DamageType::Brute, 180.0);
        let mut stat = Stat::Conscious;
        crate::health::update_stat(&hurt, &mut stat);
        assert_eq!(stat, Stat::Conscious);
        remove_kind(&mut world, b, Kind::Tenacity);
        assert!(!world.get::<Health>(mob).unwrap().mods.no_hardcrit);
        // Antimagic and godwoken resist magic.
        use crate::spells::{MAGIC_RESISTANCE, MAGIC_RESISTANCE_HOLY, can_block_magic};
        assert!(!can_block_magic(&world, mob, MAGIC_RESISTANCE));
        assert!(gain(&mut world, b, Kind::Antimagic, None));
        assert!(gain(&mut world, b, Kind::Godwoken, None));
        assert!(can_block_magic(&world, mob, MAGIC_RESISTANCE));
        assert!(can_block_magic(&world, mob, MAGIC_RESISTANCE_HOLY));
        // Magic traumas are lobotomy-deep; the criminal's is hidden from scanners.
        assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 1, "godwoken");
        assert!(has(&world, mob, Kind::Antimagic));
        assert!(gain(&mut world, b, Kind::Beepsky, None));
        assert_eq!(
            scan_line(&world, mob).unwrap(),
            "Mental trauma: Deep-rooted Таумическая пустота."
        );
        // Quantum alignment entangles with a held item before the floor.
        use super::super::special_traumas::entangle_target;
        assert_eq!(
            entangle_target(&world, mob),
            Some(Link::Turf(TilePos::new(1, 1, 1)))
        );
        let item = world.spawn(CarriedBy(mob)).id();
        world.get_mut::<Inventory>(mob).unwrap().hands[0] = Some(item);
        assert_eq!(entangle_target(&world, mob), Some(Link::Entity(item)));
    }

    #[test]
    fn hypnosis_keeps_its_phrase_until_surgery() {
        let mut world = World::new();
        let mob = human(&mut world);
        let b = brain(&world, mob);
        let mut rng = CombatRng(3);
        // `New(phrase)` without a phrase: no trauma; the plain gain needs one too.
        assert!(!gain_hypnosis(&mut world, b, "  ", None, &mut rng));
        assert!(!gain(&mut world, b, Kind::Hypnosis, None));
        assert!(gain_hypnosis(
            &mut world,
            b,
            " Слушай клоуна ",
            None,
            &mut rng
        ));
        assert!(
            !gain_hypnosis(&mut world, b, "другое", None, &mut rng),
            "one of a type"
        );
        assert_eq!(
            hypnotic_phrase(&world, mob).as_deref(),
            Some("Слушай клоуна")
        );
        assert_eq!(
            scan_line(&world, mob).unwrap(),
            "Mental trauma: Severe Повторяющийся мыслительный паттерн."
        );
        // Neurine's basic cure leaves it; brain surgery takes it.
        assert_eq!(cure_all(&mut world, b, Resilience::Basic), 0);
        assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 1);
        assert_eq!(hypnotic_phrase(&world, mob), None);
    }

    #[test]
    fn paralysis_disables_limbs_and_drops_the_hand() {
        let mut world = World::new();
        let mob = human(&mut world);
        let item = world.spawn(CarriedBy(mob)).id();
        world.get_mut::<Inventory>(mob).unwrap().hands[0] = Some(item);
        let b = brain(&world, mob);
        gain(&mut world, b, Kind::Paralysis(Paralysis::Left), None);
        let limbs = world.get::<Limbs>(mob).unwrap();
        assert!(limbs.limb(BodyZone::LArm).paralyzed && limbs.limb(BodyZone::LLeg).paralyzed);
        assert!(!limbs.limb(BodyZone::RLeg).paralyzed);
        // One leg: slower, not floored; the left hand let go and holds nothing.
        let limits = limbs.limits(world.get::<Health>(mob).unwrap());
        assert!(limits.slowdown >= 3.0 && !limits.floored);
        assert!(world.get::<TilePos>(item).is_some());
        assert!(!crate::medical::bodyparts::has_hand(&world, mob, 0));
        assert!(crate::medical::bodyparts::has_hand(&world, mob, 1));
        assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 1);
        assert!(
            !world
                .get::<Limbs>(mob)
                .unwrap()
                .limb(BodyZone::LArm)
                .paralyzed
        );
    }

    #[test]
    fn cures_go_by_resilience() {
        let mut world = World::new();
        let mob = human(&mut world);
        let b = brain(&world, mob);
        gain(&mut world, b, Kind::Stuttering, None);
        gain(&mut world, b, Kind::Mute, None);
        gain(&mut world, b, Kind::Pacifism, Some(Resilience::Lobotomy));
        gain(&mut world, b, Kind::Dumbness, Some(Resilience::Magic));
        let mut rng = CombatRng(7);
        // Neurine's cure takes a basic one only.
        cure_one(&mut world, b, Resilience::Basic, &mut rng);
        assert!(!has(&world, mob, Kind::Stuttering) && has(&world, mob, Kind::Mute));
        cure_one(&mut world, b, Resilience::Basic, &mut rng);
        assert!(has(&world, mob, Kind::Mute));
        // Brain surgery, then a lobotomy; magic stays.
        assert_eq!(cure_all(&mut world, b, Resilience::Surgery), 1);
        assert!(has(&world, mob, Kind::Pacifism));
        assert_eq!(cure_all(&mut world, b, Resilience::Lobotomy), 1);
        assert!(has(&world, mob, Kind::Dumbness));
        assert!(
            world
                .get::<ss13_chat::speech::Impediments>(mob)
                .is_some_and(|s| s.derp)
        );
    }

    #[test]
    fn brain_damage_rolls_traumas() {
        let mut world = World::new();
        let mob = human(&mut world);
        world.insert_resource(CombatRng(3));
        let b = brain(&world, mob);
        // Below the mild threshold nothing rolls.
        organs::set_organ_damage(&mut world, mob, Slot::Brain, 20.0);
        assert!(of_mob(&world, mob).is_empty());
        // 50 more at 150: the mild roll is sure (50 x 2.3), the severe one 75%.
        organs::set_organ_damage(&mut world, mob, Slot::Brain, 70.0);
        organs::adjust_organ_loss(&mut world, mob, Slot::Brain, 80.0);
        let all = of_mob(&world, mob);
        assert!(
            all.iter().any(|t| t.kind.def().category == Category::Mild),
            "{all:?}"
        );
        // Natural gains stop at the tier limits whatever the damage.
        for _ in 0..40 {
            organs::set_organ_damage(&mut world, mob, Slot::Brain, 110.0);
            organs::adjust_organ_loss(&mut world, mob, Slot::Brain, 80.0);
        }
        let all = world.get::<Traumas>(b).unwrap().0.clone();
        let count = |r| all.iter().filter(|t| t.resilience == r).count();
        assert_eq!(count(Resilience::Basic), 3, "{all:?}");
        assert_eq!(count(Resilience::Surgery), 2, "{all:?}");
    }

    #[test]
    fn healthy_and_weakness_act_on_life() {
        let mut world = World::new();
        let mob = human(&mut world);
        let b = brain(&world, mob);
        gain(&mut world, b, Kind::Healthy, None);
        crate::status_effects::adjust_stamina(&mut world, mob, 40.0);
        life(&mut world, mob, 2.0);
        let stamina = world
            .get::<crate::status_effects::StatusEffects>(mob)
            .unwrap()
            .stamina;
        assert_eq!(stamina, 28.0);
        // Possessiveness holds on to a held item for a while, then lets go.
        cure_all(&mut world, b, Resilience::Basic);
        gain(&mut world, b, Kind::Possessive, None);
        let item = world.spawn(CarriedBy(mob)).id();
        world.get_mut::<Inventory>(mob).unwrap().hands = [Some(item), Some(item)];
        world.insert_resource(CombatRng(1));
        for _ in 0..200 {
            life(&mut world, mob, 2.0);
            if world.get::<NoDrop>(item).is_some() {
                break;
            }
        }
        assert!(world.get::<NoDrop>(item).is_some(), "5% a second");
        // Taking the brain out lets go (no `on_life` holds it any more).
        remove(&mut world, b, true);
        assert!(world.get::<NoDrop>(item).is_none());
        insert(&mut world, b, mob);
        for _ in 0..200 {
            life(&mut world, mob, 2.0);
            if world.get::<NoDrop>(item).is_some() {
                break;
            }
        }
        cure_all(&mut world, b, Resilience::Basic);
        assert!(world.get::<NoDrop>(item).is_none(), "the cure lets go");
    }
}
