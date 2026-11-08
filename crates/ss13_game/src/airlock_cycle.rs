//! Cycling airlocks (server): tg airlock `closeOtherId`/`close_others` (`update_other_id`) and
//! `cyclelinkeddir`/`cyclelinkedairlock` (`cyclelinkairlock`), set by the `cyclelink_helper`
//! and `cyclelink_helper_multi` mapping helpers (`code/game/machinery/doors/airlock.dm`).
//!
//! - At map load every airlock with a cycle id links with every other airlock of that id, and
//!   every airlock with a cyclelink direction links with the first airlock within 11 tiles that
//!   way whose own direction points back.
//! - When a linked airlock starts opening, each airlock it links to closes 0.2 s later; one
//!   that is moving closes right after it finishes opening (`delayed_close_requested`). Nothing
//!   closes while either side has emergency access.
// shortcut: no shuttle docking exemption (`shuttledocked`), and an airlock built later
// (`crate::airlock_construction`) does not join a group (`passed_cycle_id`); add with docking
// and electronics cycle ids.

use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::SpatialIndex;

use crate::doors::{Door, DoorActive, DoorState};
use ss13_mapload::DoorKind;

/// tg `DOOR_VISION_DISTANCE`.
const DOOR_VISION_DISTANCE: u32 = 11;
/// tg `addtimer(close, BYPASS_DOOR_CHECKS)`: 2 deciseconds.
const CYCLE_CLOSE_DELAY: f64 = 0.2;

pub struct AirlockCyclePlugin;

impl Plugin for AirlockCyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, cycle_closes)
            .add_observer(unlink_removed);
    }
}

/// `update_other_id` for every mapped airlock with a `closeOtherId`, then `cyclelinkairlock`
/// for each `(airlock, dir)` a `cyclelink_helper` gave a direction.
pub fn link_mapped(world: &mut World, cyclelink_dirs: &[(Entity, u8)]) {
    let mut q = world.query::<(Entity, &Door)>();
    let mut groups: std::collections::HashMap<String, Vec<Entity>> = default();
    for (e, door) in q.iter(world) {
        if door.kind == DoorKind::Airlock && !door.close_other_id.is_empty() {
            groups
                .entry(door.close_other_id.clone())
                .or_default()
                .push(e);
        }
    }
    for members in groups.values() {
        for &e in members {
            if let Some(mut door) = world.get_mut::<Door>(e) {
                door.close_others = members.iter().copied().filter(|&o| o != e).collect();
            }
        }
    }
    for &(airlock, dir) in cyclelink_dirs {
        if let Some(found) = cyclelink_target(world, airlock, dir, cyclelink_dirs) {
            if let Some(mut door) = world.get_mut::<Door>(airlock) {
                door.cyclelinked = Some(found);
            }
            if let Some(mut door) = world.get_mut::<Door>(found) {
                door.cyclelinked = Some(airlock);
            }
        } else {
            let pos = world.get::<TilePos>(airlock).copied();
            warn!(
                "airlock {airlock:?} at {pos:?} failed to find a valid airlock to cyclelink with"
            );
        }
    }
}

/// `cyclelinkairlock`: the first airlock `dir`-wards within [`DOOR_VISION_DISTANCE`] tiles
/// whose own cyclelink direction points back at `airlock`.
fn cyclelink_target(
    world: &World,
    airlock: Entity,
    dir: u8,
    dirs: &[(Entity, u8)],
) -> Option<Entity> {
    let step = Dir::from_bits(dir)?;
    let back = step.reverse() as u8;
    let mut pos = *world.get::<TilePos>(airlock)?;
    let index = world.resource::<SpatialIndex>();
    for _ in 0..DOOR_VISION_DISTANCE {
        pos = pos.step(step);
        // `locate() in T`: the first airlock on the tile, kept only if it points back.
        let found = index.on_tile(pos).iter().copied().find(|&e| {
            world
                .get::<Door>(e)
                .is_some_and(|d| d.kind == DoorKind::Airlock)
        });
        if let Some(found) = found
            && dirs.iter().any(|&(e, d)| e == found && d == back)
        {
            return Some(found);
        }
    }
    None
}

/// The airlocks an opening `door` closes: its group and its cyclelinked partner.
fn links(door: &Door) -> impl Iterator<Item = Entity> + '_ {
    door.close_others.iter().copied().chain(door.cyclelinked)
}

/// `airlock/open`: every airlock that started opening closes the airlocks it links to, unless
/// either side has emergency access.
fn cycle_closes(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &mut Door)>();
    let mut opened: Vec<(Entity, Vec<Entity>)> = Vec::new();
    for (e, mut door) in q.iter_mut(world) {
        if !door.cycle_opened {
            continue;
        }
        door.bypass_change_detection().cycle_opened = false;
        if door.emergency {
            continue;
        }
        let linked: Vec<Entity> = links(&door).collect();
        if !linked.is_empty() {
            opened.push((e, linked));
        }
    }
    for (_, linked) in opened {
        for other in linked {
            let Some(mut door) = world.get_mut::<Door>(other) else {
                continue;
            };
            if door.emergency {
                continue;
            }
            if door.is_operating() {
                door.delayed_close_requested = true;
            } else if door.state == DoorState::Open {
                door.cycle_close_at = Some(now + CYCLE_CLOSE_DELAY);
                world.entity_mut(other).insert(DoorActive);
            }
        }
    }
}

/// tg airlock `Destroy`: a removed airlock leaves its partner and its group.
fn unlink_removed(remove: On<Remove<Door>>, mut doors: Query<&mut Door>) {
    let gone = remove.entity;
    let Ok(door) = doors.get(gone) else {
        return;
    };
    let linked: Vec<Entity> = links(door).collect();
    for other in linked {
        if let Ok(mut d) = doors.get_mut(other) {
            d.close_others.retain(|&e| e != gone);
            if d.cyclelinked == Some(gone) {
                d.cyclelinked = None;
            }
        }
    }
}

/// The airlock's cycling line (`airlock/examine`).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(door) = world.get::<Door>(target) else {
        return;
    };
    if door.kind != DoorKind::Airlock {
        return;
    }
    let text = if !door.close_other_id.is_empty() {
        format!("Этот шлюз циклируется по ID: {}.", door.close_other_id)
    } else if let Some(partner) = door.cyclelinked {
        format!(
            "Этот шлюз циклируется с: {}.",
            crate::security::name_of(world, partner)
        )
    } else {
        "Этот шлюз не циклируется.".to_owned()
    };
    lines.push((Span::Warning, text));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doors::test_airlock;

    #[test]
    fn links_list_group_and_partner() {
        let mut door = test_airlock(&[]);
        let (a, b) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
        );
        door.close_others = vec![a];
        door.cyclelinked = Some(b);
        assert_eq!(links(&door).collect::<Vec<_>>(), vec![a, b]);
    }
}
