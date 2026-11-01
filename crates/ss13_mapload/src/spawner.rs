//! Map spawners (`code/game/objects/effects/spawners/`): `/obj/effect/spawner` types replace
//! themselves with other atoms in `Initialize` and are deleted. Resolved once at load.
//!
//! Three static models, reproduced from their base procs:
//! - `/obj/effect/spawner/random` (`spawn_loot`): `spawn_loot_count` weighted picks
//!   (`pick_weight_recursive`: nested lists, missing or zero weights count as 1) from `loot`
//!   plus `typesof(loot_type_path)` / `subtypesof(loot_subtype_path)`, after a
//!   `spawn_loot_chance` roll; `spawn_all_loot`, `spawn_loot_double`, `spawn_scatter_radius`,
//!   `spawn_random_offset`, `spawn_loot_split`, the spawner's `dir` and pixel offsets;
//! - `/obj/effect/spawner/structure`: everything in `spawn_list`, in order;
//! - `/obj/effect/spawner/costume`: everything in `items`, in order.
//!
//! A spawner whose type chain overrides one of [`LOOT_PROCS`] below its base computes its loot in
//! code (global loot tables, station traits, holidays...): it is removed without loot and listed
//! in the report, unless the override is in [`PICK_NEUTRAL`].

use std::collections::{BTreeMap, BTreeSet, HashMap};

use bevy::prelude::*;
use ss13_assets::dmm::{DmValue, DmmObject};
use ss13_assets::manifest::AssetManifest;
use ss13_proto::Prototypes;
use ss13_shared::{Appearance, Dir, Facing, IconStateId, TilePos};
use ss13_world::SpatialIndex;

use crate::DmAtom;
use crate::Turf;
use crate::fidelity::{ConvertContext, ext, is_under, num, truthy};
use crate::load::Template;
use crate::undertile::DmTurf;
use crate::value::{Field, Skip, Val};

pub const SPAWNER_ROOT: &str = "/obj/effect/spawner";

/// `(base type, list var, weighted random picks)`.
const BASES: [(&str, &str, bool); 3] = [
    ("/obj/effect/spawner/random", "loot", true),
    ("/obj/effect/spawner/structure", "spawn_list", false),
    ("/obj/effect/spawner/costume", "items", false),
];

/// Procs of the base models; overriding one below the base changes what is spawned.
pub const LOOT_PROCS: [&str; 5] = [
    "Initialize",
    "spawn_loot",
    "make_item",
    "can_spawn",
    "get_spawn_locations",
];

/// Overrides read in the reference that keep the pick and the placement static.
// shortcut: those marked "visual" change the item's look at random (open crate, can sprite,
// graffiti color); the loot is drawn in its default look.
pub const PICK_NEUTRAL: [(&str, &str); 10] = [
    // `rcd_memory` of the turf.
    ("/obj/effect/spawner/structure/window", "Initialize"),
    // `extended_inventory` of the vendor.
    ("/obj/effect/spawner/random/vending", "make_item"),
    // Adds a hotdog vendor on HOTDOG_DAY only; holidays are not simulated.
    ("/obj/effect/spawner/random/vending/snackvend", "Initialize"),
    // Visual: opens the closet with 50%.
    (
        "/obj/effect/spawner/random/structure/closet_empty",
        "make_item",
    ),
    // Fills the crate's contents (not drawn).
    (
        "/obj/effect/spawner/random/structure/closet_empty/crate/with_loot",
        "make_item",
    ),
    // `TRAIT_CONTRABAND`.
    ("/obj/effect/spawner/random/contraband", "make_item"),
    // Anchors the statue.
    ("/obj/effect/spawner/random/decoration/statue", "make_item"),
    // Turns the light on.
    (
        "/obj/effect/spawner/random/decoration/glowstick/on",
        "make_item",
    ),
    // Visual: random can sprite.
    ("/obj/effect/spawner/random/trash/crushed_can", "make_item"),
    // Visual: random graffiti state and color.
    ("/obj/effect/spawner/random/trash/graffiti", "make_item"),
];

/// Nested spawners deeper than this are dropped (a cycle in the loot data).
const MAX_DEPTH: u32 = 8;

/// One loot list entry: a type path, or a nested list (`path` empty) whose entries point back at
/// it through `parent`.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct LootEntry {
    pub path: String,
    pub weight: u32,
    /// Index of the nested list entry this one belongs to; `None` = top level.
    pub parent: Option<u16>,
}

/// A spawner's resolved static data (prototype data; `.dmm` edits patch single fields).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmSpawner {
    pub type_path: String,
    /// Why the loot is not static (empty = resolvable).
    pub unresolved: String,
    /// Weighted picks (`/random`); otherwise every entry once, in order.
    pub random: bool,
    pub loot: Vec<LootEntry>,
    pub count: u32,
    /// `spawn_loot_chance`, percent.
    pub chance: f32,
    pub all: bool,
    pub double: bool,
    pub scatter: u8,
    pub random_offset: bool,
    pub split: bool,
    pub split_px: i16,
}

/// Deterministic generator for load-time rolls (SplitMix64).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// BYOND `rand(lo, hi)`, inclusive.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }

    /// BYOND `prob(percent)`.
    pub fn prob(&mut self, percent: f32) -> bool {
        percent >= 100.0
            || (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 * 100.0 < f64::from(percent)
    }
}

/// tg `pick_weight_recursive` over the entries whose parent is `parent` and that are still in
/// `pool` (top level) or all of them (nested). Returns the picked top-level index into `pool` and
/// the leaf entry.
pub fn pick(loot: &[LootEntry], pool: &[usize], rng: &mut Rng) -> Option<(usize, usize)> {
    let pick_in = |items: &[usize], rng: &mut Rng| -> Option<usize> {
        let total: u64 = items
            .iter()
            .map(|&i| u64::from(loot[i].weight.max(1)))
            .sum();
        if total == 0 {
            return None;
        }
        let mut r = rng.range(1, total as i64);
        items.iter().position(|&i| {
            r -= i64::from(loot[i].weight.max(1));
            r <= 0
        })
    };
    let top = pick_in(pool, rng)?;
    let mut entry = pool[top];
    for _ in 0..MAX_DEPTH {
        if !loot[entry].path.is_empty() {
            return Some((top, entry));
        }
        let children: Vec<usize> = (0..loot.len())
            .filter(|&i| loot[i].parent == Some(entry as u16))
            .collect();
        entry = children[pick_in(&children, rng)?];
    }
    None
}

/// Converter: the static model of a spawner type.
pub fn spawner(ctx: &ConvertContext, path: &str) -> DmSpawner {
    let types = ctx.types;
    let mut s = DmSpawner {
        type_path: path.to_owned(),
        count: 1,
        chance: 100.0,
        double: true,
        split_px: 2,
        ..default()
    };
    let chain: Vec<&str> = types.chain(path).collect();
    let Some(&(base, list_var, random)) = BASES.iter().find(|(b, ..)| chain.contains(b)) else {
        s.unresolved = "custom spawner".into();
        return s;
    };
    for &t in chain.iter().take_while(|t| **t != base) {
        for p in &types.types[t].procs {
            if LOOT_PROCS.contains(&p.as_str()) && !PICK_NEUTRAL.contains(&(t, p.as_str())) {
                s.unresolved = format!("{t}/{p}()");
                return s;
            }
        }
    }
    s.random = random;
    let mut entries = Vec::new();
    if let Some(v) = types.resolved(path, list_var)
        && !push_list(v, None, &mut entries)
    {
        s.unresolved = format!("{list_var} is not constant");
        return s;
    }
    if random {
        for (var, with_self) in [("loot_type_path", true), ("loot_subtype_path", false)] {
            let Some(root) = types
                .resolved(path, var)
                .and_then(|v| v.get("path"))
                .and_then(|p| p.as_str())
            else {
                continue;
            };
            for t in ctx.typesof(root) {
                if with_self || t != root {
                    entries.push(LootEntry {
                        path: t.to_owned(),
                        weight: 1,
                        parent: None,
                    });
                }
            }
        }
    }
    s.loot = clean(entries, |p| {
        let abstract_type = types
            .resolved(p, "abstract_type")
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str())
            == Some(p);
        !abstract_type && types.types.contains_key(p)
    });
    let number = |var: &str, default: f64| -> Option<f64> {
        match types.resolved(path, var) {
            None => Some(default),
            v => num(v),
        }
    };
    let (Some(count), Some(chance), Some(scatter), Some(split_px)) = (
        number("spawn_loot_count", 1.0),
        number("spawn_loot_chance", 100.0),
        number("spawn_scatter_radius", 0.0),
        number("spawn_loot_split_pixel_offsets", 2.0),
    ) else {
        s.unresolved = "spawn_* var is not constant".into();
        return s;
    };
    s.count = count.clamp(0.0, f64::from(u32::MAX)) as u32;
    s.chance = chance as f32;
    s.scatter = scatter.clamp(0.0, 255.0) as u8;
    s.split_px = split_px.clamp(1.0, 16.0) as i16;
    let flag = |var: &str, default: bool| match types.resolved(path, var) {
        None => default,
        v => truthy(v),
    };
    s.all = flag("spawn_all_loot", false);
    s.double = flag("spawn_loot_double", true);
    s.random_offset = flag("spawn_random_offset", false);
    s.split = flag("spawn_loot_split", false);
    s
}

/// Appends a dumped DM list (or a single path) as entries; `false` if it is not plain data.
fn push_list(v: &serde_json::Value, parent: Option<u16>, out: &mut Vec<LootEntry>) -> bool {
    let leaf = |item: &serde_json::Value, weight: u32, out: &mut Vec<LootEntry>| -> bool {
        if let Some(p) = item.get("path").and_then(|p| p.as_str()) {
            out.push(LootEntry {
                path: p.to_owned(),
                weight,
                parent,
            });
            return true;
        }
        if item.get("list").is_some() {
            let Ok(index) = u16::try_from(out.len()) else {
                return false;
            };
            out.push(LootEntry {
                path: String::new(),
                weight,
                parent,
            });
            return push_list(item, Some(index), out);
        }
        // `null` entries spawn nothing in tg either.
        item.is_null()
    };
    match v {
        serde_json::Value::Null => true,
        _ if v.get("path").is_some() => leaf(v, 1, out),
        _ => {
            let Some(items) = v.get("list").and_then(|l| l.as_array()) else {
                return false;
            };
            items.iter().all(|item| match item.get("assoc") {
                Some(pair) => {
                    let (Some(k), Some(w)) = (pair.get(0), pair.get(1)) else {
                        return false;
                    };
                    let Some(w) = num(Some(w)) else {
                        return false;
                    };
                    leaf(k, w.round().clamp(0.0, f64::from(u32::MAX)) as u32, out)
                }
                None => leaf(item, 1, out),
            })
        }
    }
}

/// tg's `fill_with_ones` makes each list an assoc list (duplicate keys collapse, the last weight
/// wins); `can_spawn` rejects abstract types (`abstract_type == type`), which equals never
/// picking them: `pickable` is false for those and for unknown types.
fn clean(entries: Vec<LootEntry>, pickable: impl Fn(&str) -> bool) -> Vec<LootEntry> {
    let mut keep = vec![true; entries.len()];
    for (i, e) in entries.iter().enumerate() {
        if e.path.is_empty() {
            continue;
        }
        let later_dup = entries[i + 1..]
            .iter()
            .any(|o| o.parent == e.parent && o.path == e.path);
        if later_dup || !pickable(&e.path) {
            keep[i] = false;
        }
    }
    // Re-index parents after dropping entries.
    let mut new_index = vec![None; entries.len()];
    let mut out = Vec::new();
    for (i, e) in entries.into_iter().enumerate() {
        if !keep[i] {
            continue;
        }
        new_index[i] = Some(out.len() as u16);
        out.push(LootEntry {
            parent: e.parent.and_then(|p| new_index[usize::from(p)]),
            ..e
        });
    }
    out
}

/// Every type a used spawner can spawn, through nested spawners: the converter needs their
/// prototypes.
pub fn loot_closure(ctx: &ConvertContext, needed: &[&str]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack: Vec<String> = needed
        .iter()
        .filter(|p| is_under(p, SPAWNER_ROOT))
        .map(|p| (*p).to_owned())
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(path) = stack.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let s = spawner(ctx, &path);
        if !s.unresolved.is_empty() {
            continue;
        }
        for e in s.loot.into_iter().filter(|e| !e.path.is_empty()) {
            if is_under(&e.path, SPAWNER_ROOT) {
                stack.push(e.path.clone());
            }
            out.insert(e.path);
        }
    }
    out
}

const COMPONENT: &str = "dm_spawner";

/// Converter fields: the whole model for every spawner type (it depends on several vars and
/// procs at once, so it is not split into per-declaration fields).
pub fn type_fields(ctx: &ConvertContext, path: &str) -> Vec<Field> {
    if !is_under(path, SPAWNER_ROOT) {
        return Vec::new();
    }
    let s = spawner(ctx, path);
    let ron = |v: &dyn std::fmt::Debug| format!("{v:?}");
    vec![
        ext(
            COMPONENT,
            "type_path",
            ron::to_string(&s.type_path).expect("strings serialize"),
        ),
        ext(
            COMPONENT,
            "unresolved",
            ron::to_string(&s.unresolved).expect("strings serialize"),
        ),
        ext(COMPONENT, "random", ron(&s.random)),
        ext(COMPONENT, "loot", loot_ron(&s.loot)),
        ext(COMPONENT, "count", ron(&s.count)),
        ext(COMPONENT, "chance", ron(&s.chance)),
        ext(COMPONENT, "all", ron(&s.all)),
        ext(COMPONENT, "double", ron(&s.double)),
        ext(COMPONENT, "scatter", ron(&s.scatter)),
        ext(COMPONENT, "random_offset", ron(&s.random_offset)),
        ext(COMPONENT, "split", ron(&s.split)),
        ext(COMPONENT, "split_px", ron(&s.split_px)),
    ]
}

fn loot_ron(loot: &[LootEntry]) -> String {
    let entries: Vec<String> = loot
        .iter()
        .map(|e| {
            format!(
                "(path: {}, weight: {}, parent: {})",
                ron::to_string(&e.path).expect("strings serialize"),
                e.weight,
                e.parent.map_or("None".to_owned(), |p| format!("Some({p})"))
            )
        })
        .collect();
    format!("[{}]", entries.join(", "))
}

/// `.dmm` var edit of a spawner's list var (`loot`, `spawn_list`, `items`); `None` for any other
/// var.
pub fn field_dmm(path: &str, var: &str, value: &DmValue) -> Option<Result<Field, Skip>> {
    let entries = match dmm_loot(path, var, value)? {
        Ok(entries) => entries,
        Err(skip) => return Some(Err(skip)),
    };
    // shortcut: no dm_types at load, so abstract types are not dropped as the converter drops
    // them (unknown ones count in `missing_loot`); none of the reference maps lists one. Clean
    // in the converter once map edits are converted there.
    let loot = clean(entries, |_| true);
    Some(Ok(ext(COMPONENT, "loot", loot_ron(&loot))))
}

/// Loot types named by the list var edits of a `.dmm` object: the converter needs their
/// prototypes.
pub fn dmm_loot_paths(obj: &DmmObject) -> Vec<String> {
    obj.vars
        .iter()
        .filter_map(|(var, value)| dmm_loot(&obj.path, var, value)?.ok())
        .flatten()
        .filter(|e| !e.path.is_empty())
        .map(|e| e.path)
        .collect()
}

fn dmm_loot(path: &str, var: &str, value: &DmValue) -> Option<Result<Vec<LootEntry>, Skip>> {
    let &(_, list_var, _) = BASES.iter().find(|(base, ..)| is_under(path, base))?;
    if var != list_var {
        return None;
    }
    let mut entries = Vec::new();
    let plain = dmm_json(value).is_some_and(|v| push_list(&v, None, &mut entries));
    Some(if plain {
        Ok(entries)
    } else {
        Err(Skip::Unsupported)
    })
}

/// A `.dmm` constant in the `dm_types.json` encoding [`push_list`] reads; `None` for values a
/// loot list cannot hold.
fn dmm_json(v: &DmValue) -> Option<serde_json::Value> {
    use serde_json::{Value, json};
    Some(match v {
        DmValue::Null => Value::Null,
        DmValue::Number(n) => json!(n),
        DmValue::Path(p) => json!({ "path": p }),
        DmValue::List(items) => {
            let items = items
                .iter()
                .map(|(k, w)| {
                    let k = dmm_json(k)?;
                    Some(match w {
                        None => k,
                        Some(w) => json!({ "assoc": [k, dmm_json(w)?] }),
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            json!({ "list": items })
        }
        DmValue::Str(_) | DmValue::Resource(_) | DmValue::Raw(_) => return None,
    })
}

/// `.dmm` var edits of the scalar spawner vars.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    if !is_under(path, SPAWNER_ROOT) {
        return Err(Skip::UnknownVar);
    }
    let n = |v: Val| match v {
        Val::Num(n) => Ok(n),
        Val::Null => Ok(0.0),
        _ => Err(Skip::Unsupported),
    };
    let (key, ron) = match var {
        "spawn_loot_count" => ("count", format!("{}", n(value)?.clamp(0.0, 1e9) as u32)),
        "spawn_loot_chance" => ("chance", format!("{:?}", n(value)? as f32)),
        "spawn_scatter_radius" => ("scatter", format!("{}", n(value)?.clamp(0.0, 255.0) as u8)),
        "spawn_all_loot" => ("all", (n(value)? != 0.0).to_string()),
        "spawn_loot_double" => ("double", (n(value)? != 0.0).to_string()),
        "spawn_random_offset" => ("random_offset", (n(value)? != 0.0).to_string()),
        "spawn_loot_split" => ("split", (n(value)? != 0.0).to_string()),
        "spawn_loot_split_pixel_offsets" => {
            ("split_px", format!("{}", n(value)?.clamp(1.0, 16.0) as i16))
        }
        _ => return Err(Skip::UnknownVar),
    };
    Ok(ext(COMPONENT, key, ron))
}

#[derive(Debug, Default)]
pub struct SpawnerStats {
    /// Spawners placed on the map whose loot was rolled.
    pub resolved: usize,
    /// Entities spawned from loot (nested spawners excluded).
    pub spawned: usize,
    /// Nested spawners resolved inside loot.
    pub nested: usize,
    /// Rolls that produced nothing (chance, empty list, no scatter tile).
    pub empty: usize,
    /// `type (reason)` -> spawners removed without loot.
    pub unresolved: BTreeMap<String, usize>,
    /// Loot types without a prototype (rerun the converter).
    pub missing_loot: BTreeMap<String, usize>,
}

struct Resolver<'a> {
    protos: &'a Prototypes,
    index: HashMap<(&'a str, &'a str), IconStateId>,
    templates: HashMap<String, Option<Template>>,
    stats: SpawnerStats,
}

/// Replaces every spawner on the map with its loot. Loot is spawned through the same templates
/// as map atoms, so it is indistinguishable from mapped atoms.
pub fn resolve_all(world: &mut World, manifest: &AssetManifest, rng: &mut Rng) -> SpawnerStats {
    let mut query = world.query::<(Entity, &TilePos, &DmSpawner, Option<&DmAtom>)>();
    let mut spawners: Vec<(TilePos, Entity, DmSpawner, DmAtom)> = query
        .iter(world)
        .map(|(e, p, s, a)| (*p, e, s.clone(), a.cloned().unwrap_or_default()))
        .collect();
    // Query order follows archetypes; sort so a seed always gives the same map.
    spawners.sort_by_key(|(p, e, ..)| (p.z, p.y, p.x, *e));
    if spawners.is_empty() {
        return SpawnerStats::default();
    }
    world.resource_scope(|world, protos: Mut<Prototypes>| {
        let mut r = Resolver {
            protos: &protos,
            index: manifest.index(),
            templates: HashMap::new(),
            stats: SpawnerStats::default(),
        };
        for (pos, e, spawner, atom) in spawners {
            world.despawn(e);
            if !spawner.unresolved.is_empty() {
                let key = format!("{} ({})", spawner.type_path, spawner.unresolved);
                *r.stats.unresolved.entry(key).or_default() += 1;
                continue;
            }
            r.stats.resolved += 1;
            r.resolve(world, &spawner, &atom, pos, rng, 0);
        }
        r.stats
    })
}

impl Resolver<'_> {
    fn resolve(
        &mut self,
        world: &mut World,
        s: &DmSpawner,
        atom: &DmAtom,
        pos: TilePos,
        rng: &mut Rng,
        depth: u32,
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        if !s.random {
            let mut any = false;
            for e in s
                .loot
                .iter()
                .filter(|e| !e.path.is_empty() && e.parent.is_none())
            {
                any |= self.spawn(world, &e.path, pos, None, rng, depth);
            }
            self.stats.empty += usize::from(!any);
            return;
        }
        if !rng.prob(s.chance) {
            self.stats.empty += 1;
            return;
        }
        let (count, double) = if s.all {
            (usize::MAX, false)
        } else {
            (s.count as usize, s.double)
        };
        let mut locations = if s.scatter > 0 {
            scatter_locations(world, pos, i32::from(s.scatter))
        } else {
            Vec::new()
        };
        let mut pool: Vec<usize> = (0..s.loot.len())
            .filter(|&i| s.loot[i].parent.is_none())
            .collect();
        let dir = Dir::from_bits(atom.dir).unwrap_or_default();
        let mut spawned = 0usize;
        let mut any = false;
        while spawned < count && !pool.is_empty() {
            // An empty nested list picks null: tg counts it as spawned and rolls on.
            let Some((_, leaf)) = pick(&s.loot, &pool, rng) else {
                spawned += 1;
                if s.all {
                    break; // the same as below: nothing left the pool
                }
                continue;
            };
            // tg removes the picked path from the top list: a no-op for nested picks.
            let removed = !double
                && match pool
                    .iter()
                    .position(|&i| s.loot[i].path == s.loot[leaf].path)
                {
                    Some(i) => {
                        pool.remove(i);
                        true
                    }
                    None => false,
                };
            if s.scatter == 0 || !locations.is_empty() {
                let at = if s.scatter > 0 {
                    locations.swap_remove(rng.range(0, locations.len() as i64 - 1) as usize)
                } else {
                    pos
                };
                let offset = if s.random_offset {
                    Some([rng.range(-16, 16) as i16, rng.range(-16, 16) as i16])
                } else if s.split {
                    let divider = (16 / s.split_px).max(1) as usize;
                    (spawned > 0).then(|| {
                        let column = (spawned / divider) as i16;
                        let row = (spawned % divider) as i16;
                        [s.split_px * row + column * s.split_px, s.split_px * row]
                    })
                } else {
                    let [x, y] = [atom.pixel_x, atom.pixel_y];
                    (x != 0 || y != 0).then_some([x, y])
                };
                let placed = Placement { dir, offset };
                any |= self.spawn(world, &s.loot[leaf].path, at, Some(placed), rng, depth);
            }
            spawned += 1;
            if s.all && !removed {
                break; // nothing can leave a nested list: tg would loop until INFINITY
            }
        }
        self.stats.empty += usize::from(!any);
    }

    /// Spawns one loot path (resolving nested spawners); `true` if anything was spawned.
    fn spawn(
        &mut self,
        world: &mut World,
        path: &str,
        pos: TilePos,
        placed: Option<Placement>,
        rng: &mut Rng,
        depth: u32,
    ) -> bool {
        let Some(proto) = self.protos.get(path).cloned() else {
            *self.stats.missing_loot.entry(path.to_owned()).or_default() += 1;
            return false;
        };
        if let Some(nested) = proto.get::<DmSpawner>().cloned() {
            self.stats.nested += 1;
            if !nested.unresolved.is_empty() {
                let key = format!("{} ({})", nested.type_path, nested.unresolved);
                *self.stats.unresolved.entry(key).or_default() += 1;
                return false;
            }
            let atom = proto.get::<DmAtom>().cloned().unwrap_or_default();
            let before = self.stats.spawned;
            self.resolve(world, &nested, &atom, pos, rng, depth + 1);
            return self.stats.spawned > before;
        }
        let index = &self.index;
        let template = self
            .templates
            .entry(path.to_owned())
            .or_insert_with(|| Some(Template::new(proto, index)));
        let Some(template) = template else {
            return false;
        };
        let e = template.spawn(world, Some(pos));
        self.stats.spawned += 1;
        if let Some(Placement { dir, offset }) = placed {
            let mut entity = world.entity_mut(e);
            entity.insert(Facing(dir));
            if let (Some(offset), Some(mut appearance)) = (offset, entity.get_mut::<Appearance>()) {
                appearance.pixel_offset = offset;
            }
        }
        true
    }
}

/// Random-spawner placement: `setDir(dir)` and the pixel offset rule.
#[derive(Clone, Copy)]
struct Placement {
    dir: Dir,
    offset: Option<[i16; 2]>,
}

/// tg `get_spawn_locations`: tiles within `radius` that are not closed or groundless, with an
/// unblocked line from the spawner.
// shortcut: `view()` opacity and multi-z ground below are not checked; walls block the line,
// which covers the mapped uses (maintenance rooms).
fn scatter_locations(world: &World, center: TilePos, radius: i32) -> Vec<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let turf = |pos: TilePos| {
        index
            .on_tile(pos)
            .iter()
            .find(|&&e| world.get::<Turf>(e).is_some())
            .and_then(|&e| world.get::<DmTurf>(e))
    };
    let blocked = |pos: TilePos| !index.dense_on_tile(pos).is_empty();
    let mut out = Vec::new();
    for y in center.y - radius..=center.y + radius {
        for x in center.x - radius..=center.x + radius {
            let pos = TilePos::new(x, y, center.z);
            let Some(t) = turf(pos) else {
                continue;
            };
            if t.closed || t.groundless {
                continue;
            }
            if line(center, pos).iter().all(|&p| !blocked(p)) {
                out.push(pos);
            }
        }
    }
    out
}

/// Tiles from `a` to `b` inclusive (Bresenham, like tg `get_line`).
fn line(a: TilePos, b: TilePos) -> Vec<TilePos> {
    let (mut x, mut y) = (a.x, a.y);
    let (dx, dy) = ((b.x - a.x).abs(), -(b.y - a.y).abs());
    let (sx, sy) = ((b.x - a.x).signum(), (b.y - a.y).signum());
    let mut err = dx + dy;
    let mut out = vec![a];
    while (x, y) != (b.x, b.y) {
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        out.push(TilePos::new(x, y, a.z));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, weight: u32, parent: Option<u16>) -> LootEntry {
        LootEntry {
            path: path.into(),
            weight,
            parent,
        }
    }

    #[test]
    fn weighted_picks_follow_weights_and_seed() {
        // a = 1, b = 3, a nested list (weight 4) of c = 1 and d (no weight -> 1).
        let loot = [
            entry("/a", 1, None),
            entry("/b", 3, None),
            entry("", 4, None),
            entry("/c", 1, Some(2)),
            entry("/d", 0, Some(2)),
        ];
        let pool = [0, 1, 2];
        let mut counts = [0usize; 5];
        let mut rng = Rng::new(42);
        let draws = 80_000;
        for _ in 0..draws {
            let (_, leaf) = pick(&loot, &pool, &mut rng).unwrap();
            counts[leaf] += 1;
        }
        let share = |i: usize| counts[i] as f64 / draws as f64;
        // a 1/8, b 3/8, c and d 4/8 * 1/2 each.
        for (i, expected) in [(0, 0.125), (1, 0.375), (3, 0.25), (4, 0.25)] {
            assert!((share(i) - expected).abs() < 0.01, "{i}: {}", share(i));
        }
        assert_eq!(counts[2], 0, "a nested list is never a result");

        // Same seed, same sequence.
        let seq = |seed| {
            let mut rng = Rng::new(seed);
            (0..32)
                .map(|_| pick(&loot, &pool, &mut rng).unwrap().1)
                .collect::<Vec<_>>()
        };
        assert_eq!(seq(7), seq(7));
        assert_ne!(seq(7), seq(8));
    }

    #[test]
    fn prob_and_range() {
        let mut rng = Rng::new(1);
        assert!((0..1000).all(|_| rng.prob(100.0)));
        assert!((0..1000).all(|_| !rng.prob(0.0)));
        let hits = (0..10_000).filter(|_| rng.prob(25.0)).count();
        assert!((2_200..2_800).contains(&hits), "{hits}");
        assert!((0..1000).all(|_| (1..=6).contains(&rng.range(1, 6))));
    }

    #[test]
    fn bresenham_line() {
        let p = |x, y| TilePos::new(x, y, 1);
        assert_eq!(line(p(1, 1), p(4, 2)), [p(1, 1), p(2, 1), p(3, 2), p(4, 2)]);
        assert_eq!(line(p(2, 2), p(2, 2)), [p(2, 2)]);
    }
}
