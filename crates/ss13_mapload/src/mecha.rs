//! Exosuit data from the DM dump, written by `ss13-mapload convert` to [`MECHA_FILE`]: the mechs
//! (`/obj/vehicle/sealed/mecha/*`, `code/modules/vehicles/mecha/`), their equipment
//! (`/obj/item/mecha_parts/mecha_equipment/*`), the construction chains
//! (`/datum/component/construction/unordered/mecha_chassis/*` and
//! `/datum/component/construction/mecha/*`, `mecha_construction_paths.dm`) with the chassis that
//! start them (`/obj/item/mecha_parts/chassis/*`), the wreckages
//! (`/obj/structure/mecha_wreckage/*`), the power cells (`/obj/item/stock_parts/power_store/*`)
//! and the exosuit fabricators (`/obj/machinery/mecha_part_fabricator`, `mech_fabricator.dm`).
//! The game (`ss13_game::mecha`) reads it at startup.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::is_under;
use crate::research::{amounts, list, path, paths, plain, subtypes, text, texts};

/// `<assets>/mecha.ron`.
pub const MECHA_FILE: &str = "mecha.ron";
pub const MECHA_ROOT: &str = "/obj/vehicle/sealed/mecha";
pub const EQUIPMENT_ROOT: &str = "/obj/item/mecha_parts/mecha_equipment";
pub const CHASSIS_ROOT: &str = "/obj/item/mecha_parts/chassis";
pub const WRECKAGE_ROOT: &str = "/obj/structure/mecha_wreckage";
pub const CELL_ROOT: &str = "/obj/item/stock_parts/power_store";
pub const FAB_ROOT: &str = "/obj/machinery/mecha_part_fabricator";
pub const STOCK_PART_ROOT: &str = "/obj/item/stock_parts";
const SERVO: &str = "/datum/stock_part/servo";
const MATTER_BIN: &str = "/datum/stock_part/matter_bin";
const MICRO_LASER: &str = "/datum/stock_part/micro_laser";

/// `/datum/armor` ratings, percent.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Armor {
    pub melee: f32,
    pub bullet: f32,
    pub laser: f32,
    pub energy: f32,
    pub bomb: f32,
    pub fire: f32,
    pub acid: f32,
}

impl Armor {
    /// The rating of an armor flag (`MELEE`, `BULLET`, `LASER`, `ENERGY`, `BOMB`, `FIRE`, `ACID`).
    pub fn rating(&self, flag: &str) -> f32 {
        match flag {
            "melee" => self.melee,
            "bullet" => self.bullet,
            "laser" => self.laser,
            "energy" => self.energy,
            "bomb" => self.bomb,
            "fire" => self.fire,
            "acid" => self.acid,
            _ => 0.0,
        }
    }
}

/// An `/obj/vehicle/sealed/mecha` type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MechDef {
    pub path: String,
    pub name: String,
    pub icon: String,
    pub base_icon_state: String,
    /// Deciseconds per step.
    pub movedelay: f32,
    /// Joules.
    pub step_energy_drain: f64,
    pub melee_energy_drain: f64,
    pub max_integrity: f32,
    pub armor: Armor,
    /// `facing_modifiers`: front, side, back.
    pub facing: [f32; 3],
    /// Deciseconds.
    pub enter_delay: f32,
    pub exit_delay: f32,
    pub wreckage: Option<String>,
    pub accesses: Vec<String>,
    pub mecha_flags: u32,
    pub mech_type: u32,
    /// `max_equip_by_category`: `mecha_l_arm`, `mecha_r_arm`, `mecha_utility`, ...
    pub max_equip: Vec<(String, u32)>,
    /// `equip_by_category` before `Initialize`: (category, equipment type).
    pub equipment: Vec<(String, String)>,
    pub pivot_step: bool,
    pub allow_diagonal: bool,
    /// Ripleys: `fast_pressure_step_in` and `slow_pressure_step_in` (`update_pressure`).
    pub pressure_step_in: Option<(f32, f32)>,
    /// The type's own procs (overrides of `/obj/vehicle/sealed/mecha` behaviour).
    pub procs: Vec<String>,
}

/// An `/obj/item/mecha_parts/mecha_equipment` type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct EquipDef {
    pub path: String,
    pub name: String,
    /// `equipment_slot`: `mecha_weapon` (an arm), `mecha_utility`, `mecha_power`, `mecha_armor`.
    pub slot: String,
    /// Deciseconds.
    pub equip_cooldown: f32,
    pub energy_drain: f64,
    /// `MECHA_MELEE` 1, `MECHA_RANGED` 2.
    pub range: u32,
    pub mech_flags: u32,
    pub harmful: bool,
    pub can_be_toggled: bool,
    pub detachable: bool,
    pub unstackable: bool,
    /// The cargo compartment's `cargo_capacity`.
    pub cargo_capacity: u32,
    /// Drills: `drill_delay` (deciseconds) and `drill_level`.
    pub drill_delay: f32,
    pub drill_level: u32,
    /// Clamps: `clamp_damage`, `killer_clamp`.
    pub clamp_damage: f32,
    pub killer_clamp: bool,
}

/// A chassis and its two construction components.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ConstructionDef {
    /// `/obj/item/mecha_parts/chassis/*`.
    pub chassis: String,
    /// The unordered component's `steps`: the parts, any order.
    pub parts: Vec<String>,
    /// `/datum/component/construction/mecha/*`.
    pub path: String,
    /// The mech type it builds.
    pub result: String,
    pub base_icon: String,
    pub circuit_control: String,
    pub circuit_periph: String,
    pub circuit_weapon: Option<String>,
    pub inner_plating: String,
    pub inner_plating_amount: u32,
    /// `initial(inner_plating.name)`.
    pub inner_plating_name: String,
    pub outer_plating: String,
    pub outer_plating_amount: u32,
    pub outer_plating_name: String,
    /// Step procs the type overrides (`get_outer_plating_steps`, ...).
    pub procs: Vec<String>,
}

/// An `/obj/structure/mecha_wreckage` type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct WreckageDef {
    pub path: String,
    pub parts: Vec<String>,
    pub welder_salvage: Vec<String>,
    pub salvage_num: u32,
}

/// A power cell type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CellDef {
    pub path: String,
    /// Joules.
    pub maxcharge: f64,
    /// Watts.
    pub chargerate: f64,
    /// Starts at 0 (`empty`).
    pub empty: bool,
}

/// An exosuit fabricator type: its board's stock parts (tier 1) and `link_on_init`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct FabDef {
    pub path: String,
    pub servos: u32,
    pub micro_lasers: u32,
    pub matter_bins: u32,
    pub link_on_init: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MechaData {
    pub mechs: Vec<MechDef>,
    pub equipment: Vec<EquipDef>,
    pub constructions: Vec<ConstructionDef>,
    pub wreckages: Vec<WreckageDef>,
    pub cells: Vec<CellDef>,
    pub fabs: Vec<FabDef>,
    /// `/obj/item/stock_parts/*` -> `rating` (servos, capacitors, scanning modules, ...).
    pub stock_parts: Vec<(String, u32)>,
}

impl MechaData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let text = ss13_data::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        ron::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("mecha data serializes"),
        );
        out.push('\n');
        out
    }

    /// The types the game spawns: mechs, their wreckages and default equipment, chassis, parts,
    /// boards, plating, cells.
    pub fn prototype_types(&self) -> BTreeSet<&str> {
        let mut out = BTreeSet::new();
        for m in &self.mechs {
            out.insert(m.path.as_str());
            out.extend(m.wreckage.as_deref());
            out.extend(m.equipment.iter().map(|(_, e)| e.as_str()));
        }
        for c in &self.constructions {
            out.insert(c.chassis.as_str());
            out.extend(c.parts.iter().map(String::as_str));
            out.extend([
                c.circuit_control.as_str(),
                c.circuit_periph.as_str(),
                c.inner_plating.as_str(),
                c.outer_plating.as_str(),
            ]);
            out.extend(c.circuit_weapon.as_deref());
        }
        for w in &self.wreckages {
            out.extend(w.parts.iter().map(String::as_str));
            out.extend(w.welder_salvage.iter().map(String::as_str));
        }
        out.extend(self.equipment.iter().map(|e| e.path.as_str()));
        out.extend(self.cells.iter().map(|c| c.path.as_str()));
        out.retain(|p| !p.is_empty());
        out
    }
}

/// A number, also from a constant expression the extractor kept as text (`{"expr": "40"}`).
fn number(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::Object(o)) => o
            .get("expr")
            .and_then(Value::as_str)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

fn type_path(v: Option<&Value>) -> Option<String> {
    path(v).or_else(|| {
        v?.get("expr")?
            .as_str()
            .filter(|s| s.starts_with('/'))
            .map(str::to_owned)
    })
}

/// A display name: text macros and DM's escaped quotes removed.
fn name(types: &DmTypes, p: &str) -> String {
    plain(&text(types.resolved(p, "name"))).replace("\\\"", "\"")
}

fn armor(types: &DmTypes, p: &str) -> Armor {
    let Some(datum) = type_path(types.resolved(p, "armor_type")) else {
        return Armor::default();
    };
    let r = |flag: &str| number(types.resolved(&datum, flag)) as f32;
    Armor {
        melee: r("melee"),
        bullet: r("bullet"),
        laser: r("laser"),
        energy: r("energy"),
        bomb: r("bomb"),
        fire: r("fire"),
        acid: r("acid"),
    }
}

fn own_procs(types: &DmTypes, p: &str) -> Vec<String> {
    types
        .types
        .get(p)
        .map(|t| t.procs.clone())
        .unwrap_or_default()
}

fn mech(types: &DmTypes, p: &str) -> MechDef {
    let facing_list = amounts(types.resolved(p, "facing_modifiers"));
    let facing = |key: &str, default: f32| {
        facing_list
            .iter()
            .find(|(k, _)| k == key)
            .map_or(default, |(_, n)| *n as f32)
    };
    let max_equip = amounts(types.resolved(p, "max_equip_by_category"))
        .into_iter()
        .map(|(k, n)| (k, n as u32))
        .collect();
    let equipment = list(types.resolved(p, "equip_by_category"))
        .into_iter()
        .filter_map(|(k, v)| Some((k.as_str()?.to_owned(), v?)))
        .flat_map(|(category, v)| {
            let mut items = paths(Some(v));
            items.extend(path(Some(v)));
            items.into_iter().map(move |e| (category.clone(), e))
        })
        .filter(|(_, e)| types.types.contains_key(e))
        .collect();
    let pressure = (types.resolved(p, "slow_pressure_step_in").is_some()).then(|| {
        (
            number(types.resolved(p, "fast_pressure_step_in")) as f32,
            number(types.resolved(p, "slow_pressure_step_in")) as f32,
        )
    });
    MechDef {
        path: p.to_owned(),
        name: name(types, p),
        icon: types
            .resolved(p, "icon")
            .and_then(|v| v.get("resource"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        base_icon_state: text(types.resolved(p, "base_icon_state")),
        movedelay: number(types.resolved(p, "movedelay")) as f32,
        step_energy_drain: number(types.resolved(p, "step_energy_drain")),
        melee_energy_drain: number(types.resolved(p, "melee_energy_drain")),
        max_integrity: number(types.resolved(p, "max_integrity")) as f32,
        armor: armor(types, p),
        facing: [
            facing("mechafront", 0.5),
            facing("mechaside", 1.0),
            facing("mechaback", 1.5),
        ],
        enter_delay: number(types.resolved(p, "enter_delay")) as f32,
        exit_delay: number(types.resolved(p, "exit_delay")) as f32,
        wreckage: type_path(types.resolved(p, "wreckage")).filter(|w| types.types.contains_key(w)),
        accesses: texts(types.resolved(p, "accesses")),
        mecha_flags: number(types.resolved(p, "mecha_flags")) as u32,
        mech_type: number(types.resolved(p, "mech_type")) as u32,
        max_equip,
        equipment,
        pivot_step: number(types.resolved(p, "pivot_step")) != 0.0,
        allow_diagonal: number(types.resolved(p, "allow_diagonal_movement")) != 0.0,
        pressure_step_in: pressure,
        procs: own_procs(types, p),
    }
}

fn equipment(types: &DmTypes, p: &str) -> EquipDef {
    let n = |var: &str| number(types.resolved(p, var));
    EquipDef {
        path: p.to_owned(),
        name: name(types, p),
        slot: text(types.resolved(p, "equipment_slot")),
        equip_cooldown: n("equip_cooldown") as f32,
        energy_drain: n("energy_drain"),
        range: n("range") as u32,
        mech_flags: n("mech_flags") as u32,
        harmful: n("harmful") != 0.0,
        can_be_toggled: n("can_be_toggled") != 0.0,
        detachable: types.resolved(p, "detachable").is_none() || n("detachable") != 0.0,
        unstackable: n("unstackable") != 0.0,
        cargo_capacity: n("cargo_capacity") as u32,
        drill_delay: n("drill_delay") as f32,
        drill_level: n("drill_level") as u32,
        clamp_damage: n("clamp_damage") as f32,
        killer_clamp: n("killer_clamp") != 0.0,
    }
}

fn construction(types: &DmTypes, chassis: &str) -> Option<ConstructionDef> {
    let unordered = type_path(types.resolved(chassis, "construct_type"))?;
    let path = type_path(types.resolved(&unordered, "result"))?;
    let item = |var: &str| type_path(types.resolved(&path, var)).unwrap_or_default();
    let (inner, outer) = (item("inner_plating"), item("outer_plating"));
    Some(ConstructionDef {
        chassis: chassis.to_owned(),
        parts: paths(types.resolved(&unordered, "steps")),
        result: item("result"),
        base_icon: text(types.resolved(&path, "base_icon")),
        circuit_control: item("circuit_control"),
        circuit_periph: item("circuit_periph"),
        circuit_weapon: type_path(types.resolved(&path, "circuit_weapon")),
        inner_plating_amount: number(types.resolved(&path, "inner_plating_amount")) as u32,
        inner_plating_name: name(types, &inner),
        inner_plating: inner,
        outer_plating_amount: number(types.resolved(&path, "outer_plating_amount")) as u32,
        outer_plating_name: name(types, &outer),
        outer_plating: outer,
        procs: own_procs(types, &path),
        path,
    })
}

fn count_parts(types: &DmTypes, machine: &str, part: &str) -> u32 {
    let board = type_path(types.resolved(machine, "circuit"));
    amounts(
        board
            .as_deref()
            .and_then(|b| types.resolved(b, "req_components")),
    )
    .iter()
    .filter(|(k, _)| is_under(k, part))
    .map(|(_, n)| *n as u32)
    .sum()
}

pub fn resolve(types: &DmTypes) -> MechaData {
    let mechs = subtypes(types, MECHA_ROOT)
        .into_iter()
        .map(|p| mech(types, p))
        .collect();
    let equipment = subtypes(types, EQUIPMENT_ROOT)
        .into_iter()
        .map(|p| equipment(types, p))
        .collect();
    let constructions = subtypes(types, CHASSIS_ROOT)
        .into_iter()
        .filter_map(|p| construction(types, p))
        .collect();
    let wreckages = subtypes(types, WRECKAGE_ROOT)
        .into_iter()
        .map(|p| WreckageDef {
            path: p.to_owned(),
            parts: paths(types.resolved(p, "parts")),
            welder_salvage: paths(types.resolved(p, "welder_salvage")),
            salvage_num: number(types.resolved(p, "salvage_num")) as u32,
        })
        .collect();
    let cells = std::iter::once(CELL_ROOT)
        .chain(subtypes(types, CELL_ROOT))
        .filter(|p| types.types.contains_key(*p))
        .map(|p| CellDef {
            path: p.to_owned(),
            maxcharge: number(types.resolved(p, "maxcharge")),
            chargerate: number(types.resolved(p, "chargerate")),
            empty: number(types.resolved(p, "empty")) != 0.0,
        })
        .collect();
    let fabs = std::iter::once(FAB_ROOT)
        .chain(subtypes(types, FAB_ROOT))
        .filter(|p| types.types.contains_key(*p))
        .map(|p| FabDef {
            path: p.to_owned(),
            servos: count_parts(types, p, SERVO),
            micro_lasers: count_parts(types, p, MICRO_LASER),
            matter_bins: count_parts(types, p, MATTER_BIN),
            link_on_init: types.resolved(p, "link_on_init").is_none()
                || number(types.resolved(p, "link_on_init")) != 0.0,
        })
        .collect();
    let stock_parts = subtypes(types, STOCK_PART_ROOT)
        .into_iter()
        .filter(|p| !is_under(p, CELL_ROOT))
        .map(|p| {
            (
                p.to_owned(),
                number(types.resolved(p, "rating")).max(1.0) as u32,
            )
        })
        .collect();
    MechaData {
        mechs,
        equipment,
        constructions,
        wreckages,
        cells,
        fabs,
        stock_parts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/datum/armor": { "parent": "/datum" },
        "/datum/armor/mecha_ripley": { "parent": "/datum/armor", "declared_vars": {
            "melee": { "expr": "40" }, "bullet": { "expr": "20" }, "laser": { "expr": "10" } } },
        "/datum/component": { "parent": "/datum" },
        "/datum/component/construction": { "parent": "/datum/component" },
        "/datum/component/construction/unordered": { "parent": "/datum/component/construction" },
        "/datum/component/construction/unordered/mecha_chassis": {
            "parent": "/datum/component/construction/unordered" },
        "/datum/component/construction/unordered/mecha_chassis/ripley": {
            "parent": "/datum/component/construction/unordered/mecha_chassis",
            "declared_vars": {
                "result": { "path": "/datum/component/construction/mecha/ripley" },
                "steps": { "list": [ { "path": "/obj/item/mecha_parts/part/ripley_torso" } ] } } },
        "/datum/component/construction/mecha": { "parent": "/datum/component/construction" },
        "/datum/component/construction/mecha/ripley": {
            "parent": "/datum/component/construction/mecha",
            "procs": [ "get_outer_plating_steps" ],
            "declared_vars": { "base_icon": "ripley",
                "result": { "path": "/obj/vehicle/sealed/mecha/ripley" },
                "circuit_control": { "path": "/obj/item/circuitboard/mecha/ripley/main" },
                "circuit_periph": { "path": "/obj/item/circuitboard/mecha/ripley/peripherals" },
                "inner_plating": { "path": "/obj/item/stack/sheet/iron" },
                "inner_plating_amount": 5,
                "outer_plating": { "path": "/obj/item/stack/rods" },
                "outer_plating_amount": 10 } },
        "/obj": { "parent": null },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/stack": { "parent": "/obj/item" },
        "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
        "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
            "name": "iron" } },
        "/obj/item/stack/rods": { "parent": "/obj/item/stack", "declared_vars": {
            "name": "iron rod" } },
        "/obj/item/mecha_parts": { "parent": "/obj/item" },
        "/obj/item/mecha_parts/chassis": { "parent": "/obj/item/mecha_parts" },
        "/obj/item/mecha_parts/chassis/ripley": { "parent": "/obj/item/mecha_parts/chassis",
            "declared_vars": { "construct_type": {
                "path": "/datum/component/construction/unordered/mecha_chassis/ripley" } } },
        "/obj/item/mecha_parts/mecha_equipment": { "parent": "/obj/item/mecha_parts",
            "declared_vars": { "abstract_type": { "path": "/obj/item/mecha_parts/mecha_equipment" },
                "equipment_slot": "mecha_weapon", "equip_cooldown": 0, "range": 1,
                "mech_flags": 16777215, "detachable": 1 } },
        "/obj/item/mecha_parts/mecha_equipment/hydraulic_clamp": {
            "parent": "/obj/item/mecha_parts/mecha_equipment", "declared_vars": {
                "name": "hydraulic clamp", "equip_cooldown": 15, "energy_drain": 100,
                "mech_flags": 1, "harmful": 1, "clamp_damage": 20 } },
        "/obj/item/mecha_parts/mecha_equipment/ejector": {
            "parent": "/obj/item/mecha_parts/mecha_equipment", "declared_vars": {
                "equipment_slot": "mecha_utility", "detachable": 0, "cargo_capacity": 15 } },
        "/obj/vehicle": { "parent": "/obj", "declared_vars": { "movedelay": 2 } },
        "/obj/vehicle/sealed": { "parent": "/obj/vehicle", "declared_vars": { "enter_delay": 20 } },
        "/obj/vehicle/sealed/mecha": { "parent": "/obj/vehicle/sealed", "declared_vars": {
            "icon": { "resource": "icons/mob/rideables/mecha.dmi" }, "movedelay": 10,
            "step_energy_drain": 80, "max_integrity": 300, "exit_delay": 20,
            "allow_diagonal_movement": 1,
            "facing_modifiers": { "list": [ { "assoc": [ "mechafront", 0.5 ] },
                { "assoc": [ "mechaside", 1 ] }, { "assoc": [ "mechaback", 1.5 ] } ] } } },
        "/obj/vehicle/sealed/mecha/ripley": { "parent": "/obj/vehicle/sealed/mecha",
            "procs": [ "Move", "update_pressure" ],
            "declared_vars": { "name": "\\improper APLU MK-I \\\"Ripley\\\"",
                "base_icon_state": "ripley", "movedelay": 1.5, "max_integrity": 200,
                "armor_type": { "expr": "/datum/armor/mecha_ripley" }, "enter_delay": 10,
                "accesses": { "list": [ "mech_engine" ] }, "mech_type": 1,
                "fast_pressure_step_in": 1.5, "slow_pressure_step_in": 2,
                "wreckage": { "path": "/obj/structure/mecha_wreckage/ripley" },
                "equip_by_category": { "list": [ { "assoc": [ "mecha_l_arm", null ] },
                    { "assoc": [ "mecha_utility", { "list": [
                        { "path": "/obj/item/mecha_parts/mecha_equipment/ejector" } ] } ] } ] } } },
        "/obj/structure": { "parent": "/obj" },
        "/obj/structure/mecha_wreckage": { "parent": "/obj/structure", "declared_vars": {
            "salvage_num": 5 } },
        "/obj/structure/mecha_wreckage/ripley": { "parent": "/obj/structure/mecha_wreckage",
            "declared_vars": { "parts": { "list": [
                { "path": "/obj/item/mecha_parts/part/ripley_torso" } ] } } }
    } }"##;

    #[test]
    fn resolves_mechs_equipment_and_construction() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let data = resolve(&types);
        let ripley = &data.mechs[0];
        assert_eq!(ripley.name, "APLU MK-I \"Ripley\"");
        assert_eq!(ripley.movedelay, 1.5);
        assert_eq!(ripley.step_energy_drain, 80.0, "inherited");
        assert_eq!(ripley.armor.melee, 40.0);
        assert_eq!(ripley.armor.rating("laser"), 10.0);
        assert_eq!(ripley.facing, [0.5, 1.0, 1.5]);
        assert_eq!(ripley.enter_delay, 10.0);
        assert_eq!(ripley.exit_delay, 20.0);
        assert_eq!(ripley.pressure_step_in, Some((1.5, 2.0)));
        assert_eq!(
            ripley.equipment,
            vec![(
                "mecha_utility".to_owned(),
                "/obj/item/mecha_parts/mecha_equipment/ejector".to_owned()
            )]
        );
        assert_eq!(data.equipment.len(), 2, "the abstract root is left out");
        let clamp = data
            .equipment
            .iter()
            .find(|e| e.name == "hydraulic clamp")
            .unwrap();
        assert_eq!(
            (clamp.slot.as_str(), clamp.equip_cooldown),
            ("mecha_weapon", 15.0)
        );
        assert!(clamp.detachable && clamp.harmful);
        let ejector = data
            .equipment
            .iter()
            .find(|e| e.cargo_capacity > 0)
            .unwrap();
        assert!(!ejector.detachable);
        let c = &data.constructions[0];
        assert_eq!(c.result, "/obj/vehicle/sealed/mecha/ripley");
        assert_eq!(c.parts, vec!["/obj/item/mecha_parts/part/ripley_torso"]);
        assert_eq!(
            (c.inner_plating_amount, c.inner_plating_name.as_str()),
            (5, "iron")
        );
        assert_eq!(c.outer_plating_name, "iron rod");
        assert_eq!(c.procs, vec!["get_outer_plating_steps"]);
        assert_eq!(data.wreckages[0].salvage_num, 5);
        assert!(
            data.prototype_types()
                .contains("/obj/structure/mecha_wreckage/ripley")
        );
        let back: MechaData = ron::from_str(&data.to_ron()).unwrap();
        assert_eq!(back, data);
    }
}
