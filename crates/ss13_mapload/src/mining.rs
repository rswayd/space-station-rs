//! Mining as prototype data: tg mineral walls (`/turf/closed/mineral`,
//! `code/game/turfs/closed/minerals.dm`), ore stacks (`/obj/item/stack/ore`,
//! `code/modules/mining/ores_coins.dm`), the ore redemption machine
//! (`code/modules/mining/machine_redemption.dm`) and the order consoles
//! (`code/game/machinery/computer/orders/`) with their `/datum/orderable_item` catalogue
//! resolved per console, the mining console's voucher sets (`code/modules/mining/voucher_sets.dm`),
//! kinetic accelerator modkits (`guns/energy/kinetic_accelerator.dm`) and the stacking machine
//! (`code/modules/mining/machine_stacking.dm`).

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::reagents::{amounts, path_value, string};
use crate::value::Field;

pub const MINERAL_ROOT: &str = "/turf/closed/mineral";
pub const GIBTONITE_TURF: &str = "/turf/closed/mineral/gibtonite";
pub const ORE_ROOT: &str = "/obj/item/stack/ore";
pub const ORM_ROOT: &str = "/obj/machinery/mineral/ore_redemption";
pub const ORDER_CONSOLE_ROOT: &str = "/obj/machinery/computer/order_console";
pub const MINING_CONSOLE: &str = "/obj/machinery/computer/order_console/mining";
pub const BITRUNNING_CONSOLE: &str = "/obj/machinery/computer/order_console/bitrunning";
const ORDERABLE_ROOT: &str = "/datum/orderable_item";
pub const GIBTONITE_ITEM: &str = "/obj/item/gibtonite";
pub const MODKIT_ROOT: &str = "/obj/item/borg/upgrade/modkit";
/// `AddElement(/datum/element/voucher_redeemer, /obj/item/mining_voucher, /datum/voucher_set/mining)`
/// of the mining console.
pub const MINING_VOUCHER_SETS: &str = "/datum/voucher_set/mining";
pub const STACKER_ROOT: &str = "/obj/machinery/mineral/stacking_machine";

pub const MINERAL_COMPONENT: &str = "dm_mineral";
pub const ORE_COMPONENT: &str = "dm_ore";
pub const ORM_COMPONENT: &str = "dm_ore_redemption";
pub const ORDER_COMPONENT: &str = "dm_order_console";
pub const MODKIT_COMPONENT: &str = "dm_modkit";
pub const STACKER_COMPONENT: &str = "dm_stacker";

/// tg `/turf/closed/mineral/random/mineral_chances()`: what a random rock rolls (weights).
// shortcut: the subtypes' own tables (`high_chance`, `low_chance`, volcanic, snow) and the
// April Fools bananium are not ported; every random rock uses the base table.
pub const RANDOM_ORES: [(&str, u32); 9] = [
    ("/obj/item/stack/ore/bluespace_crystal", 1),
    ("/obj/item/stack/ore/diamond", 2),
    ("/obj/item/stack/ore/gold", 8),
    ("/obj/item/stack/ore/iron", 18),
    ("/obj/item/stack/ore/plasma", 12),
    ("/obj/item/stack/ore/silver", 8),
    ("/obj/item/stack/ore/titanium", 8),
    ("/obj/item/stack/ore/uranium", 5),
    (GIBTONITE_TURF, 4),
];

/// `/turf/closed/mineral` vars.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmMineral {
    /// `mineral_type`: the ore stack it drops (empty: plain rock).
    pub mineral_type: String,
    pub mineral_amt: u32,
    /// `tool_mine_speed`, `hand_mine_speed`, deciseconds.
    pub tool_mine_speed: f32,
    pub hand_mine_speed: f32,
    /// `weak_turf`: mined by hand.
    pub weak_turf: bool,
    /// `/turf/closed/mineral/random` `mineral_chance`, percent (0: not random).
    pub mineral_chance: f32,
    /// A `/turf/closed/mineral/gibtonite`.
    pub gibtonite: bool,
}

/// `/obj/item/stack/ore` vars.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmOre {
    /// ORM points per unit.
    pub points: f32,
    /// What it refines into (empty: the ORM does not take it).
    pub refined_type: String,
}

/// `/obj/machinery/mineral/ore_redemption` vars.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmOreRedemption {
    pub requires_silo: bool,
    pub ore_multiplier: f32,
    pub point_upgrade: f32,
    /// `input_dir` (BYOND dir bits).
    pub input_dir: u8,
    /// `ore_values`: material type -> credits per sheet shown in the window.
    pub ore_values: Vec<(String, f32)>,
    /// `req_access` (`Release` and `Smelt`).
    pub req_access: Vec<String>,
}

/// One `/datum/orderable_item` a console sells.
#[derive(Reflect, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct OrderItem {
    /// The datum type (the window's `ref`).
    pub id: String,
    pub name: String,
    pub desc: String,
    pub category: String,
    pub purchase_path: String,
    pub cost: u32,
    pub max: u32,
}

/// One `/datum/voucher_set` a voucher redeems at the console.
#[derive(Reflect, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct VoucherSet {
    pub name: String,
    pub description: String,
    /// `set_items` that exist as types.
    pub items: Vec<String>,
}

/// `/obj/machinery/computer/order_console` vars and its catalogue.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmOrderConsole {
    pub categories: Vec<String>,
    pub items: Vec<OrderItem>,
    /// The mining console's voucher sets (empty on other consoles).
    pub vouchers: Vec<VoucherSet>,
    pub cargo_cost_multiplier: f32,
    pub express_cost_multiplier: f32,
    pub credit_type: String,
    /// `cooldown_time`, deciseconds.
    pub cooldown: f32,
    pub forced_express: bool,
    pub express_tooltip: String,
    pub purchase_tooltip: String,
    /// `announcement_line`: said on the supply channel after a shuttle order (empty: none).
    pub announcement_line: String,
}

/// `/obj/item/borg/upgrade/modkit` vars.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmModkit {
    /// `cost`: percent of the accelerator's `max_mod_capacity`.
    pub cost: f32,
    pub modifier: f32,
    /// `denied_type` (empty: none).
    pub denied_type: String,
    pub maximum_of_type: u32,
    pub minebot_upgrade: bool,
    pub minebot_exclusive: bool,
    pub removable: bool,
    /// `/obj/item/borg/upgrade/modkit/cooldown/aoe` `turf_aoe`, `damage_modifier`.
    pub turf_aoe: bool,
    pub damage_modifier: f32,
}

/// `/obj/machinery/mineral/stacking_machine` vars (map edits override them).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmStacker {
    /// BYOND dir bits.
    pub input_dir: u8,
    pub output_dir: u8,
    pub stack_amt: u32,
}

fn ron<T: Serialize>(value: &T) -> String {
    ron::to_string(value).expect("prototype data always serializes")
}

fn path_of(types: &DmTypes, path: &str, var: &str) -> String {
    path_value(types.resolved(path, var))
        .filter(|p| types.types.contains_key(p))
        .unwrap_or_default()
}

pub fn mineral(types: &DmTypes, path: &str) -> Option<DmMineral> {
    if !is_under(path, MINERAL_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let n = |var: &str| num(types.resolved(path, var)).unwrap_or(0.0) as f32;
    Some(DmMineral {
        mineral_type: path_of(types, path, "mineral_type"),
        mineral_amt: n("mineral_amt").max(0.0) as u32,
        tool_mine_speed: n("tool_mine_speed"),
        hand_mine_speed: n("hand_mine_speed"),
        weak_turf: truthy(types.resolved(path, "weak_turf")),
        mineral_chance: n("mineral_chance"),
        gibtonite: is_under(path, GIBTONITE_TURF),
    })
}

pub fn ore(types: &DmTypes, path: &str) -> Option<DmOre> {
    if !is_under(path, ORE_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    Some(DmOre {
        points: num(types.resolved(path, "points")).unwrap_or(0.0) as f32,
        refined_type: path_of(types, path, "refined_type"),
    })
}

pub fn redemption(types: &DmTypes, path: &str) -> Option<DmOreRedemption> {
    if !is_under(path, ORM_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let requires = types.resolved(path, "requires_silo");
    Some(DmOreRedemption {
        // Unset is tg's default `TRUE`.
        requires_silo: requires.is_none() || truthy(requires),
        ore_multiplier: num(types.resolved(path, "ore_multiplier"))
            .filter(|&v| v > 0.0)
            .unwrap_or(1.0) as f32,
        point_upgrade: num(types.resolved(path, "point_upgrade"))
            .filter(|&v| v > 0.0)
            .unwrap_or(1.0) as f32,
        input_dir: num(types.resolved(path, "input_dir")).unwrap_or(1.0) as u8,
        ore_values: amounts(types.resolved(path, "ore_values")).unwrap_or_default(),
        req_access: strings(types.resolved(path, "req_access")),
    })
}

pub fn modkit(types: &DmTypes, path: &str) -> Option<DmModkit> {
    if !is_under(path, MODKIT_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let n = |var: &str| num(types.resolved(path, var)).unwrap_or(0.0) as f32;
    let removable = types.resolved(path, "removable");
    let minebot = types.resolved(path, "minebot_upgrade");
    Some(DmModkit {
        cost: n("cost"),
        modifier: n("modifier"),
        denied_type: path_of(types, path, "denied_type"),
        maximum_of_type: n("maximum_of_type").max(0.0) as u32,
        // Unset is tg's default `TRUE`.
        minebot_upgrade: minebot.is_none() || truthy(minebot),
        minebot_exclusive: truthy(types.resolved(path, "minebot_exclusive")),
        removable: removable.is_none() || truthy(removable),
        turf_aoe: truthy(types.resolved(path, "turf_aoe")),
        damage_modifier: n("damage_modifier"),
    })
}

pub fn stacker(types: &DmTypes, path: &str) -> Option<DmStacker> {
    if !is_under(path, STACKER_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let n = |var: &str, default: f64| num(types.resolved(path, var)).unwrap_or(default);
    Some(DmStacker {
        input_dir: n("input_dir", 4.0) as u8,
        output_dir: n("output_dir", 8.0) as u8,
        stack_amt: n("stack_amt", 50.0).max(1.0) as u32,
    })
}

/// Map edits of the mineral machines' directions and the stacker's `stack_amt`.
pub fn ext_field(
    path: &str,
    var: &str,
    value: crate::value::Val,
) -> Result<Field, crate::value::Skip> {
    use crate::value::{Skip, Val};
    let component = if is_under(path, STACKER_ROOT) {
        STACKER_COMPONENT
    } else if is_under(path, ORM_ROOT) && var == "input_dir" {
        ORM_COMPONENT
    } else {
        return Err(Skip::UnknownVar);
    };
    let key = match var {
        "input_dir" => "input_dir",
        "output_dir" if component == STACKER_COMPONENT => "output_dir",
        "stack_amt" if component == STACKER_COMPONENT => "stack_amt",
        _ => return Err(Skip::UnknownVar),
    };
    let Val::Num(n) = value else {
        return Err(Skip::Unsupported);
    };
    let ron = if key == "stack_amt" {
        (n.max(1.0) as u32).to_string()
    } else {
        (n.clamp(0.0, 255.0) as u8).to_string()
    };
    Ok(ext(component, key, ron))
}

/// The subtypes of `root` with a name and items: `voucher_redeemer/generate_sets`.
pub fn voucher_sets(types: &DmTypes, root: &str) -> Vec<VoucherSet> {
    types
        .types
        .keys()
        .filter(|p| is_under(p, root) && p.as_str() != root)
        .filter_map(|p| {
            let name = string(types.resolved(p, "name"));
            let items: Vec<String> = match types.resolved(p, "set_items") {
                Some(Value::Object(o)) => o
                    .get("list")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|v| path_value(Some(v)))
                            .filter(|i| types.types.contains_key(i))
                            .collect()
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            (!name.is_empty() && !items.is_empty()).then(|| VoucherSet {
                name,
                description: string(types.resolved(p, "description")),
                items,
            })
        })
        .collect()
}

/// A list of strings (`list("Mining", ...)`).
fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Object(o)) => o
            .get("list")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// `GLOB.order_console_products`: every `/datum/orderable_item` with a `purchase_path`, as
/// `New()` fills it in (name and desc from the item, `max_per_order` from the cost).
pub fn orderable_items(types: &DmTypes) -> Vec<OrderItem> {
    types
        .types
        .keys()
        .filter(|p| is_under(p, ORDERABLE_ROOT) && p.as_str() != ORDERABLE_ROOT)
        .filter_map(|p| {
            let purchase = path_value(types.resolved(p, "purchase_path"))?;
            if !types.types.contains_key(&purchase) {
                return None;
            }
            let category = string(types.resolved(p, "category_index"));
            if category.is_empty() {
                return None;
            }
            let text = |var: &str| {
                let own = string(types.resolved(p, var));
                if own.is_empty() {
                    string(types.resolved(&purchase, var))
                } else {
                    own
                }
            };
            let cost = num(types.resolved(p, "cost_per_order"))
                .unwrap_or(10.0)
                .max(0.0) as u32;
            let max = num(types.resolved(p, "max_per_order")).unwrap_or(0.0) as u32;
            let max = if max > 0 {
                max
            } else if cost <= 10 {
                50
            } else if cost <= 25 {
                20
            } else {
                10
            };
            Some(OrderItem {
                id: p.clone(),
                name: text("name"),
                desc: text("desc"),
                category,
                purchase_path: purchase,
                cost,
                max,
            })
        })
        .collect()
}

pub fn order_console(types: &DmTypes, path: &str) -> Option<DmOrderConsole> {
    if !is_under(path, ORDER_CONSOLE_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let categories = strings(types.resolved(path, "order_categories"));
    let items = orderable_items(types)
        .into_iter()
        .filter(|i| categories.contains(&i.category))
        .collect();
    let n = |var: &str, default: f64| num(types.resolved(path, var)).unwrap_or(default) as f32;
    let vouchers = if is_under(path, MINING_CONSOLE) {
        voucher_sets(types, MINING_VOUCHER_SETS)
    } else {
        Vec::new()
    };
    Some(DmOrderConsole {
        categories,
        items,
        vouchers,
        cargo_cost_multiplier: n("cargo_cost_multiplier", 1.0),
        express_cost_multiplier: n("express_cost_multiplier", 2.0),
        credit_type: string(types.resolved(path, "credit_type")),
        cooldown: n("cooldown_time", 600.0),
        forced_express: truthy(types.resolved(path, "forced_express")),
        express_tooltip: string(types.resolved(path, "express_tooltip")),
        purchase_tooltip: string(types.resolved(path, "purchase_tooltip")),
        announcement_line: string(types.resolved(path, "announcement_line")),
    })
}

/// What mining spawns at runtime for the `needed` types: the ores and floors of mineral walls
/// (random ones roll [`RANDOM_ORES`]), gibtonite, ores' refined sheets and what the order
/// consoles sell.
pub fn mining_closure<'a>(
    types: &DmTypes,
    needed: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut ores = Vec::new();
    for path in needed {
        if let Some(rock) = mineral(types, path) {
            out.insert(path_of(types, path, "baseturfs"));
            out.insert(path_of(types, path, "turf_type"));
            if !rock.mineral_type.is_empty() {
                ores.push(rock.mineral_type);
            }
            if rock.mineral_chance > 0.0 {
                ores.extend(RANDOM_ORES.iter().map(|(p, _)| (*p).to_owned()));
            }
            if rock.gibtonite || rock.mineral_chance > 0.0 {
                out.insert(GIBTONITE_ITEM.to_owned());
            }
        }
        if is_under(path, ORE_ROOT) {
            ores.push(path.to_owned());
        }
        // What the order consoles sell (`ss13_game::mining::console`).
        if let Some(console) = order_console(types, path) {
            out.extend(console.items.into_iter().map(|i| i.purchase_path));
            out.extend(console.vouchers.into_iter().flat_map(|v| v.items));
        }
    }
    for ore_path in ores {
        out.insert(path_of(types, &ore_path, "refined_type"));
        // A gibtonite rock a random one turns into.
        if mineral(types, &ore_path).is_some() {
            out.insert(path_of(types, &ore_path, "baseturfs"));
        }
        out.insert(ore_path);
    }
    out.retain(|p| !p.is_empty() && types.types.contains_key(p));
    out
}

/// Converter fields of `path`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if let Some(m) = mineral(types, path) {
        let c = MINERAL_COMPONENT;
        out.extend([
            ext(c, "mineral_type", ron(&m.mineral_type)),
            ext(c, "mineral_amt", ron(&m.mineral_amt)),
            ext(c, "tool_mine_speed", ron(&m.tool_mine_speed)),
            ext(c, "hand_mine_speed", ron(&m.hand_mine_speed)),
            ext(c, "weak_turf", ron(&m.weak_turf)),
            ext(c, "mineral_chance", ron(&m.mineral_chance)),
            ext(c, "gibtonite", ron(&m.gibtonite)),
        ]);
    }
    if let Some(o) = ore(types, path) {
        let c = ORE_COMPONENT;
        out.extend([
            ext(c, "points", ron(&o.points)),
            ext(c, "refined_type", ron(&o.refined_type)),
        ]);
    }
    if let Some(r) = redemption(types, path) {
        let c = ORM_COMPONENT;
        out.extend([
            ext(c, "requires_silo", ron(&r.requires_silo)),
            ext(c, "ore_multiplier", ron(&r.ore_multiplier)),
            ext(c, "point_upgrade", ron(&r.point_upgrade)),
            ext(c, "input_dir", ron(&r.input_dir)),
            ext(c, "ore_values", ron(&r.ore_values)),
            ext(c, "req_access", ron(&r.req_access)),
        ]);
    }
    if let Some(m) = modkit(types, path) {
        let c = MODKIT_COMPONENT;
        out.extend([
            ext(c, "cost", ron(&m.cost)),
            ext(c, "modifier", ron(&m.modifier)),
            ext(c, "denied_type", ron(&m.denied_type)),
            ext(c, "maximum_of_type", ron(&m.maximum_of_type)),
            ext(c, "minebot_upgrade", ron(&m.minebot_upgrade)),
            ext(c, "minebot_exclusive", ron(&m.minebot_exclusive)),
            ext(c, "removable", ron(&m.removable)),
            ext(c, "turf_aoe", ron(&m.turf_aoe)),
            ext(c, "damage_modifier", ron(&m.damage_modifier)),
        ]);
    }
    if let Some(st) = stacker(types, path) {
        let c = STACKER_COMPONENT;
        out.extend([
            ext(c, "input_dir", ron(&st.input_dir)),
            ext(c, "output_dir", ron(&st.output_dir)),
            ext(c, "stack_amt", ron(&st.stack_amt)),
        ]);
    }
    if let Some(o) = order_console(types, path) {
        let c = ORDER_COMPONENT;
        out.extend([
            ext(c, "categories", ron(&o.categories)),
            ext(c, "items", ron(&o.items)),
            ext(c, "vouchers", ron(&o.vouchers)),
            ext(c, "cargo_cost_multiplier", ron(&o.cargo_cost_multiplier)),
            ext(
                c,
                "express_cost_multiplier",
                ron(&o.express_cost_multiplier),
            ),
            ext(c, "credit_type", ron(&o.credit_type)),
            ext(c, "cooldown", ron(&o.cooldown)),
            ext(c, "forced_express", ron(&o.forced_express)),
            ext(c, "express_tooltip", ron(&o.express_tooltip)),
            ext(c, "purchase_tooltip", ron(&o.purchase_tooltip)),
            ext(c, "announcement_line", ron(&o.announcement_line)),
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/datum/orderable_item": { "parent": "/datum", "declared_vars": { "category_index": 0,
            "cost_per_order": 10, "max_per_order": null, "name": null, "desc": null,
            "purchase_path": null } },
        "/datum/orderable_item/accelerator": { "parent": "/datum/orderable_item",
            "declared_vars": { "category_index": "PKAs" } },
        "/datum/orderable_item/accelerator/gun": { "parent": "/datum/orderable_item/accelerator",
            "declared_vars": { "purchase_path": { "path": "/obj/item/pka" },
            "cost_per_order": 600 } },
        "/datum/orderable_item/food": { "parent": "/datum/orderable_item",
            "declared_vars": { "category_index": "Food",
            "purchase_path": { "path": "/obj/item/pka" } } },
        "/atom": { "parent": "/datum" },
        "/obj": { "parent": "/atom" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/pka": { "parent": "/obj/item", "declared_vars": { "name": "pka",
            "desc": "A gun." } },
        "/obj/item/stack": { "parent": "/obj/item" },
        "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack" },
        "/obj/item/stack/ore": { "parent": "/obj/item/stack", "declared_vars": { "points": 0,
            "refined_type": null } },
        "/obj/item/stack/ore/iron": { "parent": "/obj/item/stack/ore", "declared_vars": {
            "points": 1, "refined_type": { "path": "/obj/item/stack/sheet/iron" } } },
        "/obj/machinery": { "parent": "/obj" },
        "/obj/machinery/computer": { "parent": "/obj/machinery" },
        "/obj/machinery/computer/order_console": { "parent": "/obj/machinery/computer",
            "declared_vars": { "order_categories": { "list": [] }, "cargo_cost_multiplier": 1,
            "express_cost_multiplier": 2, "credit_type": "cr", "cooldown_time": 600 } },
        "/obj/machinery/computer/order_console/mining": {
            "parent": "/obj/machinery/computer/order_console", "declared_vars": {
            "order_categories": { "list": [ "PKAs" ] }, "credit_type": "mp",
            "express_cost_multiplier": 1, "cooldown_time": 100 } },
        "/turf": { "parent": "/atom" },
        "/turf/open": { "parent": "/turf" },
        "/turf/open/misc/asteroid/airless": { "parent": "/turf/open" },
        "/turf/closed": { "parent": "/turf" },
        "/turf/closed/mineral": { "parent": "/turf/closed", "declared_vars": {
            "baseturfs": { "path": "/turf/open/misc/asteroid/airless" },
            "turf_type": { "path": "/turf/open/misc/asteroid/airless" },
            "mineral_type": null, "mineral_amt": 3, "tool_mine_speed": 40,
            "hand_mine_speed": 150, "weak_turf": 0 } },
        "/turf/closed/mineral/iron": { "parent": "/turf/closed/mineral", "declared_vars": {
            "mineral_type": { "path": "/obj/item/stack/ore/iron" } } }
    } }"##;

    #[test]
    fn rocks_ores_and_the_mining_catalogue() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let rock = mineral(&types, "/turf/closed/mineral/iron").unwrap();
        assert_eq!(rock.mineral_type, "/obj/item/stack/ore/iron");
        assert_eq!((rock.mineral_amt, rock.tool_mine_speed), (3, 40.0));
        assert!(!rock.gibtonite && rock.mineral_chance == 0.0);
        let iron = ore(&types, "/obj/item/stack/ore/iron").unwrap();
        assert_eq!(iron.points, 1.0);
        assert_eq!(iron.refined_type, "/obj/item/stack/sheet/iron");
        let console =
            order_console(&types, "/obj/machinery/computer/order_console/mining").unwrap();
        assert_eq!(console.items.len(), 1, "only its categories");
        let gun = &console.items[0];
        assert_eq!((gun.name.as_str(), gun.desc.as_str()), ("pka", "A gun."));
        assert_eq!((gun.cost, gun.max), (600, 10));
        assert_eq!(console.credit_type, "mp");
        let closure = mining_closure(
            &types,
            [
                "/turf/closed/mineral/iron",
                "/obj/machinery/computer/order_console/mining",
            ],
        );
        for want in [
            "/obj/item/stack/ore/iron",
            "/obj/item/stack/sheet/iron",
            "/turf/open/misc/asteroid/airless",
            "/obj/item/pka",
        ] {
            assert!(closure.contains(want), "{want} missing from {closure:?}");
        }
    }

    const KIT_TYPES: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/datum/voucher_set": { "parent": "/datum", "declared_vars": { "name": null,
            "description": null, "set_items": null } },
        "/datum/voucher_set/mining": { "parent": "/datum/voucher_set" },
        "/datum/voucher_set/mining/resonator_kit": { "parent": "/datum/voucher_set/mining",
            "declared_vars": { "name": "Resonator Kit", "description": "Rocks.",
            "set_items": { "list": [ { "path": "/obj/item/resonator" },
                { "path": "/obj/item/missing" } ] } } },
        "/datum/voucher_set/mining/nameless": { "parent": "/datum/voucher_set/mining",
            "declared_vars": { "set_items": { "list": [ { "path": "/obj/item/resonator" } ] } } },
        "/atom": { "parent": "/datum" },
        "/obj": { "parent": "/atom" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/resonator": { "parent": "/obj/item" },
        "/obj/item/borg": { "parent": "/obj/item" },
        "/obj/item/borg/upgrade": { "parent": "/obj/item/borg" },
        "/obj/item/borg/upgrade/modkit": { "parent": "/obj/item/borg/upgrade",
            "declared_vars": { "cost": 30, "modifier": 1, "denied_type": null,
            "maximum_of_type": 0, "minebot_upgrade": 1, "minebot_exclusive": 0,
            "removable": 1 } },
        "/obj/item/borg/upgrade/modkit/cooldown": { "parent": "/obj/item/borg/upgrade/modkit",
            "declared_vars": { "modifier": -3.2, "minebot_upgrade": 0 } },
        "/obj/item/borg/upgrade/modkit/cooldown/aoe": {
            "parent": "/obj/item/borg/upgrade/modkit/cooldown", "declared_vars": {
            "modifier": 0, "cost": 10, "maximum_of_type": 1, "turf_aoe": 0,
            "damage_modifier": 0,
            "denied_type": { "path": "/obj/item/borg/upgrade/modkit/cooldown/aoe" } } },
        "/obj/item/borg/upgrade/modkit/cooldown/aoe/turfs": {
            "parent": "/obj/item/borg/upgrade/modkit/cooldown/aoe", "declared_vars": {
            "turf_aoe": 1, "modifier": 3.2 } },
        "/obj/machinery": { "parent": "/obj" },
        "/obj/machinery/mineral": { "parent": "/obj/machinery", "declared_vars": {
            "input_dir": 1, "output_dir": 2 } },
        "/obj/machinery/mineral/stacking_machine": { "parent": "/obj/machinery/mineral",
            "declared_vars": { "input_dir": 4, "output_dir": 8, "stack_amt": 50 } }
    } }"##;

    #[test]
    fn modkits_vouchers_and_the_stacker() {
        let types = DmTypes::parse(KIT_TYPES.as_bytes()).unwrap();
        let kit = modkit(&types, "/obj/item/borg/upgrade/modkit/cooldown/aoe/turfs").unwrap();
        assert_eq!((kit.cost, kit.modifier), (10.0, 3.2));
        assert!(kit.turf_aoe && kit.removable && !kit.minebot_upgrade);
        assert_eq!(
            kit.denied_type,
            "/obj/item/borg/upgrade/modkit/cooldown/aoe"
        );
        assert_eq!(kit.maximum_of_type, 1);
        assert!(modkit(&types, "/obj/item/resonator").is_none());
        let sets = voucher_sets(&types, MINING_VOUCHER_SETS);
        assert_eq!(sets.len(), 1, "a set needs a name: {sets:?}");
        assert_eq!(sets[0].name, "Resonator Kit");
        assert_eq!(sets[0].items, vec!["/obj/item/resonator".to_owned()]);
        let st = stacker(&types, STACKER_ROOT).unwrap();
        assert_eq!((st.input_dir, st.output_dir, st.stack_amt), (4, 8, 50));
        use crate::value::Val;
        assert_eq!(
            ext_field(STACKER_ROOT, "stack_amt", Val::Num(10.0)).ok(),
            Some(ext(STACKER_COMPONENT, "stack_amt", "10"))
        );
        assert_eq!(
            ext_field(ORM_ROOT, "input_dir", Val::Num(2.0)).ok(),
            Some(ext(ORM_COMPONENT, "input_dir", "2"))
        );
        assert!(ext_field(ORM_ROOT, "stack_amt", Val::Num(2.0)).is_err());
    }
}
