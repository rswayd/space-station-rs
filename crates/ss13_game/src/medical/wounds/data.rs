//! The wound table: one `WoundDef` per tg wound type.

use super::*;

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
