//! Reinforced floors (server), after tg `code/game/turfs/open/floor/reinforced_floor.dm` and
//! `/turf/open/floor/plating/reinforced` (`plating.dm`).
//!
//! - Engine floors (`/turf/open/floor/engine` and its gas, hull and cult variants) do not pry
//!   up with a crowbar, never break or burn; a wrench (3 s) takes the rods off (`floor_tile`, two
//!   of them, one plastitanium sheet for the insulated floor) and scrapes the turf away with its
//!   air. Explosions only tear them up when heavy (half the time, one layer) or devastating
//!   (two layers with a lattice over space, or one layer), and what is hidden under them is
//!   only reached by devastation (`can_propagate_explosion`).
//! - Reinforced plating comes apart in three reversible steps ([`PlateState`]): wrench 10 s
//!   loosens the bolts, a welder with 3 fuel 15 s cuts the plate (screwdriver 15 s bolts it
//!   back), crowbar 20 s pries it off into two plasteel sheets (a welder 15 s welds it back).
// shortcut: no screws dropped on the level below when the bolts come loose (`drop_screws`), no
// tool sounds (like the rest of construction).

use bevy::prelude::*;
use ss13_mapload::construct::DmTurfBuild;
use ss13_mapload::{DmAtom, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use crate::construction::{Job, path_of, scrape_away, spawn_obj, spawn_stack, turf_at};
use crate::obj_damage::is_under;
use crate::tools::{Actor, ToolKind, tell, tool_usable, use_tool};

pub const ENGINE: &str = "/turf/open/floor/engine";
const INSULATION: &str = "/turf/open/floor/engine/insulation";
pub const R_PLATING: &str = "/turf/open/floor/plating/reinforced";
const FLOOR: &str = "/turf/open/floor";
const LATTICE: &str = "/obj/structure/lattice";
const PLASTEEL: &str = "/obj/item/stack/sheet/plasteel";

/// `deconstruction_state` of reinforced plating.
pub const PLATE_INTACT: u8 = 0;
pub const PLATE_BOLTS_LOOSENED: u8 = 1;
pub const PLATE_CUT: u8 = 2;
/// Not a tg state: the crowbar step's target, the plate comes off.
const PLATE_REMOVED: u8 = u8::MAX;

/// A reinforced plating's `deconstruction_state` out of `PLATE_INTACT`. Absent: intact.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlateState(pub u8);

/// A timed step on a reinforced floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloorJob {
    /// Wrench on an engine floor.
    EngineRods,
    /// A reinforced plating step from one state to the next.
    Plate { from: u8, to: u8 },
}

/// `/turf/open/floor/engine` and its subtypes.
pub fn is_engine(path: &str) -> bool {
    is_under(path, ENGINE)
}

fn plate_state(world: &World, turf: Entity) -> u8 {
    world.get::<PlateState>(turf).map_or(PLATE_INTACT, |s| s.0)
}

/// `floor_tile_amount`.
fn rods_amount(path: &str) -> u32 {
    if is_under(path, INSULATION) { 1 } else { 2 }
}

/// A tool on a reinforced floor turf: the engine floor's `crowbar_act` (nothing) and
/// `wrench_act`, the reinforced plating's `deconstruct_steps`. True when the click is used up.
pub fn tool_act(world: &mut World, actor: Actor, kind: ToolKind, turf: Entity) -> bool {
    let Some(path) = path_of(world, turf) else {
        return false;
    };
    let client = actor.client;
    if is_engine(&path) {
        return match kind {
            // `crowbar_act` returns at once: the rods are not pried up.
            ToolKind::Crowbar => true,
            ToolKind::Wrench => {
                if use_tool(
                    world,
                    actor,
                    turf,
                    30.0,
                    0,
                    Job::Floor(FloorJob::EngineRods),
                ) {
                    tell(
                        world,
                        client,
                        Span::Notice,
                        "Вы начинаете снимать прутья...",
                    );
                }
                true
            }
            _ => false,
        };
    }
    if !is_under(&path, R_PLATING) {
        return false;
    }
    let state = plate_state(world, turf);
    // (delay ds, welder fuel at the start, next state, start text).
    let step: Option<(f32, u32, u8, &str)> = match (state, kind) {
        (PLATE_INTACT, ToolKind::Wrench) => Some((
            100.0,
            0,
            PLATE_BOLTS_LOOSENED,
            "Вы начинаете откручивать болты...",
        )),
        (PLATE_BOLTS_LOOSENED, ToolKind::Welder) => {
            Some((150.0, 3, PLATE_CUT, "Вы начинаете резать пластину..."))
        }
        (PLATE_BOLTS_LOOSENED, ToolKind::Screwdriver) => {
            Some((150.0, 0, PLATE_INTACT, "Вы начинаете закручивать болты..."))
        }
        (PLATE_CUT, ToolKind::Crowbar) => {
            Some((200.0, 0, PLATE_REMOVED, "Вы начинаете отдирать пластину..."))
        }
        (PLATE_CUT, ToolKind::Welder) => Some((
            150.0,
            3,
            PLATE_BOLTS_LOOSENED,
            "Вы начинаете приваривать пластину обратно...",
        )),
        _ => None,
    };
    let Some((delay, fuel, to, text)) = step else {
        return false;
    };
    let Some(item) = actor.item else {
        return false;
    };
    // `tool_start_check(user, amount = 3)`: the welder's fuel at the start, none burnt by the
    // step itself.
    if fuel > 0 && !tool_usable(world, item, fuel, 0.0) {
        let text = crate::tools::unusable_message(world, item, fuel);
        tell(world, client, Span::Warning, text);
        return true;
    }
    let job = Job::Floor(FloorJob::Plate { from: state, to });
    if use_tool(world, actor, turf, delay, 0, job) {
        tell(world, client, Span::Notice, text);
    }
    true
}

/// The end of a [`FloorJob`]: only if the turf is still what the step started on.
pub fn complete(world: &mut World, client: Entity, target: Entity, job: FloorJob) {
    let Some(path) = path_of(world, target) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    match job {
        FloorJob::EngineRods => {
            if !is_engine(&path) {
                return;
            }
            let tile = world
                .get::<DmTurfBuild>(target)
                .map(|b| b.floor_tile.clone())
                .unwrap_or_default();
            if !tile.is_empty() {
                spawn_stack(world, &tile, pos, rods_amount(&path));
            }
            scrape_away(world, pos, true);
        }
        FloorJob::Plate { from, to } => {
            if !is_under(&path, R_PLATING) || plate_state(world, target) != from {
                return;
            }
            if to == PLATE_REMOVED {
                tell(world, client, Span::Notice, "Вы отдираете пластину.");
                spawn_stack(world, PLASTEEL, pos, 2);
                scrape_away(world, pos, true);
                return;
            }
            set_plate_state(world, target, to);
            let text = match (from, to) {
                (PLATE_INTACT, _) => "Вы выкручиваете болты.",
                (PLATE_BOLTS_LOOSENED, PLATE_CUT) => "Вы прорезаете пластину.",
                (PLATE_BOLTS_LOOSENED, _) => "Вы закручиваете болты.",
                _ => "Вы привариваете пластину обратно.",
            };
            tell(world, client, Span::Notice, text);
        }
    }
}

/// `deconstruction_state = to; update_appearance()`: `r_plate-<state>`.
fn set_plate_state(world: &mut World, turf: Entity, to: u8) {
    if to == PLATE_INTACT {
        world.entity_mut(turf).remove::<PlateState>();
    } else {
        world.entity_mut(turf).insert(PlateState(to));
    }
    let Some(icon) = world.get::<DmAtom>(turf).map(|a| a.icon.clone()) else {
        return;
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, &format!("r_plate-{to}")));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(turf))
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
}

/// `/turf/open/floor/engine/ex_act` (the floor's dense-object shielding already checked by the
/// caller). True when `pos` is an engine floor (handled here).
pub fn ex_act(
    world: &mut World,
    pos: TilePos,
    path: &str,
    severity: crate::explosions::Severity,
) -> bool {
    use crate::explosions::Severity;
    if !is_engine(path) {
        return false;
    }
    let roll = |world: &mut World, chance: f32| {
        world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(chance)
    };
    match severity {
        Severity::Devastate => {
            let (lattice, layers) = if roll(world, 80.0) {
                (true, 2)
            } else if roll(world, 50.0) {
                (false, 2)
            } else {
                (false, 1)
            };
            for _ in 0..layers {
                scrape_away(world, pos, true);
            }
            // `attempt_lattice_replacement`: a lattice when what is left is not a floor.
            let floor = turf_at(world, pos)
                .and_then(|t| path_of(world, t))
                .is_some_and(|p| is_under(&p, FLOOR));
            if lattice && !floor {
                spawn_obj(world, LATTICE, pos);
            }
        }
        Severity::Heavy => {
            if roll(world, 50.0) {
                scrape_away(world, pos, true);
            }
        }
        Severity::Light => {}
    }
    true
}

/// `/turf/open/floor/engine/can_propagate_explosion`: below devastation the blast does not
/// reach what is hidden under the floor.
pub fn shields_underfloor(
    world: &World,
    turf_path: &str,
    thing: Entity,
    severity: crate::explosions::Severity,
) -> bool {
    is_engine(turf_path)
        && severity != crate::explosions::Severity::Devastate
        && world
            .get::<ss13_mapload::undertile::HiddenUnderTile>(thing)
            .is_some()
}

/// `examine` of engine floors and `deconstruction_hints` of reinforced plating.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(path) = path_of(world, target) else {
        return;
    };
    let text = if is_engine(&path) {
        "Укрепляющие прутья надёжно закреплены болтами."
    } else if is_under(&path, R_PLATING) {
        match plate_state(world, target) {
            PLATE_BOLTS_LOOSENED => {
                "Укрепление пластины откручено, но всё ещё прочно приварено к обшивке."
            }
            PLATE_CUT => "Укрепление пластины прорезано, но всё ещё неплотно держится на месте.",
            _ => "Укрепление пластины надёжно закреплено болтами.",
        }
    } else {
        return;
    };
    lines.push((Span::Notice, text.to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_floors_and_their_rods() {
        assert!(is_engine("/turf/open/floor/engine/hull/reinforced"));
        assert!(is_engine("/turf/open/floor/engine/cult"));
        assert!(!is_engine("/turf/open/floor/plating/reinforced"));
        assert!(!is_engine("/turf/open/floor/engineering"));
        assert_eq!(rods_amount("/turf/open/floor/engine/o2"), 2);
        assert_eq!(rods_amount("/turf/open/floor/engine/insulation"), 1);
    }
}
