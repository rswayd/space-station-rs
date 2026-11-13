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
//! - BandaStation's CentCom shuttles (`modular_bandastation/shuttles`, the CentCom docks'
//!   `roundstart_template`s in `_maps/shuttles/ss220/`) load the same way: the Gamma armory
//!   (`gamma_away`, sent to the station's `gamma_home` by [`move_gamma`]), the special
//!   operations shuttle (`specops_trurl`) and the transport Argos (`argos_trurl`), the last two
//!   flown by their consoles and navigation computers with CentCom access.
//! - A template whose types were not converted fails to load: the white ship then tries the
//!   next one in the shuffled list, the ferry stays unloaded.
// shortcut: no space ruins, so the white ship's deep space dock is its own level (not the
// `whiteshipdock` ruin) and `whiteship_z4`/`whiteship_waystation` do not exist; no bridge
// console GPS ("Distress Signal": `crate::devices::gps` has no GPS on non-items); no
// `jam_on_wardec` (the navigation computer is not ported). The syndicate SIT and SST shuttles
// are not loaded (their home is the lazy `syndie_cc` template an admin team spawn brings; their
// consoles and navigation computers work once loaded), nor the Nanotrasen assault pod (the
// assault pod designator is not ported).

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

/// BandaStation's CentCom shuttles loaded at round start: `(template in _maps/shuttles/ss220,
/// shuttle id, CentCom dock id, dock name)`.
pub const FORK_SHUTTLES: [(&str, &str, &str, &str); 3] = [
    (
        "gamma_basic",
        "gamma",
        "gamma_away",
        "CentCom: Gamma Armory",
    ),
    (
        "specops_basic",
        "specops",
        "specops_trurl",
        "CentCom: Trurl",
    ),
    ("argos_basic", "argos", "argos_trurl", "CentCom: Trurl"),
];
/// The Gamma shuttle's docks (`gamma_away` at CentCom, `gamma_home` on stations that have one).
pub const GAMMA_ID: &str = "gamma";
pub const GAMMA_AWAY: &str = "gamma_away";
pub const GAMMA_HOME: &str = "gamma_home";

/// What loads at round start: the shuffled white ship candidates, the ferry template and the
/// fork's CentCom shuttles.
#[derive(Resource, Clone, Debug, Default)]
struct AwayTemplates {
    whiteships: Vec<PathBuf>,
    ferry: Option<PathBuf>,
    fork: Vec<(PathBuf, &'static str, &'static str, &'static str)>,
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
        let fork = FORK_SHUTTLES
            .iter()
            .map(|&(file, id, away, name)| {
                (
                    shuttle_template_path(&source, "ss220", file),
                    id,
                    away,
                    name,
                )
            })
            .filter(|(p, ..)| ss13_data::exists(p))
            .collect();
        app.insert_resource(AwayTemplates {
            whiteships,
            ferry,
            fork,
        })
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
    for (path, id, away, name) in &templates.fork {
        load(world, path, id, away, name);
    }
}

/// BandaStation's "Move Gamma Shuttle" (`admin_secrets.dm`, also offered by
/// `/datum/security_level/gamma/post_set_security_level`): at CentCom it flies to the station's
/// `gamma_home` with "К вам направлен..." announced, anywhere else back to `gamma_away`. `Err`
/// with tg's message when there is no Gamma shuttle or no station dock.
pub fn move_gamma(world: &mut World) -> Result<&'static str, String> {
    let Some(port) = crate::shuttle_nav::get_shuttle(world, GAMMA_ID) else {
        return Err("Шаттл «ГАММА» не найден (не заспавнен, удалён, или ошибка).".to_owned());
    };
    let dock = |world: &mut World, id: &str| {
        let mut q = world.query::<(Entity, &crate::shuttle::StationaryDock)>();
        q.iter(world).find(|(_, d)| d.id == id).map(|(e, _)| e)
    };
    let Some(home) = dock(world, GAMMA_HOME) else {
        return Err("На этой карте нет порта стыковки для оружейного шаттла «ГАММА».".to_owned());
    };
    let docked = world
        .get::<crate::shuttle::MobileDock>(port)
        .and_then(|m| m.docked);
    let at_centcom = docked
        .and_then(|d| world.get::<crate::shuttle::StationaryDock>(d))
        .is_some_and(|d| d.id == GAMMA_AWAY);
    let (dest, text) = if at_centcom {
        (home, "К вам направлен оружейный шаттл «ГАММА».")
    } else {
        let Some(away) = dock(world, GAMMA_AWAY) else {
            return Err("Шаттл «ГАММА» не найден (не заспавнен, удалён, или ошибка).".to_owned());
        };
        (away, "Оружейный шаттл «ГАММА» был отозван.")
    };
    if !crate::shuttle_nav::request(world, port, dest) {
        return Err("Шаттл «ГАММА» сейчас в пути.".to_owned());
    }
    crate::announce::Priority::new(
        text,
        format!(
            "{}: Департамент защиты активов",
            crate::events::COMMAND_NAME
        ),
    )
    .send(world);
    Ok(text)
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
