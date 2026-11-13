//! Station bots as load data (tg `code/modules/mob/living/basic/bots/`,
//! `code/game/machinery/navbeacon.dm`, `code/modules/mob/living/simple_animal/bot/
//! construction.dm`).
//!
//! [`DmNavBeacon`] carries a navigation beacon's `location` and `codes_txt` (type defaults and
//! `.dmm` edits): the game parses the codes as `/obj/machinery/navbeacon/set_codes` does.
//! [`BOT_RUNTIME_TYPES`] are what bot construction and bot deaths spawn.

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under};
use crate::value::{Field, Skip, Val};

pub const NAVBEACON_ROOT: &str = "/obj/machinery/navbeacon";
const COMPONENT: &str = "dm_navbeacon";

/// Built by bot construction or dropped by dying bots, and what MULEbots, ED-209s, honkbots and
/// hygienebots use at runtime (cells, cuffs, the clown box a honkbot starts from).
pub const BOT_RUNTIME_TYPES: [&str; 41] = [
    "/mob/living/basic/bot/cleanbot",
    "/mob/living/basic/bot/medbot",
    "/mob/living/basic/bot/secbot",
    "/mob/living/basic/bot/firebot",
    "/mob/living/basic/bot/secbot/ed209",
    "/mob/living/basic/bot/secbot/honkbot",
    "/mob/living/basic/bot/secbot/grievous",
    "/mob/living/basic/bot/secbot/grievous/toy",
    "/mob/living/basic/bot/hygienebot",
    "/obj/item/bot_assembly/cleanbot",
    "/obj/item/bot_assembly/medbot",
    "/obj/item/bot_assembly/secbot",
    "/obj/item/bot_assembly/firebot",
    "/obj/item/bot_assembly/ed209",
    "/obj/item/bot_assembly/honkbot",
    "/obj/item/bot_assembly/hygienebot",
    "/obj/item/assembly/prox_sensor",
    "/obj/item/assembly/signaler",
    "/obj/item/bodypart/arm/right/robot",
    "/obj/item/bodypart/leg/left/robot",
    "/obj/item/bodypart/leg/right/robot",
    "/obj/item/reagent_containers/cup/bucket",
    "/obj/item/clothing/head/utility/hardhat/red",
    "/obj/item/restraints/handcuffs/cable/zipties",
    "/obj/item/restraints/handcuffs/cable/zipties/used",
    "/obj/item/restraints/handcuffs/cable/zipties/fake",
    "/obj/effect/decal/cleanable/blood/oil",
    "/obj/item/healthanalyzer",
    "/obj/item/stock_parts/power_store/cell/upgraded",
    "/obj/item/stock_parts/power_store/cell/potato",
    "/obj/item/gun/energy/disabler",
    "/obj/item/clothing/head/helmet",
    "/obj/item/clothing/suit/armor/vest",
    "/obj/item/bikehorn/airhorn",
    "/obj/item/storage/box/clown",
    "/obj/item/stack/sheet/iron",
    "/obj/item/melee/energy/sword/saber",
    "/obj/item/toy/sword",
    // The repairbot's toolbox, the glass it hands back, the floor tiles it holds.
    "/obj/item/storage/toolbox/mechanical",
    "/obj/item/stack/sheet/glass",
    "/obj/item/stack/tile/iron",
];

/// `/obj/machinery/navbeacon` `location` and `codes_txt` ("patrol;next_patrol=X").
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmNavBeacon {
    pub location: String,
    pub codes_txt: String,
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

/// Converter fields: every navbeacon type carries its resolved `location` and `codes_txt`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !is_under(path, NAVBEACON_ROOT) || !types.types.contains_key(path) {
        return Vec::new();
    }
    ["location", "codes_txt"]
        .into_iter()
        .map(|var| {
            let value = types.resolved(path, var);
            ext(
                COMPONENT,
                var,
                str_ron(value.and_then(|v| v.as_str()).unwrap_or("")),
            )
        })
        .collect()
}

/// `.dmm` edits of `location` and `codes_txt` on navbeacons.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let key = match var {
        "location" => "location",
        "codes_txt" => "codes_txt",
        _ => return Err(Skip::UnknownVar),
    };
    if !is_under(path, NAVBEACON_ROOT) {
        return Err(Skip::UnknownVar);
    }
    match value {
        Val::Str(s) => Ok(ext(COMPONENT, key, str_ron(s))),
        Val::Null => Ok(ext(COMPONENT, key, str_ron(""))),
        _ => Err(Skip::Unsupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacon_edits_become_fields() {
        let field = ext_field(
            NAVBEACON_ROOT,
            "codes_txt",
            Val::Str("patrol;next_patrol=A"),
        );
        assert_eq!(
            field.ok(),
            Some(ext(COMPONENT, "codes_txt", "\"patrol;next_patrol=A\""))
        );
        assert!(ext_field("/obj/machinery/door", "location", Val::Str("x")).is_err());
        assert!(ext_field(NAVBEACON_ROOT, "name", Val::Str("x")).is_err());
    }
}
