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
// [`super::cyberimps`]); no arbitrary items as prosthetics or Frankenstein (other species')
// rejection damage; no abductor, thick-skin or skull operations (robotic limbs: the screwdriver,
// hatch, multitool and wrench operations, `Op::required_robotic`); brainwashing
// only on organic brains (`crate::antag::brainwashed`; no `mechanic` reprogramming). Amputation takes the head too (with its organs); robotic limbs come apart with a
// wrench instead (`amputate/mechanic`). Lobotomy, pacification and brain surgery work on the brain's traumas
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
use ss13_chat::capitalize;

mod autopsy;
mod buckle;
mod checks;
mod defs;
mod failure;
mod perform;
mod success;

pub use autopsy::*;
pub use buckle::*;
pub use checks::*;
pub use defs::*;
use failure::*;
pub use perform::*;
use success::*;

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
    /// The mechanical equivalents (`operation_generic_mechanic.dm`) on robotic limbs.
    MechanicalIncision,
    MechanicalOpen,
    MechanicalClose,
    PrepareElectronics,
    MechanicUnwrench,
    MechanicWrench,
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
    /// `amputate/mechanic`: a robotic limb comes apart.
    AmputateMechanic,
    /// `prosthetic_replacement` with a limb item on a stump.
    AttachLimb,
    Revival,
    CoronaryBypass,
    BrainSurgery,
    Lobotomy,
    Pacify,
    /// `organ/brainwash`: the surgeon's directive (asked before the operation starts).
    Brainwash,
    /// `organ/brainwash/sleeper`: a random sleeper agent directive.
    BrainwashSleeper,
    Lobectomy,
    Hepatectomy,
    Gastrectomy,
    EarSurgery,
    EyeSurgery,
    Debride,
    RepairPuncture,
    SealVeins,
    FilterBlood,
    /// `limb/autopsy`: the report and the operating computer's autopsy experiments.
    Autopsy,
    /// `limb/autopsy/mechanic` on a robotic chest.
    AutopsyMechanic,
    /// `basic/dissection`: research notes from a corpse.
    Dissection,
    /// `basic/viral_bonding`: the patient's diseases become carried without symptoms.
    ViralBonding,
    /// `limb/bionecrosis`: a romerol tumour in the brain.
    Bionecrosis,
    /// BandaStation `limb/tune_vocal_cords`: the patient's voice changed.
    TuneVocalCords,
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

pub(super) fn build(app: &mut App) {
    use ss13_tgui::server::{AppTguiExt, UiSystems};
    app.add_observer(add_notes);
    app.register_ui::<SurgeryListWindow>().add_systems(
        Update,
        (
            pick_operation.in_set(UiSystems::Handle),
            close_stale_lists.in_set(UiSystems::StatusHook),
            (
                drapes_fall_off,
                (carry_buckled, check_buckles).chain(),
                resist_buckle,
            )
                .after(ss13_world::WorldSystems::Movement),
        ),
    );
}

/// The combat mode of `mob`.
pub fn combat_mode(world: &World, mob: Entity) -> bool {
    world.get::<CombatMode>(mob).is_some_and(|c| c.0)
}
#[cfg(test)]
mod tests;
