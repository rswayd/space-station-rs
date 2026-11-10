//! Navigation computers (server): tg `/obj/machinery/computer/camera_advanced/shuttle_docker`
//! (`code/modules/shuttle/shuttle_consoles/navigation_computer.dm`), its White Ship and
//! Syndicate subtypes and BandaStation's (`modular_bandastation/shuttles/code/dockers.dm`).
//!
//! - A plain click on the computer takes its controls (again lets go), a right click turns the
//!   landing spot a quarter clockwise (`rotateLandingSpot`, the offsets turn with it). While the
//!   operator stays next to it, a plain click on a tile it sees designates the landing spot there
//!   (`placeLandingSpot`): the shuttle's docking port goes to the tile minus the offsets, facing
//!   the chosen way. Every tile the shuttle's areas would cover must be clear
//!   (`checkLandingTurf`): 10 tiles off the level's edge, on a level of a loaded map that is not
//!   a shuttle template or transit, of a whitelisted type (space, plating, lava, open space,
//!   misc; the Syndicate's also mineral walls) and outside every other docking port's
//!   rectangle; tiles of the shuttle itself always are.
//! - The computer's own stationary port (`<shuttle>_custom`, "custom location") is made on the
//!   first designation and moved on the next; one the shuttle sits on is left behind under a
//!   dead id and a new one made. The shuttle console flies to it like any dock.
// shortcut: no camera eye (`/mob/eye/camera/remote/shuttle_docker`), so the spot is picked among
// the tiles the operator sees, without the green/red placement preview or the jump list; no
// `designate_time` wait, no hidden ports, no jamming at war, no `z_lock`.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DEFAULT_AREA, DmObj, DmTurfBuild, MapLoadReport};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::SpatialIndex;

use crate::shuttle::{MobileDock, StationaryDock, ordered_turfs};
use crate::tools::tell;

pub const DOCKER: &str = "/obj/machinery/computer/camera_advanced/shuttle_docker";

/// A docker type: `(path and subtypes, shuttleId, shuttlePortId, x_offset, y_offset, mineral
/// walls allowed)`, most specific first.
type DockerKind = (&'static str, &'static str, &'static str, i32, i32, bool);

pub const DOCKERS: [DockerKind; 6] = [
    (
        "/obj/machinery/computer/camera_advanced/shuttle_docker/whiteship",
        "whiteship",
        "whiteship_custom",
        -6,
        -10,
        false,
    ),
    (
        "/obj/machinery/computer/camera_advanced/shuttle_docker/syndicate/sit",
        "syndicate_sit",
        "syndicate_sit_custom",
        0,
        3,
        true,
    ),
    (
        "/obj/machinery/computer/camera_advanced/shuttle_docker/syndicate/sst",
        "syndicate_sst",
        "syndicate_sst_custom",
        0,
        3,
        true,
    ),
    // BandaStation's override of the offsets (tg: 7, 1).
    (
        "/obj/machinery/computer/camera_advanced/shuttle_docker/syndicate",
        "syndicate",
        "syndicate_custom",
        0,
        10,
        true,
    ),
    (
        "/obj/machinery/computer/camera_advanced/shuttle_docker/argos",
        "argos",
        "argos_custom",
        0,
        8,
        false,
    ),
    (
        "/obj/machinery/computer/camera_advanced/shuttle_docker/specops",
        "specops",
        "specops_custom",
        0,
        3,
        false,
    ),
];

/// `whitelist_turfs`.
const WHITELIST: [&str; 5] = [
    "/turf/open/space",
    "/turf/open/floor/plating",
    "/turf/open/lava",
    "/turf/open/openspace",
    "/turf/open/misc",
];
const MINERAL: &str = "/turf/closed/mineral";
const PORT_NAME: &str = "custom location";
/// `checkLandingTurf`: tiles this close to a level's edge are never allowed.
const EDGE: i32 = 10;

/// `/obj/machinery/computer/camera_advanced/shuttle_docker`.
#[derive(Component, Clone, Debug)]
pub struct ShuttleDocker {
    pub shuttle_id: String,
    pub port_id: String,
    /// The current offsets of the eye from the port (`x_offset`, `y_offset`), turned with it.
    pub offset: (i32, i32),
    /// The landing spot's facing (the eye's `dir`); `None` until first used: the shuttle's own.
    pub dir: Option<Dir>,
    pub mineral: bool,
    /// `my_port`.
    pub my_port: Option<Entity>,
}

/// The mob working a navigation computer.
#[derive(Component, Clone, Copy, Debug)]
pub struct DockerOperator(pub Entity);

/// `checkLandingSpot` results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    Clear,
    Blocked,
}

pub struct ShuttleDockerPlugin;

impl Plugin for ShuttleDockerPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 95, |w, c| {
            interact(w, c.client, c.mob, c.target, c.pos, c.modifiers.right)
        })
        .add_systems(ss13_world::MapInitLate, setup_dockers);
    }
}

fn setup_dockers(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, String)> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, DOCKER))
        .map(|(e, o)| (e, o.path.clone()))
        .collect();
    for (e, path) in found {
        let Some(&(_, shuttle, port, x, y, mineral)) =
            DOCKERS.iter().find(|(root, ..)| is_under(&path, root))
        else {
            continue;
        };
        world.entity_mut(e).insert(ShuttleDocker {
            shuttle_id: shuttle.into(),
            port_id: port.into(),
            offset: (x, y),
            dir: None,
            mineral,
            my_port: None,
        });
    }
}

fn shuttle_port(world: &mut World, id: &str) -> Option<Entity> {
    let mut q = world.query::<(Entity, &MobileDock)>();
    q.iter(world).find(|(_, m)| m.id == id).map(|(e, _)| e)
}

/// The clicks of an operator: on the computer (take or leave it, rotate), elsewhere (designate).
fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    pos: TilePos,
    right: bool,
) -> bool {
    if let Some(docker) = world.get::<ShuttleDocker>(target).cloned() {
        if right {
            rotate(world, target);
            tell(world, client, Span::Notice, "Точка посадки повёрнута.");
            return true;
        }
        if world
            .get::<DockerOperator>(mob)
            .is_some_and(|o| o.0 == target)
        {
            world.entity_mut(mob).remove::<DockerOperator>();
            tell(
                world,
                client,
                Span::Notice,
                "Вы отходите от навигационного компьютера.",
            );
            return true;
        }
        if shuttle_port(world, &docker.shuttle_id).is_none() {
            tell(
                world,
                client,
                Span::Warning,
                "Warning: Shuttle connection severed!",
            );
            return true;
        }
        world.entity_mut(mob).insert(DockerOperator(target));
        tell(
            world,
            client,
            Span::Notice,
            "Вы за навигационным компьютером: щелчок по клетке назначает точку посадки, \
             правый щелчок по компьютеру поворачивает её.",
        );
        return true;
    }
    let Some(DockerOperator(console)) = world.get::<DockerOperator>(mob).copied() else {
        return false;
    };
    let near = match (world.get::<TilePos>(mob), world.get::<TilePos>(console)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    if !near || world.get::<crate::power::Unpowered>(console).is_some() {
        world.entity_mut(mob).remove::<DockerOperator>();
        return false;
    }
    if place_landing_spot(world, console, pos) {
        tell(world, client, Span::Notice, "Transit location designated.");
    } else {
        tell(world, client, Span::Warning, "Invalid transit location.");
    }
    true
}

/// tg `rotateLandingSpot`: a quarter turn clockwise, the offsets with it.
pub fn rotate(world: &mut World, console: Entity) {
    let shuttle_dir = world
        .get::<ShuttleDocker>(console)
        .map(|d| d.shuttle_id.clone())
        .and_then(|id| shuttle_port(world, &id))
        .and_then(|p| world.get::<MobileDock>(p).map(|m| m.dir))
        .unwrap_or(Dir::North);
    if let Some(mut d) = world.get_mut::<ShuttleDocker>(console) {
        let dir = d.dir.unwrap_or(shuttle_dir);
        d.dir = Some(turn_cw(dir));
        d.offset = (d.offset.1, -d.offset.0);
    }
}

/// BYOND `turn(dir, -90)` of a cardinal.
fn turn_cw(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::East,
        Dir::East => Dir::South,
        Dir::South => Dir::West,
        _ => Dir::North,
    }
}

/// The tiles the shuttle's areas would cover with its port at `port_pos` facing `dir`.
fn landing_tiles(
    world: &World,
    mobile: &MobileDock,
    port: TilePos,
    port_pos: TilePos,
    dir: Dir,
) -> Vec<TilePos> {
    let areas = world.resource::<Areas>();
    let now = ordered_turfs(port, mobile.dir, mobile.size);
    let then = ordered_turfs(port_pos, dir, mobile.size);
    now.iter()
        .zip(then)
        .filter(|(p, _)| {
            areas
                .area_at(**p)
                .is_some_and(|a| mobile.areas.contains(&a))
        })
        .map(|(_, q)| q)
        .collect()
}

/// tg `checkLandingSpot` for the port at `port_pos` facing `dir`.
pub fn check_landing(
    world: &mut World,
    docker: &ShuttleDocker,
    port_pos: TilePos,
    dir: Dir,
) -> Landing {
    let Some(port) = shuttle_port(world, &docker.shuttle_id) else {
        return Landing::Blocked;
    };
    let (Some(mobile), Some(&at)) = (
        world.get::<MobileDock>(port).cloned(),
        world.get::<TilePos>(port),
    ) else {
        return Landing::Blocked;
    };
    // `locked_traits`: no reserved (templates, transit), CentCom or away levels.
    let on_map = world.get_resource::<MapLoadReport>().is_some_and(|r| {
        r.maps.iter().any(|m| {
            port_pos.z >= m.first_z
                && u32::from(port_pos.z) < u32::from(m.first_z) + m.size.2
                && !m.path.to_string_lossy().contains("_maps/shuttles/")
        })
    });
    if !on_map {
        return Landing::Blocked;
    }
    let Some((w, h)) = world.resource::<SpatialIndex>().level_size(port_pos.z) else {
        return Landing::Blocked;
    };
    let mut q = world.query::<(Entity, &StationaryDock, &TilePos)>();
    let others: Vec<Vec<TilePos>> = q
        .iter(world)
        .filter(|(e, ..)| Some(*e) != docker.my_port)
        .map(|(_, d, p)| ordered_turfs(*p, d.dir, d.size))
        .collect();
    for t in landing_tiles(world, &mobile, at, port_pos, dir) {
        if t.x <= EDGE || t.y <= EDGE || t.x >= w as i32 - EDGE || t.y >= h as i32 - EDGE {
            return Landing::Blocked;
        }
        if world
            .resource::<Areas>()
            .area_at(t)
            .is_some_and(|a| mobile.areas.contains(&a))
        {
            continue;
        }
        let turf_path = crate::construction::turf_at(world, t)
            .and_then(|e| world.get::<DmTurfBuild>(e))
            .map(|b| b.path.clone())
            .unwrap_or_default();
        let allowed = WHITELIST.iter().any(|r| is_under(&turf_path, r))
            || docker.mineral && is_under(&turf_path, MINERAL);
        if !allowed || others.iter().any(|rect| rect.contains(&t)) {
            return Landing::Blocked;
        }
    }
    Landing::Clear
}

/// tg `placeLandingSpot` with the eye on `eye`: false when the spot is not clear.
pub fn place_landing_spot(world: &mut World, console: Entity, eye: TilePos) -> bool {
    let Some(docker) = world.get::<ShuttleDocker>(console).cloned() else {
        return false;
    };
    let Some(port) = shuttle_port(world, &docker.shuttle_id) else {
        return false;
    };
    let Some(mobile) = world.get::<MobileDock>(port).cloned() else {
        return false;
    };
    let dir = docker.dir.unwrap_or(mobile.dir);
    let port_pos = TilePos::new(eye.x - docker.offset.0, eye.y - docker.offset.1, eye.z);
    if check_landing(world, &docker, port_pos, dir) != Landing::Clear {
        return false;
    }
    let mut my_port = docker.my_port.filter(|&p| world.get_entity(p).is_ok());
    // A port the shuttle sits on stays behind, nameless, for its takeoff.
    if let Some(old) = my_port
        && mobile.docked == Some(old)
    {
        if let Some(mut d) = world.get_mut::<StationaryDock>(old) {
            d.id = String::new();
            d.name = format!("Old {}", d.name);
        }
        my_port = None;
    }
    let port_e = match my_port {
        Some(p) => {
            if let Some(mut d) = world.get_mut::<StationaryDock>(p) {
                d.dir = dir;
                d.size = mobile.size;
            }
            world.entity_mut(p).insert(port_pos);
            p
        }
        None => {
            let area = crate::shuttle::area_named(world, DEFAULT_AREA);
            world
                .spawn((
                    StationaryDock {
                        id: docker.port_id.clone(),
                        name: PORT_NAME.into(),
                        dir,
                        size: mobile.size,
                        area,
                    },
                    port_pos,
                ))
                .id()
        }
    };
    if let Some(mut d) = world.get_mut::<ShuttleDocker>(console) {
        d.my_port = Some(port_e);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_turns_clockwise() {
        assert_eq!(turn_cw(Dir::North), Dir::East);
        assert_eq!(turn_cw(Dir::West), Dir::North);
    }
}
