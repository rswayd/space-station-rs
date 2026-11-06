//! Effect systems (server), after tg `code/game/objects/effects/effect_system/` and
//! `SSfluids` (`code/controllers/subsystem/fluids.dm`):
//!
//! - [`particles`]: `/datum/effect_system/basic`: sparks (`do_sparks`), quantum sparks,
//!   lightning and steam: a few particles flying off a tile and fading.
//! - [`smoke`]: `do_smoke` / `do_chem_smoke`: clouds spreading through atmos-adjacent tiles
//!   (`SSsmoke`), of every ported kind, chemical smoke carrying reagents.
//! - [`foam`]: `do_foam`: foam spreading over open tiles (`SSfoam`), applying its reagents and
//!   leaving foamed metal, resin or residue; [`reactions`]: the chemical reactions that make
//!   smoke and foam.
//! - [`trail`]: `/datum/effect_system/trail_follow`: ion, steam and smoke trails left behind a
//!   moving holder.
//! - [`mines`]: `/obj/effect/mine`, set off by stepping off the pressure plate; [`webs`]:
//!   sticky spider webs catching walkers.
//!
//! Cost: each system runs only while its effects exist (`run_if(any_with_component)`), one
//! pass over its live particles per tick.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::{Dir, TilePos};
use ss13_world::SpatialIndex;

pub mod foam;
pub mod mines;
pub mod particles;
pub(crate) mod reactions;
pub mod smoke;
pub mod trail;
pub mod webs;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FluidGroups>()
            .add_systems(
                Update,
                (
                    particles::tick_particles.run_if(any_with_component::<particles::Particle>),
                    smoke::tick_smoke.run_if(any_with_component::<smoke::Smoke>),
                    foam::tick_foam.run_if(any_with_component::<foam::Foam>),
                    foam::halon_resin,
                    trail::tick_trails.run_if(any_with_component::<trail::Trail>),
                    trail::fade_trail_particles.run_if(any_with_component::<trail::TrailParticle>),
                ),
            )
            .add_systems(
                Update,
                (mines::step_on_mines, webs::walk_into_webs)
                    .after(ss13_world::WorldSystems::Movement),
            );
    }
}

/// `/datum/fluid_group`: how many nodes the flood wants (`target_size`), how many it ever had
/// (`total_size`, never decremented so a dissipating flood does not grow back), and what every
/// node carries. tg copies the reagents into each node (`copy_only`); the copies are never
/// consumed, so the group keeps one.
#[derive(Clone, Debug, Default)]
pub struct FluidGroup {
    pub target_size: u32,
    pub total_size: u32,
    /// Live nodes; the group is dropped with its last one.
    pub nodes: u32,
    pub reagents: Option<ss13_reagents::Holder>,
    pub color: Option<[u8; 4]>,
}

#[derive(Resource, Default)]
pub struct FluidGroups {
    next: u32,
    groups: std::collections::HashMap<u32, FluidGroup>,
}

impl FluidGroups {
    pub fn create(&mut self, group: FluidGroup) -> u32 {
        self.next = self.next.wrapping_add(1);
        self.groups.insert(self.next, group);
        self.next
    }

    pub fn get(&self, id: u32) -> Option<&FluidGroup> {
        self.groups.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut FluidGroup> {
        self.groups.get_mut(&id)
    }

    /// `add_node`.
    fn add_node(&mut self, id: u32) {
        if let Some(g) = self.groups.get_mut(&id) {
            g.total_size += 1;
            g.nodes += 1;
        }
    }

    /// `remove_node`, and `qdel` of the group with its last node.
    fn remove_node(&mut self, id: u32) {
        let Some(g) = self.groups.get_mut(&id) else {
            return;
        };
        g.nodes = g.nodes.saturating_sub(1);
        if g.nodes == 0 {
            self.groups.remove(&id);
        }
    }

    /// The flood may still grow (tg spreads while `total_size <= target_size`, one node past
    /// the target: `DIAMOND_AREA(4)` smoke covered 42 tiles; the port stops at the target).
    fn can_grow(&self, id: u32) -> bool {
        self.groups
            .get(&id)
            .is_some_and(|g| g.total_size < g.target_size)
    }
}

/// What a moving effect passes through (tg `pass_flags` of the mover against each dense
/// thing's `pass_flags_self`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    /// `/obj/effect/particle_effect`: `PASSTABLE | PASSGRILLE`; mobs block.
    Particle,
    /// Foam's `LinkBlockedWithAccess` with `PASSTABLE | PASSGRILLE | PASSMACHINE |
    /// PASSSTRUCTURE`: doors, windows, racks and resin block; mobs do not.
    Foam,
}

const TABLE: &str = "/obj/structure/table";
const GRILLE: &str = "/obj/structure/grille";
const WINDOW: &str = "/obj/structure/window";
const RACK: &str = "/obj/structure/rack";
const STRUCTURE: &str = "/obj/structure";
const MACHINERY: &str = "/obj/machinery";
const DOOR: &str = "/obj/machinery/door";

/// Whether the dense `e` lets `pass` through.
fn lets_through(world: &World, e: Entity, pass: Pass) -> bool {
    if world.get::<crate::health::Health>(e).is_some() {
        return pass == Pass::Foam;
    }
    let Some(path) = crate::construction::path_of(world, e) else {
        return false;
    };
    if is_under(&path, TABLE) || is_under(&path, GRILLE) {
        return true;
    }
    if pass == Pass::Particle {
        return false;
    }
    if is_under(&path, WINDOW) || is_under(&path, RACK) || is_under(&path, foam::RESIN) {
        return false;
    }
    if is_under(&path, MACHINERY) {
        return !is_under(&path, DOOR);
    }
    is_under(&path, STRUCTURE)
}

/// A step from `from` in `dir` is blocked for `pass`: off the map, a dense turf, a dense
/// thing on the target that does not let it through, or a border object across the edge.
pub(crate) fn step_blocked(world: &World, from: TilePos, dir: Dir, pass: Pass) -> bool {
    let to = from.step(dir);
    let index = world.resource::<SpatialIndex>();
    if !index.in_bounds(to) {
        return true;
    }
    let placeholder = Entity::PLACEHOLDER;
    if index
        .dense_on_tile(to)
        .iter()
        .any(|&e| !lets_through(world, e, pass))
    {
        return true;
    }
    let (dx, dy) = dir.offset();
    if dx != 0 && dy != 0 {
        return false;
    }
    index
        .exit_blockers(from, dir, placeholder)
        .chain(index.enter_blockers(to, dir, placeholder))
        .any(|e| world.get::<ss13_mapload::Turf>(e).is_none() && !lets_through(world, e, pass))
}

/// Every entity on `pos` (a copy, for loops that change the world).
pub(crate) fn on_tile(world: &World, pos: TilePos) -> Vec<Entity> {
    world.resource::<SpatialIndex>().on_tile(pos).to_vec()
}

/// The clients that see `pos` (tg `viewers(range)`, range only).
pub(crate) fn tell_near(
    world: &mut World,
    pos: TilePos,
    range: u32,
    span: ss13_shared::chat::Span,
    text: &str,
) {
    let clients = crate::obj_damage::clients_in_range(world, pos, range, &[]);
    if !clients.is_empty() {
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: crate::chem::line(span, text),
        });
    }
}
