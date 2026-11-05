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
// shortcut: no pod console (`/obj/machinery/computer/shuttle/pod`): its red-alert or emagged
// early launch lands on lavaland (`pod_lavaland` random docks), and no lavaland level is loaded.
// No pod storage lock (`/datum/storage/pod`), no station traits for cramped or luxury pods.

use std::path::PathBuf;

use bevy::prelude::*;

use crate::shuttle::{
    RoundstartPort, RoundstartPorts, dock_at_map_pos, initiate_docking, push_template,
    setup_template_copy,
};

const POD_PORT: &str = "/obj/docking_port/mobile/pod";

/// tg `launch_status` of a pod.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaunchStatus {
    /// At the station (`UNLAUNCHED`).
    #[default]
    Unlaunched,
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
