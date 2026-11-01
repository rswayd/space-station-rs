//! Surgery (server), after tg `code/modules/surgery/operations` (`_operation.dm`
//! `/mob/living/perform_surgery`, `get_available_operations`, `/datum/surgery_operation`
//! `check_availability`, `get_tool_quality`, `get_time_modifiers`, `get_location_modifier`,
//! `try_perform`, `on_failure`; `operation_generic.dm`, `operation_healing.dm`,
//! `operation_bone_repair.dm`, `operation_organ_manip.dm`, `operation_amputation.dm`), the
//! BandaStation failure chance (`modular_bandastation/medical/code/surgery/fail_chance.dm`), the
//! surgical drapes' `/datum/element/surgery_aid` and buckling to operating tables and beds
//! (`game/objects/buckling.dm`, `buckle_lying`).
//!
//! The checked-out tg has no `/datum/surgery` + `/datum/surgery_step` any more: an operation is
//! picked by the tool used on a bodypart in some surgical state ([`state`]). Drapes clicked on a
//! mob prepare the selected bodypart; out of combat mode a click on a prepared mob lists the
//! operations the held tool (or the empty hand) can do on the selected zone: one starts at once,
//! several open the tgui `ListInputWindow` (tg's radial menu). Each operation is a `do_after` of
//! `time × min(speed, 2.5)`, where speed multiplies the tool quality (its `implements` entry ×
//! `toolspeed`), the surface (operating table 1, table 1.25, bed 1.5, floor 2) and 0.8 for an
//! unconscious patient; then it fails with BandaStation's chance (the time over 2.5× of the base,
//! +40% on a conscious patient, darkness, +5% for basic tools) or succeeds. A buckled mob lies on
//! the table (dragged onto it, 2 s for someone else) until an empty hand clicks the table or it
//! resists.
// shortcut: no operating computer, surgery disks or locked (upgraded) operations, no mood events,
// sounds, surgical memory, blood on the surgeon's gloves, self-surgery trait, manual cauterizing
// of yourself; no limb removal, so amputation fails with a message; organs are not modelled, so
// organ manipulation finds nothing to take out; a drunk surgeon is not slowed; no prosthetic,
// abductor, thick-skin or skull operations (cranial fissures, debridement, organ repair, ...).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmTool, DmWounding};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject, UiWindow};
use ss13_tgui::{UiInterface, UiStatus};

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
pub const DRAPES: &str = "/obj/item/surgical_drapes";
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
}

/// tend wounds' radial options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Heal {
    Brute,
    Burn,
}

/// The unlocked operations of a human.
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
    TendWounds(Heal),
    RepairDislocation,
    RepairHairline,
    ResetCompound,
    RepairCompound,
    OrganManipulation,
    Amputate,
}

/// The static part of an operation.
#[derive(Clone, Copy, Debug)]
pub struct OpDef {
    pub name: &'static str,
    /// Deciseconds.
    pub time: f32,
    pub flags: u16,
    pub all_required: u16,
    pub any_required: u16,
    pub any_blocked: u16,
    pub implements: &'static [(Implement, f32)],
    /// The preop phrase: "Вы начинаете {verb} {zone} у {patient}...".
    pub verb: &'static str,
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

impl Op {
    pub const ALL: [Self; 17] = [
        Self::InciseSkin,
        Self::RetractSkin,
        Self::CloseSkin,
        Self::ClampBleeders,
        Self::UnclampBleeders,
        Self::SawBones,
        Self::FixBones,
        Self::DrillBones,
        Self::InciseOrgans,
        Self::TendWounds(Heal::Brute),
        Self::TendWounds(Heal::Burn),
        Self::RepairDislocation,
        Self::RepairHairline,
        Self::ResetCompound,
        Self::RepairCompound,
        Self::OrganManipulation,
        Self::Amputate,
    ];

    pub fn def(self) -> OpDef {
        use state::*;
        let base = OpDef {
            name: "",
            time: 10.0,
            flags: 0,
            all_required: 0,
            any_required: 0,
            any_blocked: 0,
            implements: &[],
            verb: "оперировать",
        };
        match self {
            Self::InciseSkin => OpDef {
                name: "Сделать разрез кожи",
                time: 16.0,
                any_blocked: ALL_SKIN,
                implements: INCISE,
                verb: "делать разрез на",
                ..base
            },
            Self::RetractSkin => OpDef {
                name: "retract skin",
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
                ..base
            },
            Self::CloseSkin => OpDef {
                name: "Прижечь разрез кожи",
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
                ..base
            },
            Self::ClampBleeders => OpDef {
                name: "Зажать сосуды",
                time: 24.0,
                all_required: SKIN_OPEN | VESSELS_UNCLAMPED,
                implements: CLAMP,
                verb: "зажимать кровеносные сосуды в",
                ..base
            },
            Self::UnclampBleeders => OpDef {
                name: "Разжать сосуды",
                time: 24.0,
                all_required: SKIN_OPEN | VESSELS_CLAMPED,
                implements: CLAMP,
                verb: "разжимать кровеносные сосуды в",
                ..base
            },
            Self::SawBones => OpDef {
                name: "Распилить кость",
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
                ..base
            },
            Self::FixBones => OpDef {
                name: "Фиксация сломанной кости",
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
                ..base
            },
            Self::DrillBones => OpDef {
                name: "Сверление кости",
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
                ..base
            },
            Self::InciseOrgans => OpDef {
                name: "Сделать разрез органа",
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
                ..base
            },
            Self::TendWounds(heal) => OpDef {
                name: match heal {
                    Heal::Brute => "Обработка ушибов",
                    Heal::Burn => "Обработка ожогов",
                },
                time: 25.0,
                flags: flags::LOOPING | flags::IGNORE_CLOTHES,
                any_required: ALL_SKIN,
                implements: &[
                    (Tool("hemostat"), 1.0),
                    (Tool("screwdriver"), 1.5),
                    (Tool("cutters"), 1.67),
                    (Path("/obj/item/pen"), 1.8),
                ],
                verb: "обрабатывать раны:",
                ..base
            },
            Self::RepairDislocation => OpDef {
                name: "reset dislocation",
                time: 24.0,
                flags: flags::STANDING_ALLOWED | flags::IGNORE_CLOTHES,
                implements: &[
                    (Tool("bonesetter"), 1.0),
                    (Tool("crowbar"), 2.0),
                    (Hand, 5.0),
                ],
                verb: "вправлять вывих на",
                ..base
            },
            Self::RepairHairline => OpDef {
                name: "Сращивание трещины",
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
                ..base
            },
            Self::ResetCompound => OpDef {
                name: "reset compound fracture",
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
                ..base
            },
            Self::RepairCompound => OpDef {
                name: "repair compound fracture",
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
                ..base
            },
            Self::OrganManipulation => OpDef {
                name: "Манипуляции с органами",
                time: 24.0,
                all_required: SKIN_OPEN | ORGANS_CUT,
                implements: &[(Tool("hemostat"), 1.0), (Tool("crowbar"), 1.8)],
                verb: "искать органы в",
                ..base
            },
            Self::Amputate => OpDef {
                name: "Ампутация конечности",
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
                ..base
            },
        }
    }
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
        }
    }

    fn under(&self, root: &str) -> bool {
        self.path.as_deref().is_some_and(|p| is_under(p, root))
    }
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

/// tg `get_tool_quality`: 0 when the tool cannot do the operation.
pub fn tool_quality(op: Op, tool: &ToolInfo) -> f32 {
    let implements = op.def().implements;
    let Some(path) = &tool.path else {
        return implements
            .iter()
            .find(|(i, _)| *i == Hand)
            .map_or(0.0, |(_, q)| *q);
    };
    if !tool_check(op, tool) {
        return 0.0;
    }
    let by_tool = implements
        .iter()
        .find(|(i, _)| !tool.behaviour.is_empty() && *i == Tool(leak(&tool.behaviour)))
        .map(|(_, q)| *q);
    let by_path = implements
        .iter()
        .find(|(i, _)| matches!(i, Path(root) if is_under(path, root)))
        .map(|(_, q)| *q);
    tool.toolspeed * by_tool.or(by_path).unwrap_or(0.0)
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

/// tg `get_time_modifiers` (tool quality × surface × patient), rounded to 0.01.
pub fn speed_modifier(quality: f32, location: f32, patient_unconscious: bool) -> f32 {
    let mut total = if quality > 0.0 { quality } else { 1.0 };
    total *= location;
    if patient_unconscious {
        total *= 0.8;
    }
    (total * 100.0).round() / 100.0
}

/// `GET_FAILURE_CHANCE(base_time, speed_mod)`, time in deciseconds.
pub fn base_failure_chance(time: f32, speed: f32) -> f32 {
    FAILURE_CHANCE_PER_SECOND * ((speed * time - SURGERY_MODIFIER_FAILURE_THRESHOLD * time) / 10.0)
}

/// BandaStation `get_modified_failure_chance`: the base chance (clamped), +40 on a patient who
/// feels it, darkness (`lumcount` below 0.6) and the tool's grade.
pub fn failure_chance(
    time: f32,
    speed: f32,
    patient_unconscious: bool,
    lumcount: Option<f32>,
    tool_path: Option<&str>,
) -> f32 {
    let mut chance = base_failure_chance(time, speed).clamp(0.0, 99.0);
    if !patient_unconscious {
        chance += 40.0;
    }
    if let Some(light) = lumcount
        && light < 0.6
    {
        chance += (1.0 - light / 0.6).powf(1.5) * 60.0;
    }
    // The "ghetto tool" test indexes `implements` with the tool object, which is never a key:
    // every item is graded by its type path.
    if let Some(path) = tool_path {
        chance += if path.contains("/advanced") {
            TOOL_MOD_ADVANCED
        } else if path.contains("/alien") {
            TOOL_MOD_ALIEN
        } else if path.contains("/augment") {
            TOOL_MOD_AUGMENT
        } else {
            TOOL_MOD_BASIC
        };
    }
    chance
}

/// Mob-level facts operations check.
#[derive(Clone, Debug)]
struct Patient {
    lying: bool,
    unconscious: bool,
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
    Some(Patient {
        lying: world.get::<BodyPosition>(mob).is_some_and(|b| b.lying()),
        unconscious: health.knocked_out(stat),
        covered,
        prepped_any: limbs.parts.iter().any(|l| l.prepped),
        brute: health.brute,
        burn: health.burn,
    })
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
    let (Some(p), Some(limbs)) = (patient(world, mob), world.get::<Limbs>(mob)) else {
        return false;
    };
    if tool_quality(op, tool) <= 0.0 {
        return false;
    }
    let def = op.def();
    // `basic` operations (tend wounds) work on the chest of a prepared patient.
    let body = match op {
        Op::TendWounds(_) => {
            if zone != TargetZone::Chest || !p.prepped_any || !limbs.limb(BodyZone::Chest).prepped {
                return false;
            }
            BodyZone::Chest
        }
        _ => zone.check_zone(),
    };
    let limb = limbs.limb(body);
    if def.all_required != 0 && !limb.has_state(def.all_required) {
        return false;
    }
    if def.any_required != 0 && !limb.has_any_state(def.any_required) {
        return false;
    }
    if def.any_blocked != 0 && limb.has_any_state(def.any_blocked) {
        return false;
    }
    let state_ok = match op {
        Op::TendWounds(Heal::Brute) => p.brute > 0.0,
        Op::TendWounds(Heal::Burn) => p.burn > 0.0,
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
        Op::Amputate => !matches!(body, BodyZone::Head | BodyZone::Chest),
        _ => true,
    };
    if !state_ok {
        return false;
    }
    if def.flags & flags::IGNORE_CLOTHES == 0 && p.covered & crate::chem::body_part_bit(body) != 0 {
        return false;
    }
    if def.flags & flags::STANDING_ALLOWED == 0 && !p.lying {
        return false;
    }
    if def.flags & flags::SELF_OPERABLE == 0 && mob == surgeon {
        return false;
    }
    true
}

/// tg `get_available_operations`, sorted by name (`cmp_name_asc`).
pub fn available_ops(
    world: &World,
    mob: Entity,
    surgeon: Entity,
    zone: TargetZone,
    tool: &ToolInfo,
) -> Vec<Op> {
    let mut ops: Vec<Op> = Op::ALL
        .into_iter()
        .filter(|&op| available(world, op, mob, surgeon, zone, tool))
        .collect();
    ops.sort_by_key(|op| op.def().name.to_lowercase());
    ops
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
    let ops = available_ops(world, mob, actor.mob, zone, &tool);
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

/// tg `try_perform` up to its `do_after`.
fn try_perform(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone) {
    let tool = ToolInfo::read(world, actor.item);
    if !available(world, op, mob, actor.mob, zone, &tool) {
        return;
    }
    preop(world, actor, mob, op, zone);
    let delay =
        op.def().time * speed(world, op, mob, &tool).min(SURGERY_MODIFIER_FAILURE_THRESHOLD);
    do_after(
        world,
        actor,
        mob,
        delay,
        0,
        crate::construction::Job::Medical(MedJob::Surgery { op, zone }),
    );
}

fn speed(world: &World, op: Op, mob: Entity, tool: &ToolInfo) -> f32 {
    let location = world.get::<TilePos>(mob).map_or(2.0, |&pos| {
        let paths: Vec<String> = world
            .resource::<ss13_world::SpatialIndex>()
            .on_tile(pos)
            .iter()
            .filter_map(|&e| crate::construction::path_of(world, e))
            .collect();
        location_modifier(&paths)
    });
    let unconscious = patient(world, mob).is_some_and(|p| p.unconscious);
    speed_modifier(tool_quality(op, tool), location, unconscious)
}

fn zone_words(op: Op, zone: TargetZone) -> String {
    match op {
        Op::TendWounds(Heal::Brute) => "ушибы".into(),
        Op::TendWounds(Heal::Burn) => "ожоги".into(),
        Op::Amputate => zone.check_zone().ru().1.into(),
        _ => super::wounds::zone_prepositional(zone.check_zone()).into(),
    }
}

/// tg `on_preop` (`display_results` reaches only adjacent mobs) and `display_pain`.
fn preop(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone) {
    let (user, target) = (name_of(world, actor.mob), name_of(world, mob));
    let verb = op.def().verb;
    let what = zone_words(op, zone);
    visible_range(
        world,
        actor.mob,
        &format!("Вы начинаете {verb} {what} у {target}..."),
        &format!("{user} начинает {verb} {what} у {target}."),
        Span::Notice,
        1,
    );
    pain(
        world,
        mob,
        "Вы чувствуете острую боль, когда вас оперируют!",
    );
}

/// tg `display_pain`: nothing for the unconscious.
fn pain(world: &mut World, mob: Entity, text: &str) {
    if patient(world, mob).is_some_and(|p| p.unconscious) {
        return;
    }
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
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
    if !available(world, op, mob, surgeon, zone, &tool) {
        return;
    }
    let speed = speed(world, op, mob, &tool);
    let unconscious = patient(world, mob).is_some_and(|p| p.unconscious);
    let light = world
        .get::<TilePos>(mob)
        .and_then(|&p| crate::botany::lumcount(world, p));
    let chance = failure_chance(
        op.def().time,
        speed,
        unconscious,
        light,
        tool.path.as_deref(),
    );
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
    if op.def().flags & flags::LOOPING != 0
        && still_holding
        && available(
            world,
            op,
            mob,
            surgeon,
            zone,
            &ToolInfo::read(world, actor.item),
        )
    {
        let delay = op.def().time * speed.min(SURGERY_MODIFIER_FAILURE_THRESHOLD);
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

/// tg `on_success` of each operation.
fn success(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone) {
    let body = zone.check_zone();
    let (user, target) = (name_of(world, actor.mob), name_of(world, mob));
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
            own = format!(
                "Вы распилили кости в {} {target}.",
                super::wounds::zone_prepositional(body)
            );
            others = format!(
                "{user} распилил кости в {} у {target}!",
                super::wounds::zone_prepositional(body)
            );
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
        Op::TendWounds(heal) => {
            let (brute, burn) = tend_amounts(world, mob, heal, 1.0);
            if let Some(mut health) = world.get::<Health>(mob).cloned() {
                heal_bodypart_damage(&mut health, brute, burn);
                world.entity_mut(mob).insert(health);
            }
            own = format!("Вам удается залечить некоторые раны {target}");
            others = format!("{user} залечивает некоторые раны {target}");
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
        Op::OrganManipulation => {
            own = "Вы не находите органов, которые можно извлечь.".into();
        }
        Op::Amputate => {
            own = "Конечность не поддаётся: отделить её не выходит.".into();
        }
    }
    visible_range(world, actor.mob, &own, &others, Span::Notice, 1);
}

/// tend wounds' healing: the flat amount plus a share of the patient's damage (×0.2 dead,
/// ×0.55 through clothes on the chest).
fn tend_amounts(world: &World, mob: Entity, heal: Heal, multiplier: f32) -> (f32, f32) {
    // tg `/datum/surgery_operation/basic/tend_wounds`: `healing_amount` 5, `healing_multiplier`
    // 0.07.
    let (amount, share) = (5.0, 0.07 * multiplier);
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
    match heal {
        Heal::Brute => (amount + round(health.brute * share * dead * access), 0.0),
        Heal::Burn => (0.0, amount + round(health.burn * share * dead * access)),
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

/// tg `on_failure` (and the operations' own).
fn failure(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone, speed: f32) {
    let screwed = if speed >= 5.0 {
        " В данных условиях это практически невозможно..."
    } else if speed >= 4.0 {
        " В таких условиях это трудно сделать правильно..."
    } else if (2.5..3.0).contains(&speed) {
        " А ведь у вас почти получилось..."
    } else {
        ""
    };
    let (user, _) = (name_of(world, actor.mob), name_of(world, mob));
    visible_range(
        world,
        actor.mob,
        &format!("Вы облажались!{screwed}"),
        &format!("{user} облажался!"),
        Span::Warning,
        1,
    );
    match op {
        Op::TendWounds(heal) => {
            // tg: 80% of the flat amount and half the share, as damage.
            let (brute, burn) = match world.get::<Health>(mob) {
                Some(h) => {
                    let round = |x: f32| (x * 10.0).round() / 10.0;
                    match heal {
                        Heal::Brute => (5.0 * 0.8 + round(h.brute * 0.07 * 0.5), 0.0),
                        Heal::Burn => (0.0, 5.0 * 0.8 + round(h.burn * 0.07 * 0.5)),
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
        }
        Op::RepairDislocation => {
            damage_limb(world, mob, zone.check_zone(), DamageType::Brute, 25.0);
            pain(world, mob, "Боль в конечности усиливается!");
        }
        _ => {}
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

    fn tool(path: &str, behaviour: &str, sharpness: u32, force: f32) -> ToolInfo {
        ToolInfo {
            path: Some(path.into()),
            behaviour: behaviour.into(),
            toolspeed: 1.0,
            sharpness,
            force,
            temperature: 0.0,
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
        assert_eq!(tool_quality(Op::TendWounds(Heal::Brute), &hemostat), 1.0);
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
        assert_eq!(speed_modifier(1.0, 1.0, false), 1.0);
        assert_eq!(base_failure_chance(16.0, 1.0).clamp(0.0, 99.0), 0.0);
        // A conscious patient: +40; a basic tool: +5; in the dark up to +60.
        assert_eq!(
            failure_chance(16.0, 1.0, false, None, Some("/obj/item/scalpel")),
            45.0
        );
        assert_eq!(
            failure_chance(16.0, 1.0, true, None, Some("/obj/item/scalpel")),
            5.0
        );
        assert_eq!(
            failure_chance(16.0, 1.0, true, None, Some("/obj/item/scalpel/advanced")),
            0.0
        );
        assert_eq!(
            failure_chance(
                16.0,
                1.0,
                true,
                Some(0.0),
                Some("/obj/item/scalpel/advanced")
            ),
            60.0
        );
        // A kitchen knife on the floor: 1.5 × 2 = 3 -> 0.5 × 1.6 s over the threshold = 8%.
        let speed = speed_modifier(1.5, 2.0, false);
        assert_eq!(speed, 3.0);
        assert!((base_failure_chance(16.0, speed) - 8.0).abs() < 1e-4);
        // Asleep: ×0.8.
        assert!((speed_modifier(1.5, 2.0, true) - 2.4).abs() < 1e-5);
        assert!(base_failure_chance(16.0, 2.4) < 0.0);
    }
}
