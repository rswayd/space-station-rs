//! Hydroponics data from the DM dump, written by `ss13-mapload convert` to [`PLANTS_FILE`]:
//! every `/obj/item/seeds` subtype as its `Initialize` leaves it (stats, product, growing icon
//! states, genes, `reagents_add`, `mutatelist`), every `/datum/plant_gene/trait` subtype
//! (`subtypesof`, what `add_random_traits` picks from), the tray types
//! (`/obj/machinery/hydroponics`) with the vars their process reads, the grown items' seeds and
//! the reagents a random reagent gene can be (`get_random_reagent_id`). The game
//! (`ss13_game::botany`) reads it at startup.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{is_under, num};
use crate::reagents::{amounts, path_value};

/// `<assets>/plants.ron`.
pub const PLANTS_FILE: &str = "plants.ron";
pub const SEED_ROOT: &str = "/obj/item/seeds";
pub const GROWN_ROOT: &str = "/obj/item/food/grown";
pub const GROWN_INEDIBLE_ROOT: &str = "/obj/item/grown";
pub const TRAY_ROOT: &str = "/obj/machinery/hydroponics";
pub const CONSTRUCTABLE_TRAY: &str = "/obj/machinery/hydroponics/constructable";
pub const SOIL_ROOT: &str = "/obj/machinery/hydroponics/soil";
pub const TRAIT_ROOT: &str = "/datum/plant_gene/trait";
pub const FUNGAL: &str = "/datum/plant_gene/trait/plant_type/fungal_metabolism";
pub const GLOW_TRAIT: &str = "/datum/plant_gene/trait/glow";
/// `/obj/item/graft`: a cutting taken with secateurs.
pub const GRAFT: &str = "/obj/item/graft";
/// What product traits leave behind: the teleport goo and the potato battery.
pub const PRODUCE_RUNTIME_TYPES: [&str; 2] = [
    "/obj/effect/decal/cleanable/molten_object",
    "/obj/item/stock_parts/power_store/cell/potato",
];
/// What unique genes and logs make (`ss13_game::botany`): planks, rods and bamboo cut from logs,
/// the mobs of dormant ferocity.
pub const BOTANY_RUNTIME_TYPES: [&str; 5] = [
    "/obj/item/stack/sheet/mineral/wood",
    "/obj/item/stack/rods",
    "/obj/item/stack/sheet/mineral/bamboo",
    "/mob/living/basic/killer_tomato",
    "/mob/living/basic/mushroom",
];
const REAGENT_ROOT: &str = "/datum/reagent";
const URANIUM: &str = "/datum/reagent/uranium";
/// `REAGENT_SPAWN_RANDOM_PRODUCERS`.
const REAGENT_SPAWN_RANDOM_PRODUCERS: u32 = 1 << 0;
/// `/obj/machinery/hydroponics/proc/weedinvasion`: `rand(1, 18)` over these, `(seed, low roll)`
/// from the highest band down (the last takes every roll below 4).
pub const WEED_INVASION: [(&str, u32); 8] = [
    ("/obj/item/seeds/reishi", 16),
    ("/obj/item/seeds/nettle", 14),
    ("/obj/item/seeds/harebell", 12),
    ("/obj/item/seeds/amanita", 10),
    ("/obj/item/seeds/chanter", 8),
    ("/obj/item/seeds/tower", 6),
    ("/obj/item/seeds/plump", 4),
    ("/obj/item/seeds/starthistle", 1),
];
/// `/obj/machinery/hydroponics/proc/mutateweed`.
pub const MUTATED_WEEDS: [&str; 4] = [
    "/obj/item/seeds/liberty",
    "/obj/item/seeds/angel",
    "/obj/item/seeds/nettle/death",
    "/obj/item/seeds/kudzu",
];

/// An `/obj/item/seeds` type after `Initialize` (icon state defaults filled in).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SeedDef {
    pub path: String,
    pub name: String,
    pub plantname: String,
    /// The harvested type; empty: none.
    pub product: String,
    pub growing_icon: String,
    pub icon_grow: String,
    pub icon_dead: String,
    /// Empty: the last growth stage (`"[icon_grow][growthstages]"`).
    pub icon_harvest: String,
    pub plant_icon_offset: i16,
    pub lifespan: f32,
    pub endurance: f32,
    pub maturation: f32,
    pub production: f32,
    /// `yield`; `-1`: never harvested.
    pub yield_: f32,
    pub potency: f32,
    pub growthstages: u32,
    pub instability: f32,
    pub rarity: f32,
    pub weed_rate: f32,
    pub weed_chance: f32,
    pub seed_flags: u32,
    pub resistance_flags: u32,
    /// `genes`: trait types.
    pub genes: Vec<String>,
    /// `reagents_add`: reagent type and rate, in order.
    pub reagents: Vec<(String, f32)>,
    pub mutatelist: Vec<String>,
    pub graft_gene: String,
}

/// A `/datum/plant_gene/trait` type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TraitDef {
    pub path: String,
    pub name: String,
    pub description: String,
    pub rate: f32,
    pub trait_ids: u32,
    pub trait_flags: u32,
    pub mutability_flags: u32,
    pub seed_blacklist: Vec<String>,
    /// `/datum/plant_gene/trait/glow` `glow_color` (`"#RRGGBB"`); empty for other traits.
    #[serde(default)]
    pub glow_color: String,
}

/// An `/obj/machinery/hydroponics` type with what `Initialize`/`RefreshParts` leave in the vars
/// the process reads.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TrayDef {
    pub path: String,
    pub maxwater: f32,
    pub maxnutri: f32,
    /// `rating` (the servo tier of a constructable tray).
    pub rating: f32,
    pub tray_flags: u32,
    pub plant_offset_y: i16,
    pub alt_tray: bool,
    /// Soil overrides `update_status_light_overlays` with nothing.
    pub status_lights: bool,
}

/// A grown item type (`/obj/item/food/grown`, `/obj/item/grown`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct GrownDef {
    pub path: String,
    /// Its `seed` type.
    pub seed: String,
    pub bite_consumption_mod: f32,
    /// `splat_type` (the squash decal); empty: none (inedible grown items).
    #[serde(default)]
    pub splat_type: String,
    /// `filling_color` (`"#RRGGBB"`), the smudge's colour; empty: none.
    #[serde(default)]
    pub filling_color: String,
    /// `distill_reagent` (auto-distilling); empty: fruit wine.
    #[serde(default)]
    pub distill_reagent: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PlantData {
    pub seeds: Vec<SeedDef>,
    pub traits: Vec<TraitDef>,
    pub trays: Vec<TrayDef>,
    pub grown: Vec<GrownDef>,
    /// Reagents with `REAGENT_SPAWN_RANDOM_PRODUCERS` (`get_random_reagent_id()`).
    pub random_reagents: Vec<String>,
    /// `tox_damage` of `/datum/reagent/uranium` and its subtypes (`on_hydroponics_apply`).
    pub tox_damage: Vec<(String, f32)>,
}

fn text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o
            .get("resource")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

fn paths(v: Option<&Value>) -> Vec<String> {
    amounts(v)
        .unwrap_or_default()
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

fn concrete(types: &DmTypes, path: &str) -> bool {
    path_value(types.resolved(path, "abstract_type")).as_deref() != Some(path)
}

fn seed(types: &DmTypes, path: &str) -> SeedDef {
    let get = |var: &str| types.resolved(path, var);
    let n = |var: &str| num(get(var)).unwrap_or(0.0) as f32;
    let species = text(get("species"));
    let genes = paths(get("genes"));
    let yield_ = n("yield");
    let or = |var: &str, default: String| {
        Some(text(get(var)))
            .filter(|s| !s.is_empty())
            .unwrap_or(default)
    };
    let icon_harvest = if genes.iter().any(|g| is_under(g, FUNGAL)) || yield_ == -1.0 {
        text(get("icon_harvest"))
    } else {
        or("icon_harvest", format!("{species}-harvest"))
    };
    SeedDef {
        path: path.to_owned(),
        name: text(get("name")),
        plantname: text(get("plantname")),
        product: path_value(get("product")).unwrap_or_default(),
        growing_icon: text(get("growing_icon")),
        icon_grow: or("icon_grow", format!("{species}-grow")),
        icon_dead: or("icon_dead", format!("{species}-dead")),
        icon_harvest,
        plant_icon_offset: n("plant_icon_offset") as i16,
        lifespan: n("lifespan"),
        endurance: n("endurance"),
        maturation: n("maturation"),
        production: n("production"),
        yield_,
        potency: n("potency"),
        growthstages: n("growthstages") as u32,
        instability: n("instability"),
        rarity: n("rarity"),
        weed_rate: n("weed_rate"),
        weed_chance: n("weed_chance"),
        seed_flags: n("seed_flags") as u32,
        resistance_flags: n("resistance_flags") as u32,
        genes,
        reagents: amounts(get("reagents_add")).unwrap_or_default(),
        mutatelist: paths(get("mutatelist")),
        graft_gene: path_value(get("graft_gene")).unwrap_or_default(),
    }
}

fn plant_trait(types: &DmTypes, path: &str) -> TraitDef {
    let get = |var: &str| types.resolved(path, var);
    let n = |var: &str| num(get(var)).unwrap_or(0.0) as f32;
    TraitDef {
        path: path.to_owned(),
        name: text(get("name")),
        description: text(get("description")),
        rate: n("rate"),
        trait_ids: n("trait_ids") as u32,
        trait_flags: n("trait_flags") as u32,
        mutability_flags: n("mutability_flags") as u32,
        seed_blacklist: paths(get("seed_blacklist")),
        glow_color: if is_under(path, GLOW_TRAIT) {
            text(get("glow_color"))
        } else {
            String::new()
        },
    }
}

fn tray(types: &DmTypes, path: &str) -> TrayDef {
    let get = |var: &str| types.resolved(path, var);
    let n = |var: &str| num(get(var)).unwrap_or(0.0) as f32;
    let (mut maxwater, mut maxnutri) = (n("maxwater"), n("maxnutri"));
    let mut rating = n("rating");
    if is_under(path, CONSTRUCTABLE_TRAY) {
        // `RefreshParts` with the board's two tier 1 matter bins and tier 1 servo:
        // `maxwater = bins * 50`, `maxnutri = bins * 5 + STATIC_NUTRIENT_CAPACITY`.
        // shortcut: a mapped tray has tier 1 numbers whatever its board (the `fullupgrade` one
        // has tier 4); built ones run `RefreshParts` (`ss13_game::botany::refresh_parts`).
        let bins = 2.0;
        maxwater = bins * 50.0;
        maxnutri = bins * 5.0 + 10.0;
        rating = 1.0;
    }
    TrayDef {
        path: path.to_owned(),
        maxwater,
        maxnutri,
        rating: rating.max(1.0),
        tray_flags: n("tray_flags") as u32,
        plant_offset_y: n("plant_offset_y") as i16,
        alt_tray: n("alt_tray") != 0.0,
        status_lights: !is_under(path, SOIL_ROOT),
    }
}

/// The plant data of the dump, in path order.
pub fn resolve(types: &DmTypes) -> PlantData {
    let all: BTreeSet<&str> = types.types.keys().map(String::as_str).collect();
    let under = |root: &'static str| {
        all.iter()
            .copied()
            .filter(move |p| is_under(p, root) && *p != root)
    };
    let mut data = PlantData {
        seeds: under(SEED_ROOT)
            .filter(|p| concrete(types, p))
            .map(|p| seed(types, p))
            .collect(),
        traits: under(TRAIT_ROOT).map(|p| plant_trait(types, p)).collect(),
        trays: std::iter::once(TRAY_ROOT)
            .chain(under(TRAY_ROOT))
            .filter(|p| all.contains(p))
            .map(|p| tray(types, p))
            .collect(),
        ..Default::default()
    };
    for p in under(GROWN_ROOT).chain(under(GROWN_INEDIBLE_ROOT)) {
        if let Some(seed) = path_value(types.resolved(p, "seed")) {
            let bite = types.resolved(p, "bite_consumption_mod");
            data.grown.push(GrownDef {
                path: p.to_owned(),
                seed,
                bite_consumption_mod: bite.and_then(|v| num(Some(v))).unwrap_or(1.0) as f32,
                splat_type: path_value(types.resolved(p, "splat_type")).unwrap_or_default(),
                filling_color: text(types.resolved(p, "filling_color")),
                distill_reagent: path_value(types.resolved(p, "distill_reagent"))
                    .unwrap_or_default(),
            });
        }
    }
    for p in under(REAGENT_ROOT).filter(|p| concrete(types, p)) {
        let flags = num(types.resolved(p, "randomized_spawns")).unwrap_or(0.0) as u32;
        if flags & REAGENT_SPAWN_RANDOM_PRODUCERS != 0 {
            data.random_reagents.push(p.to_owned());
        }
    }
    for p in std::iter::once(URANIUM).chain(under(URANIUM)) {
        if all.contains(p) {
            let tox = num(types.resolved(p, "tox_damage")).unwrap_or(0.0) as f32;
            data.tox_damage.push((p.to_owned(), tox));
        }
    }
    data
}

/// Every type a needed seed, grown item or tray can turn into, transitively: seeds' products and
/// `mutatelist`, grown items' seeds, squash decals and grown trash (banana peels), the weeds
/// that overtake a tray, the grafts cut from it and what product traits leave behind.
pub fn plant_closure<'a>(
    types: &DmTypes,
    needed: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack: Vec<String> = needed.into_iter().map(str::to_owned).collect();
    let mut seen = BTreeSet::new();
    while let Some(path) = stack.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let mut next = Vec::new();
        if is_under(&path, SEED_ROOT) {
            next.extend(path_value(types.resolved(&path, "product")));
            next.extend(paths(types.resolved(&path, "mutatelist")));
        } else if is_under(&path, GROWN_ROOT) || is_under(&path, GROWN_INEDIBLE_ROOT) {
            next.extend(path_value(types.resolved(&path, "seed")));
            next.extend(path_value(types.resolved(&path, "splat_type")));
            next.extend(
                path_value(types.resolved(&path, "trash_type"))
                    .filter(|t| is_under(t, GROWN_INEDIBLE_ROOT)),
            );
            next.extend(PRODUCE_RUNTIME_TYPES.iter().map(|p| (*p).to_owned()));
        } else if is_under(&path, TRAY_ROOT) {
            next.extend(WEED_INVASION.iter().map(|(p, _)| (*p).to_owned()));
            next.extend(MUTATED_WEEDS.iter().map(|p| (*p).to_owned()));
            next.push(GRAFT.to_owned());
        }
        for p in next {
            // Products that are mobs (seedlings) are not spawned here.
            if is_under(&p, "/obj") && types.types.contains_key(&p) && out.insert(p.clone()) {
                stack.push(p);
            }
        }
    }
    out
}

impl PlantData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        crate::read_ron(path)
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("plant data serializes"),
        );
        out.push('\n');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/plant_gene": { "parent": "/datum", "declared_vars": {
            "name": null, "description": "", "mutability_flags": null } },
        "/datum/plant_gene/trait": { "parent": "/datum/plant_gene", "declared_vars": {
            "rate": 0.05, "trait_ids": null, "trait_flags": 0, "seed_blacklist": null } },
        "/datum/plant_gene/trait/repeated_harvest": { "parent": "/datum/plant_gene/trait",
            "declared_vars": { "name": "Perennial Growth", "mutability_flags": 7,
            "seed_blacklist": { "list": [{ "path": "/obj/item/seeds/replicapod" }] } } },
        "/datum/plant_gene/trait/plant_type": { "parent": "/datum/plant_gene/trait",
            "declared_vars": { "trait_ids": 2, "mutability_flags": 4 } },
        "/datum/plant_gene/trait/plant_type/fungal_metabolism": {
            "parent": "/datum/plant_gene/trait/plant_type", "declared_vars": { "name": "Fungal Vitality" } },
        "/atom": { "parent": "/datum" },
        "/obj": { "parent": "/atom" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/seeds": { "parent": "/obj/item", "declared_vars": {
            "abstract_type": { "path": "/obj/item/seeds" }, "plantname": "Plants", "product": null,
            "species": "", "growing_icon": { "resource": "growing.dmi" }, "icon_grow": null,
            "icon_dead": null, "icon_harvest": null, "plant_icon_offset": 8, "lifespan": 25,
            "endurance": 15, "maturation": 6, "production": 6, "yield": 3, "potency": 10,
            "growthstages": 6, "instability": 5, "rarity": 0, "mutatelist": null,
            "genes": { "list": [] }, "reagents_add": null, "weed_rate": 1, "weed_chance": 5,
            "seed_flags": 1 } },
        "/obj/item/seeds/tomato": { "parent": "/obj/item/seeds", "declared_vars": {
            "name": "tomato seed pack", "plantname": "Tomato Plants", "species": "tomato",
            "product": { "path": "/obj/item/food/grown/tomato" }, "maturation": 8,
            "instability": 25, "icon_grow": "tomato-grow", "icon_dead": "tomato-dead",
            "genes": { "list": [{ "path": "/datum/plant_gene/trait/repeated_harvest" }] },
            "mutatelist": { "list": [{ "path": "/obj/item/seeds/tomato/blue" }] },
            "reagents_add": { "list": [
                { "assoc": [{ "path": "/datum/reagent/consumable/nutriment/vitamin" }, 0.04] },
                { "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 0.1] }] } } },
        "/obj/item/seeds/tomato/blue": { "parent": "/obj/item/seeds/tomato", "declared_vars": {
            "species": "bluetomato", "product": { "path": "/obj/item/food/grown/tomato/blue" },
            "mutatelist": null } },
        "/obj/item/seeds/chanter": { "parent": "/obj/item/seeds", "declared_vars": {
            "species": "chanter", "product": { "path": "/mob/living/shroom" },
            "genes": { "list": [{ "path": "/datum/plant_gene/trait/plant_type/fungal_metabolism" }] } } },
        "/obj/item/food": { "parent": "/obj/item" },
        "/obj/item/food/grown": { "parent": "/obj/item/food", "declared_vars": { "seed": null,
            "splat_type": { "path": "/obj/effect/decal/cleanable/food/plant_smudge" },
            "filling_color": null, "distill_reagent": null } },
        "/obj/item/food/grown/tomato": { "parent": "/obj/item/food/grown", "declared_vars": {
            "seed": { "path": "/obj/item/seeds/tomato" }, "filling_color": "#FF6347" } },
        "/obj/effect": { "parent": "/obj" },
        "/obj/effect/decal": { "parent": "/obj/effect" },
        "/obj/effect/decal/cleanable": { "parent": "/obj/effect/decal" },
        "/obj/effect/decal/cleanable/food": { "parent": "/obj/effect/decal/cleanable" },
        "/obj/effect/decal/cleanable/food/plant_smudge": {
            "parent": "/obj/effect/decal/cleanable/food" },
        "/datum/plant_gene/trait/glow": { "parent": "/datum/plant_gene/trait",
            "declared_vars": { "name": "Bioluminescence", "glow_color": "#C3E381" } },
        "/obj/item/food/grown/tomato/blue": { "parent": "/obj/item/food/grown/tomato",
            "declared_vars": { "seed": { "path": "/obj/item/seeds/tomato/blue" } } },
        "/obj/machinery": { "parent": "/obj" },
        "/obj/machinery/hydroponics": { "parent": "/obj/machinery", "declared_vars": {
            "maxwater": 100, "maxnutri": 20, "rating": 1, "tray_flags": 1, "plant_offset_y": 0,
            "alt_tray": 0 } },
        "/obj/machinery/hydroponics/constructable": { "parent": "/obj/machinery/hydroponics" },
        "/obj/machinery/hydroponics/soil": { "parent": "/obj/machinery/hydroponics",
            "declared_vars": { "maxnutri": 15, "tray_flags": 2 } }
      }
    }"##;

    fn types() -> DmTypes {
        DmTypes::parse(TYPES.as_bytes()).unwrap()
    }

    #[test]
    fn seeds_get_their_initialize_icon_states_genes_and_reagents() {
        let data = resolve(&types());
        let tomato = data
            .seeds
            .iter()
            .find(|s| s.path == "/obj/item/seeds/tomato")
            .unwrap();
        assert_eq!(
            (
                tomato.lifespan,
                tomato.maturation,
                tomato.yield_,
                tomato.instability
            ),
            (25.0, 8.0, 3.0, 25.0)
        );
        assert_eq!(tomato.growing_icon, "growing.dmi");
        assert_eq!(
            (
                tomato.icon_grow.as_str(),
                tomato.icon_dead.as_str(),
                tomato.icon_harvest.as_str()
            ),
            ("tomato-grow", "tomato-dead", "tomato-harvest")
        );
        assert_eq!(tomato.genes, ["/datum/plant_gene/trait/repeated_harvest"]);
        assert_eq!(tomato.reagents.len(), 2);
        assert_eq!(tomato.reagents[1].1, 0.1);
        assert_eq!(tomato.mutatelist, ["/obj/item/seeds/tomato/blue"]);
        // A mushroom has no harvest sprite: its last growth stage.
        let chanter = data
            .seeds
            .iter()
            .find(|s| s.path.ends_with("chanter"))
            .unwrap();
        assert_eq!(chanter.icon_harvest, "");
        assert!(data.seeds.iter().all(|s| s.path != SEED_ROOT), "abstract");
        let fungal = data.traits.iter().find(|t| t.path == FUNGAL).unwrap();
        assert_eq!((fungal.trait_ids, fungal.mutability_flags), (2, 4));
        let rh = data
            .traits
            .iter()
            .find(|t| t.path.ends_with("repeated_harvest"))
            .unwrap();
        assert_eq!(rh.seed_blacklist, ["/obj/item/seeds/replicapod"]);
        let constructable = data
            .trays
            .iter()
            .find(|t| t.path == CONSTRUCTABLE_TRAY)
            .unwrap();
        assert_eq!(
            (constructable.maxwater, constructable.maxnutri),
            (100.0, 20.0)
        );
        let soil = data.trays.iter().find(|t| t.path == SOIL_ROOT).unwrap();
        assert_eq!((soil.maxnutri, soil.status_lights), (15.0, false));
        assert!(
            data.grown
                .iter()
                .any(|g| g.path == "/obj/item/food/grown/tomato"
                    && g.seed == "/obj/item/seeds/tomato"
                    && g.bite_consumption_mod == 1.0
                    && g.splat_type == "/obj/effect/decal/cleanable/food/plant_smudge"
                    && g.filling_color == "#FF6347"
                    && g.distill_reagent.is_empty())
        );
        let glow = data.traits.iter().find(|t| t.path == GLOW_TRAIT).unwrap();
        assert_eq!(glow.glow_color, "#C3E381");
        assert!(
            data.traits
                .iter()
                .all(|t| t.path == GLOW_TRAIT || t.glow_color.is_empty())
        );
    }

    #[test]
    fn closure_reaches_products_mutations_and_their_seeds() {
        let types = types();
        let out = plant_closure(
            &types,
            ["/obj/item/seeds/tomato", "/obj/item/seeds/chanter"],
        );
        assert!(out.contains("/obj/item/food/grown/tomato"));
        assert!(out.contains("/obj/item/seeds/tomato/blue"));
        assert!(out.contains("/obj/item/food/grown/tomato/blue"));
        assert!(!out.contains("/mob/living/shroom"), "mobs are not spawned");
        assert!(
            out.contains("/obj/effect/decal/cleanable/food/plant_smudge"),
            "squash decal"
        );
    }
}
