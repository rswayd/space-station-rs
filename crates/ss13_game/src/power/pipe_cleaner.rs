//! Pipe cleaners (server): tg `/obj/structure/pipe_cleaner` and `/obj/item/stack/pipe_cleaner_coil`
//! (`code/modules/power/pipecleaners.dm`) — the old cable code without a powernet, for wire art.
//!
//! - A coil on plating lays a stub from the tile's centre (`0-<dir>`): towards the user, or on
//!   the user's own tile the way it faces. A coil on a stub (or on a tile with one) next to the
//!   user turns it into a full piece towards the user (`<d1>-<d2>`, smaller first); on a piece
//!   that points at the user, a stub pointing back goes onto the user's tile. One length each.
//! - Wirecutters pull a piece up: it drops as a coil of 1 (stub) or 2 lengths of its colour.
//! - Pieces take the coil's colour.
// shortcut: no alt-click pull-up, no cyborg colour radial, random coils keep their type's colour
// (tg picks one of `GLOB.cable_colors`), a dropped coil merges with any coil on the tile whatever
// its colour, no singularity pull or blueprints.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_world::SpatialIndex;

use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::tools::{Stack, ToolKind, tell, tool_of, use_stack};

pub const PIPE_CLEANER: &str = "/obj/structure/pipe_cleaner";
pub const COIL: &str = "/obj/item/stack/pipe_cleaner_coil";

/// A laid piece: `d1` 0 (stub from the centre) or the smaller end, `d2` the other end (BYOND
/// dir bits).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipeCleaner {
    pub d1: u8,
    pub d2: u8,
    pub color: Option<[u8; 4]>,
}

impl PipeCleaner {
    /// Lengths in it (`stored`): 2 for a full piece, 1 for a stub.
    pub fn lengths(&self) -> u32 {
        if self.d1 != 0 { 2 } else { 1 }
    }

    pub fn has_end(&self, dir: u8) -> bool {
        self.d1 == dir || self.d2 == dir
    }
}

/// `pipe_cleaner_join`'s stub-to-piece directions, smaller first.
pub fn joined(stub_dir: u8, to: u8) -> (u8, u8) {
    (stub_dir.min(to), stub_dir.max(to))
}

pub struct PipeCleanerPlugin;

impl Plugin for PipeCleanerPlugin {
    fn build(&self, app: &mut App) {
        // A coil on a turf or a piece, wirecutters on a piece.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2891, held_on_target);
        app.add_systems(Update, looks);
    }
}

fn is_coil(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, COIL))
}

fn pieces_on(world: &World, pos: TilePos) -> Vec<(Entity, PipeCleaner)> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter_map(|&e| world.get::<PipeCleaner>(e).map(|p| (e, *p)))
        .collect()
}

fn held_on_target(world: &mut World, c: &ClickCtx) -> bool {
    let Some(held) = c.held else {
        return false;
    };
    let piece = world.get::<PipeCleaner>(c.target).copied();
    if is_coil(world, held) {
        if piece.is_some() {
            join(world, c, held, c.target);
            return true;
        }
        if world.get::<Turf>(c.target).is_some() {
            // `floor/attackby`: a stub on the tile is joined, else a new stub.
            match pieces_on(world, c.pos).into_iter().find(|(_, p)| p.d1 == 0) {
                Some((stub, _)) => join(world, c, held, stub),
                None => {
                    place_turf(world, c, held, c.pos);
                }
            }
            return true;
        }
        return false;
    }
    if piece.is_some() && tool_of(world, held).map(|t| t.0) == Some(ToolKind::Wirecutters) {
        cut(world, c.client, c.target);
        return true;
    }
    false
}

fn coil_color(world: &World, coil: Entity) -> Option<[u8; 4]> {
    world.get::<Appearance>(coil).and_then(|a| a.color)
}

/// A new piece `d1-d2` of the coil's colour at `pos`.
fn new_piece(world: &mut World, coil: Entity, pos: TilePos, d1: u8, d2: u8) -> Option<Entity> {
    let color = coil_color(world, coil);
    let e = crate::construction::spawn_obj(world, PIPE_CLEANER, pos)?;
    world.entity_mut(e).insert(PipeCleaner { d1, d2, color });
    Some(e)
}

/// `place_turf`: a stub at `pos` towards the user, or the way the user faces on its own tile.
fn place_turf(world: &mut World, c: &ClickCtx, coil: Entity, pos: TilePos) -> Option<Entity> {
    if !crate::devices::rhd::can_lay_cable(world, pos) {
        tell(
            world,
            c.client,
            Span::Warning,
            "Трубочисты можно укладывать только на твёрдый пол!",
        );
        return None;
    }
    if crate::tools::stack_amount(world, coil) < 1 {
        tell(world, c.client, Span::Warning, "Трубочисты закончились!");
        return None;
    }
    if c.mob_pos.chebyshev(pos).is_none_or(|d| d > 1) {
        tell(
            world,
            c.client,
            Span::Warning,
            "Нельзя проложить трубочист так далеко!",
        );
        return None;
    }
    let dirn = if c.mob_pos == pos {
        world.get::<Facing>(c.mob).map(|f| f.0 as u8)
    } else {
        crate::pull::dir_between(pos, c.mob_pos).map(|d| d as u8)
    }?;
    if pieces_on(world, pos)
        .iter()
        .any(|(_, p)| p.d1 == 0 && p.d2 == dirn)
    {
        tell(world, c.client, Span::Warning, "Здесь уже есть трубочист!");
        return None;
    }
    let piece = new_piece(world, coil, pos, 0, dirn)?;
    use_stack(world, coil, 1);
    Some(piece)
}

/// `pipe_cleaner_join` on `piece` from the user's tile.
fn join(world: &mut World, c: &ClickCtx, coil: Entity, piece: Entity) {
    let (Some(&at), Some(p)) = (
        world.get::<TilePos>(piece),
        world.get::<PipeCleaner>(piece).copied(),
    ) else {
        return;
    };
    let Some(distance) = c.mob_pos.chebyshev(at).filter(|&d| d <= 1) else {
        tell(
            world,
            c.client,
            Span::Warning,
            "Нельзя проложить трубочист так далеко!",
        );
        return;
    };
    if crate::tools::stack_amount(world, coil) < 1 {
        tell(world, c.client, Span::Warning, "Недостаточно трубочистов!");
        return;
    }
    if distance == 0 {
        place_turf(world, c, coil, at);
        return;
    }
    let Some(dirn) = crate::pull::dir_between(at, c.mob_pos).map(|d| d as u8) else {
        return;
    };
    if p.has_end(dirn) {
        // It points at us: a stub pointing back on our tile.
        if !crate::devices::rhd::can_lay_cable(world, c.mob_pos) {
            tell(
                world,
                c.client,
                Span::Warning,
                "Трубочисты можно укладывать только на мостках и покрытии!",
            );
            return;
        }
        let Some(back) = Dir::from_bits(dirn).map(|d| d.reverse() as u8) else {
            return;
        };
        if pieces_on(world, c.mob_pos)
            .iter()
            .any(|(_, other)| other.has_end(back))
        {
            tell(world, c.client, Span::Warning, "Здесь уже есть трубочист!");
            return;
        }
        if new_piece(world, coil, c.mob_pos, 0, back).is_some() {
            use_stack(world, coil, 1);
        }
        return;
    }
    if p.d1 != 0 {
        return;
    }
    let (nd1, nd2) = joined(p.d2, dirn);
    if pieces_on(world, at)
        .iter()
        .any(|&(e, other)| e != piece && other.d1 == nd1 && other.d2 == nd2)
    {
        tell(world, c.client, Span::Warning, "Здесь уже есть трубочист!");
        return;
    }
    if let Some(mut live) = world.get_mut::<PipeCleaner>(piece) {
        live.d1 = nd1;
        live.d2 = nd2;
    }
    use_stack(world, coil, 1);
}

/// `cut_pipe_cleaner`: the piece comes up as a coil of its lengths and colour.
fn cut(world: &mut World, client: Entity, piece: Entity) {
    let (Some(&pos), Some(p)) = (
        world.get::<TilePos>(piece),
        world.get::<PipeCleaner>(piece).copied(),
    ) else {
        return;
    };
    world.despawn(piece);
    crate::construction::tile_changed(world, pos);
    if let Some(coil) = crate::construction::spawn_obj(world, COIL, pos) {
        world.entity_mut(coil).insert(Stack {
            amount: p.lengths(),
        });
        if let Some(mut look) = world.get_mut::<Appearance>(coil) {
            look.color = p.color;
        }
    }
    tell(world, client, Span::Notice, "Вы вытаскиваете трубочист.");
}

/// `update_icon_state` (`<d1>-<d2>`) and the colour.
fn looks(
    icons: Option<Res<IconIndex>>,
    mut q: Query<(&PipeCleaner, &DmAtom, &mut Appearance), Changed<PipeCleaner>>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (p, atom, mut appearance) in &mut q {
        if let Some(state) = icons.get(&atom.icon, &format!("{}-{}", p.d1, p.d2))
            && appearance.icon_state != state
        {
            appearance.icon_state = state;
        }
        if appearance.color != p.color {
            appearance.color = p.color;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stub_joins_into_a_piece_smaller_end_first() {
        // A stub pointing east joined from the north: 1-4.
        assert_eq!(joined(4, 1), (1, 4));
        assert_eq!(joined(1, 8), (1, 8));
        let piece = PipeCleaner {
            d1: 1,
            d2: 4,
            color: None,
        };
        assert_eq!(piece.lengths(), 2);
        assert!(piece.has_end(4) && !piece.has_end(2));
        let stub = PipeCleaner {
            d1: 0,
            d2: 2,
            color: None,
        };
        assert_eq!(stub.lengths(), 1);
    }
}
