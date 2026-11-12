//! The vampire (server) after BandaStation's `/datum/antagonist/vampire`
//! (`modular_bandastation/vampire/code/datum_vampire.dm`, `vampire_bite.dm`, `spell_handler.dm`,
//! `vampire_spell_passives.dm`, `vampire_subclasses.dm`, `vampire_objectives.dm`,
//! `vampire_holywater.dm`, `vampire_powers/*`).
//!
//! - [`add_vampire`] is `add_antag_datum(/datum/antagonist/vampire)`: the objectives (blood,
//!   specialization / steal / protect, assassinate, lair, escape / survive), the powers of 0
//!   blood, the greeting, the info panel ([`AntagInfoVampireUi`]), the antag HUD icon, the usable
//!   blood counter ([`VampireHudUi`]) and the action buttons ([`crate::actions`]).
//! - Biting (`vampire_biter`): an empty-handed click in combat mode on a human's head starts a
//!   loop of 5 s `do_after`s; each drains 25 units of the victim's [`Blood`], and while the victim
//!   lives and had a player (and gave less than 200 in total) gives the vampire
//!   `min(20, drained) × 0.5` blood, total and usable; it also feeds the vampire.
//! - Powers unlock by total blood (`upgrade_tiers`): rejuvenate and glare at 0, vision at 100,
//!   the specialization and the lair at 150, regeneration at 200, advanced vision at 500; then the
//!   specialization's own powers by blood, and its full powers at 8 victims and 1000 blood.
//!   A spell costs its blood times `1 + nullified / 100`; it fails dead, at 100 nullification, in
//!   the chapel (until full power) and without the blood.
//! - Weaknesses each `Life()`: starlight on a space tile not shaded from the sun (10 usable blood
//!   and a burn roll, or burning to death without blood), the chapel (a burn roll while holding
//!   blood), holy water in the blood (nullification, lost blood, burning), fire (5 usable blood);
//!   nullification decays by 2.
//! - [`roundend_report`]: the vampires, their objectives and the blood they drank.
// shortcut: implemented spells are rejuvenate, glare, the lair, the umbrae cloak, the gargantua
// blood rush and the dantalion subspace swap; the other specialization powers (thralls, blood
// magic, shadow snares, stomp, ...) are listed in the info panel but not granted, until their
// systems (projectiles, beams, illusions, physiology multipliers) exist. Vision passives are
// granted without the client's lighting cutoff or thermal/x-ray sight; no coffin regeneration
// (no sleeping yet), no holy weapons (no null rods), no holy water splashes (only in the blood),
// no stinger sound, moodlet or flash overlay of the glare, no bodyguard or thralls.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::antag::StealItemDef;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{Areas, DmItem, IconIndex, Opaque};
use ss13_net::server::ClientToken;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use super::{
    AntagInfoVampireUi, PowerInfo, SubclassInfo, VampireHudData, VampireHudUi, VampireInfoData,
    VampireInfoStatic, blood_text,
};
use crate::actions::{ActionButton, ActionButtonsUi, set_buttons};
use crate::antag::dynamic::DynamicRng;
use crate::antag::objectives::{
    Objective, ObjectiveKind, RoundEnd, considered_alive, find_steal_target, find_target,
    target_text,
};
use crate::antag::uplink::UplinkCatalog;
use crate::antag::{AntagHudData, AntagHudUi, ObjectiveInfo};
use crate::click::{ClickIntercept, InterceptedClick};
use crate::combat::{CombatMode, CombatRng};
use crate::construction::{Job, path_of, turf_at};
use crate::health::{
    BodyPosition, DamageType, Health, MoveSpeedModifier, Stat, ZoneSelected, apply_damage,
};
use crate::hunger::{Hunger, NUTRITION_LEVEL_WELL_FED};
use crate::inventory::Inventory;
use crate::jobs::{Jobs, PlayerJob};
use crate::medical::blood::{BLOOD_VOLUME_BAD, BLOOD_VOLUME_MAXIMUM, BLOOD_VOLUME_OKAY, Blood};
use crate::medical::{name_of, visible};
use crate::obj_damage::{Anchored, is_under};
use crate::server::PlayerMob;
use crate::status_effects::{Incap, effects_mut, incapacitation};
use crate::tools::{Actor, DoAfter, do_after, tell};
use crate::zone::BodyZone;

mod bite;
mod gain;
mod hemomancer;
mod life;
mod spells;
mod windows;
pub use bite::*;
pub use gain::*;
pub use hemomancer::{TendrilSlow, TendrilStrike, report_prey, slow};
use life::*;
pub use spells::*;
pub use windows::*;

/// `name = "\proper Вампир"`.
pub const VAMPIRE_NAME: &str = "Вампир";
/// `roundend_category`.
pub const ROUNDEND_CATEGORY: &str = "Вампиры";
/// `BLOOD_DRAIN_LIMIT`: the blood one victim can give.
pub const BLOOD_DRAIN_LIMIT: f32 = 200.0;
/// `FULLPOWER_DRAINED_REQUIREMENT`, `FULLPOWER_BLOODTOTAL_REQUIREMENT`.
pub const FULLPOWER_DRAINED_REQUIREMENT: usize = 8;
pub const FULLPOWER_BLOODTOTAL_REQUIREMENT: f32 = 1000.0;
/// `VAMPIRE_NULLIFICATION_CAP`, `VAMPIRE_COMPLETE_NULLIFICATION`.
pub const NULLIFICATION_CAP: f32 = 120.0;
pub const COMPLETE_NULLIFICATION: f32 = 100.0;
/// `BLOOD_GAINED_MODIFIER` and the most blood one bite counts.
const BLOOD_GAINED_MODIFIER: f32 = 0.5;
const BITE_GAIN_CAP: f32 = 20.0;
/// `handle_bloodsucking(suck_rate = 5 SECONDS)`, deciseconds; `bloodsuck_tick(drain_amount = 25)`.
pub const SUCK_RATE_DS: f32 = 50.0;
pub const DRAIN_AMOUNT: f32 = 25.0;
/// The lair ritual's `do_after`, deciseconds.
const LAIR_RITUAL_DS: f32 = 100.0;
/// `/datum/status_effect/vampire_rejuvenation` duration; `vampire_blood_rush` duration.
const REJUVENATION_SECS: f64 = 14.0;
const BLOOD_RUSH_SECS: f64 = 10.0;
/// `/datum/movespeed_modifier/vampire_blood_rush`, `vampire_cloak`, deciseconds.
const BLOOD_RUSH_SPEED: f32 = -0.5;
const CLOAK_SPEED: f32 = -0.25;
/// The glare's charges and their recharge.
const GLARE_CHARGES: u8 = 2;
const GLARE_RECHARGE_SECS: f64 = 30.0;
/// The objective's specialization threshold (`vampire/specialization/check_completion`).
const SPECIALIZATION_BLOOD: f32 = 150.0;
const OBJECTIVE_NAME: &str = "Задача";

const ANTAG_HUD: (&str, &str) = (
    "modular_bandastation/vampire/icons/mob/huds/vampire_antag.dmi",
    "vampire",
);
const ACTIONS_DMI: &str = "modular_bandastation/vampire/icons/mob/actions/actions.dmi";
const ITEMS_DMI: &str = "modular_bandastation/vampire/icons/obj/items.dmi";
const COFFIN: &str = "/obj/structure/closet/crate/coffin";
const CHAPEL: &str = "/area/station/service/chapel";
const SPACE_TURF: &str = "/turf/open/space";
const HOLY_WATER: &str = "/datum/reagent/water/holywater";
const BLINDFOLD: &str = "/obj/item/clothing/glasses/blindfold";

/// tg `/datum/vampire_passive/*` (the spell-granting ones grant their [`Spell`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Power {
    Rejuvenate,
    Glare,
    Vision,
    UnlockSpecialization,
    Lair,
    Regen,
    VisionAdvanced,
    Full,
    VisionFull,
    VisionXray,
    Cloak,
    BloodRush,
    SwitchPlaces,
    BloodTendrils,
    PredatorSenses,
    BloodEruption,
}

impl Power {
    pub fn spell(self) -> Option<Spell> {
        Some(match self {
            Self::Rejuvenate => Spell::Rejuvenate,
            Self::Glare => Spell::Glare,
            Self::Lair => Spell::Lair,
            Self::Cloak => Spell::Cloak,
            Self::BloodRush => Spell::BloodRush,
            Self::SwitchPlaces => Spell::SwitchPlaces,
            Self::BloodTendrils => Spell::BloodTendrils,
            Self::PredatorSenses => Spell::PredatorSenses,
            Self::BloodEruption => Spell::BloodEruption,
            _ => return None,
        })
    }

    /// `gain_desc`.
    pub fn gain_desc(self) -> &'static str {
        match self {
            Self::Rejuvenate => "Теперь вы можете использовать омоложение.",
            Self::Glare => "Теперь вы можете использовать взгляд.",
            Self::Vision => "Ваше вампирское зрение улучшилось.",
            Self::UnlockSpecialization => {
                "Теперь вы можете выбрать вампирскую специализацию для развития."
            }
            Self::Lair => "Теперь вы можете создать логово.",
            Self::Regen => {
                "Ваши способности омоложения улучшились и теперь при использовании исцеляют вас \
                 со временем."
            }
            Self::VisionAdvanced => "Теперь ваше вампирское зрение позволяет видеть всё во тьме!",
            Self::Full => {
                "Вы достигли полного потенциала. Святые предметы и эффекты больше не являются \
                 вашей слабостью."
            }
            Self::VisionFull => "Ваше вампирское зрение достигло полной силы!",
            Self::VisionXray => "Теперь вы видите сквозь стены — если вдруг не заметили.",
            Self::Cloak => {
                "Вы обрели способность «Покров тьмы»: во тьме она делает вас почти невидимым и \
                 очень ловким."
            }
            Self::BloodRush => "Вы обрели способность временно двигаться с высокой скоростью.",
            Self::SwitchPlaces => "Вы обрели способность меняться местами с выбранным существом.",
            Self::BloodTendrils => {
                "Вы обрели способность призывать кровавые щупальца, замедляющие людей в выбранной \
                 области."
            }
            Self::PredatorSenses => "Ваши чувства обострились: теперь от вас никто не скроется.",
            Self::BloodEruption => {
                "Вы обрели способность превращать лужи крови в оружие против стоящих на них."
            }
        }
    }
}

/// `upgrade_tiers`: the powers every vampire unlocks, by total blood.
pub const UPGRADE_TIERS: [(Power, f32); 7] = [
    (Power::Rejuvenate, 0.0),
    (Power::Glare, 0.0),
    (Power::Vision, 100.0),
    (Power::UnlockSpecialization, 150.0),
    (Power::Lair, 150.0),
    (Power::Regen, 200.0),
    (Power::VisionAdvanced, 500.0),
];

/// tg `/datum/action/cooldown/spell/*` of the vampire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spell {
    Rejuvenate,
    Glare,
    Lair,
    Cloak,
    BloodRush,
    SwitchPlaces,
    BloodTendrils,
    PredatorSenses,
    BloodEruption,
}

/// A spell's button and casting values (`add_vampire_ability(required_blood)`).
#[derive(Clone, Copy, Debug)]
pub struct SpellDef {
    /// The action button's id.
    pub id: &'static str,
    /// `name`, with ` (cost)` as `add_vampire_ability` appends it.
    pub name: &'static str,
    pub icon: (&'static str, &'static str),
    pub cost: u32,
    pub cooldown: f64,
    /// `check_flags & AB_CHECK_CONSCIOUS` (rejuvenate and glare only check phasing).
    pub conscious: bool,
    /// A pointed spell's `cast_range`.
    pub range: Option<u32>,
}

impl Spell {
    pub const ALL: [Spell; 9] = [
        Spell::Rejuvenate,
        Spell::Glare,
        Spell::Lair,
        Spell::Cloak,
        Spell::BloodRush,
        Spell::SwitchPlaces,
        Spell::BloodTendrils,
        Spell::PredatorSenses,
        Spell::BloodEruption,
    ];

    pub fn def(self) -> SpellDef {
        let def = |id, name, state, cost, cooldown, conscious, range| SpellDef {
            id,
            name,
            icon: (ACTIONS_DMI, state),
            cost,
            cooldown,
            conscious,
            range,
        };
        match self {
            Self::Rejuvenate => def(
                "vampire_rejuvenate",
                "Омоложение",
                "vampire_rejuvinate",
                0,
                20.0,
                false,
                None,
            ),
            Self::Glare => def(
                "vampire_glare",
                "Взгляд",
                "vampire_glare",
                0,
                2.0,
                false,
                None,
            ),
            // tg's coffin icon is too big: the modpack's own.
            Self::Lair => SpellDef {
                icon: (ITEMS_DMI, "coffin"),
                ..def("vampire_lair", "Логово", "", 0, 2.0, true, Some(1))
            },
            Self::Cloak => def(
                "vampire_cloak",
                "Покров тьмы",
                "vampire_cloak",
                0,
                2.0,
                true,
                None,
            ),
            Self::BloodRush => def(
                "vampire_blood_rush",
                "Кровавый рывок (30)",
                "blood_rush",
                30,
                30.0,
                true,
                None,
            ),
            Self::SwitchPlaces => def(
                "vampire_switch_places",
                "Подпространственный обмен (30)",
                "subspace_swap",
                30,
                30.0,
                true,
                Some(7),
            ),
            Self::BloodTendrils => def(
                "vampire_blood_tendrils",
                "Кровавые щупальца (10)",
                "blood_tendrils",
                10,
                30.0,
                true,
                Some(7),
            ),
            Self::PredatorSenses => def(
                "vampire_predator_senses",
                "Чувства хищника",
                "predator_sense",
                0,
                20.0,
                true,
                None,
            ),
            Self::BloodEruption => def(
                "vampire_blood_eruption",
                "Кровавое извержение (100)",
                "blood_spikes",
                100,
                200.0,
                true,
                None,
            ),
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.def().id == id)
    }
}

/// A granted spell: `next_use_time`, and the glare's charges with their recharge times.
#[derive(Clone, Debug, PartialEq)]
pub struct SpellState {
    pub spell: Spell,
    pub ready_at: f64,
    pub charges: u8,
    pub recharges: Vec<f64>,
}

/// tg `/datum/vampire_subclass/*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Subclass {
    Umbrae,
    Hemomancer,
    Gargantua,
    Dantalion,
    /// The admin "Ancient Vampire" (`/datum/outfit/ancient_vampire`): never picked.
    Ancient,
}

/// One row of a specialization's power list in the info panel (`get_power_ui_data`).
struct UiPower(Option<&'static str>, &'static str, Option<u32>);

impl Subclass {
    /// The info panel's order.
    pub const ALL: [Subclass; 4] = [
        Subclass::Umbrae,
        Subclass::Hemomancer,
        Subclass::Gargantua,
        Subclass::Dantalion,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Umbrae => "umbrae",
            Self::Hemomancer => "hemomancer",
            Self::Gargantua => "gargantua",
            Self::Dantalion => "dantalion",
            Self::Ancient => "ancient",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Umbrae => "Умбра",
            Self::Hemomancer => "Гемомант",
            Self::Gargantua => "Гаргантюа",
            Self::Dantalion => "Данталион",
            Self::Ancient => "Древний",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Umbrae => "Специализируется на тьме, скрытности, засадах и мобильности.",
            Self::Hemomancer => "Специализируется на магии крови и управлении окружающей кровью.",
            Self::Gargantua => "Специализируется на стойкости и уроне в ближнем бою.",
            Self::Dantalion => "Специализируется на порабощении и иллюзиях.",
            Self::Ancient => "",
        }
    }

    /// `standard_powers` this server implements, with their blood.
    pub fn standard_powers(self) -> &'static [(Power, f32)] {
        match self {
            Self::Umbrae => &[(Power::Cloak, 150.0)],
            Self::Hemomancer => &[
                (Power::BloodTendrils, 250.0),
                (Power::PredatorSenses, 600.0),
                (Power::BloodEruption, 800.0),
            ],
            Self::Gargantua => &[(Power::BloodRush, 250.0)],
            Self::Dantalion => &[(Power::SwitchPlaces, 250.0)],
            // tg lists them without blood: `bloodtotal >= null` always holds.
            Self::Ancient => &[
                (Power::Cloak, 0.0),
                (Power::BloodTendrils, 0.0),
                (Power::BloodRush, 0.0),
                (Power::SwitchPlaces, 0.0),
                (Power::PredatorSenses, 0.0),
                (Power::BloodEruption, 0.0),
                (Power::Full, 0.0),
                (Power::VisionFull, 0.0),
                (Power::VisionXray, 0.0),
            ],
        }
    }

    /// `fully_powered_abilities` this server implements.
    pub fn full_powers(self) -> &'static [Power] {
        match self {
            Self::Umbrae => &[Power::Full, Power::VisionFull, Power::VisionXray],
            Self::Ancient => &[],
            _ => &[Power::Full, Power::VisionFull],
        }
    }

    /// `improved_rejuv_healing`.
    pub fn improved_rejuv_healing(self) -> bool {
        matches!(self, Self::Gargantua | Self::Ancient)
    }

    fn unique_objectives(self) -> &'static [&'static str] {
        match self {
            Self::Umbrae => &[
                "Выведите из строя телекоммуникационное оборудование станции. Их крики никто не \
                 услышит.",
                "Окутайте %DEPARTMENT тьмой.",
                "Покажите станции, почему боятся темноты.",
            ],
            Self::Hemomancer => &[
                "Лишите медотсек крови. Она не для них.",
                "Окрасьте %DEPARTMENT в красный кровью тех, кто выступит против вас.",
                "Покажите станции, что вы на вершине силы.",
            ],
            Self::Gargantua => &[
                "Уничтожьте серверы научного отдела. Технологии не заменят силу.",
                "Разгромите %DEPARTMENT. Они стали слишком самоуверенными.",
                "Покажите станции, что вы на вершине силы.",
            ],
            Self::Dantalion => &[
                "Подчините сотрудника службы безопасности. Его потенциал растрачивается в рядах \
                 Нанотрейзен.",
                "Подчиняйте только сотрудников %DEPARTMENT. Их опыт поможет вашему повелителю.",
                "Управляйте станцией из теней.",
            ],
            Self::Ancient => {
                &["Ваши ряды редеют. Позаботьтесь, чтобы ваши рабы оставались здоровы."]
            }
        }
    }

    /// Every standard and full power of the fork, as the info panel lists them.
    fn ui_powers(self) -> (&'static [UiPower], &'static [UiPower]) {
        const FULL: &str = "Вы достигли полного потенциала. Святые предметы и эффекты больше не \
                            являются вашей слабостью.";
        const VISION_FULL: &str = "Ваше вампирское зрение достигло полной силы!";
        match self {
            Self::Umbrae => (
                &[
                    UiPower(
                        Some("Покров тьмы"),
                        "Включает или выключает сокрытие во тьме. Во тьме при включённой \
                         способности вы движетесь быстрее.",
                        Some(150),
                    ),
                    UiPower(
                        Some("Теневая ловушка (20)"),
                        "Призовите ловушку на полу. Тот, кто пересечёт её, ослепнет, погасит свои \
                         источники света и окажется опутанным.",
                        Some(250),
                    ),
                    UiPower(
                        Some("Якорь души (30)"),
                        "После задержки призовите межпространственный якорь. Повторное применение \
                         телепортирует вас к нему. Через две минуты вас принудительно вернёт назад.",
                        Some(250),
                    ),
                    UiPower(
                        Some("Тёмный проход (30)"),
                        "Телепортируйтесь на выбранную клетку.",
                        Some(400),
                    ),
                    UiPower(
                        Some("Погасить"),
                        "Погасите все источники света вокруг себя.",
                        Some(600),
                    ),
                    UiPower(
                        Some("Теневой бой (50)"),
                        "Выберите кого-то, чтобы ваша тень избила его. Для работы нужно находиться \
                         не далее двух клеток.",
                        Some(800),
                    ),
                ],
                &[
                    UiPower(None, FULL, None),
                    UiPower(None, VISION_FULL, None),
                    UiPower(
                        Some("Вечная тьма (5)"),
                        "При включении вы окутываете область вокруг себя тьмой и медленно \
                         понижаете температуру тел людей поблизости. Энергетические снаряды в \
                         радиусе действия тускнеют.",
                        None,
                    ),
                    UiPower(
                        None,
                        "Теперь вы видите сквозь стены — если вдруг не заметили.",
                        None,
                    ),
                ],
            ),
            Self::Hemomancer => (
                &[
                    UiPower(
                        Some("Вампирские когти (30)"),
                        "Направьте магию крови, чтобы выковать смертоносные вампирские когти, \
                         высасывающие кровь и быстро наносящие удары. Нельзя использовать, если вы \
                         держите невыпадающий предмет.",
                        Some(150),
                    ),
                    UiPower(
                        Some("Кровавые щупальца (10)"),
                        "После задержки призовите небольшое поле ужасных кровавых щупалец, которое \
                         опутает и замедлит людей в области.",
                        Some(250),
                    ),
                    UiPower(
                        Some("Кровавый барьер (40)"),
                        "Выберите две точки на расстоянии не более трёх клеток друг от друга, \
                         чтобы создать между ними барьер.",
                        Some(250),
                    ),
                    UiPower(
                        Some("Кровавая лужа (50)"),
                        "Превратитесь в лужу крови, став неуязвимым и способным проходить через \
                         всё, кроме стен и космоса. При движении вы оставляете кровавый след.",
                        Some(400),
                    ),
                    UiPower(
                        Some("Чувства хищника"),
                        "Выследите добычу — ей негде спрятаться...",
                        Some(600),
                    ),
                    UiPower(
                        Some("Кровавое извержение (100)"),
                        "Из каждой лужи крови в радиусе четырёх клеток вырывается шип из живой \
                         крови, ранящий стоящих на ней.",
                        Some(800),
                    ),
                ],
                &[
                    UiPower(None, FULL, None),
                    UiPower(None, VISION_FULL, None),
                    UiPower(
                        Some("Ритуал несущего кровь (10)"),
                        "При включении все вокруг начинают обильно истекать кровью. Вы \
                         высасываете её и омолаживаете себя.",
                        None,
                    ),
                ],
            ),
            Self::Gargantua => (
                &[
                    UiPower(
                        Some("Кровавое усиление (30)"),
                        "Наполните тело кровью, чтобы значительно повысить сопротивление к \
                         оглушению и физическому урону. Пока способность активна, вы не можете \
                         стрелять из дальнобойного оружия.",
                        Some(150),
                    ),
                    UiPower(
                        Some("Кровавый рывок (30)"),
                        "Наполните себя магией крови, чтобы ускориться и освободиться от пут на \
                         ногах.",
                        Some(250),
                    ),
                    UiPower(
                        Some("Сейсмический топот (30)"),
                        "Ударьте ногой о пол, пустив по корпусу станции мощную ударную волну, \
                         отбрасывающую людей. Нельзя использовать со связанными болой или \
                         подобным предметом ногами.",
                        Some(250),
                    ),
                    UiPower(
                        None,
                        "Пока кровавое усиление активно, все ваши атаки в ближнем бою наносят \
                         больше урона.",
                        Some(400),
                    ),
                    UiPower(
                        Some("Подавляющая сила"),
                        "Включите силу, чтобы выбивать двери, в которые вы врезаетесь. Используйте \
                         способность повторно, чтобы отключить её.",
                        Some(600),
                    ),
                    UiPower(
                        Some("Таран (30)"),
                        "Рваните к точке на экране, нанося большой урон, оглушая цели и разрушая \
                         стены и другие объекты.",
                        Some(800),
                    ),
                    UiPower(
                        Some("Демоническая хватка (20)"),
                        "Призовите руку демонической энергии, которая опутает и швырнёт цель: в \
                         боевом режиме толкнёт её, в обычном — притянет к вам.",
                        Some(800),
                    ),
                ],
                &[
                    UiPower(None, FULL, None),
                    UiPower(None, VISION_FULL, None),
                    UiPower(
                        Some("Осквернённая дуэль (150)"),
                        "Прыгните к кому-то. При приземлении вы создадите арену, где будете лечить \
                         физический урон и ожоги, быстрее восстанавливаться от усталости и лучше \
                         сопротивляться длительному урону. Повторное применение завершит \
                         заклинание раньше.",
                        None,
                    ),
                ],
            ),
            Self::Dantalion => (
                &[
                    UiPower(
                        Some("Подчинение (150)"),
                        "Используйте большую часть своей силы, чтобы обратить верность тех, кто \
                         никому не предан, только себе.",
                        Some(150),
                    ),
                    UiPower(
                        Some("Общение с рабами"),
                        "Телепатически общайтесь со своими рабами и вампиром-хозяином.",
                        Some(150),
                    ),
                    UiPower(
                        Some("Усмирение (10)"),
                        "Временно усмирите цель, лишив её возможности причинять вред.",
                        Some(250),
                    ),
                    UiPower(
                        Some("Подпространственный обмен (30)"),
                        "Поменяйтесь местами с целью.",
                        Some(250),
                    ),
                    UiPower(
                        Some("Создать приманку (30)"),
                        "Ненадолго станьте невидимым и создайте иллюзию-приманку, чтобы обмануть \
                         добычу.",
                        Some(400),
                    ),
                    UiPower(
                        None,
                        "Теперь вы можете подчинить ещё одного человека — вплоть до 2.",
                        Some(400),
                    ),
                    UiPower(
                        Some("Сбор рабов (100)"),
                        "Снимает все обездвиживающие эффекты с ваших рабов поблизости.",
                        Some(600),
                    ),
                    UiPower(
                        None,
                        "Теперь вы можете подчинить ещё одного человека — вплоть до 3.",
                        Some(600),
                    ),
                    UiPower(
                        Some("Кровавая связь (5)"),
                        "Создаёт между вами и ближайшими рабами сеть, поровну распределяющую весь \
                         получаемый урон.",
                        Some(800),
                    ),
                ],
                &[
                    UiPower(None, FULL, None),
                    UiPower(
                        Some("Массовая истерия (70)"),
                        "Наложите мощную иллюзию: после краткого ослепления все поблизости будут \
                         видеть друг друга случайными животными.",
                        None,
                    ),
                    UiPower(None, VISION_FULL, None),
                    UiPower(
                        None,
                        "Теперь вы можете подчинить ещё одного человека — вплоть до 4.",
                        None,
                    ),
                ],
            ),
            Self::Ancient => (&[], &[]),
        }
    }

    /// `get_ui_data`.
    pub fn ui(self) -> SubclassInfo {
        let rows = |list: &[UiPower]| {
            list.iter()
                .map(|UiPower(name, description, blood)| PowerInfo {
                    name: name.map(str::to_owned),
                    description: (*description).to_owned(),
                    blood_required: *blood,
                })
                .collect()
        };
        let (powers, full) = self.ui_powers();
        SubclassInfo {
            id: self.id().to_owned(),
            name: self.name().to_owned(),
            description: self.description().to_owned(),
            powers: rows(powers),
            full_powers: rows(full),
        }
    }
}

/// The vampire's objectives (`forge_objectives`).
#[derive(Clone, Debug, PartialEq)]
pub enum VampireObjective {
    /// `/datum/objective/vampire/blood`.
    Blood { target: u32 },
    /// `/datum/objective/vampire/specialization`: the text once a specialization was picked.
    Specialization { text: Option<String> },
    /// `/datum/objective/vampire/lair`.
    Lair,
    /// `/datum/objective/protect`.
    Protect {
        target: Option<Entity>,
        text: String,
    },
    /// `/datum/objective/survive`.
    Survive,
    /// Steal, assassinate and escape, as the traitor's.
    Generic(Objective),
}

impl VampireObjective {
    pub fn explanation(&self) -> String {
        match self {
            Self::Blood { target } => {
                format!("Наберите в общей сложности не менее {target} единиц крови.")
            }
            Self::Specialization { text: Some(text) } => text.clone(),
            Self::Specialization { text: None } => "Накопите не менее 150 единиц крови и выберите \
                 специализацию, чтобы получить дальнейшие инструкции."
                .to_owned(),
            Self::Lair => "Создайте логово, сделав гроб своим убежищем.".to_owned(),
            Self::Protect { text, .. } => text.clone(),
            Self::Survive => "Оставайтесь в живых до самого конца.".to_owned(),
            Self::Generic(o) => o.explanation.clone(),
        }
    }

    pub fn check_completion(
        &self,
        world: &World,
        owner: Entity,
        vampire: &Vampire,
        end: RoundEnd,
    ) -> bool {
        match self {
            Self::Blood { target } => vampire.bloodtotal >= *target as f32,
            Self::Specialization { .. } => {
                vampire.subclass.is_some() && vampire.bloodtotal >= SPECIALIZATION_BLOOD
            }
            Self::Lair => vampire.has_lair,
            Self::Protect { target, .. } => target.is_none_or(|t| considered_alive(world, t)),
            Self::Survive => considered_alive(world, owner),
            Self::Generic(o) => o.check_completion(world, &[owner], end),
        }
    }
}

/// The vampire antagonist datum, on the body it was given to (the mind here).
#[derive(Component, Clone, Debug, Default)]
pub struct Vampire {
    /// The player's key for the report.
    pub key: String,
    pub bloodtotal: f32,
    pub bloodusable: f32,
    pub subclass: Option<Subclass>,
    /// `powers`: granted passives (spells are in [`Self::spells`]).
    pub powers: Vec<Power>,
    /// `spent_upgrade_tiers`.
    pub spent: Vec<Power>,
    pub spells: Vec<SpellState>,
    /// `drained_humans`: blood taken from each victim (tg keys them by `REF`).
    pub drained: Vec<(Entity, f32)>,
    pub nullified: f32,
    /// `draining`: the victim of the bite in progress.
    pub draining: Option<Entity>,
    pub has_lair: bool,
    pub objectives: Vec<VampireObjective>,
    /// `iscloaking` and the alpha the cloak imposes.
    pub cloaking: bool,
    pub cloak_alpha: Option<u8>,
    /// Whether the cloak's speed applies (dark tile).
    pub cloak_fast: bool,
    /// A pointed spell waiting for its target (tg `click_intercept`).
    pub aiming: Option<Spell>,
    /// `/datum/status_effect/vampire_rejuvenation`: its end and healing multiplier.
    pub rejuvenation: Option<(f64, f32)>,
    /// `/datum/status_effect/vampire_blood_rush` end.
    pub rush_until: f64,
    /// Holy water's `current_cycle` while it is in the blood.
    pub holy_cycles: u32,
}

impl Vampire {
    pub fn has(&self, power: Power) -> bool {
        self.powers.contains(&power)
    }

    pub fn spell(&self, spell: Spell) -> Option<&SpellState> {
        self.spells.iter().find(|s| s.spell == spell)
    }

    fn spell_mut(&mut self, spell: Spell) -> Option<&mut SpellState> {
        self.spells.iter_mut().find(|s| s.spell == spell)
    }

    /// `adjust_blood`: adds blood, at most [`BLOOD_DRAIN_LIMIT`] per victim; the amount gained.
    pub fn adjust_blood(&mut self, victim: Option<Entity>, amount: f32) -> f32 {
        let mut amount = amount;
        if let Some(victim) = victim {
            let i = match self.drained.iter().position(|(v, _)| *v == victim) {
                Some(i) => i,
                None => {
                    self.drained.push((victim, 0.0));
                    self.drained.len() - 1
                }
            };
            let taken = self.drained[i].1;
            if taken >= BLOOD_DRAIN_LIMIT {
                return 0.0;
            }
            amount = amount.min(BLOOD_DRAIN_LIMIT - taken);
            self.drained[i].1 += amount;
        }
        self.bloodtotal += amount;
        self.bloodusable += amount;
        amount
    }

    /// `subtract_usable_blood`.
    pub fn subtract_usable(&mut self, amount: f32) {
        self.bloodusable = (self.bloodusable - amount).clamp(0.0, self.bloodtotal);
    }

    /// `adjust_nullification(base, extra)`.
    pub fn adjust_nullification(&mut self, base: f32, extra: f32) {
        self.nullified = (self.nullified + extra).clamp(base, NULLIFICATION_CAP);
    }

    /// The blood a victim gave so far.
    pub fn drained_from(&self, victim: Entity) -> f32 {
        self.drained
            .iter()
            .find(|(v, _)| *v == victim)
            .map_or(0.0, |(_, b)| *b)
    }

    /// `check_vampire_upgrade` + `add_subclass_ability` + `check_full_power_upgrade`: the powers
    /// to grant now, in order.
    pub fn pending_powers(&self) -> Vec<Power> {
        let mut out = Vec::new();
        let add = |p: Power, out: &mut Vec<Power>| {
            if !self.has(p) && !out.contains(&p) {
                out.push(p);
            }
        };
        for (power, level) in UPGRADE_TIERS {
            if !self.spent.contains(&power) && self.bloodtotal >= level {
                add(power, &mut out);
            }
        }
        if let Some(subclass) = self.subclass {
            for &(power, level) in subclass.standard_powers() {
                if self.bloodtotal >= level {
                    add(power, &mut out);
                }
            }
            if self.drained.len() >= FULLPOWER_DRAINED_REQUIREMENT
                && self.bloodtotal >= FULLPOWER_BLOODTOTAL_REQUIREMENT
            {
                for &power in subclass.full_powers() {
                    add(power, &mut out);
                }
            }
        }
        out
    }

    /// `vampire_ability/calculate_blood_cost`.
    pub fn blood_cost(&self, base: u32) -> u32 {
        (base as f32 * (1.0 + self.nullified / 100.0)).round() as u32
    }

    /// `vampire_ability/can_cast` + the spell's `IsAvailable` (cooldown): the message of the
    /// first failed check.
    pub fn cast_check(&self, spell: Spell, ctx: CastContext) -> Result<(), CastFail> {
        let def = spell.def();
        let state = self.spell(spell).ok_or(CastFail::Missing)?;
        if ctx.now < state.ready_at {
            return Err(CastFail::Cooldown);
        }
        if def.conscious && !ctx.conscious {
            return Err(CastFail::Unconscious);
        }
        if ctx.dead {
            return Err(CastFail::Message("Не в состоянии смерти!".into()));
        }
        let full = self.has(Power::Full);
        if self.nullified >= COMPLETE_NULLIFICATION && !full {
            return Err(CastFail::Message("Что-то блокирует ваши силы!".into()));
        }
        let cost = self.blood_cost(def.cost);
        if self.bloodusable < cost as f32 {
            return Err(CastFail::Message(format!(
                "Для этого требуется как минимум {cost} единиц доступной крови!"
            )));
        }
        if ctx.in_chapel && !full {
            return Err(CastFail::Message(
                "На этой святой земле ваши силы бесполезны.".into(),
            ));
        }
        if spell == Spell::Glare && state.charges == 0 {
            return Err(CastFail::Message("Ваш взгляд ещё не восстановился.".into()));
        }
        Ok(())
    }

    /// The glare's `after_cast`: a charge spent and its recharge queued; out of charges the
    /// cooldown lasts until the next recharge.
    fn spend_glare_charge(&mut self, now: f64) {
        if let Some(state) = self.spell_mut(Spell::Glare) {
            state.charges = state.charges.saturating_sub(1);
            state.recharges.push(now + GLARE_RECHARGE_SECS);
            if state.charges == 0 {
                state.ready_at = state
                    .recharges
                    .iter()
                    .copied()
                    .fold(f64::INFINITY, f64::min);
            }
        }
    }

    /// The glare's `recharge`: each due charge comes back and resets the cooldown. True when
    /// something changed.
    fn recharge_glare(&mut self, now: f64) -> bool {
        let Some(state) = self.spell_mut(Spell::Glare) else {
            return false;
        };
        let before = state.recharges.len();
        state.recharges.retain(|&t| t > now);
        let due = before - state.recharges.len();
        if due == 0 {
            return false;
        }
        state.charges = (state.charges + due as u8).min(GLARE_CHARGES);
        state.ready_at = now;
        true
    }
}

/// What [`Vampire::cast_check`] needs to know about the caster.
#[derive(Clone, Copy, Debug, Default)]
pub struct CastContext {
    pub now: f64,
    pub dead: bool,
    pub conscious: bool,
    pub in_chapel: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastFail {
    Missing,
    Cooldown,
    Unconscious,
    Message(String),
}

/// The vampire's timed actions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VampireJob {
    /// One `bloodsuck_tick` of the bite loop.
    Bite,
    /// The lair ritual on a coffin.
    Lair,
}

/// A coffin made into a vampire's lair (`/obj/structure/closet/crate/coffin/vampire`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VampireCoffin(pub Entity);

pub struct VampirePlugin;

impl Plugin for VampirePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AntagInfoVampireUi>()
            .register_ui::<VampireHudUi>()
            .add_systems(
                Update,
                bite_clicks
                    .after(crate::security::security_clicks)
                    .before(crate::combat::MeleeSystems),
            )
            .add_systems(
                Update,
                (open_info, pick_subclass, trigger_spells, pointed_clicks)
                    .in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (
                    vampire_life,
                    (tick_vampires, cloak_alpha).run_if(any_with_component::<Vampire>),
                )
                    .chain()
                    .before(crate::health::HealthSystems)
                    .before(UiSystems::Open),
            );
        hemomancer::plugin(app);
    }
}

use crate::tools::tell_mob as chat_to;

#[cfg(test)]
mod tests;
