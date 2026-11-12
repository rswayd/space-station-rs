//! `dm_types.json` (from `ss13_dm_extract`) -> `ss13_proto` RON: one prototype per needed DM
//! type, id = DM path, parent = DM parent, carrying only the vars the type itself declares, so the
//! prototype merge rule reproduces DM inheritance.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context as _, bail};
use serde::Deserialize;

use crate::door;
use crate::value::{Field, SUPPORTED_VARS, Skip, Val, field, patch_ron_with};

/// `dm_types.json` layout version this crate reads.
pub const DM_TYPES_SCHEMA: u32 = 3;

/// Root of every prototype the converter emits; DM types above it are not entities.
pub const ROOT: &str = "/atom";

#[derive(Deserialize)]
pub struct DmTypes {
    pub schema: u32,
    pub types: HashMap<String, DmType>,
    /// Element path -> `AddElement`/`RemoveElement` calls (see `ss13_dm_extract`).
    #[serde(default)]
    pub element_calls: HashMap<String, Vec<ElementCall>>,
    /// Contents programs per type (see [`crate::contents`]); absent in older dumps.
    #[serde(default)]
    pub populate: crate::contents::RawPopulate,
    /// `SSid_access.accesses_by_flag`: flag (decimal) -> accesses. Absent in older dumps.
    #[serde(default)]
    pub access_flags: HashMap<String, Vec<String>>,
    /// `GLOBAL_LIST_INIT` values (`ss13_dm_extract::DUMPED_GLOBALS`). Absent in older dumps.
    #[serde(default)]
    pub globals: HashMap<String, serde_json::Value>,
    /// `init_factors()` lists by reaction type (`ss13_dm_extract`). Absent in older dumps.
    #[serde(default)]
    pub reaction_factors: BTreeMap<String, serde_json::Value>,
    /// `set_holdable` lists by `/datum/storage` type (`ss13_dm_extract::holdable`). Absent in
    /// older dumps.
    #[serde(default)]
    pub holdable: HashMap<String, RawHoldLists>,
}

/// One storage type's own `set_holdable` call; a `None` list was not passed.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RawHoldLists {
    #[serde(default)]
    pub can_hold: Option<Vec<String>>,
    #[serde(default)]
    pub cant_hold: Option<Vec<String>>,
    #[serde(default)]
    pub exception_hold: Option<Vec<String>>,
    #[serde(default)]
    pub unsupported: bool,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct ElementCall {
    /// Type that owns the proc making the call.
    #[serde(rename = "type")]
    pub owner: String,
    pub proc: String,
    /// `AddElement` or `RemoveElement`.
    pub call: String,
    pub line: String,
}

#[derive(Deserialize)]
pub struct DmType {
    pub parent: Option<String>,
    /// Resolved (inherited) values of the dumped vars.
    #[serde(default)]
    pub vars: BTreeMap<String, serde_json::Value>,
    /// What this type itself assigns.
    #[serde(default)]
    pub declared_vars: BTreeMap<String, serde_json::Value>,
    /// Procs the type defines or overrides itself.
    #[serde(default)]
    pub procs: Vec<String>,
    /// Access helpers: what the type's own `get_access()` appends.
    #[serde(default)]
    pub get_access: Vec<String>,
    /// ID trims: access flag groups the type's own `New()` adds, per list var.
    #[serde(default)]
    pub new_access_flags: BTreeMap<String, Vec<i64>>,
}

impl DmTypes {
    /// The value of `var` as `path` sees it: its own declaration or the nearest ancestor's.
    pub fn resolved(&self, path: &str, var: &str) -> Option<&serde_json::Value> {
        let mut current = Some(path);
        while let Some(p) = current {
            let ty = self.types.get(p)?;
            if let Some(v) = ty.declared_vars.get(var) {
                return Some(v);
            }
            current = ty.parent.as_deref();
        }
        None
    }

    /// `path` itself and its ancestors, nearest first.
    pub fn chain<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        std::iter::successors(Some(path), |p| {
            self.types.get(*p).and_then(|t| t.parent.as_deref())
        })
        .filter(|p| self.types.contains_key(*p))
    }

    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&bytes).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Header {
            schema: u32,
        }
        // Check the version before the full parse, so an old file gives a clear error.
        let header: Header = serde_json::from_slice(bytes)?;
        if header.schema != DM_TYPES_SCHEMA {
            bail!(
                "dm_types.json schema {} (expected {DM_TYPES_SCHEMA}); rerun ss13-dm-extract",
                header.schema
            );
        }
        Ok(serde_json::from_slice(bytes)?)
    }
}

pub struct Conversion {
    /// A RON prototype file.
    pub ron: String,
    pub prototypes: usize,
    /// Supported vars whose declared value could not be converted (the type inherits instead),
    /// by var name.
    pub unsupported: BTreeMap<String, usize>,
    /// Every converted type path.
    pub types: BTreeSet<String>,
}

/// BYOND's default `name` when no type in the chain sets one: the last path element with `_`
/// as spaces.
pub fn default_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).replace('_', " ")
}

/// Converts `needed` types and all their ancestors up to [`ROOT`].
pub fn convert<'a>(
    types: &DmTypes,
    needed: impl IntoIterator<Item = &'a str>,
) -> anyhow::Result<Conversion> {
    crate::undertile::check_rules(types)?;
    let needed: Vec<&str> = needed.into_iter().collect();
    let ctx = crate::fidelity::ConvertContext::new(types);
    let mut loot = crate::spawner::loot_closure(&ctx, &needed);
    // First: what the consoles sell goes through the closures below.
    let mining = crate::mining::mining_closure(
        types,
        needed
            .iter()
            .copied()
            .chain(loot.iter().map(String::as_str)),
    );
    loot.extend(mining);
    let products = crate::vending::product_closure(
        types,
        needed
            .iter()
            .copied()
            .chain(loot.iter().map(String::as_str)),
    );
    loot.extend(products);
    let fridges = crate::smartfridge::fridge_closure(
        types,
        needed
            .iter()
            .copied()
            .chain(loot.iter().map(String::as_str)),
    );
    loot.extend(fridges);
    let contents = crate::contents::contents_closure(
        types,
        needed
            .iter()
            .copied()
            .chain(loot.iter().map(String::as_str)),
    );
    loot.extend(contents);
    let plants = crate::plants::plant_closure(
        types,
        needed
            .iter()
            .copied()
            .chain(loot.iter().map(String::as_str)),
    );
    loot.extend(plants);
    let food = crate::food::food_closure(
        types,
        needed
            .iter()
            .copied()
            .chain(loot.iter().map(String::as_str)),
    );
    loot.extend(food);
    let guns = crate::guns::gun_closure(
        types,
        needed
            .iter()
            .copied()
            .chain(loot.iter().map(String::as_str)),
    );
    loot.extend(guns);
    let machines = crate::circuit::machine_closure(
        types,
        needed
            .iter()
            .copied()
            .chain(loot.iter().map(String::as_str)),
    );
    loot.extend(machines);
    loot.extend(crate::fishing::fish_closure(types));
    let mut closure = BTreeSet::new();
    let mut bad = Vec::new();
    for path in needed
        .iter()
        .copied()
        .chain(loot.iter().map(String::as_str))
    {
        let mut current = path;
        loop {
            if !closure.insert(current) {
                break;
            }
            let Some(ty) = types.types.get(current) else {
                bad.push(format!("{current} is not a DM type"));
                break;
            };
            if current == ROOT {
                break;
            }
            match ty.parent.as_deref() {
                Some(parent) => current = parent,
                None => {
                    bad.push(format!("{path} is not under {ROOT}"));
                    break;
                }
            }
        }
    }
    if !bad.is_empty() {
        bad.sort();
        bad.dedup();
        bail!("cannot convert:\n  {}", bad.join("\n  "));
    }

    let mut ron = String::from(
        "// Generated by `ss13-mapload convert` from dm_types.json. Do not edit.\n[\n",
    );
    let mut unsupported = BTreeMap::new();
    for &path in &closure {
        let ty = &types.types[path];
        let source = if path == ROOT {
            &ty.vars
        } else {
            &ty.declared_vars
        };
        let mut fields = Vec::new();
        for (var, value) in source {
            if !SUPPORTED_VARS.contains(&var.as_str())
                && !crate::light::LIGHT_VARS.contains(&var.as_str())
            {
                continue;
            }
            match field(var, Val::from_json(value)) {
                Ok(f) => fields.push(f),
                Err(Skip::Unsupported | Skip::UnknownVar) => {
                    *unsupported.entry(var.clone()).or_default() += 1;
                }
            }
        }
        fields.extend(ctx.type_fields(path));
        if ty.vars.get("name").is_none_or(serde_json::Value::is_null) {
            fields.push(Field::Name(default_name(path)));
        }
        let mut extra = Vec::new();
        if door::is_door(path) {
            let mut door_fields: Vec<_> = door::own_kind(path)
                .map(door::DoorField::Kind)
                .into_iter()
                .collect();
            for (var, value) in &ty.declared_vars {
                match door::field_json(var, value) {
                    Some(Ok(f)) => door_fields.push(f),
                    Some(Err(_)) => *unsupported.entry(var.clone()).or_default() += 1,
                    None => {}
                }
            }
            extra.extend(door::patch_entry(&door_fields));
        }
        if let Some(helper) = door::helper(types, path) {
            extra.push(door::helper_entry(&helper));
        }
        ron.push_str("    (id: ");
        ron.push_str(&ron::to_string(path)?);
        if path != ROOT {
            let parent = ty.parent.as_deref().expect("checked in the closure walk");
            ron.push_str(", parent: ");
            ron.push_str(&ron::to_string(parent)?);
        }
        ron.push_str(", components: ");
        ron.push_str(&patch_ron_with(&fields, &extra));
        ron.push_str("),\n");
    }
    ron.push_str("]\n");
    Ok(Conversion {
        ron,
        prototypes: closure.len(),
        unsupported,
        types: closure.iter().map(|p| (*p).to_owned()).collect(),
    })
}
