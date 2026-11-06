//! Escape pods (server), after tg `/obj/docking_port/mobile/pod`
//! (`code/modules/shuttle/mobile_port/variants/emergency/pods.dm`), the escape pod loaders
//! (`/obj/docking_port/stationary/escape_pod`) and `on_emergency_launch`/`on_emergency_dock`
//! (`mobile_port.dm`).
//!
//! - Round start: each escape pod loader of the station map loads its template
//!   (`_maps/shuttles/escape_pod/escape_pod_default.dmm`, one copy per loader, each onto its own
//!   level like the emergency shuttle) and the pod docks at the loader.
//! - When the emergency shuttle leaves the station, every pod still there enters its own transit
//!   (`ENDGAME_LAUNCHED`); when the shuttle reaches CentCom, the pods dock at their `pod_away`
//!   docks there (the template's level stands in for CentCom), so whoever rode one escaped.
//! - Early launch: the pod's console (`/obj/machinery/computer/shuttle/pod`, `crate::shuttle_nav`)
//!   sends it to its `<pod>_lavaland` dock (`/obj/docking_port/stationary/random`) on red alert
//!   or above, or once emagged ([`request`]); a pod launches once (`EARLY_LAUNCHED`) and no
//!   longer follows the emergency shuttle, so its riders did not escape.
// shortcut: no lavaland level is loaded, so the random dock does not move to lavaland
// (`target_area`): at the launch it moves to a random open space spot of its own (the station's)
// level, away from the edges (`edge_distance`), where the pod's whole footprint is space.
// No pod storage lock (`/datum/storage/pod`), no station traits for cramped or luxury pods.

use std::path::PathBuf;

use bevy::prelude::*;

use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DEFAULT_AREA, DmDockingPort, MapSeed};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

use crate::security_level::SecLevel;
use crate::shuttle::{
    MobileDock, RoundstartPort, RoundstartPorts, StationaryDock, dock_at_map_pos, initiate_docking,
    ordered_turfs, push_template, setup_template_copy,
};
use crate::shuttle_nav::NavShuttle;

const POD_PORT: &str = "/obj/docking_port/mobile/pod";
const RANDOM_DOCK: &str = "/obj/docking_port/stationary/random";
const SPACE_TURF: &str = "/turf/open/space";
/// `/obj/docking_port/stationary/random` `edge_distance`.
const EDGE_DISTANCE: i32 = 16;
/// Random spots tried before the pod refuses to launch.
const PLACEMENT_TRIES: usize = 400;
/// tg `/obj/docking_port/mobile/pod/request` refusal.
pub const RED_ONLY: &str = "Escape pods will only launch during \"Code Red\" security alert.";

/// tg `launch_status` of a pod.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaunchStatus {
    /// At the station (`UNLAUNCHED`).
    #[default]
    Unlaunched,
    /// Sent away by its console (`EARLY_LAUNCHED`).
    EarlyLaunched,
    /// In transit with the emergency shuttle (`ENDGAME_LAUNCHED`).
    EndgameLaunched,
    /// Docked at CentCom (`mode = SHUTTLE_ENDGAME`).
    Endgame,
}

/// One escape pod.
#[derive(Clone, Debug)]
pub struct Pod {
    pub port: Entity,
    /// The station loader it docks at.
    pub station: Entity,
    /// Its `pod_away` dock on the CentCom stand-in.
    pub away: Entity,
    pub transit: Entity,
    /// The template's level (the CentCom stand-in).
    pub level_z: u16,
    pub status: LaunchStatus,
}

/// The round's escape pods.
#[derive(Resource, Debug, Default)]
pub struct EscapePods(pub Vec<Pod>);

#[derive(Resource)]
struct PodTemplates(Vec<(RoundstartPort, PathBuf)>);

pub struct EscapePodsPlugin;

impl Plugin for EscapePodsPlugin {
    fn build(&self, app: &mut App) {
        let pods: Vec<(RoundstartPort, PathBuf)> = app
            .world()
            .get_resource::<RoundstartPorts>()
            .map(|r| r.of("escape_pod"))
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, path)| ss13_data::exists(path))
            .collect();
        for (_, path) in &pods {
            push_template(app, path.clone());
        }
        app.init_resource::<EscapePods>()
            .insert_resource(PodTemplates(pods))
            .add_systems(Startup, setup_pods.after(crate::shuttle::setup_shuttle));
    }
}

/// tg `load_roundstart` of each loader: the template's copy registers and docks at it.
fn setup_pods(world: &mut World) {
    let Some(templates) = world.remove_resource::<PodTemplates>() else {
        return;
    };
    for (k, (port, path)) in templates.0.iter().enumerate() {
        let nth = templates.0[..k].iter().filter(|(_, p)| p == path).count();
        let Some(station) = dock_at_map_pos(world, port.x, port.y, port.z) else {
            warn!(
                "escape pod loader at {},{},{} is not a dock",
                port.x, port.y, port.z
            );
            continue;
        };
        // tg's duplicate ids: `pod`, `pod_2`, ...
        let id = if k == 0 {
            "pod".to_owned()
        } else {
            format!("pod_{}", k + 1)
        };
        let Some(shuttle) = setup_template_copy(
            world,
            path,
            nth,
            POD_PORT,
            &id,
            &format!("{id}_away"),
            "CentCom escape pod dock",
        ) else {
            continue;
        };
        let moved = initiate_docking(world, shuttle.port, station);
        info!("escape pod {id} at the station: {moved:?}");
        if moved.is_err() {
            continue;
        }
        world
            .entity_mut(shuttle.port)
            .insert(NavShuttle::new(shuttle.transit));
        world.resource_mut::<EscapePods>().0.push(Pod {
            port: shuttle.port,
            station,
            away: shuttle.away,
            transit: shuttle.transit,
            level_z: shuttle.level_z,
            status: LaunchStatus::Unlaunched,
        });
    }
}

/// tg `on_emergency_launch` of the pods: those at the station enter transit.
pub fn on_emergency_launch(world: &mut World) {
    let Some(pods) = world.get_resource::<EscapePods>().map(|p| p.0.clone()) else {
        return;
    };
    for (i, pod) in pods.iter().enumerate() {
        if pod.status != LaunchStatus::Unlaunched {
            continue;
        }
        let moved = initiate_docking(world, pod.port, pod.transit);
        info!("escape pod launched: {moved:?}");
        world.resource_mut::<EscapePods>().0[i].status = LaunchStatus::EndgameLaunched;
    }
}

/// `/obj/docking_port/mobile/pod/on_emergency_dock`: launched pods dock at CentCom.
pub fn on_emergency_dock(world: &mut World) {
    let Some(pods) = world.get_resource::<EscapePods>().map(|p| p.0.clone()) else {
        return;
    };
    for (i, pod) in pods.iter().enumerate() {
        if pod.status != LaunchStatus::EndgameLaunched {
            continue;
        }
        let moved = initiate_docking(world, pod.port, pod.away);
        info!("escape pod at CentCom: {moved:?}");
        world.resource_mut::<EscapePods>().0[i].status = LaunchStatus::Endgame;
    }
}

/// `pos` is on the CentCom stand-in of a pod that docked there.
pub fn at_centcom(world: &World, pos: ss13_shared::TilePos) -> bool {
    world.get_resource::<EscapePods>().is_some_and(|pods| {
        pods.0
            .iter()
            .any(|p| p.status == LaunchStatus::Endgame && p.level_z == pos.z)
    })
}

/// `/obj/docking_port/mobile/pod/request(dest)` from its console: red alert or an emagged
/// console, and only from the station (`UNLAUNCHED` becomes `EARLY_LAUNCHED`). A random dock is
/// placed first ([`place_random_dock`]).
pub fn request(
    world: &mut World,
    port: Entity,
    dest: Entity,
    emagged: bool,
) -> Result<(), &'static str> {
    if crate::security_level::current(world) < SecLevel::Red && !emagged {
        return Err(RED_ONLY);
    }
    let Some(i) = world
        .get_resource::<EscapePods>()
        .and_then(|pods| pods.0.iter().position(|p| p.port == port))
    else {
        return Err("Unable to comply.");
    };
    if world.resource::<EscapePods>().0[i].status != LaunchStatus::Unlaunched {
        return Err("Unable to comply.");
    }
    let random = world
        .get::<DmDockingPort>(dest)
        .is_some_and(|d| is_under(&d.path, RANDOM_DOCK));
    if random && !place_random_dock(world, port, dest) {
        warn!("escape pod {port}: no free space for its random dock {dest}");
        return Err("Unable to comply.");
    }
    world.resource_mut::<EscapePods>().0[i].status = LaunchStatus::EarlyLaunched;
    Ok(())
}

/// `/obj/docking_port/stationary/random/Initialize` without a lavaland level: moves `dock` to a
/// random spot of its level where the pod at `port` would cover only space, at least
/// `edge_distance` from the edges. False when no try fits.
pub fn place_random_dock(world: &mut World, port: Entity, dock: Entity) -> bool {
    let (Some(mobile), Some(stationary), Some(&at)) = (
        world.get::<MobileDock>(port).cloned(),
        world.get::<StationaryDock>(dock).cloned(),
        world.get::<TilePos>(dock),
    ) else {
        return false;
    };
    let Some((w, h)) = world.resource::<SpatialIndex>().level_size(at.z) else {
        return false;
    };
    let (w, h) = (w as i32, h as i32);
    if w < 2 * EDGE_DISTANCE || h < 2 * EDGE_DISTANCE {
        return false;
    }
    let space = crate::shuttle::area_named(world, DEFAULT_AREA);
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let mut rng = Rng::new(seed ^ 0x504f_445f ^ dock.to_bits());
    for _ in 0..PLACEMENT_TRIES {
        let pos = TilePos::new(
            rng.range(i64::from(EDGE_DISTANCE), i64::from(w + 1 - EDGE_DISTANCE)) as i32,
            rng.range(i64::from(EDGE_DISTANCE), i64::from(h + 1 - EDGE_DISTANCE)) as i32,
            at.z,
        );
        let fits = ordered_turfs(pos, stationary.dir, mobile.size)
            .into_iter()
            .all(|tile| open_space(world, tile, space));
        if fits {
            world.entity_mut(dock).insert(pos);
            info!("random dock {} placed at {pos:?}", stationary.id);
            return true;
        }
    }
    false
}

/// In bounds, in the space area, a space turf with nothing dense on it.
fn open_space(world: &World, tile: TilePos, space: Option<Entity>) -> bool {
    let index = world.resource::<SpatialIndex>();
    if !index.in_bounds(tile) || !index.dense_on_tile(tile).is_empty() {
        return false;
    }
    if space.is_some() && world.resource::<ss13_mapload::Areas>().area_at(tile) != space {
        return false;
    }
    crate::construction::turf_at(world, tile)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, SPACE_TURF))
}
