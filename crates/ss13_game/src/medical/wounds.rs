//! Wounds (server), after tg `code/datums/wounds` (`_wounds.dm`, `_wound_static_data.dm`,
//! `slash.dm`, `pierce.dm`, `bones.dm`, `burns.dm`) and the bodypart side
//! (`code/modules/surgery/bodyparts/wounds.dm` `check_wounding`/`check_woundings_mods`,
//! `_bodyparts.dm` `receive_damage`, `refresh_bleed_rate`, `update_disabled`).
//!
//! - A brute or burn hit of at least [`WOUND_MINIMUM_DAMAGE`] rolls `rand(1, damage^1.4)` (damage
//!   capped at 25), adds the attack's `wound_bonus`, the limb's wounds' `threshold_penalty`, the
//!   disabled-limb penalty and the exposed bonus (or the limb's `wound_resistance` as armor), and
//!   picks among the wounds of the hit's type (blunt, slash, pierce, burn) whose threshold it
//!   reaches, the worst severity winning; a worse wound replaces a milder one of its series.
//! - Slashes and pierces bleed (`blood_flow`, see `crate::medical::blood`): slashes clot over
//!   time (critical ones open up), demote to milder cuts and close; gauze soaks the flow away;
//!   sutures and cauteries take flow off at once. Pierces bleed internally when hit again.
//! - Bones: a dislocation (arms and legs) limps and is reset with a bonesetter; a hairline
//!   fracture and a compound fracture limp, the compound one disables the limb until splinted;
//!   bone gel then surgical tape knit them over minutes; chest fractures bleed internally.
//! - Burns: flesh damage heals with ointment, mesh and gauze; severe and critical burns get
//!   infected without sanitization (toxins, a limb that goes limp, sepsis).
//! - Effects on the body ([`Limbs::limits`]): limping legs slow every step on average, disabled
//!   legs slow by 3 deciseconds each and none left floors the mob.
//! - Dismemberment (`dismemberment.dm` `try_dismember`, `check_wounding`'s outright roll): a hit
//!   on a limb whose skin and bone are both mangled rolls its damage plus half the limb's damage
//!   share (+15 with a critical blunt wound) to come off instead of wounding; a roll over
//!   `WOUND_DISMEMBER_OUTRIGHT_THRESH` takes it off outright by the limb's damage share
//!   (`super::bodyparts::apply_dismember`, tg `/datum/wound/loss`).
// shortcut: no cranial fissure, no eye puncture, no
// bruises, scars, brain traumas from skull fractures, wound status effects, actionspeed
// penalties, grasping wounds, embedded objects, blood sprays or splatters; clothing has no
// `wound` armor (the exposed bonus always applies off the head and chest); `determination` from
// `second_wind` is not added; disinfecting reagents (`on_burn_wound_processing`) do nothing.
// shortcut: limping slows every step by the average (`limp_slowdown × limp_chance`, alternating
// legs) instead of rolling per step: `ss13_world::Mover` has no per-step delay hook.

use bevy::prelude::*;
use ss13_mapload::DmMedical;
use ss13_shared::chat::Span;

use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::zone::BodyZone;

/// `WOUND_DAMAGE_EXPONENT`, `WOUND_MAX_CONSIDERED_DAMAGE`, `WOUND_MINIMUM_DAMAGE`.
pub const WOUND_DAMAGE_EXPONENT: f32 = 1.4;
pub const WOUND_MAX_CONSIDERED_DAMAGE: f32 = 25.0;
pub const WOUND_MINIMUM_DAMAGE: f32 = 5.0;
/// `CANT_WOUND`: a `wound_bonus` that never wounds.
pub const CANT_WOUND: f32 = -100.0;
/// `DISMEMBER_MINIMUM_DAMAGE`, `WOUND_DISMEMBER_OUTRIGHT_THRESH`,
/// `WOUND_CRITICAL_BLUNT_DISMEMBER_BONUS`.
pub const DISMEMBER_MINIMUM_DAMAGE: f32 = 10.0;
pub const WOUND_DISMEMBER_OUTRIGHT_THRESH: f32 = 150.0;
const CRITICAL_BLUNT_DISMEMBER_BONUS: f32 = 15.0;
/// `WOUND_MAX_BLOODFLOW`, `WOUND_SLASH_DAMAGE_FLOW_COEFF`, `WOUND_BURN_SANITIZATION_RATE`.
pub const WOUND_MAX_BLOODFLOW: f32 = 4.5;
pub const WOUND_SLASH_DAMAGE_FLOW_COEFF: f32 = 0.025;
pub const WOUND_BURN_SANITIZATION_RATE: f32 = 0.075;
/// `WOUND_INFECTION_*`.
pub const WOUND_INFECTION_MODERATE: f32 = 4.0;
pub const WOUND_INFECTION_SEVERE: f32 = 8.0;
pub const WOUND_INFECTION_CRITICAL: f32 = 12.0;
pub const WOUND_INFECTION_SEPTIC: f32 = 20.0;
/// `SHARP_EDGED`, `SHARP_POINTY`.
pub const SHARP_EDGED: u32 = 1;
pub const SHARP_POINTY: u32 = 2;
/// `UNCLAMPED_VESSELS_BLEEDING`, `CLAMPED_VESSELS_BLEEDING`.
pub const UNCLAMPED_VESSELS_BLEEDING: f32 = 1.5;
pub const CLAMPED_VESSELS_BLEEDING: f32 = 0.2;
/// `NUTRITION_LEVEL_FED`: burns heal on their own above it.
const NUTRITION_LEVEL_FED: f32 = 350.0;
/// `BODYTEMP_NORMAL`: pierces clot faster 10 K below it.
const BODYTEMP_NORMAL: f64 = 310.15;

/// tg `surgery_state` bits of a bodypart (`code/__DEFINES/bodyparts.dm`).
pub mod state {
    pub const SKIN_CUT: u16 = 1 << 0;
    pub const SKIN_OPEN: u16 = 1 << 1;
    pub const VESSELS_UNCLAMPED: u16 = 1 << 2;
    pub const VESSELS_CLAMPED: u16 = 1 << 3;
    pub const ORGANS_CUT: u16 = 1 << 4;
    pub const BONE_DRILLED: u16 = 1 << 5;
    pub const BONE_SAWED: u16 = 1 << 6;
    pub const CAVITY_WIDENED: u16 = 1 << 9;
    pub const ALL_SKIN: u16 = SKIN_CUT | SKIN_OPEN;
    pub const ALL_VESSEL: u16 = VESSELS_UNCLAMPED | VESSELS_CLAMPED;
    pub const ALL_BONE: u16 = BONE_DRILLED | BONE_SAWED;
    /// `ALL_SURGERY_STATES_UNSET_ON_CLOSE`.
    pub const UNSET_ON_CLOSE: u16 = ALL_SKIN | ALL_VESSEL | ALL_BONE | ORGANS_CUT | CAVITY_WIDENED;
}

/// tg `WOUND_BLUNT`, `WOUND_SLASH`, `WOUND_PIERCE`, `WOUND_BURN`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WoundType {
    Blunt,
    Slash,
    Pierce,
    Burn,
}

/// tg `wound_series`: one wound per series and limb, worse ones replace milder ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Series {
    Slash,
    Pierce,
    Bone,
    Burn,
}

/// The randomly generated wounds of a human (tg `/datum/wound_pregen_data` with
/// `can_be_randomly_generated`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WoundKind {
    SlashModerate,
    SlashSevere,
    SlashCritical,
    PierceModerate,
    PierceSevere,
    PierceCritical,
    Dislocation,
    Hairline,
    Compound,
    BurnModerate,
    BurnSevere,
    BurnCritical,
}

/// The static vars of one wound type and its pregen data.
#[derive(Clone, Copy, Debug)]
pub struct WoundDef {
    pub series: Series,
    pub wounding: WoundType,
    /// `WOUND_SEVERITY_MODERATE` 1 .. `CRITICAL` 3.
    pub severity: u8,
    pub threshold_minimum: f32,
    pub threshold_penalty: f32,
    pub series_threshold_penalty: f32,
    pub name: &'static str,
    pub desc: &'static str,
    pub treat_text_short: &'static str,
    pub occur_text: &'static str,
    pub examine_desc: &'static str,
    /// `MANGLES_INTERIOR` / `MANGLES_EXTERIOR`.
    pub mangles_interior: bool,
    pub mangles_exterior: bool,
    pub initial_flow: f32,
    pub minimum_flow: f32,
    pub clot_rate: f32,
    pub gauzed_clot_rate: f32,
    pub internal_bleeding_chance: f32,
    pub internal_bleeding_coefficient: f32,
    pub demotes_to: Option<WoundKind>,
    pub limp_slowdown: f32,
    pub limp_chance: f32,
    pub disabling: bool,
    pub damage_multiplier_penalty: f32,
    pub flesh_damage: f32,
    pub infection_rate: f32,
    pub regen_ticks_needed: u32,
    /// `surgery_states` the wound puts on its limb.
    pub surgery_states: u16,
    /// `BIO_JOINTED` required (arms and legs).
    pub jointed_only: bool,
}

const BASE: WoundDef = WoundDef {
    series: Series::Slash,
    wounding: WoundType::Slash,
    severity: 1,
    threshold_minimum: 0.0,
    threshold_penalty: 5.0,
    series_threshold_penalty: 0.0,
    name: "",
    desc: "",
    treat_text_short: "",
    occur_text: "",
    examine_desc: "",
    mangles_interior: false,
    mangles_exterior: false,
    initial_flow: 0.0,
    minimum_flow: 0.0,
    clot_rate: 0.0,
    gauzed_clot_rate: 0.0,
    internal_bleeding_chance: 0.0,
    internal_bleeding_coefficient: 1.0,
    demotes_to: None,
    limp_slowdown: 0.0,
    limp_chance: 0.0,
    disabling: false,
    damage_multiplier_penalty: 1.0,
    flesh_damage: 0.0,
    infection_rate: 0.0,
    regen_ticks_needed: 0,
    surgery_states: 0,
    jointed_only: false,
};

const SLASH_TREAT: &str = "Примените повязку, швы, средства для свертывания крови или прижигание.";

impl WoundKind {
    pub const ALL: [Self; 12] = [
        Self::SlashModerate,
        Self::SlashSevere,
        Self::SlashCritical,
        Self::PierceModerate,
        Self::PierceSevere,
        Self::PierceCritical,
        Self::Dislocation,
        Self::Hairline,
        Self::Compound,
        Self::BurnModerate,
        Self::BurnSevere,
        Self::BurnCritical,
    ];

    pub fn def(self) -> WoundDef {
        use state::*;
        match self {
            Self::SlashModerate => WoundDef {
                threshold_minimum: 20.0,
                series_threshold_penalty: 10.0,
                name: "Легкий порез",
                desc: "Кожа пациента сильно соскоблена, что приводит к умеренной потере крови.",
                treat_text_short: "Наложите повязку или сделайте шов.",
                occur_text: "разрезается, начиная медленно кровоточить",
                examine_desc: "имеет открытую рану",
                initial_flow: 1.75,
                minimum_flow: 0.5,
                clot_rate: 0.04,
                ..BASE
            },
            Self::SlashSevere => WoundDef {
                severity: 2,
                threshold_minimum: 50.0,
                series_threshold_penalty: 25.0,
                name: "Открытый разрез",
                desc: "Кожа пациента разорвана, что вызывает значительную потерю крови.",
                treat_text_short: SLASH_TREAT,
                occur_text: "разрывается, и вены начинают брызгать кровью",
                examine_desc: "имеет серьезный порез",
                initial_flow: 2.75,
                minimum_flow: 2.0,
                clot_rate: 0.02,
                demotes_to: Some(Self::SlashModerate),
                surgery_states: SKIN_CUT | VESSELS_UNCLAMPED,
                ..BASE
            },
            Self::SlashCritical => WoundDef {
                severity: 3,
                threshold_minimum: 80.0,
                threshold_penalty: 15.0,
                name: "Авульсивный разрез",
                desc: "Кожа пациента полностью разорвана, что приводит к значительной потере ткани. \
                       Экстремальная потеря крови приведет к быстрой смерти без вмешательства.",
                treat_text_short: SLASH_TREAT,
                occur_text: "разрывается, брызгая кровью",
                examine_desc: "рвется до кости, брызгая кровью",
                mangles_exterior: true,
                initial_flow: 3.75,
                minimum_flow: 3.5,
                clot_rate: -0.012,
                demotes_to: Some(Self::SlashSevere),
                surgery_states: SKIN_OPEN | VESSELS_UNCLAMPED,
                ..BASE
            },
            Self::PierceModerate => WoundDef {
                series: Series::Pierce,
                wounding: WoundType::Pierce,
                threshold_minimum: 30.0,
                series_threshold_penalty: 20.0,
                name: "Незначительный прокол",
                desc: "Кожа пациента была повреждена, вызвав сильный синяк и незначительное \
                       внутреннее кровотечение в пораженной области.",
                treat_text_short: SLASH_TREAT,
                occur_text: "выбрызгивает тонкую струю крови",
                examine_desc: "имеет небольшое, слегка кровоточащее, разорванное отверстие",
                initial_flow: 1.25,
                gauzed_clot_rate: 0.75,
                clot_rate: 0.03,
                internal_bleeding_chance: 30.0,
                internal_bleeding_coefficient: 1.5,
                ..BASE
            },
            Self::PierceSevere => WoundDef {
                series: Series::Pierce,
                wounding: WoundType::Pierce,
                severity: 2,
                threshold_minimum: 50.0,
                series_threshold_penalty: 35.0,
                name: "Открытая колотая рана",
                desc: "Внутренние ткани пациента повреждены, что вызывает значительное внутреннее \
                       кровотечение и снижение стабильности конечности.",
                treat_text_short: SLASH_TREAT,
                occur_text: "брызжет сильным фонтаном крови, обнажая проколотую рану",
                examine_desc: "проколота насквозь, с кусочками ткани, закрывающими открытое отверстие",
                initial_flow: 2.0,
                gauzed_clot_rate: 0.5,
                clot_rate: 0.02,
                internal_bleeding_chance: 60.0,
                internal_bleeding_coefficient: 2.0,
                ..BASE
            },
            Self::PierceCritical => WoundDef {
                series: Series::Pierce,
                wounding: WoundType::Pierce,
                severity: 3,
                threshold_minimum: 100.0,
                threshold_penalty: 15.0,
                name: "Разрыв полости",
                desc: "Внутренние ткани и кровеносная система пациента разорваны, что вызывает \
                       значительное внутреннее кровотечение и повреждение внутренних органов.",
                treat_text_short: SLASH_TREAT,
                occur_text: "разлетается на куски, отправляя части внутренностей во все стороны",
                examine_desc: "разорвана насквозь, едва удерживается на месте открытой костью",
                mangles_exterior: true,
                initial_flow: 2.5,
                gauzed_clot_rate: 0.3,
                internal_bleeding_chance: 80.0,
                internal_bleeding_coefficient: 2.5,
                surgery_states: SKIN_CUT | VESSELS_UNCLAMPED,
                ..BASE
            },
            Self::Dislocation => WoundDef {
                series: Series::Bone,
                wounding: WoundType::Blunt,
                threshold_minimum: 35.0,
                series_threshold_penalty: 15.0,
                name: "Вывих сустава",
                desc: "Конечность пациента выпала из пазухи, вызывая боль и моторную дисфункцию.",
                treat_text_short: "Используйте костоправ или вручную вправьте сустав.",
                occur_text: "болезненно сгибается и начинает безвольно шататься",
                examine_desc: "выглядит не на своем месте",
                limp_slowdown: 3.0,
                limp_chance: 50.0,
                jointed_only: true,
                ..BASE
            },
            Self::Hairline => WoundDef {
                series: Series::Bone,
                wounding: WoundType::Blunt,
                severity: 2,
                threshold_minimum: 60.0,
                series_threshold_penalty: 30.0,
                name: "Закрытый перелом",
                desc: "Кость пациента имеет трещину, вызывая сильную боль и снижение \
                       функциональности конечности.",
                treat_text_short: "Хирургическое лечение или нанесение костного геля. Можно также \
                                   использовать шину или повязку из марли.",
                occur_text: "осыпается костной крошкой, и образуется синяк",
                examine_desc: "выглядит ужасно опухшей, с зазубренными буграми, указывающими на \
                               трещины в кости",
                mangles_interior: true,
                internal_bleeding_chance: 40.0,
                limp_slowdown: 6.0,
                limp_chance: 60.0,
                regen_ticks_needed: 120,
                ..BASE
            },
            Self::Compound => WoundDef {
                series: Series::Bone,
                wounding: WoundType::Blunt,
                severity: 3,
                threshold_minimum: 115.0,
                threshold_penalty: 15.0,
                name: "Открытый перелом",
                desc: "Кости пациента получили множественные переломы, сопровождающиеся разрывом \
                       кожи, вызывая значительную боль и практически полную бесполезность \
                       конечности.",
                treat_text_short: "Проведите хирургическое лечение или нанесите костный гель и \
                                   хирургическую ленту. Также следует использовать шину или \
                                   повязку из марли.",
                occur_text: "раскалывается, обнажая сломанные кости",
                examine_desc: "полностью раздроблена и треснута, обнажая осколки кости",
                mangles_interior: true,
                internal_bleeding_chance: 60.0,
                limp_slowdown: 7.0,
                limp_chance: 70.0,
                disabling: true,
                regen_ticks_needed: 240,
                surgery_states: SKIN_CUT | BONE_SAWED,
                ..BASE
            },
            Self::BurnModerate => WoundDef {
                series: Series::Burn,
                wounding: WoundType::Burn,
                threshold_minimum: 40.0,
                threshold_penalty: 15.0,
                series_threshold_penalty: 30.0,
                name: "Ожог второй степени",
                desc: "Пациент страдает от значительных ожогов с незначительным поражением кожи, \
                       что ослабляет целостность конечности и вызывает уязвимость к ожогам.",
                treat_text_short: "Нанесите средство для лечения, например, регенеративную сетку.",
                occur_text: "покрывается ярко-красными ожогами",
                examine_desc: "сильно обожжёна и покрывается волдырями",
                damage_multiplier_penalty: 1.1,
                flesh_damage: 5.0,
                ..BASE
            },
            Self::BurnSevere => WoundDef {
                series: Series::Burn,
                wounding: WoundType::Burn,
                severity: 2,
                threshold_minimum: 80.0,
                threshold_penalty: 15.0,
                series_threshold_penalty: 40.0,
                name: "Ожог третьей степени",
                desc: "Пациент страдает от сильнейших ожогов с полным проникновением в кожу, что \
                       создает серьезный риск инфекции и значительно снижает целостность \
                       конечности.",
                treat_text_short: "Примените лечебные средства, такие как регенеративная сетка, \
                                   синтплоть или криогеника, и продезинфицируйте / удалите \
                                   инфицированные участки.",
                occur_text: "обугливается, обнажая разрушенные ткани и распространяя ярко-красные \
                             ожоги",
                examine_desc: "выглядит сильно обугленной, с агрессивными красными пятнами",
                damage_multiplier_penalty: 1.2,
                flesh_damage: 12.5,
                infection_rate: 0.07,
                ..BASE
            },
            Self::BurnCritical => WoundDef {
                series: Series::Burn,
                wounding: WoundType::Burn,
                severity: 3,
                threshold_minimum: 140.0,
                threshold_penalty: 25.0,
                name: "Катастрофический ожог",
                desc: "Пациент страдает от почти полного разрушения тканей, с серьезными \
                       обугленными мышцами и костями, что создает угрожающую жизни опасность \
                       инфекции и практически нулевую целостность конечности.",
                treat_text_short: "Нанесите лечебное средство, такое как регенеративная сетка, \
                                   Синтплоть или используйте криогенику и продезинфицируйте / \
                                   удалите инфицированные участки.",
                occur_text: "источает дымок, когда плоть, кости и жир плавятся в ужасающую массу",
                examine_desc: "выглядит как разрушенный беспорядок из обесцвеченных костей, \
                               расплавленного жира и обугленной ткани",
                damage_multiplier_penalty: 1.3,
                flesh_damage: 20.0,
                infection_rate: 0.075,
                ..BASE
            },
        }
    }

    pub fn is_bleeding_kind(self) -> bool {
        matches!(self.def().series, Series::Slash | Series::Pierce)
    }
}

/// tg `severity_text()`.
pub fn severity_text(severity: u8) -> &'static str {
    match severity {
        0 => "Тривиальная",
        1 => "Умеренная",
        2 => "Тяжелая",
        _ => "Критическая",
    }
}

/// One wound instance on a limb.
#[derive(Clone, Debug, PartialEq)]
pub struct Wound {
    pub kind: WoundKind,
    pub blood_flow: f32,
    /// tg `surgery_states` of this instance: what it put on the limb and still owns.
    pub surgery_states: u16,
    /// Bones.
    pub gelled: bool,
    pub taped: bool,
    pub regen_ticks: u32,
    pub regen_ticks_needed: u32,
    /// Compound fracture reset in surgery.
    pub reset: bool,
    /// Pierce: `mend_state`, vessels joined and waiting to be sealed.
    pub mend_state: bool,
    /// Burns.
    pub flesh_damage: f32,
    pub flesh_healing: f32,
    pub infection: f32,
    pub sanitization: f32,
    pub strikes_to_lose_limb: u32,
    pub threshold_penalty: f32,
    /// tg `disabling` (live: splints and infection change it).
    pub disabling: bool,
}

impl Wound {
    pub fn new(kind: WoundKind) -> Self {
        let def = kind.def();
        Self {
            kind,
            blood_flow: 0.0,
            surgery_states: def.surgery_states,
            gelled: false,
            taped: false,
            regen_ticks: 0,
            regen_ticks_needed: def.regen_ticks_needed,
            reset: false,
            mend_state: false,
            flesh_damage: def.flesh_damage,
            flesh_healing: 0.0,
            infection: 0.0,
            sanitization: 0.0,
            strikes_to_lose_limb: 3,
            threshold_penalty: def.threshold_penalty,
            disabling: def.disabling,
        }
    }

    pub fn def(&self) -> WoundDef {
        self.kind.def()
    }

    /// `/datum/wound/burn/flesh/can_be_ointmented_or_meshed`.
    pub fn can_be_ointmented_or_meshed(&self) -> bool {
        (self.infection > 0.0 && self.sanitization < self.infection)
            || (self.flesh_damage > 0.0 && self.flesh_healing <= self.flesh_damage)
    }
}

/// A wrap on a limb (tg `applied_items[LIMB_ITEM_GAUZE]`): the wrap's live absorption and its
/// static vars.
#[derive(Clone, Debug, PartialEq)]
pub struct Gauze {
    pub name: String,
    pub absorption_capacity: f32,
    pub absorption_rate: f32,
    /// 1 when the wrap does not splint (`splint_factor` unset).
    pub splint_factor: f32,
    pub burn_cleanliness_bonus: f32,
}

impl Gauze {
    pub fn from_dm(name: String, med: &DmMedical) -> Self {
        Self {
            name,
            absorption_capacity: med.absorption_capacity,
            absorption_rate: med.absorption_rate,
            splint_factor: if med.splint_factor > 0.0 {
                med.splint_factor
            } else {
                1.0
            },
            burn_cleanliness_bonus: med.burn_cleanliness_bonus,
        }
    }
}

/// A bodypart's medical state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limb {
    pub wounds: Vec<Wound>,
    pub gauze: Option<Gauze>,
    /// tg `surgery_state` bits ([`state`]).
    pub surgery: u16,
    /// `TRAIT_READY_TO_OPERATE` of the bodypart (surgical drapes).
    pub prepped: bool,
    /// Amputated: the stump (tg `BODYPART_STUMP`) left by `super::bodyparts::drop_limb`.
    pub missing: bool,
    /// tg `TRAIT_PARALYSIS_*` of this limb (brain traumas, `super::traumas`): disabled.
    pub paralyzed: bool,
    /// A robotic prosthetic (`/obj/item/bodypart/*/robot`, `BIO_ROBOTIC`) is attached here.
    // shortcut: robotic limbs get no wounds (tg has metal/wired wounds) and keep the body's
    // damage modifiers (tg `brute_modifier`/`burn_modifier` 0.8 per limb).
    pub robotic: bool,
}

impl Limb {
    /// tg `get_splint_factor`.
    pub fn splint_factor(&self) -> f32 {
        self.gauze.as_ref().map_or(1.0, |g| g.splint_factor)
    }

    pub fn is_splinted(&self) -> bool {
        self.splint_factor() < 1.0
    }

    /// tg `wound_damage_multiplier`.
    pub fn damage_multiplier(&self) -> f32 {
        self.wounds
            .iter()
            .map(|w| w.def().damage_multiplier_penalty)
            .product()
    }

    /// tg `mangled_state`: (exterior, interior).
    pub fn mangled(&self) -> (bool, bool) {
        let ext = self.wounds.iter().any(|w| w.def().mangles_exterior);
        let int = self.wounds.iter().any(|w| w.def().mangles_interior);
        (ext, int)
    }

    pub fn wound(&self, series: Series) -> Option<&Wound> {
        self.wounds.iter().find(|w| w.def().series == series)
    }

    pub fn has_state(&self, states: u16) -> bool {
        self.surgery & states == states
    }

    pub fn has_any_state(&self, states: u16) -> bool {
        self.surgery & states != 0
    }

    /// tg `add_surgical_state`: wounds give up the states surgery touches.
    pub fn add_surgery(&mut self, states: u16) {
        self.surgery |= states;
        for wound in &mut self.wounds {
            wound.surgery_states &= !states;
        }
    }

    /// tg `remove_surgical_state`.
    pub fn remove_surgery(&mut self, states: u16) {
        if self.surgery & states == 0 {
            return;
        }
        self.surgery &= !states;
        for wound in &mut self.wounds {
            wound.surgery_states &= !states;
        }
    }

    /// tg `refresh_bleed_rate` of a human bodypart in `zone`.
    pub fn bleed_rate(&self, zone: BodyZone, lying: bool) -> f32 {
        let mut rate = 0.0;
        let mut surgery = 0.0;
        if self.has_any_state(state::VESSELS_UNCLAMPED) {
            surgery += UNCLAMPED_VESSELS_BLEEDING;
        } else if self.has_any_state(state::VESSELS_CLAMPED | state::ORGANS_CUT) {
            surgery += CLAMPED_VESSELS_BLEEDING;
        }
        match zone {
            BodyZone::Head => surgery *= 0.5,
            BodyZone::Chest => {}
            _ => surgery *= 0.25,
        }
        if self.gauze.is_some() {
            surgery *= 0.4;
        }
        rate += surgery;
        for wound in &self.wounds {
            rate += wound.blood_flow;
            if wound.surgery_states & state::VESSELS_UNCLAMPED == 0 || surgery == 0.0 {
                continue;
            }
            // tg counts the open vessels as the wound's own bleeding.
            rate -= UNCLAMPED_VESSELS_BLEEDING;
            surgery = 0.0;
        }
        if lying {
            rate *= 0.75;
        }
        rate
    }

    /// tg `bodypart_disabled` of a limb that `can_be_disabled` (arms and legs).
    pub fn disabled(&self, damage: f32, max_damage: f32) -> bool {
        self.missing
            || self.paralyzed
            || self.wounds.iter().any(|w| w.disabling)
            || damage >= max_damage
    }

    /// The limp of a leg: (`limp_slowdown` sum, max `limp_chance`), scaled by its splint
    /// (tg `update_inefficiencies` and `/datum/status_effect/limp/update_limp`).
    fn limp(&self) -> (f32, f32) {
        let splint = self.splint_factor();
        let mut slowdown = 0.0;
        let mut chance: f32 = 0.0;
        for wound in &self.wounds {
            let def = wound.def();
            slowdown += def.limp_slowdown * splint;
            chance = chance.max(def.limp_chance * splint);
        }
        (slowdown, chance)
    }
}

/// The medical state of a human's six bodyparts, by [`BodyZone::index`].
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Limbs {
    pub parts: [Limb; 6],
    /// `/datum/status_effect/surgery_prepped` `movement_counter`.
    pub prep_moves: u32,
}

/// What the wounds do to moving (see [`Limbs::limits`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Limits {
    /// Deciseconds per step.
    pub slowdown: f32,
    pub floored: bool,
}

impl Limbs {
    pub fn limb(&self, zone: BodyZone) -> &Limb {
        &self.parts[zone.index()]
    }

    pub fn limb_mut(&mut self, zone: BodyZone) -> &mut Limb {
        &mut self.parts[zone.index()]
    }

    /// tg `get_bleed_rate`.
    pub fn bleed_rate(&self, lying: bool) -> f32 {
        BodyZone::ALL
            .iter()
            .map(|&z| self.limb(z).bleed_rate(z, lying))
            .sum()
    }

    pub fn any_wounds(&self) -> bool {
        self.parts.iter().any(|l| !l.wounds.is_empty())
    }

    /// tg `update_usable_leg_status` (`limbless_slowdown`) and the limp of both legs.
    pub fn limits(&self, health: &Health) -> Limits {
        let legs = [BodyZone::LLeg, BodyZone::RLeg];
        let disabled = legs
            .iter()
            .filter(|&&z| {
                self.limb(z)
                    .disabled(health.part(z).damage(), z.max_damage())
            })
            .count();
        let usable = 2 - disabled;
        let mut slowdown = (2 - usable) as f32 * 3.0;
        // The limp alternates legs: each step rolls one leg's chance.
        for leg in legs {
            let (limp, chance) = self.limb(leg).limp();
            slowdown += limp * chance / 100.0 / 2.0;
        }
        Limits {
            slowdown,
            floored: usable == 0,
        }
    }
}

/// tg `SPT_PROB(percent, seconds)`.
pub fn spt_prob(rng: &mut CombatRng, percent: f32, secs: f32) -> bool {
    rng.chance((1.0 - (1.0 - percent / 100.0).powf(secs)) * 100.0)
}

/// What a hit brings to the wound roll (tg `receive_damage` args).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub kind: DamageType,
    /// The damage the limb took (after the wound damage multiplier).
    pub damage: f32,
    pub sharpness: u32,
    pub wound_bonus: f32,
    pub exposed_wound_bonus: f32,
    /// The limb's damage before the hit (tg checks wounding before applying it).
    pub part_damage_before: f32,
}

/// The wounding type and damage of a hit on a flesh-and-bone limb (tg `receive_damage`'s
/// wound handling up to `check_wounding`): `None` for damage that does not wound.
pub fn wounding_of(hit: Hit, limb: &Limb) -> Option<(WoundType, f32)> {
    let (brute, burn) = match hit.kind {
        DamageType::Brute => (hit.damage, 0.0),
        DamageType::Burn => (0.0, hit.damage),
        _ => return None,
    };
    let mut wounding = if brute > burn {
        WoundType::Blunt
    } else {
        WoundType::Burn
    };
    let mut damage = brute.max(burn);
    if wounding == WoundType::Blunt && hit.sharpness != 0 {
        if hit.sharpness & SHARP_EDGED != 0 {
            wounding = WoundType::Slash;
        } else if hit.sharpness & SHARP_POINTY != 0 {
            wounding = WoundType::Pierce;
        }
    }
    // A mangled skin passes sharp hits on to the bone, at 60% (edged) or 75% (pointy).
    let (exterior, interior) = limb.mangled();
    if exterior && !interior && hit.sharpness != 0 {
        if wounding == WoundType::Slash {
            damage *= 0.6;
        }
        if wounding == WoundType::Pierce {
            damage *= 0.75;
        }
        wounding = WoundType::Blunt;
    }
    Some((wounding, damage))
}

/// tg `wound_resistance` and `disabled_wound_penalty` of a human bodypart.
fn resistance(zone: BodyZone) -> (f32, f32) {
    match zone {
        BodyZone::Head => (5.0, 25.0),
        BodyZone::Chest => (10.0, 15.0),
        _ => (0.0, 15.0),
    }
}

/// tg `check_wounding` on a human bodypart: the wound to apply, if any.
// shortcut: armor is only the limb's `wound_resistance` (no clothing `wound` armor).
pub fn check_wounding(
    rng: &mut CombatRng,
    limb: &Limb,
    zone: BodyZone,
    wounding: WoundType,
    damage: f32,
    hit: Hit,
) -> Option<WoundKind> {
    match roll_wounding(rng, limb, zone, wounding, damage, hit, false)? {
        Rolled::Wound(kind) => Some(kind),
        Rolled::Dismember => None,
    }
}

/// What [`roll_wounding`] decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rolled {
    Wound(WoundKind),
    /// The roll beat `WOUND_DISMEMBER_OUTRIGHT_THRESH`: the limb comes off outright.
    Dismember,
}

/// tg `check_wounding` with the outright dismemberment of a limb that `can_dismember`.
pub fn roll_wounding(
    rng: &mut CombatRng,
    limb: &Limb,
    zone: BodyZone,
    wounding: WoundType,
    damage: f32,
    hit: Hit,
    can_dismember: bool,
) -> Option<Rolled> {
    let damage = damage.min(WOUND_MAX_CONSIDERED_DAMAGE);
    let top = (damage.powf(WOUND_DAMAGE_EXPONENT)).round().max(1.0) as u32;
    let base_roll = rng.range(1, top) as f32;
    let injury_roll = woundings_mods(limb, zone, base_roll, hit);
    if can_dismember
        && injury_roll > WOUND_DISMEMBER_OUTRIGHT_THRESH
        && rng.chance(hit.part_damage_before / zone.max_damage() * 100.0)
    {
        return Some(Rolled::Dismember);
    }
    let mut series_mods = [0.0f32; 4];
    for wound in &limb.wounds {
        series_mods[wound.def().series as usize] += wound.def().series_threshold_penalty;
    }
    let mut possible: Vec<WoundKind> = WoundKind::ALL
        .into_iter()
        .filter(|kind| {
            let def = kind.def();
            def.threshold_minimum <= injury_roll + series_mods[def.series as usize]
                && can_be_applied(limb, zone, *kind, wounding)
        })
        .collect();
    // `WOUND_COMPETITION_OVERPOWER_LESSERS`: the worst severity wins.
    let worst = possible.iter().map(|k| k.def().severity).max()?;
    possible.retain(|k| k.def().severity == worst);
    // Every candidate weighs `WOUND_DEFAULT_WEIGHT`; one per series and severity remains.
    let pick = rng.range(0, possible.len() as u32 - 1) as usize;
    Some(Rolled::Wound(possible[pick]))
}

/// tg `try_dismember` of a limb with mangled skin and bone: the hit's wounding damage plus half
/// the limb's damage share, +15 per critical blunt wound.
fn try_dismember(rng: &mut CombatRng, limb: &Limb, zone: BodyZone, damage: f32, hit: Hit) -> bool {
    if damage < DISMEMBER_MINIMUM_DAMAGE {
        return false;
    }
    let mut chance = damage + hit.part_damage_before / zone.max_damage() * 50.0;
    for wound in &limb.wounds {
        let def = wound.def();
        if def.wounding == WoundType::Blunt && def.severity >= 3 {
            chance += CRITICAL_BLUNT_DISMEMBER_BONUS;
        }
    }
    rng.chance(chance)
}

/// tg `check_woundings_mods` without clothing armor.
pub fn woundings_mods(limb: &Limb, zone: BodyZone, roll: f32, hit: Hit) -> f32 {
    let (resist, disabled_penalty) = resistance(zone);
    let mut armor = 0.0;
    let mut injury = roll + hit.wound_bonus;
    for wound in &limb.wounds {
        injury += wound.threshold_penalty;
    }
    let (exterior, interior) = limb.mangled();
    if !exterior && !interior {
        armor += resist;
    }
    if hit.part_damage_before >= zone.max_damage() {
        injury += disabled_penalty;
    }
    if armor == 0.0 {
        injury += hit.exposed_wound_bonus;
    } else {
        injury *= (100.0 - armor) / 100.0;
    }
    injury
}

/// tg `/datum/wound_pregen_data/can_be_applied_to` with `random_roll`.
fn can_be_applied(limb: &Limb, zone: BodyZone, kind: WoundKind, wounding: WoundType) -> bool {
    let def = kind.def();
    if def.wounding != wounding {
        return false;
    }
    if def.jointed_only && matches!(zone, BodyZone::Head | BodyZone::Chest) {
        return false;
    }
    !limb
        .wounds
        .iter()
        .any(|w| w.def().series == def.series && w.def().severity >= def.severity)
}

/// What happened to a limb's wounds, for the caller's messages and side effects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitOutcome {
    /// A new or worse wound (tg `apply_wound` without `demoted`).
    pub gained: Option<WoundKind>,
    /// Blood lost at once by internal bleeding, with the victim's message.
    pub bled: Vec<(f32, &'static str)>,
    /// A bone wound wants the item in this hand dropped.
    pub drop_hand: Option<usize>,
    /// The limb comes off (tg `/datum/wound/loss/apply_dismember`): the wounding type and
    /// whether outright; nothing else happened to it.
    pub dismember: Option<(WoundType, bool)>,
}

/// tg `apply_wound` (or `replace_wound` of a milder wound of the series) on `limb`.
pub fn apply_wound(limb: &mut Limb, kind: WoundKind) {
    let def = kind.def();
    let old = limb
        .wounds
        .iter()
        .position(|w| w.def().series == def.series)
        .map(|i| remove_wound(limb, i));
    let mut wound = Wound::new(kind);
    // `set_limb`: states overridden by what surgery did are left out.
    for (st, over) in [
        (state::SKIN_CUT, state::SKIN_OPEN),
        (state::VESSELS_UNCLAMPED, state::VESSELS_CLAMPED),
        (state::BONE_SAWED, state::BONE_DRILLED),
        (state::BONE_DRILLED, state::BONE_SAWED),
    ] {
        if limb.surgery & over != 0 {
            wound.surgery_states &= !st;
        }
    }
    limb.surgery |= wound.surgery_states;
    // `wound_injury`.
    match def.series {
        Series::Slash => {
            wound.blood_flow = old
                .as_ref()
                .map_or(def.initial_flow, |o| o.blood_flow.max(def.initial_flow));
        }
        Series::Pierce => wound.blood_flow = def.initial_flow,
        _ => {}
    }
    // A new cut or puncture through a gauze soaks 80% of it; a burn destroys it.
    if old.is_none() {
        match def.series {
            Series::Slash | Series::Pierce => {
                if let Some(gauze) = &mut limb.gauze {
                    gauze.absorption_capacity -= gauze_initial_capacity(gauze) * 0.8;
                    if gauze.absorption_capacity <= 0.0 {
                        limb.gauze = None;
                    }
                }
            }
            Series::Burn => limb.gauze = None,
            Series::Bone => {}
        }
    }
    if def.disabling {
        wound.disabling = !limb.is_splinted();
    }
    limb.wounds.push(wound);
}

/// The wrap's original capacity: tg reads `initial(absorption_capacity)` of its type; every
/// wrap here starts at its type's value, so the remaining capacity rounded up stands in.
// shortcut: the initial capacity is not kept per wrap; gauze types start at 5 or 4.
fn gauze_initial_capacity(gauze: &Gauze) -> f32 {
    gauze.absorption_capacity.max(4.0).ceil()
}

/// tg `update_inefficiencies` after a wrap went on or off: splinted fractures stop disabling.
pub fn refresh_splints(limb: &mut Limb) {
    let splinted = limb.is_splinted();
    for wound in &mut limb.wounds {
        if wound.def().disabling {
            wound.disabling = !splinted;
        }
    }
}

/// tg `remove_wound`: the wound's own surgery states leave the limb with it.
pub fn remove_wound(limb: &mut Limb, index: usize) -> Wound {
    let wound = limb.wounds.remove(index);
    if wound.surgery_states != 0 {
        limb.surgery &= !wound.surgery_states;
    }
    wound
}

/// The wound part of tg `receive_damage` on a human bodypart: the dismemberment roll of a
/// mangled limb that `can_dismember`, the wound roll, then every wound's `receive_damage` (the
/// new one included).
pub fn receive_damage(
    rng: &mut CombatRng,
    limb: &mut Limb,
    zone: BodyZone,
    hit: Hit,
    dead: bool,
    has_blood: bool,
    can_dismember: bool,
) -> HitOutcome {
    let mut out = HitOutcome::default();
    if limb.robotic {
        return out;
    }
    let Some((wounding, damage)) = wounding_of(hit, limb) else {
        return out;
    };
    let (exterior, interior) = limb.mangled();
    if can_dismember && exterior && interior && try_dismember(rng, limb, zone, damage, hit) {
        out.dismember = Some((wounding, false));
        return out;
    }
    let rolled = if damage >= WOUND_MINIMUM_DAMAGE && hit.wound_bonus != CANT_WOUND {
        roll_wounding(rng, limb, zone, wounding, damage, hit, can_dismember)
    } else {
        None
    };
    if rolled == Some(Rolled::Dismember) {
        out.dismember = Some((wounding, true));
        return out;
    }
    if let Some(Rolled::Wound(kind)) = rolled {
        let def = kind.def();
        apply_wound(limb, kind);
        out.gained = Some(kind);
        // Bone `wound_injury`: the hand of a broken arm may drop what it holds.
        if def.series == Series::Bone {
            let hand = match zone {
                BodyZone::LArm => Some(0),
                BodyZone::RArm => Some(1),
                _ => None,
            };
            let disabling = limb.wounds.last().is_some_and(|w| w.disabling);
            if hand.is_some() && (disabling || rng.chance(30.0 * f32::from(def.severity))) {
                out.drop_hand = hand;
            }
        }
    }
    let splint = limb.splint_factor();
    for wound in &mut limb.wounds {
        let def = wound.def();
        match def.series {
            Series::Slash => {
                if !dead && hit.wound_bonus != CANT_WOUND && wounding == WoundType::Slash {
                    wound.blood_flow = (wound.blood_flow + WOUND_SLASH_DAMAGE_FLOW_COEFF * damage)
                        .min(WOUND_MAX_BLOODFLOW);
                }
            }
            Series::Pierce => {
                if dead
                    || damage < 5.0
                    || !has_blood
                    || !rng.chance(def.internal_bleeding_chance + damage)
                {
                    continue;
                }
                let factor = [0.75, 1.0, 1.25, 1.5][rng.range(0, 3) as usize];
                let bled = damage.sqrt() * def.internal_bleeding_coefficient * splint * factor;
                let text = if bled >= 12.0 {
                    "Вы выплевываете струю крови из-за удара!"
                } else if bled >= 8.0 {
                    "Вы откашливаетесь, выплевывая немного крови из-за удара."
                } else {
                    ""
                };
                out.bled.push((bled, text));
            }
            Series::Bone => {
                if damage < WOUND_MINIMUM_DAMAGE
                    || !has_blood
                    || zone != BodyZone::Chest
                    || !rng.chance(def.internal_bleeding_chance + damage)
                {
                    continue;
                }
                let factor = if def.severity == 3 { 2.0 } else { 1.5 };
                let bled = rng.range(1, ((damage * factor) as u32).max(1)) as f32;
                let text = match bled as u32 {
                    0..=6 => "",
                    7..=13 => "Вы откашливаете кровь после удара в грудь.",
                    14..=19 => "Вы сплевываете струйкой крови после удара в грудь!",
                    _ => "Вы задыхаетесь от струи крови после удара в грудь!",
                };
                out.bled.push((bled, text));
            }
            Series::Burn => {}
        }
    }
    out
}

/// Something a wound tick asks of the body.
#[derive(Clone, Debug, PartialEq)]
pub enum TickEffect {
    Tox(f32),
    /// `take_bodypart_damage(brute)` + stamina (bone regrowth pains).
    Pain {
        brute: f32,
        stamina: f32,
    },
    Tell(Span, String),
}

/// The body around a wound tick.
#[derive(Clone, Copy, Debug)]
pub struct TickCtx {
    pub secs: f32,
    pub lying: bool,
    pub bodytemperature: f64,
    pub nutrition: f32,
    pub burn_damage: f32,
    pub has_blood: bool,
}

/// tg `handle_process` of every wound on `limb` (one `Life()` tick). Wounds that heal are
/// removed; returns the effects on the body.
pub fn process(
    rng: &mut CombatRng,
    limb: &mut Limb,
    zone: BodyZone,
    ctx: TickCtx,
) -> Vec<TickEffect> {
    let mut effects = Vec::new();
    let mut i = 0;
    while i < limb.wounds.len() {
        let def = limb.wounds[i].def();
        let keep = match def.series {
            Series::Slash => process_slash(limb, i, ctx, &mut effects, zone),
            Series::Pierce => process_pierce(limb, i, ctx, &mut effects, zone),
            Series::Bone => process_bone(rng, limb, i, ctx, &mut effects),
            Series::Burn => process_burn(rng, limb, i, ctx, &mut effects),
        };
        if keep {
            i += 1;
        }
    }
    effects
}

/// tg `seep_gauze`: true when there is a gauze (it may have fallen apart).
pub fn seep_gauze(
    limb: &mut Limb,
    amount: f32,
    zone: BodyZone,
    effects: &mut Vec<TickEffect>,
) -> bool {
    let Some(gauze) = &mut limb.gauze else {
        return false;
    };
    gauze.absorption_capacity -= amount;
    if gauze.absorption_capacity <= 0.0 {
        effects.push(TickEffect::Tell(
            Span::Warning,
            format!(
                "{} на вашей {} разваливается в лохмотья.",
                gauze.name,
                zone_prepositional(zone)
            ),
        ));
        limb.gauze = None;
    }
    true
}

/// `adjust_blood_flow` of the slash at `i`: clamps it, demotes or closes it. False when it is
/// no longer at `i` (closed, or a replacement went to the end).
pub fn adjust_slash(
    limb: &mut Limb,
    i: usize,
    by: f32,
    zone: BodyZone,
    effects: &mut Vec<TickEffect>,
) -> bool {
    let def = limb.wounds[i].def();
    let wound = &mut limb.wounds[i];
    wound.blood_flow = (wound.blood_flow + by).clamp(0.0, WOUND_MAX_BLOODFLOW);
    if wound.blood_flow >= def.minimum_flow {
        return true;
    }
    match def.demotes_to {
        // `replace_wound`: the milder cut keeps the flow if it is higher than its own.
        Some(lesser) => apply_wound(limb, lesser),
        None => {
            remove_wound(limb, i);
            effects.push(TickEffect::Tell(
                Span::Notice,
                format!(
                    "Порез на вашей {} перестал кровоточить!",
                    zone_prepositional(zone)
                ),
            ));
        }
    }
    false
}

fn process_slash(
    limb: &mut Limb,
    i: usize,
    ctx: TickCtx,
    effects: &mut Vec<TickEffect>,
    zone: BodyZone,
) -> bool {
    let def = limb.wounds[i].def();
    if ctx.has_blood
        && def.clot_rate > 0.0
        && !adjust_slash(limb, i, -def.clot_rate * ctx.secs, zone, effects)
    {
        return false;
    }
    let rate = limb.gauze.as_ref().map_or(0.0, |g| g.absorption_rate);
    if rate > 0.0 {
        seep_gauze(limb, rate * ctx.secs, zone, effects);
        return adjust_slash(limb, i, -rate * ctx.secs, zone, effects);
    }
    true
}

/// `/datum/wound/pierce/bleed/adjust_blood_flow`: gone at 0.
pub fn adjust_pierce(limb: &mut Limb, i: usize, by: f32) -> bool {
    let wound = &mut limb.wounds[i];
    wound.blood_flow = (wound.blood_flow + by).clamp(0.0, WOUND_MAX_BLOODFLOW);
    if wound.blood_flow <= 0.0 {
        remove_wound(limb, i);
        return false;
    }
    true
}

fn process_pierce(
    limb: &mut Limb,
    i: usize,
    ctx: TickCtx,
    effects: &mut Vec<TickEffect>,
    zone: BodyZone,
) -> bool {
    let def = limb.wounds[i].def();
    if ctx.has_blood
        && ctx.bodytemperature < BODYTEMP_NORMAL - 10.0
        && !adjust_pierce(limb, i, -0.1 * ctx.secs)
    {
        return false;
    }
    let rate = limb.gauze.as_ref().map_or(0.0, |g| g.absorption_rate);
    if rate > 0.0 {
        seep_gauze(limb, rate * ctx.secs, zone, effects);
        return adjust_pierce(
            limb,
            i,
            -def.clot_rate * ctx.secs - rate * def.gauzed_clot_rate * ctx.secs,
        );
    }
    if ctx.has_blood {
        return adjust_pierce(limb, i, -def.clot_rate * ctx.secs);
    }
    true
}

fn process_bone(
    rng: &mut CombatRng,
    limb: &mut Limb,
    i: usize,
    ctx: TickCtx,
    effects: &mut Vec<TickEffect>,
) -> bool {
    let wound = &mut limb.wounds[i];
    let severity = f32::from(wound.def().severity);
    // Gel and tape knit a fracture of a flesh limb.
    if !(wound.gelled && wound.taped) {
        return true;
    }
    wound.regen_ticks += 1;
    if ctx.lying && spt_prob(rng, 30.0, ctx.secs) {
        wound.regen_ticks += 1;
    }
    if spt_prob(rng, severity * 1.5, ctx.secs) {
        let brute = rng.range(1, (severity * 2.0) as u32) as f32;
        let stamina = rng.range(2, (severity * 2.5) as u32) as f32;
        effects.push(TickEffect::Pain { brute, stamina });
        if rng.chance(33.0) {
            effects.push(TickEffect::Tell(
                Span::Danger,
                "Вы чувствуете острую боль во всём теле, пока ваши кости срастаются!".into(),
            ));
        }
    }
    if wound.regen_ticks > wound.regen_ticks_needed {
        remove_wound(limb, i);
        return false;
    }
    true
}

fn process_burn(
    rng: &mut CombatRng,
    limb: &mut Limb,
    i: usize,
    ctx: TickCtx,
    effects: &mut Vec<TickEffect>,
) -> bool {
    let secs = ctx.secs;
    if limb.wounds[i].strikes_to_lose_limb == 0 {
        effects.push(TickEffect::Tox(0.25 * secs));
        return true;
    }
    let mut dummy = Vec::new();
    let gauzed = limb.gauze.is_some();
    let bandage = limb
        .gauze
        .as_ref()
        .map(|g| g.burn_cleanliness_bonus)
        .filter(|b| *b > 0.0)
        .unwrap_or(1.0);
    if gauzed {
        seep_gauze(
            limb,
            WOUND_BURN_SANITIZATION_RATE * secs,
            BodyZone::Chest,
            &mut dummy,
        );
    }
    let wound = &mut limb.wounds[i];
    if wound.flesh_healing > 0.0 {
        wound.flesh_damage = (wound.flesh_damage - 0.5 * secs).max(0.0);
        wound.flesh_healing = (wound.flesh_healing - 0.5 * bandage * secs).max(0.0);
    }
    if wound.infection <= WOUND_INFECTION_MODERATE
        && ctx.burn_damage < 5.0
        && ctx.nutrition >= NUTRITION_LEVEL_FED
    {
        wound.flesh_healing += 0.2;
    }
    if wound.flesh_damage <= 0.0 && wound.infection <= WOUND_INFECTION_MODERATE {
        remove_wound(limb, i);
        return false;
    }
    if wound.sanitization > 0.0 {
        wound.infection = (wound.infection - WOUND_BURN_SANITIZATION_RATE * secs).max(0.0);
        wound.sanitization =
            (wound.sanitization - WOUND_BURN_SANITIZATION_RATE * bandage * secs).max(0.0);
        return true;
    }
    wound.infection += wound.def().infection_rate * secs;
    let infection = wound.infection;
    if infection <= WOUND_INFECTION_MODERATE {
        return true;
    }
    if infection <= WOUND_INFECTION_SEVERE {
        if spt_prob(rng, 15.0, secs) {
            effects.push(TickEffect::Tox(0.2));
        }
    } else if infection <= WOUND_INFECTION_CRITICAL {
        if !wound.disabling {
            if spt_prob(rng, 1.0, secs) {
                wound.disabling = true;
                return true;
            }
        } else if spt_prob(rng, 4.0, secs) {
            wound.disabling = false;
            return true;
        }
        if spt_prob(rng, 10.0, secs) {
            effects.push(TickEffect::Tox(0.5));
        }
    } else if infection <= WOUND_INFECTION_SEPTIC {
        if !wound.disabling {
            if spt_prob(rng, 1.5, secs) {
                wound.disabling = true;
                return true;
            }
        } else if spt_prob(rng, 1.5, secs) {
            wound.disabling = false;
            return true;
        }
        if spt_prob(rng, 2.48, secs) {
            effects.push(TickEffect::Tox(if rng.chance(20.0) { 0.75 } else { 1.0 }));
        }
    } else if spt_prob(rng, 0.5 * infection, secs) {
        wound.strikes_to_lose_limb = wound.strikes_to_lose_limb.saturating_sub(1);
        if wound.strikes_to_lose_limb == 0 {
            wound.threshold_penalty *= 2.0;
            wound.disabling = true;
            effects.push(TickEffect::Tell(
                Span::Danger,
                "Инфекция полностью поглотила конечность, она больше не двигается.".into(),
            ));
        } else {
            effects.push(TickEffect::Tell(
                Span::Danger,
                "Вы едва чувствуете поражённую конечность, по ней ползёт гниль!".into(),
            ));
        }
    }
    true
}

/// `ru_plaintext_zone[PREPOSITIONAL]`.
pub fn zone_prepositional(zone: BodyZone) -> &'static str {
    match zone {
        BodyZone::Head => "голове",
        BodyZone::Chest => "груди",
        BodyZone::LArm => "левой руке",
        BodyZone::RArm => "правой руке",
        BodyZone::LLeg => "левой ноге",
        BodyZone::RLeg => "правой ноге",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(kind: DamageType, damage: f32, sharpness: u32) -> Hit {
        Hit {
            kind,
            damage,
            sharpness,
            wound_bonus: 0.0,
            exposed_wound_bonus: 0.0,
            part_damage_before: 0.0,
        }
    }

    #[test]
    fn setting_a_bone_spares_a_wound_that_worsened() {
        let mut world = World::new();
        let mut limbs = Limbs::default();
        apply_wound(limbs.limb_mut(BodyZone::LArm), WoundKind::Compound);
        let patient = world.spawn(limbs).id();
        let mob = world.spawn_empty().id();
        let actor = crate::tools::Actor {
            client: mob,
            mob,
            item: None,
        };
        // Started on a dislocation, which became a compound fracture during the do_after.
        finish_treatment(
            &mut world,
            actor,
            patient,
            BodyZone::LArm,
            Treat::SetBone,
            WoundKind::Dislocation,
        );
        let wounds = &world
            .get::<Limbs>(patient)
            .unwrap()
            .limb(BodyZone::LArm)
            .wounds;
        assert_eq!(wounds.len(), 1);
        assert_eq!(wounds[0].kind, WoundKind::Compound);
    }

    #[test]
    fn mangled_limbs_come_off_only_when_dismemberable() {
        // Skin (avulsion) and bone (hairline fracture) mangled: a 30 slash at an arm at half
        // damage rolls 30 + 25 = 55% (no critical blunt bonus); some seeds take it off.
        let mut mangled = Limb::default();
        apply_wound(&mut mangled, WoundKind::SlashCritical);
        apply_wound(&mut mangled, WoundKind::Hairline);
        let h = Hit {
            part_damage_before: 25.0,
            ..hit(DamageType::Brute, 30.0, SHARP_EDGED)
        };
        let mut off = 0;
        for seed in 0..200 {
            let mut rng = CombatRng(seed);
            let mut limb = mangled.clone();
            let out = receive_damage(&mut rng, &mut limb, BodyZone::LArm, h, false, true, true);
            if let Some((wounding, outright)) = out.dismember {
                assert_eq!(wounding, WoundType::Slash);
                assert!(!outright);
                assert_eq!(out.gained, None, "a limb that comes off takes no new wound");
                off += 1;
            }
            let mut rng = CombatRng(seed);
            let mut limb = mangled.clone();
            let out = receive_damage(&mut rng, &mut limb, BodyZone::LArm, h, false, true, false);
            assert_eq!(out.dismember, None, "not dismemberable");
        }
        assert!((60..160).contains(&off), "about 55% of 200: {off}");
        // Below `DISMEMBER_MINIMUM_DAMAGE` nothing comes off; an intact limb never does.
        for seed in 0..50 {
            let small = Hit { damage: 9.0, ..h };
            let out = receive_damage(
                &mut CombatRng(seed),
                &mut mangled.clone(),
                BodyZone::LArm,
                small,
                false,
                true,
                true,
            );
            assert_eq!(out.dismember, None);
            let out = receive_damage(
                &mut CombatRng(seed),
                &mut Limb::default(),
                BodyZone::LArm,
                h,
                false,
                true,
                true,
            );
            assert_eq!(out.dismember, None);
        }
        // A robotic limb gets no flesh wounds at all.
        let robotic = Limb {
            robotic: true,
            ..Limb::default()
        };
        let out = receive_damage(
            &mut CombatRng(3),
            &mut robotic.clone(),
            BodyZone::LArm,
            hit(DamageType::Brute, 25.0, SHARP_EDGED),
            false,
            true,
            true,
        );
        assert_eq!(out, HitOutcome::default());
    }

    #[test]
    fn a_huge_wound_roll_takes_a_damaged_limb_off_outright() {
        // `wound_bonus` 200 beats the 150 threshold; a limb at full damage always goes.
        let h = Hit {
            wound_bonus: 200.0,
            part_damage_before: BodyZone::RLeg.max_damage(),
            ..hit(DamageType::Brute, 20.0, 0)
        };
        let limb = Limb::default();
        let rolled = roll_wounding(
            &mut CombatRng(5),
            &limb,
            BodyZone::RLeg,
            WoundType::Blunt,
            20.0,
            h,
            true,
        );
        assert_eq!(rolled, Some(Rolled::Dismember));
        let rolled = roll_wounding(
            &mut CombatRng(5),
            &limb,
            BodyZone::RLeg,
            WoundType::Blunt,
            20.0,
            h,
            false,
        );
        assert!(matches!(rolled, Some(Rolled::Wound(_))));
    }

    #[test]
    fn wounding_type_follows_sharpness_and_mangling() {
        let limb = Limb::default();
        assert_eq!(
            wounding_of(hit(DamageType::Brute, 10.0, 0), &limb),
            Some((WoundType::Blunt, 10.0))
        );
        assert_eq!(
            wounding_of(hit(DamageType::Brute, 10.0, SHARP_EDGED), &limb),
            Some((WoundType::Slash, 10.0))
        );
        assert_eq!(
            wounding_of(hit(DamageType::Brute, 10.0, SHARP_POINTY), &limb),
            Some((WoundType::Pierce, 10.0))
        );
        assert_eq!(
            wounding_of(hit(DamageType::Burn, 10.0, SHARP_EDGED), &limb),
            Some((WoundType::Burn, 10.0))
        );
        assert_eq!(wounding_of(hit(DamageType::Tox, 10.0, 0), &limb), None);
        // A critical cut exposes the bone: slashes become blunt at 60%.
        let mut mangled = Limb::default();
        apply_wound(&mut mangled, WoundKind::SlashCritical);
        let (kind, damage) =
            wounding_of(hit(DamageType::Brute, 10.0, SHARP_EDGED), &mangled).unwrap();
        assert_eq!(kind, WoundType::Blunt);
        assert!((damage - 6.0).abs() < 1e-5);
    }

    #[test]
    fn roll_modifiers_follow_check_woundings_mods() {
        let limb = Limb::default();
        let mut h = hit(DamageType::Brute, 10.0, SHARP_EDGED);
        h.wound_bonus = 10.0;
        h.exposed_wound_bonus = 15.0;
        // An arm has no resistance: the exposed bonus applies.
        assert_eq!(woundings_mods(&limb, BodyZone::LArm, 20.0, h), 45.0);
        // The chest's `wound_resistance` 10 counts as armor: no exposed bonus, 90%.
        assert!((woundings_mods(&limb, BodyZone::Chest, 20.0, h) - 27.0).abs() < 1e-4);
        // A maxed arm adds `disabled_wound_penalty`; existing wounds their penalty.
        let mut hurt = Limb::default();
        apply_wound(&mut hurt, WoundKind::Dislocation);
        h.part_damage_before = 50.0;
        assert_eq!(
            woundings_mods(&hurt, BodyZone::LArm, 20.0, h),
            20.0 + 10.0 + 5.0 + 15.0 + 15.0
        );
    }

    #[test]
    fn wound_thresholds_and_seeded_chances() {
        // A scalpel (10 force, +10, +15 exposed) on an arm: roll 1..25 + 25.
        let mut h = hit(DamageType::Brute, 10.0, SHARP_EDGED);
        h.wound_bonus = 10.0;
        h.exposed_wound_bonus = 15.0;
        let mut rng = CombatRng(11);
        let mut counts = std::collections::HashMap::new();
        for _ in 0..10_000 {
            let limb = Limb::default();
            let got = check_wounding(&mut rng, &limb, BodyZone::LArm, WoundType::Slash, 10.0, h);
            *counts.entry(got).or_insert(0u32) += 1;
        }
        // 10^1.4 = 25.1 -> rand(1, 25) + 25: always >= 20 (abrasion), >= 50 (laceration) only at 25.
        assert_eq!(counts.get(&None), None, "{counts:?}");
        let severe = counts
            .get(&Some(WoundKind::SlashSevere))
            .copied()
            .unwrap_or(0);
        assert!((250..550).contains(&severe), "1 in 25: {counts:?}");
        // The same hit to the chest: (roll + 10) * 0.9, abrasion from roll 13 on.
        let mut moderate = 0;
        for _ in 0..10_000 {
            let limb = Limb::default();
            if check_wounding(&mut rng, &limb, BodyZone::Chest, WoundType::Slash, 10.0, h).is_some()
            {
                moderate += 1;
            }
        }
        assert!(
            (4800..5600).contains(&moderate),
            "13 of 25 rolls: {moderate}"
        );
        // A punch (7 damage, no bonus) rolls at most 15: never wounds.
        let punch = hit(DamageType::Brute, 7.0, 0);
        for _ in 0..1000 {
            let limb = Limb::default();
            assert_eq!(
                check_wounding(
                    &mut rng,
                    &limb,
                    BodyZone::LArm,
                    WoundType::Blunt,
                    7.0,
                    punch
                ),
                None
            );
        }
        // Damage over 25 is capped: 25^1.4 = 90.6 -> 91.
        let big = hit(DamageType::Brute, 100.0, 0);
        let mut compound = 0;
        for _ in 0..2000 {
            let limb = Limb::default();
            if check_wounding(
                &mut rng,
                &limb,
                BodyZone::LLeg,
                WoundType::Blunt,
                100.0,
                big,
            ) == Some(WoundKind::Compound)
            {
                compound += 1;
            }
        }
        assert_eq!(compound, 0, "115 is out of reach of a single fresh hit");
    }

    #[test]
    fn worse_wounds_replace_milder_ones_and_bleeding_clots() {
        let mut limb = Limb::default();
        apply_wound(&mut limb, WoundKind::SlashModerate);
        assert_eq!(limb.wounds[0].blood_flow, 1.75);
        apply_wound(&mut limb, WoundKind::SlashSevere);
        assert_eq!(limb.wounds.len(), 1);
        assert_eq!(limb.wounds[0].blood_flow, 2.75);
        assert!(limb.has_state(state::SKIN_CUT | state::VESSELS_UNCLAMPED));
        // An arm's bleeding: the wound's flow, the vessels counted once (0.375 - 1.5).
        assert!((limb.bleed_rate(BodyZone::LArm, false) - (2.75 + 0.375 - 1.5)).abs() < 1e-5);
        // Clotting 0.02/s takes it below 2 after 37.5 s: demoted to a moderate cut.
        let mut rng = CombatRng(1);
        let ctx = TickCtx {
            secs: 2.0,
            lying: false,
            bodytemperature: BODYTEMP_NORMAL,
            nutrition: 400.0,
            burn_damage: 0.0,
            has_blood: true,
        };
        for _ in 0..19 {
            process(&mut rng, &mut limb, BodyZone::LArm, ctx);
        }
        assert_eq!(limb.wounds[0].kind, WoundKind::SlashModerate);
        assert!(limb.surgery == 0, "the severe cut's states left with it");
        // Gauze soaks 0.125/s more: the cut closes.
        limb.gauze = Some(Gauze {
            name: "марля".into(),
            absorption_capacity: 5.0,
            absorption_rate: 0.125,
            splint_factor: 0.7,
            burn_cleanliness_bonus: 0.35,
        });
        let mut ticks = 0;
        while !limb.wounds.is_empty() {
            process(&mut rng, &mut limb, BodyZone::LArm, ctx);
            ticks += 1;
            assert!(ticks < 20);
        }
        assert!(limb.gauze.as_ref().unwrap().absorption_capacity < 5.0);
    }

    #[test]
    fn splints_and_disabled_legs_limit_walking() {
        let mut limbs = Limbs::default();
        let health = Health::default();
        apply_wound(limbs.limb_mut(BodyZone::LLeg), WoundKind::Compound);
        assert!(limbs.limb(BodyZone::LLeg).wounds[0].disabling);
        // One leg out (3 ds) and its limp 7 × 70% on every other step.
        let l = limbs.limits(&health);
        assert!((l.slowdown - (3.0 + 7.0 * 0.7 / 2.0)).abs() < 1e-4, "{l:?}");
        assert!(!l.floored);
        apply_wound(limbs.limb_mut(BodyZone::RLeg), WoundKind::Compound);
        assert!(limbs.limits(&health).floored);
    }
}

// World side: treating wounds with tools (tg `try_treating` -> `treat`).

/// What a wound treatment `do_after` does at its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Treat {
    /// `tool_cauterize` of a cut or puncture.
    Cauterize,
    /// A bonesetter on a dislocation.
    SetBone,
    /// Bone gel on a fracture.
    Gel,
    /// Surgical tape on a gelled fracture.
    Tape,
}

fn treatment_for(
    wound: &Wound,
    tool: &super::surgery::ToolInfo,
    self_treat: bool,
    prepped: bool,
) -> Option<Treat> {
    let path = tool.path.as_deref().unwrap_or_default();
    match wound.def().series {
        Series::Slash | Series::Pierce => (tool.behaviour == "cautery"
            || (self_treat
                && tool.temperature >= super::surgery::FIRE_MINIMUM_TEMPERATURE_TO_EXIST))
            .then_some(Treat::Cauterize),
        Series::Bone if wound.kind == WoundKind::Dislocation => {
            (tool.behaviour == "bonesetter").then_some(Treat::SetBone)
        }
        // `item_can_treat` leaves a limb ready to operate on to real surgery.
        Series::Bone if !prepped => {
            if crate::obj_damage::is_under(path, "/obj/item/stack/medical/bone_gel") {
                Some(Treat::Gel)
            } else if crate::obj_damage::is_under(
                path,
                "/obj/item/stack/medical/wrap/sticky_tape/surgical",
            ) {
                Some(Treat::Tape)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// tg `/datum/wound/try_treating` over the wounds of the user's selected zone. True when a
/// wound took the click (a treatment started or was refused).
pub fn try_treating(world: &mut World, actor: crate::tools::Actor, patient: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let zone = world
        .get::<crate::health::ZoneSelected>(actor.mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone();
    let Some(limb) = world.get::<Limbs>(patient).map(|l| l.limb(zone).clone()) else {
        return false;
    };
    let tool = super::surgery::ToolInfo::read(world, Some(item));
    if super::surgery::combat_mode(world, actor.mob) && tool.force > 0.0 {
        return false;
    }
    let self_treat = patient == actor.mob;
    let Some((treat, kind)) = limb
        .wounds
        .iter()
        .find_map(|w| treatment_for(w, &tool, self_treat, limb.prepped).map(|t| (t, w.kind)))
    else {
        return false;
    };
    if world.get::<crate::tools::DoAfter>(actor.mob).is_some() {
        let name = super::name_of(world, patient);
        crate::tools::tell(
            world,
            actor.client,
            Span::Warning,
            format!("Вы уже взаимодействуете с {name}!"),
        );
        return true;
    }
    if !crate::chem::try_inject(world, actor, patient) {
        return true;
    }
    start_treatment(world, actor, patient, zone, treat, kind, &tool);
    true
}

fn start_treatment(
    world: &mut World,
    actor: crate::tools::Actor,
    patient: Entity,
    zone: BodyZone,
    treat: Treat,
    kind: WoundKind,
    tool: &super::surgery::ToolInfo,
) {
    let self_mult = if patient == actor.mob { 1.5 } else { 1.0 };
    let (user, target) = (
        super::name_of(world, actor.mob),
        super::name_of(world, patient),
    );
    let item = actor
        .item
        .map(|i| super::name_of(world, i))
        .unwrap_or_default();
    let part = zone.ru().1;
    let (delay, own, others) = match treat {
        Treat::Cauterize => {
            let improv = if tool.behaviour == "cautery" {
                1.0
            } else {
                1.25
            };
            (
                30.0 * self_mult * improv,
                format!("Вы начинаете прижигать {part} у {target} с помощью {item}..."),
                format!("{user} начинает прижигать {part} у {target} с помощью {item}..."),
            )
        }
        Treat::SetBone => (
            50.0 * self_mult,
            format!("Вы начинаете вправлять {part} у {target} с помощью {item}..."),
            format!("{user} начинает вправлять {part} у {target} с помощью {item}..."),
        ),
        Treat::Gel => {
            let gelled = world
                .get::<Limbs>(patient)
                .and_then(|l| l.limb(zone).wound(Series::Bone).map(|w| w.gelled))
                .unwrap_or(true);
            if gelled {
                return;
            }
            (
                50.0 * 1.5 * self_mult,
                format!("Вы начинаете наносить {item} на {part} у {target}..."),
                format!("{user} начинает наносить {item} на {part} у {target}..."),
            )
        }
        Treat::Tape => {
            let ready = world
                .get::<Limbs>(patient)
                .and_then(|l| {
                    l.limb(zone)
                        .wound(Series::Bone)
                        .map(|w| w.gelled && !w.taped)
                })
                .unwrap_or(false);
            if !ready {
                return;
            }
            (
                50.0 * self_mult,
                format!("Вы начинаете обматывать {part} у {target} с помощью {item}..."),
                format!("{user} начинает обматывать {part} у {target} с помощью {item}..."),
            )
        }
    };
    super::visible(world, actor.mob, &own, &others, Span::Warning);
    crate::tools::do_after(
        world,
        actor,
        patient,
        delay,
        0,
        crate::construction::Job::Medical(super::MedJob::Treat { zone, treat, kind }),
    );
}

/// The end of a wound treatment.
pub fn finish_treatment(
    world: &mut World,
    actor: crate::tools::Actor,
    patient: Entity,
    zone: BodyZone,
    treat: Treat,
    kind: WoundKind,
) {
    let Some(mut limbs) = world.get::<Limbs>(patient).cloned() else {
        return;
    };
    let series = kind.def().series;
    // `still_exists`: that very wound is still there; one that worsened meanwhile is a new
    // wound (`replace_wound`), so a dislocation turned compound fracture is not set.
    let Some(index) = limbs.limb(zone).wounds.iter().position(|w| w.kind == kind) else {
        return;
    };
    let self_treat = patient == actor.mob;
    let self_mult = if self_treat { 1.5 } else { 1.0 };
    let mut rng = super::fork_rng(world);
    let (user, target) = (
        super::name_of(world, actor.mob),
        super::name_of(world, patient),
    );
    match treat {
        Treat::Cauterize => {
            let tool = super::surgery::ToolInfo::read(world, actor.item);
            let improv = if tool.behaviour == "cautery" {
                1.0
            } else {
                1.25
            };
            let severity = f32::from(limbs.limb(zone).wounds[index].def().severity);
            let limb = limbs.limb_mut(zone);
            let by = -0.6 / (self_mult * improv);
            let left = match series {
                Series::Slash => {
                    let mut effects = Vec::new();
                    adjust_slash(limb, index, by, zone, &mut effects);
                    limb.wounds
                        .iter()
                        .find(|w| w.def().series == Series::Slash)
                        .is_some_and(|w| w.blood_flow > w.def().minimum_flow)
                }
                _ => adjust_pierce(limb, index, by),
            };
            world.entity_mut(patient).insert(limbs);
            super::hurt_limb(world, patient, zone, DamageType::Burn, 2.0 + severity);
            super::visible(
                world,
                actor.mob,
                &format!("Вы прижигаете часть кровотечений у {target}."),
                &format!("{user} прижигает часть кровотечений у {target}."),
                Span::Notice,
            );
            if left {
                try_treating(world, actor, patient);
            }
        }
        Treat::SetBone => {
            remove_wound(limbs.limb_mut(zone), index);
            world.entity_mut(patient).insert(limbs);
            super::hurt_limb(
                world,
                patient,
                zone,
                DamageType::Brute,
                if self_treat { 15.0 } else { 10.0 },
            );
            super::visible(
                world,
                actor.mob,
                &format!("Вы вправляете {} у {target} на место!", zone.ru().1),
                &format!("{user} вправляет {} у {target} на место!", zone.ru().1),
                Span::Notice,
            );
        }
        Treat::Gel => {
            let Some(item) = actor.item else {
                return;
            };
            if limbs.limb(zone).wounds[index].gelled || !crate::tools::use_stack(world, item, 1) {
                return;
            }
            let severity = f32::from(kind.def().severity);
            if self_treat && rng.chance(25.0 + 20.0 * (severity - 2.0)) {
                super::visible(
                    world,
                    actor.mob,
                    "Вы теряете сознание от боли, нанося гель на кость!",
                    &format!("{user} теряет сознание от боли!"),
                    Span::Danger,
                );
                crate::status_effects::apply(
                    world,
                    patient,
                    crate::status_effects::Incap::Paralyze,
                    5.0,
                );
                return;
            }
            limbs.limb_mut(zone).wounds[index].gelled = true;
            world.entity_mut(patient).insert(limbs);
            super::hurt_limb(world, patient, zone, DamageType::Brute, 25.0);
            crate::status_effects::adjust_stamina(world, patient, 100.0);
            super::visible(
                world,
                actor.mob,
                &format!(
                    "Вы наносите гель на {} у {target}, кость начинает срастаться!",
                    zone.ru().1
                ),
                &format!("{user} наносит гель на {} у {target}!", zone.ru().1),
                Span::Notice,
            );
        }
        Treat::Tape => {
            let Some(item) = actor.item else {
                return;
            };
            let wound = &limbs.limb(zone).wounds[index];
            if !wound.gelled || wound.taped || !crate::tools::use_stack(world, item, 1) {
                return;
            }
            let wound = &mut limbs.limb_mut(zone).wounds[index];
            if self_treat {
                wound.regen_ticks_needed = (wound.regen_ticks_needed as f32 * 1.5) as u32;
            }
            wound.taped = true;
            world.entity_mut(patient).insert(limbs);
            super::visible(
                world,
                actor.mob,
                &format!(
                    "Вы закрепляете {} у {target} хирургической лентой.",
                    zone.ru().1
                ),
                &format!(
                    "{user} закрепляет {} у {target} хирургической лентой.",
                    zone.ru().1
                ),
                Span::Notice,
            );
        }
    }
}

/// tg `receive_damage`'s wound handling after a hit landed on `victim`'s `zone` (melee, thrown
/// items): the roll, the existing wounds' reactions, the messages, blood lost at once and a
/// dropped item.
pub fn after_hit(world: &mut World, victim: Entity, zone: BodyZone, hit: Hit) {
    let Some(mut limbs) = world.get::<Limbs>(victim).cloned() else {
        return;
    };
    // The hit went to the chest (`crate::health::Health::damage_zone`).
    let zone = if limbs.limb(zone).missing {
        BodyZone::Chest
    } else {
        zone
    };
    let dead = world.get::<Stat>(victim) == Some(&Stat::Dead);
    let has_blood = world
        .get::<super::blood::Blood>(victim)
        .is_some_and(|b| b.volume > 0.0);
    let can_dismember = super::bodyparts::can_dismember(world, victim, zone);
    let mut rng = super::fork_rng(world);
    let out = receive_damage(
        &mut rng,
        limbs.limb_mut(zone),
        zone,
        hit,
        dead,
        has_blood,
        can_dismember,
    );
    world.entity_mut(victim).insert(limbs);
    if let Some((wounding, outright)) = out.dismember {
        super::bodyparts::apply_dismember(world, victim, zone, wounding, outright);
        return;
    }
    if let Some(kind) = out.gained {
        let def = kind.def();
        let name = super::name_of(world, victim);
        let part = capitalize(zone.ru().0);
        let others = format!("{part} {name} {}!", def.occur_text);
        let own = format!("Ваша {} {}!", zone.ru().0, def.occur_text);
        let range = if def.severity > 2 {
            crate::health::DEFAULT_MESSAGE_RANGE
        } else {
            crate::health::COMBAT_MESSAGE_RANGE
        };
        super::visible_range(world, victim, &own, &others, Span::Danger, range);
    }
    for (amount, text) in out.bled {
        super::blood::bleed(world, victim, amount);
        if !text.is_empty()
            && let Some(client) = world
                .get::<crate::server::PlayerMob>(victim)
                .map(|p| p.client)
        {
            crate::tools::tell(world, client, Span::Danger, text);
        }
    }
    if let Some(hand) = out.drop_hand
        && let Some(item) = world
            .get::<crate::inventory::Inventory>(victim)
            .and_then(|inv| inv.hands[hand])
        && let Some(&pos) = world.get::<ss13_shared::TilePos>(victim)
    {
        world
            .entity_mut(item)
            .remove::<crate::inventory::CarriedBy>()
            .insert(pos);
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
