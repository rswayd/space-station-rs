//! What tg does to a freshly loaded map in `Initialize`, reproduced once at load from static data:
//! random spawners are replaced by their loot ([`crate::spawner`]), pipes and cables under floor
//! tiles are hidden ([`crate::undertile`]) and walls, windows, tables, carpets... pick their
//! smoothed icon state ([`crate::smooth`]). Each module also offers a per-tile recompute for
//! later changes; [`tile_changed`] runs all of them.
//!
//! The data comes from three prototype components the converter fills from `dm_types.json`
//! ([`ConvertContext::type_fields`]) and `.dmm` var edits can change ([`ext_field`]).

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::time::Duration;

use bevy::ecs::prelude::{Resource, World};
use bevy::platform::time::Instant;
use ss13_assets::manifest::AssetManifest;
use ss13_shared::{IconStateId, TilePos};

use crate::convert::DmTypes;
use crate::smooth::{self, SmoothStats};
use crate::spawner::{self, Rng, SpawnerStats};
use crate::undertile::{self, UndertileStats};
use crate::value::{Field, Skip, Val};

/// Seed of the load-time randomness (spawner loot). `SS13_MAP_SEED` fixes it (tests, bug
/// reports); otherwise every start differs, like a tg round.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapSeed(pub u64);

impl MapSeed {
    pub fn from_env() -> Self {
        let seed = std::env::var("SS13_MAP_SEED")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or_else(|| {
                web_time::SystemTime::now()
                    .duration_since(web_time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64)
            });
        Self(seed)
    }
}

/// `(dmi, state) -> id` with owned keys, kept after load for recomputes that pick new states.
// shortcut: the whole manifest index (~tens of thousands of entries, a few MB); restrict it to
// smoothing states if memory starts to matter.
#[derive(Resource, Default)]
pub struct IconIndex(pub HashMap<(String, String), IconStateId>);

impl IconIndex {
    pub fn new(manifest: &AssetManifest) -> Self {
        Self(
            manifest
                .index()
                .into_iter()
                .map(|((dmi, state), id)| ((dmi.to_owned(), state.to_owned()), id))
                .collect(),
        )
    }

    pub fn get(&self, dmi: &str, state: &str) -> Option<IconStateId> {
        // shortcut: allocates the key; called only at load and on tile changes.
        self.0.get(&(dmi.to_owned(), state.to_owned())).copied()
    }
}

#[derive(Resource, Debug, Default)]
pub struct FidelityReport {
    pub seed: u64,
    pub spawners: SpawnerStats,
    pub undertile: UndertileStats,
    pub smooth: SmoothStats,
    pub random: crate::look::RandomStats,
    /// Cables whose sprite changed, cable states missing from the manifest.
    pub cables: (usize, usize),
    pub elapsed: Duration,
}

impl fmt::Display for FidelityReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = &self.spawners;
        writeln!(
            f,
            "fidelity pass in {:.2?} (seed {})",
            self.elapsed, self.seed
        )?;
        writeln!(
            f,
            "spawners: {} resolved -> {} entities ({} nested, {} empty rolls), {} unresolved, \
             {} loot types without prototype",
            s.resolved,
            s.spawned,
            s.nested,
            s.empty,
            s.unresolved.values().sum::<usize>(),
            s.missing_loot.values().sum::<usize>(),
        )?;
        for (ty, n) in top(&s.unresolved, usize::MAX) {
            writeln!(f, "  unresolved {n} {ty}")?;
        }
        for (ty, n) in top(&s.missing_loot, 10) {
            writeln!(f, "  loot without prototype {n} {ty}")?;
        }
        let u = &self.undertile;
        writeln!(
            f,
            "undertile: {} hidden, {} visible (see-through floor), {} on plating/open",
            u.hidden, u.visible, u.exposed
        )?;
        writeln!(
            f,
            "random looks: {} picked, {} missing sprites",
            self.random.picked, self.random.missing
        )?;
        writeln!(
            f,
            "cables: {} relinked, {} missing states",
            self.cables.0, self.cables.1
        )?;
        let m = &self.smooth;
        writeln!(
            f,
            "smoothing: {} atoms ({} diagonal), {} missing states, {} unsupported (border/proc \
             filter)",
            m.smoothed.values().sum::<usize>(),
            m.diagonal,
            m.missing.values().sum::<usize>(),
            m.unsupported
        )?;
        for (family, n) in top(&m.smoothed, 25) {
            writeln!(f, "  smoothed {n} {family}")?;
        }
        for (state, n) in top(&m.missing, 10) {
            writeln!(f, "  missing {n} {state}")?;
        }
        Ok(())
    }
}

fn top(counts: &BTreeMap<String, usize>, n: usize) -> Vec<(&str, usize)> {
    let mut v: Vec<_> = counts.iter().map(|(k, &c)| (k.as_str(), c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    v.truncate(n);
    v
}

/// Runs the passes over everything the open map pass spawned (`ss13_world::Mapped`: what
/// [`crate::load_maps`] spawned since the last `run_map_init`), in tg's order: spawners first
/// (their loot can be windows and cables), then undertile, then smoothing. Inserts
/// [`IconIndex`] for later recomputes.
pub fn apply(world: &mut World, manifest: &AssetManifest, seed: u64) -> FidelityReport {
    let start = Instant::now();
    let mut rng = Rng::new(seed);
    let spawners = spawner::resolve_all(world, manifest, &mut rng);
    let undertile = undertile::update_all(world);
    let icons = IconIndex::new(manifest);
    let random = crate::look::randomize_all(world, &icons, &mut rng);
    let cables = crate::cable::connect_all(world, &icons);
    let smooth = smooth::smooth_all(world, &icons);
    world.insert_resource(icons);
    FidelityReport {
        seed,
        spawners,
        undertile,
        smooth,
        random,
        cables,
        elapsed: start.elapsed(),
    }
}

/// Call after a turf or an atom on `pos` changed (construction, destruction): re-hides what is
/// under the floor there and resmooths the tile and its 8 neighbors.
pub fn tile_changed(world: &mut World, pos: TilePos) {
    undertile::refresh_tile(world, pos);
    smooth::resmooth_around(world, pos);
}

/// `.dmm` var edits of the fidelity components' vars (see [`crate::value::field_for`]).
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    smooth::ext_field(var, value)
        .or_else(|_| undertile::ext_field(path, var, value))
        .or_else(|_| spawner::ext_field(path, var, value))
        .or_else(|_| crate::item::ext_field(path, var, value))
        .or_else(|_| crate::contents::ext_field(path, var, value))
        .or_else(|_| crate::power::ext_field(path, var, value))
        .or_else(|_| crate::atmos::ext_field(path, var, value))
        .or_else(|_| crate::atmos_machine::ext_field(path, var, value))
        .or_else(|_| crate::construct::ext_field(path, var, value))
        .or_else(|_| crate::radio::ext_field(path, var, value))
        .or_else(|_| crate::dock::ext_field(path, var, value))
        .or_else(|_| crate::disposal::ext_field(path, var, value))
        .or_else(|_| crate::engmisc::ext_field(path, var, value))
        .or_else(|_| crate::bots::ext_field(path, var, value))
        .or_else(|_| crate::service::ext_field(path, var, value))
        .or_else(|_| crate::mining::ext_field(path, var, value))
        .or_else(|_| crate::triggers::ext_field(path, var, value))
        .or_else(|_| crate::puzzle::ext_field(path, var, value))
}

pub(crate) fn ext(component: &'static str, key: &'static str, ron: impl Into<String>) -> Field {
    Field::Ext {
        component,
        key,
        ron: ron.into(),
    }
}

/// `/a/b` is under `/a` and `/a/b`, not under `/a/bc`.
pub fn is_under(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// DM truthiness of a dumped value.
pub(crate) fn truthy(v: Option<&serde_json::Value>) -> bool {
    match v {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

pub(crate) fn num(v: Option<&serde_json::Value>) -> Option<f64> {
    match v {
        None | Some(serde_json::Value::Null) => Some(0.0),
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(_) => None,
    }
}

/// Converter side: per-type data shared by the passes' field builders.
pub struct ConvertContext<'a> {
    pub types: &'a DmTypes,
    /// Direct subtypes per type, sorted.
    pub children: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> ConvertContext<'a> {
    pub fn new(types: &'a DmTypes) -> Self {
        let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
        for (path, ty) in &types.types {
            if let Some(parent) = &ty.parent {
                children.entry(parent).or_default().push(path);
            }
        }
        for list in children.values_mut() {
            list.sort_unstable();
        }
        Self { types, children }
    }

    /// DM `typesof(path)`: the type and all its subtypes, depth-first in path order.
    pub fn typesof(&self, path: &'a str) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut stack = vec![path];
        while let Some(p) = stack.pop() {
            if !self.types.types.contains_key(p) {
                continue;
            }
            out.push(p);
            if let Some(c) = self.children.get(p) {
                stack.extend(c.iter().rev());
            }
        }
        out
    }

    /// Fields of the fidelity components for one type, from what it declares.
    pub fn type_fields(&self, path: &str) -> Vec<Field> {
        let mut fields = smooth::type_fields(self.types, path);
        fields.extend(undertile::type_fields(self.types, path));
        fields.extend(spawner::type_fields(self, path));
        fields.extend(crate::item::type_fields(self.types, path));
        fields.extend(crate::vending::type_fields(self.types, path));
        fields.extend(crate::border::type_fields(self.types, path));
        fields.extend(crate::contents::type_fields(self.types, path));
        fields.extend(crate::power::type_fields(self.types, path));
        fields.extend(crate::look::type_fields(self, path));
        fields.extend(crate::cable::type_fields(self.types, path));
        fields.extend(crate::atmos::type_fields(self.types, path));
        fields.extend(crate::turf_damage::type_fields(self.types, path));
        fields.extend(crate::atmos_machine::type_fields(self.types, path));
        fields.extend(crate::clothing::type_fields(self.types, path));
        fields.extend(crate::construct::type_fields(self.types, path));
        fields.extend(crate::radio::type_fields(self.types, path));
        fields.extend(crate::silicon::type_fields(self.types, path));
        fields.extend(crate::mob_spawn::type_fields(self.types, path));
        fields.extend(crate::security::type_fields(self.types, path));
        fields.extend(crate::melee::type_fields(self.types, path));
        fields.extend(crate::guns::type_fields(self.types, path));
        fields.extend(crate::medical::type_fields(self.types, path));
        fields.extend(crate::dock::type_fields(self.types, path));
        fields.extend(crate::reagents::type_fields(self.types, path));
        fields.extend(crate::food::type_fields(self.types, path));
        fields.extend(crate::dispenser::type_fields(self.types, path));
        fields.extend(crate::smartfridge::type_fields(self.types, path));
        fields.extend(crate::drinks::type_fields(self.types, path));
        fields.extend(crate::barsign::type_fields(self.types, path));
        fields.extend(crate::service::type_fields(self.types, path));
        fields.extend(crate::fishing::type_fields(self.types, path));
        fields.extend(crate::disposal::type_fields(self.types, path));
        fields.extend(crate::engmisc::type_fields(self.types, path));
        fields.extend(crate::bots::type_fields(self.types, path));
        fields.extend(crate::mobs::type_fields(self.types, path));
        fields.extend(crate::wallmount::type_fields(self.types, path));
        fields.extend(crate::airlock_control::type_fields(self.types, path));
        fields.extend(crate::circuit::type_fields(self.types, path));
        fields.extend(crate::mining::type_fields(self.types, path));
        fields.extend(crate::triggers::type_fields(self.types, path));
        fields.extend(crate::puzzle::type_fields(self.types, path));
        // Last: GAGS replaces the icon vars read above.
        fields.extend(crate::gags::type_fields(self.types, path));
        fields
    }
}
