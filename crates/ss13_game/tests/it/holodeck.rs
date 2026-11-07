//! The holodeck (`ss13_game::holodeck`) on the power fixture station with a 4x4 holodeck room
//! east of it: the console links the room, loads a program after its cooldown (its template's
//! holo floors and a hologram dodgeball with stamina damage), refuses restricted programs until
//! emagged, derezzes a hologram that left the room; holo floors take no items; a dodgeball
//! hit costs a human stamina.

use std::path::PathBuf;

use bevy::prelude::*;
use ss13_game::click_handlers::{self, ClickCtx, ClickPhase};
use ss13_game::holodeck::{self, Holodeck, HolodeckAct, Hologram};
use ss13_game::status_effects::StatusEffects;
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_mapload::{DmItem, DmTurfBuild, Turf};
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
use ss13_world::SpatialIndex;

const CONSOLE: TilePos = TilePos::new(3, 2, 1);
const ROOM: TilePos = TilePos::new(14, 2, 1);

const HOLO_TYPES: &str = r##"
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "m.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/holodeck": { "parent": "/obj/machinery/computer" },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "w_class": 1 } },
    "/obj/item/toy/dodgeball": { "parent": "/obj/item/toy", "declared_vars": {
        "icon_state": "dodgeball", "damtype": "brute", "w_class": 4 } },
    "/turf/open/floor/holofloor": { "parent": "/turf/open/floor" },
    "/turf/open/floor/holofloor/grass": { "parent": "/turf/open/floor/holofloor",
        "declared_vars": { "icon_state": "grass0" } },
    "/area/station/holodeck": { "parent": "/area/station" },
    "/area/station/holodeck/rec_center": { "parent": "/area/station/holodeck" },
    "/mob": {"##;

/// The power station, a console (`H`) and the holodeck room (`h`, offline holo floors).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"R" = (/obj/structure/cable,/obj/machinery/power/rtg,/turf/open/floor,/area/station)
"T" = (/obj/structure/cable,/obj/machinery/power/terminal{dir = 4},/turf/open/floor,/area/station)
"S" = (/obj/structure/cable,/obj/machinery/power/smes/engineering,/turf/open/floor,/area/station)
"c" = (/obj/structure/cable,/turf/open/floor,/area/station)
"A" = (/obj/structure/cable,/obj/machinery/power/apc{dir = 1; pixel_y = 25},/turf/open/floor,/area/station)
"l" = (/obj/machinery/light{dir = 1},/turf/open/floor,/area/station)
"n" = (/obj/machinery/light{dir = 1; no_low_power = 1},/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock{normalspeed = 0},/turf/open/floor,/area/station)
"H" = (/obj/machinery/computer/holodeck,/turf/open/floor,/area/station)
"h" = (/turf/open/floor/holofloor,/area/station/holodeck/rec_center)

(1,1,1) = {"
wwwwwwwwwwwwwwwwww
wRTSccAffffwwhhhhw
wfffflfffnfwwhhhhw
wfsfDffffffwwhhhhw
wfHffffffffwwhhhhw
wwwwwwwwwwwwwwwwww
"}
"##;

/// A grass court with a dodgeball in its bottom-left corner.
const COURT: &str = r##""g" = (/turf/open/floor/holofloor/grass,/area/template_noop)
"b" = (/obj/item/toy/dodgeball,/turf/open/floor/holofloor/grass,/area/template_noop)

(1,1,1) = {"
gggg
gggg
gggg
bggg
"}
"##;

fn fixture() -> PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("holodeck_fixture");
    std::fs::create_dir_all(dir.join("_maps/templates")).unwrap();
    let types = crate::power::TYPES.replacen(r#"    "/mob": {"#, HOLO_TYPES, 1);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/door/airlock",
        "/obj/machinery/light",
        "/obj/machinery/power/apc",
        "/obj/machinery/power/smes/engineering",
        "/obj/machinery/power/terminal",
        "/obj/machinery/power/rtg",
        "/obj/machinery/computer/holodeck",
        "/obj/structure/cable",
        "/obj/item/toy/dodgeball",
        "/turf/open/floor",
        "/turf/open/floor/holofloor",
        "/turf/open/floor/holofloor/grass",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/station/holodeck/rec_center",
    ];
    let states = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("t.dmi", ""),
        ("t.dmi", "grass0"),
        ("t.dmi", "grass1"),
        ("t.dmi", "grass2"),
        ("t.dmi", "grass3"),
        ("l.dmi", "x2"),
        ("l.dmi", "Assistant"),
        ("h.dmi", "human_basic"),
        ("a.dmi", "closed"),
        ("a.dmi", "opening"),
        ("a.dmi", "open"),
        ("a.dmi", "closing"),
        ("light.dmi", "tube"),
        ("light.dmi", "tube_emergency"),
        ("apc.dmi", "apc0"),
        ("m.dmi", "computer"),
        ("i.dmi", "toy"),
        ("i.dmi", "dodgeball"),
    ];
    write_fixture(
        &dir,
        &types,
        &used,
        &states,
        &[
            ("map.dmm", MAP),
            ("_maps/templates/holodeck_emptycourt.dmm", COURT),
        ],
    )
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn console(world: &mut World) -> Entity {
    let mut q = world.query::<(Entity, &Holodeck)>();
    q.iter(world).next().expect("the console linked its room").0
}

fn ball_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<DmItem>(e).is_some())
}

fn turf_path(world: &World, pos: TilePos) -> String {
    let turf = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Turf>(e).is_some())
        .unwrap();
    world.get::<DmTurfBuild>(turf).unwrap().path.clone()
}

#[test]
fn console_loads_programs_and_emag_unlocks_the_restricted_ones() {
    let mut st = testing::Station::start(fixture_map(&fixture(), &["map.dmm"]), |_| {});
    let p = st.join("a", |_| {});
    let (client, mob) = (st.players[p].link, st.players[p].mob);
    let app = &mut st.server;
    let world = app.world_mut();
    let deck = console(world);
    let h = world.get::<Holodeck>(deck).unwrap();
    assert_eq!(h.bottom_left, ROOM);
    assert_eq!(h.tiles.len(), 16);
    assert_eq!(h.program, holodeck::OFFLINE);

    // The cooldown of the round start.
    holodeck::ui_act(
        world,
        deck,
        client,
        mob,
        HolodeckAct::Load("holodeck_emptycourt".into()),
    );
    assert_eq!(
        world.get::<Holodeck>(deck).unwrap().program,
        holodeck::OFFLINE
    );
    run(app, holodeck::COOLDOWN + 0.5);
    let world = app.world_mut();
    holodeck::ui_act(
        world,
        deck,
        client,
        mob,
        HolodeckAct::Load("holodeck_emptycourt".into()),
    );
    assert_eq!(
        world.get::<Holodeck>(deck).unwrap().program,
        "holodeck_emptycourt"
    );
    assert_eq!(turf_path(world, ROOM), "/turf/open/floor/holofloor/grass");
    let ball = ball_at(world, ROOM).expect("the court's dodgeball");
    assert!(world.get::<Hologram>(ball).is_some());
    assert_eq!(world.get::<DmItem>(ball).unwrap().damtype, "stamina");
    assert!(world.get::<Holodeck>(deck).unwrap().spawned.contains(&ball));

    // Holo floors take no items.
    let turf = world
        .resource::<SpatialIndex>()
        .on_tile(TilePos::new(15, 3, 1))
        .iter()
        .copied()
        .find(|&e| world.get::<Turf>(e).is_some())
        .unwrap();
    let ctx = ClickCtx {
        client,
        mob,
        mob_pos: TilePos::new(15, 2, 1),
        held: Some(ball),
        target: turf,
        pos: TilePos::new(15, 3, 1),
        modifiers: ClickModifiers::default(),
        active_hand: 0,
    };
    assert!(click_handlers::run(world, ClickPhase::HeldOnTarget, &ctx));

    // A dodgeball hit costs a human stamina.
    let before = world.get::<StatusEffects>(mob).map_or(0.0, |s| s.stamina);
    holodeck::dodgeball_hit(world, ball, mob);
    let after = world.get::<StatusEffects>(mob).map_or(0.0, |s| s.stamina);
    assert!(after > before, "stamina {before} -> {after}");

    // Restricted programs need the emag.
    run(app, holodeck::COOLDOWN + 0.5);
    let world = app.world_mut();
    holodeck::ui_act(
        world,
        deck,
        client,
        mob,
        HolodeckAct::Load("holodeck_burntest".into()),
    );
    assert_eq!(
        world.get::<Holodeck>(deck).unwrap().program,
        "holodeck_emptycourt"
    );

    // A hologram carried out of the room fades on the next machine tick.
    world.entity_mut(ball).insert(TilePos::new(8, 3, 1));
    run(app, 2.5);
    let world = app.world_mut();
    assert!(world.get_entity(ball).is_err(), "derezzed outside the room");

    // The emag: restricted programs are offered.
    assert!(holodeck::emag_act(world, client, deck));
    assert!(!holodeck::emag_act(world, client, deck), "once");
    assert!(world.get::<Holodeck>(deck).unwrap().emagged);
    holodeck::ui_act(
        world,
        deck,
        client,
        mob,
        HolodeckAct::Load("holodeck_burntest".into()),
    );
    assert_eq!(
        world.get::<Holodeck>(deck).unwrap().program,
        "holodeck_burntest"
    );
}

#[test]
fn emagged_holograms_keep_their_real_damage() {
    let mut app = testing::server_app(fixture_map(&fixture(), &["map.dmm"]), |_| {});
    run(&mut app, holodeck::COOLDOWN + 0.5);
    let world = app.world_mut();
    let deck = console(world);
    let user = world.spawn_empty().id();
    assert!(holodeck::load_program(
        world,
        deck,
        "holodeck_emptycourt",
        false
    ));
    let ball = ball_at(world, ROOM).unwrap();
    assert_eq!(world.get::<DmItem>(ball).unwrap().damtype, "stamina");
    assert!(holodeck::emag_act(world, user, deck));
    assert_eq!(world.get::<DmItem>(ball).unwrap().damtype, "brute");
    // Emagged, holograms may leave the room.
    world.entity_mut(ball).insert(TilePos::new(8, 3, 1));
    run(&mut app, 2.5);
    assert!(app.world().get_entity(ball).is_ok());
}
