//! Security gear as prototype data: the vars of batons
//! (`code/game/objects/items/weaponry/melee/baton.dm`) and handcuffs
//! (`code/game/objects/items/handcuffs.dm`) that hitting, stunning, cuffing and breaking out
//! read. Every type under a root carries its resolved vars, so a subtype never depends on the
//! parent's patch. A baton's preloaded cell (`preload_cell_type`) is resolved here to its
//! `maxcharge`: tg cells start full.

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::Field;

pub const BATON_ROOT: &str = "/obj/item/melee/baton";
pub const SECURITY_BATON_ROOT: &str = "/obj/item/melee/baton/security";
pub const TELESCOPIC_BATON_ROOT: &str = "/obj/item/melee/baton/telescopic";
pub const HANDCUFFS_ROOT: &str = "/obj/item/restraints/handcuffs";

const BATON_COMPONENT: &str = "dm_baton";
const HANDCUFFS_COMPONENT: &str = "dm_handcuffs";

/// `/obj/item/melee/baton` vars, times in deciseconds as in DM.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmBaton {
    pub path: String,
    /// The baton starts switched on (`active`).
    pub active: bool,
    pub cooldown: f32,
    pub knockdown_time: f32,
    pub stamina_damage: f32,
    pub stun_on_harmbaton: bool,
    /// `/obj/item/melee/baton/security`: a cell powers it; `cell_hit_cost` joules per stun.
    pub uses_cell: bool,
    pub cell_hit_cost: f64,
    /// `maxcharge` of `preload_cell_type`; 0 without a cell.
    pub cell_maxcharge: f64,
    /// `chargerate` of `preload_cell_type`, watts (what a recharger feeds it).
    pub cell_chargerate: f64,
    /// Telescopic batons: `active_force` when extended (tg `/datum/component/transforming`).
    pub telescopic: bool,
    pub active_force: f32,
    /// `on_inhand_icon_state` of a telescopic baton.
    pub on_inhand_icon_state: String,
}

/// `/obj/item/restraints/handcuffs` vars, deciseconds.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmHandcuffs {
    pub path: String,
    pub handcuff_time: f32,
    pub breakouttime: f32,
    /// Used zipties cannot cuff again (`used`).
    pub used: bool,
}

/// Converter fields of `path`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !types.types.contains_key(path) {
        return out;
    }
    let get = |var: &str| types.resolved(path, var);
    let n = |var: &str| num(get(var)).unwrap_or(0.0);
    if is_under(path, BATON_ROOT) {
        let c = BATON_COMPONENT;
        out.push(ext(c, "path", str_ron(path)));
        out.push(ext(c, "active", truthy(get("active")).to_string()));
        for var in ["cooldown", "knockdown_time", "stamina_damage"] {
            out.push(ext(c, var, format!("{:?}", n(var) as f32)));
        }
        out.push(ext(
            c,
            "stun_on_harmbaton",
            truthy(get("stun_on_harmbaton")).to_string(),
        ));
        if is_under(path, SECURITY_BATON_ROOT) {
            out.push(ext(c, "uses_cell", "true"));
            out.push(ext(c, "cell_hit_cost", format!("{:?}", n("cell_hit_cost"))));
            let cell = get("preload_cell_type")
                .and_then(|v| v.get("path"))
                .and_then(Value::as_str);
            for (field, var) in [
                ("cell_maxcharge", "maxcharge"),
                ("cell_chargerate", "chargerate"),
            ] {
                let value = cell.and_then(|p| num(types.resolved(p, var)));
                out.push(ext(c, field, format!("{:?}", value.unwrap_or(0.0))));
            }
        }
        if is_under(path, TELESCOPIC_BATON_ROOT) {
            out.push(ext(c, "telescopic", "true"));
            out.push(ext(
                c,
                "active_force",
                format!("{:?}", n("active_force") as f32),
            ));
            let inhand = get("on_inhand_icon_state")
                .and_then(Value::as_str)
                .unwrap_or_default();
            out.push(ext(c, "on_inhand_icon_state", str_ron(inhand)));
        }
    }
    if is_under(path, HANDCUFFS_ROOT) {
        let c = HANDCUFFS_COMPONENT;
        out.push(ext(c, "path", str_ron(path)));
        for var in ["handcuff_time", "breakouttime"] {
            out.push(ext(c, var, format!("{:?}", n(var) as f32)));
        }
        out.push(ext(c, "used", truthy(get("used")).to_string()));
    }
    out
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/obj": { "parent": null },
        "/obj/item": { "parent": "/obj", "declared_vars": { "breakouttime": 0 } },
        "/obj/item/melee": { "parent": "/obj/item" },
        "/obj/item/melee/baton": { "parent": "/obj/item/melee", "declared_vars": {
            "active": 1, "cooldown": 40, "knockdown_time": 15, "stamina_damage": 55,
            "stun_on_harmbaton": 0 } },
        "/obj/item/melee/baton/security": { "parent": "/obj/item/melee/baton",
            "declared_vars": { "active": 0, "cooldown": 25, "knockdown_time": 50,
            "stamina_damage": 60, "stun_on_harmbaton": 1, "cell_hit_cost": 10000,
            "preload_cell_type": null } },
        "/obj/item/melee/baton/security/loaded": { "parent": "/obj/item/melee/baton/security",
            "declared_vars": { "preload_cell_type": {
                "path": "/obj/item/stock_parts/power_store/cell/high" } } },
        "/obj/item/melee/baton/telescopic": { "parent": "/obj/item/melee/baton",
            "declared_vars": { "active": 0, "active_force": 10, "stamina_damage": 35,
            "on_inhand_icon_state": "nullrod" } },
        "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item",
            "declared_vars": { "maxcharge": 10000 } },
        "/obj/item/stock_parts/power_store/cell/high": {
            "parent": "/obj/item/stock_parts/power_store/cell",
            "declared_vars": { "maxcharge": 100000 } },
        "/obj/item/restraints": { "parent": "/obj/item", "declared_vars": { "breakouttime": 600 } },
        "/obj/item/restraints/handcuffs": { "parent": "/obj/item/restraints",
            "declared_vars": { "handcuff_time": 40, "used": 0 } },
        "/obj/item/restraints/handcuffs/cable": { "parent": "/obj/item/restraints/handcuffs",
            "declared_vars": { "breakouttime": 300 } }
      }
    }"##;

    fn ron(types: &DmTypes, path: &str) -> String {
        crate::value::patch_ron(&type_fields(types, path))
    }

    #[test]
    fn batons_resolve_their_vars_and_preloaded_cell() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let loaded = ron(&types, "/obj/item/melee/baton/security/loaded");
        for part in [
            "active: false",
            "cooldown: 25.0",
            "knockdown_time: 50.0",
            "stamina_damage: 60.0",
            "stun_on_harmbaton: true",
            "uses_cell: true",
            "cell_hit_cost: 10000.0",
            "cell_maxcharge: 100000.0",
        ] {
            assert!(loaded.contains(part), "{part} in {loaded}");
        }
        let empty = ron(&types, "/obj/item/melee/baton/security");
        assert!(empty.contains("cell_maxcharge: 0.0"), "{empty}");
        let classic = ron(&types, "/obj/item/melee/baton");
        assert!(classic.contains("active: true") && !classic.contains("uses_cell"));
        let tele = ron(&types, "/obj/item/melee/baton/telescopic");
        assert!(tele.contains("telescopic: true") && tele.contains("active_force: 10.0"));
        assert!(type_fields(&types, "/obj/item/melee").is_empty());
    }

    #[test]
    fn handcuffs_resolve_times() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let cable = ron(&types, "/obj/item/restraints/handcuffs/cable");
        assert!(cable.contains("handcuff_time: 40.0") && cable.contains("breakouttime: 300.0"));
        let cuffs = ron(&types, "/obj/item/restraints/handcuffs");
        assert!(cuffs.contains("breakouttime: 600.0") && cuffs.contains("used: false"));
    }
}
