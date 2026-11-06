//! The mining, lavaland (common mining) and labour camp shuttles (server), after tg
//! `/obj/docking_port/stationary/mining_home` (`code/modules/mining/mine_items.dm`),
//! `/obj/docking_port/stationary/laborcamp_home` (`code/modules/mining/laborcamp/laborshuttle.dm`)
//! and `load_roundstart`.
//!
//! - Round start: each of these station docks loads its type's `roundstart_template`
//!   (MetaStation: `mining_delta`, `mining_common_meta`, `labour_delta`) and the shuttle docks
//!   there; consoles fly it ([`crate::shuttle_nav`]: `mining`, `mining/common`, `labor`,
//!   `labor/one_way`) to its dock on the mining level (`mining_away`, `lavaland_common_away`,
//!   `laborcamp_away` in `Lavaland.dmm`, [`ss13_mapload::MiningLevel`]). Without a mining level
//!   the shuttles stay at the station with no destination.
// shortcut: no labour camp point stacker recall (`laborstacker.dm` `moveShuttle`), no mining
// console exile implant or head revolutionary refusal, no `no_destination_swap`.

use std::path::PathBuf;

use bevy::prelude::*;

use crate::shuttle::{
    RoundstartPort, RoundstartPorts, dock_at_map_pos, initiate_docking, push_template,
    setup_template_copy,
};
use crate::shuttle_nav::NavShuttle;

const MOBILE_PORT: &str = "/obj/docking_port/mobile";

/// `(template port_id, mobile port shuttle_id)` of the shuttles loaded here.
pub const SHUTTLES: [(&str, &str); 3] = [
    ("mining", "mining"),
    ("mining_common", "mining_common"),
    ("labour", "laborcamp"),
];

#[derive(Resource)]
struct Templates(Vec<(RoundstartPort, PathBuf, &'static str)>);

pub struct MiningShuttlesPlugin;

impl Plugin for MiningShuttlesPlugin {
    fn build(&self, app: &mut App) {
        let Some(ports) = app.world().get_resource::<RoundstartPorts>().cloned() else {
            return;
        };
        let found: Vec<(RoundstartPort, PathBuf, &'static str)> = SHUTTLES
            .iter()
            .flat_map(|&(port_id, id)| {
                ports
                    .of(port_id)
                    .into_iter()
                    .map(move |(port, path)| (port, path, id))
            })
            .filter(|(_, path, _)| {
                let exists = ss13_data::exists(path);
                if !exists {
                    warn!("shuttle template {} is missing", path.display());
                }
                exists
            })
            .collect();
        if found.is_empty() {
            return;
        }
        for (_, path, _) in &found {
            push_template(app, path.clone());
        }
        app.insert_resource(Templates(found)).add_systems(
            Startup,
            setup_mining_shuttles.after(crate::shuttle::setup_shuttle),
        );
    }
}

/// tg `load_roundstart` of each dock: the template registers and docks there.
fn setup_mining_shuttles(world: &mut World) {
    let Some(Templates(templates)) = world.remove_resource::<Templates>() else {
        return;
    };
    for (k, (port, path, id)) in templates.iter().enumerate() {
        let nth = templates[..k].iter().filter(|(_, p, _)| p == path).count();
        let Some(station) = dock_at_map_pos(world, port.x, port.y, port.z) else {
            warn!(
                "{id} dock at {},{},{} is not a dock",
                port.x, port.y, port.z
            );
            continue;
        };
        let Some(shuttle) = setup_template_copy(
            world,
            path,
            nth,
            MOBILE_PORT,
            id,
            &format!("{id}_template"),
            "template",
        ) else {
            continue;
        };
        let moved = initiate_docking(world, shuttle.port, station);
        info!("{id} shuttle at the station: {moved:?}");
        if moved.is_err() {
            continue;
        }
        world
            .entity_mut(shuttle.port)
            .insert(NavShuttle::new(shuttle.transit));
    }
}
