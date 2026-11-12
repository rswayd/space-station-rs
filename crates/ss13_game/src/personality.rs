//! Personalities (server), after tg `code/datums/personality` (`/datum/personality` singletons),
//! `SSpersonalities` (`code/controllers/subsystem/processing/personality.dm`) and the preferences
//! menu's personality middleware (`preferences/middleware/personality.dm`).
//!
//! - The character's personalities are picked by savefile key in the preferences window
//!   ([`PersonalityPrefs`] on the client, `handle_personality`/`clear_personalities` acts checked by
//!   [`toggle_prefs`]): at most [`MAX_PERSONALITIES`], none sharing a [`Group`] with another.
//! - A human joining the round gets them ([`assign`] -> [`apply_to_mob`]): their bits on its
//!   [`Mood`] (tg `mob.personalities`, `HAS_PERSONALITY` is [`Mood::has_personality`]) and their
//!   mood multipliers.
//! - Mood events react to them when added ([`can_effect`], [`on_add`]: tg `can_effect_mob`,
//!   `on_add`'s event flags and the per-event `add_effects`), the drunk, food and death moodlets
//!   through `crate::mood`.
//! - Every `SSpersonalities` fire (3 s) the processing ones act ([`personality_life`]): Paranoid
//!   counts the people around, Loyalist and Disillusioned look for corporate signs, and the area
//!   moodlets (`/datum/status_effect/moodlet_in_area`) of Lazy, Diligent, Analytical and Impulsive
//!   follow the mob's area (the area bonuses that need Extrovert, Introvert or Spiritual,
//!   `check_area_mood`'s `mood_trait`, are `crate::area_mood`'s).
// shortcut: the area moodlets follow the 3 s fire instead of `COMSIG_ENTER_AREA` (no area-enter
// signal here); upgrade when areas announce entries. Paranoid ignores invisibility (no
// `see_invisible` here). Aloof shows no `annoyed` visual. The reactions whose events are not
// ported wait for their mechanics: gaming (Gambler, Industrious: arcades, cards, slots), exercise
// (Athletic, Lazy), petting and pet deaths (the animal personalities), art and music (Creative,
// Unimaginative), the empathy mutations (Empathetic, Misanthropic), help-ups (Compassionate,
// Callous), whimsical slips and creampies (Whimsical), the revolution's end
// moodlets (Loyalist, Disillusioned), `drunk_after` (Bibulous) and dirty food (Ascetic, Gourmand).

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

use crate::health::Stat;
use crate::human::HumanLook;
use crate::mood::{ActiveMood, Mood, MoodEvent, SANITY_NEUTRAL};

/// `max_personalities` (config default).
pub const MAX_PERSONALITIES: usize = 5;
/// `SSpersonalities` `wait`, seconds.
pub const PERSONALITIES_WAIT_SECS: f64 = 3.0;
/// Paranoid's and the NT personalities' `view(subject, 5)`.
const VIEW_RANGE: u32 = 5;

/// A `/datum/personality` subtype with a savefile key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Personality {
    // reaction_generic.dm
    Apathetic,
    Sensitive,
    Resilient,
    Brooding,
    Hopeful,
    Pessimistic,
    Whimsical,
    Snob,
    // reaction_to_animals.dm
    AnimalFriend,
    CatFan,
    DogFan,
    AnimalDisliker,
    // reaction_to_art.dm
    Creative,
    Unimaginative,
    // reaction_to_fear.dm
    Brave,
    Cowardly,
    // reaction_to_food.dm
    Ascetic,
    Gourmand,
    Teetotal,
    Bibulous,
    // reaction_to_groups.dm
    Introvert,
    Extrovert,
    Paranoid,
    // reaction_to_job.dm
    Analytical,
    Impulsive,
    // reaction_to_nt.dm
    Loyalist,
    Disillusioned,
    // reaction_to_others.dm
    Callous,
    Compassionate,
    Empathetic,
    Misanthropic,
    Aloof,
    // reaction_to_recreation.dm
    Gambler,
    Lazy,
    Diligent,
    Industrious,
    Athletic,
    Erudite,
    Uneducated,
    Spiritual,
}

/// The `PERSONALITY_GROUP_*` defines: personalities sharing one exclude each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    MoodLength,
    MoodPower,
    Death,
    GeneralFear,
    PeopleFear,
    Department,
    Animals,
    Nt,
    Others,
    Interaction,
    Recreation,
    Work,
    Alcohol,
    Food,
    Hope,
    Misanthropy,
    Athletics,
    Art,
    Reading,
}

impl Group {
    /// The define's value (what tgui gets).
    pub fn key(self) -> &'static str {
        match self {
            Self::MoodLength => "reaction_to_thought",
            Self::MoodPower => "general_reaction",
            Self::Death => "reaction_to_death",
            Self::GeneralFear => "reaction_to_fear",
            Self::PeopleFear => "reaction_to_fear_others",
            Self::Department => "reaction_to_department",
            Self::Animals => "reaction_to_animals",
            Self::Nt => "reaction_to_nt",
            Self::Others => "reaction_to_others",
            Self::Interaction => "reaction_to_interaction",
            Self::Recreation => "reaction_to_recreation",
            Self::Work => "reaction_to_work",
            Self::Alcohol => "reaction_to_alcohol",
            Self::Food => "reaction_to_food",
            Self::Hope => "reaction_to_hope",
            Self::Misanthropy => "reaction_to_misanthropy",
            Self::Athletics => "reaction_to_athletics",
            Self::Art => "reaction_to_art",
            Self::Reading => "reaction_to_reading",
        }
    }
}

/// One personality's `savefile_key`, `name`, `desc`, the three gameplay blurbs ("" for none) and
/// `groups`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PersonalityDef {
    pub personality: Personality,
    pub key: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub pos: &'static str,
    pub neg: &'static str,
    pub neut: &'static str,
    pub groups: &'static [Group],
}

const fn def(
    personality: Personality,
    key: &'static str,
    name: &'static str,
    desc: &'static str,
    [pos, neg, neut]: [&'static str; 3],
    groups: &'static [Group],
) -> PersonalityDef {
    PersonalityDef {
        personality,
        key,
        name,
        desc,
        pos,
        neg,
        neut,
        groups,
    }
}

use Group as G;
use Personality as P;

/// Every personality, in the order of tg's files (BandaStation texts).
pub const PERSONALITIES: &[PersonalityDef] = &[
    def(
        P::Apathetic,
        "apathetic",
        "Апатичный",
        "Меня практически ничего не волнует. Ни хорошее, ни плохое, и уж точно не уродливое.",
        ["", "", "Все модификаторы настроения влияют на вас слабее"],
        &[G::MoodPower],
    ),
    def(
        P::Sensitive,
        "sensitive",
        "Чувствительный",
        "Я легко поддаюсь влиянию окружающего мира.",
        ["", "", "Все модификаторы настроения влияют на вас сильнее"],
        &[G::MoodPower],
    ),
    def(
        P::Resilient,
        "resilient",
        "Стойкий",
        "Неважно. Я могу это вынести!",
        [
            "Негативные модификаторы настроения заканчиваются быстрее",
            "",
            "",
        ],
        &[G::MoodLength],
    ),
    def(
        P::Brooding,
        "brooding",
        "Задумчивый",
        "Все это задевает меня, и я не могу не думать об этом.",
        [
            "",
            "Негативные модификаторы настроения сохраняются дольше",
            "",
        ],
        &[G::MoodLength],
    ),
    def(
        P::Hopeful,
        "hopeful",
        "Оптимистичный",
        "Я верю, что все всегда будет становиться лучше.",
        [
            "Позитивные модификаторы настроения сохраняются дольше",
            "",
            "",
        ],
        &[G::Hope],
    ),
    def(
        P::Pessimistic,
        "pessimistic",
        "Пессимистичный",
        "Я верю, что наши лучшие дни остались позади.",
        ["", "Позитивные модификаторы настроения длятся меньше", ""],
        &[G::Hope],
    ),
    def(
        P::Whimsical,
        "whimsical",
        "Эксцентричный",
        "Эта станция иногда бывает слишком серьезной. Расслабься!",
        [
            "Любит якобы бессмысленные, но глупые вещи и не возражает против клоунских выходок",
            "",
            "",
        ],
        &[],
    ),
    def(
        P::Snob,
        "snob",
        "Сноб",
        "Я ожидаю от этой станции только самого лучшего - меньшее неприемлемо!",
        ["", "", "Качество комнаты влияет на ваше настроение"],
        &[],
    ),
    def(
        P::AnimalFriend,
        "animal_friend",
        "Друг животных",
        "Я обожаю животных! Они никогда не оставят меня.",
        [
            "Любит находиться рядом с животными",
            "Смерть животного сильно влияет на ваше настроение",
            "",
        ],
        &[G::Animals],
    ),
    def(
        P::CatFan,
        "cat_fan",
        "Кошатник",
        "Кошки такие милые!",
        [
            "Любит находиться рядом с кошками",
            "Не любит находиться рядом с собаками",
            "",
        ],
        &[G::Animals],
    ),
    def(
        P::DogFan,
        "dog_fan",
        "Собачник",
        "Собаки - самые лучшие!",
        [
            "Любит находиться рядом с собаками",
            "Не любит находиться рядом с кошками",
            "",
        ],
        &[G::Animals],
    ),
    def(
        P::AnimalDisliker,
        "animal_disliker",
        "Избегающий животных",
        "Мы едва выживаем на этой станции, а ты хочешь завести питомца?",
        ["", "Не любит находиться рядом с животными", ""],
        &[G::Animals],
    ),
    def(
        P::Creative,
        "creative",
        "Творческий",
        "Мне нравится самовыражаться, особенно в таком хаотичном месте, как это.",
        ["Любит заниматься искусством или музицировать", "", ""],
        &[G::Art],
    ),
    def(
        P::Unimaginative,
        "unimaginative",
        "Лишенный воображения",
        "Я не силен в нестандартном мышлении. Коробка существует не просто так.",
        [
            "",
            "Игнорирует искусство или музыку и не любит создавать новые произведения искусства",
            "",
        ],
        &[G::Art],
    ),
    def(
        P::Brave,
        "brave",
        "Храбрый",
        "Потребуется гораздо больше, чем просто капля крови, чтобы напугать меня.",
        [
            "Страх накапливается медленнее, а модификаторы настроения, связанные со страхом, влияют слабее",
            "",
            "",
        ],
        &[G::GeneralFear, G::PeopleFear],
    ),
    def(
        P::Cowardly,
        "cowardly",
        "Трусливый",
        "Здесь всё представляет опасность! Даже воздух!",
        [
            "",
            "Страх накапливается быстрее, а модификаторы настроения, связанные со страхом, влияют сильнее",
            "",
        ],
        &[G::GeneralFear],
    ),
    def(
        P::Ascetic,
        "ascetic",
        "Аскетичный",
        "Я не очень люблю изысканные блюда - все это только топливо для организма.",
        [
            "Грусть от употребления нелюбимой еды уменьшена",
            "Радость от поедания любимой еды ограничена",
            "",
        ],
        &[G::Food],
    ),
    def(
        P::Gourmand,
        "gourmand",
        "Гурман",
        "Еда для меня — это всё!",
        [
            "Радость от поедания любимой еды усиливается",
            "Грусть от съеденной нелюбимой пищи увеличивается, а обычная еда приносит меньше удовольствия",
            "",
        ],
        &[G::Food],
    ),
    def(
        P::Teetotal,
        "teetotal",
        "Трезвенник",
        "Алкоголь не для меня.",
        ["", "Не любит употреблять алкоголь", ""],
        &[G::Alcohol],
    ),
    def(
        P::Bibulous,
        "bibulous",
        "Любитель выпить",
        "Я всегда готов выпить ещё!",
        [
            "Удовлетворение от выпивки длится дольше, даже после того, как вы больше не пьяны",
            "",
            "",
        ],
        &[G::Alcohol],
    ),
    def(
        P::Introvert,
        "introvert",
        "Интроверт",
        "Я предпочитаю побыть один, почитать или порисовать в библиотеке.",
        ["Любит бывать в библиотеке", "", ""],
        &[G::Interaction],
    ),
    def(
        P::Extrovert,
        "extrovert",
        "Экстраверт",
        "Я предпочитаю быть в окружении людей, выпивая что-нибудь в баре.",
        ["Любит бывать в баре", "", ""],
        &[G::Interaction, G::Others],
    ),
    def(
        P::Paranoid,
        "paranoid",
        "Параноидальный",
        "Все и вся здесь хотят меня достать! Это место - смертельная ловушка!",
        [
            "Любит бывать в одиночестве или в небольших группах",
            "Испытывает стресс, находясь наедине с другим человеком или в больших группах",
            "",
        ],
        &[G::PeopleFear],
    ),
    def(
        P::Analytical,
        "analytical",
        "Аналитик",
        "При принятии решений я склонен действовать более отстранённо и объективно.",
        [
            "",
            "",
            "Предпочитает работать в структурированных системах, таких как инженерия, наука или медицина",
        ],
        &[G::Department],
    ),
    def(
        P::Impulsive,
        "impulsive",
        "Импульсивный",
        "Я лучше справляюсь, действуя спонтанно и импровизируя по ходу дела.",
        [
            "",
            "",
            "Предпочитает работать в социальных сферах, таких как карго, командование, безопасность или обслуживание",
        ],
        &[G::Department],
    ),
    def(
        P::Loyalist,
        "loyalist",
        "Лояльный",
        "Я верю в станцию и в Центральное Командование до самого конца!",
        ["Нравятся корпоративные постеры и знаки", "", ""],
        &[G::Nt],
    ),
    def(
        P::Disillusioned,
        "disillusioned",
        "Разочарованный",
        "Нанотрейзен уже не тот, что был раньше. Я на это не подписывался.",
        ["", "Не нравятся корпоративные постеры и знаки", ""],
        &[G::Nt],
    ),
    def(
        P::Callous,
        "callous",
        "Бессердечный",
        "Меня не очень волнует, что происходит с другими людьми.",
        [
            "Не прочь увидеть смерть",
            "Предпочитает не помогать людям",
            "",
        ],
        &[G::Death],
    ),
    def(
        P::Compassionate,
        "compassionate",
        "Сострадательный",
        "Я люблю помогать тем, кто нуждается в поддержке.",
        [
            "Любит помогать людям",
            "Вид смерти больше влияет на настроение",
            "",
        ],
        &[G::Death, G::Misanthropy],
    ),
    def(
        P::Empathetic,
        "empathetic",
        "Эмпатичный",
        "Чувства других людей важны для меня.",
        [
            "Любит видеть других людей счастливыми",
            "Не любит видеть других людей грустными",
            "",
        ],
        &[G::Others],
    ),
    def(
        P::Misanthropic,
        "misanthropic",
        "Мизантроп",
        "Нам вообще не следовало отправляться к звёздам.",
        [
            "Любит видеть других людей грустными",
            "Не любит видеть других людей счастливыми",
            "",
        ],
        &[G::Others, G::Misanthropy],
    ),
    def(
        P::Aloof,
        "aloof",
        "Отчужденный",
        "Почему все такие чувствительные? Я бы предпочёл остаться один.",
        ["", "Не любит, когда его хватают, трогают или обнимают", ""],
        &[],
    ),
    def(
        P::Gambler,
        "gambler",
        "Азартный игрок",
        "Игра в кости всегда того стоит!",
        [
            "Любит азартные игры и карточные развлечения, спокойно воспринимает проигрыши",
            "",
            "",
        ],
        &[],
    ),
    def(
        P::Lazy,
        "lazy",
        "Ленивый",
        "На самом деле мне сегодня не хочется работать.",
        ["Приятно проводить время в баре или зонах отдыха", "", ""],
        &[G::Recreation, G::Work, G::Athletics],
    ),
    def(
        P::Diligent,
        "diligent",
        "Старательный",
        "Тут столько всего надо сделать!",
        [
            "Счастлив, находясь в своём отделе.",
            "Несчастен, когда отдыхает в баре или зонах отдыха, уклоняясь от обязанностей",
            "",
        ],
        &[G::Recreation],
    ),
    def(
        P::Industrious,
        "industrious",
        "Трудолюбивый",
        "Каждый должен работать, иначе мы все напрасно тратим свое время.",
        ["", "Не любит играть в игры", ""],
        &[G::Work],
    ),
    def(
        P::Athletic,
        "athletic",
        "Атлетичный",
        "Нельзя же весь день сидеть сложа руки! Нужно двигаться.",
        ["Любит заниматься спортом", "Не любит лениться", ""],
        &[G::Athletics],
    ),
    def(
        P::Erudite,
        "erudite",
        "Эрудированный",
        "Знание — сила. Особенно далеко в космосе.",
        ["Любит читать книги", "", ""],
        &[G::Reading],
    ),
    def(
        P::Uneducated,
        "uneducated",
        "Необразованный",
        "Меня не очень интересуют книги - я уже знаю все, что мне нужно знать.",
        ["", "Не любит читать книги", ""],
        &[G::Reading],
    ),
    def(
        P::Spiritual,
        "spiritual",
        "Духовный",
        "Я верю в высшие силы.",
        [
            "Любит церковь и священника, имеет особые молитвы.",
            "Не любит нечестивые вещи",
            "",
        ],
        &[],
    ),
];

impl Personality {
    pub fn def(self) -> &'static PersonalityDef {
        PERSONALITIES
            .iter()
            .find(|d| d.personality == self)
            .expect("every personality has a def")
    }

    /// `SSpersonalities.personalities_by_key`.
    pub fn by_key(key: &str) -> Option<Self> {
        PERSONALITIES
            .iter()
            .find(|d| d.key == key)
            .map(|d| d.personality)
    }

    /// Its bit in [`Mood::personalities`].
    pub const fn bit(self) -> u64 {
        1 << self as u8
    }
}

/// `SSpersonalities.is_incompatible`: `new` shares a group with one of `selected`.
pub fn is_incompatible(selected: &[Personality], new: Personality) -> bool {
    new.def()
        .groups
        .iter()
        .any(|g| selected.iter().any(|s| s.def().groups.contains(g)))
}

/// On a client connection: the character's personalities by savefile key (tg
/// `/datum/preference/personality`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct PersonalityPrefs(pub Vec<String>);

/// `/datum/preference/personality/deserialize`: unknown keys, incompatible ones and those past
/// [`MAX_PERSONALITIES`] are dropped.
pub fn sanitize(keys: &[String]) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    let mut types: Vec<Personality> = Vec::new();
    for key in keys {
        let Some(personality) = Personality::by_key(key) else {
            continue;
        };
        if is_incompatible(&types, personality) {
            continue;
        }
        if kept.len() >= MAX_PERSONALITIES {
            break;
        }
        kept.push(key.clone());
        types.push(personality);
    }
    kept
}

/// `handle_personality`: a chosen key is removed, another one added if there is room and it fits.
/// True if the act was well-formed (tg returns TRUE for a refused addition too); false for an
/// unknown key.
pub fn toggle_prefs(prefs: &mut PersonalityPrefs, key: &str) -> bool {
    let Some(personality) = Personality::by_key(key) else {
        return false;
    };
    let mut current = sanitize(&prefs.0);
    if let Some(i) = current.iter().position(|k| k == key) {
        current.remove(i);
    } else {
        let types: Vec<Personality> = current
            .iter()
            .filter_map(|k| Personality::by_key(k))
            .collect();
        if current.len() < MAX_PERSONALITIES && !is_incompatible(&types, personality) {
            current.push(key.to_owned());
        }
    }
    prefs.0 = current;
    true
}

/// `/datum/personality/apply_to_mob`: false without a mood (tg asserts one) or when it has it.
pub fn apply_to_mob(world: &mut World, mob: Entity, personality: Personality) -> bool {
    let Some(mut mood) = world.get_mut::<Mood>(mob) else {
        return false;
    };
    if mood.has_personality(personality) {
        return false;
    }
    mood.personalities |= personality.bit();
    match personality {
        P::Apathetic => mood.mood_modifier -= 0.2,
        P::Sensitive => mood.mood_modifier += 0.2,
        P::Resilient => mood.negative_length_modifier -= 0.2,
        P::Brooding => mood.negative_length_modifier += 0.2,
        P::Hopeful => mood.positive_length_modifier += 0.2,
        P::Pessimistic => mood.positive_length_modifier -= 0.2,
        _ => {}
    }
    mood.update_mood();
    true
}

/// `/datum/personality/remove_from_mob`: false if the mob does not have it.
pub fn remove_from_mob(world: &mut World, mob: Entity, personality: Personality) -> bool {
    let Some(mut mood) = world.get_mut::<Mood>(mob) else {
        return false;
    };
    if !mood.has_personality(personality) {
        return false;
    }
    mood.personalities &= !personality.bit();
    match personality {
        P::Apathetic => mood.mood_modifier += 0.2,
        P::Sensitive => mood.mood_modifier -= 0.2,
        P::Resilient => mood.negative_length_modifier += 0.2,
        P::Brooding => mood.negative_length_modifier -= 0.2,
        P::Hopeful => mood.positive_length_modifier -= 0.2,
        P::Pessimistic => mood.positive_length_modifier += 0.2,
        P::Paranoid => {
            mood.clear_event(PARANOIA_CATEGORY);
        }
        P::Loyalist | P::Disillusioned => {
            mood.clear_event(NT_CATEGORY);
        }
        P::Lazy => {
            mood.clear_event(LAZY_CATEGORY);
        }
        P::Diligent => {
            mood.clear_event(DILIGENT_SLACK_CATEGORY);
            mood.clear_event(DILIGENT_WORK_CATEGORY);
        }
        P::Analytical | P::Impulsive => {
            mood.clear_event(DEPARTMENT_CATEGORY);
        }
        _ => {}
    }
    mood.update_mood();
    true
}

/// tg `clear_personalities`.
pub fn clear_personalities(world: &mut World, mob: Entity) {
    for def in PERSONALITIES {
        remove_from_mob(world, mob, def.personality);
    }
}

/// `/datum/preference/personality/apply_to_human`: the client's chosen personalities onto its new
/// human.
pub fn assign(world: &mut World, client: Entity, mob: Entity) {
    let Some(prefs) = world.get::<PersonalityPrefs>(client) else {
        return;
    };
    let keys = sanitize(&prefs.0);
    clear_personalities(world, mob);
    for key in keys {
        if let Some(personality) = Personality::by_key(&key) {
            apply_to_mob(world, mob, personality);
        }
    }
}

/// tg `HAS_PERSONALITY` on a mob.
pub fn has(world: &World, mob: Entity, personality: Personality) -> bool {
    world
        .get::<Mood>(mob)
        .is_some_and(|m| m.has_personality(personality))
}

// --- Mood event reactions -------------------------------------------------------------------

/// `MOOD_EVENT_*` flags.
pub const MOOD_EVENT_FEAR: u8 = 1 << 0;
pub const MOOD_EVENT_ART: u8 = 1 << 1;
pub const MOOD_EVENT_WHIMSY: u8 = 1 << 2;
pub const MOOD_EVENT_GAMING: u8 = 1 << 3;
pub const MOOD_EVENT_FOOD: u8 = 1 << 4;
pub const MOOD_EVENT_PAIN: u8 = 1 << 5;
pub const MOOD_EVENT_SPIRITUAL: u8 = 1 << 6;

/// The `event_flags` of a mood event type by its id (the ported types and those waiting for
/// their sources).
pub fn event_flags(id: &str) -> u8 {
    match id {
        "on_fire"
        | "suffocation"
        | "photophobia"
        | "nyctophobia"
        | "claustrophobia"
        | "choke"
        | "startled"
        | "phobia"
        | "spooked"
        | "gunpoint"
        | "gates_of_mansus"
        | "surgery"
        | "moon_insanity"
        | "amulet_insanity"
        | "paranoid_one_on_one"
        | "paranoid_large_group" => MOOD_EVENT_FEAR,
        // `/datum/mood_event/food` has the flag too, but its `add_effects` sets the change after
        // the multipliers (`food_mood_change`), so it is left out.
        "favorite_food"
        | "gross_food"
        | "disgusting_food"
        | "allergic_food"
        | "breakfast"
        | "pacifist_eating_fish_item" => MOOD_EVENT_FOOD,
        "artbad" | "artok" | "artgood" | "artgreat" | "goodmusic" => MOOD_EVENT_ART,
        "honk"
        | "creampie"
        | "slipped"
        | "high_ten"
        | "down_low"
        | "observed_soda_spill"
        | "fish_petting"
        | "prophat"
        | "whimsical_slip"
        | "it_was_on_the_mouse" => MOOD_EVENT_WHIMSY,
        "lost_52_card_pickup" | "won_52_card_pickup" | "basketball_score" | "basketball_dunk" => {
            MOOD_EVENT_WHIMSY | MOOD_EVENT_GAMING
        }
        "arcade" | "gaming" | "gamer_won" | "gamer_withdrawal" | "gamer_lost" | "playing_cards"
        | "slots" | "slots_win" | "slots_loss" => MOOD_EVENT_GAMING,
        "reattachment" | "table_limbsmash" | "painful_medicine" | "back_pain" => MOOD_EVENT_PAIN,
        "blessing" | "religiously_comforted" | "sacrifice_good" => MOOD_EVENT_SPIRITUAL,
        _ => 0,
    }
}

/// `/datum/mood_event/can_effect_mob`'s personality checks: whimsical events please only the
/// whimsical (and their bad side spares them), art is lost on the unimaginative.
pub fn can_effect(personalities: u64, event: &ActiveMood) -> bool {
    let has = |p: Personality| personalities & p.bit() != 0;
    let flags = event_flags(event.id);
    if flags & MOOD_EVENT_WHIMSY != 0 {
        let whimsical = has(P::Whimsical);
        if event.mood_change >= 0.0 && !whimsical {
            return false;
        }
        if event.mood_change < 0.0 && whimsical {
            return false;
        }
    }
    !(flags & MOOD_EVENT_ART != 0 && has(P::Unimaginative))
}

/// `/datum/mood_event/on_add`'s flag multipliers, then the personality branches of the events'
/// `add_effects` (hugs, books). Runs before the change is floored.
pub fn on_add(personalities: u64, event: &mut ActiveMood) {
    let has = |p: Personality| personalities & p.bit() != 0;
    let flags = event_flags(event.id);
    if flags & MOOD_EVENT_ART != 0 && has(P::Creative) {
        event.mood_change *= 1.2;
    }
    // `TRAIT_SPIRITUAL` comes from the Spiritual personality only.
    if flags & MOOD_EVENT_SPIRITUAL != 0 && !has(P::Spiritual) {
        event.mood_change *= 0.2;
    }
    if flags & MOOD_EVENT_FOOD != 0 {
        if has(P::Ascetic) {
            event.mood_change *= 0.75;
        }
        if has(P::Gourmand) {
            event.mood_change *= 1.25;
        }
    }
    if flags & MOOD_EVENT_FEAR != 0 {
        if has(P::Cowardly) {
            event.mood_change *= 1.25;
        }
        if has(P::Brave) {
            event.mood_change *= 0.75;
        }
    }
    let (aloof, callous) = (has(P::Aloof), has(P::Callous));
    let id = event.id;
    let mut set = |change: f32, text: &str| {
        event.mood_change = change;
        event.description = text.to_owned();
    };
    match id {
        "hug" if aloof => set(-1.0, "Я бы хотел, чтобы меня не трогали."),
        "hug" if callous => set(0.0, "Ненавижу обниматься."),
        "warmhug" if aloof => set(
            0.0,
            "Я бы предпочел, чтобы ко мне не прикасались, но тепло - это приятно....",
        ),
        "warmhug" if callous => set(0.0, "Ненавижу обнимашки. Я могу согреть себя самого."),
        "betterhug" if aloof => set(
            1.0,
            "Это приятный человек, но мне хотелось бы, чтобы он перестал меня трогать",
        ),
        "betterhug" if callous => set(0.0, "Этот человек слишком приятный для этой станции."),
        "besthug" if aloof => set(
            2.0,
            "Приятно находиться с этим человеком, но хочу, чтобы меня перестали трогать.",
        ),
        "besthug" if callous => set(0.0, "Этот человек слишком хорош для этой станции."),
        "book_nerd" if has(P::Uneducated) => set(-1.0, "Кому есть дело до этих книг?"),
        "book_nerd" if has(P::Erudite) => set(2.0, "Я люблю читать книги!"),
        _ => {}
    }
}

/// `/datum/mood_event/drunk/update_change`'s personality part: the teetotal hate it, the bibulous
/// love it more.
pub fn drunk_mood(
    personalities: u64,
    change: f32,
    description: &'static str,
) -> (f32, &'static str) {
    if personalities & P::Teetotal.bit() != 0 {
        return (
            change * -1.5,
            "Я не люблю пить... Из-за этого я чувствую себя ужасно.",
        );
    }
    if personalities & P::Bibulous.bit() != 0 {
        return (change * 1.5, description);
    }
    (change, description)
}

/// `/datum/mood_event/food/calculate_mood_change`: `ceil(1 + 1.5 * quality)`, halved for the
/// ascetic; for the gourmand ordinary food (up to `FOOD_QUALITY_GOOD`) is only normal.
pub fn food_mood_change(personalities: u64, quality: i32) -> f32 {
    let mut value = 1.0 + 1.5 * quality as f32;
    if personalities & P::Ascetic.bit() != 0 {
        value *= 0.5;
    }
    // `FOOD_QUALITY_GOOD`, `FOOD_QUALITY_NORMAL`.
    if personalities & P::Gourmand.bit() != 0 && value <= 3.0 {
        value = 1.0;
    }
    value.ceil()
}

/// `/datum/mood_event/conditional/see_death`'s personality branches: the callous do not care
/// (`dontcare`: nothing, half as long), the compassionate feel it half again as much and as long.
pub fn see_death(personalities: u64, event: &mut ActiveMood, descriptor: &str) {
    if personalities & P::Callous.bit() != 0 {
        event.mood_change = 0.0;
        event.timeout *= 0.5;
        event.description = format!("Оу, {descriptor} умер. Досадно, наверное.");
        event.stacks = false;
        return;
    }
    if event.mood_change < 0.0 && personalities & P::Compassionate.bit() != 0 {
        event.mood_change *= 1.5;
        event.timeout *= 1.5;
    }
}

/// `/datum/component/fearful/get_fear_multiplier`.
pub fn fear_multiplier(world: &World, mob: Entity) -> f32 {
    let Some(mood) = world.get::<Mood>(mob) else {
        return 1.0;
    };
    let mut multiplier = 1.0;
    if mood.has_personality(P::Cowardly) {
        multiplier *= 1.25;
    }
    if mood.has_personality(P::Paranoid) {
        multiplier *= 1.10;
    }
    if mood.has_personality(P::Brave) {
        multiplier *= 0.75;
    }
    multiplier
}

/// `/datum/mood_event/bad_touch`, `very_bad_touch`.
pub const BAD_TOUCH: MoodEvent = MoodEvent {
    id: "bad_touch",
    description: "Я не люблю, когда меня трогают.",
    mood_change: -3.0,
    timeout: 240.0,
    hidden: false,
};
pub const VERY_BAD_TOUCH: MoodEvent = MoodEvent {
    id: "very_bad_touch",
    description: "Я очень не люблю, когда меня трогают.",
    mood_change: -5.0,
    timeout: 240.0,
    hidden: false,
};

/// Aloof's `uncomfortable_touch` (`COMSIG_LIVING_GET_PULLED`, `COMSIG_CARBON_HELP_ACT`).
pub fn uncomfortable_touch(world: &mut World, mob: Entity) {
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    let Some(mood) = world.get::<Mood>(mob) else {
        return;
    };
    if !mood.has_personality(P::Aloof) {
        return;
    }
    let event = if mood.sanity <= SANITY_NEUTRAL {
        &VERY_BAD_TOUCH
    } else {
        &BAD_TOUCH
    };
    crate::mood::add_event(world, mob, "bad_touch", event);
}

fn on_pulled(add: On<Add<crate::pull::PulledBy>>, mut commands: Commands) {
    let mob = add.entity;
    commands.queue(move |world: &mut World| uncomfortable_touch(world, mob));
}

// --- Processing ------------------------------------------------------------------------------

const PARANOIA_CATEGORY: &str = "paranoia_personality";
const NT_CATEGORY: &str = "nt_personality";
const LAZY_CATEGORY: &str = "moodlet_in_area_slacking_off_lazy";
const DILIGENT_SLACK_CATEGORY: &str = "moodlet_in_area_slacking_off_diligent";
const DILIGENT_WORK_CATEGORY: &str = "moodlet_in_area_working_diligent";
const DEPARTMENT_CATEGORY: &str = "moodlet_in_area_enjoying_department_area";
/// `check_area_mood`'s category.

const fn event(id: &'static str, description: &'static str, mood_change: f32) -> MoodEvent {
    MoodEvent {
        id,
        description,
        mood_change,
        timeout: 0.0,
        hidden: false,
    }
}

pub const PARANOID_ALONE: MoodEvent = event(
    "paranoid_alone",
    "Тишина и покой, вокруг никого, кто мог бы мне угрожать.",
    1.0,
);
pub const PARANOID_ONE_ON_ONE: MoodEvent = event(
    "paranoid_one_on_one",
    "Я с кем-то наедине - что, если они захотят меня убить?",
    -3.0,
);
pub const PARANOID_SMALL_GROUP: MoodEvent = event(
    "paranoid_small_group",
    "В этой маленькой группе я чувствую себя в большей безопасности. Мы прикрываем спины друг друга.",
    2.0,
);
pub const PARANOID_LARGE_GROUP: MoodEvent = event(
    "paranoid_large_group",
    "Вокруг так много людей - любой из них может напасть на меня!",
    -3.0,
);
pub const NT_LOYALIST: MoodEvent = event(
    "nt_loyalist",
    "Я горжусь тем, что являюсь частью семьи НТ™!",
    2.0,
);
pub const NT_DISILLUSIONED: MoodEvent = event(
    "nt_disillusioned",
    "Я ненавижу эту компанию и всё, что она олицетворяет.",
    -2.0,
);
pub const SLACKING_OFF_LAZY: MoodEvent = event(
    "slacking_off_lazy",
    "Шеф бабки гребёт, я копейки считаю. Вот почему я бью баклуши.",
    1.0,
);
pub const SLACKING_OFF_DILIGENT: MoodEvent = event(
    "slacking_off_diligent",
    "Я должен вернуться к своей работе.",
    -1.0,
);
pub const WORKING_DILIGENT: MoodEvent = event(
    "working_diligent",
    "Усердный труд сам по себе является наградой.",
    1.0,
);
pub const ENJOYING_DEPARTMENT_AREA: MoodEvent =
    event("enjoying_department_area", "Я люблю свою работу.", 1.0);

/// `/datum/personality/slacking` `slacker_areas`.
const SLACKER_AREAS: [&str; 7] = [
    "/area/station/commons/fitness",
    "/area/station/commons/lounge",
    "/area/station/service/bar",
    "/area/station/service/cafeteria",
    "/area/station/service/library",
    "/area/station/service/minibar",
    "/area/station/service/theater",
];
/// `/datum/job_department/*::primary_work_area`.
const WORK_AREAS: [(&str, &str); 8] = [
    ("/datum/job_department/captain", "/area/station/command"),
    ("/datum/job_department/command", "/area/station/command"),
    ("/datum/job_department/security", "/area/station/security"),
    (
        "/datum/job_department/engineering",
        "/area/station/engineering",
    ),
    ("/datum/job_department/medical", "/area/station/medical"),
    ("/datum/job_department/science", "/area/station/science"),
    ("/datum/job_department/cargo", "/area/station/cargo"),
    ("/datum/job_department/service", "/area/station/service"),
];
/// Analytical's and Impulsive's `applicable_areas`.
const ANALYTICAL_AREAS: [&str; 3] = [
    "/area/station/engineering",
    "/area/station/science",
    "/area/station/medical",
];
const IMPULSIVE_AREAS: [&str; 4] = [
    "/area/station/cargo",
    "/area/station/command",
    "/area/station/security",
    "/area/station/service",
];
/// `/obj/structure/sign/nanotrasen`, `/obj/structure/sign/poster/official/nanotrasen_logo`.
const NT_SIGNS: [&str; 2] = [
    "/obj/structure/sign/nanotrasen",
    "/obj/structure/sign/poster/official/nanotrasen_logo",
];

/// What a personality tick reads about one mob.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surroundings {
    /// Conscious humans in view other than the mob (Paranoid's `is_dangerous_mob`).
    pub people: usize,
    pub sees_nt_sign: bool,
    /// The mob's area type.
    pub area: Option<String>,
    /// `mind.get_work_areas()`.
    pub work_areas: Vec<&'static str>,
    /// The job is of the service department (`is_slacking`).
    pub service_worker: bool,
}

fn in_any(area: &str, roots: &[&str]) -> bool {
    roots.iter().any(|r| is_under(area, r))
}

/// One fire of the processing personalities and the area moodlets on `mood` at time `now`.
/// `conscious`: the processing ones skip unconscious mobs (tg `IS_UNCONSCIOUS`).
pub fn tick(mood: &mut Mood, now: f64, conscious: bool, around: &Surroundings) {
    let set = mood.personalities;
    let has = |p: Personality| set & p.bit() != 0;
    if conscious && has(P::Paranoid) {
        let event = match around.people {
            0 => &PARANOID_ALONE,
            1 => &PARANOID_ONE_ON_ONE,
            // "6 people is roughly the size of the larger jobs like meddoc or secoff".
            2..=6 => &PARANOID_SMALL_GROUP,
            _ => &PARANOID_LARGE_GROUP,
        };
        mood.add_event(now, PARANOIA_CATEGORY, event);
    }
    if conscious && (has(P::Loyalist) || has(P::Disillusioned)) {
        if around.sees_nt_sign {
            let event = if has(P::Loyalist) {
                &NT_LOYALIST
            } else {
                &NT_DISILLUSIONED
            };
            mood.add_event(now, NT_CATEGORY, event);
        } else if mood.get(NT_CATEGORY).is_some() {
            mood.clear_event(NT_CATEGORY);
        }
    }
    let area = around.area.as_deref().unwrap_or("");
    let follow = |mood: &mut Mood, on: bool, category: &str, event: &MoodEvent| {
        if on {
            mood.add_event(now, category, event);
        } else if mood.get(category).is_some() {
            mood.clear_event(category);
        }
    };
    // `/datum/personality/slacking/is_slacking`: service workers do not slack in service.
    let slacking = in_any(area, &SLACKER_AREAS)
        && !(is_under(area, "/area/station/service") && around.service_worker);
    if has(P::Lazy) {
        follow(mood, slacking, LAZY_CATEGORY, &SLACKING_OFF_LAZY);
    }
    if has(P::Diligent) {
        follow(
            mood,
            slacking,
            DILIGENT_SLACK_CATEGORY,
            &SLACKING_OFF_DILIGENT,
        );
        let working = in_any(area, &around.work_areas);
        follow(mood, working, DILIGENT_WORK_CATEGORY, &WORKING_DILIGENT);
    }
    for (personality, applicable) in [
        (P::Analytical, &ANALYTICAL_AREAS[..]),
        (P::Impulsive, &IMPULSIVE_AREAS[..]),
    ] {
        if has(personality) {
            // `applicable_areas & get_work_areas()`.
            let areas: Vec<&str> = around
                .work_areas
                .iter()
                .copied()
                .filter(|w| applicable.contains(w))
                .collect();
            follow(
                mood,
                in_any(area, &areas),
                DEPARTMENT_CATEGORY,
                &ENJOYING_DEPARTMENT_AREA,
            );
        }
    }
}

fn area_path(world: &World, pos: TilePos) -> Option<String> {
    let area = world.get_resource::<ss13_mapload::Areas>()?.area_at(pos)?;
    world
        .get::<ss13_mapload::DmPower>(area)
        .map(|p| p.path.clone())
}

/// `mind.get_work_areas()` of the mob's job, and whether it is a service job.
fn work_areas(world: &World, mob: Entity) -> (Vec<&'static str>, bool) {
    let (Some(job), Some(jobs)) = (
        world.get::<crate::jobs::PlayerJob>(mob),
        world.get_resource::<crate::jobs::Jobs>(),
    ) else {
        return (Vec::new(), false);
    };
    let mut areas = Vec::new();
    let mut service = false;
    for department in &jobs.data.departments {
        if !department.jobs.iter().any(|&j| usize::from(j) == job.0) {
            continue;
        }
        service |= department.path == "/datum/job_department/service";
        if let Some(&(_, area)) = WORK_AREAS.iter().find(|(d, _)| *d == department.path)
            && !areas.contains(&area)
        {
            areas.push(area);
        }
    }
    (areas, service)
}

fn surroundings(world: &World, mob: Entity, pos: TilePos, set: u64) -> Surroundings {
    let wants = |p: Personality| set & p.bit() != 0;
    let mut around = Surroundings::default();
    let nearby = move || {
        world
            .resource::<SpatialIndex>()
            .in_radius(pos, VIEW_RANGE)
            .filter(move |&e| e != mob)
            .filter(move |&e| {
                world.get::<TilePos>(e).is_some_and(|&at| {
                    crate::mobs::ai::behaviors::can_see(world, pos, at, VIEW_RANGE)
                })
            })
    };
    if wants(P::Paranoid) {
        around.people = nearby()
            .filter(|&e| {
                world.get::<HumanLook>(e).is_some()
                    && matches!(world.get::<Stat>(e), Some(Stat::Conscious | Stat::SoftCrit))
            })
            .count();
    }
    if wants(P::Loyalist) || wants(P::Disillusioned) {
        around.sees_nt_sign = nearby().any(|e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| NT_SIGNS.iter().any(|s| is_under(&o.path, s)))
        });
    }
    around.area = area_path(world, pos);
    (around.work_areas, around.service_worker) = work_areas(world, mob);
    around
}

pub struct PersonalityServerPlugin;

impl Plugin for PersonalityServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_pulled).add_systems(
            Update,
            personality_life.before(crate::health::HealthSystems),
        );
    }
}

/// `SSpersonalities`: the processing personalities and the area moodlets, every 3 s.
// Cost: 1/3 Hz, one pass over humans with personalities (one per player); Paranoid and the NT
// ones scan a radius of 5 once each.
fn personality_life(world: &mut World, mut clock: Local<f64>) {
    *clock += world.resource::<Time>().delta_secs_f64();
    if *clock < PERSONALITIES_WAIT_SECS {
        return;
    }
    *clock %= PERSONALITIES_WAIT_SECS;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &Mood, &Stat, &TilePos)>();
    let mobs: Vec<(Entity, u64, Stat, TilePos)> = query
        .iter(world)
        .filter(|(_, m, s, _)| m.personalities != 0 && **s != Stat::Dead)
        .map(|(e, m, s, p)| (e, m.personalities, *s, *p))
        .collect();
    for (mob, set, stat, pos) in mobs {
        let around = surroundings(world, mob, pos, set);
        let conscious = matches!(stat, Stat::Conscious | Stat::SoftCrit);
        let Some(mut mood) = world.get_mut::<Mood>(mob) else {
            continue;
        };
        let mut new = mood.clone();
        tick(&mut new, now, conscious, &around);
        mood.set_if_neq(new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mood::events;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|k| (*k).to_owned()).collect()
    }

    fn with(personalities: &[Personality]) -> Mood {
        let mut world = World::new();
        let mob = world.spawn(Mood::default()).id();
        for &p in personalities {
            assert!(apply_to_mob(&mut world, mob, p));
        }
        world.get::<Mood>(mob).unwrap().clone()
    }

    #[test]
    fn keys_are_unique_and_groups_exclude() {
        for (i, a) in PERSONALITIES.iter().enumerate() {
            assert!(PERSONALITIES[i + 1..].iter().all(|b| b.key != a.key));
            assert_eq!(Personality::by_key(a.key), Some(a.personality));
        }
        assert!(PERSONALITIES.len() <= 64);
        assert!(is_incompatible(&[P::Apathetic], P::Sensitive));
        assert!(is_incompatible(&[P::Brave], P::Paranoid));
        assert!(!is_incompatible(&[P::Brave], P::Apathetic));
        // No groups: compatible with everything.
        assert!(!is_incompatible(&[P::Snob, P::Aloof], P::Whimsical));
    }

    #[test]
    fn prefs_sanitize_and_toggle() {
        // Unknown and incompatible keys go, the rest past the limit too.
        let input = keys(&[
            "apathetic",
            "nonsense",
            "sensitive",
            "brave",
            "paranoid",
            "hopeful",
            "snob",
            "aloof",
            "gambler",
            "whimsical",
        ]);
        assert_eq!(
            sanitize(&input),
            keys(&["apathetic", "brave", "hopeful", "snob", "aloof"])
        );
        let mut prefs = PersonalityPrefs::default();
        assert!(toggle_prefs(&mut prefs, "apathetic"));
        assert!(toggle_prefs(&mut prefs, "sensitive"));
        assert_eq!(prefs.0, keys(&["apathetic"]));
        assert!(!toggle_prefs(&mut prefs, "nonsense"));
        for key in ["brave", "hopeful", "snob", "aloof", "gambler"] {
            toggle_prefs(&mut prefs, key);
        }
        assert_eq!(prefs.0.len(), MAX_PERSONALITIES);
        assert!(!prefs.0.contains(&"gambler".to_owned()));
        // A chosen key comes off.
        assert!(toggle_prefs(&mut prefs, "apathetic"));
        assert!(!prefs.0.contains(&"apathetic".to_owned()));
    }

    #[test]
    fn assign_applies_the_prefs() {
        let mut world = World::new();
        let client = world
            .spawn(PersonalityPrefs(keys(&[
                "sensitive",
                "apathetic",
                "loyalist",
            ])))
            .id();
        let mob = world.spawn(Mood::default()).id();
        assign(&mut world, client, mob);
        assert!(has(&world, mob, P::Sensitive));
        assert!(!has(&world, mob, P::Apathetic));
        assert!(has(&world, mob, P::Loyalist));
        assert!(!apply_to_mob(&mut world, mob, P::Sensitive));
        assert!(remove_from_mob(&mut world, mob, P::Sensitive));
        assert!(!remove_from_mob(&mut world, mob, P::Sensitive));
        assert_eq!(world.get::<Mood>(mob).unwrap().mood_modifier, 1.0);
    }

    #[test]
    fn generic_reactions_scale_power_and_length() {
        let mut mood = with(&[P::Sensitive, P::Brooding, P::Pessimistic]);
        mood.add_event(0.0, "depression", &events::DEPRESSION);
        // -12 * 1.2, 120 s * 1.2.
        assert!((mood.mood + 14.4).abs() < 1e-4, "{}", mood.mood);
        assert!(close(
            mood.get("depression").unwrap().expires.unwrap(),
            144.0
        ));
        mood.add_event(0.0, "jolly", &events::JOLLY);
        assert!(close(mood.get("jolly").unwrap().expires.unwrap(), 96.0));
        let mut mood = with(&[P::Apathetic, P::Resilient, P::Hopeful]);
        mood.add_event(0.0, "depression", &events::DEPRESSION);
        assert!((mood.mood + 9.6).abs() < 1e-4, "{}", mood.mood);
        assert!(close(
            mood.get("depression").unwrap().expires.unwrap(),
            96.0
        ));
        mood.add_event(0.0, "jolly", &events::JOLLY);
        assert!(close(mood.get("jolly").unwrap().expires.unwrap(), 144.0));
        // Whimsy: a honk pleases only the whimsical.
        let honk = MoodEvent {
            id: "honk",
            description: "Меня захонкали!",
            mood_change: 2.0,
            timeout: 240.0,
            hidden: false,
        };
        let mut plain = Mood::default();
        plain.add_event(0.0, "honk", &honk);
        assert!(plain.get("honk").is_none());
        let mut whimsical = with(&[P::Whimsical]);
        whimsical.add_event(0.0, "honk", &honk);
        assert_eq!(whimsical.get("honk").unwrap().mood_change, 2.0);
    }

    #[test]
    fn recreation_and_work_reactions() {
        let book = MoodEvent {
            id: "book_nerd",
            description: "Я недавно прочитал книгу.",
            mood_change: 1.0,
            timeout: 300.0,
            hidden: false,
        };
        let mut erudite = with(&[P::Erudite]);
        erudite.add_event(0.0, "book_nerd", &book);
        assert_eq!(erudite.get("book_nerd").unwrap().mood_change, 2.0);
        let mut uneducated = with(&[P::Uneducated]);
        uneducated.add_event(0.0, "book_nerd", &book);
        assert_eq!(uneducated.get("book_nerd").unwrap().mood_change, -1.0);
        // A blessing barely touches the non-spiritual.
        let blessing = MoodEvent {
            id: "blessing",
            description: "Меня благословили.",
            mood_change: 5.0,
            timeout: 480.0,
            hidden: false,
        };
        let mut plain = Mood::default();
        plain.add_event(0.0, "blessing", &blessing);
        assert_eq!(plain.get("blessing").unwrap().mood_change, 1.0);
        let mut spiritual = with(&[P::Spiritual]);
        spiritual.add_event(0.0, "blessing", &blessing);
        assert_eq!(spiritual.get("blessing").unwrap().mood_change, 5.0);
        // Slacking in the bar, working in the department.
        let in_bar = Surroundings {
            area: Some("/area/station/service/bar".into()),
            work_areas: vec!["/area/station/engineering"],
            ..default()
        };
        let mut lazy = with(&[P::Lazy]);
        tick(&mut lazy, 0.0, true, &in_bar);
        assert_eq!(lazy.get(LAZY_CATEGORY).unwrap().mood_change, 1.0);
        let mut diligent = with(&[P::Diligent, P::Analytical]);
        tick(&mut diligent, 0.0, true, &in_bar);
        assert_eq!(
            diligent.get(DILIGENT_SLACK_CATEGORY).unwrap().mood_change,
            -1.0
        );
        assert!(diligent.get(DILIGENT_WORK_CATEGORY).is_none());
        let at_work = Surroundings {
            area: Some("/area/station/engineering/main".into()),
            ..in_bar.clone()
        };
        tick(&mut diligent, 3.0, true, &at_work);
        assert!(diligent.get(DILIGENT_SLACK_CATEGORY).is_none());
        assert!(diligent.get(DILIGENT_WORK_CATEGORY).is_some());
        assert!(diligent.get(DEPARTMENT_CATEGORY).is_some());
        // A bartender does not slack in the bar.
        let bartender = Surroundings {
            service_worker: true,
            work_areas: vec!["/area/station/service"],
            ..in_bar.clone()
        };
        let mut lazy = with(&[P::Lazy, P::Impulsive]);
        tick(&mut lazy, 0.0, true, &bartender);
        assert!(lazy.get(LAZY_CATEGORY).is_none());
        assert!(lazy.get(DEPARTMENT_CATEGORY).is_some());
        // Corporate signs.
        let mut loyalist = with(&[P::Loyalist]);
        let sign = Surroundings {
            sees_nt_sign: true,
            ..default()
        };
        tick(&mut loyalist, 0.0, true, &sign);
        assert_eq!(loyalist.get(NT_CATEGORY).unwrap().mood_change, 2.0);
        tick(&mut loyalist, 3.0, true, &Surroundings::default());
        assert!(loyalist.get(NT_CATEGORY).is_none());
        let mut disillusioned = with(&[P::Disillusioned]);
        tick(&mut disillusioned, 0.0, true, &sign);
        assert_eq!(disillusioned.get(NT_CATEGORY).unwrap().mood_change, -2.0);
    }

    #[test]
    fn reactions_to_people() {
        let mut paranoid = with(&[P::Paranoid]);
        for (people, change) in [(0, 1.0), (1, -3.0), (4, 2.0), (7, -3.0)] {
            let around = Surroundings {
                people,
                ..default()
            };
            tick(&mut paranoid, 0.0, true, &around);
            assert_eq!(paranoid.get(PARANOIA_CATEGORY).unwrap().mood_change, change);
        }
        // Hugs: the aloof dislike them, the callous do not care.
        let mut aloof = with(&[P::Aloof]);
        aloof.add_event(0.0, "hug", &events::HUG);
        assert_eq!(aloof.get("hug").unwrap().mood_change, -1.0);
        let mut callous = with(&[P::Callous]);
        callous.add_event(0.0, "hug", &events::BESTHUG);
        assert_eq!(callous.get("hug").unwrap().mood_change, 0.0);
        // Deaths: nothing for the callous, more for the compassionate.
        let mut death = crate::mood::see_death_event("кто-то");
        see_death(P::Callous.bit(), &mut death, "кто-то");
        assert_eq!((death.mood_change, death.timeout), (0.0, 150.0));
        let mut death = crate::mood::see_death_event("кто-то");
        see_death(P::Compassionate.bit(), &mut death, "кто-то");
        assert_eq!((death.mood_change, death.timeout), (-12.0, 450.0));
        // Aloof: touching makes them sad.
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn((Mood::default(), Stat::Conscious)).id();
        uncomfortable_touch(&mut world, mob);
        assert!(world.get::<Mood>(mob).unwrap().get("bad_touch").is_none());
        apply_to_mob(&mut world, mob, P::Aloof);
        uncomfortable_touch(&mut world, mob);
        // Neutral sanity (100): the very bad one.
        assert_eq!(
            world
                .get::<Mood>(mob)
                .unwrap()
                .get("bad_touch")
                .unwrap()
                .mood_change,
            -5.0
        );
    }

    #[test]
    fn fear_art_food_and_drink() {
        let fear = MoodEvent {
            id: "phobia",
            description: "",
            mood_change: -12.0,
            timeout: 0.0,
            hidden: false,
        };
        let mut brave = with(&[P::Brave]);
        brave.add_event(0.0, "phobia", &fear);
        assert_eq!(brave.get("phobia").unwrap().mood_change, -9.0);
        let mut coward = with(&[P::Cowardly]);
        coward.add_event(0.0, "phobia", &fear);
        assert_eq!(coward.get("phobia").unwrap().mood_change, -15.0);
        let mut world = World::new();
        let mob = world.spawn(Mood::default()).id();
        assert_eq!(fear_multiplier(&world, mob), 1.0);
        apply_to_mob(&mut world, mob, P::Cowardly);
        assert_eq!(fear_multiplier(&world, mob), 1.25);
        // Art: more for the creative, nothing for the unimaginative.
        let art = MoodEvent {
            id: "artgood",
            description: "",
            mood_change: 5.0,
            timeout: 0.0,
            hidden: false,
        };
        let mut creative = with(&[P::Creative]);
        creative.add_event(0.0, "art", &art);
        assert_eq!(creative.get("art").unwrap().mood_change, 6.0);
        let mut unimaginative = with(&[P::Unimaginative]);
        unimaginative.add_event(0.0, "art", &art);
        assert!(unimaginative.get("art").is_none());
        // Food.
        assert_eq!(food_mood_change(0, 2), 4.0);
        assert_eq!(food_mood_change(P::Ascetic.bit(), 2), 2.0);
        assert_eq!(food_mood_change(P::Gourmand.bit(), 1), 1.0);
        assert_eq!(food_mood_change(P::Gourmand.bit(), 4), 7.0);
        let mut gourmand = with(&[P::Gourmand]);
        gourmand.add_event(0.0, "gross_food", &events::DISGUSTING_FOOD);
        // -6 * 1.25, floored.
        assert_eq!(gourmand.get("gross_food").unwrap().mood_change, -8.0);
        // Drink.
        assert_eq!(drunk_mood(P::Teetotal.bit(), 4.0, "x").0, -6.0);
        assert_eq!(drunk_mood(P::Bibulous.bit(), 4.0, "x"), (6.0, "x"));
        assert_eq!(drunk_mood(0, 4.0, "x"), (4.0, "x"));
    }
}
