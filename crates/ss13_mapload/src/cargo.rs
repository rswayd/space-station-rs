//! Cargo data from the DM dump, written by `ss13-mapload convert` to [`CARGO_FILE`]: the supply
//! packs (`/datum/supply_pack/*`, `code/modules/cargo/packs/`, with BandaStation's modpacks) and
//! the exports (`/datum/export/*`, `code/modules/cargo/exports/`) whose value the game can compute.
//! The game (`ss13_game::cargo`) reads it at startup.
//!
//! - Packs: tg `SSshuttle.Initialize` keeps every subtype with a non-null `contains`; the same
//!   rule applies here. Packs whose own `fill()`, `generate()` or `generate_supply_packs()`
//!   replace the static contents are marked [`SupplyPackDef::custom_fill`].
//! - Exports: the ones tg values with the base procs (`get_base_cost` = `cost`,
//!   `get_amount` = 1), `/datum/export/stack` (the stack's amount) and `/datum/export/material`
//!   (the material per unit of a sheet stack, `* 0.8` for ore) with the market subtypes at their
//!   starting price (`value_per_unit * SHEET_MATERIAL_AMOUNT`). Exports with their own value procs
//!   (gas canisters, kegs, organs, manifests, ...) are left out.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::is_under;

/// `<assets>/cargo.ron`.
pub const CARGO_FILE: &str = "cargo.ron";
const PACK_ROOT: &str = "/datum/supply_pack";
const EXPORT_ROOT: &str = "/datum/export";
const STACK_EXPORT: &str = "/datum/export/stack";
const MATERIAL_EXPORT: &str = "/datum/export/material";
const MARKET_EXPORT: &str = "/datum/export/material/market";
/// `EXPORT_MARKET_STATION`.
const MARKET_STATION: &str = "supply";
/// `SHEET_MATERIAL_AMOUNT`.
pub const SHEET_MATERIAL_AMOUNT: f64 = 100.0;
/// The value procs an export may override; with any of them the value is not computable here.
const VALUE_PROCS: [&str; 4] = ["get_base_cost", "get_amount", "applies_to", "sell_object"];
/// The manifest paper every crate gets (`generateManifest`).
pub const MANIFEST_PAPER: &str = "/obj/item/paper/fluff/jobs/cargo/manifest";

/// `ORDER_*` flags (`code/__DEFINES/cargo.dm`).
pub mod order_flags {
    pub const INVISIBLE: u32 = 1 << 0;
    pub const EMAG_ONLY: u32 = 1 << 1;
    pub const CONTRABAND: u32 = 1 << 2;
    pub const GOODY: u32 = 1 << 3;
    pub const POD_ONLY: u32 = 1 << 4;
    pub const SPECIAL: u32 = 1 << 5;
    pub const SPECIAL_ENABLED: u32 = 1 << 6;
    pub const NOT_DEPARTMENTAL: u32 = 1 << 7;
    pub const DANGEROUS: u32 = 1 << 8;
}

/// A `/datum/supply_pack`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SupplyPackDef {
    /// The type path: tg's `id` and the console's `add` param.
    pub id: String,
    pub name: String,
    pub desc: String,
    /// The console category.
    pub group: String,
    /// `cost` before `pack_price_modifier`.
    pub cost: f64,
    /// Empty: none (`FALSE`).
    pub access: String,
    pub access_view: String,
    pub access_any: Vec<String>,
    /// `contains`: (type, amount), `amount || 1`.
    pub contains: Vec<(String, u32)>,
    pub crate_name: String,
    /// `None`: goodies (no crate).
    pub crate_type: Option<String>,
    pub order_flags: u32,
    /// The pack's own `fill()`, `generate()` or `generate_supply_packs()` decides the real
    /// contents.
    pub custom_fill: bool,
}

impl SupplyPackDef {
    pub fn has(&self, flag: u32) -> bool {
        self.order_flags & flag != 0
    }
}

/// How an export counts the units of an item (`get_amount`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ExportAmount {
    /// `/datum/export`: 1.
    One,
    /// `/datum/export/stack`: the stack's `amount`.
    Stack,
    /// `/datum/export/material`: sheets of the material per stack unit, by exact item type
    /// (`mats_per_unit[material] / SHEET_MATERIAL_AMOUNT`, `* 0.8` for ore).
    Material(Vec<(String, f64)>),
}

/// A `/datum/export` the game can value.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExportDef {
    pub id: String,
    pub unit_name: String,
    pub message: String,
    /// Per unit; for market materials the starting market price per sheet.
    pub cost: f64,
    /// The initial `k_elasticity`; 0 disables elasticity.
    pub k_elasticity: f64,
    pub k_hit_percentile: f64,
    /// `k_recovery_time` in seconds.
    pub k_recovery_secs: f64,
    pub amount_report_multiplier: f64,
    pub include_subtypes: bool,
    pub export_types: Vec<String>,
    pub exclude_types: Vec<String>,
    pub amount: ExportAmount,
}

/// A `/datum/bounty/item` whose procs are the base ones (`applies_to`, `ship`, ...), with the
/// `New()` of the botany and slime groups (applied by the game).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BountyDef {
    /// The type path.
    pub id: String,
    pub name: String,
    pub description: String,
    /// `reward` in credits.
    pub reward: f64,
    pub required_count: u32,
    /// `wanted_types`: (type, accepted) as the zebra typecache reads them.
    pub wanted: Vec<(String, bool)>,
    pub include_subtypes: bool,
    pub allow_duplicate: bool,
    pub global_exempt: bool,
    pub unique: bool,
    /// Botany: `multiplier`, `bonus_desc`, `foodtype`.
    pub multiplier: f64,
    pub bonus_desc: String,
    pub foodtype: String,
}

/// What cargo needs of a `/datum/job`: the civilian bounty category and the mail goodies.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct JobCargoDef {
    pub title: String,
    /// `bounty_types` (`CIV_JOB_*`).
    pub bounty_types: u32,
    /// `mail_goodies`: (type, weight).
    pub mail_goodies: Vec<(String, u32)>,
    pub exclusive_mail_goodies: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CargoData {
    /// By type path.
    pub packs: Vec<SupplyPackDef>,
    /// In type path order (the first that applies sells an item).
    pub exports: Vec<ExportDef>,
    /// Item bounties the game can run, by type path.
    #[serde(default)]
    pub bounties: Vec<BountyDef>,
    /// By job title.
    #[serde(default)]
    pub jobs: Vec<JobCargoDef>,
    /// `/obj/item/mail` `generic_goodies`: (type, weight).
    #[serde(default)]
    pub mail_generic_goodies: Vec<(String, u32)>,
}

impl CargoData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let text = ss13_data::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        ron::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("cargo data serializes"),
        );
        out.push('\n');
        out
    }

    /// Everything a delivery spawns: crates, their contents and the manifest. The converter
    /// emits prototypes for them.
    pub fn spawned_types(&self) -> BTreeSet<&str> {
        let mut out: BTreeSet<&str> = self
            .packs
            .iter()
            .flat_map(|p| {
                p.contains
                    .iter()
                    .map(|(t, _)| t.as_str())
                    .chain(p.crate_type.as_deref())
            })
            .collect();
        out.insert(MANIFEST_PAPER);
        out.extend(self.mail_generic_goodies.iter().map(|(t, _)| t.as_str()));
        out.extend(
            self.jobs
                .iter()
                .flat_map(|j| j.mail_goodies.iter().map(|(t, _)| t.as_str())),
        );
        out
    }
}

/// `/obj/item/bounty_cube`.
pub const BOUNTY_CUBE: &str = "/obj/item/bounty_cube";
/// `/obj/item/mail` and the envelope (`goodie_count = 2`).
pub const MAIL: &str = "/obj/item/mail";
pub const MAIL_ENVELOPE: &str = "/obj/item/mail/envelope";
/// The supply shuttle's mail crate (`/obj/structure/closet/crate/mail/economy`).
pub const MAIL_CRATE: &str = "/obj/structure/closet/crate/mail/economy";
/// The bounty sheet the civilian console prints.
pub const PAPER: &str = "/obj/item/paper";
/// Junk mail contents (`junk_mail()`): the plain one, then the `prob(25)` specials with their
/// letter names (`%A`: a random adjective).
pub const JUNK_MAIL: [(&str, &str); 5] = [
    ("/obj/item/paper/fluff/junkmail_generic", ""),
    ("/obj/item/paper/pamphlet/gateway", "for %A adventurers"),
    (
        "/obj/item/paper/pamphlet/violent_video_games",
        "for the truth about the arcade centcom doesn't want to hear",
    ),
    (
        "/obj/item/paper/fluff/junkmail_redpill",
        "for those feeling %A working at Nanotrasen",
    ),
    (
        "/obj/effect/decal/cleanable/ash",
        "with INCREDIBLY IMPORTANT ARTIFACT- DELIVER TO SCIENCE DIVISION. HANDLE WITH CARE.",
    ),
];
/// A buyer's goodies, up to `GOODY_FREE_SHIPPING_MAX` orders (`/obj/item/storage/lockbox/order`).
pub const GOODY_CASE: &str = "/obj/item/storage/lockbox/order";
/// A buyer's goodies past the free shipping limit, account-locked.
pub const GOODY_CRATE: &str = "/obj/structure/closet/crate/secure";
/// Fixed types the game spawns for bounties, mail and goodies (the converter keeps the ones the
/// dump has; [`CargoData::spawned_types`] are the data-driven ones).
pub const RUNTIME_TYPES: [&str; 12] = [
    GOODY_CASE,
    GOODY_CRATE,
    BOUNTY_CUBE,
    MAIL,
    MAIL_ENVELOPE,
    MAIL_CRATE,
    PAPER,
    JUNK_MAIL[0].0,
    JUNK_MAIL[1].0,
    JUNK_MAIL[2].0,
    JUNK_MAIL[3].0,
    JUNK_MAIL[4].0,
];

fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn path(v: Option<&Value>) -> Option<String> {
    v?.get("path")?.as_str().map(str::to_owned)
}

fn num(v: Option<&Value>) -> f64 {
    v.and_then(Value::as_f64).unwrap_or(0.0)
}

/// The items of a dumped list: plain entries, or keys of `assoc` entries with their values.
fn list(v: Option<&Value>) -> Vec<(&Value, Option<&Value>)> {
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

fn paths(v: Option<&Value>) -> Vec<String> {
    list(v)
        .into_iter()
        .filter_map(|(k, _)| path(Some(k)))
        .collect()
}

/// A var that is an access string or `FALSE`.
fn access(v: Option<&Value>) -> String {
    text(v)
}

fn is_abstract(types: &DmTypes, p: &str) -> bool {
    path(types.resolved(p, "abstract_type")).as_deref() == Some(p)
}

/// Every pack tg lists and every export the game can value.
pub fn resolve(types: &DmTypes) -> CargoData {
    let mut pack_paths: Vec<&str> = types
        .types
        .keys()
        .map(String::as_str)
        .filter(|p| is_under(p, PACK_ROOT) && *p != PACK_ROOT)
        .collect();
    pack_paths.sort_unstable();
    let packs = pack_paths
        .into_iter()
        .filter_map(|p| pack(types, p))
        .collect();
    let mut export_paths: Vec<&str> = types
        .types
        .keys()
        .map(String::as_str)
        .filter(|p| is_under(p, EXPORT_ROOT) && *p != EXPORT_ROOT && !is_abstract(types, p))
        .collect();
    export_paths.sort_unstable();
    let exports = export_paths
        .into_iter()
        .filter_map(|p| export(types, p))
        .collect();
    let mut bounty_paths: Vec<&str> = types
        .types
        .keys()
        .map(String::as_str)
        .filter(|p| is_under(p, BOUNTY_ITEM) && *p != BOUNTY_ITEM)
        .collect();
    bounty_paths.sort_unstable();
    let bounties = bounty_paths
        .into_iter()
        .filter_map(|p| bounty(types, p))
        .collect();
    let mut job_paths: Vec<&str> = types
        .types
        .keys()
        .map(String::as_str)
        .filter(|p| is_under(p, JOB_ROOT) && *p != JOB_ROOT)
        .collect();
    job_paths.sort_unstable();
    let jobs = job_paths
        .into_iter()
        .filter_map(|p| job(types, p))
        .collect();
    CargoData {
        packs,
        exports,
        bounties,
        jobs,
        mail_generic_goodies: weighted(types, types.resolved(MAIL, "generic_goodies")),
    }
}

const BOUNTY_ITEM: &str = "/datum/bounty/item";
/// Groups whose own `New()` the game applies (`description`, `reward`, `required_count`).
pub const BOUNTY_NEW_ROOTS: [&str; 2] = ["/datum/bounty/item/botany", "/datum/bounty/item/slime"];
const JOB_ROOT: &str = "/datum/job";

/// An assoc list of known types to weights (`|| 1` for a missing weight, as `pick_weight`).
fn weighted(types: &DmTypes, v: Option<&Value>) -> Vec<(String, u32)> {
    list(v)
        .into_iter()
        .filter_map(|(k, w)| {
            let t = path(Some(k)).filter(|t| types.types.contains_key(t))?;
            let w = w.and_then(Value::as_f64).unwrap_or(1.0);
            (w > 0.0).then_some((t, w as u32))
        })
        .collect()
}

/// A bounty with only the base procs; others compute what they want in code.
fn bounty(types: &DmTypes, p: &str) -> Option<BountyDef> {
    let custom = types.chain(p).take_while(|t| *t != BOUNTY_ITEM).any(|t| {
        types.types[t]
            .procs
            .iter()
            .any(|proc| !(proc == "New" && BOUNTY_NEW_ROOTS.contains(&t)))
    });
    if custom {
        return None;
    }
    // `zebra_typecacheof`: the value decides; a plain entry counts as wanted.
    let wanted: Vec<(String, bool)> = list(types.resolved(p, "wanted_types"))
        .into_iter()
        .filter_map(|(k, v)| {
            let t = path(Some(k)).filter(|t| types.types.contains_key(t))?;
            Some((t, v.and_then(Value::as_f64).is_none_or(|n| n != 0.0)))
        })
        .collect();
    if !wanted.iter().any(|(_, w)| *w) {
        return None;
    }
    let flag = |var: &str| num(types.resolved(p, var)) != 0.0;
    let count = num(types.resolved(p, "required_count"));
    Some(BountyDef {
        id: p.to_owned(),
        name: text(types.resolved(p, "name")),
        description: text(types.resolved(p, "description")),
        reward: num(types.resolved(p, "reward")),
        required_count: if count >= 1.0 { count as u32 } else { 1 },
        wanted,
        include_subtypes: types
            .resolved(p, "include_subtypes")
            .and_then(Value::as_f64)
            .is_none_or(|n| n != 0.0),
        allow_duplicate: flag("allow_duplicate"),
        global_exempt: flag("global_exempt"),
        unique: flag("unique"),
        multiplier: num(types.resolved(p, "multiplier")),
        bonus_desc: text(types.resolved(p, "bonus_desc")),
        foodtype: text(types.resolved(p, "foodtype")),
    })
}

fn job(types: &DmTypes, p: &str) -> Option<JobCargoDef> {
    let title = text(types.resolved(p, "title"));
    if title.is_empty() {
        return None;
    }
    Some(JobCargoDef {
        title,
        bounty_types: num(types.resolved(p, "bounty_types")) as u32,
        mail_goodies: weighted(types, types.resolved(p, "mail_goodies")),
        exclusive_mail_goodies: num(types.resolved(p, "exclusive_mail_goodies")) != 0.0,
    })
}

fn pack(types: &DmTypes, p: &str) -> Option<SupplyPackDef> {
    let contains_value = types.resolved(p, "contains")?;
    if contains_value.is_null() {
        return None;
    }
    let contains: Vec<(String, u32)> = list(Some(contains_value))
        .into_iter()
        .filter_map(|(k, amount)| {
            let item = path(Some(k)).filter(|t| types.types.contains_key(t))?;
            let n = amount.and_then(Value::as_f64).unwrap_or(0.0);
            Some((item, if n >= 1.0 { n as u32 } else { 1 }))
        })
        .collect();
    let custom_fill = types.chain(p).take_while(|t| *t != PACK_ROOT).any(|t| {
        types.types[t]
            .procs
            .iter()
            .any(|proc| matches!(proc.as_str(), "fill" | "generate" | "generate_supply_packs"))
    });
    let name = text(types.resolved(p, "name"));
    // `New()`: `if(!crate_name) crate_name = name`.
    let crate_name = match text(types.resolved(p, "crate_name")) {
        n if n.is_empty() => name.clone(),
        n => n,
    };
    let access_any = match types.resolved(p, "access_any") {
        Some(v) if v.get("list").is_some() => list(Some(v))
            .into_iter()
            .filter_map(|(k, _)| k.as_str().map(str::to_owned))
            .collect(),
        v => Some(access(v))
            .filter(|a| !a.is_empty())
            .into_iter()
            .collect(),
    };
    Some(SupplyPackDef {
        id: p.to_owned(),
        name,
        desc: text(types.resolved(p, "desc")),
        group: text(types.resolved(p, "group")),
        cost: num(types.resolved(p, "cost")),
        access: access(types.resolved(p, "access")),
        access_view: access(types.resolved(p, "access_view")),
        access_any,
        contains,
        crate_name,
        crate_type: path(types.resolved(p, "crate_type")).filter(|t| types.types.contains_key(t)),
        order_flags: num(types.resolved(p, "order_flags")) as u32,
        custom_fill,
    })
}

/// The handled root of `p` (`/datum/export`, `.../stack`, `.../material`, `.../material/market`),
/// unless a type on the way overrides a value proc.
fn export_root<'a>(types: &'a DmTypes, p: &'a str) -> Option<&'a str> {
    for t in types.chain(p) {
        if matches!(
            t,
            EXPORT_ROOT | STACK_EXPORT | MATERIAL_EXPORT | MARKET_EXPORT
        ) {
            return Some(t);
        }
        if types.types[t]
            .procs
            .iter()
            .any(|proc| VALUE_PROCS.contains(&proc.as_str()))
        {
            return None;
        }
    }
    None
}

fn export(types: &DmTypes, p: &str) -> Option<ExportDef> {
    let root = export_root(types, p)?;
    if text(types.resolved(p, "sales_market")) != MARKET_STATION {
        return None;
    }
    let include_subtypes = num(types.resolved(p, "include_subtypes")) != 0.0;
    // A single path is a list of one (`export_types = /obj/item/stack/sheet/hot_ice`).
    let mut export_types = paths(types.resolved(p, "export_types"));
    if export_types.is_empty()
        && let Some(single) = path(types.resolved(p, "export_types"))
    {
        export_types.push(single);
    }
    let exclude_types = paths(types.resolved(p, "exclude_types"));
    let mut cost = num(types.resolved(p, "cost"));
    let amount = match root {
        STACK_EXPORT => ExportAmount::Stack,
        MATERIAL_EXPORT | MARKET_EXPORT => {
            let material = path(types.resolved(p, "material_id"))?;
            if root == MARKET_EXPORT {
                // `SSstock_market.materials_prices` start at the material's value per sheet.
                let price =
                    num(types.resolved(&material, "value_per_unit")) * SHEET_MATERIAL_AMOUNT;
                cost *= price;
            }
            ExportAmount::Material(material_units(
                types,
                &export_types,
                include_subtypes,
                &material,
            ))
        }
        _ => ExportAmount::One,
    };
    Some(ExportDef {
        id: p.to_owned(),
        unit_name: text(types.resolved(p, "unit_name")),
        message: text(types.resolved(p, "message")),
        cost,
        k_elasticity: num(types.resolved(p, "k_elasticity")),
        k_hit_percentile: num(types.resolved(p, "k_hit_percentile")),
        k_recovery_secs: num(types.resolved(p, "k_recovery_time")) / 10.0,
        amount_report_multiplier: num(types.resolved(p, "amount_report_multiplier")),
        include_subtypes,
        export_types,
        exclude_types,
        amount,
    })
}

/// Stack types under `roots` whose `mats_per_unit` holds `material`, with the sheets of it per
/// stack unit.
// shortcut: stacks only; non-stack items with `custom_materials` (coins, stock blocks) are not
// valued by material.
fn material_units(
    types: &DmTypes,
    roots: &[String],
    include_subtypes: bool,
    material: &str,
) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = types
        .types
        .keys()
        .filter(|t| {
            is_under(t, "/obj/item/stack")
                && roots.iter().any(|r| {
                    if include_subtypes {
                        is_under(t, r)
                    } else {
                        *t == r
                    }
                })
        })
        .filter_map(|t| {
            let mats = list(types.resolved(t, "mats_per_unit"));
            let per_unit = mats
                .iter()
                .find(|(k, _)| path(Some(k)).as_deref() == Some(material))
                .and_then(|(_, v)| v.and_then(Value::as_f64))?;
            let ore = if is_under(t, "/obj/item/stack/ore") {
                0.8
            } else {
                1.0
            };
            let units = per_unit * ore / SHEET_MATERIAL_AMOUNT;
            (units > 0.0).then(|| (t.clone(), units))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/datum/supply_pack": { "parent": "/datum", "declared_vars": { "name": "Crate",
            "group": "", "cost": 280, "access": 0, "access_view": 0, "access_any": 0,
            "contains": null, "crate_name": "crate", "desc": "",
            "crate_type": { "path": "/obj/structure/closet/crate" }, "order_flags": 0 },
            "procs": ["fill", "generate_supply_packs"] },
        "/datum/supply_pack/engineering": { "parent": "/datum/supply_pack",
            "declared_vars": { "group": "Engineering",
                "crate_type": { "path": "/obj/structure/closet/crate/engineering" } } },
        "/datum/supply_pack/engineering/tools": { "parent": "/datum/supply_pack/engineering",
            "declared_vars": { "name": "Toolbox Crate", "cost": 1000, "access": "engineering",
                "crate_name": null,
                "contains": { "list": [ { "path": "/obj/item/wrench" },
                    { "assoc": [ { "path": "/obj/item/stack/sheet/iron" }, 3 ] },
                    { "path": "/obj/item/missing" } ] } } },
        "/datum/supply_pack/engineering/random": { "parent": "/datum/supply_pack/engineering",
            "declared_vars": { "name": "Random", "order_flags": 8,
                "contains": { "list": [ { "path": "/obj/item/wrench" } ] } },
            "procs": ["fill"] },
        "/datum/export": { "parent": "/datum", "declared_vars": {
            "abstract_type": { "path": "/datum/export" }, "cost": 1, "k_elasticity": 1,
            "k_hit_percentile": 0.05, "k_recovery_time": 6000, "amount_report_multiplier": 1,
            "include_subtypes": 1, "export_types": { "list": [] },
            "exclude_types": { "list": [] }, "sales_market": "supply", "unit_name": "",
            "message": "" },
            "procs": ["get_base_cost", "get_amount", "applies_to", "sell_object"] },
        "/datum/export/crate": { "parent": "/datum/export", "declared_vars": { "cost": 200,
            "k_elasticity": 0, "unit_name": "crate",
            "export_types": { "list": [ { "path": "/obj/structure/closet/crate" } ] },
            "exclude_types": { "list": [ { "path": "/obj/structure/closet/crate/large" } ] } },
            "procs": ["total_printout"] },
        "/datum/export/stack": { "parent": "/datum/export", "declared_vars": {
            "abstract_type": { "path": "/datum/export/stack" }, "unit_name": "sheet" },
            "procs": ["get_amount"] },
        "/datum/export/stack/plasteel": { "parent": "/datum/export/stack", "declared_vars": {
            "cost": 82, "export_types": { "list": [ { "path": "/obj/item/stack/sheet/plasteel" } ] } } },
        "/datum/export/keg": { "parent": "/datum/export", "procs": ["get_base_cost"] },
        "/datum/export/material": { "parent": "/datum/export", "declared_vars": {
            "abstract_type": { "path": "/datum/export/material" }, "cost": 5,
            "amount_report_multiplier": 100, "material_id": null,
            "export_types": { "list": [ { "path": "/obj/item/stack/sheet/mineral" },
                { "path": "/obj/item/stack/ore" } ] } },
            "procs": ["get_amount"] },
        "/datum/export/material/plasma": { "parent": "/datum/export/material",
            "declared_vars": { "cost": 80, "k_elasticity": 0,
                "material_id": { "path": "/datum/material/plasma" } } },
        "/datum/export/material/market": { "parent": "/datum/export/material",
            "declared_vars": { "abstract_type": { "path": "/datum/export/material/market" },
                "cost": 1 }, "procs": ["get_base_cost", "sell_object"] },
        "/datum/export/material/market/iron": { "parent": "/datum/export/material/market",
            "declared_vars": { "material_id": { "path": "/datum/material/iron" },
                "export_types": { "list": [ { "path": "/obj/item/stack/sheet/iron" } ] } } },
        "/datum/export/pirate": { "parent": "/datum/export", "declared_vars": {
            "sales_market": "piracy", "cost": 5 } },
        "/datum/material/iron": { "parent": "/datum", "declared_vars": { "value_per_unit": 0.05 } },
        "/datum/material/plasma": { "parent": "/datum", "declared_vars": { "value_per_unit": 2 } },
        "/obj/item/wrench": { "parent": "/datum" },
        "/obj/item/stack": { "parent": "/datum", "declared_vars": { "mats_per_unit": null } },
        "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack", "declared_vars": {
            "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 100 ] } ] } } },
        "/obj/item/stack/sheet/mineral/plasma": { "parent": "/obj/item/stack", "declared_vars": {
            "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/plasma" }, 100 ] } ] } } },
        "/obj/item/stack/ore/plasma": { "parent": "/obj/item/stack", "declared_vars": {
            "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/plasma" }, 100 ] } ] } } },
        "/obj/item/stack/sheet/plasteel": { "parent": "/obj/item/stack" },
        "/obj/structure/closet/crate": { "parent": "/datum" },
        "/obj/structure/closet/crate/engineering": { "parent": "/obj/structure/closet/crate" }
    } }"##;

    #[test]
    fn packs_and_exports_from_the_dump() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let data = resolve(&types);
        assert_eq!(data.packs.len(), 2, "the group root has no contents");
        let tools = &data.packs[1];
        assert_eq!(tools.id, "/datum/supply_pack/engineering/tools");
        assert_eq!(tools.group, "Engineering");
        assert_eq!(tools.cost, 1000.0);
        assert_eq!(tools.access, "engineering");
        assert_eq!(
            tools.crate_name, "Toolbox Crate",
            "null crate_name takes the name"
        );
        assert_eq!(
            tools.contains,
            vec![
                ("/obj/item/wrench".into(), 1),
                ("/obj/item/stack/sheet/iron".into(), 3)
            ],
            "unknown types are dropped"
        );
        assert_eq!(
            tools.crate_type.as_deref(),
            Some("/obj/structure/closet/crate/engineering")
        );
        assert!(!tools.custom_fill, "the root's fill() is the static one");
        let random = &data.packs[0];
        assert!(random.custom_fill && random.has(order_flags::GOODY));

        let ids: Vec<&str> = data.exports.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "/datum/export/crate",
                "/datum/export/material/market/iron",
                "/datum/export/material/plasma",
                "/datum/export/stack/plasteel"
            ],
            "abstract, own-value and piracy exports are left out"
        );
        let crate_export = &data.exports[0];
        assert_eq!(crate_export.cost, 200.0);
        assert_eq!(crate_export.k_elasticity, 0.0);
        assert_eq!(crate_export.k_recovery_secs, 600.0);
        assert_eq!(crate_export.amount, ExportAmount::One);
        assert_eq!(data.exports[3].amount, ExportAmount::Stack);
        let iron = &data.exports[1];
        assert!((iron.cost - 5.0).abs() < 1e-9, "market price 0.05 * 100");
        assert_eq!(
            iron.amount,
            ExportAmount::Material(vec![("/obj/item/stack/sheet/iron".into(), 1.0)])
        );
        let plasma = &data.exports[2];
        assert_eq!(
            plasma.amount,
            ExportAmount::Material(vec![
                ("/obj/item/stack/ore/plasma".into(), 0.8),
                ("/obj/item/stack/sheet/mineral/plasma".into(), 1.0)
            ])
        );
        let spawned = data.spawned_types();
        assert!(spawned.contains("/obj/structure/closet/crate/engineering"));
        assert!(spawned.contains(MANIFEST_PAPER));
    }

    const BOUNTY_TYPES: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/datum/bounty": { "parent": "/datum", "declared_vars": { "name": null,
            "description": null, "reward": 1000, "allow_duplicate": 0, "global_exempt": 0,
            "unique": 0 }, "procs": ["can_claim", "applies_to", "ship"] },
        "/datum/bounty/item": { "parent": "/datum/bounty", "declared_vars": {
            "required_count": 1, "wanted_types": null, "include_subtypes": 1 },
            "procs": ["New", "applies_to", "ship"] },
        "/datum/bounty/item/assistant": { "parent": "/datum/bounty/item" },
        "/datum/bounty/item/assistant/soap": { "parent": "/datum/bounty/item/assistant",
            "declared_vars": { "name": "Soap", "description": "Send soap.", "reward": 800,
                "required_count": 3, "wanted_types": { "list": [
                    { "assoc": [ { "path": "/obj/item/soap" }, 1 ] },
                    { "assoc": [ { "path": "/obj/item/soap/fake" }, 0 ] } ] } } },
        "/datum/bounty/item/assistant/toolbox": { "parent": "/datum/bounty/item/assistant",
            "declared_vars": { "name": "Toolbox", "wanted_types": { "list": [
                { "assoc": [ { "path": "/obj/item/soap" }, 1 ] } ] } },
            "procs": ["applies_to"] },
        "/datum/bounty/item/botany": { "parent": "/datum/bounty/item", "declared_vars": {
            "multiplier": 0, "bonus_desc": null, "foodtype": "meal" }, "procs": ["New"] },
        "/datum/bounty/item/botany/apple": { "parent": "/datum/bounty/item/botany",
            "declared_vars": { "name": "Apples", "multiplier": 2, "include_subtypes": 0,
                "wanted_types": { "list": [ { "assoc": [ { "path": "/obj/item/soap" }, 1 ] } ] } } },
        "/datum/job": { "parent": "/datum", "declared_vars": { "title": "NOPE",
            "bounty_types": 1, "mail_goodies": { "list": [] }, "exclusive_mail_goodies": 0 } },
        "/datum/job/cook": { "parent": "/datum/job", "declared_vars": { "title": "Cook",
            "bounty_types": 3, "mail_goodies": { "list": [
                { "assoc": [ { "path": "/obj/item/soap" }, 15 ] },
                { "assoc": [ { "path": "/obj/item/missing" }, 5 ] } ] } } },
        "/obj/item/soap": { "parent": "/datum" },
        "/obj/item/soap/fake": { "parent": "/obj/item/soap" },
        "/obj/item/mail": { "parent": "/datum", "declared_vars": { "generic_goodies": {
            "list": [ { "assoc": [ { "path": "/obj/item/soap" }, 25 ] } ] } } }
    } }"##;

    #[test]
    fn bounties_and_job_mail_from_the_dump() {
        let types = DmTypes::parse(BOUNTY_TYPES.as_bytes()).unwrap();
        let data = resolve(&types);
        let ids: Vec<&str> = data.bounties.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "/datum/bounty/item/assistant/soap",
                "/datum/bounty/item/botany/apple"
            ],
            "own procs and empty wanted lists are left out; botany's New() is the game's"
        );
        let soap = &data.bounties[0];
        assert_eq!(soap.reward, 800.0);
        assert_eq!(soap.required_count, 3);
        assert!(soap.include_subtypes);
        assert_eq!(
            soap.wanted,
            vec![
                ("/obj/item/soap".into(), true),
                ("/obj/item/soap/fake".into(), false)
            ]
        );
        let apple = &data.bounties[1];
        assert!(!apple.include_subtypes);
        assert_eq!((apple.multiplier, apple.foodtype.as_str()), (2.0, "meal"));
        assert_eq!(data.jobs.len(), 1, "the root is not a job");
        let cook = data.jobs.iter().find(|j| j.title == "Cook").unwrap();
        assert_eq!(cook.bounty_types, 3);
        assert_eq!(cook.mail_goodies, vec![("/obj/item/soap".into(), 15)]);
        assert_eq!(
            data.mail_generic_goodies,
            vec![("/obj/item/soap".into(), 25)]
        );
        assert!(data.spawned_types().contains("/obj/item/soap"));
    }

    #[test]
    fn real_dump_when_present() {
        let path = std::env::var_os("SS13_ASSETS")
            .map(|a| std::path::PathBuf::from(a).join("dm_types.json"))
            .filter(|p| p.exists());
        let Some(path) = path else {
            return;
        };
        let data = resolve(&DmTypes::read(&path).unwrap());
        assert!(data.packs.len() > 300, "{} packs", data.packs.len());
        assert!(data.exports.len() > 100, "{} exports", data.exports.len());
        let crate_export = data
            .exports
            .iter()
            .find(|e| e.id == "/datum/export/crate")
            .unwrap();
        assert_eq!(crate_export.cost, 200.0);
    }
}
