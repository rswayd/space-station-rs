//! Containers as prototype data: tg closets/crates (`/obj/structure/closet`, `closets.dm`,
//! `crates.dm`), storage items (`/obj/item/storage` with its `/datum/storage` limits) and what
//! both spawn into themselves (`PopulateContents()`, `populate_contents_immediate()`), lowered
//! by `ss13_dm_extract` into `dm_types.json` `populate` and resolved per type here.
//!
//! - [`DmCloset`]: the closet vars `update_icon`, `open`, `close` and `togglelock` read.
//! - [`DmLockbox`]: the access and lock sprites of `/obj/item/storage/lockbox` types.
//! - [`DmContents`]: the contents program of a type, `..()` inlined and `if(var)` decided by the
//!   type's own vars, so every type carries its complete program.
//! - [`DmStorage`]: `max_slots`, `max_specific_storage`, `max_total_storage` of the item's
//!   `storage_type` datum.
//! - [`DmWeight`]: `w_class` of every item.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use ss13_assets::dmm::DmValue;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::{Field, Skip, Val};

pub const CLOSET_ROOT: &str = "/obj/structure/closet";
pub const CRATE_ROOT: &str = "/obj/structure/closet/crate";
pub const STORAGE_ROOT: &str = "/obj/item/storage";
pub const LOCKBOX_ROOT: &str = "/obj/item/storage/lockbox";
const ITEM_ROOT: &str = crate::item::ITEM_ROOT;
/// `WEIGHT_CLASS_NORMAL`, `/obj/item::w_class`.
pub const WEIGHT_CLASS_NORMAL: u8 = 3;
/// Programs nested deeper than this (`..()` chains, repeats) are cut: a cycle in the data.
const MAX_DEPTH: u32 = 16;

/// One step of a contents program (see `ss13_dm_extract::populate`), as the game runs it.
#[derive(Reflect, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[reflect(no_field_bounds)]
pub enum Fill {
    New(String),
    Repeat {
        count: u32,
        body: Vec<Fill>,
    },
    Prob {
        percent: f32,
        then: Vec<Fill>,
        otherwise: Vec<Fill>,
    },
    Pick {
        arms: Vec<PickArm>,
    },
    /// A parent proc's body (`..()`): a `Return` in it ends only this scope.
    Scope(Vec<Fill>),
    Return,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[reflect(no_field_bounds)]
pub struct PickArm {
    pub weight: f32,
    pub body: Vec<Fill>,
}

/// The extractor's form, before `..()` and `if(var)` are resolved.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RawFill {
    New(String),
    NewVar(String),
    Repeat {
        count: u32,
        body: Vec<RawFill>,
    },
    RepeatVar {
        var: String,
        body: Vec<RawFill>,
    },
    Prob {
        percent: f32,
        then: Vec<RawFill>,
        otherwise: Vec<RawFill>,
    },
    Pick {
        arms: Vec<(f32, Vec<RawFill>)>,
    },
    IfVar {
        var: String,
        then: Vec<RawFill>,
        otherwise: Vec<RawFill>,
    },
    Parent,
    Return,
    Unsupported(String),
}

/// `PopulateContents` / `populate_contents_immediate` per type that defines them.
pub type RawPopulate = HashMap<String, HashMap<String, Vec<RawFill>>>;

pub const POPULATE: &str = "PopulateContents";
pub const POPULATE_IMMEDIATE: &str = "populate_contents_immediate";

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmContents {
    /// `PopulateContents()`: closets run it on their first opening, storage items at spawn.
    pub populate: Vec<Fill>,
    /// `populate_contents_immediate()`: at spawn.
    pub immediate: Vec<Fill>,
    /// Statements of either program that were not understood (their spawns are missing).
    pub unsupported: u32,
}

/// The closet vars of a `/obj/structure/closet` type (type defaults + map var edits); empty
/// strings are DM `null`. Prototype data: the game builds its live closet from it once.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmCloset {
    /// `/obj/structure/closet/crate` and subtypes: their own `update_icon_state` and overlays.
    pub is_crate: bool,
    pub opened: bool,
    pub locked: bool,
    pub welded: bool,
    pub secure: bool,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    pub base_icon_state: String,
    pub icon_door: String,
    pub icon_welded: String,
    pub enable_door_overlay: bool,
    pub has_opened_overlay: bool,
    pub has_closed_overlay: bool,
    pub dense_when_open: bool,
    pub wall_mounted: bool,
    pub horizontal: bool,
    pub storage_capacity: u32,
    /// `secure_closet/brig::id`: the door timer that locks it.
    pub id: String,
    /// `card_reader_installed`: personal closets (an ID swipe makes it the card's).
    pub card_reader_installed: bool,
}

/// The lock of a `/obj/item/storage/lockbox` type (`req_access`, the `icon_*` states);
/// empty strings are DM `null`.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmLockbox {
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    pub icon_locked: String,
    pub icon_closed: String,
    pub icon_broken: String,
}

const LOCKBOX_VARS: [&str; 5] = [
    "req_access",
    "req_one_access",
    "icon_locked",
    "icon_closed",
    "icon_broken",
];

/// `atom_storage` limits of a storage item (`/datum/storage` vars of its `storage_type`).
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
pub struct DmStorage {
    pub max_slots: u32,
    /// Largest `w_class` that fits.
    pub max_specific_storage: u8,
    /// Sum of `w_class` of the contents.
    pub max_total_storage: u32,
}

/// What a storage item's `storage_type` may hold (`set_holdable` in the datum's `New`): type
/// paths, subtypes included. Only on storages with at least one list.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
pub struct DmHoldable {
    /// Empty: anything.
    pub can_hold: Vec<String>,
    pub cant_hold: Vec<String>,
    /// These fit whatever their size, `exception_max` of them at once.
    pub exception_hold: Vec<String>,
    pub exception_max: u32,
}

/// `path`'s `storage_type` lists: the nearest `/datum/storage` ancestor with its own
/// `set_holdable` call.
pub fn holdable(types: &DmTypes, path: &str) -> Option<DmHoldable> {
    let datum = match types.resolved(path, "storage_type")? {
        serde_json::Value::Object(o) => o.get("path")?.as_str()?,
        _ => return None,
    };
    let raw = types.chain(datum).find_map(|t| types.holdable.get(t))?;
    let known = |list: &Option<Vec<String>>| -> Vec<String> {
        list.iter()
            .flatten()
            .filter(|p| types.types.contains_key(*p))
            .cloned()
            .collect()
    };
    let out = DmHoldable {
        can_hold: known(&raw.can_hold),
        cant_hold: known(&raw.cant_hold),
        exception_hold: known(&raw.exception_hold),
        exception_max: types
            .resolved(datum, "exception_max")
            .and_then(|v| num(Some(v)))
            .map_or(1, |n| n.max(0.0) as u32),
    };
    (!(out.can_hold.is_empty() && out.cant_hold.is_empty() && out.exception_hold.is_empty()))
        .then_some(out)
}

pub const FANCY_ROOT: &str = "/obj/item/storage/fancy";

/// `/obj/item/storage/fancy` vars: boxes whose sprite shows how many items are left.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
pub struct DmFancy {
    pub base_icon_state: String,
    /// `FANCY_CONTAINER_CLOSED` 0, `_OPEN` 1, `_ALWAYS_OPEN` 2.
    pub open_status: u8,
    pub has_open_closed_states: bool,
    pub contents_tag: String,
    /// Empty: it does not fold.
    pub foldable_result: String,
}

fn fancy(types: &DmTypes, path: &str) -> DmFancy {
    let text = |var: &str| match types.resolved(path, var) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Object(o)) => o
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    };
    DmFancy {
        base_icon_state: text("base_icon_state"),
        open_status: types
            .resolved(path, "open_status")
            .and_then(|v| num(Some(v)))
            .map_or(0, |n| n.clamp(0.0, 2.0) as u8),
        has_open_closed_states: truthy(types.resolved(path, "has_open_closed_states")),
        contents_tag: text("contents_tag"),
        foldable_result: text("foldable_result"),
    }
}

impl Default for DmStorage {
    /// `/datum/storage` defaults.
    fn default() -> Self {
        Self {
            max_slots: 7,
            max_specific_storage: WEIGHT_CLASS_NORMAL,
            max_total_storage: 14,
        }
    }
}

/// `/obj/item::w_class`.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
pub struct DmWeight {
    pub w_class: u8,
}

impl Default for DmWeight {
    fn default() -> Self {
        Self {
            w_class: WEIGHT_CLASS_NORMAL,
        }
    }
}

/// Closet vars and their [`DmCloset`] field names.
const CLOSET_VARS: [(&str, &str); 17] = [
    ("opened", "opened"),
    ("locked", "locked"),
    ("welded", "welded"),
    ("secure", "secure"),
    ("req_access", "req_access"),
    ("req_one_access", "req_one_access"),
    ("base_icon_state", "base_icon_state"),
    ("icon_door", "icon_door"),
    ("icon_welded", "icon_welded"),
    ("enable_door_overlay", "enable_door_overlay"),
    ("has_opened_overlay", "has_opened_overlay"),
    ("has_closed_overlay", "has_closed_overlay"),
    ("dense_when_open", "dense_when_open"),
    ("wall_mounted", "wall_mounted"),
    ("horizontal", "horizontal"),
    ("id", "id"),
    ("card_reader_installed", "card_reader_installed"),
];

const STORAGE_VARS: [&str; 3] = ["max_slots", "max_specific_storage", "max_total_storage"];

fn is_text_var(var: &str) -> bool {
    matches!(
        var,
        "base_icon_state"
            | "icon_door"
            | "icon_welded"
            | "id"
            | "icon_locked"
            | "icon_closed"
            | "icon_broken"
    )
}

fn is_list_var(var: &str) -> bool {
    matches!(var, "req_access" | "req_one_access")
}

/// Converter fields of `path` for the container components.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(ty) = types.types.get(path) else {
        return out;
    };
    // The roots also carry what they inherit, so the prototype defaults are DM's.
    let value = |root: &str, var: &str| {
        if path == root {
            types.resolved(path, var)
        } else {
            ty.declared_vars.get(var)
        }
    };
    if is_under(path, CLOSET_ROOT) {
        if path == CLOSET_ROOT || path == CRATE_ROOT {
            out.push(ext(
                "dm_closet",
                "is_crate",
                (path == CRATE_ROOT).to_string(),
            ));
        }
        for (var, key) in CLOSET_VARS {
            if let Some(Some(ron)) = value(CLOSET_ROOT, var).map(|v| json_ron(var, v)) {
                out.push(ext("dm_closet", key, ron));
            }
        }
        if let Some(n) = value(CLOSET_ROOT, "storage_capacity").and_then(|v| num(Some(v))) {
            out.push(ext(
                "dm_closet",
                "storage_capacity",
                (n.max(0.0) as u32).to_string(),
            ));
        }
    }
    if is_under(path, LOCKBOX_ROOT) {
        for var in LOCKBOX_VARS {
            if let Some(Some(ron)) = value(LOCKBOX_ROOT, var).map(|v| json_ron(var, v)) {
                out.push(ext("dm_lockbox", var, ron));
            }
        }
    }
    if is_under(path, ITEM_ROOT)
        && let Some(n) = value(ITEM_ROOT, "w_class").and_then(|v| num(Some(v)))
    {
        out.push(ext(
            "dm_weight",
            "w_class",
            (n.clamp(0.0, 255.0) as u8).to_string(),
        ));
    }
    if is_under(path, STORAGE_ROOT)
        && (path == STORAGE_ROOT || ty.declared_vars.contains_key("storage_type"))
        && let Some(storage) = storage_limits(types, path)
    {
        out.push(ext(
            "dm_storage",
            "max_slots",
            storage.max_slots.to_string(),
        ));
        out.push(ext(
            "dm_storage",
            "max_specific_storage",
            storage.max_specific_storage.to_string(),
        ));
        out.push(ext(
            "dm_storage",
            "max_total_storage",
            storage.max_total_storage.to_string(),
        ));
    }
    if is_under(path, FANCY_ROOT) {
        let f = fancy(types, path);
        let s = |v: &str| ron::to_string(v).expect("strings always serialize");
        out.push(ext("dm_fancy", "base_icon_state", s(&f.base_icon_state)));
        out.push(ext("dm_fancy", "open_status", f.open_status.to_string()));
        out.push(ext(
            "dm_fancy",
            "has_open_closed_states",
            f.has_open_closed_states.to_string(),
        ));
        out.push(ext("dm_fancy", "contents_tag", s(&f.contents_tag)));
        out.push(ext("dm_fancy", "foldable_result", s(&f.foldable_result)));
    }
    // A subtype whose `storage_type` holds anything still overrides its parent's lists.
    let parent_holds = ty
        .parent
        .as_deref()
        .is_some_and(|p| holdable(types, p).is_some());
    let hold = holdable(types, path);
    if is_under(path, STORAGE_ROOT)
        && (hold.is_some() || parent_holds)
        && let Some(hold) = hold.or_else(|| Some(DmHoldable::default()))
    {
        let list = |l: &Vec<String>| ron::to_string(l).expect("strings always serialize");
        out.push(ext("dm_holdable", "can_hold", list(&hold.can_hold)));
        out.push(ext("dm_holdable", "cant_hold", list(&hold.cant_hold)));
        out.push(ext(
            "dm_holdable",
            "exception_hold",
            list(&hold.exception_hold),
        ));
        out.push(ext(
            "dm_holdable",
            "exception_max",
            hold.exception_max.to_string(),
        ));
    }
    if let Some(contents) = contents(types, path) {
        let ron = |f: &Vec<Fill>| ron::to_string(f).expect("fills always serialize");
        out.push(ext("dm_contents", "populate", ron(&contents.populate)));
        out.push(ext("dm_contents", "immediate", ron(&contents.immediate)));
        out.push(ext(
            "dm_contents",
            "unsupported",
            contents.unsupported.to_string(),
        ));
    }
    out
}

/// The limits of `path`'s `storage_type` datum; `None` when it has none.
pub fn storage_limits(types: &DmTypes, path: &str) -> Option<DmStorage> {
    let datum = match types.resolved(path, "storage_type")? {
        serde_json::Value::Object(o) => o.get("path")?.as_str()?,
        _ => return None,
    };
    types.types.get(datum)?;
    let mut storage = DmStorage::default();
    for var in STORAGE_VARS {
        let Some(n) = types.resolved(datum, var).and_then(|v| num(Some(v))) else {
            continue;
        };
        match var {
            "max_slots" => storage.max_slots = n.max(0.0) as u32,
            "max_specific_storage" => storage.max_specific_storage = n.clamp(0.0, 255.0) as u8,
            _ => storage.max_total_storage = n.max(0.0) as u32,
        }
    }
    Some(storage)
}

fn json_ron(var: &str, value: &serde_json::Value) -> Option<String> {
    if is_list_var(var) {
        let items: Vec<&str> = match value {
            serde_json::Value::Null => Vec::new(),
            serde_json::Value::Object(o) => o
                .get("list")?
                .as_array()?
                .iter()
                .map(serde_json::Value::as_str)
                .collect::<Option<_>>()?,
            _ => return None,
        };
        return Some(ron::to_string(&items).expect("string lists always serialize"));
    }
    if is_text_var(var) {
        let text = match value {
            serde_json::Value::Null => "",
            serde_json::Value::String(s) => s,
            _ => return None,
        };
        return Some(ron::to_string(text).expect("strings always serialize"));
    }
    num(Some(value)).map(|_| truthy(Some(value)).to_string())
}

/// Resolved programs of `path`; `None` when no type in its chain defines either proc.
pub fn contents(types: &DmTypes, path: &str) -> Option<DmContents> {
    let mut unsupported = 0;
    let populate = program(types, path, path, POPULATE, &mut unsupported, 0);
    let immediate = program(types, path, path, POPULATE_IMMEDIATE, &mut unsupported, 0);
    if populate.is_none() && immediate.is_none() {
        return None;
    }
    Some(DmContents {
        populate: populate.unwrap_or_default(),
        immediate: immediate.unwrap_or_default(),
        unsupported,
    })
}

/// The program of `proc` as `instance` runs it, starting the lookup at `from` (the instance
/// type, or the parent of the type whose `..()` is being inlined).
fn program(
    types: &DmTypes,
    instance: &str,
    from: &str,
    proc: &str,
    unsupported: &mut u32,
    depth: u32,
) -> Option<Vec<Fill>> {
    let (owner, raw) = types
        .chain(from)
        .find_map(|p| Some((p, types.populate.get(p)?.get(proc)?)))?;
    if depth > MAX_DEPTH {
        *unsupported += 1;
        return Some(Vec::new());
    }
    let parent = types.types.get(owner).and_then(|t| t.parent.as_deref());
    let mut resolve = Resolve {
        types,
        instance,
        parent,
        proc,
        unsupported,
        depth,
    };
    Some(resolve.block(raw))
}

struct Resolve<'a, 'b> {
    types: &'a DmTypes,
    instance: &'a str,
    parent: Option<&'a str>,
    proc: &'a str,
    unsupported: &'b mut u32,
    depth: u32,
}

impl Resolve<'_, '_> {
    fn block(&mut self, raw: &[RawFill]) -> Vec<Fill> {
        let mut out = Vec::new();
        for r in raw {
            match r {
                RawFill::New(path) => {
                    // shortcut: mobs never start inside (no mob contents yet); count them.
                    if is_under(path, "/obj") {
                        out.push(Fill::New(path.clone()));
                    } else {
                        *self.unsupported += 1;
                    }
                }
                RawFill::NewVar(var) => {
                    let value = self.types.resolved(self.instance, var).map(literal);
                    let paths: Option<Vec<String>> = match &value {
                        None | Some(serde_json::Value::Null) => Some(Vec::new()),
                        Some(serde_json::Value::Object(o)) => {
                            match (o.get("path"), o.get("list")) {
                                (Some(p), _) => p.as_str().map(|p| vec![p.to_owned()]),
                                // `pick(list)`: one of the entries (keys of an assoc list).
                                (_, Some(serde_json::Value::Array(items))) => items
                                    .iter()
                                    .map(|i| {
                                        let key =
                                            i.get("assoc").and_then(|a| a.get(0)).unwrap_or(i);
                                        key.get("path")?.as_str().map(str::to_owned)
                                    })
                                    .collect(),
                                _ => None,
                            }
                        }
                        Some(_) => None,
                    };
                    match paths {
                        Some(paths) if paths.iter().all(|p| is_under(p, "/obj")) => {
                            match &paths[..] {
                                [] => {}
                                [one] => out.push(Fill::New(one.clone())),
                                many => out.push(Fill::Pick {
                                    arms: many
                                        .iter()
                                        .map(|p| PickArm {
                                            weight: 1.0,
                                            body: vec![Fill::New(p.clone())],
                                        })
                                        .collect(),
                                }),
                            }
                        }
                        _ => *self.unsupported += 1,
                    }
                }
                RawFill::Repeat { count, body } => {
                    let body = self.block(body);
                    if !body.is_empty() {
                        out.push(Fill::Repeat {
                            count: *count,
                            body,
                        });
                    }
                }
                RawFill::RepeatVar { var, body } => {
                    let value = self.types.resolved(self.instance, var).map(literal);
                    match num(value.as_ref()) {
                        Some(n) if n >= 1.0 => {
                            let body = self.block(body);
                            if !body.is_empty() {
                                out.push(Fill::Repeat {
                                    count: n.min(1000.0) as u32,
                                    body,
                                });
                            }
                        }
                        Some(_) => {}
                        None => *self.unsupported += 1,
                    }
                }
                RawFill::Prob {
                    percent,
                    then,
                    otherwise,
                } => out.push(Fill::Prob {
                    percent: *percent,
                    then: self.block(then),
                    otherwise: self.block(otherwise),
                }),
                RawFill::Pick { arms } => out.push(Fill::Pick {
                    arms: arms
                        .iter()
                        .map(|(weight, body)| PickArm {
                            weight: *weight,
                            body: self.block(body),
                        })
                        .collect(),
                }),
                RawFill::IfVar {
                    var,
                    then,
                    otherwise,
                } => {
                    let branch = if truthy(self.types.resolved(self.instance, var)) {
                        then
                    } else {
                        otherwise
                    };
                    out.extend(self.block(branch));
                }
                RawFill::Parent => {
                    let inner = self.parent.and_then(|p| {
                        program(
                            self.types,
                            self.instance,
                            p,
                            self.proc,
                            self.unsupported,
                            self.depth + 1,
                        )
                    });
                    if let Some(inner) = inner.filter(|i| !i.is_empty()) {
                        out.push(Fill::Scope(inner));
                    }
                }
                RawFill::Return => out.push(Fill::Return),
                RawFill::Unsupported(_) => *self.unsupported += 1,
            }
        }
        out
    }
}

/// A dumped value, with `{ "expr": ... }` that is only a number or a type path read as that
/// (the extractor leaves some plain literals unevaluated).
fn literal(value: &serde_json::Value) -> serde_json::Value {
    let Some(expr) = value.get("expr").and_then(serde_json::Value::as_str) else {
        return value.clone();
    };
    let expr = expr.trim().trim_end_matches(';').trim();
    if let Ok(n) = expr.parse::<f64>() {
        return serde_json::json!(n);
    }
    let is_path = expr.starts_with('/')
        && expr
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/');
    if is_path {
        return serde_json::json!({ "path": expr });
    }
    value.clone()
}

/// Every type the contents programs of `needed` (and of what they spawn) can spawn: the
/// converter needs their prototypes.
pub fn contents_closure<'a>(
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
        let Some(c) = contents(types, &path) else {
            continue;
        };
        let mut paths = Vec::new();
        collect_paths(&c.populate, &mut paths);
        collect_paths(&c.immediate, &mut paths);
        for p in paths {
            if types.types.contains_key(&p) && out.insert(p.clone()) {
                stack.push(p);
            }
        }
    }
    out
}

pub fn collect_paths(fills: &[Fill], out: &mut Vec<String>) {
    for f in fills {
        match f {
            Fill::New(p) => out.push(p.clone()),
            Fill::Repeat { body, .. } | Fill::Scope(body) => collect_paths(body, out),
            Fill::Prob {
                then, otherwise, ..
            } => {
                collect_paths(then, out);
                collect_paths(otherwise, out);
            }
            Fill::Pick { arms } => arms.iter().for_each(|a| collect_paths(&a.body, out)),
            Fill::Return => {}
        }
    }
}

/// Runs a program: `spawn` gets every type path to create, in order. `rng` gives
/// `prob`/`pick_weight` their rolls (uniform in `[0, 1)`).
pub fn run(fills: &[Fill], rng: &mut impl FnMut() -> f64, spawn: &mut impl FnMut(&str)) {
    run_scope(fills, rng, spawn, 0);
}

/// `false` when a `Return` ended the scope.
fn run_scope(
    fills: &[Fill],
    rng: &mut impl FnMut() -> f64,
    spawn: &mut impl FnMut(&str),
    depth: u32,
) -> bool {
    if depth > MAX_DEPTH {
        return true;
    }
    for f in fills {
        let go_on = match f {
            Fill::New(p) => {
                spawn(p);
                true
            }
            Fill::Repeat { count, body } => {
                let mut go_on = true;
                for _ in 0..*count {
                    if !run_scope(body, rng, spawn, depth + 1) {
                        go_on = false;
                        break;
                    }
                }
                go_on
            }
            Fill::Prob {
                percent,
                then,
                otherwise,
            } => {
                // tg `prob(n)`: n percent.
                let branch = if rng() * 100.0 < f64::from(*percent) {
                    then
                } else {
                    otherwise
                };
                run_scope(branch, rng, spawn, depth + 1)
            }
            Fill::Pick { arms } => {
                let total: f64 = arms.iter().map(|a| f64::from(a.weight.max(0.0))).sum();
                let mut roll = rng() * total;
                let arm = arms.iter().find(|a| {
                    roll -= f64::from(a.weight.max(0.0));
                    roll < 0.0
                });
                arm.is_none_or(|a| run_scope(&a.body, rng, spawn, depth + 1))
            }
            Fill::Scope(body) => {
                run_scope(body, rng, spawn, depth + 1);
                true
            }
            Fill::Return => false,
        };
        if !go_on {
            return false;
        }
    }
    true
}

/// `.dmm` var edits of closet scalar vars (`opened`, `locked`, `welded`, ...).
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let Some(&(_, key)) = CLOSET_VARS.iter().find(|(v, _)| *v == var) else {
        return Err(Skip::UnknownVar);
    };
    if !is_under(path, CLOSET_ROOT) || is_list_var(var) {
        return Err(Skip::UnknownVar);
    }
    let ron = match (value, is_text_var(var)) {
        (Val::Null, true) => ron::to_string("").ok(),
        (Val::Str(s), true) => ron::to_string(s).ok(),
        (Val::Null, false) => Some("false".to_owned()),
        (Val::Num(n), false) => Some((n != 0.0).to_string()),
        _ => None,
    };
    ron.map(|r| ext("dm_closet", key, r))
        .ok_or(Skip::Unsupported)
}

/// `.dmm` var edit of a closet's access list; `None` for any other var.
pub fn field_dmm(path: &str, var: &str, value: &DmValue) -> Option<Result<Field, Skip>> {
    if !is_under(path, CLOSET_ROOT) || !is_list_var(var) {
        return None;
    }
    let key = if var == "req_access" {
        "req_access"
    } else {
        "req_one_access"
    };
    let items: Option<Vec<&str>> = match value {
        DmValue::Null => Some(Vec::new()),
        DmValue::List(items) => items
            .iter()
            .map(|(k, v)| match (k, v) {
                (DmValue::Str(s), None) => Some(s.as_str()),
                _ => None,
            })
            .collect(),
        _ => None,
    };
    Some(
        items
            .map(|items| {
                ext(
                    "dm_closet",
                    key,
                    ron::to_string(&items).expect("lists serialize"),
                )
            })
            .ok_or(Skip::Unsupported),
    )
}

/// How many closet / storage types of `paths` resolve their contents fully, partially (some
/// statements not understood) or have none.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub full: usize,
    pub partial: BTreeMap<String, u32>,
    pub empty: usize,
}

pub fn coverage<'a>(types: &DmTypes, paths: impl IntoIterator<Item = &'a str>) -> Coverage {
    let mut out = Coverage::default();
    for path in paths {
        match contents(types, path) {
            None => out.empty += 1,
            Some(c) if c.unsupported == 0 => {
                let mut paths = Vec::new();
                collect_paths(&c.populate, &mut paths);
                collect_paths(&c.immediate, &mut paths);
                if paths.is_empty() {
                    out.empty += 1;
                } else {
                    out.full += 1;
                }
            }
            Some(c) => {
                out.partial.insert(path.to_owned(), c.unsupported);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/atom": { "parent": null },
        "/atom/movable": { "parent": "/atom" },
        "/obj": { "parent": "/atom/movable", "declared_vars": { "req_access": null } },
        "/obj/item": { "parent": "/obj", "declared_vars": { "w_class": 3 } },
        "/obj/item/a": { "parent": "/obj/item", "declared_vars": { "w_class": 1 } },
        "/obj/item/b": { "parent": "/obj/item" },
        "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
            "storage_type": { "path": "/datum/storage" } } },
        "/obj/item/storage/backpack": { "parent": "/obj/item/storage", "declared_vars": {
            "storage_type": { "path": "/datum/storage/backpack" }, "w_class": 4 } },
        "/obj/item/storage/lockbox": { "parent": "/obj/item/storage", "declared_vars": {
            "req_access": { "list": ["armory"] }, "icon_locked": "lockbox+l",
            "icon_closed": "lockbox", "icon_broken": "lockbox+b" } },
        "/obj/item/storage/lockbox/medal": { "parent": "/obj/item/storage/lockbox",
            "declared_vars": { "req_access": { "list": ["captain"] }, "icon_locked": "medalbox+l" } },
        "/obj/structure/closet/secure_closet/brig": {
            "parent": "/obj/structure/closet/secure_closet", "declared_vars": { "id": null } },
        "/datum": { "parent": null },
        "/datum/storage": { "parent": "/datum", "declared_vars": {
            "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14 } },
        "/datum/storage/backpack": { "parent": "/datum/storage", "declared_vars": {
            "max_slots": 21, "max_total_storage": 21 } },
        "/obj/structure": { "parent": "/obj" },
        "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
            "opened": 0, "locked": 0, "secure": 0, "icon_door": null, "storage_capacity": 30,
            "enable_door_overlay": 1, "empty": 0 } },
        "/obj/structure/closet/secure_closet": { "parent": "/obj/structure/closet",
            "declared_vars": { "locked": 1, "secure": 1, "req_access": { "list": ["captain"] } } },
        "/obj/structure/closet/secure_closet/empty": {
            "parent": "/obj/structure/closet/secure_closet", "declared_vars": { "empty": 1 } }
      },
      "populate": {
        "/obj/structure/closet": { "PopulateContents": ["return"] },
        "/obj/structure/closet/secure_closet": { "PopulateContents": [
            "parent",
            { "new": "/obj/item/a" },
            { "if_var": { "var": "empty", "then": ["return"], "otherwise": [] } },
            { "repeat": { "count": 2, "body": [{ "new": "/obj/item/b" }] } },
            { "new": "/mob/x" },
            { "unsupported": "for(...)" }
          ],
          "populate_contents_immediate": [{ "new": "/obj/item/storage/backpack" }] }
      }
    }"##;

    fn types() -> DmTypes {
        DmTypes::parse(TYPES.as_bytes()).unwrap()
    }

    #[test]
    fn programs_inline_parents_and_type_vars() {
        let types = types();
        let c = contents(&types, "/obj/structure/closet/secure_closet").unwrap();
        let new = |p: &str| Fill::New(p.to_owned());
        assert_eq!(
            c.populate,
            [
                Fill::Scope(vec![Fill::Return]),
                new("/obj/item/a"),
                Fill::Repeat {
                    count: 2,
                    body: vec![new("/obj/item/b")]
                },
            ]
        );
        assert_eq!(c.immediate, [new("/obj/item/storage/backpack")]);
        assert_eq!(c.unsupported, 2, "the mob and the unsupported loop");
        let empty = contents(&types, "/obj/structure/closet/secure_closet/empty").unwrap();
        assert_eq!(empty.populate[2], Fill::Return);

        let mut spawned = Vec::new();
        run(&c.populate, &mut || 0.5, &mut |p| {
            spawned.push(p.to_owned())
        });
        assert_eq!(spawned, ["/obj/item/a", "/obj/item/b", "/obj/item/b"]);
        spawned.clear();
        run(&empty.populate, &mut || 0.5, &mut |p| {
            spawned.push(p.to_owned())
        });
        assert_eq!(spawned, ["/obj/item/a"], "return ends the program");

        let closure = contents_closure(&types, ["/obj/structure/closet/secure_closet"]);
        assert!(closure.contains("/obj/item/storage/backpack"));
        assert!(closure.contains("/obj/item/b"));
        let cov = coverage(
            &types,
            [
                "/obj/structure/closet",
                "/obj/structure/closet/secure_closet",
            ],
        );
        assert_eq!((cov.full, cov.partial.len(), cov.empty), (0, 1, 1));
    }

    #[test]
    fn prob_and_pick_follow_the_roll() {
        let fills = vec![
            Fill::Prob {
                percent: 40.0,
                then: vec![Fill::New("yes".into())],
                otherwise: vec![Fill::New("no".into())],
            },
            Fill::Pick {
                arms: vec![
                    PickArm {
                        weight: 1.0,
                        body: vec![Fill::New("first".into())],
                    },
                    PickArm {
                        weight: 3.0,
                        body: vec![Fill::New("second".into())],
                    },
                ],
            },
        ];
        let mut out = Vec::new();
        run(&fills, &mut || 0.3, &mut |p| out.push(p.to_owned()));
        assert_eq!(out, ["yes", "second"]);
        out.clear();
        run(&fills, &mut || 0.1, &mut |p| out.push(p.to_owned()));
        assert_eq!(out, ["yes", "first"]);
        out.clear();
        run(&fills, &mut || 0.9, &mut |p| out.push(p.to_owned()));
        assert_eq!(out, ["no", "second"]);
    }

    #[test]
    fn closet_storage_and_weight_fields() {
        let types = types();
        let ron = |path| crate::value::patch_ron(&type_fields(&types, path));
        let secure = ron("/obj/structure/closet/secure_closet");
        assert!(
            secure.contains(r#"locked: true, secure: true, req_access: ["captain"]"#),
            "{secure}"
        );
        let root = ron("/obj/structure/closet");
        assert!(root.contains("is_crate: false"), "{root}");
        assert!(root.contains("enable_door_overlay: true"), "{root}");
        assert!(root.contains("storage_capacity: 30"), "{root}");
        assert_eq!(
            storage_limits(&types, "/obj/item/storage/backpack"),
            Some(DmStorage {
                max_slots: 21,
                max_specific_storage: 3,
                max_total_storage: 21
            })
        );
        let pack = ron("/obj/item/storage/backpack");
        assert!(pack.contains(r#""dm_weight": (w_class: 4)"#), "{pack}");
        assert!(pack.contains("max_slots: 21"), "{pack}");
        assert_eq!(
            ext_field("/obj/structure/closet", "opened", Val::Num(1.0)),
            Ok(ext("dm_closet", "opened", "true"))
        );
        assert_eq!(
            ext_field("/obj/item/a", "opened", Val::Num(1.0)),
            Err(Skip::UnknownVar)
        );
    }

    #[test]
    fn lockbox_and_brig_closet_fields() {
        let types = types();
        let ron = |path| crate::value::patch_ron(&type_fields(&types, path));
        let root = ron("/obj/item/storage/lockbox");
        assert!(
            root.contains(
                r#""dm_lockbox": (req_access: ["armory"], icon_locked: "lockbox+l", icon_closed: "lockbox", icon_broken: "lockbox+b")"#
            ),
            "{root}"
        );
        let medal = ron("/obj/item/storage/lockbox/medal");
        assert!(
            medal.contains(r#""dm_lockbox": (req_access: ["captain"], icon_locked: "medalbox+l")"#),
            "{medal}"
        );
        assert!(!ron("/obj/item/storage/backpack").contains("dm_lockbox"));
        let brig = ron("/obj/structure/closet/secure_closet/brig");
        assert!(brig.contains(r#"id: """#), "{brig}");
        // The map names the cell: `id = "Cell 1"`.
        assert_eq!(
            ext_field(
                "/obj/structure/closet/secure_closet/brig",
                "id",
                Val::Str("Cell 1")
            ),
            Ok(ext("dm_closet", "id", r#""Cell 1""#))
        );
    }
}
