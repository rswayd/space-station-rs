//! Surgery (server), after tg `code/modules/surgery/operations` (`_operation.dm`
//! `/mob/living/perform_surgery`, `get_available_operations`, `/datum/operation_holder`,
//! `/datum/surgery_operation` `check_availability`, `is_available`, `get_tool_quality`,
//! `get_time_modifiers`, `get_location_modifier`, `try_perform`, `display_pain`, `on_failure`;
//! `operation_generic.dm`, `operation_healing.dm`, `operation_bone_repair.dm`,
//! `operation_organ_manip.dm`, `operation_amputation.dm`, `operation_add_limb.dm`
//! (`prosthetic_replacement` with a limb), `operation_revival.dm`, `operation_organ_repair.dm`,
//! `operation_debride.dm`, `operation_puncture.dm`, `operation_lobotomy.dm`, `operation_pacify.dm`,
//! `operation_filter.dm`), the BandaStation failure chance
//! (`modular_bandastation/medical/code/surgery/fail_chance.dm`), `/datum/element/surgery_aid`,
//! the advanced tools' mode switch (`surgery_tools.dm`) and buckling to operating tables and beds
//! (`game/objects/buckling.dm`, `buckle_lying`).
//!
//! The checked-out tg has no `/datum/surgery` + `/datum/surgery_step` any more: an operation is
//! picked by the tool used on a bodypart in some surgical state ([`state`]). A surgical aid
//! (drapes, bedsheet, cloak) clicked on a mob prepares the selected bodypart; out of combat mode
//! a click on a prepared mob lists the operations the held tool (or the empty hand) can do on the
//! selected zone: one starts at once, several open the tgui `ListInputWindow` (tg's radial
//! menu). The offered operations are the unlocked ones plus, for a patient lying on an operating
//! table, its operating computer's list ([`super::operating`]: disks, research), without those
//! another offered one replaces (`replaced_by`). Each operation is a `do_after` of
//! `time × min(speed, 2.5)`, where speed multiplies the tool quality (its `implements` entry ×
//! `toolspeed`), the surgeon's drunkenness (0.8 at the Ballmer peak), the surface (operating table
//! 1, table 1.25, bed 1.5, floor 2) and 0.8 for a patient who feels nothing (unconscious or an
//! analgesic in the blood); then it fails with BandaStation's chance (the time over 2.5× of the
//! base, +40% on a patient who feels it, darkness, +5% for basic tools, +10% for improvised
//! ones) or succeeds. A buckled mob lies on the table (dragged onto it, 2 s for someone else)
//! until an empty hand clicks the table or it resists.
//
// Organ manipulation takes an organ of the operated zone out onto the floor (hemostat) or puts
// the held organ into its empty slot ([`super::organs`]); amputation drops an arm or leg as an
// item and a limb item goes back onto its stump ([`super::bodyparts`]); the organ repairs
// (coronary bypass, lobectomy, hepatectomy, gastrectomy, ear, eye and brain surgery) work on the
// organ's damage.
// shortcut: no mood events, sounds, surgical memory, blood on the surgeon's gloves, self-surgery
// trait, manual cauterizing of yourself, ring accessories on gloves, wound scanning; no external
// organs or dummy organs from food (cybernetic implants go in and out like organs,
// [`super::cyberimps`]); no decapitation, robotic limb
// replacement or arbitrary prosthetics; no abductor, robotic, thick-skin or skull operations; no
// brainwashing. Lobotomy, pacification and brain surgery work on the brain's traumas
// ([`super::traumas`]).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmTool, DmWounding};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject, UiWindow};
use ss13_tgui::{UiInterface, UiStatus};

use super::bioware::Bioware;
use super::cyberimps::{self, ImplantSlot};
use super::operating::{OperatingComputer, advanced_surgeries};
use super::organs::{self, Slot};
use super::wounds::{CANT_WOUND, Limbs, Series, WoundKind, state};
use super::{MedJob, name_of, visible_range};
use crate::combat::CombatMode;
use crate::health::{BodyPosition, DamageType, Health, Stat, ZoneSelected, update_stat};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, Welder, do_after, stack_amount, tell, use_stack};
use crate::zone::{BodyZone, TargetZone};

/// `SURGERY_MODIFIER_FAILURE_THRESHOLD`, `FAILURE_CHANCE_PER_SECOND`.
pub const SURGERY_MODIFIER_FAILURE_THRESHOLD: f32 = 2.5;
pub const FAILURE_CHANCE_PER_SECOND: f32 = 10.0;
/// `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`.
pub const FIRE_MINIMUM_TEMPERATURE_TO_EXIST: f32 = 100.0 + 273.15;
/// BandaStation `SURGERY_TOOL_MOD_*`.
const TOOL_MOD_ADVANCED: f32 = 0.0;
const TOOL_MOD_ALIEN: f32 = 1.0;
const TOOL_MOD_AUGMENT: f32 = 0.0;
const TOOL_MOD_BASIC: f32 = 5.0;
const TOOL_MOD_GHETTO: f32 = 10.0;
/// `BALLMER_PEAK_LOW_END`, `BALLMER_PEAK_HIGH_END`.
const BALLMER_PEAK_LOW_END: f32 = 12.9;
const BALLMER_PEAK_HIGH_END: f32 = 13.8;
/// `/datum/surgery_operation/organ/repair` `heal_to_percent`, `failure_damage_percent`.
const REPAIR_HEAL_TO: f32 = 0.6;
const REPAIR_FAILURE_DAMAGE: f32 = 0.2;
/// Hepatectomy and gastrectomy `heal_to_percent`; their and lobectomy's `failure_damage_percent`.
const HEPATECTOMY_HEAL_TO: f32 = 0.1;
const GASTRECTOMY_HEAL_TO: f32 = 0.2;
const LOBECTOMY_FAILURE_DAMAGE: f32 = 0.1;
const CUT_FAILURE_DAMAGE: f32 = 0.15;
/// Ear and eye surgery's failure: the brain behind them takes 70.
const SENSE_FAILURE_BRAIN_DAMAGE: f32 = 70.0;
/// The brain's: `heal_to_percent` 0.25 (healed per step), `failure_damage_percent` 0.3.
const BRAIN_HEAL: f32 = 0.25;
const BRAIN_FAILURE_DAMAGE: f32 = 0.3;
/// Ear surgery's `40 SECONDS - temporary_deafness` added: 40 s of deafness left.
const EAR_SURGERY_DEAFNESS: f32 = 40.0;
/// Items with `/datum/element/surgery_aid`: drapes, the surgical processor, bedsheets, cloaks
/// and robe capes.
const AIDS: [&str; 5] = [
    "/obj/item/surgical_drapes",
    "/obj/item/surgical_processor",
    "/obj/item/bedsheet",
    "/obj/item/clothing/neck/cloak",
    "/obj/item/clothing/neck/robe_cape",
];

/// The item prepares patients for surgery (`surgery_aid`).
pub fn is_aid(path: &str) -> bool {
    AIDS.iter().any(|root| is_under(path, root))
}

/// The advanced tools' `/datum/component/transforming`: (type, inactive and active behaviour,
/// the message for inactive and active).
const ADVANCED_TOOLS: [(&str, &str, &str, &str, &str); 3] = [
    (
        "/obj/item/scalpel/advanced",
        "scalpel",
        "saw",
        "disabled bone-cutting mode",
        "enabled bone-cutting mode",
    ),
    (
        "/obj/item/retractor/advanced",
        "retractor",
        "hemostat",
        "gears set to retract",
        "gears set to clamp",
    ),
    (
        "/obj/item/cautery/advanced",
        "cautery",
        "drill",
        "lenses set to mend",
        "lenses set to drill",
    ),
];

/// tg `attack_self` of the laser scalpel, mechanical pinches and searing tool: the other
/// `tool_behaviour` (the laser scalpel also gains 1 force while sawing). True when `item` is one.
pub fn toggle_advanced_tool(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    let Some(&(root, off, on, off_text, on_text)) = ADVANCED_TOOLS
        .iter()
        .find(|(root, ..)| is_under(&path, root))
    else {
        return false;
    };
    let Some(mut tool) = world.get_mut::<DmTool>(item) else {
        return false;
    };
    let active = tool.behaviour != on;
    tool.behaviour = if active { on } else { off }.to_owned();
    if root == "/obj/item/scalpel/advanced"
        && let Some(mut dm) = world.get_mut::<DmItem>(item)
    {
        dm.force += if active { 1.0 } else { -1.0 };
    }
    tell(
        world,
        client,
        Span::Notice,
        if active { on_text } else { off_text },
    );
    true
}
/// `buckle_delay`.
pub const BUCKLE_DELAY: f32 = 20.0;

/// What an operation takes (tg `implements` keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Implement {
    /// `TOOL_*` behaviour.
    Tool(&'static str),
    /// An item type and its subtypes.
    Path(&'static str),
    /// `IMPLEMENT_HAND`.
    Hand,
}

/// tg `operation_flags` this port acts on.
pub mod flags {
    pub const LOOPING: u16 = 1 << 2;
    pub const SELF_OPERABLE: u16 = 1 << 5;
    pub const STANDING_ALLOWED: u16 = 1 << 6;
    pub const IGNORE_CLOTHES: u16 = 1 << 8;
    /// `OPERATION_LOCKED`: offered only from an operating computer's list (disks, research).
    pub const LOCKED: u16 = 1 << 9;
    /// `OPERATION_PRIORITY_NEXT_STEP`: the computer recommends it first.
    pub const PRIORITY_NEXT_STEP: u16 = 1 << 10;
}

/// tend wounds' radial options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Heal {
    Brute,
    Burn,
    /// `COMBO_SURGERY`: both at once.
    Combo,
}

/// The tend wounds operation types (tg `/datum/surgery_operation/basic/tend_wounds` and the
/// researched upgrades).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tend {
    Basic,
    Upgraded,
    Master,
    Combo,
    ComboUpgraded,
    ComboMaster,
}

impl Tend {
    pub const ALL: [Self; 6] = [
        Self::Basic,
        Self::Upgraded,
        Self::Master,
        Self::Combo,
        Self::ComboUpgraded,
        Self::ComboMaster,
    ];

    /// `can_heal = COMBO_SURGERY`.
    pub fn combo(self) -> bool {
        matches!(self, Self::Combo | Self::ComboUpgraded | Self::ComboMaster)
    }

    /// `healing_amount`, `healing_multiplier`.
    pub fn healing(self) -> (f32, f32) {
        match self {
            Self::Basic => (5.0, 0.07),
            Self::Upgraded => (5.0, 0.1),
            Self::Master => (5.0, 0.2),
            Self::Combo => (3.0, 0.07),
            Self::ComboUpgraded => (3.0, 0.1),
            Self::ComboMaster => (1.0, 0.4),
        }
    }

    fn path(self) -> &'static str {
        match self {
            Self::Basic => "/datum/surgery_operation/basic/tend_wounds",
            Self::Upgraded => "/datum/surgery_operation/basic/tend_wounds/upgraded",
            Self::Master => "/datum/surgery_operation/basic/tend_wounds/upgraded/master",
            Self::Combo => "/datum/surgery_operation/basic/tend_wounds/combo",
            Self::ComboUpgraded => "/datum/surgery_operation/basic/tend_wounds/combo/upgraded",
            Self::ComboMaster => "/datum/surgery_operation/basic/tend_wounds/combo/upgraded/master",
        }
    }

    fn replaced_by(self) -> Option<Self> {
        match self {
            Self::Basic => Some(Self::Upgraded),
            Self::Upgraded => Some(Self::Master),
            Self::Master | Self::ComboUpgraded => Some(Self::ComboMaster),
            Self::Combo => Some(Self::ComboUpgraded),
            Self::ComboMaster => None,
        }
    }

    fn rnd_name(self) -> &'static str {
        match self {
            Self::Basic => "Обработка ран",
            Self::Upgraded => "Обработка ран+",
            Self::Master => "Обработка ран++",
            Self::Combo => "Улучшенная обработка ран",
            Self::ComboUpgraded => "Улучшенная обработка ран+",
            Self::ComboMaster => "Улучшенная обработка ран++",
        }
    }
}

/// Where an operation works: tg `/datum/surgery_operation/limb` (the selected bodypart),
/// `basic` (its `target_zone` exactly) or `organ` (its organ's `zone` exactly).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Limb,
    Basic(TargetZone),
    /// The organ of this slot, on its `zone`.
    Organ(Slot),
}

/// The operations of a human (one per tg operation type and radial option).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    InciseSkin,
    RetractSkin,
    CloseSkin,
    ClampBleeders,
    UnclampBleeders,
    SawBones,
    FixBones,
    DrillBones,
    InciseOrgans,
    TendWounds(Tend, Heal),
    RepairDislocation,
    RepairHairline,
    ResetCompound,
    RepairCompound,
    /// Organ manipulation taking this organ out (`hemostat`).
    RemoveOrgan(Slot),
    /// Organ manipulation putting the held organ of this slot in.
    InsertOrgan(Slot),
    /// Organ manipulation taking the implant of this slot out.
    RemoveImplant(ImplantSlot),
    /// Organ manipulation putting the held implant in (its slot by the zone).
    InsertImplant,
    /// A bioware operation ([`super::bioware`]).
    Bioware(Bioware),
    Amputate,
    /// `prosthetic_replacement` with a limb item on a stump.
    AttachLimb,
    Revival,
    CoronaryBypass,
    BrainSurgery,
    Lobotomy,
    Pacify,
    Lobectomy,
    Hepatectomy,
    Gastrectomy,
    EarSurgery,
    EyeSurgery,
    Debride,
    RepairPuncture,
    SealVeins,
    FilterBlood,
}

/// The static part of an operation.
#[derive(Clone, Copy, Debug)]
pub struct OpDef {
    /// The tg type, what disks and research designs unlock.
    pub path: &'static str,
    /// The radial option's name.
    pub name: &'static str,
    /// `rnd_name || name`: the operating computer's catalog name.
    pub catalog_name: &'static str,
    pub desc: &'static str,
    /// Deciseconds.
    pub time: f32,
    pub flags: u16,
    pub target: Target,
    pub all_required: u16,
    pub any_required: u16,
    pub any_blocked: u16,
    pub implements: &'static [(Implement, f32)],
    /// The preop phrase: "Вы начинаете {verb} {zone} у {patient}...".
    pub verb: &'static str,
    /// `replaced_by`: hidden while that operation is offered too.
    pub replaced_by: Option<&'static str>,
    /// `get_recommended_tool()`.
    pub tool_rec: &'static str,
    /// The operation's own `all_required_strings()` / `all_blocked_strings()` additions.
    pub required: &'static [&'static str],
    pub blocked: &'static [&'static str],
}

use Implement::{Hand, Path, Tool};

const INCISE: &[(Implement, f32)] = &[
    (Tool("scalpel"), 1.0),
    (Path("/obj/item/melee/energy/sword"), 1.33),
    (Path("/obj/item/knife"), 1.5),
    (Path("/obj/item/shard"), 2.25),
    (Path("/obj/item/screwdriver"), 5.0),
    (Path("/obj/item/pen"), 5.0),
    (Path("/obj/item"), 3.33),
];
const CLAMP: &[(Implement, f32)] = &[
    (Tool("hemostat"), 1.0),
    (Tool("cutters"), 1.67),
    (Path("/obj/item/stack/package_wrap"), 2.85),
    (Path("/obj/item/stack/cable_coil"), 6.67),
];
const TEND: &[(Implement, f32)] = &[
    (Tool("hemostat"), 1.0),
    (Tool("screwdriver"), 1.5),
    (Tool("cutters"), 1.67),
    (Path("/obj/item/pen"), 1.8),
];
const ORGAN_CUT: &[(Implement, f32)] = &[
    (Tool("scalpel"), 1.05),
    (Path("/obj/item/melee/energy/sword"), 1.5),
    (Path("/obj/item/knife"), 2.25),
    (Path("/obj/item/shard"), 2.85),
];
/// organ manipulation's `remove_implements`.
const ORGAN_REMOVE: &[(Implement, f32)] = &[
    (Tool("hemostat"), 1.0),
    (Tool("crowbar"), 1.8),
    (Path("/obj/item/kitchen/fork"), 2.85),
];
const SENSE_REPAIR: &[(Implement, f32)] = &[
    (Tool("hemostat"), 1.05),
    (Tool("screwdriver"), 2.25),
    (Path("/obj/item/pen"), 4.0),
];
const SENSE_REPAIR_BLOCKED: &[&str] = &[
    "орган не должен быть кибернетическим",
    "если в конечности есть кости, они должны быть целы",
];
const ORGAN_REPAIR_BLOCKED: &[&str] = &[
    "орган не должен быть кибернетическим",
    "орган не должен был подвергаться хирургическому восстановлению ранее",
];

impl Op {
    /// Every operation and option, in tg type order.
    pub fn all() -> Vec<Self> {
        let mut ops = vec![
            Self::InciseSkin,
            Self::RetractSkin,
            Self::CloseSkin,
            Self::ClampBleeders,
            Self::UnclampBleeders,
            Self::SawBones,
            Self::FixBones,
            Self::DrillBones,
            Self::InciseOrgans,
        ];
        for tend in Tend::ALL {
            if tend.combo() {
                ops.push(Self::TendWounds(tend, Heal::Combo));
            } else {
                ops.push(Self::TendWounds(tend, Heal::Brute));
                ops.push(Self::TendWounds(tend, Heal::Burn));
            }
        }
        ops.extend([
            Self::RepairDislocation,
            Self::RepairHairline,
            Self::ResetCompound,
            Self::RepairCompound,
        ]);
        ops.extend(Slot::ALL.map(Self::RemoveOrgan));
        ops.extend(Slot::ALL.map(Self::InsertOrgan));
        ops.extend(ImplantSlot::ALL.map(Self::RemoveImplant));
        ops.push(Self::InsertImplant);
        ops.extend([
            Self::Amputate,
            Self::AttachLimb,
            Self::Revival,
            Self::CoronaryBypass,
            Self::BrainSurgery,
            Self::Lobotomy,
            Self::Pacify,
            Self::Lobectomy,
            Self::Hepatectomy,
            Self::Gastrectomy,
            Self::EarSurgery,
            Self::EyeSurgery,
            Self::Debride,
            Self::RepairPuncture,
            Self::SealVeins,
            Self::FilterBlood,
        ]);
        ops.extend(Bioware::ALL.map(Self::Bioware));
        ops
    }

    /// One operation per tg type (the operating computer's catalog).
    pub fn types() -> Vec<Self> {
        let mut seen = Vec::new();
        Self::all()
            .into_iter()
            .filter(|op| {
                let path = op.def().path;
                let new = !seen.contains(&path);
                seen.push(path);
                new
            })
            .collect()
    }

    /// The operation of a tg type path, for disks and designs (`None`: not ported).
    pub fn of_path(path: &str) -> Option<Self> {
        Self::types().into_iter().find(|op| op.def().path == path)
    }

    pub fn def(self) -> OpDef {
        use state::*;
        let base = OpDef {
            path: "",
            name: "",
            catalog_name: "",
            desc: "",
            time: 10.0,
            flags: 0,
            target: Target::Limb,
            all_required: 0,
            any_required: 0,
            any_blocked: 0,
            implements: &[],
            verb: "оперировать",
            replaced_by: None,
            tool_rec: "",
            required: &[],
            blocked: &[],
        };
        let organ_manipulation = OpDef {
            path: "/datum/surgery_operation/limb/organ_manipulation/internal",
            catalog_name: "Манипуляции с органами",
            desc: "Манипуляции с внутренними органами пациента.",
            time: 24.0,
            all_required: SKIN_OPEN | ORGANS_CUT,
            tool_rec: "hemostat / орган",
            required: &[
                "если оперируется мозг или любые органы грудной клетки, кость ДОЛЖНА быть \
                 распилена",
            ],
            ..base
        };
        let organ_repair = OpDef {
            all_required: SKIN_OPEN | ORGANS_CUT | BONE_SAWED,
            required: &["орган должен быть умеренно поврежден"],
            blocked: ORGAN_REPAIR_BLOCKED,
            ..base
        };
        let def = match self {
            Self::InciseSkin => OpDef {
                path: "/datum/surgery_operation/limb/incise_skin",
                name: "Сделать разрез кожи",
                desc: "Сделайте разрез на коже пациента, чтобы получить доступ к внутренним \
                       органам. Вызывает хирургическое состояние \"кожа разрезана\".",
                time: 16.0,
                any_blocked: ALL_SKIN,
                implements: INCISE,
                verb: "делать разрез на",
                tool_rec: "scalpel",
                ..base
            },
            Self::RetractSkin => OpDef {
                path: "/datum/surgery_operation/limb/retract_skin",
                name: "retract skin",
                desc: "Раздвигает кожу пациента, чтобы получить доступ к внутренним органам. \
                       Вызывает хирургическое состояние \"кожа раздвинута\".",
                time: 24.0,
                all_required: SKIN_CUT,
                implements: &[
                    (Tool("retractor"), 1.0),
                    (Tool("screwdriver"), 2.25),
                    (Tool("cutters"), 2.85),
                    (Path("/obj/item/stack/rods"), 2.85),
                    (Path("/obj/item/kitchen/fork"), 2.85),
                ],
                verb: "раздвигать кожу на",
                tool_rec: "retractor",
                ..base
            },
            Self::CloseSkin => OpDef {
                path: "/datum/surgery_operation/limb/close_skin",
                name: "Прижечь разрез кожи",
                desc: "Прижечь разрез на коже пациента, закрывая его. Устраняет большинство \
                       хирургических состояний.",
                time: 24.0,
                any_required: ALL_SKIN,
                implements: &[
                    (Tool("cautery"), 1.0),
                    (Path("/obj/item/stack/medical/suture"), 1.0),
                    (Path("/obj/item/gun/energy/laser"), 1.15),
                    (Tool("welder"), 1.5),
                    (Path("/obj/item"), 3.33),
                ],
                verb: "прижигать разрез на",
                tool_rec: "cautery",
                required: &["конечность должна иметь кожу"],
                ..base
            },
            Self::ClampBleeders => OpDef {
                path: "/datum/surgery_operation/limb/clamp_bleeders",
                name: "Зажать сосуды",
                desc: "Накладывает зажим на кровеносных сосудах пациента, чтобы предотвратить \
                       потерю крови. Вызывает хирургическое состояние \"сосуды зажаты\".",
                time: 24.0,
                all_required: SKIN_OPEN | VESSELS_UNCLAMPED,
                implements: CLAMP,
                verb: "зажимать кровеносные сосуды в",
                tool_rec: "hemostat",
                ..base
            },
            Self::UnclampBleeders => OpDef {
                path: "/datum/surgery_operation/limb/unclamp_bleeders",
                name: "Разжать сосуды",
                desc: "Снять зажимы с кровеносных сосудов в теле пациента, чтобы восстановить \
                       кровоток. Убирает хирургическое состояние \"сосуды зажаты\".",
                time: 24.0,
                all_required: SKIN_OPEN | VESSELS_CLAMPED,
                implements: CLAMP,
                verb: "разжимать кровеносные сосуды в",
                tool_rec: "hemostat",
                required: &["конечность должна иметь кровеносные сосуды"],
                ..base
            },
            Self::SawBones => OpDef {
                path: "/datum/surgery_operation/limb/saw_bones",
                name: "Распилить кость",
                desc: "Пропилите кость пациента, чтобы получить доступ к внутренним органам. \
                       Вызывает хирургическое состояние \"кость распилена\".",
                time: 54.0,
                all_required: SKIN_OPEN,
                any_blocked: BONE_SAWED | BONE_DRILLED,
                implements: &[
                    (Tool("saw"), 1.0),
                    (Path("/obj/item/shovel/serrated"), 1.33),
                    (Path("/obj/item/melee/arm_blade"), 1.33),
                    (Path("/obj/item/fireaxe"), 2.0),
                    (Path("/obj/item/hatchet"), 2.85),
                    (Path("/obj/item/knife/butcher"), 2.85),
                    (Path("/obj/item"), 4.0),
                ],
                verb: "пилить кости в",
                tool_rec: "saw",
                ..base
            },
            Self::FixBones => OpDef {
                path: "/datum/surgery_operation/limb/fix_bones",
                name: "Фиксация сломанной кости",
                desc: "Соединить рассечённые или зафиксировать сломанные кости. Убирает \
                       хирургические состояния \"кость распилена\" и \"кость просверлена\".",
                time: 40.0,
                all_required: SKIN_OPEN,
                any_required: ALL_BONE,
                implements: &[
                    (Path("/obj/item/stack/medical/bone_gel"), 1.0),
                    (
                        Path("/obj/item/stack/medical/wrap/sticky_tape/surgical"),
                        1.0,
                    ),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape/super"), 2.0),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape"), 3.33),
                ],
                verb: "лечить кости в",
                tool_rec: "bone gel",
                required: &["конечность должна иметь кости"],
                ..base
            },
            Self::DrillBones => OpDef {
                path: "/datum/surgery_operation/limb/drill_bones",
                name: "Сверление кости",
                desc: "Просверливание кости пациента. Вызывает хирургическое состояние \"кость \
                       просверлена\".",
                time: 30.0,
                all_required: SKIN_OPEN,
                any_blocked: BONE_SAWED | BONE_DRILLED,
                implements: &[
                    (Tool("drill"), 1.0),
                    (Path("/obj/item/screwdriver/power"), 1.25),
                    (Path("/obj/item/pickaxe/drill"), 1.67),
                    (Tool("screwdriver"), 4.0),
                    (Path("/obj/item/kitchen/spoon"), 5.0),
                    (Path("/obj/item"), 6.67),
                ],
                verb: "просверливать кость в",
                tool_rec: "drill",
                ..base
            },
            Self::InciseOrgans => OpDef {
                path: "/datum/surgery_operation/limb/incise_organs",
                name: "Сделать разрез органа",
                desc: "Сделайте разрез на тканях внутренних органов, что позволит вылечить или \
                       манипулировать с органом. Вызывает хирургическое состояние \"орган \
                       разрезан\".",
                time: 24.0,
                all_required: SKIN_OPEN,
                any_blocked: ORGANS_CUT,
                implements: &[
                    (Tool("scalpel"), 1.0),
                    (Path("/obj/item/melee/energy/sword"), 1.33),
                    (Path("/obj/item/knife"), 1.5),
                    (Path("/obj/item/shard"), 2.25),
                    (Path("/obj/item/pen"), 5.0),
                    (Path("/obj/item"), 3.33),
                ],
                verb: "делать разрез органа в",
                tool_rec: "scalpel",
                ..base
            },
            Self::TendWounds(tend, heal) => OpDef {
                path: tend.path(),
                name: match heal {
                    Heal::Brute => "Обработка ушибов",
                    Heal::Burn => "Обработка ожогов",
                    Heal::Combo => "Обработайте ушибы и ожоги",
                },
                catalog_name: tend.rnd_name(),
                desc: "Проведите поверхностный уход за ушибами и ожогами пациента.",
                time: if tend.combo() { 10.0 } else { 25.0 },
                flags: flags::LOOPING
                    | flags::IGNORE_CLOTHES
                    | if tend == Tend::Basic {
                        0
                    } else {
                        flags::LOCKED
                    },
                target: Target::Basic(TargetZone::Chest),
                any_required: ALL_SKIN,
                implements: TEND,
                verb: "обрабатывать раны:",
                replaced_by: tend.replaced_by().map(Tend::path),
                tool_rec: "hemostat",
                required: &["у пациента должны быть ушибы или ожоги"],
                ..base
            },
            Self::RepairDislocation => OpDef {
                path: "/datum/surgery_operation/limb/repair_dislocation",
                name: "reset dislocation",
                desc: "Reset a dislocated bone in a patient's limb. Similar to the field \
                       procedure, but quicker and safer due to being performed in a controlled \
                       environment.",
                time: 24.0,
                flags: flags::STANDING_ALLOWED | flags::IGNORE_CLOTHES,
                implements: &[
                    (Tool("bonesetter"), 1.0),
                    (Tool("crowbar"), 2.0),
                    (Hand, 5.0),
                ],
                verb: "вправлять вывих на",
                tool_rec: "bonesetter",
                required: &["the limb must be dislocated"],
                ..base
            },
            Self::RepairHairline => OpDef {
                path: "/datum/surgery_operation/limb/repair_hairline",
                name: "Сращивание трещины",
                desc: "Заживление трещины в кости пациента.",
                time: 40.0,
                any_required: ALL_SKIN,
                implements: &[
                    (Tool("bonesetter"), 1.0),
                    (Path("/obj/item/stack/medical/bone_gel"), 1.0),
                    (
                        Path("/obj/item/stack/medical/wrap/sticky_tape/surgical"),
                        1.0,
                    ),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape/super"), 2.0),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape"), 3.33),
                ],
                verb: "сращивать трещину в",
                tool_rec: "bonesetter",
                required: &["конечность должна иметь трещину"],
                ..base
            },
            Self::ResetCompound => OpDef {
                path: "/datum/surgery_operation/limb/reset_compound",
                name: "reset compound fracture",
                desc: "Вправление сложного перелома кости пациента, для подготавливки её к \
                       правильному заживлению.",
                time: 60.0,
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Tool("bonesetter"), 1.0),
                    (
                        Path("/obj/item/stack/medical/wrap/sticky_tape/surgical"),
                        1.66,
                    ),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape/super"), 2.5),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape"), 5.0),
                ],
                verb: "вправлять открытый перелом в",
                tool_rec: "bonesetter",
                required: &["конечность должна иметь сложный перелом"],
                ..base
            },
            Self::RepairCompound => OpDef {
                path: "/datum/surgery_operation/limb/repair_compound",
                name: "repair compound fracture",
                desc: "Залечивание сложного перелома кости пациента.",
                time: 40.0,
                any_required: ALL_SKIN,
                implements: &[
                    (Path("/obj/item/stack/medical/bone_gel"), 1.0),
                    (
                        Path("/obj/item/stack/medical/wrap/sticky_tape/surgical"),
                        1.0,
                    ),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape/super"), 2.0),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape"), 3.33),
                ],
                verb: "заживлять открытый перелом в",
                tool_rec: "bone gel",
                required: &["сложный перелом конечности должен быть вправлен"],
                ..base
            },
            Self::RemoveOrgan(slot) => OpDef {
                name: manip_names(slot).0,
                verb: "извлекать",
                implements: ORGAN_REMOVE,
                ..organ_manipulation
            },
            Self::RemoveImplant(slot) => OpDef {
                name: slot.remove_name(),
                verb: "извлекать",
                implements: ORGAN_REMOVE,
                ..organ_manipulation
            },
            Self::InsertImplant => OpDef {
                name: "Вставить имплант",
                verb: "вставлять",
                implements: &[(Path(cyberimps::ROOT), 1.0)],
                ..organ_manipulation
            },
            Self::InsertOrgan(slot) => OpDef {
                name: manip_names(slot).1,
                verb: "вставлять",
                // shortcut: no food or plants as dummy organs (`random_item_organ_pool`).
                implements: &[(Path("/obj/item/organ"), 1.0)],
                ..organ_manipulation
            },
            Self::Amputate => OpDef {
                path: "/datum/surgery_operation/limb/amputate",
                name: "Ампутация конечности",
                catalog_name: "Разъединение (Ампутация)",
                desc: "Отделение конечности от тела пациента.",
                time: 64.0,
                all_required: SKIN_OPEN | BONE_SAWED,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Path("/obj/item/shears"), 0.33),
                    (Tool("saw"), 1.0),
                    (Tool("scalpel"), 1.0),
                    (Path("/obj/item/melee/arm_blade"), 1.25),
                    (Path("/obj/item/shovel/serrated"), 1.33),
                    (Path("/obj/item/fireaxe"), 2.0),
                    (Path("/obj/item/hatchet"), 2.5),
                    (Path("/obj/item/knife/butcher"), 4.0),
                ],
                verb: "ампутировать",
                tool_rec: "amputation shears",
                ..base
            },
            Self::AttachLimb => OpDef {
                path: "/datum/surgery_operation/limb/prosthetic_replacement",
                name: "Прикрепить конечность",
                catalog_name: "prosthetic replacement",
                desc: "Замена отсутствующей конечности протезом (или произвольным предметом).",
                time: 32.0,
                flags: flags::STANDING_ALLOWED | flags::PRIORITY_NEXT_STEP | flags::IGNORE_CLOTHES,
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                // shortcut: no arbitrary items as prosthetic arms.
                implements: &[(Path("/obj/item/bodypart"), 1.0)],
                verb: "заменять отсутствующую конечность в",
                tool_rec: "любая конечность / любой предмет",
                required: &["конечность должна отсутствовать / быть повреждённой"],
                ..base
            },
            Self::Revival => OpDef {
                path: "/datum/surgery_operation/basic/revival",
                name: "Дефибрилляция мозга",
                catalog_name: "Дефибриляция мозга (Оживление)",
                desc: "Использование дефибриллятора, чтобы вернуть мозг пациента к жизни.",
                time: 50.0,
                target: Target::Basic(TargetZone::Head),
                all_required: SKIN_OPEN | BONE_SAWED,
                implements: &[
                    (Path("/obj/item/shockpaddles"), 1.0),
                    (Path("/obj/item/melee/touch_attack/shock"), 1.0),
                    (Path("/obj/item/melee/baton/security"), 1.33),
                    (Path("/obj/item/gun/energy"), 1.67),
                ],
                tool_rec: "defibrillator paddles",
                required: &[
                    "пациент должен быть мёртв",
                    "пациент должен быть в состоянии, пригодном для реанимации",
                ],
                ..base
            },
            Self::CoronaryBypass => OpDef {
                path: "/datum/surgery_operation/organ/repair/coronary_bypass",
                name: "Коронарное шунтирование",
                catalog_name: "Аортокоронарное шунтирование (Восстановление сердца)",
                desc: "Установка шунта на поврежденное сердце пациента для восстановления \
                       нормального кровотока. Эту процедуру можно провести только один раз для \
                       каждого органа.",
                time: 90.0,
                target: Target::Organ(Slot::Heart),
                all_required: SKIN_OPEN | ORGANS_CUT | BONE_SAWED,
                implements: &[
                    (Tool("hemostat"), 1.05),
                    (Tool("cutters"), 2.85),
                    (Path("/obj/item/stack/package_wrap"), 6.67),
                    (Path("/obj/item/stack/cable_coil"), 2.0),
                ],
                tool_rec: "hemostat",
                required: &["орган должен быть умеренно поврежден"],
                blocked: ORGAN_REPAIR_BLOCKED,
                ..base
            },
            Self::BrainSurgery => OpDef {
                path: "/datum/surgery_operation/organ/repair/brain",
                name: "Операция на мозге",
                catalog_name: "Нейрохирургия (Хирургия мозга)",
                desc: "Восстановление поврежденных тканей мозга пациента для возвращения \
                       когнитивных функций.",
                time: 100.0,
                flags: flags::LOOPING,
                target: Target::Organ(Slot::Brain),
                all_required: SKIN_OPEN | BONE_SAWED,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Tool("hemostat"), 1.05),
                    (Tool("screwdriver"), 2.85),
                    (Path("/obj/item/pen"), 6.67),
                ],
                tool_rec: "hemostat",
                blocked: &["орган не должен быть кибернетическим"],
                ..base
            },
            Self::Lobotomy => OpDef {
                path: "/datum/surgery_operation/organ/lobotomy",
                name: "Лоботомия",
                catalog_name: "Лоботомия (Лоботомия)",
                desc: "Исправление большинства травм мозга пациента с риском вызвать новые \
                       постоянные травмы.",
                flags: flags::LOCKED,
                target: Target::Organ(Slot::Brain),
                all_required: SKIN_OPEN | BONE_SAWED,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Tool("scalpel"), 1.15),
                    (Path("/obj/item/melee/energy/sword"), 0.55),
                    (Path("/obj/item/knife"), 2.85),
                    (Path("/obj/item/shard"), 4.0),
                    (Path("/obj/item"), 5.0),
                ],
                tool_rec: "Любой острый предмет",
                blocked: &["орган не должен быть кибернетическим"],
                ..base
            },
            Self::Pacify => OpDef {
                path: "/datum/surgery_operation/organ/pacify",
                name: "pacification",
                catalog_name: "Паксопсия (Пацификация)",
                desc: "Удаление агрессивных наклонностей из мозга пациента.",
                time: 40.0,
                flags: flags::LOCKED,
                target: Target::Organ(Slot::Brain),
                all_required: SKIN_OPEN | BONE_SAWED,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Tool("hemostat"), 1.0),
                    (Tool("screwdriver"), 2.85),
                    (Path("/obj/item/pen"), 6.67),
                ],
                tool_rec: "hemostat",
                blocked: &["орган не должен быть кибернетическим"],
                ..base
            },
            Self::Lobectomy => OpDef {
                path: "/datum/surgery_operation/organ/repair/lobectomy",
                name: "Удаление поврежденной доли легкого",
                catalog_name: "Лобэктомия (Восстановление лёгких)",
                desc: "Восстановление поврежденных лёгких пациента путем удаления наиболее \
                       поврежденной доли. Эту процедуру можно провести только один раз для \
                       каждого органа.",
                time: 42.0,
                target: Target::Organ(Slot::Lungs),
                implements: ORGAN_CUT,
                tool_rec: "scalpel",
                ..organ_repair
            },
            Self::Hepatectomy => OpDef {
                path: "/datum/surgery_operation/organ/repair/hepatectomy",
                name: "Удаление поврежденной части печени",
                catalog_name: "Гепатэктомия (Восстановление печени)",
                desc: "Восстановление поврежденной печени пациента путем удаления наиболее \
                       поврежденного сегмента. Эту процедуру можно провести только один раз \
                       для каждого органа.",
                time: 52.0,
                target: Target::Organ(Slot::Liver),
                implements: ORGAN_CUT,
                tool_rec: "scalpel",
                ..organ_repair
            },
            Self::Gastrectomy => OpDef {
                path: "/datum/surgery_operation/organ/repair/gastrectomy",
                name: "Удаление поврежденной части желудка",
                catalog_name: "Гастрэктомия (Восстановление желудка)",
                desc: "Восстановление желудка пациента путем удаления поврежденного сегмента. \
                       Эту процедуру можно провести только один раз для каждого органа.",
                time: 52.0,
                target: Target::Organ(Slot::Stomach),
                implements: &[
                    (Tool("scalpel"), 1.05),
                    (Path("/obj/item/melee/energy/sword"), 1.5),
                    (Path("/obj/item/knife"), 2.25),
                    (Path("/obj/item/shard"), 2.85),
                    (Path("/obj/item"), 4.0),
                ],
                tool_rec: "scalpel",
                ..organ_repair
            },
            Self::EarSurgery => OpDef {
                path: "/datum/surgery_operation/organ/repair/ears",
                name: "Операция на ушах",
                catalog_name: "Ототомия (Хирургия ушей)",
                desc: "Восстановление поврежденных ушей пациента для возвращения слуха.",
                time: 64.0,
                target: Target::Organ(Slot::Ears),
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                implements: SENSE_REPAIR,
                tool_rec: "hemostat",
                required: &[],
                blocked: SENSE_REPAIR_BLOCKED,
                ..organ_repair
            },
            Self::EyeSurgery => OpDef {
                path: "/datum/surgery_operation/organ/repair/eyes",
                name: "Операция на глазах",
                catalog_name: "Витрэктомия (Хирургия глаз)",
                desc: "Восстановление поврежденных глаз пациента для возвращения зрения.",
                time: 64.0,
                target: Target::Organ(Slot::Eyes),
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                implements: SENSE_REPAIR,
                tool_rec: "hemostat",
                required: &[],
                blocked: SENSE_REPAIR_BLOCKED,
                ..organ_repair
            },
            Self::FilterBlood => OpDef {
                path: "/datum/surgery_operation/limb/filter_blood",
                name: "Фильтрация крови",
                catalog_name: "Гемодиализ (Фильтрация крови)",
                desc: "Удаление нежелательных химических веществ из кровотока пациента.",
                time: 25.0,
                flags: flags::LOOPING,
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[(Path("/obj/item/blood_filter"), 1.0)],
                tool_rec: "blood filter",
                required: &["операция на груди", "пациент не должен быть хаском"],
                ..base
            },
            Self::Bioware(kind) => bioware_def(kind, base),
            Self::Debride => OpDef {
                path: "/datum/surgery_operation/limb/debride",
                name: "Обработка зараженной плоти",
                catalog_name: "Обработка раны",
                desc: "Удаление инфицированной или омертвевшей плоти из раны пациента, чтобы \
                       ускорить заживление.",
                time: 30.0,
                flags: flags::LOOPING | flags::PRIORITY_NEXT_STEP,
                implements: &[
                    (Tool("hemostat"), 1.0),
                    (Tool("scalpel"), 1.25),
                    (Tool("saw"), 1.66),
                    (Tool("cutters"), 2.5),
                ],
                tool_rec: "hemostat",
                required: &["на конечности должен быть ожог второй степени или хуже"],
                ..base
            },
            Self::RepairPuncture => OpDef {
                path: "/datum/surgery_operation/limb/repair_puncture",
                name: "Соединение сосудов",
                desc: "Соединение разорванных кровеносных сосудов пациента для подготовки к их \
                       прижиганию.",
                time: 30.0,
                flags: flags::PRIORITY_NEXT_STEP,
                all_required: SKIN_OPEN | ORGANS_CUT,
                implements: &[
                    (Tool("hemostat"), 1.0),
                    (Tool("scalpel"), 1.15),
                    (Tool("cutters"), 2.5),
                ],
                tool_rec: "hemostat",
                required: &["конечность должна иметь необработанную колотую рану"],
                ..base
            },
            Self::SealVeins => OpDef {
                path: "/datum/surgery_operation/limb/seal_veins",
                name: "Прижигание сосудов",
                desc: "Прижигание ранее соединенных кровеносных сосудов пациента.",
                time: 32.0,
                flags: flags::PRIORITY_NEXT_STEP,
                all_required: SKIN_OPEN | ORGANS_CUT,
                implements: &[
                    (Tool("cautery"), 1.0),
                    (Path("/obj/item/gun/energy/laser"), 1.12),
                    (Tool("welder"), 1.5),
                    (Path("/obj/item"), 3.33),
                ],
                tool_rec: "cautery",
                required: &["конечность должна иметь подготовленную колотую рану"],
                ..base
            },
        };
        OpDef {
            catalog_name: if def.catalog_name.is_empty() {
                def.name
            } else {
                def.catalog_name
            },
            ..def
        }
    }

    /// tg `get_requirements()`: all of, one of, optional, blocking.
    pub fn requirements(self) -> [Vec<String>; 4] {
        let def = self.def();
        let mut all: Vec<String> = match def.target {
            Target::Basic(zone) => {
                let zone = zone_name(zone);
                vec![format!("операция на {zone} (цель: {zone})")]
            }
            Target::Organ(slot) => vec![format!(
                "операция на {} (цель {})",
                slot.spec().name,
                slot.spec().zone.dm()
            )],
            Target::Limb => Vec::new(),
        };
        all.extend(state_guides(def.all_required, "требуется"));
        if def.flags & flags::STANDING_ALLOWED == 0 {
            all.push("пациент должен лежать".into());
        }
        all.extend(def.required.iter().map(|s| (*s).to_owned()));
        let mut any = Vec::new();
        let mut parsed = def.any_required;
        for (group, text) in [
            (
                state::ALL_BONE,
                "кость должна быть распилена или просверлена",
            ),
            (state::ALL_SKIN, "кожа должна быть разрезана или вскрыта"),
            (
                state::ALL_VESSEL,
                "кровеносные сосуды должны быть зажаты или разжаты",
            ),
        ] {
            if parsed & group == group {
                parsed &= !group;
                any.push(text.to_owned());
            }
        }
        any.extend(state_guides(parsed, "требуется"));
        let optional = if def.flags & flags::SELF_OPERABLE != 0 {
            vec!["хирург может сделать это самостоятельно".to_owned()]
        } else {
            Vec::new()
        };
        let mut blocked = Vec::new();
        let mut parsed = def.any_blocked;
        for (group, text) in [
            (state::ALL_BONE, "кость должна быть целой"),
            (state::ALL_SKIN, "кожа должна быть целой"),
            (state::ALL_VESSEL, "кровеносные сосуды должны быть целы"),
        ] {
            if parsed & group == group {
                parsed &= !group;
                blocked.push(text.to_owned());
            }
        }
        blocked.extend(state_guides(parsed, "не требуется"));
        if matches!(def.target, Target::Basic(_)) {
            blocked.push("пациент не должен быть роботом".into());
        }
        blocked.extend(def.blocked.iter().map(|s| (*s).to_owned()));
        if def.flags & flags::IGNORE_CLOTHES == 0 {
            blocked.push("рабочая область не должна быть закрыта одеждой".into());
        }
        [all, any, optional, blocked]
    }
}

/// The bioware operations (`/datum/surgery_operation/limb/bioware`: by hand, through open skin,
/// sawed bone and cut organs, locked).
fn bioware_def(kind: Bioware, base: OpDef) -> OpDef {
    use state::*;
    const CIRCULATION: &[&str] = &[
        "пациент ранее не должен был проходить Симваскулодез (Венозная \
                                    нить) OR Миоваскулопластика (Мышечная мембрана вены)",
    ];
    const NERVES: &[&str] = &[
        "пациент ранее не должен был проходить Симневродез (Сращивание \
                               нервов) OR Ксантоневропластика (Заземление нервов)",
    ];
    const CORTEX: &[&str] = &[
        "пациент ранее не должен был проходить Энцефалофрактопластика \
                               (Складывание коры) OR Энцефалопластика (Отпечаток коры)",
    ];
    const CHEST: &[&str] = &["операция на грудь (цель грудь)"];
    const HEAD: &[&str] = &["операция на голова (цель голова)"];
    let bioware = OpDef {
        time: 125.0,
        flags: flags::LOCKED,
        all_required: SKIN_OPEN | BONE_SAWED | ORGANS_CUT,
        implements: &[(Hand, 1.0)],
        tool_rec: "hands",
        required: CHEST,
        ..base
    };
    match kind {
        Bioware::VeinThreading => OpDef {
            path: "/datum/surgery_operation/limb/bioware/vein_threading",
            name: "Венозная нить",
            catalog_name: "Симваскулодез (Венозная нить)",
            desc: "Сплетение вен пациента в усиленную сетку, для уменьшения кровопотери при \
                   травмах.",
            blocked: CIRCULATION,
            ..bioware
        },
        Bioware::MuscledVeins => OpDef {
            path: "/datum/surgery_operation/limb/bioware/muscled_veins",
            name: "Мышечная мембрана вены",
            catalog_name: "Миоваскулопластика (Мышечная мембрана вены)",
            desc: "Добавление мышечной оболочки к венам пациента, что позволит ему перекачивать \
                   кровь без участия сердца.",
            blocked: CIRCULATION,
            ..bioware
        },
        Bioware::NerveSplicing => OpDef {
            path: "/datum/surgery_operation/limb/bioware/nerve_splicing",
            name: "Сращивание нервов",
            catalog_name: "Симневродез (Сращивание нервов)",
            desc: "Соединение нервов пациента вместе, чтобы сделать их более устойчивыми к \
                   оглушению.",
            time: 155.0,
            blocked: NERVES,
            ..bioware
        },
        Bioware::NerveGrounding => OpDef {
            path: "/datum/surgery_operation/limb/bioware/nerve_grounding",
            name: "Заземление нервов",
            catalog_name: "Ксантоневропластика (Заземление нервов)",
            desc: "Перенаправление нервов пациента так, чтобы они действовали как заземляющие \
                   стержни, защищая их от поражения электрическим током.",
            time: 155.0,
            blocked: NERVES,
            ..bioware
        },
        Bioware::CortexImprint => OpDef {
            path: "/datum/surgery_operation/limb/bioware/cortex_imprint",
            name: "Отпечаток коры",
            catalog_name: "Энцефалопластика (Отпечаток коры)",
            desc: "Биологическая модернизация, которая формирует самозапечатлевающийся узор на \
                   коре головного мозга пациента, увеличивая плотность и устойчивость нервных \
                   клеток.",
            required: HEAD,
            blocked: CORTEX,
            ..bioware
        },
    }
}

/// The bioware operations' `on_preop` and `on_success` lines: (own, others) of each, the pain.
fn bioware_lines(kind: Bioware, target: &str, user: &str) -> [String; 6] {
    let (start, started, done, did, pain_start, pain_done) = match kind {
        Bioware::VeinThreading => (
            "начинаете плести кровеносную систему",
            "начинает плести кровеносную систему",
            "сплетаете кровеносную систему у {t} в прочную сеть!",
            "сплетает кровеносную систему у {t} в прочную сеть!",
            "Всё ваше тело горит в агонии!",
            "Вы можете почувствовать, как кровь движется по усиленным венам!",
        ),
        Bioware::MuscledVeins => (
            "начинаете обматывать мышцами кровеносные сосуды",
            "начинает обматывать мышцами кровеносные сосуды",
            "изменяете форму кровеносных сосудов у {t}, добавляя мышечную оболочку!",
            "изменяет форму кровеносных сосудов у {t}, добавляя мышечную оболочку!",
            "Всё ваше тело горит в агонии!",
            "Вы можете чувствовать, как мощные удары вашего сердца разносятся по всему телу!",
        ),
        Bioware::NerveSplicing => (
            "начинаете соединять нервы",
            "начинает соединять нервы",
            "успешно сращиваете нервную систему у {t}!",
            "успешно сращивает нервную систему у {t}!",
            "Все ваше тело немеет!",
            "Вы вновь обретаете чувствительность в своем теле; вам кажется, что всё происходит \
             вокруг вас в замедлении!",
        ),
        Bioware::NerveGrounding => (
            "начинаете перенаправлять нервы",
            "начинает перенаправлять нервы",
            "успешно перенаправляете нервную систему у {t}!",
            "успешно перенаправляет нервы у {t}!",
            "Все ваше тело немеет!",
            "Вы возвращаете своему телу ощущение свежести! Вы чувствуете прилив сил!",
        ),
        Bioware::CortexImprint => (
            "начинаете вырезать на внешней коре головного мозга самопечатающийся шаблон",
            "начинает вырезать на внешней коре головного мозга самопечатающийся шаблон",
            "преобразуете внешнюю кору головного мозга у {t} в самопечатающийся шаблон!",
            "перестраивает внешнюю кору головного мозга у {t} в самопечатающийся шаблон!",
            "Ваша голова раскалывается от ужасной боли, с ней почти невозможно справиться!",
            "Ваш мозг становится сильнее... более устойчивым!",
        ),
    };
    [
        format!("Вы {start} у {target}."),
        format!("{user} {started} у {target}."),
        format!("Вы {}", done.replace("{t}", target)),
        format!("{user} {}", did.replace("{t}", target)),
        pain_start.to_owned(),
        pain_done.to_owned(),
    ]
}

/// Organ manipulation's radial options: "Извлечь …", "Вставить …".
fn manip_names(slot: Slot) -> (&'static str, &'static str) {
    match slot {
        Slot::Brain => ("Извлечь мозг", "Вставить мозг"),
        Slot::Appendix => ("Извлечь аппендикс", "Вставить аппендикс"),
        Slot::Stomach => ("Извлечь желудок", "Вставить желудок"),
        Slot::Ears => ("Извлечь уши", "Вставить уши"),
        Slot::Eyes => ("Извлечь глаза", "Вставить глаза"),
        Slot::Lungs => ("Извлечь лёгкие", "Вставить лёгкие"),
        Slot::Heart => ("Извлечь сердце", "Вставить сердце"),
        Slot::Liver => ("Извлечь печень", "Вставить печень"),
        Slot::Tongue => ("Извлечь язык", "Вставить язык"),
    }
}

/// The organ an `organ` operation works on.
pub fn organ_target(op: Op) -> Option<Slot> {
    match op.def().target {
        Target::Organ(slot) => Some(slot),
        _ => None,
    }
}

/// `parse_zone`.
fn zone_name(zone: TargetZone) -> &'static str {
    match zone {
        TargetZone::Head => "голова",
        TargetZone::Eyes => "глаза",
        TargetZone::Mouth => "рот",
        TargetZone::Chest => "грудь",
        TargetZone::Groin => "пах",
        TargetZone::LArm => "левая рука",
        TargetZone::RArm => "правая рука",
        TargetZone::LLeg => "левая нога",
        TargetZone::RLeg => "правая нога",
    }
}

/// `SURGERY_STATE_READABLE`, in bit order.
pub const STATE_READABLE: [(u16, &str); 8] = [
    (state::SKIN_CUT, "Кожа разрезана"),
    (state::SKIN_OPEN, "Кожа раскрыта"),
    (state::VESSELS_UNCLAMPED, "Сосуды не зажаты"),
    (state::VESSELS_CLAMPED, "Сосуды зажаты"),
    (state::ORGANS_CUT, "Органы разрезаны"),
    (state::BONE_DRILLED, "Кость просверлена"),
    (state::BONE_SAWED, "Кость распилена"),
    (state::CAVITY_WIDENED, "Грудная полость широко раскрыта"),
];

/// `bitfield_to_list(states, SURGERY_STATE_READABLE)`.
pub fn readable_states(states: u16) -> Vec<&'static str> {
    STATE_READABLE
        .iter()
        .filter(|(bit, _)| states & bit != 0)
        .map(|(_, text)| *text)
        .collect()
}

/// `bitfield_to_list(states, SURGERY_STATE_GUIDES(must))`.
fn state_guides(states: u16, must: &str) -> Vec<String> {
    [
        (state::SKIN_CUT, format!("{must} разрез кожи")),
        (
            state::SKIN_OPEN,
            format!("{must}, чтобы кожа была раскрыта"),
        ),
        (
            state::VESSELS_UNCLAMPED,
            format!("{must}, чтобы сосуды были разжаты"),
        ),
        (
            state::VESSELS_CLAMPED,
            format!("{must}, чтобы сосуды были зажаты"),
        ),
        (
            state::ORGANS_CUT,
            format!("{must}, чтобы орган был разрезан"),
        ),
        (
            state::BONE_DRILLED,
            format!("{must}, чтобы кость была просверлена"),
        ),
        (
            state::BONE_SAWED,
            format!("{must}, чтобы кость была распилена"),
        ),
        (
            state::CAVITY_WIDENED,
            format!("{must}, чтобы грудная полость была раскрыта"),
        ),
    ]
    .into_iter()
    .filter(|(bit, _)| states & bit != 0)
    .map(|(_, text)| text)
    .collect()
}

/// tg `is_replaced`: an operation hidden because what replaces it (directly or down the chain)
/// is offered too.
pub fn is_replaced(op: Op, pool: &[&str]) -> bool {
    let mut next = op.def().replaced_by;
    while let Some(path) = next {
        if pool.contains(&path) {
            return true;
        }
        next = Op::of_path(path).and_then(|o| o.def().replaced_by);
    }
    false
}

/// The operations a surgeon may pick from: tg `GLOB.operations.unlocked` plus `extra` (the
/// operating computer's list), without the replaced ones.
pub fn operation_pool(extra: &[&'static str]) -> Vec<Op> {
    let mut paths: Vec<&str> = Op::types()
        .into_iter()
        .filter(|op| op.def().flags & flags::LOCKED == 0)
        .map(|op| op.def().path)
        .collect();
    for path in extra {
        if !paths.contains(path) {
            paths.push(path);
        }
    }
    Op::all()
        .into_iter()
        .filter(|op| paths.contains(&op.def().path) && !is_replaced(*op, &paths))
        .collect()
}

/// The tool as an operation sees it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolInfo {
    /// `None`: the empty hand.
    pub path: Option<String>,
    pub behaviour: String,
    pub toolspeed: f32,
    pub sharpness: u32,
    pub force: f32,
    /// `get_temperature()`.
    pub temperature: f32,
    /// Revival's `tool_check`: paddles of a powered unit, wielded and off cooldown; an active
    /// stun baton.
    pub shock_ready: bool,
}

impl ToolInfo {
    pub fn read(world: &World, item: Option<Entity>) -> Self {
        let Some(item) = item else {
            return Self::default();
        };
        let tool = world.get::<DmTool>(item);
        let lit = world.get::<Welder>(item).is_none_or(|w| w.on);
        Self {
            path: world.get::<DmItem>(item).map(|i| i.path.clone()),
            behaviour: tool.map(|t| t.behaviour.clone()).unwrap_or_default(),
            toolspeed: tool.map_or(1.0, |t| t.speed),
            sharpness: world.get::<DmWounding>(item).map_or(0, |w| w.sharpness),
            force: world.get::<DmItem>(item).map_or(0.0, |i| i.force),
            temperature: if lit {
                tool.map_or(0.0, |t| t.heat)
            } else {
                0.0
            },
            shock_ready: shock_ready(world, item),
        }
    }

    fn under(&self, root: &str) -> bool {
        self.path.as_deref().is_some_and(|p| is_under(p, root))
    }
}

fn shock_ready(world: &World, item: Entity) -> bool {
    if let Some(super::defib::Paddles(unit)) = world.get::<super::defib::Paddles>(item) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let wielded = world
            .get::<crate::inventory::CarriedBy>(item)
            .is_some_and(|c| super::defib::is_wielded(world, c.0));
        return wielded
            && world
                .get::<super::defib::Defib>(*unit)
                .is_some_and(|d| d.powered() && now >= d.cooldown_until);
    }
    world
        .get::<crate::security::Baton>(item)
        .is_some_and(|b| b.active)
}

/// tg `tool_check` overrides.
fn tool_check(op: Op, tool: &ToolInfo) -> bool {
    let def = op.def();
    let listed = def
        .implements
        .iter()
        .any(|(i, _)| *i == Tool(leak(&tool.behaviour)));
    match op {
        Op::InciseSkin => {
            tool.sharpness & super::wounds::SHARP_EDGED != 0
                || listed
                || tool.under("/obj/item/pen")
                || tool.under("/obj/item/screwdriver")
        }
        Op::SawBones => {
            (tool.sharpness & super::wounds::SHARP_EDGED != 0 && tool.force >= 10.0) || listed
        }
        Op::DrillBones => {
            (tool.sharpness & super::wounds::SHARP_POINTY != 0 && tool.force >= 10.0) || listed
        }
        // Saws would rip the organs up.
        Op::InciseOrgans => {
            (tool.sharpness & super::wounds::SHARP_EDGED != 0 || listed) && tool.behaviour != "saw"
        }
        Op::CloseSkin => {
            tool.under("/obj/item/stack/medical/suture")
                || tool.temperature >= FIRE_MINIMUM_TEMPERATURE_TO_EXIST
        }
        Op::SealVeins => tool.temperature >= FIRE_MINIMUM_TEMPERATURE_TO_EXIST,
        Op::Gastrectomy | Op::Lobotomy => {
            tool.sharpness & super::wounds::SHARP_EDGED != 0 || listed
        }
        // shortcut: an energy gun never counts as charged with an electrode (no fire modes
        // here), so only paddles and batons shock a brain.
        Op::Revival => {
            !tool.under("/obj/item/gun/energy")
                && (tool.shock_ready || tool.under("/obj/item/melee/touch_attack/shock"))
        }
        _ => true,
    }
}

/// A `&'static str` view for comparing with [`Implement::Tool`] (only the listed behaviours
/// can match, so unknown ones become "").
fn leak(behaviour: &str) -> &'static str {
    const KNOWN: [&str; 12] = [
        "scalpel",
        "hemostat",
        "retractor",
        "cautery",
        "saw",
        "drill",
        "bonesetter",
        "screwdriver",
        "cutters",
        "crowbar",
        "welder",
        "knife",
    ];
    KNOWN.into_iter().find(|k| *k == behaviour).unwrap_or("")
}

/// The `implements` entry `tool` matches (its behaviour first, else its type), unscaled.
fn listed_quality(op: Op, tool: &ToolInfo) -> Option<f32> {
    let implements = op.def().implements;
    let Some(path) = &tool.path else {
        return implements.iter().find(|(i, _)| *i == Hand).map(|(_, q)| *q);
    };
    let by_tool = implements
        .iter()
        .find(|(i, _)| !tool.behaviour.is_empty() && *i == Tool(leak(&tool.behaviour)))
        .map(|(_, q)| *q);
    let by_path = implements
        .iter()
        .find(|(i, _)| matches!(i, Path(root) if is_under(path, root)))
        .map(|(_, q)| *q);
    by_tool.or(by_path)
}

/// tg `get_tool_quality`: 0 when the tool cannot do the operation.
pub fn tool_quality(op: Op, tool: &ToolInfo) -> f32 {
    if tool.path.is_none() {
        return listed_quality(op, tool).unwrap_or(0.0);
    }
    if !tool_check(op, tool) {
        return 0.0;
    }
    tool.toolspeed * listed_quality(op, tool).unwrap_or(0.0)
}

/// tg `get_location_modifier`: the best surface on the patient's tile.
pub fn location_modifier(paths: &[String]) -> f32 {
    const MODS: [(&str, f32); 6] = [
        ("/obj/structure/table/optable/abductor", 0.85),
        ("/obj/structure/table/optable", 1.0),
        ("/obj/machinery/stasis", 1.15),
        ("/obj/structure/table", 1.25),
        ("/obj/structure/bed", 1.5),
        ("/obj/structure/kitchenspike", 1.5),
    ];
    paths
        .iter()
        .filter_map(|p| {
            MODS.iter()
                .find(|(root, _)| is_under(p, root))
                .map(|(_, m)| *m)
        })
        .fold(2.0, f32::min)
}

/// tg `get_time_modifiers`: tool quality × surgeon × surface × patient, rounded to 0.01.
pub fn speed_modifier(quality: f32, surgeon: f32, location: f32, patient: f32) -> f32 {
    let total = if quality > 0.0 { quality } else { 1.0 } * surgeon * location * patient;
    (total * 100.0).round() / 100.0
}

/// `get_mob_surgery_speed_mod`: ×0.8 on a patient who feels nothing (BandaStation: analgesia or
/// `stat >= UNCONSCIOUS`).
pub fn patient_speed_mod(painless: bool) -> f32 {
    if painless { 0.8 } else { 1.0 }
}

/// `get_surgeon_surgery_speed_mod`: drunkenness slows the surgeon, except at the Ballmer peak.
pub fn surgeon_speed_mod(drunk: f32) -> f32 {
    if (BALLMER_PEAK_LOW_END..=BALLMER_PEAK_HIGH_END).contains(&drunk) {
        0.8
    } else {
        1.0 + (drunk.powf(1.5) / 90.0 * 10.0).round() / 10.0
    }
}

/// `GET_FAILURE_CHANCE(base_time, speed_mod)`, time in deciseconds.
pub fn base_failure_chance(time: f32, speed: f32) -> f32 {
    FAILURE_CHANCE_PER_SECOND * ((speed * time - SURGERY_MODIFIER_FAILURE_THRESHOLD * time) / 10.0)
}

/// BandaStation's tool grade: an improvised implement (listed above 1.15) +10, else by the type
/// path: advanced 0, alien 1, augment 0, anything else 5.
pub fn tool_mod(op: Op, tool: &ToolInfo) -> f32 {
    // tg indexes `implements` with the tool object, which is never a key, so the improvised
    // grade never applied there; the listed quality of the matched implement is what it meant.
    if listed_quality(op, tool).is_some_and(|q| q > 1.15) {
        return TOOL_MOD_GHETTO;
    }
    let Some(path) = tool.path.as_deref() else {
        return 0.0;
    };
    if path.contains("/advanced") {
        TOOL_MOD_ADVANCED
    } else if path.contains("/alien") {
        TOOL_MOD_ALIEN
    } else if path.contains("/augment") {
        TOOL_MOD_AUGMENT
    } else {
        TOOL_MOD_BASIC
    }
}

/// BandaStation `get_modified_failure_chance`: the base chance (clamped), +40 on a patient who
/// feels it, darkness (`lumcount` below 0.6), the tool's grade.
pub fn failure_chance(
    time: f32,
    speed: f32,
    painless: bool,
    lumcount: Option<f32>,
    tool_mod: f32,
) -> f32 {
    let mut chance = base_failure_chance(time, speed).clamp(0.0, 99.0);
    if !painless {
        chance += 40.0;
    }
    if let Some(light) = lumcount
        && light < 0.6
    {
        chance += (1.0 - light / 0.6).powf(1.5) * 60.0;
    }
    chance + tool_mod
}

/// Reagents with `TRAIT_ANALGESIA` in `metabolized_traits` (BandaStation adds tramadol).
const ANALGESICS: [&str; 12] = [
    "/datum/reagent/medicine/adminordrazine",
    "/datum/reagent/medicine/mine_salve",
    "/datum/reagent/medicine/morphine",
    "/datum/reagent/medicine/stimulants",
    "/datum/reagent/medicine/muscle_stimulant",
    "/datum/reagent/medicine/granibitaluri",
    "/datum/reagent/medicine/tramadol",
    "/datum/reagent/toxin/berserker",
    "/datum/reagent/drug/bath_salts",
    "/datum/reagent/drug/happiness",
    "/datum/reagent/drug/pumpup",
    "/datum/reagent/drug/maint/sludge",
];

/// `HAS_TRAIT(mob, TRAIT_ANALGESIA)` from what is metabolizing in its blood.
pub fn analgesia(world: &World, mob: Entity) -> bool {
    let (Some(reagents), Some(chem)) = (
        world.get::<crate::chem::MobReagents>(mob),
        world.get_resource::<crate::chem::Chem>(),
    ) else {
        return false;
    };
    reagents
        .blood
        .reagents
        .iter()
        .any(|r| r.metabolizing && ANALGESICS.contains(&chem.0.def(r.id).path.as_str()))
}

fn drunkenness(world: &World, mob: Entity) -> f32 {
    world
        .get::<crate::chem::MobReagents>(mob)
        .map_or(0.0, |r| r.drunk)
}

/// Mob-level facts operations check.
#[derive(Clone, Debug)]
struct Patient {
    lying: bool,
    unconscious: bool,
    /// Analgesia or unconscious (BandaStation `stat >= UNCONSCIOUS`).
    painless: bool,
    dead: bool,
    covered: u32,
    prepped_any: bool,
    brute: f32,
    burn: f32,
}

fn patient(world: &World, mob: Entity) -> Option<Patient> {
    let health = world.get::<Health>(mob)?;
    let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    let limbs = world.get::<Limbs>(mob)?;
    let covered = world.get::<Inventory>(mob).map_or(0, |inv| {
        inv.equipped()
            .filter(|&item| inv.slot_of(item) != Some(InventorySlot::Neck))
            .filter_map(|item| world.get::<ss13_mapload::DmClothing>(item))
            .fold(0, |bits, c| bits | c.body_parts_covered)
    });
    let unconscious = health.knocked_out(stat);
    Some(Patient {
        lying: world.get::<BodyPosition>(mob).is_some_and(|b| b.lying()),
        unconscious,
        painless: unconscious || analgesia(world, mob),
        dead: stat == Stat::Dead,
        covered,
        prepped_any: limbs.parts.iter().any(|l| l.prepped),
        brute: health.brute,
        burn: health.burn,
    })
}

/// tg `is_available`: the target's state allows `op` on `zone` (no tool, posture or surgeon
/// checks; what the operating computer shows as a next step).
pub(crate) fn is_available(world: &World, op: Op, mob: Entity, zone: TargetZone) -> bool {
    let (Some(p), Some(limbs)) = (patient(world, mob), world.get::<Limbs>(mob)) else {
        return false;
    };
    let def = op.def();
    let body = match def.target {
        // `basic`: the exact zone, a prepared patient and bodypart.
        Target::Basic(target) => {
            if zone != target || !p.prepped_any {
                return false;
            }
            target.check_zone()
        }
        // `organ`: the organ's exact zone, its bodypart prepared; organic (`required_organ_flag`).
        Target::Organ(slot) => {
            let organic = organs::state(world, mob, slot).is_some_and(|o| !o.robotic);
            if zone != slot.spec().zone || !organic {
                return false;
            }
            slot.bodypart()
        }
        Target::Limb => zone.check_zone(),
    };
    let limb = limbs.limb(body);
    if !limb.prepped {
        return false;
    }
    // A stump takes only operations with `allow_stumps`.
    if limb.missing != (op == Op::AttachLimb) {
        return false;
    }
    if def.all_required != 0 && !limb.has_state(def.all_required) {
        return false;
    }
    if def.any_required != 0 && !limb.has_any_state(def.any_required) {
        return false;
    }
    if def.any_blocked != 0 && limb.has_any_state(def.any_blocked) {
        return false;
    }
    let organ = organ_target(op).and_then(|slot| organs::state(world, mob, slot));
    let damage = organ.map_or(0.0, |o| o.damage);
    let max = organ.map_or(0.0, |o| o.spec().max_health);
    // `organ/repair/state_check`: moderately damaged and not repaired before.
    let repairable = |heal_to: f32| organ.is_some_and(|o| !o.operated) && damage >= max * heal_to;
    let bones_intact = !limb.has_any_state(state::BONE_SAWED | state::BONE_DRILLED);
    let state_ok = match op {
        Op::TendWounds(_, Heal::Brute) => p.brute > 0.0,
        Op::TendWounds(_, Heal::Burn) => p.burn > 0.0,
        Op::TendWounds(_, Heal::Combo) => p.brute > 0.0 || p.burn > 0.0,
        // A wound's own broken-bone state does not offer the fix (tg avoids the confusion).
        Op::FixBones => !limb
            .wounds
            .iter()
            .any(|w| w.surgery_states & state::ALL_BONE != 0),
        Op::RepairDislocation => limb
            .wound(Series::Bone)
            .is_some_and(|w| w.kind == WoundKind::Dislocation),
        Op::RepairHairline => limb
            .wound(Series::Bone)
            .is_some_and(|w| w.kind == WoundKind::Hairline),
        Op::ResetCompound => limb
            .wound(Series::Bone)
            .is_some_and(|w| w.kind == WoundKind::Compound && !w.reset),
        Op::RepairCompound => limb
            .wound(Series::Bone)
            .is_some_and(|w| w.kind == WoundKind::Compound && w.reset),
        // shortcut: no decapitation (heads with their organs are not items).
        Op::Amputate => !matches!(body, BodyZone::Head | BodyZone::Chest),
        Op::RemoveOrgan(slot) => {
            zone == slot.spec().zone
                && organs::organ(world, mob, slot).is_some()
                && organ_bone_ok(slot, limb)
        }
        Op::InsertOrgan(slot) => {
            zone == slot.spec().zone && organs::lacks(world, mob, slot) && organ_bone_ok(slot, limb)
        }
        Op::RemoveImplant(slot) => {
            zone == slot.zone()
                && cyberimps::implant_in(world, mob, slot).is_some()
                && implant_bone_ok(zone, limb)
        }
        // The held implant's slot is checked with the tool ([`available`]).
        Op::InsertImplant => cyberimps::takes_implants(zone) && implant_bone_ok(zone, limb),
        // shortcut: no suicide, husk or DNR flags yet, so every corpse with a brain qualifies.
        Op::Revival => p.dead,
        Op::CoronaryBypass | Op::Lobectomy => repairable(REPAIR_HEAL_TO),
        Op::Hepatectomy => repairable(HEPATECTOMY_HEAL_TO),
        Op::Gastrectomy => repairable(GASTRECTOMY_HEAL_TO),
        Op::EarSurgery | Op::EyeSurgery => bones_intact,
        Op::Debride => limb.wound(Series::Burn).is_some_and(|w| w.infection > 0.0),
        // shortcut: no husks yet.
        Op::FilterBlood => body == BodyZone::Chest,
        // `bioware/state_check`: its zone, no bioware of the same kind yet.
        Op::Bioware(kind) => {
            body == kind.zone().check_zone() && super::bioware::can_gain(world, mob, kind)
        }
        Op::RepairPuncture => limb
            .wound(Series::Pierce)
            .is_some_and(|w| w.blood_flow > 0.0 && !w.mend_state),
        Op::SealVeins => limb
            .wound(Series::Pierce)
            .is_some_and(|w| w.blood_flow > 0.0 && w.mend_state),
        _ => true,
    };
    if !state_ok {
        return false;
    }
    def.flags & flags::IGNORE_CLOTHES != 0 || p.covered & crate::chem::body_part_bit(body) == 0
}

/// `organ_manipulation/internal/organ_check`: chest organs and the brain need the bone sawed.
fn organ_bone_ok(slot: Slot, limb: &super::wounds::Limb) -> bool {
    let locked = slot.spec().zone == TargetZone::Chest || slot == Slot::Brain;
    !locked || limb.has_state(state::BONE_SAWED)
}

/// `organ_check` of an implant: the ones in the chest need the bone sawed.
fn implant_bone_ok(zone: TargetZone, limb: &super::wounds::Limb) -> bool {
    zone != TargetZone::Chest || limb.has_state(state::BONE_SAWED)
}

/// tg `snowflake_check_availability` with the tool in hand: an organ goes into its own slot, a
/// limb onto its own stump.
fn tool_fits(op: Op, zone: TargetZone, tool: &ToolInfo) -> bool {
    let path = tool.path.as_deref().unwrap_or_default();
    match op {
        Op::InsertOrgan(slot) => Slot::of_path(path) == Some(slot),
        Op::AttachLimb => super::bodyparts::zone_of_path(path) == Some(zone.check_zone()),
        _ => true,
    }
}

/// tg `check_availability` of `op` on `mob`'s `zone` with `tool`, by `surgeon`.
fn available(
    world: &World,
    op: Op,
    mob: Entity,
    surgeon: Entity,
    zone: TargetZone,
    tool: &ToolInfo,
) -> bool {
    if tool_quality(op, tool) <= 0.0
        || !tool_fits(op, zone, tool)
        || !is_available(world, op, mob, zone)
    {
        return false;
    }
    // `is_insert_available` of an implant: a slot of the zone, empty.
    if op == Op::InsertImplant
        && !cyberimps::slot_for(tool.path.as_deref().unwrap_or_default(), zone)
            .is_some_and(|slot| cyberimps::implant_in(world, mob, slot).is_none())
    {
        return false;
    }
    let def = op.def();
    if def.flags & flags::STANDING_ALLOWED == 0 && !patient(world, mob).is_some_and(|p| p.lying) {
        return false;
    }
    def.flags & flags::SELF_OPERABLE != 0 || mob != surgeon
}

/// tg `get_available_operations` over `pool` ([`operation_pool`]), sorted by name
/// (`cmp_name_asc`).
pub fn available_ops(
    world: &World,
    pool: &[Op],
    mob: Entity,
    surgeon: Entity,
    zone: TargetZone,
    tool: &ToolInfo,
) -> Vec<Op> {
    let mut ops: Vec<Op> = pool
        .iter()
        .copied()
        .filter(|&op| available(world, op, mob, surgeon, zone, tool))
        .collect();
    ops.sort_by_key(|op| op.def().name.to_lowercase());
    ops
}

/// The operating table `mob` lies on and the operating computer next to it (tg
/// `table.patient`, `table.computer`: the first computer around the table in `alldirs` order).
pub fn table_and_computer(world: &World, mob: Entity) -> Option<(Entity, Option<Entity>)> {
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let pos = *world.get::<TilePos>(mob)?;
    if !lying {
        return None;
    }
    let index = world.resource::<ss13_world::SpatialIndex>();
    let table = index.on_tile(pos).iter().copied().find(|&e| {
        crate::construction::path_of(world, e)
            .is_some_and(|p| is_under(&p, "/obj/structure/table/optable"))
    })?;
    Some((table, computer_near(world, pos)))
}

/// tg `alldirs`.
pub const ALLDIRS: [(i32, i32); 8] = [
    (0, 1),
    (0, -1),
    (1, 0),
    (-1, 0),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, -1),
];

/// The first operating computer around `pos` (`locate() in get_step(src, direction)`).
pub fn computer_near(world: &World, pos: TilePos) -> Option<Entity> {
    let index = world.resource::<ss13_world::SpatialIndex>();
    ALLDIRS.iter().find_map(|&(dx, dy)| {
        let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
        index
            .on_tile(tile)
            .iter()
            .copied()
            .find(|&e| world.get::<OperatingComputer>(e).is_some())
    })
}

/// The operations offered on `mob`: tg `unlocked` plus, for a patient lying on an operating
/// table, its computer's `advanced_surgeries` (`get_surgeries`, no power check).
pub fn pool_for(world: &World, mob: Entity) -> Vec<Op> {
    let extra = table_and_computer(world, mob)
        .and_then(|(_, computer)| computer)
        .map(|c| advanced_surgeries(world, c))
        .unwrap_or_default();
    operation_pool(&extra)
}

/// A list of operations waiting for the surgeon's pick (tg's radial menu).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SurgeryChoice {
    pub patient: Entity,
    pub tool: Option<Entity>,
    pub zone: TargetZone,
    pub ops: Vec<Op>,
}

/// The server side of `ListInputWindow` for the operation pick.
pub struct SurgeryListWindow;

impl UiInterface for SurgeryListWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

/// `submit` (`entry`) or `cancel`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListAct {
    Submit(String),
    Cancel,
}

impl ServerUi for SurgeryListWindow {
    type Source = &'static SurgeryChoice;
    type UserParam = ();
    type Act = ListAct;

    fn static_data(_: &SurgeryChoice) {}

    fn data(choice: &SurgeryChoice, _: Entity, _: &()) -> ListInputData {
        let items: Vec<String> = choice
            .ops
            .iter()
            .map(|op| op.def().name.to_owned())
            .collect();
        ListInputData {
            title: "Операция".into(),
            message: String::new(),
            init_value: items.first().cloned().unwrap_or_default(),
            items,
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ListAct, UiReject> {
        match action {
            "submit" => params
                .get("entry")
                .map(|e| ListAct::Submit(e.to_owned()))
                .ok_or_else(|| UiReject::BadParams("entry: expected an item".into())),
            "cancel" => Ok(ListAct::Cancel),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `perform_surgery` from a click of `surgeon` (holding `tool`) on `mob`. True when the click
/// was the surgery's (an operation started, a list opened, or a surgical tool was stopped from
/// stabbing the patient).
pub fn perform_surgery(world: &mut World, actor: Actor, mob: Entity) -> bool {
    if world.get::<DoAfter>(actor.mob).is_some() {
        tell(
            world,
            actor.client,
            Span::Warning,
            "операция уже проводится!",
        );
        return true;
    }
    let zone = world
        .get::<ZoneSelected>(actor.mob)
        .copied()
        .unwrap_or_default()
        .0;
    let tool = ToolInfo::read(world, actor.item);
    let pool = pool_for(world, mob);
    let ops = available_ops(world, &pool, mob, actor.mob, zone, &tool);
    match ops.as_slice() {
        [] => {
            let Some(item) = actor.item else {
                return false;
            };
            if !world
                .get::<DmWounding>(item)
                .is_some_and(|w| w.surgical_tool)
            {
                return false;
            }
            let Some(p) = patient(world, mob) else {
                return false;
            };
            if !world
                .get::<Limbs>(mob)
                .is_some_and(|l| l.limb(zone.check_zone()).prepped)
            {
                return false;
            }
            let text = if p.covered & crate::chem::body_part_bit(zone.check_zone()) != 0 {
                "операционная зона перекрыта!"
            } else if !p.lying {
                "не лежит!"
            } else {
                "ничего не сделать с данным инструментом!"
            };
            tell(world, actor.client, Span::Warning, text);
            true
        }
        [op] => {
            try_perform(world, actor, mob, *op, zone);
            true
        }
        _ => {
            world.entity_mut(actor.mob).insert(SurgeryChoice {
                patient: mob,
                tool: actor.item,
                zone,
                ops,
            });
            world.write_message(OpenUi::new::<SurgeryListWindow>(
                actor.client,
                actor.mob,
                actor.mob,
                "Операция",
            ));
            true
        }
    }
}

/// `op` is still offered to `surgeon` (the pool may have changed: the patient left the table)
/// and available: tg `operate_check`.
fn offered(
    world: &World,
    op: Op,
    mob: Entity,
    surgeon: Entity,
    zone: TargetZone,
    tool: &ToolInfo,
) -> bool {
    pool_for(world, mob).contains(&op) && available(world, op, mob, surgeon, zone, tool)
}

/// tg `try_perform` up to its `do_after`.
fn try_perform(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone) {
    let tool = ToolInfo::read(world, actor.item);
    if !offered(world, op, mob, actor.mob, zone, &tool) {
        return;
    }
    preop(world, actor, mob, op, zone);
    let delay = op.def().time
        * speed(world, op, mob, actor.mob, &tool).min(SURGERY_MODIFIER_FAILURE_THRESHOLD);
    do_after(
        world,
        actor,
        mob,
        delay,
        0,
        crate::construction::Job::Medical(MedJob::Surgery { op, zone }),
    );
}

fn speed(world: &World, op: Op, mob: Entity, surgeon: Entity, tool: &ToolInfo) -> f32 {
    let location = world.get::<TilePos>(mob).map_or(2.0, |&pos| {
        let paths: Vec<String> = world
            .resource::<ss13_world::SpatialIndex>()
            .on_tile(pos)
            .iter()
            .filter_map(|&e| crate::construction::path_of(world, e))
            .collect();
        location_modifier(&paths)
    });
    let painless = patient(world, mob).is_some_and(|p| p.painless);
    speed_modifier(
        tool_quality(op, tool),
        surgeon_speed_mod(drunkenness(world, surgeon)),
        location,
        patient_speed_mod(painless),
    )
}

fn zone_words(op: Op, zone: TargetZone) -> String {
    match op {
        Op::Amputate => zone.check_zone().ru().1.into(),
        _ => super::wounds::zone_prepositional(zone.check_zone()).into(),
    }
}

fn item_name(world: &World, item: Option<Entity>) -> String {
    item.map(|i| name_of(world, i)).unwrap_or_default()
}

/// tend wounds' word for what it treats.
fn wound_words(heal: Heal) -> &'static str {
    match heal {
        Heal::Combo => "раны",
        Heal::Brute => "ушибы",
        Heal::Burn => "ожоги",
    }
}

/// tg `on_preop` (`display_results` reaches only adjacent mobs) and `display_pain`.
fn preop(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone) {
    let (user, target) = (name_of(world, actor.mob), name_of(world, mob));
    let tool = item_name(world, actor.item);
    let place = zone_words(op, zone);
    let (own, others, hurt) = match op {
        Op::TendWounds(_, heal) => {
            let what = wound_words(heal);
            (
                format!("Вы пытаетесь наложить швы на {what} у {target}."),
                format!("{user} пытается наложить швы на {what} у {target}."),
                format!("Ваши {what} адски болят!"),
            )
        }
        Op::Revival => (
            format!("Вы готовитесь дать мозгу {target} искру жизни с помощью {tool}."),
            format!("{user} готовится дать мозгу {target} искру жизни с помощью {tool}."),
            String::new(),
        ),
        Op::CoronaryBypass => (
            format!("Вы начинаете устанавливать шунт на сердце {target}..."),
            format!("{user} начинает устанавливать шунт на сердце {target}."),
            "Боль в груди невыносима! Вы едва можете это терпеть!".into(),
        ),
        Op::BrainSurgery => (
            format!("Вы начинаете лечить мозг {target}..."),
            format!("{user} начинает лечить мозг {target}."),
            "Ваша голова пульсирует от невообразимой боли!".into(),
        ),
        Op::Lobotomy => (
            format!("Вы начинаете проводить лоботомию мозга {target}..."),
            format!("{user} начинает проводить лоботомию мозга {target}."),
            "Ваша голова пульсирует от невообразимой боли!".into(),
        ),
        Op::Pacify => (
            format!("Вы начинаете пацификацию {target}..."),
            format!("{user} начинает оперировать мозг {target}."),
            "Ваша голова разрывается от невообразимой боли!".into(),
        ),
        Op::RemoveOrgan(slot) => {
            let organ = slot.spec().ru.0;
            (
                format!("Вы начинаете извлекать {organ} у {target}..."),
                format!("{user} начинает извлекать {organ} у {target}."),
                format!("Вы чувствуете тянущее ощущение в {place}!"),
            )
        }
        Op::RemoveImplant(slot) => {
            let implant = cyberimps::implant_in(world, mob, slot)
                .map(|i| name_of(world, i))
                .unwrap_or_default();
            (
                format!("Вы начинаете извлекать {implant} у {target}..."),
                format!("{user} начинает извлекать {implant} у {target}."),
                format!("Вы чувствуете тянущее ощущение в {place}!"),
            )
        }
        Op::InsertOrgan(_) | Op::InsertImplant => (
            format!("Вы начинаете вставлять {tool} в {target}..."),
            format!("{user} начинает вставлять {tool} в {target}."),
            format!(
                "Вы чувствуете, как что-то помещают в вашу {}!",
                zone.check_zone().ru().1
            ),
        ),
        Op::AttachLimb => (
            format!(
                "Вы начинаете заменять отсутствующую {} у {target}...",
                zone.check_zone().ru().1
            ),
            format!(
                "{user} начинает заменять отсутствующую {} у {target}.",
                zone.check_zone().ru().1
            ),
            format!(
                "Вы испытываете неприятное ощущение там, где должна быть ваша {}!",
                zone.check_zone().ru().0
            ),
        ),
        Op::Lobectomy => (
            format!("Вы начинаете делать надрез в легких {target}..."),
            format!("{user} начинает делать надрез у {target}."),
            "Вы чувствуете колющую боль в груди!".into(),
        ),
        Op::Hepatectomy => (
            format!("Вы начинаете вырезать поврежденный фрагмент печени {target}..."),
            format!("{user} начинает делать надрез у {target}."),
            "Ваш живот горит от ужасной колющей боли!".into(),
        ),
        Op::Gastrectomy => (
            format!("Вы начинаете вырезать поврежденный фрагмент желудка {target}..."),
            format!("{user} начинает делать надрез у {target}."),
            "Вы чувствуете ужасную колющую боль в животе!".into(),
        ),
        Op::EarSurgery => (
            format!("Вы начинаете лечить уши {target}..."),
            format!("{user} начинает лечить уши {target}."),
            "Вы чувствуете головокружительную боль в голове!".into(),
        ),
        Op::EyeSurgery => (
            format!("Вы начинаете лечить глаза {target}..."),
            format!("{user} начинает лечить глаза {target}."),
            "Вы чувствуете колющую боль в глазах!".into(),
        ),
        Op::Debride => (
            format!("Вы начинаете удалять зараженную плоть с {place} у {target}..."),
            format!(
                "{user} начинает удалять зараженную плоть с {place} у {target} с помощью {tool}."
            ),
            format!(
                "Инфекция в вашей {place} приносит адскую боль! Такое чувство, что вас режут \
                 ножом!"
            ),
        ),
        Op::RepairPuncture => (
            format!(
                "Вы начинаете соединять разорванные кровеносные сосуды в {place} у {target}..."
            ),
            format!(
                "{user} начинает соединять разорванные кровеносные сосуды в {place} у {target} с \
                 помощью {tool}."
            ),
            format!("Вы чувствуете ужасную колющую боль в вашей {place}!"),
        ),
        Op::SealVeins => (
            format!(
                "Вы начинаете прижигать соединенные кровеносные сосуды в {place} у {target}..."
            ),
            format!(
                "{user} начинает прижигать соединенные кровеносные сосуды в {place} у {target} с \
                 помощью {tool}."
            ),
            format!("Вы чувствуете жжение в вашей {place}!"),
        ),
        Op::Bioware(kind) => {
            let [own, others, _, _, hurt, _] = bioware_lines(kind, &target, &user);
            (own, others, hurt)
        }
        Op::FilterBlood => (
            format!("Вы начинаете оперировать {target}..."),
            format!("{user} начинает оперировать {target}."),
            "Вы чувствуете пульсирующую боль в груди!".into(),
        ),
        _ => {
            let verb = op.def().verb;
            (
                format!("Вы начинаете {verb} {place} у {target}..."),
                format!("{user} начинает {verb} {place} у {target}."),
                "Вы чувствуете острую боль, когда вас оперируют!".into(),
            )
        }
    };
    visible_range(world, actor.mob, &own, &others, Span::Notice, 1);
    pain(world, mob, &hurt);
}

/// tg `display_pain`: nothing for the unconscious; a dull tingle under analgesia, and maybe for
/// a drunk patient (its drunkenness as a percent, at most 90).
fn pain(world: &mut World, mob: Entity, text: &str) {
    if text.is_empty() {
        return;
    }
    let Some(p) = patient(world, mob) else {
        return;
    };
    if p.unconscious {
        return;
    }
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    let drunk = drunkenness(world, mob);
    let dulled =
        p.painless || (drunk > 0.0 && super::fork_rng(world).chance(drunk.clamp(0.0, 90.0)));
    if dulled {
        tell(
            world,
            client,
            Span::Notice,
            "Вы испытываете притупленное покалывание, пока ваше тело оперируют.",
        );
    } else {
        tell(world, client, Span::BoldWarning, text);
    }
}

/// The surgeon picked an entry of the list (or closed it).
fn pick_operation(world: &mut World, mut acts: Local<MessageCursor<UiActed<SurgeryListWindow>>>) {
    let picked: Vec<(Entity, Entity, ListAct)> = acts
        .read(world.resource::<Messages<UiActed<SurgeryListWindow>>>())
        .filter(|a| a.actor == a.source)
        .map(|a| (a.client, a.actor, a.act.clone()))
        .collect();
    for (client, surgeon, act) in picked {
        let Some(choice) = world.get::<SurgeryChoice>(surgeon).cloned() else {
            continue;
        };
        world.entity_mut(surgeon).remove::<SurgeryChoice>();
        let ListAct::Submit(entry) = act else {
            continue;
        };
        // tg `show_radial_menu` `custom_check`: the same tool, still next to the patient.
        let held = world
            .get::<Inventory>(surgeon)
            .and_then(Inventory::active_item);
        let near = match (
            world.get::<TilePos>(surgeon),
            world.get::<TilePos>(choice.patient),
        ) {
            (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
            _ => false,
        };
        let Some(&op) = choice.ops.iter().find(|op| op.def().name == entry) else {
            continue;
        };
        // tg `can_perform_action` of the radial's check: a surgeon knocked out or stunned
        // while the list was open picks nothing.
        let able = world.get::<Stat>(surgeon) == Some(&Stat::Conscious)
            && !crate::status_effects::incapacitation(world, surgeon).incapacitated;
        if held != choice.tool || !near || !able {
            continue;
        }
        let actor = Actor {
            client,
            mob: surgeon,
            item: held,
        };
        try_perform(world, actor, choice.patient, op, choice.zone);
    }
}

/// The list closes when the pick is gone (made, or the surgeon moved on).
fn close_stale_lists(
    mut windows: Query<&mut UiWindow, With<ss13_tgui::server::UiKind<SurgeryListWindow>>>,
    choices: Query<(), With<SurgeryChoice>>,
) {
    for mut window in &mut windows {
        if !choices.contains(window.source) {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// The end of an operation's `do_after`: `operate_check`, the failure roll, the effects, the
/// stack unit, the loop.
pub fn complete(world: &mut World, surgeon: Entity, action: &DoAfter, op: Op, zone: TargetZone) {
    let mob = action.target;
    let actor = Actor {
        client: action.client,
        mob: surgeon,
        item: action.item,
    };
    let tool = ToolInfo::read(world, actor.item);
    if !offered(world, op, mob, surgeon, zone, &tool) {
        return;
    }
    let speed = speed(world, op, mob, surgeon, &tool);
    let painless = patient(world, mob).is_some_and(|p| p.painless);
    let light = world
        .get::<TilePos>(mob)
        .and_then(|&p| crate::botany::lumcount(world, p));
    let chance = failure_chance(op.def().time, speed, painless, light, tool_mod(op, &tool));
    let failed = super::fork_rng(world).chance(chance.clamp(0.0, 99.0));
    if failed {
        failure(world, actor, mob, op, zone, speed);
    } else {
        success(world, actor, mob, op, zone);
    }
    // `isstack(tool)`: one unit per step.
    if let Some(item) = actor.item
        && world.get::<crate::tools::Stack>(item).is_some()
    {
        use_stack(world, item, 1);
    }
    refresh_patient(world, mob);
    // `OPERATION_LOOPING` while `can_loop`.
    let still_holding = actor
        .item
        .is_none_or(|i| world.get_entity(i).is_ok() && stack_amount(world, i) > 0);
    let tool = ToolInfo::read(world, actor.item);
    if op.def().flags & flags::LOOPING != 0
        && still_holding
        && can_loop(world, op, mob)
        && offered(world, op, mob, surgeon, zone, &tool)
    {
        let delay = op.def().time
            * self::speed(world, op, mob, surgeon, &tool).min(SURGERY_MODIFIER_FAILURE_THRESHOLD);
        do_after(
            world,
            actor,
            mob,
            delay,
            0,
            crate::construction::Job::Medical(MedJob::Surgery { op, zone }),
        );
    }
}

fn refresh_patient(world: &mut World, mob: Entity) {
    let Some(health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    if let Some(mut stat) = world.get_mut::<Stat>(mob) {
        let mut new = *stat;
        update_stat(&health, &mut new);
        stat.set_if_neq(new);
    }
}

fn with_limbs(world: &mut World, mob: Entity, f: impl FnOnce(&mut Limbs)) {
    if let Some(mut limbs) = world.get::<Limbs>(mob).cloned() {
        f(&mut limbs);
        world.entity_mut(mob).insert(limbs);
    }
}

/// `organ/repair/on_success`: the organ set to `heal_to` of its max health, marked operated on.
fn repair_organ(world: &mut World, mob: Entity, slot: Slot, heal_to: f32) {
    organs::set_organ_damage(world, mob, slot, slot.spec().max_health * heal_to);
    if let Some(organ) = organs::organ(world, mob, slot)
        && let Some(mut state) = world.get_mut::<organs::Organ>(organ)
    {
        state.operated = true;
        // `operation_organ_repair`: an EMPed organ works again.
        state.emped = false;
    }
}

/// An operation's own `can_loop`: blood filtering goes on while there is something to filter
/// (`has_filterable_chems`, the filter beeps when done).
fn can_loop(world: &mut World, op: Op, mob: Entity) -> bool {
    if op != Op::FilterBlood {
        return true;
    }
    let any = world
        .get::<crate::chem::MobReagents>(mob)
        .is_some_and(|r| !r.blood.reagents.is_empty());
    if !any {
        let target = name_of(world, mob);
        let text = format!(
            "Фильтр крови сигнализирует о том, что в крови у {target} не обнаружено химических \
             веществ."
        );
        super::visible(world, mob, &text, &text, Span::Notice);
    }
    any
}

/// `filter_blood/on_success`: each reagent in the blood loses 22% (0.4 to 10 units, rounded
/// to 0.2).
// shortcut: no whitelist (the filter's own UI), no chemical scan with an analyzer in hand.
fn filter_blood(world: &mut World, mob: Entity) {
    let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) else {
        return;
    };
    let amounts: Vec<_> = reagents
        .blood
        .reagents
        .iter()
        .map(|r| {
            (
                r.id,
                ((r.volume * 0.22 / 0.2).round() * 0.2).clamp(0.4, 10.0),
            )
        })
        .collect();
    for (id, amount) in amounts {
        reagents.blood.remove_reagent(id, amount);
    }
}

/// The new trauma of a lobotomy (75% on success, always on failure): a mild, severe or special
/// one that only magic cures.
fn lobotomy_trauma(world: &mut World, brain: Entity, rng: &mut crate::combat::CombatRng) {
    use super::traumas::{Category, Resilience, gain_type};
    let category = match rng.range(1, 3) {
        1 => Category::Mild,
        2 => Category::Severe,
        _ => Category::Special,
    };
    gain_type(world, brain, category, Some(Resilience::Magic), false, rng);
}

/// A failed brain surgery or pacification: a severe trauma that only a lobotomy cures.
fn severe_lobotomy_trauma(world: &mut World, mob: Entity) {
    use super::traumas::{Category, Resilience, gain_type};
    if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
        let mut rng = super::fork_rng(world);
        gain_type(
            world,
            brain,
            Category::Severe,
            Some(Resilience::Lobotomy),
            false,
            &mut rng,
        );
    }
}

/// `organ/repair/on_failure`: `failure_damage_percent` of the organ's max health.
fn fail_organ(world: &mut World, mob: Entity, slot: Slot, share: f32) {
    let max = slot.spec().max_health;
    organs::adjust_organ_loss(world, mob, slot, max * share);
}

fn damage_limb(world: &mut World, mob: Entity, zone: BodyZone, kind: DamageType, amount: f32) {
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.damage_zone(zone, kind, amount);
        world.entity_mut(mob).insert(health);
    }
}

fn heal_limb(world: &mut World, mob: Entity, zone: BodyZone, brute: f32) {
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.heal_zone(zone, brute, 0.0);
        world.entity_mut(mob).insert(health);
    }
}

/// tend wounds' `show_stats`: the surgeon holds a health analyzer, or an operating computer
/// next to the patient has it on its table.
fn shows_stats(world: &World, surgeon: Entity, mob: Entity) -> bool {
    let analyzer = world.get::<Inventory>(surgeon).is_some_and(|inv| {
        inv.hands.iter().flatten().any(|&i| {
            world
                .get::<DmItem>(i)
                .is_some_and(|d| is_under(&d.path, super::ANALYZER_ROOT))
        })
    });
    analyzer
        || table_and_computer(world, mob).is_some_and(|(_, computer)| {
            let near = |c: Entity| match (world.get::<TilePos>(c), world.get::<TilePos>(mob)) {
                (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
                _ => false,
            };
            computer.is_some_and(near)
        })
}

/// tend wounds' `get_progress`.
fn tend_progress(
    world: &World,
    surgeon: Entity,
    mob: Entity,
    heal: Heal,
    brute: f32,
    burn: f32,
) -> String {
    let Some(health) = world.get::<Health>(mob) else {
        return String::new();
    };
    let mut text = String::new();
    if shows_stats(world, surgeon, mob) {
        if brute > 0.0 && health.brute > 0.0 {
            text += &format!(". Оставшиеся раны: {}", health.brute.round());
        }
        if burn > 0.0 && health.burn > 0.0 {
            text += &format!(". Оставшиеся ожоги: {}", health.burn.round());
        }
        return text;
    }
    let mut steps: f32 = 0.0;
    if brute > 0.0 {
        steps = (health.brute / brute).max(0.0);
    }
    if burn > 0.0 {
        steps = steps.max(health.burn / burn);
    }
    let pick = |combo: &'static str, b: &'static str, f: &'static str| match heal {
        Heal::Combo => combo,
        Heal::Brute => b,
        Heal::Burn => f,
    };
    if steps <= 1.0 {
        String::new()
    } else if steps <= 3.0 {
        format!(
            ", заканчиваю работу над последними {}",
            pick("признаками повреждения", "ссадинами", "ожогами")
        )
    } else if steps <= 6.0 {
        format!(
            ", считаю последние {}",
            pick("очаги повреждений", "гематомы", "ожоговые пузыри")
        )
    } else if steps <= 9.0 {
        format!(
            ", продолжаю методично обрабатывать его обширные {}",
            pick("травмы", "разрывы", "ожоги")
        )
    } else if steps <= 12.0 {
        ", настраиваясь на долгую операцию впереди".into()
    } else if steps <= 15.0 {
        format!(
            ", хотя всё ещё больше напоминает {}, чем на человека",
            pick("мясное пюре", "говяжий фарш", "обугленный стейк")
        )
    } else {
        format!(
            ", хотя кажется, будто ваши усилия почти не сокращают объём {} тела",
            pick("искореженного", "размозжённого", "обугленного")
        )
    }
}

/// debride's `get_progress`.
fn debride_progress(infection: f32) -> &'static str {
    let steps = infection / DEBRIDE_INFECTION_REMOVED;
    if infection <= 0.0 || steps <= 1.0 {
        ""
    } else if steps <= 2.0 {
        ", подготовка к удалению последних остатков инфекции"
    } else if steps <= 4.0 {
        ", неуклонно сокращая количество оставшихся очагов инфекции"
    } else if steps >= 5.0 {
        ", хотя ещё довольно много подлежит удалению"
    } else {
        ""
    }
}

/// debride's `infestation_removed`, `sanitization_added`.
const DEBRIDE_INFECTION_REMOVED: f32 = 4.0;
const DEBRIDE_SANITIZATION_ADDED: f32 = 0.5;

/// tg `on_success` of each operation.
fn success(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone) {
    let body = zone.check_zone();
    let (user, target) = (name_of(world, actor.mob), name_of(world, mob));
    let tool = item_name(world, actor.item);
    let place = super::wounds::zone_prepositional(body);
    let mut own = "Вы преуспеваете.".to_owned();
    let mut others = format!("{user} преуспел!");
    match op {
        Op::InciseSkin => with_limbs(world, mob, |l| {
            l.limb_mut(body)
                .add_surgery(state::SKIN_CUT | state::VESSELS_UNCLAMPED);
        }),
        Op::RetractSkin => with_limbs(world, mob, |l| {
            let limb = l.limb_mut(body);
            let vessels = if limb.has_state(state::VESSELS_CLAMPED) {
                0
            } else {
                state::VESSELS_UNCLAMPED
            };
            limb.add_surgery(state::SKIN_OPEN | vessels);
            limb.remove_surgery(state::SKIN_CUT);
        }),
        Op::CloseSkin => {
            let sawed = world
                .get::<Limbs>(mob)
                .is_some_and(|l| l.limb(body).has_state(state::BONE_SAWED));
            if sawed {
                heal_limb(world, mob, body, 40.0);
            }
            with_limbs(world, mob, |l| {
                l.limb_mut(body).remove_surgery(state::UNSET_ON_CLOSE)
            });
        }
        Op::ClampBleeders => {
            let sawed = world
                .get::<Limbs>(mob)
                .is_some_and(|l| l.limb(body).has_state(state::BONE_SAWED));
            if sawed {
                heal_limb(world, mob, body, 20.0);
            }
            with_limbs(world, mob, |l| {
                let limb = l.limb_mut(body);
                limb.add_surgery(state::VESSELS_CLAMPED);
                limb.remove_surgery(state::VESSELS_UNCLAMPED);
            });
        }
        Op::UnclampBleeders => with_limbs(world, mob, |l| {
            let limb = l.limb_mut(body);
            limb.add_surgery(state::VESSELS_UNCLAMPED);
            limb.remove_surgery(state::VESSELS_CLAMPED);
        }),
        Op::SawBones => {
            with_limbs(world, mob, |l| {
                l.limb_mut(body).add_surgery(state::BONE_SAWED)
            });
            damage_limb(world, mob, body, DamageType::Brute, 50.0);
            own = format!("Вы распилили кости в {place} {target}.");
            others = format!("{user} распилил кости в {place} у {target}!");
        }
        Op::FixBones => {
            with_limbs(world, mob, |l| {
                l.limb_mut(body).remove_surgery(state::ALL_BONE)
            });
            heal_limb(world, mob, body, 40.0);
        }
        Op::DrillBones => with_limbs(world, mob, |l| {
            l.limb_mut(body).add_surgery(state::BONE_DRILLED)
        }),
        Op::InciseOrgans => {
            with_limbs(world, mob, |l| {
                l.limb_mut(body).add_surgery(state::ORGANS_CUT)
            });
            damage_limb(world, mob, body, DamageType::Brute, 10.0);
        }
        Op::TendWounds(tend, heal) => {
            let (brute, burn) = tend_amounts(world, mob, tend, heal);
            if let Some(mut health) = world.get::<Health>(mob).cloned() {
                heal_bodypart_damage(&mut health, brute, burn);
                world.entity_mut(mob).insert(health);
            }
            let covered = patient(world, mob)
                .is_some_and(|p| p.covered & crate::chem::body_part_bit(BodyZone::Chest) != 0);
            let clothes = if covered {
                " насколько это возможно, пока на нём одежда"
            } else {
                ""
            };
            let progress = tend_progress(world, actor.mob, mob, heal, brute, burn);
            own = format!("Вам удается залечить некоторые раны {target}{clothes}{progress}.");
            others = format!("{user} залечивает некоторые раны {target}{clothes}.");
        }
        Op::RepairDislocation | Op::RepairHairline | Op::RepairCompound => {
            with_limbs(world, mob, |l| {
                let limb = l.limb_mut(body);
                if let Some(i) = limb
                    .wounds
                    .iter()
                    .position(|w| w.def().series == Series::Bone)
                {
                    super::wounds::remove_wound(limb, i);
                }
            })
        }
        Op::ResetCompound => with_limbs(world, mob, |l| {
            if let Some(w) = l
                .limb_mut(body)
                .wounds
                .iter_mut()
                .find(|w| w.kind == WoundKind::Compound)
            {
                w.reset = true;
            }
        }),
        Op::RemoveOrgan(slot) => {
            let organ = slot.spec().ru.0;
            visible_range(
                world,
                actor.mob,
                &format!("Вы успешно извлекаете {organ} у {target}."),
                &format!("{user} успешно извлекает {organ} у {target}!"),
                Span::Notice,
                1,
            );
            pain(
                world,
                mob,
                &format!(
                    "Ваша {} пульсирует от боли, вы больше не чувствуете свой {organ}!",
                    body.ru().0
                ),
            );
            if let (Some(removed), Some(&pos)) =
                (organs::organ(world, mob, slot), world.get::<TilePos>(mob))
            {
                organs::remove(world, removed, false);
                crate::storage::take_out(world, removed, pos);
            }
            return;
        }
        Op::RemoveImplant(slot) => {
            let Some(implant) = cyberimps::implant_in(world, mob, slot) else {
                return;
            };
            let name = name_of(world, implant);
            visible_range(
                world,
                actor.mob,
                &format!("Вы успешно извлекаете {name} у {target}."),
                &format!("{user} успешно извлекает {name} у {target}!"),
                Span::Notice,
                1,
            );
            pain(
                world,
                mob,
                &format!(
                    "Ваша {} пульсирует от боли, вы больше не чувствуете свой {name}!",
                    body.ru().0
                ),
            );
            cyberimps::remove(world, implant);
            if let Some(&pos) = world.get::<TilePos>(mob) {
                crate::storage::take_out(world, implant, pos);
            }
            return;
        }
        Op::InsertOrgan(_) | Op::InsertImplant => {
            let Some(organ) = actor.item else {
                return;
            };
            let inserted = if op == Op::InsertImplant {
                let path = world
                    .get::<DmItem>(organ)
                    .map(|i| i.path.clone())
                    .unwrap_or_default();
                cyberimps::slot_for(&path, zone)
                    .is_some_and(|slot| cyberimps::insert(world, organ, mob, slot))
            } else {
                organs::insert(world, organ, mob)
            };
            if !inserted {
                return;
            }
            own = format!("Вы успешно вставляете {tool} в {target}.");
            others = format!("{user} успешно вставляет {tool} в {target}.");
            pain(
                world,
                mob,
                &format!(
                    "Ваша {} пульсирует от боли, пока орган начинает приживаться!",
                    body.ru().0
                ),
            );
        }
        Op::Amputate => {
            let part = body.ru().1;
            own = format!("Вы успешно ампутировали {part} у {target}!");
            others = format!("{user} успешно ампутировал {part} у {target}!");
            pain(world, mob, &format!("Вы больше не чувствуете свою {part}!"));
            super::bodyparts::drop_limb(world, mob, body);
        }
        Op::AttachLimb => {
            let Some(limb) = actor.item else {
                return;
            };
            // `OPERATION_REJECTION_DAMAGE` of a flesh limb of the same species.
            super::hurt_limb(world, mob, BodyZone::Chest, DamageType::Tox, 10.0);
            if !super::bodyparts::attach_limb(world, mob, limb) {
                return;
            }
            let part = body.ru().1;
            own = format!("Вам удалось заменить {part} у {target}.");
            others = format!("{user} успешно заменяет {part} у {target}!");
            pain(
                world,
                mob,
                &format!(
                    "Вы ощущаете синтетическое чувство, которое распространяется от вашей {}, \
                     снова чувствуя её!",
                    super::wounds::zone_prepositional(body)
                ),
            );
        }
        Op::Revival => {
            visible_range(
                world,
                actor.mob,
                &format!("Вам удаётся успешно ударить мозг {target} разрядом с помощью {tool}..."),
                &format!("{user} посылает мощный разряд в мозг {target} с помощью {tool}..."),
                Span::Notice,
                1,
            );
            revive_by_shock(world, mob);
            return;
        }
        Op::CoronaryBypass => {
            repair_organ(world, mob, Slot::Heart, REPAIR_HEAL_TO);
            own = format!("Вы успешно установили шунт на сердце {target}.");
            others = format!("{user} успешно установил шунт на сердце {target}.");
            pain(
                world,
                mob,
                "Боль в груди пульсирует, но сердце чувствует себя лучше, чем когда-либо!",
            );
        }
        Op::BrainSurgery => {
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                organs::damage_organ(
                    world,
                    brain,
                    -organs::BRAIN_DAMAGE_DEATH * BRAIN_HEAL,
                    organs::BRAIN_DAMAGE_DEATH,
                );
            }
            own = format!("Вам удалось прооперировать мозг {target}.");
            others = format!("{user} успешно оперирует мозг {target}!");
            pain(
                world,
                mob,
                "Боль в голове отступает, думать становится немного легче!",
            );
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                super::traumas::cure_all(world, brain, super::traumas::Resilience::Surgery);
            }
            let brain = organs::state(world, mob, Slot::Brain).map_or(0.0, |o| o.damage);
            if brain > organs::BRAIN_DAMAGE_DEATH * 0.1 {
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    format!("Мозг {target} выглядит так, будто его можно вылечить еще немного."),
                );
            }
        }
        Op::Bioware(kind) => {
            super::bioware::gain(world, mob, kind);
            let [_, _, done, did, _, hurt] = bioware_lines(kind, &target, &user);
            own = done;
            others = did;
            pain(world, mob, &hurt);
        }
        Op::FilterBlood => {
            filter_blood(world, mob);
            let tool = capitalize(&tool);
            own = format!("{tool} завершает цикл фильтрации крови у {target}.");
            others = format!("{tool} гудит, фильтруя кровь у {target}.");
        }
        Op::Lobotomy => {
            own = format!("Вы успешно провели лоботомию {target}!");
            others = format!("{user} успешно проводит лоботомию {target}!");
            pain(
                world,
                mob,
                "Ваша голова на мгновение полностью немеет, боль просто невыносима!",
            );
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                super::traumas::cure_all(world, brain, super::traumas::Resilience::Lobotomy);
                let mut rng = super::fork_rng(world);
                if rng.chance(75.0) {
                    lobotomy_trauma(world, brain, &mut rng);
                }
            }
        }
        Op::Pacify => {
            own = format!("Вам удалось пацифицировать {target}.");
            others = format!("{user} успешно оперирует мозг {target}!");
            pain(
                world,
                mob,
                "Ваша голова пульсирует... мысль о насилии вспыхивает в вашем разуме, и вас \
                 едва не выворачивает наизнанку!",
            );
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                super::traumas::gain(
                    world,
                    brain,
                    super::traumas::Kind::Pacifism,
                    Some(super::traumas::Resilience::Lobotomy),
                );
            }
        }
        Op::Lobectomy | Op::Hepatectomy | Op::Gastrectomy => {
            let (slot, heal_to, organ, pain_text) = match op {
                Op::Lobectomy => (
                    Slot::Lungs,
                    REPAIR_HEAL_TO,
                    "наиболее повреждённую долю легкого",
                    "",
                ),
                Op::Hepatectomy => (
                    Slot::Liver,
                    HEPATECTOMY_HEAL_TO,
                    "повреждённую часть печени",
                    "Боль немного отступает!",
                ),
                _ => (
                    Slot::Stomach,
                    GASTRECTOMY_HEAL_TO,
                    "повреждённую часть желудка",
                    "Боль в животе немного утихает!",
                ),
            };
            repair_organ(world, mob, slot, heal_to);
            own = format!("Вы успешно удаляете {organ} {target}.");
            others = format!("{user} успешно удаляет {organ} {target}.");
            pain(world, mob, pain_text);
        }
        Op::EarSurgery | Op::EyeSurgery => {
            let ears = op == Op::EarSurgery;
            let slot = if ears { Slot::Ears } else { Slot::Eyes };
            repair_organ(world, mob, slot, 0.0);
            if !ears {
                crate::vision::cure_temp_blindness(world, mob);
                // "This will fix itself slowly."
                crate::vision::set_eye_blur_if_lower(world, mob, 70.0);
            }
            if ears
                && let Some(organ) = organs::organ(world, mob, slot)
                && let Some(mut state) = world.get_mut::<organs::Organ>(organ)
            {
                state.deafness = EAR_SURGERY_DEAFNESS;
            }
            let (what, text) = if ears {
                (
                    "уши",
                    "Голова идет кругом, но, кажется, слух начинает возвращаться!",
                )
            } else {
                (
                    "глаза",
                    "Зрение затуманено, но кажется, теперь вы видите немного лучше!",
                )
            };
            own = format!("Вы успешно вылечили {what} {target}.");
            others = format!("{user} успешно вылечил {what} {target}.");
            pain(world, mob, text);
        }
        Op::Debride => {
            damage_limb(world, mob, body, DamageType::Brute, 3.0);
            let mut left = 0.0;
            with_limbs(world, mob, |l| {
                if let Some(w) = l
                    .limb_mut(body)
                    .wounds
                    .iter_mut()
                    .find(|w| w.def().series == Series::Burn)
                {
                    w.infection -= DEBRIDE_INFECTION_REMOVED;
                    w.sanitization += DEBRIDE_SANITIZATION_ADDED;
                    left = w.infection;
                }
            });
            let progress = debride_progress(left);
            own = format!(
                "Вы успешно удалили часть зараженной плоти с {place}{progress} у {target}."
            );
            others = format!(
                "{user} успешно удалил часть зараженной плоти с {place} у {target} с помощью {tool}!"
            );
        }
        Op::RepairPuncture | Op::SealVeins => {
            let seal = op == Op::SealVeins;
            if !seal {
                damage_limb(world, mob, body, DamageType::Brute, 3.0);
            }
            let mut closed = false;
            with_limbs(world, mob, |l| {
                let limb = l.limb_mut(body);
                let Some(i) = limb
                    .wounds
                    .iter()
                    .position(|w| w.def().series == Series::Pierce)
                else {
                    return;
                };
                let by = if seal { -0.5 } else { -0.25 };
                if super::wounds::adjust_pierce(limb, i, by) {
                    limb.wounds[i].mend_state = !seal;
                } else {
                    closed = true;
                }
            });
            let (verb_own, verb_others) = if seal {
                ("прижгли", "прижигает")
            } else {
                ("соединили", "соединяет")
            };
            if closed {
                own = format!(
                    "Вы успешно {verb_own} последние разорванные кровеносные сосуды в {place} у \
                     {target}."
                );
                others = format!(
                    "{user} успешно {verb_others} последние разорванные кровеносные сосуды в \
                     {place} у {target} с помощью {tool}!"
                );
            } else {
                own = format!(
                    "Вы успешно {verb_own} часть кровеносных сосудов в {place} у {target}."
                );
                others = format!(
                    "{user} успешно {verb_others} часть кровеносных сосудов в {place} у {target} \
                     с помощью {tool}!"
                );
            }
        }
    }
    visible_range(world, actor.mob, &own, &others, Span::Notice, 1);
}

/// revival's `on_success` after the shock: the ghost back, 50 oxygen healed, `revive()`; brain
/// damage either way (15 up to 180 alive, 50 up to 199 not).
fn revive_by_shock(world: &mut World, mob: Entity) {
    crate::ghost::return_to_body(world, mob);
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.adjust(DamageType::Oxy, -50.0);
        world.entity_mut(mob).insert(health);
    }
    let target = name_of(world, mob);
    if super::defib::revive(world, mob) {
        let text = format!("...{target} приходит в себя - живой и в сознании!");
        super::visible(world, mob, &text, &text, Span::Notice);
        organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 15.0, 180.0);
    } else {
        let text = format!("...{target} дёргается в конвульсиях и замирает.");
        super::visible(world, mob, &text, &text, Span::Warning);
        organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 50.0, 199.0);
    }
}

/// tend wounds' healing: the flat amount plus a share of the patient's damage (×0.2 dead,
/// ×0.55 through clothes on the chest).
fn tend_amounts(world: &World, mob: Entity, tend: Tend, heal: Heal) -> (f32, f32) {
    let (amount, share) = tend.healing();
    let Some(health) = world.get::<Health>(mob) else {
        return (0.0, 0.0);
    };
    let dead = if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        0.2
    } else {
        1.0
    };
    let covered = patient(world, mob)
        .is_some_and(|p| p.covered & crate::chem::body_part_bit(BodyZone::Chest) != 0);
    let access = if covered { 0.55 } else { 1.0 };
    let round = |x: f32| (x * 10.0).round() / 10.0;
    let brute = amount + round(health.brute * share * dead * access);
    let burn = amount + round(health.burn * share * dead * access);
    match heal {
        Heal::Brute => (brute, 0.0),
        Heal::Burn => (0.0, burn),
        Heal::Combo => (brute, burn),
    }
}

/// tg `heal_bodypart_damage`: over the damaged parts (`heal_overall_damage`).
fn heal_bodypart_damage(health: &mut Health, brute: f32, burn: f32) {
    if brute > 0.0 {
        health.adjust(DamageType::Brute, -brute);
    }
    if burn > 0.0 {
        health.adjust(DamageType::Burn, -burn);
    }
}

/// tg `on_failure` (the operations' own replace the generic message where tg does not call the
/// parent).
fn failure(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone, speed: f32) {
    let body = zone.check_zone();
    let place = super::wounds::zone_prepositional(body);
    let (user, target) = (name_of(world, actor.mob), name_of(world, mob));
    let tool = item_name(world, actor.item);
    let say = |world: &mut World, own: String, others: String| {
        visible_range(world, actor.mob, &own, &others, Span::Warning, 1);
    };
    match op {
        Op::TendWounds(tend, heal) => {
            say(world, "Вы ошибаетесь!".into(), format!("{user} ошибается!"));
            // tg: 80% of the flat amount and half the share, as damage.
            let (amount, share) = tend.healing();
            let (brute, burn) = match world.get::<Health>(mob) {
                Some(h) => {
                    let round = |x: f32| (x * 10.0).round() / 10.0;
                    let brute = amount * 0.8 + round(h.brute * share * 0.5);
                    let burn = amount * 0.8 + round(h.burn * share * 0.5);
                    match heal {
                        Heal::Brute => (brute, 0.0),
                        Heal::Burn => (0.0, burn),
                        Heal::Combo => (brute, burn),
                    }
                }
                None => (0.0, 0.0),
            };
            if let Some(mut health) = world.get::<Health>(mob).cloned() {
                if brute > 0.0 {
                    health.adjust(DamageType::Brute, brute);
                }
                if burn > 0.0 {
                    health.adjust(DamageType::Burn, burn);
                }
                world.entity_mut(mob).insert(health);
            }
            return;
        }
        Op::Revival => {
            say(
                world,
                format!(
                    "Вы бьёте мозг {target} разрядом с помощью {tool}, но он никак не реагирует."
                ),
                format!(
                    "{user} бьёт мозг {target} разрядом с помощью {tool}, но он никак не реагирует."
                ),
            );
            return;
        }
        Op::CoronaryBypass => {
            fail_organ(world, mob, Slot::Heart, REPAIR_FAILURE_DAMAGE);
            say(
                world,
                "Вы ошибаетесь при креплении шунта, и он отрывается, повреждая часть сердца!"
                    .into(),
                format!(
                    "{user} совершает ошибку, из-за чего поток крови обильно хлещет из груди {target}!"
                ),
            );
            pain(
                world,
                mob,
                "Грудь горит; кажется, вы сходите с ума от боли!",
            );
            return;
        }
        Op::BrainSurgery => {
            fail_organ(world, mob, Slot::Brain, BRAIN_FAILURE_DAMAGE);
            say(
                world,
                "Вы совершаете ошибку, нанося еще больше повреждений!".into(),
                format!("{user} совершает ошибку, вызывая повреждение мозга!"),
            );
            pain(
                world,
                mob,
                "Ваша голова пульсирует от ужасной боли; даже думать больно!",
            );
            severe_lobotomy_trauma(world, mob);
            return;
        }
        // `cortex_imprint/on_failure`: 60 brain damage and a severe trauma (lobotomy-level).
        Op::Bioware(Bioware::CortexImprint) if organs::organ(world, mob, Slot::Brain).is_some() => {
            say(
                world,
                "Вы ошибаетесь, повреждая мозг!".into(),
                format!("{user} ошибается, нанеся повреждения мозгу!"),
            );
            pain(
                world,
                mob,
                "Голова раскалывается от ужасной боли; от одной мысли об этом уже начинает болеть \
                 голова!",
            );
            organs::adjust_organ_loss(world, mob, Slot::Brain, 60.0);
            severe_lobotomy_trauma(world, mob);
            return;
        }
        Op::FilterBlood => {
            say(
                world,
                format!("Вы ошибаетесь, оставляя синяк на груди у {target}!"),
                format!("{user} ошибается, оставляя синяк на груди у {target}!"),
            );
            damage_limb(world, mob, BodyZone::Chest, DamageType::Brute, 5.0);
            return;
        }
        Op::Lobotomy => {
            say(
                world,
                "Вы удалили не ту часть, нанеся еще больше повреждений!".into(),
                format!("{user} безуспешно пытается провести лоботомию {target}!"),
            );
            pain(
                world,
                mob,
                "Боль в вашей голове, кажется, только усиливается!",
            );
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                organs::damage_organ(world, brain, 80.0, organs::BRAIN_DAMAGE_DEATH);
                let mut rng = super::fork_rng(world);
                lobotomy_trauma(world, brain, &mut rng);
            }
            return;
        }
        Op::Pacify => {
            say(
                world,
                format!("Вы совершаете ошибку, перепутав все связи в мозгу {target}..."),
                format!("{user} совершает ошибку, вызывая повреждение мозга!"),
            );
            pain(
                world,
                mob,
                "Ваша голова пульсирует, и кажется, становится только хуже!",
            );
            severe_lobotomy_trauma(world, mob);
            return;
        }
        Op::Lobectomy => {
            fail_organ(world, mob, Slot::Lungs, LOBECTOMY_FAILURE_DAMAGE);
            if let Some(mut resp) = world.get_mut::<crate::breath::Respiration>(mob) {
                resp.losebreath += 4.0;
            }
            say(
                world,
                format!(
                    "Вы совершаете ошибку, не сумев удалить повреждённую долю легкого {target}!"
                ),
                format!("{user} совершает ошибку!"),
            );
            pain(
                world,
                mob,
                "Вы чувствуете резкий укол в груди; у вас перехватывает дыхание, и каждый вдох \
                 причиняет боль!",
            );
            return;
        }
        Op::Hepatectomy | Op::Gastrectomy => {
            let (slot, what) = if op == Op::Hepatectomy {
                (Slot::Liver, "печени")
            } else {
                (Slot::Stomach, "желудка")
            };
            fail_organ(world, mob, slot, CUT_FAILURE_DAMAGE);
            say(
                world,
                format!("Вы отрезали не ту часть {what} {target}!"),
                format!("{user} отрезал не ту часть {what} {target}!"),
            );
            pain(world, mob, "Боль в животе усиливается!");
            return;
        }
        // tg damages the ears or eyes themselves here, against its own lines (and the older
        // surgeries) that hit the brain; the brain it is.
        Op::EarSurgery | Op::EyeSurgery => {
            match organs::organ(world, mob, Slot::Brain) {
                Some(brain) => {
                    say(
                        world,
                        format!("Вы случайно тычите инструментом прямо в мозг {target}!"),
                        format!("{user} случайно тычет инструментом прямо в мозг {target}!"),
                    );
                    pain(
                        world,
                        mob,
                        "Вы чувствуете пронзающую боль, проходящую сквозь голову прямо в мозг!",
                    );
                    organs::damage_organ(
                        world,
                        brain,
                        SENSE_FAILURE_BRAIN_DAMAGE,
                        organs::BRAIN_DAMAGE_DEATH,
                    );
                }
                None => say(
                    world,
                    format!(
                        "Вы случайно тыкаете туда, где должен быть мозг {target}! Хорошо, что \
                         его там нет."
                    ),
                    format!(
                        "{user} случайно тыкает инструментом прямо в мозг {target}! Или сделал \
                         бы это, будь у пациента мозг."
                    ),
                ),
            }
            return;
        }
        Op::Debride => {
            let hurt = super::fork_rng(world).range(4, 8) as f32;
            damage_limb(world, mob, body, DamageType::Brute, hurt);
            say(
                world,
                format!("Вы отрезаете немного здоровой плоти с {place} у {target}."),
                format!(
                    "{user} отрезает немного здоровой плоти с {place} у {target} с помощью {tool}!"
                ),
            );
            return;
        }
        Op::RepairPuncture => {
            // shortcut: the damage rolls no new wound (tg `wound_bonus = 10`, edged).
            let hurt = super::fork_rng(world).range(4, 8) as f32;
            damage_limb(world, mob, body, DamageType::Brute, hurt);
            say(
                world,
                format!("Вы случайно разрываете кровеносные сосуды в {place} у {target}."),
                format!(
                    "{user} разрывает кровеносные сосуды в {place} у {target} с помощью {tool}!"
                ),
            );
            return;
        }
        _ => {}
    }
    let screwed = if speed >= 5.0 {
        " В данных условиях это практически невозможно..."
    } else if speed >= 4.0 {
        " В таких условиях это трудно сделать правильно..."
    } else if (2.5..3.0).contains(&speed) {
        " А ведь у вас почти получилось..."
    } else {
        ""
    };
    say(
        world,
        format!("Вы облажались!{screwed}"),
        format!("{user} облажался!"),
    );
    if op == Op::RepairDislocation {
        damage_limb(world, mob, body, DamageType::Brute, 25.0);
        pain(world, mob, "Боль в конечности усиливается!");
    }
    let _ = CANT_WOUND;
}

/// tg `/datum/element/surgery_aid/surgery_prep`: drapes toggle the selected bodypart (the chest
/// when the zone has none).
pub fn drapes(world: &mut World, actor: Actor, mob: Entity) {
    let zone = world
        .get::<ZoneSelected>(actor.mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone();
    let Some(mut limbs) = world.get::<Limbs>(mob).cloned() else {
        return;
    };
    let limb = limbs.limb_mut(zone);
    let text = if limb.prepped {
        limb.prepped = false;
        "операция отменена".to_owned()
    } else {
        if !limbs.parts.iter().any(|l| l.prepped) {
            limbs.prep_moves = 0;
        }
        limbs.limb_mut(zone).prepped = true;
        format!("{} подготовлена для операции", capitalize(zone.ru().0))
    };
    world.entity_mut(mob).insert(limbs);
    tell(world, actor.client, Span::Notice, text);
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// `/datum/status_effect/surgery_prepped/on_move`: four steps standing and the drapes fall off.
fn drapes_fall_off(
    mut moved: MessageReader<ss13_world::Moved>,
    mut mobs: Query<(&mut Limbs, &BodyPosition, Option<&Name>)>,
    mut commands: Commands,
) {
    for m in moved.read() {
        let Ok((mut limbs, posture, name)) = mobs.get_mut(m.entity) else {
            continue;
        };
        if !limbs.parts.iter().any(|l| l.prepped) || posture.lying() {
            continue;
        }
        limbs.prep_moves += 1;
        if limbs.prep_moves < 4 {
            continue;
        }
        for limb in &mut limbs.parts {
            limb.prepped = false;
        }
        let mob = m.entity;
        let name = name.map_or_else(|| "кого-то".to_owned(), |n| n.as_str().to_owned());
        commands.queue(move |world: &mut World| {
            let text = format!("Простынь спадает с {name}!");
            super::visible(world, mob, &text, &text, Span::Warning);
        });
    }
}

/// A mob lying buckled to an operating table or a bed (tg `buckled`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Buckled(pub Entity);

/// The furniture a mob can be buckled to lying down (`can_buckle` with `buckle_lying`).
pub fn buckles_lying(path: &str) -> bool {
    is_under(path, "/obj/structure/table/optable") || is_under(path, "/obj/structure/bed")
}

/// tg `mouse_drop_receive` -> `mouse_buckle_handling` -> `user_buckle_mob` of a table or bed.
pub fn mouse_drop_receive(
    world: &mut World,
    actor: Actor,
    furniture: Entity,
    dropped: Entity,
) -> bool {
    let path = crate::construction::path_of(world, furniture).unwrap_or_default();
    if !buckles_lying(&path) || world.get::<Health>(dropped).is_none() {
        return false;
    }
    if furniture_taken(world, furniture) || world.get::<Buckled>(dropped).is_some() {
        return true;
    }
    let same_tile = world.get::<TilePos>(dropped) == world.get::<TilePos>(furniture);
    if dropped != actor.mob && !same_tile {
        let (user, target) = (name_of(world, actor.mob), name_of(world, dropped));
        let what = crate::construction::path_of(world, furniture)
            .map(|_| name_of(world, furniture))
            .unwrap_or_default();
        super::visible(
            world,
            actor.mob,
            &format!("Вы начинаете пристегивать {target} к {what}!"),
            &format!("{user} начинает пристегивать {target} к {what}!"),
            Span::Warning,
        );
        do_after(
            world,
            actor,
            dropped,
            BUCKLE_DELAY,
            0,
            crate::construction::Job::Medical(MedJob::Buckle { to: furniture }),
        );
        return true;
    }
    buckle(world, actor, dropped, furniture);
    true
}

fn furniture_taken(world: &mut World, furniture: Entity) -> bool {
    world
        .query::<&Buckled>()
        .iter(world)
        .any(|b| b.0 == furniture)
}

/// tg `buckle_mob` + `buckle_feedback` of the operating table.
pub fn buckle(world: &mut World, actor: Actor, mob: Entity, furniture: Entity) {
    let Some(&pos) = world.get::<TilePos>(furniture) else {
        return;
    };
    let near = world
        .get::<TilePos>(mob)
        .and_then(|p| p.chebyshev(pos))
        .is_some_and(|d| d <= 1);
    if !near || furniture_taken(world, furniture) || world.get::<Buckled>(mob).is_some() {
        return;
    }
    world.entity_mut(mob).insert((pos, Buckled(furniture)));
    let (user, target, what) = (
        name_of(world, actor.mob),
        name_of(world, mob),
        name_of(world, furniture),
    );
    if mob == actor.mob {
        let text = format!("{user} ложится на {what}.");
        super::visible(
            world,
            mob,
            &format!("Вы ложитесь на {what}."),
            &text,
            Span::Notice,
        );
    } else {
        super::visible(
            world,
            actor.mob,
            &format!("Вы кладёте {target} на {what}."),
            &format!("{user} кладёт {target} на {what}."),
            Span::Notice,
        );
    }
}

/// tg `user_unbuckle_mob` (an empty hand on the furniture, or the buckled mob resisting).
pub fn unbuckle(world: &mut World, actor: Actor, mob: Entity) {
    let Some(Buckled(furniture)) = world.get::<Buckled>(mob).copied() else {
        return;
    };
    world.entity_mut(mob).remove::<Buckled>();
    let (user, target, what) = (
        name_of(world, actor.mob),
        name_of(world, mob),
        name_of(world, furniture),
    );
    if mob == actor.mob {
        super::visible(
            world,
            mob,
            &format!("Вы встаёте с {what}."),
            &format!("{user} встаёт с {what}."),
            Span::Notice,
        );
    } else {
        super::visible(
            world,
            actor.mob,
            &format!("Вы отстёгиваете {target} с {what}."),
            &format!("{user} отстёгивает {target} с {what}."),
            Span::Notice,
        );
    }
}

/// The mob buckled to `furniture`, if any.
pub fn buckled_to(world: &mut World, furniture: Entity) -> Option<Entity> {
    world
        .query::<(Entity, &Buckled)>()
        .iter(world)
        .find(|(_, b)| b.0 == furniture)
        .map(|(e, _)| e)
}

/// A buckled mob that left the furniture's tile (teleported, the furniture gone) is free.
fn check_buckles(
    mobs: Query<(Entity, &Buckled, &TilePos)>,
    furniture: Query<&TilePos>,
    mut commands: Commands,
) {
    for (mob, Buckled(to), pos) in &mobs {
        if furniture.get(*to).ok() != Some(pos) {
            commands.entity(mob).remove::<Buckled>();
        }
    }
}

/// A buckled mob resisting gets up (tg `resist` -> `resist_buckle`); a cuffed one is left to
/// `crate::restraints`.
fn resist_buckle(
    mut intents: MessageReader<ss13_net::server::IntentReceived>,
    players: Res<crate::server::Players>,
    mobs: Query<(), (With<Buckled>, Without<crate::restraints::Handcuffed>)>,
    mut commands: Commands,
) {
    for intent in intents.read() {
        if !matches!(intent.intent, ss13_shared::protocol::ClientIntent::Resist) {
            continue;
        }
        let client = intent.client;
        let Some(mob) = players.mob(client).filter(|m| mobs.contains(*m)) else {
            continue;
        };
        commands.queue(move |world: &mut World| {
            let actor = Actor {
                client,
                mob,
                item: None,
            };
            unbuckle(world, actor, mob);
        });
    }
}

pub(super) fn build(app: &mut App) {
    use ss13_tgui::server::{AppTguiExt, UiSystems};
    app.register_ui::<SurgeryListWindow>().add_systems(
        Update,
        (
            pick_operation.in_set(UiSystems::Handle),
            close_stale_lists.in_set(UiSystems::StatusHook),
            (drapes_fall_off, check_buckles, resist_buckle)
                .after(ss13_world::WorldSystems::Movement),
        ),
    );
}

/// The combat mode of `mob`.
pub fn combat_mode(world: &World, mob: Entity) -> bool {
    world.get::<CombatMode>(mob).is_some_and(|c| c.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implants_and_bioware_need_their_zone_and_state() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world
            .spawn((
                Health::default(),
                Stat::Conscious,
                organs::OrganHost,
                Limbs::default(),
                BodyPosition::default(),
            ))
            .id();
        let tube =
            cyberimps::spawn_implant(&mut world, "/obj/item/organ/cyberimp/mouth/breathing_tube")
                .unwrap();
        assert!(cyberimps::insert(
            &mut world,
            tube,
            mob,
            ImplantSlot::BreathingTube
        ));
        let remove = Op::RemoveImplant(ImplantSlot::BreathingTube);
        assert!(!is_available(&world, remove, mob, TargetZone::Mouth));
        with_limbs(&mut world, mob, |l| {
            let head = l.limb_mut(BodyZone::Head);
            head.prepped = true;
            head.add_surgery(state::SKIN_OPEN | state::ORGANS_CUT);
        });
        assert!(is_available(&world, remove, mob, TargetZone::Mouth));
        assert!(!is_available(&world, remove, mob, TargetZone::Head));
        // Chest implants and bioware need the chest's bone sawed too.
        with_limbs(&mut world, mob, |l| {
            let chest = l.limb_mut(BodyZone::Chest);
            chest.prepped = true;
            chest.add_surgery(state::SKIN_OPEN | state::ORGANS_CUT);
        });
        let veins = Op::Bioware(Bioware::VeinThreading);
        assert!(!is_available(
            &world,
            Op::InsertImplant,
            mob,
            TargetZone::Chest
        ));
        assert!(!is_available(&world, veins, mob, TargetZone::Chest));
        with_limbs(&mut world, mob, |l| {
            l.limb_mut(BodyZone::Chest).add_surgery(state::BONE_SAWED)
        });
        assert!(is_available(
            &world,
            Op::InsertImplant,
            mob,
            TargetZone::Chest
        ));
        assert!(is_available(&world, veins, mob, TargetZone::Chest));
        // One circulation bioware per patient.
        super::super::bioware::gain(&mut world, mob, Bioware::MuscledVeins);
        assert!(!is_available(&world, veins, mob, TargetZone::Chest));
        // Bioware is locked: only disks and research offer it.
        assert!(!operation_pool(&[]).contains(&veins));
        assert!(operation_pool(&[veins.def().path]).contains(&veins));
    }

    #[test]
    fn blood_filter_takes_a_share_of_each_reagent() {
        use ss13_reagents::{Holder, Owner, Reagent};
        let mut world = World::new();
        let reagent = |id, volume| Reagent {
            id,
            volume,
            purity: 1.0,
            creation_purity: 1.0,
            ph: 7.0,
            metabolizing: false,
            overdosed: false,
            current_cycle: 0,
            overdose_threshold: 0.0,
            metabolization_rate: 0.2,
            data: None,
        };
        let mut blood = Holder::new(1000.0, 0, Owner::default());
        blood.reagents = vec![reagent(1, 100.0), reagent(2, 10.0), reagent(3, 0.5)];
        blood.update_total();
        let mob = world
            .spawn(crate::chem::MobReagents {
                blood,
                stomach: Holder::new(1000.0, 0, Owner::default()),
                drunk: 0.0,
            })
            .id();
        filter_blood(&mut world, mob);
        let left: Vec<f32> = world
            .get::<crate::chem::MobReagents>(mob)
            .unwrap()
            .blood
            .reagents
            .iter()
            .map(|r| r.volume)
            .collect();
        // 22%: capped at 10, 2.2 rounded to 0.2 steps, at least 0.4.
        let want = [90.0, 7.8, 0.1];
        assert!(
            left.iter().zip(want).all(|(l, w)| (l - w).abs() < 1e-3),
            "{left:?}"
        );
        assert!(can_loop(&mut world, Op::FilterBlood, mob));
        world
            .get_mut::<crate::chem::MobReagents>(mob)
            .unwrap()
            .blood
            .reagents
            .clear();
        assert!(!can_loop(&mut world, Op::FilterBlood, mob));
    }

    fn tool(path: &str, behaviour: &str, sharpness: u32, force: f32) -> ToolInfo {
        ToolInfo {
            path: Some(path.into()),
            behaviour: behaviour.into(),
            toolspeed: 1.0,
            sharpness,
            force,
            temperature: 0.0,
            shock_ready: false,
        }
    }

    #[test]
    fn tool_quality_follows_implements() {
        let scalpel = tool("/obj/item/scalpel", "scalpel", 1, 10.0);
        assert_eq!(tool_quality(Op::InciseSkin, &scalpel), 1.0);
        let knife = tool("/obj/item/knife/kitchen", "knife", 1, 10.0);
        assert_eq!(tool_quality(Op::InciseSkin, &knife), 1.5);
        // Any edged item: `/obj/item` 3.33; a blunt one cannot cut.
        let glass = tool("/obj/item/broken_bottle", "", 1, 9.0);
        assert_eq!(tool_quality(Op::InciseSkin, &glass), 3.33);
        let toolbox = tool("/obj/item/storage/toolbox", "", 0, 12.0);
        assert_eq!(tool_quality(Op::InciseSkin, &toolbox), 0.0);
        // A saw needs an edge with 10 force, or the saw behaviour.
        assert_eq!(tool_quality(Op::SawBones, &knife), 4.0);
        assert_eq!(tool_quality(Op::SawBones, &glass), 0.0);
        let saw = tool("/obj/item/circular_saw", "saw", 1, 15.0);
        assert_eq!(tool_quality(Op::SawBones, &saw), 1.0);
        // Closing needs heat (a lit cautery) or sutures.
        let mut cautery = tool("/obj/item/cautery", "cautery", 0, 5.0);
        assert_eq!(tool_quality(Op::CloseSkin, &cautery), 0.0);
        cautery.temperature = 500.0;
        assert_eq!(tool_quality(Op::CloseSkin, &cautery), 1.0);
        // Hands reset a dislocation at 5×.
        assert_eq!(
            tool_quality(Op::RepairDislocation, &ToolInfo::default()),
            5.0
        );
        let hemostat = tool("/obj/item/hemostat", "hemostat", 0, 0.0);
        assert_eq!(
            tool_quality(Op::TendWounds(Tend::Basic, Heal::Brute), &hemostat),
            1.0
        );
        let mut fast = hemostat.clone();
        fast.toolspeed = 0.5;
        assert_eq!(tool_quality(Op::ClampBleeders, &fast), 0.5);
    }

    #[test]
    fn surgery_step_success_modifiers() {
        assert_eq!(
            location_modifier(&["/obj/structure/table/optable".into()]),
            1.0
        );
        assert_eq!(
            location_modifier(&["/obj/structure/table/reinforced".into()]),
            1.25
        );
        assert_eq!(location_modifier(&["/obj/structure/bed".into()]), 1.5);
        assert_eq!(location_modifier(&[]), 2.0);
        assert_eq!(
            location_modifier(&[
                "/obj/structure/bed".into(),
                "/obj/structure/table/optable".into()
            ]),
            1.0
        );
        // A scalpel on an operating table: speed 1, no time penalty.
        assert_eq!(speed_modifier(1.0, 1.0, 1.0, 1.0), 1.0);
        assert_eq!(base_failure_chance(16.0, 1.0).clamp(0.0, 99.0), 0.0);
        // A conscious patient: +40; a basic tool: +5; in the dark up to +60.
        let scalpel = tool("/obj/item/scalpel", "scalpel", 1, 10.0);
        let basic = tool_mod(Op::InciseSkin, &scalpel);
        assert_eq!(basic, 5.0);
        assert_eq!(failure_chance(16.0, 1.0, false, None, basic), 45.0);
        assert_eq!(failure_chance(16.0, 1.0, true, None, basic), 5.0);
        let advanced = tool("/obj/item/scalpel/advanced", "scalpel", 1, 10.0);
        let advanced = tool_mod(Op::InciseSkin, &advanced);
        assert_eq!(failure_chance(16.0, 1.0, true, None, advanced), 0.0);
        assert_eq!(failure_chance(16.0, 1.0, true, Some(0.0), advanced), 60.0);
        // An improvised implement (listed above 1.15): +10, whatever its type.
        let knife = tool("/obj/item/knife/kitchen", "knife", 1, 10.0);
        assert_eq!(tool_mod(Op::InciseSkin, &knife), 10.0);
        assert_eq!(tool_mod(Op::RepairDislocation, &ToolInfo::default()), 10.0);
        // A kitchen knife on the floor: 1.5 × 2 = 3 -> 0.5 × 1.6 s over the threshold = 8%.
        let speed = speed_modifier(1.5, 1.0, 2.0, 1.0);
        assert_eq!(speed, 3.0);
        assert!((base_failure_chance(16.0, speed) - 8.0).abs() < 1e-4);
        // Asleep or numbed: ×0.8.
        let numb = speed_modifier(1.5, 1.0, 2.0, patient_speed_mod(true));
        assert!((numb - 2.4).abs() < 1e-5);
        assert!(base_failure_chance(16.0, 2.4) < 0.0);
    }

    #[test]
    fn drunk_surgeons_are_slow_except_at_the_ballmer_peak() {
        assert_eq!(surgeon_speed_mod(0.0), 1.0);
        assert_eq!(surgeon_speed_mod(13.0), 0.8);
        // 30^1.5 / 90 = 1.83 -> 1.8.
        assert!((surgeon_speed_mod(30.0) - 2.8).abs() < 1e-5);
    }

    #[test]
    fn locked_operations_and_replacements() {
        let unlocked = operation_pool(&[]);
        assert!(unlocked.contains(&Op::TendWounds(Tend::Basic, Heal::Brute)));
        assert!(
            !unlocked
                .iter()
                .any(|op| matches!(op, Op::TendWounds(t, _) if *t != Tend::Basic))
        );
        // The upgrade replaces the basic tending; the combo sits next to it.
        let upgraded = operation_pool(&[Tend::Upgraded.path(), Tend::Combo.path()]);
        assert!(!upgraded.contains(&Op::TendWounds(Tend::Basic, Heal::Brute)));
        assert!(upgraded.contains(&Op::TendWounds(Tend::Upgraded, Heal::Burn)));
        assert!(upgraded.contains(&Op::TendWounds(Tend::Combo, Heal::Combo)));
        // The best combo replaces every tending below it, down the chain.
        let master = operation_pool(&[
            Tend::Upgraded.path(),
            Tend::Master.path(),
            Tend::ComboMaster.path(),
        ]);
        let tends: Vec<_> = master
            .iter()
            .filter(|op| matches!(op, Op::TendWounds(..)))
            .collect();
        assert_eq!(tends, [&Op::TendWounds(Tend::ComboMaster, Heal::Combo)]);
        // A disk path the port has no operation for is ignored.
        assert_eq!(
            operation_pool(&["/datum/surgery_operation/organ/brainwash"]).len(),
            unlocked.len()
        );
        // Every type maps back to itself.
        for op in Op::types() {
            assert_eq!(Op::of_path(op.def().path), Some(op));
        }
    }

    #[test]
    fn requirements_read_like_tg() {
        let [all, any, optional, blocked] = Op::InciseSkin.requirements();
        assert_eq!(all, ["пациент должен лежать"]);
        assert!(any.is_empty() && optional.is_empty());
        assert_eq!(
            blocked,
            [
                "кожа должна быть целой",
                "рабочая область не должна быть закрыта одеждой"
            ]
        );
        let [all, ..] = Op::Revival.requirements();
        assert_eq!(all[0], "операция на голова (цель: голова)");
        assert!(all.contains(&"требуется, чтобы кость была распилена".to_owned()));
        let [_, any, ..] = Op::TendWounds(Tend::Basic, Heal::Brute).requirements();
        assert_eq!(any, ["кожа должна быть разрезана или вскрыта"]);
    }
}
