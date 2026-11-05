//! Research data from the DM dump, written by `ss13-mapload convert` to [`RESEARCH_FILE`]: the
//! techweb nodes (`/datum/techweb_node/*`, `code/modules/research/techweb/nodes/`), the designs
//! (`/datum/design/*`, `code/modules/research/designs/`), the materials the ore silo stores
//! (`/datum/material/*` with `MATERIAL_SILO_STORED`) with the stacks that carry them
//! (`mats_per_unit`), the R&D production machines (`/obj/machinery/rnd/production/*`: build
//! types, department flags and their circuit board's parts) and `SSresearch`'s point tables.
//! The game (`ss13_game::science`) reads it at startup.
//!
//! As tg's `valid_subtypesof`, types that are their own `abstract_type` are left out.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::is_under;

/// `<assets>/research.ron`.
pub const RESEARCH_FILE: &str = "research.ron";
const NODE_ROOT: &str = "/datum/techweb_node";
const DESIGN_ROOT: &str = "/datum/design";
const MATERIAL_ROOT: &str = "/datum/material";
const STACK_ROOT: &str = "/obj/item/stack";
pub const PRODUCTION_ROOT: &str = "/obj/machinery/rnd/production";
pub const IMPRINTER_ROOT: &str = "/obj/machinery/rnd/production/circuit_imprinter";
pub const CONSOLE_ROOT: &str = "/obj/machinery/computer/rdconsole";
const RESEARCH_SS: &str = "/datum/controller/subsystem/research";
const ECONOMY_SS: &str = "/datum/controller/subsystem/economy";
const DEPARTMENT_ROOT: &str = "/datum/job_department";
const SERVO: &str = "/datum/stock_part/servo";
const MATTER_BIN: &str = "/datum/stock_part/matter_bin";
const EXPERIMENT_ROOT: &str = "/datum/experiment";
/// `MATERIAL_SILO_STORED` (`code/__DEFINES/materials.dm`).
const MATERIAL_SILO_STORED: u32 = 1 << 0;
/// `SHEET_MATERIAL_AMOUNT`.
pub const SHEET_MATERIAL_AMOUNT: f64 = 100.0;
/// `MAX_STACK_SIZE`.
pub const MAX_STACK_SIZE: u32 = 50;
/// DM `ALL` as `departmental_flags` default.
pub const ALL_FLAGS: u32 = 16_777_215;

/// Build type bits (`code/__DEFINES/machines.dm`).
pub mod build_type {
    pub const IMPRINTER: u32 = 1 << 0;
    pub const PROTOLATHE: u32 = 1 << 1;
    pub const AUTOLATHE: u32 = 1 << 2;
    pub const MECHFAB: u32 = 1 << 3;
    pub const BIOGENERATOR: u32 = 1 << 4;
    pub const LIMBGROWER: u32 = 1 << 5;
    pub const SMELTER: u32 = 1 << 6;
    pub const AWAY_LATHE: u32 = 1 << 8;
    pub const AWAY_IMPRINTER: u32 = 1 << 9;
    pub const COMPONENT_PRINTER: u32 = 1 << 10;
    /// `GLOB.build_types_to_string` without the away types (the R&D console drops them).
    pub const NAMES: [(u32, &str); 8] = [
        (IMPRINTER, "Circuit Imprinter"),
        (PROTOLATHE, "Protolathe"),
        (AUTOLATHE, "Autolathe"),
        (MECHFAB, "Exosuit Fabricator"),
        (BIOGENERATOR, "Biogenerator"),
        (LIMBGROWER, "Limb Grower"),
        (SMELTER, "Smelter"),
        (COMPONENT_PRINTER, "Component Printer"),
    ];
}

/// `TECHWEB_NODE_*` (`code/__DEFINES/research.dm`).
pub mod node_flags {
    pub const STARTER: u32 = 1 << 0;
    pub const HIDDEN: u32 = 1 << 1;
    pub const EXPERIMENTAL: u32 = 1 << 2;
}

/// A `/datum/material` the silo stores.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MaterialDef {
    /// The type path (`SSmaterials` key).
    pub id: String,
    pub name: String,
    /// `sheet_type || ore_type`: what ejecting it makes.
    pub sheet_type: Option<String>,
    /// `#RRGGBB`.
    pub color: Option<String>,
}

/// A `/datum/design`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DesignDef {
    pub id: String,
    pub name: String,
    /// `get_description()`: `desc`, else the built type's.
    pub desc: String,
    pub build_type: u32,
    /// `None` for reagent designs (`make_reagent`) and paths that are not object types.
    pub build_path: Option<String>,
    /// Silo material type -> units per item.
    pub materials: Vec<(String, f64)>,
    /// A `materials` key is a `/datum/material_requirement` or `/datum/material_slot` (any
    /// material of a class), which [`Self::materials`] cannot express.
    pub material_requirements: bool,
    pub category: Vec<String>,
    pub departmental_flags: u32,
    /// Deciseconds.
    pub construction_time: f64,
    pub lathe_time_factor: f64,
    pub fixed_cost_efficiency: bool,
    /// `ispath(build_path, /obj/item/stack)`.
    pub stack: bool,
}

/// A `/datum/techweb_node`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct NodeDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub flags: u32,
    pub prerequisites: Vec<String>,
    pub designs: Vec<String>,
    /// Point type -> amount (`research_costs`).
    pub costs: Vec<(String, f64)>,
    pub required_experiments: Vec<String>,
    pub discount_experiments: Vec<(String, f64)>,
    /// Item types whose deconstruction unhides it (`required_items_to_unlock`).
    pub required_items: Vec<String>,
    pub announce_channels: Vec<String>,
    /// `experiments_to_unlock`: experiments the techweb gains when this node is researched.
    #[serde(default)]
    pub experiments_to_unlock: Vec<String>,
}

/// What an experiment checks beyond the scanned type (its `/datum/experiment` subtype).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub enum ExperimentKind {
    /// `/datum/experiment/scanning` (and plain `scanning/random`): scan the required atoms.
    #[default]
    Scanning,
    /// `scanning/points`: each required atom type is worth its number of points.
    Points,
    /// `scanning/points/machinery_tiered_scan`: machines with a part of at least this tier.
    MachineryTiered { tier: u32 },
    /// `scanning/points/machinery_pinpoint_scan`: machines with this stock part type.
    MachineryPinpoint { part: String, part_name: String },
    /// `scanning/reagent`: a container whose master reagent is this one, pure enough.
    Reagent {
        reagent: String,
        reagent_name: String,
        min_purity: f64,
    },
    /// Experiments whose checks need systems the port does not model here (people, plants,
    /// materials, mechs, cytology, fish, autopsies, ordnance, physical events, exploration).
    Unsupported,
}

/// A `/datum/experiment` type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ExperimentDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub exp_tag: String,
    pub performance_hint: String,
    /// `EXPERIMENT_TRAIT_*` bits.
    pub traits: u32,
    pub kind: ExperimentKind,
    /// Types that may perform it (`allowed_experimentors`); empty: any.
    pub allowed_experimentors: Vec<String>,
    /// Type -> count to scan (points per scan for the points kinds).
    pub required_atoms: Vec<(String, u32)>,
    /// `scanning/points` `required_points`.
    pub required_points: u32,
    /// `scanning/random`: the types `New()` picks `total_requirement` scans from, at most
    /// `max_requirement_per_type` of one.
    pub possible_types: Vec<String>,
    pub total_requirement: u32,
    pub max_requirement_per_type: u32,
    /// `scan_message`: replaces the generic stage text.
    pub scan_message: String,
    /// Point type -> amount given on completion (`points_reward`).
    pub points_reward: Vec<(String, f64)>,
    /// Names of the required and possible types, for the stage texts.
    pub names: Vec<(String, String)>,
}

/// An R&D console type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ConsoleDef {
    pub path: String,
    /// Its board's `locked`.
    pub locked: bool,
    /// Locking and unlocking needs it.
    pub req_access: Vec<String>,
}

/// An `/obj/machinery/rnd/production` type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LatheDef {
    pub path: String,
    pub build_types: u32,
    pub department_flags: u32,
    /// Circuit imprinter efficiency (`0.5 ** (servo rating - 1)`) instead of the protolathe's
    /// (`1.2 - 0.1 * servo rating`).
    pub imprinter: bool,
    /// Servos and matter bins of its board (`req_components`); tier 1 parts.
    pub servos: u32,
    pub matter_bins: u32,
    /// `payment_department`.
    pub payment_department: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ResearchData {
    pub nodes: Vec<NodeDef>,
    pub designs: Vec<DesignDef>,
    pub materials: Vec<MaterialDef>,
    /// Stack type -> silo material units per stack unit (`mats_per_unit`).
    pub stacks: Vec<(String, Vec<(String, f64)>)>,
    pub lathes: Vec<LatheDef>,
    /// `/datum/job_department` `department_bitflags` -> `department_name`.
    pub departments: Vec<(u32, String)>,
    /// `SSresearch.point_types`: point type -> abbreviation.
    pub point_types: Vec<(String, String)>,
    /// `SSresearch.single_server_income`, per second.
    pub server_income: Vec<(String, f64)>,
    /// `SSresearch.techweb_point_items`: item type -> points.
    pub point_items: Vec<(String, Vec<(String, f64)>)>,
    /// `SSeconomy.techweb_bounty`: credits to the science budget per researched node.
    pub techweb_bounty: f64,
    pub consoles: Vec<ConsoleDef>,
    #[serde(default)]
    pub experiments: Vec<ExperimentDef>,
}

impl ResearchData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let text = ss13_data::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        ron::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("research data serializes"),
        );
        out.push('\n');
        out
    }

    /// The types that need prototypes: what protolathes and circuit imprinters print, the sheets
    /// ejected from storage, and every design's build path, whose icon the techweb and the
    /// design details show for all build types (exosuit fabricator, autolathe, ...).
    pub fn prototype_types(&self) -> BTreeSet<&str> {
        self.designs
            .iter()
            .filter_map(|d| d.build_path.as_deref())
            .chain(
                self.materials
                    .iter()
                    .filter_map(|m| m.sheet_type.as_deref()),
            )
            .collect()
    }
}

pub(crate) fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

pub(crate) fn path(v: Option<&Value>) -> Option<String> {
    v?.get("path")?.as_str().map(str::to_owned)
}

fn num(v: Option<&Value>) -> f64 {
    v.and_then(Value::as_f64).unwrap_or(0.0)
}

/// The items of a dumped list: plain entries, or keys of `assoc` entries with their values.
pub(crate) fn list(v: Option<&Value>) -> Vec<(&Value, Option<&Value>)> {
    v.and_then(|v| v.get("list"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| match item.get("assoc").and_then(Value::as_array) {
                    Some(pair) if pair.len() == 2 => (&pair[0], Some(&pair[1])),
                    _ => (item, None),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn paths(v: Option<&Value>) -> Vec<String> {
    list(v)
        .into_iter()
        .filter_map(|(k, _)| path(Some(k)))
        .collect()
}

pub(crate) fn texts(v: Option<&Value>) -> Vec<String> {
    list(v)
        .into_iter()
        .filter_map(|(k, _)| k.as_str().map(str::to_owned))
        .collect()
}

/// `key = number` pairs whose keys are strings (`research_costs`) or paths.
pub(crate) fn amounts(v: Option<&Value>) -> Vec<(String, f64)> {
    list(v)
        .into_iter()
        .filter_map(|(k, amount)| {
            let key = k.as_str().map(str::to_owned).or_else(|| path(Some(k)))?;
            Some((key, amount.and_then(Value::as_f64).unwrap_or(0.0)))
        })
        .collect()
}

pub(crate) fn subtypes<'a>(types: &'a DmTypes, root: &str) -> Vec<&'a str> {
    let mut out: Vec<&str> = types
        .types
        .keys()
        .map(String::as_str)
        .filter(|p| is_under(p, root) && *p != root)
        .filter(|p| path(types.resolved(p, "abstract_type")).as_deref() != Some(*p))
        .collect();
    out.sort_unstable();
    out
}

/// Strips BYOND text macros from a name.
pub(crate) fn plain(name: &str) -> String {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
        .to_owned()
}

pub fn resolve(types: &DmTypes) -> ResearchData {
    let materials: Vec<MaterialDef> = subtypes(types, MATERIAL_ROOT)
        .into_iter()
        .filter(|p| num(types.resolved(p, "mat_flags")) as u32 & MATERIAL_SILO_STORED != 0)
        .map(|p| MaterialDef {
            id: p.to_owned(),
            name: text(types.resolved(p, "name")),
            sheet_type: path(types.resolved(p, "sheet_type"))
                .or_else(|| path(types.resolved(p, "ore_type")))
                .filter(|t| types.types.contains_key(t)),
            color: types
                .resolved(p, "color")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
        .collect();
    let silo: BTreeSet<&str> = materials.iter().map(|m| m.id.as_str()).collect();
    let stacks = types
        .types
        .keys()
        .filter(|t| is_under(t, STACK_ROOT))
        .filter_map(|t| {
            let mats: Vec<(String, f64)> = amounts(types.resolved(t, "mats_per_unit"))
                .into_iter()
                .filter(|(m, n)| silo.contains(m.as_str()) && *n > 0.0)
                .collect();
            (!mats.is_empty()).then(|| (t.clone(), mats))
        })
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_iter()
        .collect();
    let designs = subtypes(types, DESIGN_ROOT)
        .into_iter()
        .map(|p| design(types, p))
        .collect();
    let nodes = subtypes(types, NODE_ROOT)
        .into_iter()
        .map(|p| NodeDef {
            id: p.to_owned(),
            name: text(types.resolved(p, "display_name")),
            description: text(types.resolved(p, "description")),
            category: text(types.resolved(p, "category")),
            flags: num(types.resolved(p, "node_flags")) as u32,
            prerequisites: paths(types.resolved(p, "prerequisite_nodes")),
            designs: paths(types.resolved(p, "unlocked_designs")),
            costs: amounts(types.resolved(p, "research_costs")),
            required_experiments: paths(types.resolved(p, "required_experiments")),
            discount_experiments: amounts(types.resolved(p, "discount_experiments")),
            required_items: paths(types.resolved(p, "required_items_to_unlock")),
            announce_channels: texts(types.resolved(p, "announce_channels")),
            experiments_to_unlock: paths(types.resolved(p, "experiments_to_unlock")),
        })
        .collect();
    let lathes = subtypes(types, PRODUCTION_ROOT)
        .into_iter()
        .map(|p| {
            let board = path(types.resolved(p, "circuit"));
            let parts = amounts(
                board
                    .as_deref()
                    .and_then(|b| types.resolved(b, "req_components")),
            );
            let count = |part: &str| {
                parts
                    .iter()
                    .filter(|(k, _)| is_under(k, part))
                    .map(|(_, n)| *n as u32)
                    .sum()
            };
            LatheDef {
                path: p.to_owned(),
                build_types: num(types.resolved(p, "allowed_buildtypes")) as u32,
                department_flags: types
                    .resolved(p, "allowed_department_flags")
                    .and_then(Value::as_f64)
                    .map_or(ALL_FLAGS, |n| n as u32),
                imprinter: is_under(p, IMPRINTER_ROOT),
                servos: count(SERVO),
                matter_bins: count(MATTER_BIN),
                payment_department: text(types.resolved(p, "payment_department")),
            }
        })
        .collect();
    let mut departments: Vec<(u32, String)> = subtypes(types, DEPARTMENT_ROOT)
        .into_iter()
        .filter_map(|p| {
            let flag = num(types.resolved(p, "department_bitflags")) as u32;
            (flag != 0).then(|| (flag, text(types.resolved(p, "department_name"))))
        })
        .collect();
    departments.sort();
    departments.dedup_by_key(|(f, _)| *f);
    let point_items = list(types.resolved(RESEARCH_SS, "techweb_point_items"))
        .into_iter()
        .filter_map(|(k, v)| Some((path(Some(k))?, amounts(v))))
        .collect();
    let consoles = std::iter::once(CONSOLE_ROOT)
        .chain(subtypes(types, CONSOLE_ROOT))
        .filter(|p| types.types.contains_key(*p))
        .map(|p| {
            let board = path(types.resolved(p, "circuit"));
            let locked = board
                .as_deref()
                .is_some_and(|b| num(types.resolved(b, "locked")) != 0.0);
            ConsoleDef {
                path: p.to_owned(),
                locked,
                req_access: texts(types.resolved(p, "req_access")),
            }
        })
        .collect();
    ResearchData {
        nodes,
        designs,
        materials,
        stacks,
        lathes,
        departments,
        point_types: list(types.resolved(RESEARCH_SS, "point_types"))
            .into_iter()
            .filter_map(|(k, v)| Some((k.as_str()?.to_owned(), text(v))))
            .collect(),
        server_income: amounts(types.resolved(RESEARCH_SS, "single_server_income")),
        point_items,
        techweb_bounty: num(types.resolved(ECONOMY_SS, "techweb_bounty")),
        consoles,
        experiments: subtypes(types, EXPERIMENT_ROOT)
            .into_iter()
            .map(|p| experiment(types, p))
            .collect(),
    }
}

/// The checks of `/datum/experiment` subtypes, most specific first; `None`: unsupported.
const EXPERIMENT_KINDS: [(&str, Option<&str>); 16] = [
    (
        "/datum/experiment/scanning/points/machinery_tiered_scan",
        Some("tiered"),
    ),
    (
        "/datum/experiment/scanning/points/machinery_pinpoint_scan",
        Some("pinpoint"),
    ),
    ("/datum/experiment/scanning/points", Some("points")),
    ("/datum/experiment/scanning/reagent", Some("reagent")),
    ("/datum/experiment/scanning/random/plants", None),
    ("/datum/experiment/scanning/random/material", None),
    ("/datum/experiment/scanning/random/mecha_damage_scan", None),
    (
        "/datum/experiment/scanning/random/mecha_equipped_scan",
        None,
    ),
    ("/datum/experiment/scanning/random/cytology", None),
    ("/datum/experiment/scanning/cytology", None),
    ("/datum/experiment/scanning/people", None),
    ("/datum/experiment/scanning/fish", None),
    ("/datum/experiment/scanning", Some("scanning")),
    ("/datum/experiment/autopsy", None),
    ("/datum/experiment/ordnance", None),
    ("/datum/experiment/physical", None),
];

fn experiment(types: &DmTypes, p: &str) -> ExperimentDef {
    let name_of = |t: &str| plain(&text(types.resolved(t, "name")));
    let kind = match EXPERIMENT_KINDS
        .iter()
        .find(|(root, _)| is_under(p, root))
        .and_then(|(_, k)| *k)
    {
        Some("tiered") => ExperimentKind::MachineryTiered {
            tier: num(types.resolved(p, "required_tier")) as u32,
        },
        Some("pinpoint") => {
            let part = path(types.resolved(p, "required_stock_part")).unwrap_or_default();
            ExperimentKind::MachineryPinpoint {
                part_name: name_of(part.as_str()),
                part,
            }
        }
        Some("points") => ExperimentKind::Points,
        Some("reagent") => {
            let reagent = path(types.resolved(p, "required_reagent")).unwrap_or_default();
            ExperimentKind::Reagent {
                reagent_name: name_of(reagent.as_str()),
                reagent,
                min_purity: num(types.resolved(p, "min_purity")),
            }
        }
        Some(_) => ExperimentKind::Scanning,
        None => ExperimentKind::Unsupported,
    };
    let required_atoms: Vec<(String, u32)> = amounts(types.resolved(p, "required_atoms"))
        .into_iter()
        .map(|(t, n)| (t, n as u32))
        .collect();
    let possible_types = paths(types.resolved(p, "possible_types"));
    let names = required_atoms
        .iter()
        .map(|(t, _)| t)
        .chain(&possible_types)
        .map(|t| (t.clone(), name_of(t.as_str())))
        .collect();
    ExperimentDef {
        id: p.to_owned(),
        name: text(types.resolved(p, "name")),
        description: text(types.resolved(p, "description")),
        exp_tag: text(types.resolved(p, "exp_tag")),
        performance_hint: text(types.resolved(p, "performance_hint")),
        traits: num(types.resolved(p, "traits")) as u32,
        kind,
        allowed_experimentors: paths(types.resolved(p, "allowed_experimentors")),
        required_atoms,
        required_points: num(types.resolved(p, "required_points")) as u32,
        possible_types,
        total_requirement: num(types.resolved(p, "total_requirement")) as u32,
        max_requirement_per_type: types
            .resolved(p, "max_requirement_per_type")
            .and_then(Value::as_f64)
            .map_or(100, |n| n as u32),
        scan_message: text(types.resolved(p, "scan_message")),
        points_reward: amounts(types.resolved(p, "points_reward")),
        names,
    }
}

fn design(types: &DmTypes, p: &str) -> DesignDef {
    let build_path = path(types.resolved(p, "build_path"))
        .filter(|t| types.types.contains_key(t) && is_under(t, "/obj"));
    let mut material_requirements = false;
    let materials = amounts(types.resolved(p, "materials"))
        .into_iter()
        .filter(|(m, _)| {
            let plain = is_under(m, MATERIAL_ROOT);
            material_requirements |= !plain;
            plain
        })
        .collect();
    let built_name = |var: &str| {
        build_path
            .as_deref()
            .map(|b| plain(&text(types.resolved(b, var))))
            .unwrap_or_default()
    };
    let name = match plain(&text(types.resolved(p, "name"))) {
        n if n.is_empty() => built_name("name"),
        n => n,
    };
    let desc = match text(types.resolved(p, "desc")) {
        d if d.is_empty() => built_name("desc"),
        d => d,
    };
    DesignDef {
        id: p.to_owned(),
        name,
        desc,
        build_type: num(types.resolved(p, "build_type")) as u32,
        stack: build_path
            .as_deref()
            .is_some_and(|b| is_under(b, STACK_ROOT)),
        build_path,
        materials,
        material_requirements,
        category: texts(types.resolved(p, "category")),
        departmental_flags: types
            .resolved(p, "departmental_flags")
            .and_then(Value::as_f64)
            .map_or(ALL_FLAGS, |n| n as u32),
        construction_time: types
            .resolved(p, "construction_time")
            .and_then(Value::as_f64)
            .unwrap_or(32.0),
        lathe_time_factor: types
            .resolved(p, "lathe_time_factor")
            .and_then(Value::as_f64)
            .unwrap_or(1.0),
        fixed_cost_efficiency: num(types.resolved(p, "fixed_cost_efficiency")) != 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/datum/material": { "parent": "/datum", "declared_vars": {
            "abstract_type": { "path": "/datum/material" }, "mat_flags": 0, "name": "material",
            "sheet_type": null, "ore_type": null, "color": null } },
        "/datum/material/iron": { "parent": "/datum/material", "declared_vars": {
            "mat_flags": 23, "name": "iron", "color": "#B6BEC2",
            "sheet_type": { "path": "/obj/item/stack/sheet/iron" } } },
        "/datum/material/meat": { "parent": "/datum/material", "declared_vars": {
            "mat_flags": 0, "name": "meat" } },
        "/datum/design": { "parent": "/datum", "declared_vars": { "name": null, "desc": null,
            "build_type": null, "build_path": null, "materials": { "list": [] },
            "category": { "list": [] }, "departmental_flags": 16777215,
            "construction_time": 32, "lathe_time_factor": 1, "fixed_cost_efficiency": 0 } },
        "/datum/design/board": { "parent": "/datum/design", "declared_vars": {
            "abstract_type": { "path": "/datum/design/board" }, "build_type": 1 } },
        "/datum/design/board/thing": { "parent": "/datum/design/board", "declared_vars": {
            "name": "Thing Board", "build_path": { "path": "/obj/item/circuitboard/thing" },
            "materials": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 50 ] },
                { "assoc": [ { "path": "/datum/material_requirement/any" }, 10 ] } ] },
            "category": { "list": [ "/Boards" ] }, "departmental_flags": 16 } },
        "/datum/techweb_node": { "parent": "/datum", "declared_vars": { "display_name": null,
            "description": null, "category": "Misc", "node_flags": 8,
            "prerequisite_nodes": null, "unlocked_designs": null, "research_costs": null,
            "required_experiments": null, "discount_experiments": null,
            "required_items_to_unlock": null, "announce_channels": null } },
        "/datum/techweb_node/base": { "parent": "/datum/techweb_node", "declared_vars": {
            "display_name": "Base", "node_flags": 9 } },
        "/datum/techweb_node/next": { "parent": "/datum/techweb_node", "declared_vars": {
            "display_name": "Next",
            "prerequisite_nodes": { "list": [ { "path": "/datum/techweb_node/base" } ] },
            "unlocked_designs": { "list": [ { "path": "/datum/design/board/thing" } ] },
            "research_costs": { "list": [ { "assoc": [ "General Research", 40 ] } ] } } },
        "/datum/controller": { "parent": "/datum" },
        "/datum/controller/subsystem": { "parent": "/datum/controller" },
        "/datum/controller/subsystem/research": { "parent": "/datum/controller/subsystem",
            "declared_vars": {
                "point_types": { "list": [ { "assoc": [ "General Research", "Gen. Res." ] } ] },
                "single_server_income": { "list": [ { "assoc": [ "General Research", 1 ] } ] } } },
        "/datum/controller/subsystem/economy": { "parent": "/datum/controller/subsystem",
            "declared_vars": { "techweb_bounty": 250 } },
        "/datum/stock_part": { "parent": "/datum" },
        "/datum/stock_part/servo": { "parent": "/datum/stock_part" },
        "/datum/stock_part/matter_bin": { "parent": "/datum/stock_part" },
        "/obj": { "parent": null },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/circuitboard": { "parent": "/obj/item" },
        "/obj/item/circuitboard/thing": { "parent": "/obj/item/circuitboard", "declared_vars": {
            "desc": "A board." } },
        "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard" },
        "/obj/item/circuitboard/machine/protolathe": { "parent": "/obj/item/circuitboard/machine",
            "declared_vars": { "req_components": { "list": [
                { "assoc": [ { "path": "/datum/stock_part/matter_bin" }, 2 ] },
                { "assoc": [ { "path": "/datum/stock_part/servo" }, 2 ] } ] } } },
        "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "mats_per_unit": null } },
        "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
        "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
            "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 100 ] } ] } } },
        "/obj/machinery": { "parent": "/obj" },
        "/obj/machinery/rnd": { "parent": "/obj/machinery" },
        "/obj/machinery/rnd/production": { "parent": "/obj/machinery/rnd", "declared_vars": {
            "allowed_buildtypes": 0, "allowed_department_flags": 16777215 } },
        "/obj/machinery/rnd/production/protolathe": { "parent": "/obj/machinery/rnd/production",
            "declared_vars": { "allowed_buildtypes": 2,
                "circuit": { "path": "/obj/item/circuitboard/machine/protolathe" } } }
    } }"##;

    #[test]
    fn resolves_nodes_designs_materials_and_lathes() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let data = resolve(&types);
        assert_eq!(data.materials.len(), 1, "only silo-stored materials");
        assert_eq!(
            data.materials[0].sheet_type.as_deref(),
            Some("/obj/item/stack/sheet/iron")
        );
        assert_eq!(
            data.stacks,
            vec![(
                "/obj/item/stack/sheet/iron".to_owned(),
                vec![("/datum/material/iron".to_owned(), 100.0)]
            )]
        );
        assert_eq!(data.designs.len(), 1, "abstract designs are left out");
        let board = &data.designs[0];
        assert_eq!(board.build_type, 1);
        assert_eq!(board.desc, "A board.");
        assert_eq!(board.departmental_flags, 16);
        assert_eq!(board.materials, vec![("/datum/material/iron".into(), 50.0)]);
        assert!(board.material_requirements);
        assert_eq!(data.nodes.len(), 2);
        let next = data.nodes.iter().find(|n| n.name == "Next").unwrap();
        assert_eq!(next.prerequisites, vec!["/datum/techweb_node/base"]);
        assert_eq!(next.costs, vec![("General Research".into(), 40.0)]);
        assert_eq!(data.nodes[0].flags & node_flags::STARTER, 1);
        let lathe = &data.lathes[0];
        assert_eq!(
            (lathe.servos, lathe.matter_bins, lathe.imprinter),
            (2, 2, false)
        );
        assert_eq!(lathe.department_flags, ALL_FLAGS);
        assert_eq!(data.server_income, vec![("General Research".into(), 1.0)]);
        assert_eq!(data.techweb_bounty, 250.0);
        let back: ResearchData = ron::from_str(&data.to_ron()).unwrap();
        assert_eq!(back, data);
    }

    const EXPERIMENTS: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/datum/experiment": { "parent": "/datum", "declared_vars": { "name": "Experiment",
            "description": "Base", "exp_tag": "Base", "traits": null,
            "allowed_experimentors": null, "performance_hint": null, "points_reward": null } },
        "/datum/experiment/scanning": { "parent": "/datum/experiment", "declared_vars": {
            "exp_tag": "Scan", "required_atoms": { "list": [] }, "scan_message": null,
            "allowed_experimentors": { "list": [ { "path": "/obj/item/experi_scanner" } ] } } },
        "/datum/experiment/scanning/points": { "parent": "/datum/experiment/scanning",
            "declared_vars": { "required_points": null } },
        "/datum/experiment/scanning/points/bluespace_crystal": {
            "parent": "/datum/experiment/scanning/points", "declared_vars": {
            "name": "Bluespace", "required_points": 1, "required_atoms": { "list": [
                { "assoc": [ { "path": "/obj/item/crystal" }, 1 ] } ] } } },
        "/datum/experiment/scanning/points/machinery_tiered_scan": {
            "parent": "/datum/experiment/scanning/points", "declared_vars": { "required_tier": 1 } },
        "/datum/experiment/scanning/points/machinery_tiered_scan/tier2_any": {
            "parent": "/datum/experiment/scanning/points/machinery_tiered_scan",
            "declared_vars": { "required_points": 6, "required_tier": 2 } },
        "/datum/experiment/scanning/reagent": { "parent": "/datum/experiment/scanning",
            "declared_vars": { "required_reagent": { "path": "/datum/reagent/water" },
            "min_purity": 0 } },
        "/datum/experiment/scanning/reagent/haloperidol": {
            "parent": "/datum/experiment/scanning/reagent", "declared_vars": {
            "required_reagent": { "path": "/datum/reagent/medicine/haloperidol" },
            "min_purity": 0.98 } },
        "/datum/experiment/scanning/random": { "parent": "/datum/experiment/scanning",
            "declared_vars": { "possible_types": { "list": [] }, "total_requirement": 0,
            "max_requirement_per_type": 100 } },
        "/datum/experiment/scanning/random/janitor_trash": {
            "parent": "/datum/experiment/scanning/random", "declared_vars": {
            "possible_types": { "list": [ { "path": "/obj/effect/decal/cleanable/blood" } ] },
            "total_requirement": 3 } },
        "/datum/experiment/scanning/people": { "parent": "/datum/experiment/scanning" },
        "/datum/experiment/ordnance": { "parent": "/datum/experiment" },
        "/datum/reagent": { "parent": "/datum" },
        "/datum/reagent/medicine": { "parent": "/datum/reagent" },
        "/datum/reagent/medicine/haloperidol": { "parent": "/datum/reagent/medicine",
            "declared_vars": { "name": "Haloperidol" } },
        "/obj": { "parent": "/datum" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/crystal": { "parent": "/obj/item", "declared_vars": {
            "name": "\\improper crystal" } },
        "/obj/effect": { "parent": "/obj" },
        "/obj/effect/decal": { "parent": "/obj/effect" },
        "/obj/effect/decal/cleanable": { "parent": "/obj/effect/decal" },
        "/obj/effect/decal/cleanable/blood": { "parent": "/obj/effect/decal/cleanable",
            "declared_vars": { "name": "blood" } }
    } }"##;

    #[test]
    fn resolves_experiments() {
        let types = DmTypes::parse(EXPERIMENTS.as_bytes()).unwrap();
        let data = resolve(&types);
        let get = |id: &str| data.experiments.iter().find(|e| e.id == id).unwrap();
        let crystal = get("/datum/experiment/scanning/points/bluespace_crystal");
        assert_eq!(crystal.kind, ExperimentKind::Points);
        assert_eq!(crystal.required_points, 1);
        assert_eq!(
            crystal.required_atoms,
            vec![("/obj/item/crystal".to_owned(), 1)]
        );
        assert_eq!(
            crystal.names,
            vec![("/obj/item/crystal".to_owned(), "crystal".to_owned())]
        );
        assert_eq!(crystal.exp_tag, "Scan");
        assert_eq!(
            crystal.allowed_experimentors,
            vec!["/obj/item/experi_scanner"]
        );
        assert_eq!(
            get("/datum/experiment/scanning/points/machinery_tiered_scan/tier2_any").kind,
            ExperimentKind::MachineryTiered { tier: 2 }
        );
        assert_eq!(
            get("/datum/experiment/scanning/reagent/haloperidol").kind,
            ExperimentKind::Reagent {
                reagent: "/datum/reagent/medicine/haloperidol".into(),
                reagent_name: "Haloperidol".into(),
                min_purity: 0.98,
            }
        );
        let trash = get("/datum/experiment/scanning/random/janitor_trash");
        assert_eq!(trash.kind, ExperimentKind::Scanning);
        assert_eq!(
            (trash.total_requirement, trash.max_requirement_per_type),
            (3, 100)
        );
        assert_eq!(
            get("/datum/experiment/scanning/people").kind,
            ExperimentKind::Unsupported
        );
        assert_eq!(
            get("/datum/experiment/ordnance").kind,
            ExperimentKind::Unsupported
        );
        let back: ResearchData = ron::from_str(&data.to_ron()).unwrap();
        assert_eq!(back, data);
    }
}
