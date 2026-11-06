//! tg `/datum/element/undertile` (`code/datums/elements/undertile.dm`): pipes, cables, disposal
//! pipes and a few others become invisible on turfs whose `underfloor_accessibility` is
//! `UNDERFLOOR_HIDDEN` (floor tiles, walls), move to the floor plane on `UNDERFLOOR_VISIBLE`
//! turfs (glass floors, catwalks) and stay as they are on plating and open ground.
//!
//! Which types get the element is decided in procs (`AddElement` in `Initialize` and friends),
//! not in vars. `ss13-dm-extract` dumps every `AddElement(/datum/element/undertile, ...)` with its
//! owner type and proc; [`RULES`] is that list read by a person, with the static var that gates
//! the call where there is one. [`check_rules`] fails the conversion when the dump and [`RULES`]
//! disagree, so an upstream change is reviewed instead of silently drawing pipes on floors.

use bevy::prelude::*;
use ss13_shared::{Appearance, Layer, TilePos};
use ss13_world::SpatialIndex;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::{Field, Skip, Val};
use crate::{DmAtom, Turf};

pub const ELEMENT: &str = "/datum/element/undertile";

/// `(owner type, proc, gating var)`: every `AddElement(/datum/element/undertile)` in the
/// reference. The element applies to the owner and all its subtypes, when the gating var is
/// true for them.
/// - `/obj/machinery/atmospherics/pipe/setup_hiding` runs from atmos `Initialize` under
///   `if(hide)`.
/// - pressure plates add it under `if(undertile_pressureplate)`.
pub const RULES: [(&str, &str, Option<&str>); 15] = [
    ("/obj/effect/weakpoint", "Initialize", None),
    (
        "/obj/item/pressure_plate",
        "Initialize",
        Some("undertile_pressureplate"),
    ),
    (
        "/obj/item/pressure_plate/hologrid",
        "Initialize",
        Some("undertile_pressureplate"),
    ),
    (
        "/obj/item/storage/backpack/satchel/flat",
        "Initialize",
        None,
    ),
    ("/obj/item/storage/box/flat", "Initialize", None),
    ("/obj/item/undertile_circuit", "Initialize", None),
    (
        "/obj/machinery/atmospherics/pipe",
        "setup_hiding",
        Some("hide"),
    ),
    ("/obj/machinery/bluespace_beacon", "Initialize", None),
    ("/obj/machinery/duct", "Initialize", None),
    ("/obj/machinery/navbeacon", "Initialize", None),
    ("/obj/machinery/power/terminal", "Initialize", None),
    ("/obj/structure/cable", "Initialize", None),
    ("/obj/structure/disposalconstruct", "Initialize", None),
    ("/obj/structure/disposalpipe", "Initialize", None),
    ("/obj/structure/safe/floor", "Initialize", None),
];

/// `code/__DEFINES/turfs.dm`.
pub const UNDERFLOOR_HIDDEN: u8 = 0;
pub const UNDERFLOOR_VISIBLE: u8 = 1;
/// `code/__DEFINES/layers.dm`: `FLOOR_PLANE`, `BELOW_CATWALK_LAYER` (`11 + TOPDOWN_LAYER`).
pub const FLOOR_PLANE: i16 = -6;
pub const BELOW_CATWALK_LAYER: f32 = 10011.0;

/// `GLOB.turfs_without_ground` (`code/__DEFINES/is_helpers.dm`).
pub const GROUNDLESS_TURFS: [&str; 5] = [
    "/turf/open/space",
    "/turf/open/chasm",
    "/turf/open/lava",
    "/turf/open/water",
    "/turf/open/openspace",
];

/// Turf vars the passes need (prototype data).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmTurf {
    /// `UNDERFLOOR_HIDDEN` / `_VISIBLE` / `_INTERACTABLE`.
    pub underfloor_accessibility: u8,
    /// tg `isgroundlessturf`.
    pub groundless: bool,
    /// tg `isclosedturf`.
    pub closed: bool,
    /// tg `footstep`, `barefootstep`, `clawfootstep`, `heavyfootstep`: keys of the footstep
    /// tables (`crate::sound::Footsteps`); empty for `null` (no step sound, e.g. open space).
    pub footstep: String,
    pub barefootstep: String,
    pub clawfootstep: String,
    pub heavyfootstep: String,
}

/// [`DmTurf`]'s footstep vars.
const FOOTSTEP_VARS: [&str; 4] = ["footstep", "barefootstep", "clawfootstep", "heavyfootstep"];

/// The type gets the undertile element (prototype data; `enabled` follows the gating var).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmUndertile {
    pub enabled: bool,
}

/// Under a floor tile: players cannot see it, so it is not replicated. Added and removed only by
/// this module; the server maps it to replication (`ss13_game`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct HiddenUnderTile;

/// Fails when the extractor found undertile calls that [`RULES`] does not describe (or the
/// reverse), or any `RemoveElement` (subtypes opting out, which [`RULES`] cannot express).
pub fn check_rules(types: &DmTypes) -> anyhow::Result<()> {
    let calls = types
        .element_calls
        .get(ELEMENT)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut problems = Vec::new();
    for c in calls {
        if c.call != "AddElement" {
            problems.push(format!("{} {}/{}: {}", c.call, c.owner, c.proc, c.line));
        } else if !RULES
            .iter()
            .any(|(owner, proc, _)| *owner == c.owner && *proc == c.proc)
        {
            problems.push(format!("new call {}/{}: {}", c.owner, c.proc, c.line));
        }
    }
    for (owner, proc, _) in RULES {
        // Hand-written fixtures hold a few types only.
        let known = types.types.contains_key(owner);
        if known && !calls.iter().any(|c| c.owner == owner && c.proc == proc) {
            problems.push(format!("{owner}/{proc} no longer adds the element"));
        }
    }
    if !problems.is_empty() {
        anyhow::bail!(
            "undertile calls in dm_types.json differ from ss13_mapload::undertile::RULES \
             (review the DM code and update RULES):\n  {}",
            problems.join("\n  ")
        );
    }
    Ok(())
}

const TURF: &str = "dm_turf";
const UNDERTILE: &str = "dm_undertile";

/// Converter fields of `path`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(ty) = types.types.get(path) else {
        return out;
    };
    if is_under(path, "/turf")
        && let Some(v) = ty.declared_vars.get("underfloor_accessibility")
        && let Some(n) = num(Some(v))
    {
        out.push(ext(TURF, "underfloor_accessibility", (n as u8).to_string()));
    }
    if is_under(path, "/turf") {
        for var in FOOTSTEP_VARS {
            if let Some(v) = ty.declared_vars.get(var) {
                let key = v.as_str().unwrap_or_default();
                out.push(ext(
                    TURF,
                    var,
                    ron::to_string(key).expect("strings serialize"),
                ));
            }
        }
    }
    if path == "/turf/closed" {
        out.push(ext(TURF, "closed", "true"));
    }
    if GROUNDLESS_TURFS.contains(&path) {
        out.push(ext(TURF, "groundless", "true"));
    }
    for (owner, _, gate) in RULES {
        match gate {
            None if path == owner => out.push(ext(UNDERTILE, "enabled", "true")),
            Some(var)
                if is_under(path, owner)
                    && (path == owner || ty.declared_vars.contains_key(var)) =>
            {
                let on = truthy(types.resolved(path, var));
                out.push(ext(UNDERTILE, "enabled", on.to_string()));
            }
            _ => {}
        }
    }
    out
}

/// `.dmm` var edits: a turf's `underfloor_accessibility`, a gating var (`hide = 1` on a pipe).
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let number = |v: Val| match v {
        Val::Num(n) => Ok(n),
        Val::Null => Ok(0.0),
        _ => Err(Skip::Unsupported),
    };
    if var == "underfloor_accessibility" && is_under(path, "/turf") {
        return Ok(ext(
            TURF,
            "underfloor_accessibility",
            (number(value)? as u8).to_string(),
        ));
    }
    if RULES
        .iter()
        .any(|(owner, _, gate)| *gate == Some(var) && is_under(path, owner))
    {
        return Ok(ext(
            UNDERTILE,
            "enabled",
            (number(value)? != 0.0).to_string(),
        ));
    }
    Err(Skip::UnknownVar)
}

#[derive(Debug, Default)]
pub struct UndertileStats {
    pub hidden: usize,
    /// On `UNDERFLOOR_VISIBLE` turfs: drawn on the floor plane.
    pub visible: usize,
    /// On plating / open turfs: drawn as is.
    pub exposed: usize,
}

/// Applies the element to every undertile atom on a tile.
pub fn update_all(world: &mut World) -> UndertileStats {
    let mut query =
        world.query_filtered::<(Entity, &TilePos, &DmUndertile), With<ss13_world::Mapped>>();
    let targets: Vec<(Entity, TilePos)> = query
        .iter(world)
        .filter(|(_, _, u)| u.enabled)
        .map(|(e, p, _)| (e, *p))
        .collect();
    let mut stats = UndertileStats::default();
    for (e, pos) in targets {
        update(world, e, pos, &mut stats);
    }
    stats
}

/// tg `levelupdate` for one tile: call after its turf changed.
pub fn refresh_tile(world: &mut World, pos: TilePos) {
    let targets: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<DmUndertile>(e).is_some_and(|u| u.enabled))
        .collect();
    let mut stats = UndertileStats::default();
    for e in targets {
        update(world, e, pos, &mut stats);
    }
}

/// `underfloor_accessibility` of the turf on `pos`; no turf behaves like open ground.
pub fn accessibility(world: &World, pos: TilePos) -> u8 {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .find(|&&e| world.get::<Turf>(e).is_some())
        .map_or(u8::MAX, |&e| {
            world
                .get::<DmTurf>(e)
                .map_or(UNDERFLOOR_HIDDEN, |t| t.underfloor_accessibility)
        })
}

fn update(world: &mut World, e: Entity, pos: TilePos, stats: &mut UndertileStats) {
    let access = accessibility(world, pos);
    let Some(atom) = world.get::<DmAtom>(e).cloned() else {
        return;
    };
    let mut entity = world.entity_mut(e);
    if access < UNDERFLOOR_VISIBLE {
        stats.hidden += 1;
        if !entity.contains::<HiddenUnderTile>() {
            entity.insert(HiddenUnderTile);
        }
        return;
    }
    if entity.contains::<HiddenUnderTile>() {
        entity.remove::<HiddenUnderTile>();
    }
    let see_through = access == UNDERFLOOR_VISIBLE;
    if see_through {
        stats.visible += 1;
    } else {
        stats.exposed += 1;
    }
    let (plane, layer) = if see_through && atom.plane != FLOOR_PLANE {
        (FLOOR_PLANE, BELOW_CATWALK_LAYER)
    } else {
        (atom.plane, atom.layer)
    };
    if let Some(mut appearance) = entity.get_mut::<Appearance>()
        && (appearance.plane != plane || appearance.layer != Layer(layer))
    {
        appearance.plane = plane;
        appearance.layer = Layer(layer);
    }
}
