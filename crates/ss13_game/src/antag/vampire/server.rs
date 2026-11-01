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
// no stinger sound, moodlet, silence or flash overlay of the glare, no bodyguard or thralls.

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
    pub const ALL: [Spell; 6] = [
        Spell::Rejuvenate,
        Spell::Glare,
        Spell::Lair,
        Spell::Cloak,
        Spell::BloodRush,
        Spell::SwitchPlaces,
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
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Umbrae => "Специализируется на тьме, скрытности, засадах и мобильности.",
            Self::Hemomancer => "Специализируется на магии крови и управлении окружающей кровью.",
            Self::Gargantua => "Специализируется на стойкости и уроне в ближнем бою.",
            Self::Dantalion => "Специализируется на порабощении и иллюзиях.",
        }
    }

    /// `standard_powers` this server implements, with their blood.
    pub fn standard_powers(self) -> &'static [(Power, f32)] {
        match self {
            Self::Umbrae => &[(Power::Cloak, 150.0)],
            Self::Hemomancer => &[],
            Self::Gargantua => &[(Power::BloodRush, 250.0)],
            Self::Dantalion => &[(Power::SwitchPlaces, 250.0)],
        }
    }

    /// `fully_powered_abilities` this server implements.
    pub fn full_powers(self) -> &'static [Power] {
        match self {
            Self::Umbrae => &[Power::Full, Power::VisionFull, Power::VisionXray],
            _ => &[Power::Full, Power::VisionFull],
        }
    }

    /// `improved_rejuv_healing`.
    pub fn improved_rejuv_healing(self) -> bool {
        self == Self::Gargantua
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
    }
}

fn chat_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

fn rng_of(world: &mut World, seed: u64) -> Rng {
    world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(seed), |r| r.0)
}

/// `add_antag_datum(/datum/antagonist/vampire)` on `mob`. `false` if it already is one.
pub fn add_vampire(world: &mut World, mob: Entity) -> bool {
    if world.get::<Vampire>(mob).is_some() || world.get::<Blood>(mob).is_none() {
        return false;
    }
    let mut rng = rng_of(world, mob.to_bits());
    let objectives = forge_objectives(world, mob, &mut rng);
    world.insert_resource(DynamicRng(rng));
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(ANTAG_HUD.0, ANTAG_HUD.1));
    world.entity_mut(mob).insert((
        Vampire {
            key,
            objectives,
            ..default()
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
        VampireHudData::default(),
        MoveSpeedModifier::default(),
    ));
    // `apply_innate_effects`: the powers of 0 blood, silently.
    check_upgrades(world, mob, false);
    // `greet()`, the panel note and the panel.
    chat_to(world, mob, Span::Bold, format!("Вы - {VAMPIRE_NAME}."));
    chat_to(world, mob, Span::Danger, "Вы — вампир!");
    chat_to(
        world,
        mob,
        Span::Notice,
        "Чтобы укусить кого-то, выберите голову, включите боевой режим и используйте пустую руку. \
         Пейте кровь, чтобы получить новые силы. Вы слабы перед святыми предметами, светом звёзд \
         и огнём. Не выходите в космос и избегайте священника, часовни и особенно святой воды.",
    );
    announce_objectives(world, mob);
    chat_to(
        world,
        mob,
        Span::BoldNotice,
        "Для большей информации - прочтите панель. Вы можете вернуться снова, нажав кнопку \
         сверху слева.",
    );
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<VampireHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<AntagInfoVampireUi>(
            client,
            mob,
            mob,
            VAMPIRE_NAME,
        ));
    }
    refresh(world, mob);
    info!("{mob} is a vampire");
    true
}

/// `announce_objectives`.
fn announce_objectives(world: &mut World, mob: Entity) {
    let Some(lines) = world.get::<Vampire>(mob).map(|v| {
        v.objectives
            .iter()
            .enumerate()
            .map(|(i, o)| format!("{OBJECTIVE_NAME} #{}: {}", i + 1, o.explanation()))
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    chat_to(world, mob, Span::Notice, "Ваши текущие задачи:");
    for line in lines {
        chat_to(world, mob, Span::Plain, line);
    }
}

/// `forge_objectives`.
fn forge_objectives(world: &mut World, owner: Entity, rng: &mut Rng) -> Vec<VampireObjective> {
    // `round(rand(150, 400) / 5) * 5`.
    let target = (rng.range(150, 400) as f64 / 5.0).round() as u32 * 5;
    let mut objectives = vec![VampireObjective::Blood { target }];
    let mut protected = None;
    if rng.prob(5.0) {
        let target = find_target(world, &[owner], &[], assassinate, rng);
        protected = target;
        let text = match target {
            Some(t) => format!("Защитите {}.", target_text(world, t)),
            None => "Свободная задача.".to_owned(),
        };
        objectives.push(VampireObjective::Protect { target, text });
    } else if rng.prob(50.0) {
        objectives.push(VampireObjective::Specialization { text: None });
    } else {
        let targets: Vec<StealItemDef> = world
            .get_resource::<UplinkCatalog>()
            .map(|c| c.data.steal_items.clone())
            .unwrap_or_default();
        let item = find_steal_target(world, &[owner], &[], &targets, rng);
        objectives.push(VampireObjective::Generic(objective(
            world,
            ObjectiveKind::Steal { item },
        )));
    }
    // The protected one is never the kill target.
    let blacklist: Vec<Objective> = protected
        .map(|t| objective(world, assassinate(Some(t))))
        .into_iter()
        .collect();
    let target = find_target(world, &[owner], &blacklist, assassinate, rng);
    objectives.push(VampireObjective::Generic(objective(
        world,
        assassinate(target),
    )));
    objectives.push(VampireObjective::Lair);
    objectives.push(if rng.prob(20.0) {
        VampireObjective::Survive
    } else {
        VampireObjective::Generic(objective(world, ObjectiveKind::Escape))
    });
    objectives
}

fn assassinate(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Assassinate { target }
}

fn objective(world: &World, kind: ObjectiveKind) -> Objective {
    Objective {
        explanation: kind.explanation(world),
        kind,
    }
}

/// `check_vampire_upgrade(announce)`: grants what the blood (and specialization) unlocked.
pub fn check_upgrades(world: &mut World, mob: Entity, announce: bool) {
    let Some(pending) = world.get::<Vampire>(mob).map(Vampire::pending_powers) else {
        return;
    };
    if pending.is_empty() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        for &power in &pending {
            vampire.powers.push(power);
            if let Some(spell) = power.spell() {
                vampire.spells.push(SpellState {
                    spell,
                    ready_at: now,
                    charges: if spell == Spell::Glare {
                        GLARE_CHARGES
                    } else {
                        0
                    },
                    recharges: Vec::new(),
                });
            }
        }
    }
    if announce {
        for power in pending {
            chat_to(world, mob, Span::BoldNotice, power.gain_desc());
        }
    }
    refresh(world, mob);
}

/// The action buttons and the blood counter from the vampire's state (`build_all_button_icons`,
/// `update_blood_hud`). Cost: a handful of icon lookups, on blood changes and each `Life()`.
pub fn refresh(world: &mut World, mob: Entity) {
    let Some(vampire) = world.get::<Vampire>(mob).cloned() else {
        return;
    };
    let ctx = cast_context(world, mob);
    let icon = |(dmi, state): (&str, &str)| {
        world
            .get_resource::<IconIndex>()
            .and_then(|index| index.get(dmi, state))
    };
    let (background, border) = (
        icon((ACTIONS_DMI, "bg_vampire")),
        icon((ACTIONS_DMI, "bg_vampire_border")),
    );
    let buttons: Vec<(ActionButton, f64)> = vampire
        .spells
        .iter()
        .map(|state| {
            let def = state.spell.def();
            let available = match vampire.cast_check(
                state.spell,
                CastContext {
                    now: f64::MAX,
                    ..ctx
                },
            ) {
                Ok(()) => true,
                Err(CastFail::Message(_) | CastFail::Unconscious | CastFail::Missing) => false,
                Err(CastFail::Cooldown) => true,
            };
            let maptext = (state.spell == Spell::Glare && state.charges > 0)
                .then(|| state.charges.to_string());
            (
                ActionButton {
                    id: def.id.to_owned(),
                    name: def.name.to_owned(),
                    icon: icon(def.icon),
                    background,
                    border,
                    available,
                    cooldown: 0.0,
                    active: vampire.aiming == Some(state.spell),
                    maptext,
                },
                state.ready_at,
            )
        })
        .collect();
    set_buttons(world, mob, buttons);
    if let Some(mut hud) = world.get_mut::<VampireHudData>(mob) {
        hud.set_if_neq(VampireHudData {
            usable: vampire.bloodusable,
        });
    }
}

fn area_path(world: &World, pos: TilePos) -> Option<String> {
    let area = world.get_resource::<Areas>()?.area_at(pos)?;
    world
        .get::<ss13_mapload::power::DmPower>(area)
        .map(|p| p.path.clone())
}

fn in_chapel(world: &World, mob: Entity) -> bool {
    world
        .get::<TilePos>(mob)
        .and_then(|&pos| area_path(world, pos))
        .is_some_and(|p| is_under(&p, CHAPEL))
}

fn cast_context(world: &World, mob: Entity) -> CastContext {
    let stat = world.get::<Stat>(mob).copied().unwrap_or(Stat::Dead);
    CastContext {
        now: world.resource::<Time>().elapsed_secs_f64(),
        dead: stat == Stat::Dead,
        conscious: stat == Stat::Conscious,
        in_chapel: in_chapel(world, mob),
    }
}

/// Adds blood from `victim` (`adjust_blood`) and grants what it unlocked. The amount gained.
pub fn gain_blood(world: &mut World, mob: Entity, victim: Option<Entity>, amount: f32) -> f32 {
    let Some(gained) = world
        .get_mut::<Vampire>(mob)
        .map(|mut v| v.adjust_blood(victim, amount))
    else {
        return 0.0;
    };
    check_upgrades(world, mob, true);
    refresh(world, mob);
    gained
}

fn subtract_usable(world: &mut World, mob: Entity, amount: f32) {
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.subtract_usable(amount);
    }
    refresh(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Biting

/// `vampire_biter/try_bite`: empty-handed clicks in combat mode on a human's head, before melee.
fn bite_clicks(world: &mut World, mut clicks: Local<MessageCursor<crate::click::ClickResolved>>) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&mob)
        {
            continue;
        }
        let Some(vampire) = world.get::<Vampire>(mob) else {
            continue;
        };
        if vampire.draining.is_some() || target == mob || click.modifiers.right {
            continue;
        }
        let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
        let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
        if !combat || held.is_some() || zone(world, mob) != BodyZone::Head {
            continue;
        }
        if world.get::<Blood>(target).is_none() || world.get::<Health>(target).is_none() {
            continue;
        }
        world
            .resource_mut::<crate::security::MeleeHandled>()
            .0
            .insert(mob);
        // tg `ClickOn` -> `face_atom` before the unarmed attack.
        if let Some(dir) = dir_to(click.mob_pos, click.target_pos) {
            world.entity_mut(mob).insert(Facing(dir));
        }
        start_bite(world, click.client, mob, target);
    }
}

fn zone(world: &World, mob: Entity) -> BodyZone {
    world
        .get::<ZoneSelected>(mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone()
}

/// `can_bloodsuck`: every condition of starting, holding and draining a bite. Dead victims
/// are valid (only the usable blood needs a living one).
fn can_bloodsuck(world: &mut World, mob: Entity, victim: Entity, feedback: bool) -> bool {
    let (Some(&at), Some(&victim_at)) = (world.get::<TilePos>(mob), world.get::<TilePos>(victim))
    else {
        return false;
    };
    if victim == mob || world.get::<Vampire>(mob).is_none() {
        return false;
    }
    if !world
        .resource::<SpatialIndex>()
        .adjacent(at, victim_at, |e| e == victim || e == mob)
    {
        return false;
    }
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let traits = incapacitation(world, mob);
    let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
    if !combat
        || zone(world, mob) != BodyZone::Head
        || held.is_some()
        || traits.incapacitated
        || !conscious
        // `can_unarmed_attack`: `TRAIT_HANDS_BLOCKED`.
        || traits.hands_blocked
    {
        return false;
    }
    let victim_name = name_of(world, victim);
    // shortcut: anything in the mask slot covers the mouth (tg `is_mouth_covered`).
    if world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.mask.is_some())
    {
        if feedback {
            chat_to(
                world,
                mob,
                Span::Warning,
                format!("Ваша маска или намордник не позволяют укусить {victim_name}!"),
            );
        }
        return false;
    }
    if world.get::<Blood>(victim).is_none_or(|b| b.volume <= 0.0) {
        if feedback {
            chat_to(
                world,
                mob,
                Span::Warning,
                format!("В {victim_name} нет крови!"),
            );
        }
        return false;
    }
    // `TRAIT_VAMPIRE_LIKE`.
    if world.get::<Vampire>(victim).is_some() {
        if feedback {
            chat_to(
                world,
                mob,
                Span::Warning,
                format!("Ваши клыки не могут пронзить холодную плоть {victim_name}!"),
            );
        }
        return false;
    }
    true
}

/// `handle_bloodsucking`: the bite starts and its first `do_after`.
fn start_bite(world: &mut World, client: Entity, mob: Entity, victim: Entity) {
    if !can_bloodsuck(world, mob, victim, true) || world.get::<DoAfter>(mob).is_some() {
        return;
    }
    let (user, target) = (name_of(world, mob), name_of(world, victim));
    visible(
        world,
        mob,
        &format!("Вы вонзаете клыки в {target} и начинаете высасывать его кровь."),
        &format!("{user} грубо хватает {target} за шею и вонзает свои клыки!"),
        Span::Danger,
    );
    info!("{mob} bites {victim} (vampire)");
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.draining = Some(victim);
    }
    next_bite(world, client, mob, victim);
}

fn next_bite(world: &mut World, client: Entity, mob: Entity, victim: Entity) {
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if !do_after(
        world,
        actor,
        victim,
        SUCK_RATE_DS,
        0,
        Job::Vampire(VampireJob::Bite),
    ) {
        stop_bite(world, mob);
    }
}

/// The end of the loop: "Вы прекращаете высасывать кровь из ...".
fn stop_bite(world: &mut World, mob: Entity) {
    let Some(victim) = world
        .get_mut::<Vampire>(mob)
        .and_then(|mut v| v.draining.take())
    else {
        return;
    };
    if world.get_entity(victim).is_ok() {
        let name = name_of(world, victim);
        chat_to(
            world,
            mob,
            Span::Notice,
            format!("Вы прекращаете высасывать кровь из {name}."),
        );
    }
}

/// `can_gain_usable_blood_from`: alive, under the limit, and a player's body (tg: a `ckey` or a
/// ghost; here a crew body).
fn can_gain_from(world: &World, vampire: &Vampire, victim: Entity) -> bool {
    world.get::<Stat>(victim).is_some_and(|s| *s != Stat::Dead)
        && vampire.drained_from(victim) < BLOOD_DRAIN_LIMIT
        && world.get::<PlayerJob>(victim).is_some()
}

/// `bloodsuck_tick`: the blood physically drained (from corpses too).
pub fn bloodsuck_tick(world: &mut World, mob: Entity, victim: Entity, drain: f32) -> f32 {
    if drain <= 0.0 || !can_bloodsuck(world, mob, victim, true) {
        return 0.0;
    }
    let drained = world
        .get_mut::<Blood>(victim)
        .map_or(0.0, |mut b| -b.adjust(-drain, 0.0, BLOOD_VOLUME_MAXIMUM));
    if drained <= 0.0 {
        return 0.0;
    }
    let eligible = world
        .get::<Vampire>(mob)
        .is_some_and(|v| can_gain_from(world, v, victim));
    let mut gain = if eligible {
        drained.min(BITE_GAIN_CAP) * BLOOD_GAINED_MODIFIER
    } else {
        0.0
    };
    if gain > 0.0 {
        gain = gain_blood(world, mob, Some(victim), gain);
        let (total, usable) = world
            .get::<Vampire>(mob)
            .map(|v| (v.bloodtotal, v.bloodusable))
            .unwrap_or_default();
        chat_to(
            world,
            mob,
            Span::BoldNotice,
            format!(
                "Вы накопили {} ед. крови; для использования осталось {}.",
                blood_text(total),
                blood_text(usable)
            ),
        );
    } else {
        let name = name_of(world, victim);
        chat_to(
            world,
            mob,
            Span::BoldNotice,
            format!("Питание кровью {name} утоляет ваш голод, но не даёт доступной крови."),
        );
    }
    // `set_nutrition(min(NUTRITION_LEVEL_WELL_FED, nutrition + (gain || 5)), forced = TRUE)`.
    if let Some(mut hunger) = world.get_mut::<Hunger>(mob) {
        let fed = if gain > 0.0 { gain } else { 5.0 };
        hunger.nutrition = NUTRITION_LEVEL_WELL_FED.min(hunger.nutrition + fed);
    }
    drained
}

/// The end of a bite's `do_after`: a drain, the victim's blood warnings, the next bite.
fn complete_bite(world: &mut World, mob: Entity, action: &DoAfter) {
    let victim = action.target;
    if world.get::<Vampire>(mob).and_then(|v| v.draining) != Some(victim) {
        return;
    }
    let before = world.get::<Blood>(victim).map_or(0.0, |b| b.volume);
    if bloodsuck_tick(world, mob, victim, DRAIN_AMOUNT) <= 0.0 {
        stop_bite(world, mob);
        return;
    }
    let volume = world.get::<Blood>(victim).map_or(0.0, |b| b.volume);
    if volume <= 0.0 {
        chat_to(world, mob, Span::Warning, "Вы обескровили свою жертву!");
        stop_bite(world, mob);
        return;
    }
    if volume <= BLOOD_VOLUME_BAD && before > BLOOD_VOLUME_BAD {
        chat_to(
            world,
            mob,
            Span::Danger,
            "Объём крови вашей жертвы опасно низок.",
        );
    } else if volume <= BLOOD_VOLUME_OKAY && before > BLOOD_VOLUME_OKAY {
        chat_to(
            world,
            mob,
            Span::Warning,
            "У вашей жертвы слишком мало крови!",
        );
    }
    next_bite(world, action.client, mob, victim);
}

/// [`Job::Vampire`] completions.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: VampireJob) {
    match job {
        VampireJob::Bite => complete_bite(world, mob, action),
        VampireJob::Lair => complete_lair(world, mob, action.target),
    }
}

// ---------------------------------------------------------------------------------------------
// Spells

/// Button clicks: tg `Trigger` of the vampire's spells.
fn trigger_spells(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        let Some(spell) = Spell::from_id(&id) else {
            continue;
        };
        // The client must still control the vampire (not its ghost through a stale window).
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world
                .get::<Vampire>(actor)
                .and_then(|v| v.spell(spell))
                .is_none()
        {
            continue;
        }
        if spell.def().range.is_some() {
            toggle_aim(world, client, actor, spell);
        } else {
            cast(world, client, actor, spell, None);
        }
    }
}

/// A pointed spell's button: aiming on (after the cast checks) or off.
fn toggle_aim(world: &mut World, client: Entity, mob: Entity, spell: Spell) {
    let aiming = world.get::<Vampire>(mob).and_then(|v| v.aiming);
    if aiming == Some(spell) {
        set_aim(world, mob, None);
        return;
    }
    if !checked(world, client, mob, spell) {
        return;
    }
    set_aim(world, mob, Some(spell));
}

fn set_aim(world: &mut World, mob: Entity, spell: Option<Spell>) {
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.aiming = spell;
    }
    if spell.is_some() {
        world.entity_mut(mob).insert(ClickIntercept);
    } else {
        world.entity_mut(mob).remove::<ClickIntercept>();
    }
    refresh(world, mob);
}

/// The cast checks with their feedback.
fn checked(world: &mut World, client: Entity, mob: Entity, spell: Spell) -> bool {
    let ctx = cast_context(world, mob);
    let Some(result) = world.get::<Vampire>(mob).map(|v| v.cast_check(spell, ctx)) else {
        return false;
    };
    match result {
        Ok(()) => true,
        Err(CastFail::Message(text)) => {
            tell(world, client, Span::Warning, text);
            false
        }
        Err(_) => false,
    }
}

/// Clicks of an aiming vampire: tg pointed `InterceptClickOn` (`cast_range`, `is_valid_target`);
/// a right click stops aiming.
fn pointed_clicks(world: &mut World, mut clicks: Local<MessageCursor<InterceptedClick>>) {
    let clicks: Vec<InterceptedClick> = clicks
        .read(world.resource::<Messages<InterceptedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(spell) = world.get::<Vampire>(click.mob).and_then(|v| v.aiming) else {
            // Not a vampire's intercept.
            continue;
        };
        if click.right {
            set_aim(world, click.mob, None);
            continue;
        }
        let range = spell.def().range.unwrap_or(0);
        if click
            .mob_pos
            .chebyshev(click.target_pos)
            .is_none_or(|d| d > range)
        {
            tell(world, click.client, Span::Warning, "Слишком далеко!");
            continue;
        }
        if !valid_target(world, click.mob, spell, click.target) {
            continue;
        }
        if cast(world, click.client, click.mob, spell, Some(click.target)) {
            set_aim(world, click.mob, None);
        }
    }
}

/// `is_valid_target`.
fn valid_target(world: &World, mob: Entity, spell: Spell, target: Entity) -> bool {
    match spell {
        Spell::Lair => path_of(world, target).is_some_and(|p| is_under(&p, COFFIN)),
        Spell::SwitchPlaces => target != mob && world.get::<Health>(target).is_some(),
        _ => true,
    }
}

/// tg spell `Trigger` -> `Activate`: the checks, `before_cast`, the blood (`COMSIG_SPELL_CAST`),
/// `cast`, the cooldown and `after_cast`. True when it was cast.
fn cast(
    world: &mut World,
    client: Entity,
    mob: Entity,
    spell: Spell,
    target: Option<Entity>,
) -> bool {
    if !checked(world, client, mob, spell) || !before_cast(world, client, mob, spell, target) {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let def = spell.def();
    let cost = world
        .get::<Vampire>(mob)
        .map_or(0, |v| v.blood_cost(def.cost));
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        if def.cost > 0 {
            vampire.subtract_usable(cost as f32);
        }
        if let Some(state) = vampire.spell_mut(spell) {
            state.ready_at = now + def.cooldown;
        }
        if spell == Spell::Glare {
            vampire.spend_glare_charge(now);
        }
    }
    info!("{mob} casts {spell:?} (vampire)");
    match spell {
        Spell::Rejuvenate => rejuvenate(world, mob, now),
        Spell::Glare => glare(world, mob),
        Spell::Lair => {
            if let Some(coffin) = target {
                begin_lair(world, client, mob, coffin);
            }
        }
        Spell::Cloak => toggle_cloak(world, mob),
        Spell::BloodRush => blood_rush(world, mob, now),
        Spell::SwitchPlaces => {
            if let Some(target) = target {
                switch_places(world, mob, target);
            }
        }
    }
    // `vampire_ability/after_cast`.
    if def.cost > 0 {
        let usable = world.get::<Vampire>(mob).map_or(0.0, |v| v.bloodusable);
        chat_to(
            world,
            mob,
            Span::BoldNotice,
            format!(
                "У вас осталось {} единиц доступной крови.",
                blood_text(usable)
            ),
        );
    }
    refresh(world, mob);
    true
}

/// The spells' own `can_cast_spell`/`before_cast` checks; false cancels without cost.
fn before_cast(
    world: &mut World,
    client: Entity,
    mob: Entity,
    spell: Spell,
    target: Option<Entity>,
) -> bool {
    match spell {
        Spell::BloodRush => {
            let knocked_down = world
                .get::<crate::status_effects::StatusEffects>(mob)
                .is_some_and(|e| e.knockdown > 0.0);
            if knocked_down || world.get::<crate::medical::surgery::Buckled>(mob).is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Нельзя использовать это в беспомощном состоянии!",
                );
                return false;
            }
            true
        }
        Spell::Lair => {
            let Some(coffin) = target else {
                return false;
            };
            let name = name_of(world, coffin);
            if world.get::<VampireCoffin>(coffin).is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("{name} служит другому и отказывается подчиняться вашей воле!"),
                );
                return false;
            }
            let Some(&at) = world.get::<TilePos>(coffin) else {
                return false;
            };
            let dense = (-1..=1)
                .flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)))
                .any(|(dx, dy)| {
                    let pos = TilePos::new(at.x + dx, at.y + dy, at.z);
                    turf_at(world, pos)
                        .is_some_and(|t| world.get::<ss13_world::Density>(t).is_some())
                });
            if dense {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("Для ритуала вокруг {name} нужно больше места!"),
                );
                return false;
            }
            true
        }
        _ => true,
    }
}

/// `vampire_rejuvenate/cast`: every stun off, stamina reset, standing up; with the regeneration
/// passive 14 s of healing.
fn rejuvenate(world: &mut World, mob: Entity, now: f64) {
    if let Some(mut effects) = effects_mut(world, mob) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            effects.set(kind, 0.0);
        }
        // `set_stamina_loss(0)`.
        effects.stamina = 0.0;
        effects.stamcrit = false;
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        "Вы наполняете тело чистой кровью и снимаете все обездвиживающие эффекты.",
    );
    let Some(vampire) = world.get::<Vampire>(mob) else {
        return;
    };
    // `get_rejuv_mult`.
    if !vampire.has(Power::Regen) {
        return;
    }
    let health = world.get::<Health>(mob).map_or(100.0, Health::health);
    let mult = if vampire
        .subclass
        .is_some_and(Subclass::improved_rejuv_healing)
    {
        ((100.0 - health) / 20.0).clamp(1.0, 5.0)
    } else {
        1.0
    };
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.rejuvenation = Some((now + REJUVENATION_SECS, mult));
    }
}

/// `get_dir` of BYOND: the signs of the offsets.
fn dir_to(from: TilePos, to: TilePos) -> Option<Dir> {
    let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
    Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy))
}

/// The glare's `calculate_deviation`: 0 none (the victim is in front), 1 partial (beside),
/// 2 full (behind).
pub fn glare_deviation(attacker: TilePos, facing: Dir, victim: TilePos) -> u8 {
    let Some(to_victim) = dir_to(attacker, victim) else {
        // The same tile: looking down is easy.
        return 0;
    };
    let (f, v) = (facing as u8, to_victim as u8);
    if f & v != 0 {
        0
    } else if f & to_victim.reverse() as u8 != 0 {
        2
    } else {
        1
    }
}

/// tg `affects_vampire`: other vampires never; holy people (the chaplain's `TRAIT_HOLY`) only to
/// a vampire at full power.
fn affects_vampire(world: &World, target: Entity, vampire: Entity) -> bool {
    if world.get::<Vampire>(target).is_some() {
        return false;
    }
    let full = world
        .get::<Vampire>(vampire)
        .is_some_and(|v| v.has(Power::Full));
    let holy = world
        .get::<PlayerJob>(target)
        .and_then(|j| world.get_resource::<Jobs>()?.data.jobs.get(j.0))
        .is_some_and(|j| j.title == "Chaplain");
    full || !holy
}

/// `vampire_glare/cast` and `cast_on_thing_in_aoe` over the mobs around (radius 1).
fn glare(world: &mut World, mob: Entity) {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let blindfolded = world
        .get::<Inventory>(mob)
        .and_then(|i| i.eyes)
        .and_then(|e| world.get::<DmItem>(e))
        .is_some_and(|dm| is_under(&dm.path, BLINDFOLD));
    if blindfolded {
        chat_to(world, mob, Span::Warning, "На ваших глазах повязка!");
        return;
    }
    let user = name_of(world, mob);
    let text = format!("Глаза {user} испускают ослепительную вспышку!");
    visible(world, mob, &text, &text, Span::Warning);
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let facing = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    let targets: Vec<(Entity, TilePos)> = world
        .resource::<SpatialIndex>()
        .in_radius(at, 1)
        .filter(|&e| e != mob)
        // `orange(1, user)` holds turf contents, not mobs inside a closet there.
        .filter(|&e| {
            world.get::<Health>(e).is_some() && world.get::<crate::closets::InCloset>(e).is_none()
        })
        .filter_map(|e| Some((e, *world.get::<TilePos>(e)?)))
        .collect();
    for (target, pos) in targets {
        if !affects_vampire(world, target, mob) {
            continue;
        }
        let deviation = if lying {
            1
        } else {
            glare_deviation(at, facing, pos)
        };
        let (confusion, stamina, knockdown) = match deviation {
            2 => (6.0, 20.0, 0.0),
            1 => (6.0, 40.0, 5.0),
            _ => (10.0, 70.0, 12.0),
        };
        if let Some(mut effects) = effects_mut(world, target) {
            // `adjust_confusion`, `AdjustKnockdown`.
            effects.confusion += confusion;
            if knockdown > 0.0 {
                effects.adjust(Incap::Knockdown, knockdown);
            }
        }
        crate::status_effects::adjust_stamina(world, target, stamina);
        chat_to(
            world,
            target,
            Span::Warning,
            format!("Вас ослепляет взгляд {user}."),
        );
        info!("{mob} glared at {target} (vampire)");
    }
}

/// The lair ritual's start (`vampire_lair/cast`).
fn begin_lair(world: &mut World, client: Entity, mob: Entity, coffin: Entity) {
    let name = name_of(world, coffin);
    chat_to(
        world,
        mob,
        Span::Danger,
        format!("Вы начинаете помечать {name}!"),
    );
    do_after(
        world,
        Actor {
            client,
            mob,
            item: None,
        },
        coffin,
        LAIR_RITUAL_DS,
        0,
        Job::Vampire(VampireJob::Lair),
    );
}

/// The ritual done: the coffin is the vampire's (anchored), the lair objective is met and the
/// spell goes away (`COMSIG_VAMPIRE_ABILITY_COMPLETE_LAIR`).
fn complete_lair(world: &mut World, mob: Entity, coffin: Entity) {
    if world.get::<VampireCoffin>(coffin).is_some()
        || !path_of(world, coffin).is_some_and(|p| is_under(&p, COFFIN))
    {
        return;
    }
    world
        .entity_mut(coffin)
        .insert((VampireCoffin(mob), Anchored(true)));
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.has_lair = true;
        // `remove_spell_ability`: the tier is removed, so it is never granted again.
        vampire.powers.retain(|p| *p != Power::Lair);
        vampire.spells.retain(|s| s.spell != Spell::Lair);
        vampire.spent.push(Power::Lair);
        vampire.aiming = None;
    }
    world.entity_mut(mob).remove::<ClickIntercept>();
    info!("{mob} made {coffin} its lair (vampire)");
    refresh(world, mob);
}

/// `vampire_cloak/cast`.
fn toggle_cloak(world: &mut World, mob: Entity) {
    let Some(on) = world.get_mut::<Vampire>(mob).map(|mut v| {
        v.cloaking = !v.cloaking;
        v.cloaking
    }) else {
        return;
    };
    chat_to(
        world,
        mob,
        Span::Notice,
        format!(
            "Теперь во тьме вас будут {}.",
            if on {
                "не замечать"
            } else {
                "видеть"
            }
        ),
    );
    handle_cloak(world, mob);
}

/// `vampire_blood_rush/cast`: faster for 10 s, knockdown off, standing up.
fn blood_rush(world: &mut World, mob: Entity, now: f64) {
    chat_to(world, mob, Span::Notice, "Вы чувствуете прилив энергии!");
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.rush_until = now + BLOOD_RUSH_SECS;
    }
    if let Some(mut effects) = effects_mut(world, mob) {
        effects.set(Incap::Knockdown, 0.0);
    }
    update_speed(world, mob, now);
}

/// `vampire_switch_places/cast`: the two swap tiles.
fn switch_places(world: &mut World, mob: Entity, target: Entity) {
    let (Some(&a), Some(&b)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return;
    };
    world.entity_mut(target).insert(a);
    world.entity_mut(mob).insert(b);
}

// ---------------------------------------------------------------------------------------------
// Life

/// The cloak and the blood rush as one movespeed modifier.
fn update_speed(world: &mut World, mob: Entity, now: f64) {
    let Some(vampire) = world.get::<Vampire>(mob) else {
        return;
    };
    let mut speed = 0.0;
    if vampire.cloaking && vampire.cloak_fast {
        speed += CLOAK_SPEED;
    }
    if now < vampire.rush_until {
        speed += BLOOD_RUSH_SPEED;
    }
    if let Some(mut modifier) = world.get_mut::<MoveSpeedModifier>(mob) {
        modifier.set_if_neq(MoveSpeedModifier(speed));
    }
}

/// `handle_vampire_cloak`: alpha 40 and faster at `lumcount * 10 <= 2`, else alpha 200; off
/// while burning or not cloaking.
fn handle_cloak(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let burning = crate::fire::on_fire(world, mob);
    let cloaking = world.get::<Vampire>(mob).is_some_and(|v| v.cloaking);
    let (alpha, fast) = if !cloaking || burning {
        (None, false)
    } else {
        // A tile without lighting is never dark (tg `get_lumcount` of fullbright areas is 1).
        let light = crate::botany::lumcount(world, pos).unwrap_or(1.0) * 10.0;
        if light <= 2.0 {
            (Some(40), true)
        } else {
            (Some(200), false)
        }
    };
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.cloak_alpha = alpha;
        vampire.cloak_fast = fast;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    update_speed(world, mob, now);
}

/// The cloak's alpha on the mob, again whenever its appearance was rebuilt (clothes).
fn cloak_alpha(
    mut mobs: Query<(&Vampire, &mut Appearance), Or<(Changed<Vampire>, Changed<Appearance>)>>,
) {
    for (vampire, mut appearance) in &mut mobs {
        let color = vampire.cloak_alpha.map(|a| [255, 255, 255, a]);
        if appearance.color != color {
            appearance.color = color;
        }
    }
}

/// Per tick: the glare's recharges, the end of the blood rush, a bite whose `do_after` broke.
// Cost: one pass over the vampires (a few per round).
fn tick_vampires(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Vampire>>()
        .iter(world)
        .collect();
    for mob in mobs {
        let recharged = world
            .get_mut::<Vampire>(mob)
            .is_some_and(|mut v| v.recharge_glare(now));
        if recharged {
            refresh(world, mob);
        }
        let rush_over = world
            .get::<Vampire>(mob)
            .is_some_and(|v| v.rush_until > 0.0 && now >= v.rush_until);
        if rush_over {
            if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
                vampire.rush_until = 0.0;
            }
            update_speed(world, mob, now);
        }
        let biting = world
            .get::<DoAfter>(mob)
            .is_some_and(|d| d.job == Job::Vampire(VampireJob::Bite));
        if !biting
            && world
                .get::<Vampire>(mob)
                .is_some_and(|v| v.draining.is_some())
        {
            stop_bite(world, mob);
        }
    }
}

/// tg `COMSIG_LIVING_LIFE` -> `handle_vampire` each `SSmobs` fire, with the holy water reagent
/// tick, the fire handler's blood cost and the rejuvenation's healing.
// Cost: every 2 s per vampire a lumcount (radius 8 sources) while cloaked and a 20-step sun ray
// on space tiles.
fn vampire_life(world: &mut World, mut last_fired: Local<Option<u64>>) {
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    // No backlog of the fires before the first run.
    let fires = fired.saturating_sub(last_fired.unwrap_or(fired));
    *last_fired = Some(fired);
    if fires == 0 {
        return;
    }
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Vampire>>()
        .iter(world)
        .collect();
    for _ in 0..fires {
        for &mob in &mobs {
            live(world, mob, secs, now);
        }
    }
}

fn live(world: &mut World, mob: Entity, secs: f32, now: f64) {
    let (Some(&pos), Some(&stat)) = (world.get::<TilePos>(mob), world.get::<Stat>(mob)) else {
        return;
    };
    let Some(vampire) = world.get::<Vampire>(mob).cloned() else {
        return;
    };
    let full = vampire.has(Power::Full);
    handle_cloak(world, mob);
    if stat != Stat::Dead {
        let space = turf_at(world, pos)
            .and_then(|t| path_of(world, t))
            .is_some_and(|p| is_under(&p, SPACE_TURF));
        if space {
            check_sun(world, mob, pos);
        }
        if in_chapel(world, mob) && !full && vampire.bloodtotal > 0.0 {
            vamp_burn(world, mob, 7.0);
        }
        holy_water(world, mob, full);
        // `on_fire_stacks_updated`: burning costs blood while it can harm.
        // shortcut: every `Life()` while the mob has fire stacks, not on each change of them;
        // no thermal protection check.
        let stacks = world
            .get::<crate::fire::FireStacks>(mob)
            .is_some_and(|f| f.stacks > 0.0);
        if stacks && !full {
            subtract_usable(world, mob, 5.0);
        }
        rejuvenation_tick(world, mob, secs, now);
    }
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.nullified = (vampire.nullified - 2.0).max(0.0);
    }
    refresh(world, mob);
}

/// `/datum/status_effect/vampire_rejuvenation/tick`: the healing of 14 s spread over its ticks.
fn rejuvenation_tick(world: &mut World, mob: Entity, secs: f32, now: f64) {
    let Some((ends_at, mult)) = world.get::<Vampire>(mob).and_then(|v| v.rejuvenation) else {
        return;
    };
    if now >= ends_at + f64::from(secs) {
        if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
            vampire.rejuvenation = None;
        }
        return;
    }
    let healing = mult * secs / REJUVENATION_SECS as f32;
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.adjust(DamageType::Brute, -10.0 * healing);
        health.adjust(DamageType::Oxy, -25.0 * healing);
        health.adjust(DamageType::Tox, -10.0 * healing);
        health.adjust(DamageType::Burn, -10.0 * healing);
    }
    if let (Some(chem), Some(mut reagents)) = (
        world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone()),
        world.get::<crate::chem::MobReagents>(mob).cloned(),
    ) {
        let toxins: Vec<_> = reagents
            .blood
            .reagents
            .iter()
            .map(|r| r.id)
            .filter(|&id| chem.is_type(id, "/datum/reagent/toxin"))
            .collect();
        if !toxins.is_empty() {
            for id in toxins {
                reagents.blood.remove_reagent(id, 10.0 * healing);
            }
            world.entity_mut(mob).insert(reagents);
        }
    }
    if now >= ends_at
        && let Some(mut vampire) = world.get_mut::<Vampire>(mob)
    {
        vampire.rejuvenation = None;
    }
}

/// `check_sun`: starlight drains blood and burns, or turns a bloodless vampire to ash.
fn check_sun(world: &mut World, mob: Entity, pos: TilePos) {
    let Some(sun) = world.get_resource::<crate::power::solar::Sun>().cloned() else {
        return;
    };
    let index = world.resource::<SpatialIndex>();
    let size = index.level_size(pos.z).unwrap_or((0, 0));
    let blocked = crate::power::solar::is_sunlight_blocked(pos, &sun, size, |p| {
        index
            .on_tile(p)
            .iter()
            .any(|&e| world.get::<Opaque>(e).is_some())
    });
    if blocked {
        return;
    }
    let usable = world.get::<Vampire>(mob).map_or(0.0, |v| v.bloodusable);
    if usable >= 10.0 {
        chat_to(
            world,
            mob,
            Span::Danger,
            "Свет звёзд высасывает ваши силы — покиньте его!",
        );
        subtract_usable(world, mob, 10.0);
        vamp_burn(world, mob, 10.0);
    } else {
        chat_to(
            world,
            mob,
            Span::Danger,
            "Ваше тело превращается в пепел — НЕМЕДЛЕННО уйдите от света звёзд!",
        );
        vamp_burn(world, mob, 85.0);
        // shortcut: `dust()` is death here (no dusting of bodies yet).
        let dead = world
            .get::<Health>(mob)
            .is_some_and(|h| h.health() <= crate::health::HEALTH_THRESHOLD_DEAD);
        if dead && let Some(mut stat) = world.get_mut::<Stat>(mob) {
            *stat = Stat::Dead;
        }
    }
}

/// `vamp_burn(burn_chance)`.
fn vamp_burn(world: &mut World, mob: Entity, chance: f32) {
    let Some(health) = world.get::<Health>(mob).map(Health::health) else {
        return;
    };
    let roll = world.resource_mut::<CombatRng>().chance(chance);
    if roll && health >= 50.0 {
        if health >= 75.0 {
            chat_to(world, mob, Span::Warning, "Ваша кожа отслаивается...");
        } else {
            chat_to(world, mob, Span::Warning, "Ваша кожа шипит!");
        }
        damage(world, mob, DamageType::Burn, 3.0);
    } else if health < 50.0 {
        if crate::fire::on_fire(world, mob) {
            chat_to(world, mob, Span::Danger, "Вы продолжаете гореть!");
        } else {
            chat_to(world, mob, Span::Danger, "Ваша кожа загорается!");
        }
        crate::fire::adjust_fire_stacks(world, mob, 5.0);
        crate::fire::ignite_mob(world, mob);
    }
}

fn damage(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let changed = apply_damage(&mut health, &mut stat, kind, amount);
    world.entity_mut(mob).insert(health);
    if changed.is_some() {
        world.entity_mut(mob).insert(stat);
    }
}

/// `vampire_holywater/on_reagent_tick` while holy water is in the blood (80% of its ticks).
fn holy_water(world: &mut World, mob: Entity, full: bool) {
    let chem = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone());
    let id = chem.as_ref().and_then(|c| c.id(HOLY_WATER));
    let present = id.is_some_and(|id| {
        world
            .get::<crate::chem::MobReagents>(mob)
            .is_some_and(|r| r.blood.amount(id) > 0.0)
    });
    let Some(mut vampire) = world.get::<Vampire>(mob).cloned() else {
        return;
    };
    if !present {
        if vampire.holy_cycles != 0
            && let Some(mut v) = world.get_mut::<Vampire>(mob)
        {
            v.holy_cycles = 0;
        }
        return;
    }
    vampire.holy_cycles += 1;
    let cycle = vampire.holy_cycles;
    if let Some(mut v) = world.get_mut::<Vampire>(mob) {
        v.holy_cycles = cycle;
    }
    let roll = world.resource_mut::<CombatRng>().chance(80.0);
    if full || !roll || vampire.bloodtotal <= 0.0 {
        return;
    }
    if vampire.bloodusable > 0.0 {
        crate::status_effects::adjust_stamina(world, mob, 5.0);
        if let Some(mut v) = world.get_mut::<Vampire>(mob) {
            v.adjust_nullification(20.0, 4.0);
            v.subtract_usable(3.0);
        }
        let empty = world
            .get::<Vampire>(mob)
            .is_some_and(|v| v.bloodusable <= 0.0);
        if empty {
            // The holy water purges itself.
            if let (Some(id), Some(mut reagents)) =
                (id, world.get::<crate::chem::MobReagents>(mob).cloned())
            {
                let volume = reagents.blood.amount(id);
                reagents.blood.remove_reagent(id, volume);
                world.entity_mut(mob).insert(reagents);
            }
            return;
        }
        damage(world, mob, DamageType::Brute, 3.0);
        return;
    }
    if let Some(mut v) = world.get_mut::<Vampire>(mob) {
        v.adjust_nullification(20.0, 4.0);
    }
    match cycle {
        1..=4 => chat_to(world, mob, Span::Warning, "Что-то шипит в ваших венах!"),
        5..=12 => {
            chat_to(
                world,
                mob,
                Span::Danger,
                "Вы чувствуете сильное жжение внутри!",
            );
            damage(world, mob, DamageType::Burn, 1.0);
        }
        _ => {
            let name = name_of(world, mob);
            visible(
                world,
                mob,
                "Вас внезапно охватывает священное пламя!",
                &format!("{name} внезапно вспыхивает!"),
                Span::Danger,
            );
            let stacks = world
                .get::<crate::fire::FireStacks>(mob)
                .map_or(0.0, |f| f.stacks);
            let add = (stacks + 3.0).min(5.0) - stacks;
            crate::fire::adjust_fire_stacks(world, mob, add.max(0.0));
            crate::fire::ignite_mob(world, mob);
            damage(world, mob, DamageType::Burn, 3.0);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Windows

impl ServerUi for AntagInfoVampireUi {
    type Source = &'static Vampire;
    type UserParam = ();
    type Act = Subclass;

    fn static_data(_: &Vampire) -> VampireInfoStatic {
        VampireInfoStatic {
            subclasses: Subclass::ALL.into_iter().map(Subclass::ui).collect(),
        }
    }

    fn data(vampire: &Vampire, _: Entity, _: &()) -> VampireInfoData {
        VampireInfoData {
            total_blood: vampire.bloodtotal,
            specialization_blood_required: 150,
            selected_subclass: vampire.subclass.map(|s| s.id().to_owned()),
            can_select_subclass: vampire.has(Power::UnlockSpecialization),
            objectives: vampire
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation(),
                    complete: false,
                })
                .collect(),
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<Subclass, UiReject> {
        Subclass::from_id(action).ok_or(UiReject::UnknownAction)
    }
}

impl ServerUi for VampireHudUi {
    type Source = &'static VampireHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &VampireHudData) {}

    fn data(source: &VampireHudData, _: Entity, _: &()) -> VampireHudData {
        *source
    }

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// `/datum/action/antag_info/Trigger` of a vampire.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Vampire>(source).is_some() {
            world.write_message(OpenUi::new::<AntagInfoVampireUi>(
                client,
                actor,
                source,
                VAMPIRE_NAME,
            ));
        }
    }
}

/// `ui_act` of the info panel: the specialization, once, after 150 blood.
fn pick_subclass(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagInfoVampireUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, Subclass)> = acts
        .read(world.resource::<Messages<UiActed<AntagInfoVampireUi>>>())
        .map(|a| (a.window, a.actor, a.source, a.act))
        .collect();
    for (window, actor, source, subclass) in pending {
        if actor != source {
            continue;
        }
        let allowed = world
            .get::<Vampire>(source)
            .is_some_and(|v| v.subclass.is_none() && v.has(Power::UnlockSpecialization));
        if !allowed {
            continue;
        }
        add_subclass(world, source, subclass);
        // `ui.close()`.
        if let Some(mut window) = world.get_mut::<ss13_tgui::server::UiWindow>(window) {
            window.cap_status(ss13_shared::ui::UiStatus::Close);
        }
    }
}

/// `add_subclass` after the pick: the unlock power is spent, the specialization's powers
/// granted and its objective written.
pub fn add_subclass(world: &mut World, mob: Entity, subclass: Subclass) {
    let mut rng = rng_of(world, mob.to_bits());
    let departments = [
        "службу безопасности",
        "отдел обслуживания",
        "научный отдел",
        "медицинский отдел",
        "инженерный отдел",
        "отдел снабжения",
    ];
    let mut pick = |n: usize| rng.range(0, n as i64 - 1) as usize;
    let objectives = subclass.unique_objectives();
    let text = objectives[pick(objectives.len())]
        .replace("%DEPARTMENT", departments[pick(departments.len())]);
    world.insert_resource(DynamicRng(rng));
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.subclass = Some(subclass);
        vampire.spent.push(Power::UnlockSpecialization);
        vampire.powers.retain(|p| *p != Power::UnlockSpecialization);
        for objective in &mut vampire.objectives {
            if let VampireObjective::Specialization { text: t @ None } = objective {
                *t = Some(text.clone());
            }
        }
    }
    info!("{mob} chose the {subclass:?} specialization (vampire)");
    check_upgrades(world, mob, true);
    announce_objectives(world, mob);
    refresh(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Round end

/// tg `antag_report` for vampires: the header, then each vampire's `roundend_report` with the
/// blood it drank.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Vampire)>();
    let mut vampires: Vec<(Entity, Vampire)> =
        query.iter(world).map(|(e, v)| (e, v.clone())).collect();
    if vampires.is_empty() {
        return Vec::new();
    }
    vampires.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, vampire) in vampires {
        out.push(crate::antag::traitor::print_player(
            world,
            mob,
            &vampire.key,
        ));
        // shortcut: the fork's report has no blood line; its admin panel's
        // (`antag_panel_data`) is shown here.
        let class = vampire.subclass.map_or("N/A", Subclass::name);
        out.push(line(vec![ChatSegment::new(
            Span::Plain,
            format!(
                "Класс: {class} | Всего крови: {} | Жертв: {}",
                blood_text(vampire.bloodtotal),
                vampire.drained.len()
            ),
        )]));
        let mut won = true;
        for (i, objective) in vampire.objectives.iter().enumerate() {
            let done = objective.check_completion(world, mob, &vampire, end);
            won &= done;
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", objective.explanation())),
                if done {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Неудача.")
                },
            ]));
        }
        out.push(line(vec![if won {
            ChatSegment::new(Span::GreenText, format!("{VAMPIRE_NAME} был успешен!"))
        } else {
            ChatSegment::new(Span::RedText, format!("{VAMPIRE_NAME} провалился!"))
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blood_accounting_respects_the_victim_limit() {
        let mut v = Vampire::default();
        let (a, b) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
        );
        // A full bite: min(20, 25) * 0.5 = 10.
        assert_eq!(v.adjust_blood(Some(a), 10.0), 10.0);
        assert_eq!((v.bloodtotal, v.bloodusable), (10.0, 10.0));
        // 200 per victim: the rest of the limit, then nothing.
        assert_eq!(v.adjust_blood(Some(a), 500.0), 190.0);
        assert_eq!(v.adjust_blood(Some(a), 10.0), 0.0);
        assert_eq!(v.drained_from(a), 200.0);
        assert_eq!(v.adjust_blood(Some(b), 10.0), 10.0);
        assert_eq!(v.drained.len(), 2);
        // Spending never goes below 0 or above the total.
        v.subtract_usable(205.0);
        assert_eq!((v.bloodtotal, v.bloodusable), (210.0, 5.0));
        v.subtract_usable(10.0);
        assert_eq!(v.bloodusable, 0.0);
        v.subtract_usable(-1000.0);
        assert_eq!(v.bloodusable, 210.0);
        // Without a victim (admin edits) there is no limit.
        assert_eq!(v.adjust_blood(None, 1000.0), 1000.0);
    }

    #[test]
    fn thresholds_unlock_the_fork_powers() {
        let mut v = Vampire::default();
        assert_eq!(v.pending_powers(), [Power::Rejuvenate, Power::Glare]);
        v.powers = vec![Power::Rejuvenate, Power::Glare];
        v.bloodtotal = 99.0;
        assert!(v.pending_powers().is_empty());
        v.bloodtotal = 150.0;
        assert_eq!(
            v.pending_powers(),
            [Power::Vision, Power::UnlockSpecialization, Power::Lair]
        );
        v.powers.extend(v.pending_powers());
        // A spent tier never comes back.
        v.powers.retain(|p| *p != Power::Lair);
        v.spent.push(Power::Lair);
        v.bloodtotal = 500.0;
        assert_eq!(v.pending_powers(), [Power::Regen, Power::VisionAdvanced]);
        v.powers.extend(v.pending_powers());
        // The specialization's powers by blood; full power needs 8 victims and 1000 blood.
        v.subclass = Some(Subclass::Gargantua);
        assert_eq!(v.pending_powers(), [Power::BloodRush]);
        v.powers.push(Power::BloodRush);
        v.bloodtotal = 1000.0;
        v.drained = (1..=7)
            .map(|i| (Entity::from_raw_u32(i).unwrap(), 200.0))
            .collect();
        assert!(v.pending_powers().is_empty());
        v.drained.push((Entity::from_raw_u32(8).unwrap(), 10.0));
        assert_eq!(v.pending_powers(), [Power::Full, Power::VisionFull]);
    }

    fn with_spell(spell: Spell, usable: f32) -> Vampire {
        Vampire {
            bloodtotal: usable,
            bloodusable: usable,
            spells: vec![SpellState {
                spell,
                ready_at: 0.0,
                charges: GLARE_CHARGES,
                recharges: Vec::new(),
            }],
            ..default()
        }
    }

    #[test]
    fn spell_cost_and_cooldown_rules() {
        let ctx = CastContext {
            now: 1.0,
            conscious: true,
            ..default()
        };
        let mut v = with_spell(Spell::BloodRush, 30.0);
        assert_eq!(v.cast_check(Spell::BloodRush, ctx), Ok(()));
        assert_eq!(v.cast_check(Spell::Cloak, ctx), Err(CastFail::Missing));
        // Nullification raises the cost: 30 * 1.5 = 45.
        v.adjust_nullification(50.0, 4.0);
        assert_eq!(v.nullified, 50.0);
        assert_eq!(v.blood_cost(30), 45);
        assert!(matches!(
            v.cast_check(Spell::BloodRush, ctx),
            Err(CastFail::Message(m)) if m.contains("45")
        ));
        // At 100 nullification nothing works, until full power.
        v.bloodusable = 100.0;
        v.bloodtotal = 100.0;
        v.adjust_nullification(100.0, 0.0);
        assert!(v.cast_check(Spell::BloodRush, ctx).is_err());
        v.powers.push(Power::Full);
        v.nullified = 0.0;
        assert_eq!(v.cast_check(Spell::BloodRush, ctx), Ok(()));
        // The chapel blocks a vampire without full power; dead blocks everyone; blood rush
        // needs a conscious caster, rejuvenate does not.
        v.powers.clear();
        let chapel = CastContext {
            in_chapel: true,
            ..ctx
        };
        assert!(v.cast_check(Spell::BloodRush, chapel).is_err());
        let unconscious = CastContext {
            conscious: false,
            ..ctx
        };
        assert_eq!(
            v.cast_check(Spell::BloodRush, unconscious),
            Err(CastFail::Unconscious)
        );
        let r = with_spell(Spell::Rejuvenate, 0.0);
        assert_eq!(r.cast_check(Spell::Rejuvenate, unconscious), Ok(()));
        let dead = CastContext { dead: true, ..ctx };
        assert!(r.cast_check(Spell::Rejuvenate, dead).is_err());
        // The cooldown.
        v.spells[0].ready_at = 31.0;
        assert_eq!(v.cast_check(Spell::BloodRush, ctx), Err(CastFail::Cooldown));
        assert_eq!(Spell::BloodRush.def().cooldown, 30.0);
        assert_eq!(Spell::Rejuvenate.def().cooldown, 20.0);
    }

    #[test]
    fn glare_has_two_charges_recharging_in_30_seconds() {
        let ctx = |now| CastContext {
            now,
            conscious: true,
            ..default()
        };
        let mut v = with_spell(Spell::Glare, 0.0);
        v.spend_glare_charge(10.0);
        assert_eq!(v.spell(Spell::Glare).unwrap().charges, 1);
        v.spend_glare_charge(12.0);
        // Out of charges: cooling down until the first recharge.
        assert_eq!(v.spell(Spell::Glare).unwrap().ready_at, 40.0);
        assert_eq!(
            v.cast_check(Spell::Glare, ctx(39.0)),
            Err(CastFail::Cooldown)
        );
        assert!(!v.recharge_glare(39.0));
        assert!(v.recharge_glare(40.0));
        assert_eq!(v.spell(Spell::Glare).unwrap().charges, 1);
        assert_eq!(v.cast_check(Spell::Glare, ctx(40.0)), Ok(()));
        assert!(v.recharge_glare(42.0));
        assert_eq!(v.spell(Spell::Glare).unwrap().charges, 2);
    }

    #[test]
    fn glare_deviation_follows_the_facing() {
        let at = TilePos::new(5, 5, 1);
        // Facing north: a victim in front (north, north-east) gets the full glare.
        assert_eq!(glare_deviation(at, Dir::North, TilePos::new(5, 6, 1)), 0);
        assert_eq!(glare_deviation(at, Dir::North, TilePos::new(6, 6, 1)), 0);
        assert_eq!(glare_deviation(at, Dir::North, TilePos::new(6, 5, 1)), 1);
        assert_eq!(glare_deviation(at, Dir::North, TilePos::new(5, 4, 1)), 2);
        assert_eq!(glare_deviation(at, Dir::North, at), 0);
    }

    #[test]
    fn info_panel_lists_every_specialization() {
        let umbrae = Subclass::Umbrae.ui();
        assert_eq!(umbrae.id, "umbrae");
        assert_eq!(umbrae.powers[0].name.as_deref(), Some("Покров тьмы"));
        assert_eq!(umbrae.powers[0].blood_required, Some(150));
        assert_eq!(Subclass::Gargantua.ui().powers.len(), 7);
        assert_eq!(Subclass::Dantalion.ui().full_powers.len(), 4);
        assert_eq!(Subclass::from_id("hemomancer"), Some(Subclass::Hemomancer));
        assert_eq!(Subclass::from_id("ancient"), None);
        assert_eq!(blood_text(12.5), "12.5");
        assert_eq!(blood_text(10.0), "10");
    }
}
