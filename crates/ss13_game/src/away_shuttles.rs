//! The white ship and the CentCom ferry (server), after tg
//! `/obj/docking_port/stationary/picked/whiteship` (`stationary_port/port_types.dm`), the
//! ferry dock's `json_key = "ferry"` (`_maps/map_files/generic/CentCom.dmm`), `load_roundstart`
//! and their consoles (`shuttle_consoles/white_ship.dm`, `mobile_port/variants/ferry.dm`).
//!
//! - Round start: the white ship is one of tg's `shuttlekeys` templates picked at random, the
//!   ferry is the map json's `"ferry"` template (MetaStation: `ferry_fancy`). Each loads onto its
//!   own level (`ss13_mapload::load_template`, initialized with the station at `PostStartup`),
//!   which stands in for its tg home: the deep space dock `whiteship_away` ("Deep Space") and the
//!   CentCom dock `ferry_away` ("CentCom Ferry Dock"). Both are flown by consoles
//!   ([`crate::shuttle_nav`]: `white_ship` to `whiteship_home` and the other white ship docks
//!   that exist, `ferry` to the station's `ferry_home` with CentCom access).
//! - A template whose types were not converted fails to load: the white ship then tries the
//!   next one in the shuffled list, the ferry stays unloaded.
// shortcut: no space ruins, so the white ship's deep space dock is its own level (not the
// `whiteshipdock` ruin) and `whiteship_z4`/`whiteship_waystation` do not exist; no bridge
// console GPS ("Distress Signal": `crate::devices::gps` has no GPS on non-items); no
// `jam_on_wardec` (the navigation computer is not ported).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_mapload::MapLoadPlugin;
use ss13_mapload::dock::{map_shuttle, shuttle_template_path};
use ss13_mapload::spawner::Rng;

use crate::shuttle::setup_template_shuttle;
use crate::shuttle_nav::NavShuttle;

const MOBILE_PORT: &str = "/obj/docking_port/mobile";

/// `/obj/docking_port/stationary/picked/whiteship` `shuttlekeys`.
pub const WHITESHIPS: [&str; 11] = [
    "whiteship_meta",
    "whiteship_pubby",
    "whiteship_box",
    "whiteship_cere",
    "whiteship_kilo",
    "whiteship_donut",
    "whiteship_delta",
    "whiteship_tram",
    "whiteship_personalshuttle",
    "whiteship_obelisk",
    "whiteship_birdshot",
];

/// What loads at round start: the shuffled white ship candidates and the ferry template.
#[derive(Resource, Clone, Debug, Default)]
struct AwayTemplates {
    whiteships: Vec<PathBuf>,
    ferry: Option<PathBuf>,
}

pub struct AwayShuttlesPlugin;

impl Plugin for AwayShuttlesPlugin {
    fn build(&self, app: &mut App) {
        let Some(map) = app.world().get_resource::<MapLoadPlugin>() else {
            return;
        };
        // Only a real station map: fixture maps name `.dmm` files.
        let Some(name) = map.maps.first().filter(|m| !m.ends_with(".dmm")).cloned() else {
            return;
        };
        let source = map.source.clone();
        let seed = app
            .world()
            .get_resource::<ss13_mapload::MapSeed>()
            .map_or(0, |s| s.0);
        let whiteships = whiteship_order(seed ^ 0x5748_4954)
            .into_iter()
            .map(|id| shuttle_template_path(&source, "whiteship", id))
            .filter(|p| ss13_data::exists(p))
            .collect();
        let ferry = map_shuttle(&source, &name, "ferry")
            .map(|id| shuttle_template_path(&source, "ferry", &id))
            .filter(|p| ss13_data::exists(p));
        app.insert_resource(AwayTemplates { whiteships, ferry })
            .add_systems(
                Startup,
                setup_away_shuttles.after(crate::shuttle::setup_shuttle),
            );
    }
}

/// [`WHITESHIPS`] in a random order (tg `pick(shuttlekeys)`, then the others as fallbacks).
pub fn whiteship_order(seed: u64) -> Vec<&'static str> {
    let mut rng = Rng::new(seed);
    let mut out = WHITESHIPS.to_vec();
    for i in (1..out.len()).rev() {
        let j = rng.range(0, i as i64) as usize;
        out.swap(i, j);
    }
    out
}

fn setup_away_shuttles(world: &mut World) {
    let Some(templates) = world.remove_resource::<AwayTemplates>() else {
        return;
    };
    for path in &templates.whiteships {
        if load(world, path, "whiteship", "whiteship_away", "Deep Space") {
            break;
        }
    }
    if let Some(path) = &templates.ferry {
        load(world, path, "ferry", "ferry_away", "CentCom Ferry Dock");
    }
}

/// tg `action_load(template, dock)`: the template on its own level, registered at its stand-in
/// dock and flown by consoles. The station's map init pass initializes what it brought.
fn load(world: &mut World, path: &Path, id: &str, away_id: &str, away_name: &str) -> bool {
    if let Err(e) = ss13_mapload::load_template(world, path) {
        warn!("{id} shuttle {}: {e:#}", path.display());
        return false;
    }
    let Some(shuttle) = setup_template_shuttle(world, path, MOBILE_PORT, id, away_id, away_name)
    else {
        return false;
    };
    info!(
        "{id} shuttle {} ({}) at z{}",
        shuttle.name,
        path.display(),
        shuttle.level_z
    );
    world
        .entity_mut(shuttle.port)
        .insert(NavShuttle::new(shuttle.transit));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whiteship_order_is_a_seeded_permutation() {
        let a = whiteship_order(7);
        let mut sorted = a.clone();
        sorted.sort_unstable();
        let mut all = WHITESHIPS.to_vec();
        all.sort_unstable();
        assert_eq!(sorted, all);
        assert_eq!(a, whiteship_order(7));
        assert!((0..16).any(|s| whiteship_order(s)[0] != a[0]));
    }
}
