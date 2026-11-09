//! Engineering odds and ends as prototype data: conveyor belts and switches
//! (`code/modules/recycling/conveyor.dm`), fire extinguishers (`extinguisher.dm`), what a recycler
//! gets back from an item (`custom_materials`, `mats_per_unit`), and the disposal pipe recipes
//! (`GLOB.disposal_pipe_recipes`). `ss13_game::recycling`, `ss13_game::extinguisher` build the
//! live state from these.
//!
//! Map var edits of a belt's or switch's `id`, `speed`, `flipped`, `inverted`, `oneway`,
//! `invert_icon` and `conveyor_speed` are supported.

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::door::link_id;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::research::amounts;
use crate::value::{Field, Skip, Val};

pub const CONVEYOR_ROOT: &str = "/obj/machinery/conveyor";
pub const CONVEYOR_SWITCH_ROOT: &str = "/obj/machinery/conveyor_switch";
pub const CONVEYOR_STACK: &str = "/obj/item/stack/conveyor";
pub const CONVEYOR_SWITCH_ITEM: &str = "/obj/item/conveyor_switch_construct";
pub const RECYCLER_ROOT: &str = "/obj/machinery/recycler";
pub const FLAPS_ROOT: &str = "/obj/structure/plasticflaps";
pub const EXTINGUISHER_ROOT: &str = "/obj/item/extinguisher";
pub const DISPOSAL_CONSTRUCT: &str = "/obj/structure/disposalconstruct";
pub const EXTINGUISHER_WATER: &str = "/obj/effect/particle_effect/water/extinguisher";

const COMPONENT: &str = "dm_conveyor";
const EXTINGUISHER_COMPONENT: &str = "dm_extinguisher";
const MATERIALS_COMPONENT: &str = "dm_materials";

/// One `/datum/pipe_info/disposal`: its button and the type it makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisposalRecipe {
    pub name: &'static str,
    pub path: &'static str,
    /// `PIPE_BENDABLE`: a "Bent" button facing north-east is offered too.
    pub bendable: bool,
}

const fn recipe(name: &'static str, path: &'static str, bendable: bool) -> DisposalRecipe {
    DisposalRecipe {
        name,
        path,
        bendable,
    }
}

/// `GLOB.disposal_pipe_recipes["Disposal Pipes"]`, in its order.
pub const DISPOSAL_RECIPES: [DisposalRecipe; 11] = [
    recipe("Pipe", "/obj/structure/disposalpipe/segment", true),
    recipe("Junction", "/obj/structure/disposalpipe/junction", false),
    recipe(
        "Y-Junction",
        "/obj/structure/disposalpipe/junction/yjunction",
        false,
    ),
    recipe(
        "Sort Junction",
        "/obj/structure/disposalpipe/sorting/mail",
        false,
    ),
    recipe("Rotator", "/obj/structure/disposalpipe/rotator", false),
    recipe("Trunk", "/obj/structure/disposalpipe/trunk", false),
    recipe(
        "Down Turn",
        "/obj/structure/disposalpipe/trunk/multiz/down",
        false,
    ),
    recipe("Up Turn", "/obj/structure/disposalpipe/trunk/multiz", false),
    recipe("Bin", "/obj/machinery/disposal/bin", false),
    recipe("Outlet", "/obj/structure/disposaloutlet", false),
    recipe("Chute", "/obj/machinery/disposal/delivery_chute", false),
];

/// What the game spawns for these mechanics by a constant path.
pub const ENGMISC_RUNTIME_TYPES: &[&str] = &[
    CONVEYOR_ROOT,
    CONVEYOR_SWITCH_ROOT,
    CONVEYOR_STACK,
    CONVEYOR_SWITCH_ITEM,
    DISPOSAL_CONSTRUCT,
    EXTINGUISHER_WATER,
    "/obj/item/stack/sheet/plastic/five",
    // A coil lays these (`target_type`); a coil's own restraints.
    "/obj/structure/cable",
    "/obj/structure/cable/layer1",
    "/obj/structure/cable/layer3",
    "/obj/structure/cable/multilayer",
    "/obj/item/restraints/handcuffs/cable",
    // Flips of what the dispenser makes (`flip_type`).
    "/obj/structure/disposalpipe/junction/flip",
    "/obj/structure/disposalpipe/rotator/flip",
    "/obj/structure/disposalpipe/sorting/mail/flip",
];

/// Paths of the [`DISPOSAL_RECIPES`] for the runtime type registry.
pub fn disposal_recipe_paths() -> impl Iterator<Item = &'static str> {
    DISPOSAL_RECIPES.iter().map(|r| r.path)
}

/// The vars of a conveyor belt or switch. Prototype data like [`DmAtom`](crate::DmAtom).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmConveyor {
    /// `id`: what links belts and switches (DM `"[id]"`).
    pub id: String,
    /// A switch (`/obj/machinery/conveyor_switch`), not a belt.
    pub is_switch: bool,
    /// Belts: `speed`, seconds per tile.
    pub speed: f32,
    pub flipped: bool,
    pub inverted: bool,
    /// `/obj/machinery/conveyor/auto`: on from the start.
    pub auto_on: bool,
    /// Switches: one way only.
    pub oneway: bool,
    pub invert_icon: bool,
    /// Switches: the seconds per tile they give their belts (`conveyor_speed`).
    pub conveyor_speed: f32,
}

/// The vars of a fire extinguisher type.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmExtinguisher {
    pub max_water: f32,
    /// `chem`: the reagent's type path; empty is `null`.
    pub chem: String,
    pub starting_water: bool,
    /// `power`: the range of the spray, in tiles.
    pub power: u32,
    pub precision: bool,
    pub cooling_power: f32,
    /// `sprite_name`: icon states are `<sprite_name>0`/`1`.
    pub sprite_name: String,
    pub tanktypes: Vec<String>,
}

/// `custom_materials` of an item, or a stack's `mats_per_unit` (per unit): what a recycler
/// reclaims from it, as `SHEET_MATERIAL_AMOUNT` units.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmMaterials {
    pub materials: Vec<String>,
    pub amounts: Vec<f32>,
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

fn list_ron<T: serde::Serialize>(items: &[T]) -> String {
    ron::to_string(items).expect("lists always serialize")
}

fn type_path(v: Option<&serde_json::Value>) -> String {
    v.and_then(|v| v.get("path"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Converter fields of `path`: belts and switches, extinguishers, item materials.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !types.types.contains_key(path) {
        return out;
    }
    let belt = is_under(path, CONVEYOR_ROOT);
    let switch = is_under(path, CONVEYOR_SWITCH_ROOT);
    if belt || switch {
        let c = COMPONENT;
        let id = types
            .resolved(path, "id")
            .and_then(|v| link_id(Val::from_json(v)).ok())
            .unwrap_or_default();
        let flag = |var: &str| truthy(types.resolved(path, var)).to_string();
        out.push(ext(c, "id", str_ron(&id)));
        out.push(ext(c, "is_switch", switch.to_string()));
        if belt {
            let speed = num(types.resolved(path, "speed")).unwrap_or(0.2) as f32;
            out.push(ext(c, "speed", format!("{speed:?}")));
            out.push(ext(c, "flipped", flag("flipped")));
            out.push(ext(c, "inverted", flag("inverted")));
            out.push(ext(
                c,
                "auto_on",
                is_under(path, "/obj/machinery/conveyor/auto").to_string(),
            ));
        } else {
            let speed = num(types.resolved(path, "conveyor_speed")).unwrap_or(0.2) as f32;
            out.push(ext(c, "oneway", flag("oneway")));
            out.push(ext(c, "invert_icon", flag("invert_icon")));
            out.push(ext(c, "conveyor_speed", format!("{speed:?}")));
        }
    }
    if is_under(path, EXTINGUISHER_ROOT) {
        let c = EXTINGUISHER_COMPONENT;
        let get = |var: &str| types.resolved(path, var);
        let n = |var: &str| num(get(var)).unwrap_or(0.0) as f32;
        let tanks: Vec<String> = amounts(get("tanktypes"))
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        out.push(ext(c, "max_water", format!("{:?}", n("max_water"))));
        out.push(ext(c, "chem", str_ron(&type_path(get("chem")))));
        out.push(ext(
            c,
            "starting_water",
            truthy(get("starting_water")).to_string(),
        ));
        out.push(ext(c, "power", (n("power") as u32).to_string()));
        out.push(ext(c, "precision", truthy(get("precision")).to_string()));
        out.push(ext(c, "cooling_power", format!("{:?}", n("cooling_power"))));
        out.push(ext(
            c,
            "sprite_name",
            str_ron(
                get("sprite_name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
            ),
        ));
        out.push(ext(c, "tanktypes", list_ron(&tanks)));
    }
    if is_under(path, "/obj/item") {
        let var = if is_under(path, "/obj/item/stack") {
            "mats_per_unit"
        } else {
            "custom_materials"
        };
        let mats = amounts(types.resolved(path, var));
        if !mats.is_empty() {
            let names: Vec<String> = mats.iter().map(|(k, _)| k.clone()).collect();
            let values: Vec<f32> = mats.iter().map(|&(_, v)| v as f32).collect();
            out.push(ext(MATERIALS_COMPONENT, "materials", list_ron(&names)));
            out.push(ext(MATERIALS_COMPONENT, "amounts", list_ron(&values)));
        }
    }
    out
}

/// `.dmm` edits of belts and switches.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    if !is_under(path, CONVEYOR_ROOT) && !is_under(path, CONVEYOR_SWITCH_ROOT) {
        return Err(Skip::UnknownVar);
    }
    let c = COMPONENT;
    let key: &'static str = match var {
        "id" => return Ok(ext(c, "id", str_ron(&link_id(value)?))),
        "speed" => "speed",
        "conveyor_speed" => "conveyor_speed",
        "flipped" => "flipped",
        "inverted" => "inverted",
        "oneway" => "oneway",
        "invert_icon" => "invert_icon",
        _ => return Err(Skip::UnknownVar),
    };
    let Val::Num(n) = value else {
        return Err(Skip::Unsupported);
    };
    Ok(match key {
        "speed" | "conveyor_speed" => ext(c, key, format!("{:?}", n as f32)),
        _ => ext(c, key, (n != 0.0).to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types() -> DmTypes {
        DmTypes::parse(
            br#"{"schema": 3, "types": {
                "/atom": { "parent": null },
                "/obj": { "parent": "/atom" },
                "/obj/machinery": { "parent": "/obj" },
                "/obj/machinery/conveyor": { "parent": "/obj/machinery",
                    "declared_vars": { "id": "", "speed": 0.2, "flipped": 0, "inverted": 0 } },
                "/obj/machinery/conveyor/auto/inverted": { "parent": "/obj/machinery/conveyor",
                    "declared_vars": { "flipped": 1 } },
                "/obj/machinery/conveyor_switch": { "parent": "/obj/machinery",
                    "declared_vars": { "id": "", "conveyor_speed": 0.2, "oneway": 0, "invert_icon": 0 } },
                "/obj/machinery/conveyor_switch/oneway": { "parent": "/obj/machinery/conveyor_switch",
                    "declared_vars": { "oneway": 1 } },
                "/obj/item": { "parent": "/obj" },
                "/obj/item/extinguisher": { "parent": "/obj/item", "declared_vars": {
                    "max_water": 50, "chem": { "path": "/datum/reagent/water" }, "starting_water": 1,
                    "power": 5, "precision": 0, "cooling_power": 2, "sprite_name": "fire_extinguisher",
                    "tanktypes": { "list": [ { "path": "/obj/structure/reagent_dispensers/watertank" } ] } } },
                "/obj/item/stack": { "parent": "/obj/item" },
                "/obj/item/stack/sheet": { "parent": "/obj/item/stack",
                    "declared_vars": { "mats_per_unit": { "list": [
                        { "assoc": [ { "path": "/datum/material/iron" }, 100 ] } ] } } }
            }}"#,
        )
        .unwrap()
    }

    fn field(fields: &[Field], name: &str) -> String {
        fields
            .iter()
            .find_map(|f| match f {
                Field::Ext { key, ron, .. } if *key == name => Some(ron.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no field {name} in {fields:?}"))
    }

    #[test]
    fn belts_and_switches_carry_their_vars() {
        let t = types();
        let f = type_fields(&t, "/obj/machinery/conveyor/auto/inverted");
        assert_eq!(field(&f, "flipped"), "true");
        assert_eq!(field(&f, "auto_on"), "true");
        assert_eq!(field(&f, "is_switch"), "false");
        let f = type_fields(&t, "/obj/machinery/conveyor_switch/oneway");
        assert_eq!(field(&f, "oneway"), "true");
        assert_eq!(field(&f, "is_switch"), "true");
        assert_eq!(field(&f, "conveyor_speed"), "0.2");
    }

    #[test]
    fn extinguishers_and_materials() {
        let t = types();
        let f = type_fields(&t, "/obj/item/extinguisher");
        assert_eq!(field(&f, "chem"), r#""/datum/reagent/water""#);
        assert!(field(&f, "tanktypes").contains("watertank"));
        let f = type_fields(&t, "/obj/item/stack/sheet");
        assert_eq!(field(&f, "materials"), r#"["/datum/material/iron"]"#);
        assert!(type_fields(&t, "/obj/item").is_empty());
    }

    #[test]
    fn map_edits() {
        assert!(matches!(
            ext_field("/obj/machinery/conveyor", "id", Val::Str("QMLoad2")),
            Ok(Field::Ext { ref ron, .. }) if ron == r#""QMLoad2""#
        ));
        assert!(ext_field("/obj/machinery/door", "id", Val::Str("x")).is_err());
    }
}
