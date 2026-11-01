//! tg icon smoothing (`code/__HELPERS/icon_smoothing.dm`, `bitmask_smooth`): an atom with
//! `SMOOTH_BITMASK` looks at its neighbors, sets a junction bit per direction where the neighbor's
//! `smoothing_groups` meet its `canSmoothWith`, and shows `"<base_icon_state>-<junction>"`.
//!
//! - Cardinals first; a diagonal is checked only when both of its cardinals connect, and not at
//!   all with `SMOOTH_BITMASK_CARDINALS`.
//! - The neighbor turf always counts; movables on it count only when `canSmoothWith` has an
//!   object group (negative number, tg's `SMOOTH_OBJ`) and they are anchored.
//! - `SMOOTH_BORDER`: the map edge connects.
//! - `/turf/closed` with `SMOOTH_DIAGONAL_CORNERS` uses `"-diagonal"` states for the 8 corner
//!   junctions.

use std::collections::BTreeMap;

use bevy::prelude::*;
use ss13_shared::{Appearance, Layer, Overlay, TilePos};
use ss13_world::SpatialIndex;

use crate::convert::DmTypes;
use crate::fidelity::{IconIndex, ext, is_under, num};
use crate::load::fold_color;
use crate::value::{Field, Skip, Val};
use crate::{DmAtom, DmLook, Turf};

pub const SMOOTH_BITMASK: u8 = 1 << 0;
pub const SMOOTH_BITMASK_CARDINALS: u8 = 1 << 1;
pub const SMOOTH_DIAGONAL_CORNERS: u8 = 1 << 2;
pub const SMOOTH_BORDER: u8 = 1 << 3;
pub const SMOOTH_BORDER_OBJECT: u8 = 1 << 6;
pub const SMOOTH_PROC_FILTER: u8 = 1 << 7;
const USES_SMOOTHING: u8 = SMOOTH_BITMASK | SMOOTH_BITMASK_CARDINALS;

pub const NORTH_JUNCTION: u8 = 1 << 0;
pub const SOUTH_JUNCTION: u8 = 1 << 1;
pub const EAST_JUNCTION: u8 = 1 << 2;
pub const WEST_JUNCTION: u8 = 1 << 3;
pub const NORTHEAST_JUNCTION: u8 = 1 << 4;
pub const SOUTHEAST_JUNCTION: u8 = 1 << 5;
pub const SOUTHWEST_JUNCTION: u8 = 1 << 6;
pub const NORTHWEST_JUNCTION: u8 = 1 << 7;

/// The smoothing vars of a type (prototype data, like [`DmAtom`]).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmSmooth {
    /// `smoothing_flags`.
    pub flags: u8,
    /// `smoothing_groups`: tg's group numbers, sorted; `S_OBJ` groups are negative.
    pub groups: Vec<i16>,
    /// `canSmoothWith`, same encoding.
    pub can_smooth_with: Vec<i16>,
    pub base_icon_state: String,
    /// Movables only smooth with anchored neighbors.
    pub anchored: bool,
    /// Under `/turf/closed`: only those draw diagonal corner states.
    pub closed_turf: bool,
    /// `.dmi` of the smoothed states when it is not the atom's `icon` (closed false walls draw
    /// from `fake_icon`, `/obj/structure/falsewall/update_icon_state`); empty = `icon`.
    pub icon: String,
}

impl DmSmooth {
    pub fn smooths(&self) -> bool {
        self.flags & USES_SMOOTHING != 0
    }

    fn smooth_obj(&self) -> bool {
        self.can_smooth_with.first().is_some_and(|g| *g < 0)
    }

    /// The icon state for `junction`.
    pub fn state(&self, junction: u8) -> String {
        let base = &self.base_icon_state;
        if self.closed_turf && self.flags & SMOOTH_DIAGONAL_CORNERS != 0 && is_corner(junction) {
            format!("{base}-{junction}-diagonal")
        } else {
            format!("{base}-{junction}")
        }
    }
}

fn is_corner(j: u8) -> bool {
    const N: u8 = NORTH_JUNCTION;
    const S: u8 = SOUTH_JUNCTION;
    const E: u8 = EAST_JUNCTION;
    const W: u8 = WEST_JUNCTION;
    [
        N | W,
        N | E,
        S | W,
        S | E,
        N | W | NORTHWEST_JUNCTION,
        N | E | NORTHEAST_JUNCTION,
        S | W | SOUTHWEST_JUNCTION,
        S | E | SOUTHEAST_JUNCTION,
    ]
    .contains(&j)
}

fn intersects(a: &[i16], b: &[i16]) -> bool {
    a.iter().any(|g| b.contains(g))
}

/// tg `bitmask_smooth`. `connects(dx, dy)` says whether the neighbor at that offset smooths with
/// this atom, `None` when it is off the map.
pub fn junction(flags: u8, mut connects: impl FnMut(i32, i32) -> Option<bool>) -> u8 {
    let mut j = 0;
    let mut check = |dx, dy, bit| match connects(dx, dy) {
        Some(true) => j |= bit,
        Some(false) => {}
        None if flags & SMOOTH_BORDER != 0 => j |= bit,
        None => {}
    };
    check(0, 1, NORTH_JUNCTION);
    check(0, -1, SOUTH_JUNCTION);
    check(1, 0, EAST_JUNCTION);
    check(-1, 0, WEST_JUNCTION);
    let j0 = j;
    if flags & SMOOTH_BITMASK_CARDINALS != 0
        || j0 & (NORTH_JUNCTION | SOUTH_JUNCTION) == 0
        || j0 & (EAST_JUNCTION | WEST_JUNCTION) == 0
    {
        return j0;
    }
    let mut j = j0;
    let mut check = |dx, dy, bit| match connects(dx, dy) {
        Some(true) => j |= bit,
        Some(false) => {}
        None if flags & SMOOTH_BORDER != 0 => j |= bit,
        None => {}
    };
    if j0 & NORTH_JUNCTION != 0 {
        if j0 & WEST_JUNCTION != 0 {
            check(-1, 1, NORTHWEST_JUNCTION);
        }
        if j0 & EAST_JUNCTION != 0 {
            check(1, 1, NORTHEAST_JUNCTION);
        }
    }
    if j0 & SOUTH_JUNCTION != 0 {
        if j0 & WEST_JUNCTION != 0 {
            check(-1, -1, SOUTHWEST_JUNCTION);
        }
        if j0 & EAST_JUNCTION != 0 {
            check(1, -1, SOUTHEAST_JUNCTION);
        }
    }
    j
}

/// Whether anything on `pos` smooths with `me` (tg `SEARCH_ADJ_IN_DIR` for one tile).
// shortcut: `area_limited_icon_smoothing` (shuttle areas) is ignored; no station map sets it,
// add it with shuttle loading.
fn tile_connects(world: &World, index: &SpatialIndex, pos: TilePos, me: &DmSmooth) -> Option<bool> {
    if !index.in_bounds(pos) {
        return None;
    }
    let smooth_obj = me.smooth_obj();
    Some(index.on_tile(pos).iter().any(|&e| {
        let Some(other) = world.get::<DmSmooth>(e) else {
            return false;
        };
        let counts = world.get::<Turf>(e).is_some() || (smooth_obj && other.anchored);
        counts && intersects(&me.can_smooth_with, &other.groups)
    }))
}

#[derive(Debug, Default)]
pub struct SmoothStats {
    /// Smoothed atoms by `base_icon_state` (the smoothing family: wall, table, carpet...).
    pub smoothed: BTreeMap<String, usize>,
    /// Of those, drawn with a `-diagonal` state.
    pub diagonal: usize,
    /// `icon:state` the junction asked for that the `.dmi` lacks; those keep their old state.
    pub missing: BTreeMap<String, usize>,
    /// `SMOOTH_BORDER_OBJECT` / `SMOOTH_PROC_FILTER` atoms: they need a proc, left as they are.
    pub unsupported: usize,
}

/// Smooths every smoothing atom on a tile.
pub fn smooth_all(world: &mut World, icons: &IconIndex) -> SmoothStats {
    let mut query = world.query::<(Entity, &TilePos, &DmSmooth)>();
    let targets: Vec<(Entity, TilePos)> = query
        .iter(world)
        .filter(|(_, _, s)| s.smooths())
        .map(|(e, p, _)| (e, *p))
        .collect();
    let mut stats = SmoothStats::default();
    smooth_entities(world, icons, &targets, &mut stats);
    stats
}

/// Resmooths the atoms on `pos` and its 8 neighbors (their junctions depend on `pos`).
/// Needs [`IconIndex`] (inserted by [`crate::fidelity::apply`]).
pub fn resmooth_around(world: &mut World, pos: TilePos) {
    let index = world.resource::<SpatialIndex>();
    let targets: Vec<(Entity, TilePos)> = index
        .in_radius(pos, 1)
        .filter(|&e| world.get::<DmSmooth>(e).is_some_and(DmSmooth::smooths))
        .filter_map(|e| Some((e, *world.get::<TilePos>(e)?)))
        .collect();
    world.resource_scope(|world, icons: Mut<IconIndex>| {
        smooth_entities(world, &icons, &targets, &mut SmoothStats::default());
    });
}

fn smooth_entities(
    world: &mut World,
    icons: &IconIndex,
    targets: &[(Entity, TilePos)],
    stats: &mut SmoothStats,
) {
    let mut updates = Vec::with_capacity(targets.len());
    {
        let world: &World = world;
        let index = world.resource::<SpatialIndex>();
        for &(e, pos) in targets {
            let (Some(me), Some(atom)) = (world.get::<DmSmooth>(e), world.get::<DmAtom>(e)) else {
                continue;
            };
            if me.flags & (SMOOTH_BORDER_OBJECT | SMOOTH_PROC_FILTER) != 0 {
                stats.unsupported += 1;
                continue;
            }
            let j = junction(me.flags, |dx, dy| {
                tile_connects(
                    world,
                    index,
                    TilePos::new(pos.x + dx, pos.y + dy, pos.z),
                    me,
                )
            });
            let state = me.state(j);
            let dmi = if me.icon.is_empty() {
                &atom.icon
            } else {
                &me.icon
            };
            // BandaStation window edging follows the junction.
            let edge = world
                .get::<DmLook>(e)
                .filter(|l| !l.edge_icon.is_empty())
                .and_then(|l| {
                    let id = icons.get(&l.edge_icon, &format!("edge-{j}"))?;
                    Some(Overlay {
                        color: Some(l.edge_color.unwrap_or([255; 4])),
                        ..Overlay::new(id)
                    })
                });
            match icons.get(dmi, &state) {
                Some(id) => {
                    *stats
                        .smoothed
                        .entry(me.base_icon_state.clone())
                        .or_default() += 1;
                    stats.diagonal += usize::from(state.ends_with("-diagonal"));
                    updates.push((e, id, edge));
                }
                None => *stats.missing.entry(format!("{dmi}:{state}")).or_default() += 1,
            }
        }
    }
    for (e, id, edge) in updates {
        let mut entity = world.entity_mut(e);
        let overlays: Vec<Overlay> = edge.into_iter().collect();
        if let Some(mut appearance) = entity.get_mut::<Appearance>() {
            if appearance.icon_state != id {
                appearance.icon_state = id;
            }
            if !overlays.is_empty() && appearance.overlays != overlays {
                appearance.overlays = overlays;
            }
            continue;
        }
        // The unsmoothed state did not exist: the entity was spawned undrawn.
        let atom = entity.get::<DmAtom>().expect("checked above").clone();
        entity.insert(Appearance {
            icon_state: id,
            layer: Layer(atom.layer),
            plane: atom.plane,
            color: fold_color(atom.color, atom.alpha),
            pixel_offset: [atom.pixel_x, atom.pixel_y],
            rotation: 0,
            mouse_opacity: ss13_shared::MouseOpacity::from_dm(atom.mouse_opacity),
            overlays,
        });
    }
}

const COMPONENT: &str = "dm_smooth";
const VARS: [&str; 4] = [
    "smoothing_flags",
    "smoothing_groups",
    "canSmoothWith",
    "base_icon_state",
];

/// Group numbers of a smoothing var: tg writes them as `"-1,58,"` strings (`S_OBJ`/`S_TURF`),
/// concatenated with `+`, which the extractor dumps as a string or a list of strings.
pub fn parse_groups(v: &serde_json::Value) -> Option<Vec<i16>> {
    let mut text = String::new();
    match v {
        serde_json::Value::Null => {}
        serde_json::Value::String(s) => text.push_str(s),
        serde_json::Value::Object(o) => {
            for item in o.get("list")?.as_array()? {
                text.push_str(item.as_str()?);
            }
        }
        _ => return None,
    }
    parse_group_text(&text)
}

fn parse_group_text(text: &str) -> Option<Vec<i16>> {
    let mut out: Vec<i16> = text
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().ok())
        .collect::<Option<_>>()?;
    out.sort_unstable();
    out.dedup();
    Some(out)
}

fn groups_ron(groups: &[i16]) -> String {
    format!("{groups:?}")
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

/// Converter: smoothing fields of `path` from its declarations. `anchored` and
/// `base_icon_state` are emitted only for smoothing types (they are declared on most atoms).
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(ty) = types.types.get(path) else {
        return out;
    };
    let declared = |var: &str| ty.declared_vars.get(var);
    if path == "/turf/closed" {
        out.push(ext(COMPONENT, "closed_turf", "true"));
    }
    // shortcut: false walls are mapped closed; an opened one (`density = 0`) would draw `-open`.
    if is_under(path, "/obj/structure/falsewall")
        && let Some(icon) = ty
            .declared_vars
            .get("fake_icon")
            .and_then(|v| v.get("resource"))
            .and_then(|v| v.as_str())
    {
        out.push(ext(COMPONENT, "icon", str_ron(icon)));
    }
    if let Some(n) = num(declared("smoothing_flags")) {
        out.push(ext(COMPONENT, "flags", format!("{}", n as i64 as u8)));
    }
    for (var, key) in [
        ("smoothing_groups", "groups"),
        ("canSmoothWith", "can_smooth_with"),
    ] {
        if let Some(groups) = declared(var).and_then(parse_groups) {
            out.push(ext(COMPONENT, key, groups_ron(&groups)));
        }
    }
    let smooths = num(types.resolved(path, "smoothing_flags"))
        .is_some_and(|f| f as i64 as u8 & USES_SMOOTHING != 0);
    let has_groups = types
        .resolved(path, "smoothing_groups")
        .and_then(parse_groups)
        .is_some_and(|g| !g.is_empty());
    let declares_smoothing = VARS.iter().any(|v| declared(v).is_some());
    if smooths && (declares_smoothing || declared("base_icon_state").is_some()) {
        let base = types.resolved(path, "base_icon_state");
        out.push(ext(
            COMPONENT,
            "base_icon_state",
            str_ron(base.and_then(|b| b.as_str()).unwrap_or("")),
        ));
    }
    if has_groups
        && !is_under(path, "/turf")
        && (declares_smoothing || declared("anchored").is_some())
    {
        let anchored = crate::fidelity::truthy(types.resolved(path, "anchored"));
        out.push(ext(COMPONENT, "anchored", anchored.to_string()));
    }
    out
}

/// `.dmm` var edits.
pub fn ext_field(var: &str, value: Val) -> Result<Field, Skip> {
    let groups = |v: Val| match v {
        Val::Null => Ok(Vec::new()),
        Val::Str(s) => parse_group_text(s).ok_or(Skip::Unsupported),
        _ => Err(Skip::Unsupported),
    };
    Ok(match var {
        "smoothing_flags" => match value {
            Val::Num(n) => ext(COMPONENT, "flags", format!("{}", n as i64 as u8)),
            Val::Null => ext(COMPONENT, "flags", "0"),
            _ => return Err(Skip::Unsupported),
        },
        "smoothing_groups" => ext(COMPONENT, "groups", groups_ron(&groups(value)?)),
        "canSmoothWith" => ext(COMPONENT, "can_smooth_with", groups_ron(&groups(value)?)),
        "base_icon_state" => match value {
            Val::Str(s) => ext(COMPONENT, "base_icon_state", str_ron(s)),
            Val::Null => ext(COMPONENT, "base_icon_state", str_ron("")),
            _ => return Err(Skip::Unsupported),
        },
        "anchored" => match value {
            Val::Num(n) => ext(COMPONENT, "anchored", (n != 0.0).to_string()),
            _ => return Err(Skip::Unsupported),
        },
        _ => return Err(Skip::UnknownVar),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `#` = connects, `.` = does not, outside the grid = off the map. The atom is at `(cx, cy)`
    /// of a grid whose first row is the northmost.
    fn junction_in(flags: u8, grid: &[&str]) -> u8 {
        let h = grid.len() as i32;
        let (cx, cy) = (1, 1);
        junction(flags, |dx, dy| {
            let (x, y) = (cx + dx, cy - dy);
            if x < 0 || y < 0 || y >= h || x >= grid[0].len() as i32 {
                return None;
            }
            Some(grid[y as usize].as_bytes()[x as usize] == b'#')
        })
    }

    #[test]
    fn cardinals_and_diagonals() {
        // A lone atom.
        assert_eq!(junction_in(SMOOTH_BITMASK, &["...", ".#.", "..."]), 0);
        // A horizontal run: east + west, no diagonals without a vertical neighbor.
        assert_eq!(
            junction_in(SMOOTH_BITMASK, &["#.#", "###", "#.#"]),
            EAST_JUNCTION | WEST_JUNCTION
        );
        // A filled block: all 8 directions -> 255.
        assert_eq!(junction_in(SMOOTH_BITMASK, &["###", "###", "###"]), 255);
        // An inner corner: N + E connect, so NE is checked; NW/SE are not even looked at.
        assert_eq!(
            junction_in(SMOOTH_BITMASK, &["###", ".##", "..."]),
            NORTH_JUNCTION | EAST_JUNCTION | NORTHEAST_JUNCTION
        );
        // Cross without corners.
        assert_eq!(
            junction_in(SMOOTH_BITMASK, &[".#.", "###", ".#."]),
            NORTH_JUNCTION | SOUTH_JUNCTION | EAST_JUNCTION | WEST_JUNCTION
        );
        // Cardinals only: a filled block stays at 15.
        assert_eq!(
            junction_in(
                SMOOTH_BITMASK | SMOOTH_BITMASK_CARDINALS,
                &["###", "###", "###"]
            ),
            15
        );
    }

    #[test]
    fn map_border() {
        // Atom in the top-left corner of the map: north and west are off the map.
        let at_corner = |flags| {
            junction(flags, |dx, dy| {
                (dx >= 0 && dy <= 0).then_some(dx == 1 && dy == 0 || dx == 0 && dy == -1)
            })
        };
        assert_eq!(at_corner(SMOOTH_BITMASK), SOUTH_JUNCTION | EAST_JUNCTION);
        assert_eq!(
            at_corner(SMOOTH_BITMASK | SMOOTH_BORDER),
            !SOUTHEAST_JUNCTION
        );
    }

    #[test]
    fn states_and_groups() {
        let mut wall = DmSmooth {
            flags: SMOOTH_BITMASK | SMOOTH_DIAGONAL_CORNERS,
            base_icon_state: "plastitanium_wall".into(),
            ..default()
        };
        assert_eq!(
            wall.state(NORTH_JUNCTION | EAST_JUNCTION),
            "plastitanium_wall-5"
        );
        wall.closed_turf = true;
        assert_eq!(
            wall.state(NORTH_JUNCTION | EAST_JUNCTION),
            "plastitanium_wall-5-diagonal"
        );
        assert_eq!(wall.state(15), "plastitanium_wall-15");
        assert_eq!(
            parse_groups(&serde_json::json!({ "list": ["-1,", "58,"] })),
            Some(vec![-1, 58])
        );
        assert_eq!(parse_groups(&serde_json::json!("0,10,")), Some(vec![0, 10]));
        assert_eq!(parse_groups(&serde_json::json!(null)), Some(vec![]));
        assert_eq!(parse_groups(&serde_json::json!("x,")), None);
    }
}
