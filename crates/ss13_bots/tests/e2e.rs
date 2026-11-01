//! The runner end to end on fixture assets (no generated assets needed): two cases on one pooled
//! server (each on its own level, run one after the other on the same worker), real bots over
//! real WebTransport; one case fails on purpose and must say where and why.

use std::path::{Path, PathBuf};

use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_bots::case;
use ss13_bots::pool::{RunConfig, run_all};
use ss13_mapload::convert::{DmTypes, convert};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 2, "max_total_storage": 14 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": { "name": "toy", "w_class": 1 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
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

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "card_grey"),
];

const STAGING: &str = r#""w" = (/turf/closed/wall,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wsssw
wwwww
"}
"#;

const ROOM: &str = r#""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"t" = (/obj/item/toy,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwwwwwwwwwww
wfffftfffffffffffffw
wffffffffffffffffffw
wwwwwwwwwwwwwwwwwwww
"}
"#;

/// Walks, picks up the toy at (6,3), talks within earshot of bob but not of carol.
const PLAY: &str = r#"
map room.dmm
bot alice 2,3
bot bob 3,2
bot carol 18,2

test walk_and_pick_up
  alice move E E
  expect alice.pos == 4,3
  expect mob(alice).pos == 4,3
  alice move E
  alice click 6,3 /obj/item/toy
  until mob(alice).held ~ /obj/item/toy
  until alice.hands[0] == toy
  expect tile(6,3).entities[].path !~ toy

test say_is_heard_nearby_only
  bob say "hello there"
  until alice.chat ~ "ello there"
  wait 0.5
  expect carol.chat !~ "ello there"
"#;

/// The same map on the next level of the same server: the toy is still there; then a step that
/// must fail.
const AFTER: &str = r#"
map room.dmm
bot dave 2,3

test fresh_level
  expect tile(6,3).entities[].path ~ /obj/item/toy
  expect dave.chat_all !~ "ello there"

test fails_on_purpose
  dave move N
  expect dave.pos == 2,4
"#;

fn fixture() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bots_e2e");
    std::fs::create_dir_all(dir.join("assets/prototypes")).unwrap();
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/item/toy",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    used.extend(ss13_mapload::RUNTIME_TYPES);
    let conversion = convert(&types, used).unwrap();
    std::fs::write(dir.join("assets/prototypes/dm_types.ron"), conversion.ron).unwrap();
    let manifest = AssetManifest {
        atlases: vec![],
        states: STATES
            .iter()
            .map(|&(dmi, state)| IconStateInfo {
                dmi: dmi.into(),
                state: state.into(),
                atlas: 0,
                width: 32,
                height: 32,
                dirs: 1,
                rects: vec![[0, 0, 32, 32]],
                delays: vec![],
                loop_count: 0,
                rewind: false,
                movement: false,
            })
            .collect(),
    };
    std::fs::write(
        dir.join("assets").join(ss13_assets::import::MANIFEST_FILE),
        postcard::to_stdvec(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cases")).unwrap();
    std::fs::write(dir.join("cases/room.dmm"), ROOM).unwrap();
    std::fs::write(dir.join("cases/a_play.case"), PLAY).unwrap();
    std::fs::write(dir.join("cases/b_after.case"), AFTER).unwrap();
    dir
}

#[test]
fn pooled_cases_run_on_fresh_levels_and_failures_say_why() {
    ss13_bots::logcap::install();
    let dir = fixture();
    let files = case::discover(&[dir.join("cases")]).unwrap();
    let cases: Vec<_> = files.iter().map(|f| case::parse_file(f).unwrap()).collect();
    assert_eq!(cases.len(), 2);
    let config = RunConfig {
        jobs: 1,
        assets: dir.join("assets"),
        source: dir.clone(),
        out: dir.join("out"),
        staging: STAGING.into(),
    };
    let results = run_all(&cases, &config, &|r| {
        eprintln!("{}", ss13_bots::report::progress_line(r))
    })
    .unwrap();
    let [play, after] = &results[..] else {
        panic!("{results:?}");
    };
    assert!(play.passed(), "{play:?}");
    assert_eq!(play.tests.len(), 2);
    assert_eq!((play.worker, after.worker), (0, 0), "one pooled server");
    assert!(after.error.is_none(), "{after:?}");
    assert_eq!(after.tests[0].failure, None, "a fresh level: {after:?}");
    let failure = after.tests[1].failure.as_deref().expect("fails on purpose");
    assert!(
        failure.starts_with(
            "line 11 `expect dave.pos == 2,4`: expected `dave.pos == 2,4`; actual: [2,3]"
        ),
        "{failure}"
    );
    assert!(failure.contains("last chat of dave"), "{failure}");
    let log = std::fs::read_to_string(&after.log).unwrap();
    assert!(log.contains("FAIL   11 expect dave.pos == 2,4"), "{log}");
    let junit = ss13_bots::report::junit(&results);
    assert!(junit.contains("failures=\"1\""), "{junit}");
}
