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
//! (`healthy`), muscle weakness, nervous cough, possessiveness, achromatopsia; severe — mute,
//! paralysis, pacifism, kleptomania, cortical blindness (both read by `crate::vision`). Random
//! gains pick among all of tg's types ([`TG_POOL`]): an unported pick gives nothing, so each
//! ported trauma comes as often as in tg.
// shortcut: no hallucinations, muscle spasms, expressive aphasia (needs the common words list),
// mind echo (needs hearing hooks), aphasia (no languages), narcolepsy and hypnotic stupor (no
// sleep or trance), achromatopsia's `TRAIT_COLORBLIND` (wire colours),
// monophobia, discoordination, dyslexia, hypnotic triggers, split personality, imaginary friend,
// obsession, special and magic traumas, phobias; no forced brain damage lines of dumbness
// (`traumas.json` is not converted), no fake health HUD, mood events or `TRAIT_DUMB`;
// kleptomania ignores the damage
// cooldown; an inserted brain applies its traumas without their gain messages; no head-wound
// (`TRAUMA_RESILIENCE_WOUND`) traumas, no `TRAIT_SPECIAL_TRAUMA_BOOST`. Add each with the
// system it needs.

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

    fn gain_text(self) -> (Span, String) {
        match self {
            Self::Paralysis(p) => (
                Span::Warning,
                format!("Вы больше не чувствуете {}!", p.subject()),
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
            _ => (self.def().lose.0, self.def().lose.1.to_owned()),
        }
    }
}

/// tg's randomly gainable trauma types (`subtypesof(category)` with `random_gain` and
/// `can_gain`, the abstract roots out): type, the ported kind (a paralysis of a placeholder
/// kind), category, `initial(resilience)`. Random gains pick among all of them, so each ported
/// trauma comes as often as in tg.
// shortcut: a pick of an unported type gives no trauma; add each type's kind as it is ported.
pub const TG_POOL: [(&str, Option<Kind>, Category, Resilience); 37] = {
    use Category::{Mild, Severe, Special};
    use Resilience::{Basic, Surgery};
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
        ("/datum/brain_trauma/mild/hallucinations", None, Mild, Basic),
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
        ("/datum/brain_trauma/mild/phobia", None, Mild, Basic),
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
        ("/datum/brain_trauma/severe/aphasia", None, Severe, Surgery),
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
        ("/datum/brain_trauma/severe/dyslexia", None, Severe, Surgery),
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
            None,
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
            None,
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/narcolepsy/permanent",
            None,
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
            None,
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/special/bluespace_prophet",
            None,
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/death_whispers",
            None,
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/existential_crisis",
            None,
            Special,
            Basic,
        ),
        ("/datum/brain_trauma/special/godwoken", None, Special, Basic),
        (
            "/datum/brain_trauma/special/imaginary_friend",
            None,
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
            None,
            Special,
            Basic,
        ),
        ("/datum/brain_trauma/special/tenacity", None, Special, Basic),
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
    let trauma = Trauma {
        kind,
        resilience,
        possessed: Vec::new(),
        steal_at: 0.0,
    };
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
    if let Kind::Paralysis(_) = kind {
        kind = Kind::Paralysis(Paralysis::ALL[rng.range(0, 8) as usize]);
    }
    gain(world, brain, kind, resilience).then_some(kind)
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
    if let Some(mob) = owner(world, brain) {
        info!("{mob} lost the brain trauma {}", trauma.kind.def().path);
        tell_owner(world, brain, trauma.kind.lose_text());
        refresh(world, mob);
    }
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
    let all = of_mob(world, mob);
    if all.is_empty() {
        return None;
    }
    let parts: Vec<String> = all
        .iter()
        .map(|t| {
            format!(
                "{} {}",
                t.resilience.scan_word(),
                ss13_chat::speech::capitalize(t.kind.def().scan_desc)
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

/// Applies the traumas of `mob`'s brain to `mob` (the traits of their `on_gain`/`on_lose`), with
/// the quirks that give the same traits.
pub fn refresh(world: &mut World, mob: Entity) {
    if world.get_entity(mob).is_err() {
        return;
    }
    let mut speech = ss13_chat::speech::Impediments::default();
    let mut pacifist = false;
    let mut paralyzed = [false; 6];
    for trauma in of_mob(world, mob) {
        match trauma.kind {
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
    // The Mute (`TRAIT_MUTE`) and Pacifist (`TRAIT_PACIFISM`) quirks (`crate::quirks`).
    if let Some(quirks) = world.get::<crate::quirks::Quirks>(mob) {
        speech.mute |= quirks.has(crate::quirks::Quirk::Mute);
        pacifist |= quirks.has(crate::quirks::Quirk::Pacifist);
    }
    // `/datum/status_effect/pacify` (`crate::status_effects` calls this when it starts or ends).
    pacifist |= world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(crate::status_effects::StatusEffects::pacified);
    // The mime's vow (`TRAIT_MIMING`, `can_speak(allow_mimes = FALSE)`).
    speech.mute |= world.get::<crate::service::MimeVow>(mob).is_some();
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
        on_life(world, mob, &mut trauma, secs, &mut rng);
        if let Some(mut all) = world.get_mut::<Traumas>(brain)
            && let Some(slot) = all.0.get_mut(index)
            && slot.kind == trauma.kind
            && *slot != trauma
        {
            *slot = trauma;
        }
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
        Kind::Kleptomaniac => kleptomania(world, mob, trauma, now, rng, secs),
        _ => {}
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
        // 1/13 each (1000 expected); 8 of 13 unported give nothing.
        assert!((850..1150).contains(&paralysis), "{paralysis}");
        assert!((7_500..8_500).contains(&none), "{none}");
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
