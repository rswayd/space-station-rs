//! The paradox clone over real WebTransport on 127.0.0.1: two assistants join; B ghosts and opts
//! in to the paradox clone; an admin forces the "Paradox Clone" midround. B gets a new body on
//! the maintenance landmark: A's name and body, the assistant's outfit, a toolbox in hand, no
//! station job, the antag HUD icon and the objectives "nobody named A stays alive" (done once A
//! is dead) and "take A's place" (done while the clone lives). Nobody else wants it: the next
//! forced one fails. The round-end report lists the clone.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::antag::dynamic::{AntagPreferences, DevTraitor, DynamicConfig};
use ss13_game::antag::midround::{Midround, PARADOX_CLONE, force_midround};
use ss13_game::antag::objectives::RoundEnd;
use ss13_game::antag::paradox_clone::{
    CloneObjective, ParadoxClone, TOOLBOX, can_spawn, roundend_report,
};
use ss13_game::antag::{AntagHudData, ROLE_PARADOX_CLONE};
use ss13_game::ghost::{Ghost, ghostize_client};
use ss13_game::health::Stat;
use ss13_game::human::HumanLook;
use ss13_game::inventory::Inventory;
use ss13_game::jobs::{PlayerJob, fallback_jobs};
use ss13_game::server::Players;
use ss13_game::testing::{self, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, JobsData};
use ss13_shared::TilePos;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark",
        "vars": { "name": "start" }, "declared_vars": { "name": "start" } },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Assistant" }, "declared_vars": { "name": "Assistant" } },
    "/obj/effect/landmark/latejoin": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/generic_maintenance_landmark": { "parent": "/obj/effect/landmark" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/toolbox": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "toolbox" } },
    "/obj/item/storage/toolbox/mechanical": { "parent": "/obj/item/storage/toolbox",
        "declared_vars": { "name": "mechanical toolbox" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Assistant starts `s`, latejoin `L`, the maintenance landmark `m` (8,2): dmm rows run top down.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/space)
"m" = (/obj/effect/landmark/generic_maintenance_landmark,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wffffffffw
wsffsfffLw
wffffffmfw
wwwwwwwwww
"}
"##;

const MAINT: TilePos = TilePos::new(8, 2, 1);

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("icons/mob/huds/antag_hud.dmi", "paradox_clone"),
];

fn fixture_jobs() -> JobsData {
    let mut data = fallback_jobs();
    data.jobs[0].total_positions = 10;
    data.jobs[0].spawn_positions = 10;
    data
}

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("paradox_clone_fixture");
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = fixture_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/latejoin",
        "/obj/effect/landmark/generic_maintenance_landmark",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        TOOLBOX,
    ];
    used.extend(
        data.outfit_items()
            .into_iter()
            .filter(|p| types.types.contains_key(*p)),
    );
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    std::fs::write(dir.join(JOBS_FILE), data.to_ron()).unwrap();
    dir
}

#[test]
fn midround_paradox_clone_copies_a_crew_member() {
    let assets = fixture_assets();
    let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
        app.insert_resource(DynamicConfig {
            forced_rulesets: Some(Vec::new()),
            protect_roles: true,
            seed: Some(7),
            dev_traitor: DevTraitor::Off,
        })
        .add_plugins(ChatServerPlugin);
    });
    let a = s.join("p0", |_| {});
    let b = s.join("p1", |_| {});
    let (mob_a, link_b) = (s.players[a].mob, s.players[b].link);

    // Nobody wants to be a clone yet.
    assert!(force_midround(s.server.world_mut(), PARADOX_CLONE).is_err());

    // B ghosts and opts in.
    let world = s.server.world_mut();
    let ghost = ghostize_client(world, link_b).expect("B ghosts");
    world.entity_mut(link_b).insert(AntagPreferences {
        antag_roll: true,
        be_special: [ROLE_PARADOX_CLONE.to_owned()].into(),
    });
    // A is the only crew member left to clone, and there is a maintenance spawn.
    assert!(can_spawn(world));
    let picked = force_midround(world, PARADOX_CLONE).unwrap();
    assert_eq!(picked, [ghost]);
    assert!(
        world
            .resource::<Midround>()
            .executed
            .contains(&PARADOX_CLONE)
    );

    let clone = world.resource::<Players>().mob(link_b).unwrap();
    assert_ne!(clone, ghost);
    assert!(world.get::<Ghost>(clone).is_none());
    assert_eq!(world.get::<TilePos>(clone), Some(&MAINT));
    // A's name and body.
    let name_a = world.get::<Name>(mob_a).unwrap().to_string();
    assert_eq!(world.get::<Name>(clone).unwrap().as_str(), name_a);
    let (look_a, look_c) = (
        world.get::<HumanLook>(mob_a).unwrap().clone(),
        world.get::<HumanLook>(clone).unwrap().clone(),
    );
    assert_eq!(look_c.skin_tone, look_a.skin_tone);
    assert_eq!(look_c.hair_style, look_a.hair_style);
    assert_eq!(look_c.hair_color, look_a.hair_color);
    assert_eq!(look_c.eye_color, look_a.eye_color);
    assert_eq!(look_c.female, look_a.female);
    // Dressed as the assistant, the toolbox in hand, but no station job.
    let inventory = world.get::<Inventory>(clone).unwrap().clone();
    assert!(inventory.uniform.is_some());
    assert!(
        inventory
            .hands
            .iter()
            .flatten()
            .any(|&h| { world.get::<DmItem>(h).is_some_and(|i| i.path == TOOLBOX) })
    );
    assert!(world.get::<PlayerJob>(clone).is_none());
    assert!(world.get::<AntagHudData>(clone).unwrap().hud_icon.is_some());

    let data = world.get::<ParadoxClone>(clone).unwrap().clone();
    assert_eq!(data.original, mob_a);
    assert_eq!(
        data.objectives[0],
        CloneObjective::AcceptNoSubstitutes {
            target_name: name_a.clone()
        }
    );
    assert!(matches!(data.objectives[1], CloneObjective::Replace { .. }));
    // A lives: the clone has not won yet; A dies: both are done.
    assert!(!data.objectives[0].check_completion(world, clone));
    assert!(data.objectives[1].check_completion(world, clone));
    world.entity_mut(mob_a).insert(Stat::Dead);
    assert!(data.objectives[0].check_completion(world, clone));

    // The only willing ghost is a clone now.
    assert!(force_midround(world, PARADOX_CLONE).is_err());

    let report: Vec<String> = roundend_report(world, RoundEnd::default())
        .iter()
        .map(|m| m.plain_text())
        .collect();
    assert!(
        report.iter().any(|l| l.contains("Парадокс-клоны были:")),
        "{report:?}"
    );
    assert!(report.iter().any(|l| l.contains("Успех!")), "{report:?}");
}
