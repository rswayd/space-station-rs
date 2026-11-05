//! Antagonist data from the DM dump, written by `ss13-mapload convert` to [`ANTAG_FILE`]: the
//! uplink catalog (`/datum/uplink_item`, `/datum/uplink_category`) and the steal objective
//! targets (`/datum/objective_item/steal`), plus the traitor flavor texts and the phonetic
//! alphabet from `strings/`. The game (`ss13_game::antag`) reads it at startup.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;

/// `<assets>/antag.ron`.
pub const ANTAG_FILE: &str = "antag.ron";

/// Items a changeling grows at runtime (`/datum/action/changeling/weapon/arm_blade`,
/// `/datum/action/changeling/suit/armor`).
pub const CHANGELING_RUNTIME_TYPES: [&str; 3] = [
    "/obj/item/melee/arm_blade",
    "/obj/item/clothing/suit/armor/changeling",
    "/obj/item/clothing/head/helmet/changeling",
];

/// What the blood cult makes at runtime (`ss13_game::antag::cult`): its runes, the ritual dagger
/// and runed metal, the blood magic auras, the shadow shackles, the barrier and Nar'Sie.
pub const CULT_RUNTIME_TYPES: [&str; 17] = [
    "/obj/effect/rune/convert",
    "/obj/effect/rune/empower",
    "/obj/effect/rune/teleport",
    "/obj/effect/rune/raise_dead",
    "/obj/effect/rune/wall",
    "/obj/effect/rune/summon",
    "/obj/effect/rune/narsie",
    "/obj/item/melee/cultblade/dagger",
    "/obj/item/stack/sheet/runed_metal",
    "/obj/item/stack/sheet/runed_metal/ten",
    "/obj/item/melee/blood_magic/stun",
    "/obj/item/melee/blood_magic/teleport",
    "/obj/item/melee/blood_magic/shackles",
    "/obj/item/melee/blood_magic/construction",
    "/obj/item/restraints/handcuffs/cult",
    "/obj/structure/emergency_shield/cult/barrier",
    "/obj/narsie",
];

/// What the heretic makes at runtime (`ss13_game::antag::heretic`): influences, the rune, the
/// Mansus grasp's hand and every ritual's results (`result_atoms`).
pub const HERETIC_RUNTIME_TYPES: [&str; 46] = [
    "/obj/effect/heretic_influence",
    "/obj/effect/visible_heretic_influence",
    "/obj/effect/heretic_rune/big",
    "/obj/item/melee/touch_attack/mansus_fist",
    "/obj/item/ammo_box/speedloader/strilka310/lionhunter",
    "/obj/item/canvas",
    "/obj/item/card/id/advanced/heretic",
    "/obj/item/clothing/mask/madness_mask",
    "/obj/item/clothing/neck/eldritch_amulet",
    "/obj/item/clothing/neck/heretic_focus",
    "/obj/item/clothing/neck/moon_amulet",
    "/obj/item/clothing/shoes/greaves_of_the_prophet",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/ash",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/blade",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/cosmic",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/flesh",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/lock",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/moon",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/rust",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/void",
    "/obj/item/clothing/suit/hooded/cultrobes/void",
    "/obj/item/codex_cicatrix",
    "/obj/item/codex_cicatrix/morbus",
    "/obj/item/coin/eldritch",
    "/obj/item/ether",
    "/obj/item/flashlight/lantern/heretic",
    "/obj/item/food/grown/flower/poppy/blessed",
    "/obj/item/grenade/chem_grenade/rust_sower",
    "/obj/item/gun/ballistic/rifle/lionhunter",
    "/obj/item/heretic_labyrinth_handbook",
    "/obj/item/knife/butcher/heretic",
    "/obj/item/melee/rune_carver",
    "/obj/item/melee/sickly_blade/ash",
    "/obj/item/melee/sickly_blade/cosmic",
    "/obj/item/melee/sickly_blade/dark",
    "/obj/item/melee/sickly_blade/flesh",
    "/obj/item/melee/sickly_blade/lock",
    "/obj/item/melee/sickly_blade/moon",
    "/obj/item/melee/sickly_blade/rust",
    "/obj/item/melee/sickly_blade/void",
    "/obj/item/reagent_containers/cup/beaker/eldritch",
    "/obj/item/reagent_containers/cup/phylactery",
    "/obj/item/storage/belt/unfathomable_curio",
    "/obj/item/void_prison",
    "/obj/item/wallframe/mirror/heretic",
    "/obj/structure/destructible/eldritch_crucible",
];

/// tg `UPLINK_TRAITORS`.
pub const UPLINK_TRAITORS: u32 = 1 << 0;
/// tg `UPLINK_NUKE_OPS`.
pub const UPLINK_NUKE_OPS: u32 = 1 << 1;

/// The nuclear operatives' outfits (`/datum/antagonist/nukeop` `nukeop_outfit`, the leader's).
pub const NUKEOP_OUTFITS: [&str; 2] = ["/datum/outfit/syndicate", "/datum/outfit/syndicate/leader"];

/// Items the nuclear operatives get at runtime: the leader's code paper and war declaration
/// item (`/datum/antagonist/nukeop/leader`), the nuclear uplink (`/datum/outfit/syndicate`
/// `uplink_type`) and the operatives' pinpointer (on the syndicate base in tg).
pub const NUKEOP_RUNTIME_TYPES: [&str; 5] = [
    "/obj/item/paper",
    "/obj/item/nuclear_challenge",
    "/obj/item/uplink/nuclear",
    "/obj/item/pinpointer/nuke/syndicate",
    "/obj/item/disk/nuclear",
];

/// The healing school's staff (`ss13_game::antag::wizard`).
pub const HEALING_STAFF: &str = "/obj/item/gun/magic/staff/healing";

/// What the wizard and its spells make at runtime (`ss13_game::antag::wizard`,
/// `ss13_game::spells`): `/datum/outfit/wizard` (+ `/apprentice`), the spellbook's items, the
/// touch hands, the forcefield, the magic bolts and the door bolt's airlock and plating.
pub const WIZARD_RUNTIME_TYPES: [&str; 40] = [
    "/obj/item/spellbook",
    "/obj/item/teleportation_scroll",
    "/obj/item/teleportation_scroll/apprentice",
    "/obj/item/antag_spawner/contract",
    "/obj/item/clothing/suit/wizrobe",
    "/obj/item/clothing/head/wizard",
    "/obj/item/clothing/shoes/sandal/magic",
    "/obj/item/clothing/under/color/lightpurple",
    "/obj/item/radio/headset",
    "/obj/item/staff",
    "/obj/item/storage/backpack",
    "/obj/item/storage/box/survival",
    "/obj/item/gun/magic/staff/chaos",
    HEALING_STAFF,
    "/obj/item/gun/magic/staff/door",
    "/obj/item/storage/belt/wands/full",
    "/obj/item/gun/magic/wand/death",
    "/obj/item/gun/magic/wand/resurrection",
    "/obj/item/gun/magic/wand/polymorph",
    "/obj/item/gun/magic/wand/teleport",
    "/obj/item/gun/magic/wand/door",
    "/obj/item/gun/magic/wand/fireball",
    "/obj/item/gun/magic/wand/shrink",
    "/obj/item/melee/touch_attack/smite",
    "/obj/effect/forcefield/wizard",
    "/obj/effect/particle_effect/fluid/smoke/bad",
    "/obj/projectile/magic/nothing",
    "/obj/projectile/magic/aoe/magic_missile",
    "/obj/projectile/magic/fireball",
    "/obj/projectile/magic/aoe/lightning",
    "/obj/projectile/magic/death",
    "/obj/projectile/magic/resurrection",
    "/obj/projectile/magic/teleport",
    "/obj/projectile/magic/safety",
    "/obj/projectile/magic/door",
    "/obj/machinery/door/airlock/wood",
    "/turf/open/floor/plating",
    "/obj/effect/landmark/start/wizard",
    "/area/centcom/wizard_station",
    "/obj/item/clothing/suit/wizrobe/red",
];

/// The uplink items sold here: the ones whose item exists in this codebase. The rest of tg's
/// catalog needs mechanics that are not modelled yet (explosions, implants, chameleon).
// shortcut: a curated list; grow it as the items' mechanics land.
pub const UPLINK_ITEMS: [&str; 30] = [
    // Guns that fire (`ss13_game::guns`): the traitors' Makarov case and .357 revolver, the
    // operatives' C-20r case, and their ammunition.
    "/datum/uplink_item/dangerous/pistol",
    "/datum/uplink_item/ammo/pistol",
    "/datum/uplink_item/dangerous/revolver",
    "/datum/uplink_item/ammo/revolver",
    "/datum/uplink_item/weapon_kits/medium_cost/smg",
    "/datum/uplink_item/ammo_nuclear/basic/smg",
    "/datum/uplink_item/device_tools/toolbox",
    "/datum/uplink_item/device_tools/duffelbag",
    "/datum/uplink_item/device_tools/military_belt",
    "/datum/uplink_item/device_tools/tactical_medkit",
    // shortcut: the key cannot be put into a headset yet (no `keyslot` changes).
    "/datum/uplink_item/device_tools/encryptionkey",
    // The emag and the doorjack (`ss13_game::emag`): the emag does not open airlocks, the
    // doorjack only does.
    "/datum/uplink_item/device_tools/emag",
    "/datum/uplink_item/device_tools/doorjack",
    // shortcut: an ID card without the agent card's forging and access copying.
    "/datum/uplink_item/stealthy_tools/agent_card",
    "/datum/uplink_item/stealthy_tools/syndigaloshes",
    // C-4 (`ss13_game::explosions::sources`): timer, planting, the blast; no wires or
    // assemblies.
    "/datum/uplink_item/explosives/c4",
    // The nuclear operatives' bag of C-4.
    "/datum/uplink_item/explosives/military_grade/c4bag",
    // X-4, the frag grenade, the minibomb and the EMP kit (`ss13_game::grenades`, the EMP
    // implanter of `ss13_game::implants`).
    "/datum/uplink_item/explosives/x4",
    "/datum/uplink_item/explosives/frag",
    "/datum/uplink_item/explosives/syndicate_minibomb",
    "/datum/uplink_item/explosives/emp",
    "/datum/uplink_item/implants/freedom",
    // The sleeping carp scroll (`ss13_game::martial`).
    "/datum/uplink_item/stealthy_weapons/martialarts",
    "/datum/uplink_item/device_tools/stimpack",
    // The energy sword, the energy dagger and the sleepy pen
    // (`ss13_game::antag::syndie_items`).
    "/datum/uplink_item/dangerous/sword",
    "/datum/uplink_item/stealthy_weapons/edagger",
    "/datum/uplink_item/stealthy_weapons/sleepy_pen",
    // The failsafe code (`ss13_game::antag::uplink`; its item is abstract).
    "/datum/uplink_item/device_tools/failsafe",
    // Not sold (`purchasable_from = NONE`): brings the raw telecrystal's prototype for
    // `buy_raw_tc`.
    "/datum/uplink_item/bundles_tc/telecrystal",
    "/datum/uplink_item/badass/balloon",
];

/// tg `/datum/uplink_category/discounts`: the category of an uplink's sales.
pub const DISCOUNT_CATEGORY: &str = "/datum/uplink_category/discounts";

/// A `/datum/uplink_item` (the fields `/datum/asset/json/uplink` sends).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct UplinkItemDef {
    /// The datum type: the `buy` act's `path`.
    pub path: String,
    pub name: String,
    pub desc: String,
    /// The spawned item type.
    pub item: String,
    pub cost: u32,
    /// `category.name`.
    pub category: String,
    pub purchasable_from: u32,
    /// `-1`: unlimited.
    pub limited_stock: i32,
    pub lock_other_purchases: bool,
    pub cant_discount: bool,
    /// `limited_discount_stock`: the stock of this item on sale (`-1`: as `limited_stock`).
    #[serde(default = "no_limit")]
    pub limited_discount_stock: i32,
    pub population_minimum: u32,
    pub restricted_roles: Vec<String>,
}

/// A `/datum/objective_item/steal` of `objective_type` `normal` (tg `GLOB.possible_items`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct StealItemDef {
    pub path: String,
    /// How the objective names it (`"Украдите [name]"`).
    pub name: String,
    pub targetitem: String,
    /// Jobs that cannot get this objective.
    pub excludefromjob: Vec<String>,
    /// Only a target while one exists on the station.
    pub exists_on_map: bool,
    pub altitems: Vec<String>,
    /// The type has its own `check_special_completion`, `target_exists` or `New` (an AI in the
    /// card, a supermatter engine, extraction kits handed out).
    pub special: bool,
}

/// One employer of `strings/antagonist_flavor/traitor_flavor.json`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TraitorFlavor {
    #[serde(default)]
    pub allies: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub introduction: String,
    #[serde(default)]
    pub roundend_report: String,
    #[serde(default)]
    pub ui_theme: String,
    #[serde(default)]
    pub uplink: String,
}

fn no_limit() -> i32 {
    -1
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AntagData {
    /// Category names by `weight`, highest first (`cmp_uplink_category_desc`).
    pub uplink_categories: Vec<String>,
    /// [`DISCOUNT_CATEGORY`]'s name.
    #[serde(default)]
    pub discount_category: String,
    pub uplink_items: Vec<UplinkItemDef>,
    pub steal_items: Vec<StealItemDef>,
    /// `TRAITOR_FLAVOR_FILE` by employer name, sorted.
    #[serde(default)]
    pub traitor_flavor: Vec<(String, TraitorFlavor)>,
    /// `strings/phonetic_alphabet.txt` (`GLOB.phonetic_alphabet`).
    #[serde(default)]
    pub phonetic_alphabet: Vec<String>,
    /// [`NUKEOP_OUTFITS`] that resolve, in that order.
    #[serde(default)]
    pub nukeop_outfits: Vec<crate::jobs::Outfit>,
    /// `strings/names/operative_alias.txt` (`GLOB.operative_aliases`).
    #[serde(default)]
    pub operative_aliases: Vec<String>,
    /// `strings/names/last.txt` (`GLOB.last_names`).
    #[serde(default)]
    pub last_names: Vec<String>,
}

/// Reads the traitor flavor and the phonetic alphabet from the BYOND checkout's `strings/`;
/// a missing file leaves its part empty (reported).
pub fn read_strings(source: &Path, data: &mut AntagData) -> Vec<String> {
    let mut problems = Vec::new();
    let flavor = source.join("strings/antagonist_flavor/traitor_flavor.json");
    match std::fs::read(&flavor)
        .map_err(anyhow::Error::from)
        .and_then(|b| {
            Ok(serde_json::from_slice::<
                std::collections::BTreeMap<String, TraitorFlavor>,
            >(&b)?)
        }) {
        Ok(map) => data.traitor_flavor = map.into_iter().collect(),
        Err(e) => problems.push(format!("{}: {e}", flavor.display())),
    }
    let lines = |file: &str, problems: &mut Vec<String>| {
        let path = source.join(file);
        match std::fs::read_to_string(&path) {
            Ok(text) => text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect(),
            Err(e) => {
                problems.push(format!("{}: {e}", path.display()));
                Vec::new()
            }
        }
    };
    data.phonetic_alphabet = lines("strings/phonetic_alphabet.txt", &mut problems);
    data.operative_aliases = lines("strings/names/operative_alias.txt", &mut problems);
    data.last_names = lines("strings/names/last.txt", &mut problems);
    problems
}

impl AntagData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let text = ss13_data::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        ron::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("antag data serializes"),
        );
        out.push('\n');
        out
    }

    /// Item types the uplink spawns: the converter emits prototypes for them.
    pub fn item_types(&self) -> BTreeSet<&str> {
        self.uplink_items.iter().map(|i| i.item.as_str()).collect()
    }

    /// Item types the nuclear operatives' outfits spawn, and [`NUKEOP_RUNTIME_TYPES`].
    pub fn nukeop_types(&self) -> BTreeSet<&str> {
        self.nukeop_outfits
            .iter()
            .flat_map(|o| {
                o.items
                    .iter()
                    .map(|(_, p)| p.as_str())
                    .chain(o.backpack_contents.iter().map(|(p, _)| p.as_str()))
            })
            .chain(NUKEOP_RUNTIME_TYPES)
            .collect()
    }
}

fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn path(v: Option<&Value>) -> Option<String> {
    v?.get("path")?.as_str().map(str::to_owned)
}

fn int(v: Option<&Value>) -> i64 {
    v.and_then(Value::as_f64).map_or(0, |f| f as i64)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(|v| v.get("list"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_owned).or_else(|| path(Some(i))))
                .collect()
        })
        .unwrap_or_default()
}

/// The catalog ([`UPLINK_ITEMS`] that resolve to an item) and every normal steal target.
pub fn resolve(types: &DmTypes) -> AntagData {
    let mut categories: Vec<(i64, String)> = types
        .types
        .keys()
        .filter(|p| p.starts_with("/datum/uplink_category/"))
        .map(|p| {
            (
                int(types.resolved(p, "weight")),
                text(types.resolved(p, "name")),
            )
        })
        .collect();
    // `sortTim(cmp_uplink_category_desc)`: stable by weight; the dump's keys are path-sorted.
    categories.sort_by_key(|(w, _)| -w);
    let uplink_items: Vec<UplinkItemDef> = UPLINK_ITEMS
        .iter()
        .filter(|p| types.types.contains_key(**p))
        .filter_map(|&p| {
            let item = path(types.resolved(p, "item"))?;
            let category = path(types.resolved(p, "category"))
                .map(|c| text(types.resolved(&c, "name")))
                .unwrap_or_default();
            Some(UplinkItemDef {
                path: p.to_owned(),
                name: text(types.resolved(p, "name")),
                desc: text(types.resolved(p, "desc")),
                item,
                cost: int(types.resolved(p, "cost")).max(0) as u32,
                category,
                purchasable_from: int(types.resolved(p, "purchasable_from")) as u32,
                limited_stock: int(types.resolved(p, "limited_stock")) as i32,
                lock_other_purchases: int(types.resolved(p, "lock_other_purchases")) != 0,
                cant_discount: int(types.resolved(p, "cant_discount")) != 0,
                limited_discount_stock: types
                    .resolved(p, "limited_discount_stock")
                    .map_or(-1, |v| int(Some(v)) as i32),
                population_minimum: int(types.resolved(p, "population_minimum")).max(0) as u32,
                restricted_roles: strings(types.resolved(p, "restricted_roles")),
            })
        })
        .collect();
    let mut steal_items: Vec<StealItemDef> = types
        .types
        .iter()
        .filter(|(p, _)| p.starts_with("/datum/objective_item/steal/"))
        .filter(|(p, _)| text(types.resolved(p, "objective_type")) == "normal")
        .filter_map(|(p, ty)| {
            Some(StealItemDef {
                path: p.clone(),
                name: text(types.resolved(p, "name")),
                targetitem: path(types.resolved(p, "targetitem"))?,
                excludefromjob: strings(types.resolved(p, "excludefromjob")),
                exists_on_map: int(types.resolved(p, "exists_on_map")) != 0,
                altitems: strings(types.resolved(p, "altitems")),
                special: ty.procs.iter().any(|f| {
                    ["check_special_completion", "target_exists", "New"].contains(&f.as_str())
                }),
            })
        })
        .collect();
    steal_items.sort_by(|a, b| a.path.cmp(&b.path));
    let nukeop_outfits = NUKEOP_OUTFITS
        .iter()
        .filter(|p| types.types.contains_key(**p))
        .map(|p| crate::jobs::outfit(types, p))
        .collect();
    AntagData {
        uplink_categories: categories.into_iter().map(|(_, n)| n).collect(),
        discount_category: text(types.resolved(DISCOUNT_CATEGORY, "name")),
        uplink_items,
        steal_items,
        nukeop_outfits,
        ..AntagData::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/uplink_category": { "parent": "/datum", "declared_vars": { "name": null, "weight": 0 } },
        "/datum/uplink_category/device_tools": { "parent": "/datum/uplink_category",
            "declared_vars": { "name": "Misc. Gadgets", "weight": 3 } },
        "/datum/uplink_category/badassery": { "parent": "/datum/uplink_category",
            "declared_vars": { "name": "(Pointless) Badassery", "weight": 0 } },
        "/datum/uplink_category/explosives": { "parent": "/datum/uplink_category",
            "declared_vars": { "name": "Explosives", "weight": 6 } },
        "/datum/uplink_category/discounts": { "parent": "/datum/uplink_category",
            "declared_vars": { "name": "Discounted Gear", "weight": -1 } },
        "/datum/uplink_item": { "parent": "/datum", "declared_vars": { "cost": 0,
            "item": null, "category": null, "purchasable_from": 16777215, "limited_stock": -1,
            "lock_other_purchases": 0, "cant_discount": 0, "population_minimum": 0,
            "restricted_roles": { "list": [] }, "name": "item name", "desc": "item description" } },
        "/datum/uplink_item/device_tools": { "parent": "/datum/uplink_item",
            "declared_vars": { "category": { "path": "/datum/uplink_category/device_tools" } } },
        "/datum/uplink_item/device_tools/toolbox": { "parent": "/datum/uplink_item/device_tools",
            "declared_vars": { "cost": 1, "name": "Full Syndicate Toolbox", "desc": "Tools.",
            "item": { "path": "/obj/item/storage/toolbox/syndicate" } } },
        "/datum/uplink_item/badass": { "parent": "/datum/uplink_item",
            "declared_vars": { "category": { "path": "/datum/uplink_category/badassery" } } },
        "/datum/uplink_item/badass/balloon": { "parent": "/datum/uplink_item/badass",
            "declared_vars": { "cost": 20, "lock_other_purchases": 1, "cant_discount": 1,
            "name": "Syndicate Balloon", "item": { "path": "/obj/item/toy/balloon/syndicate" } } },
        "/datum/objective_item": { "parent": "/datum", "declared_vars": {
            "name": "a silly bike horn! Honk!", "targetitem": { "path": "/obj/item/bikehorn" },
            "objective_type": "normal", "excludefromjob": { "list": [] }, "exists_on_map": 0,
            "altitems": { "list": [] } },
            "procs": ["check_special_completion"] },
        "/datum/objective_item/steal": { "parent": "/datum/objective_item" },
        "/datum/objective_item/steal/caplaser": { "parent": "/datum/objective_item/steal",
            "declared_vars": { "name": "капитанское антикварное лазерное оружие",
            "targetitem": { "path": "/obj/item/gun/energy/laser/captain" },
            "excludefromjob": { "list": ["Captain"] }, "exists_on_map": 1 } },
        "/datum/objective_item/steal/nukedisc": { "parent": "/datum/objective_item/steal",
            "declared_vars": { "name": "диск", "targetitem": { "path": "/obj/item/disk/nuclear" } },
            "procs": ["check_special_completion"] },
        "/datum/objective_item/steal/traitor": { "parent": "/datum/objective_item/steal",
            "declared_vars": { "objective_type": "traitor" } },
        "/datum/objective_item/steal/traitor/fireaxe": { "parent": "/datum/objective_item/steal/traitor",
            "declared_vars": { "name": "пожарный топор", "targetitem": { "path": "/obj/item/fireaxe" } } }
      }
    }"##;

    #[test]
    fn catalog_and_steal_targets_resolve_through_parents() {
        let data = resolve(&DmTypes::parse(TYPES.as_bytes()).unwrap());
        assert_eq!(
            data.uplink_categories,
            [
                "Explosives",
                "Misc. Gadgets",
                "(Pointless) Badassery",
                "Discounted Gear"
            ]
        );
        assert_eq!(data.discount_category, "Discounted Gear");
        let paths: Vec<&str> = data.uplink_items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/datum/uplink_item/device_tools/toolbox",
                "/datum/uplink_item/badass/balloon"
            ]
        );
        let toolbox = &data.uplink_items[0];
        assert_eq!(
            (
                toolbox.cost,
                toolbox.category.as_str(),
                toolbox.limited_stock
            ),
            (1, "Misc. Gadgets", -1)
        );
        assert_eq!(toolbox.limited_discount_stock, -1);
        assert_eq!(toolbox.purchasable_from & UPLINK_TRAITORS, UPLINK_TRAITORS);
        assert!(data.uplink_items[1].lock_other_purchases);
        let steal: Vec<(&str, bool, bool)> = data
            .steal_items
            .iter()
            .map(|s| (s.name.as_str(), s.exists_on_map, s.special))
            .collect();
        // Traitor-type items are not steal objectives; only types defining the proc are special.
        assert_eq!(
            steal,
            [
                ("капитанское антикварное лазерное оружие", true, false),
                ("диск", false, true)
            ]
        );
        assert_eq!(data.steal_items[0].excludefromjob, ["Captain"]);
        assert_eq!(data.item_types().len(), 2);
    }
}
