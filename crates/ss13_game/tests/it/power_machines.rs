//! SMES building and shield generators (`ss13_game::power::{smes, shields}`) without clients.
//!
//! - On the power fixture room (`super::power`): a SMES with a terminal refuses the crowbar;
//!   with its panel open the terminal is cut away (the SMES stops working and no longer opens its
//!   window), then the crowbar is allowed.
//! - On a room of its own: two wired shield wall generators turned on fill the four tiles
//!   between them with shield walls, which go when one is turned off; an emergency shield
//!   projector needs anchoring and covers the space tiles around it with shields, dropped when
//!   it is switched off.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::construction::{Job, PanelOpen};
use ss13_game::power::shields::{self, EmergencyShield, ShieldGen, ShieldWall, ShieldWallGen};
use ss13_game::power::smes::{self, SmesJob};
use ss13_game::power::{Grid, Smes};
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const SMES: TilePos = TilePos::new(4, 5, 1);
const FLOOR: TilePos = TilePos::new(8, 3, 1);

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn find<T: Component>(app: &App, pos: TilePos) -> Entity {
    let world = app.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<T>(e).is_some())
        .unwrap_or_else(|| panic!("nothing with {} at {pos:?}", std::any::type_name::<T>()))
}

fn count<T: Component>(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&T>().iter(world).count()
}

#[test]
fn smes_terminal_is_cut_before_the_crowbar() {
    let assets = super::power::fixture_assets("power_machines_smes");
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    run(&mut app, 3.0);
    let smes_e = find::<Smes>(&app, SMES);
    let s = app.world().get::<Smes>(smes_e).unwrap().clone();
    assert_eq!(
        (s.input_level_max, s.output_level_max),
        (200_000.0, 200_000.0)
    );
    let terminal = s.terminal.expect("the mapped terminal");
    let world = app.world_mut();
    let mob = world.spawn(FLOOR).id();
    assert!(
        smes::crowbar_refused(world, Entity::PLACEHOLDER, smes_e),
        "a terminal is in the way"
    );

    // Closed panel: the cut does nothing.
    let cut = DoAfter {
        client: Entity::PLACEHOLDER,
        target: terminal,
        target_pos: TilePos::new(3, 5, 1),
        user_pos: FLOOR,
        item: None,
        amount: 0,
        ends_at: 0.0,
        job: Job::Smes(SmesJob::Cut),
    };
    smes::complete(world, mob, &cut, SmesJob::Cut, |_| true);
    assert!(world.get_entity(terminal).is_ok());

    // Open: the terminal is gone (a bare mob is never shocked), the SMES is broken.
    world.entity_mut(smes_e).insert(PanelOpen(true));
    smes::complete(world, mob, &cut, SmesJob::Cut, |_| true);
    assert!(world.get_entity(terminal).is_err());
    assert_eq!(world.get::<Smes>(smes_e).unwrap().terminal, None);
    assert_eq!(world.resource::<Grid>().node_net(terminal), None);
    assert!(
        smes::interact(world, Entity::PLACEHOLDER, mob, smes_e),
        "the click is the SMES's"
    );
    assert!(!smes::crowbar_refused(world, Entity::PLACEHOLDER, smes_e));
    // A broken SMES neither charges nor outputs.
    let before = world.get::<Smes>(smes_e).unwrap().charge;
    run(&mut app, 4.0);
    let after = app.world().get::<Smes>(smes_e).unwrap();
    assert_eq!(after.charge, before);
    assert!(!after.outputting);
}

const SHIELD_TYPES: &str = r##"
    "/obj/machinery/power/shieldwallgen": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "sg.dmi" }, "icon_state": "shield_wall_gen", "density": 1,
        "anchored": 0 } },
    "/obj/machinery/power/shieldwallgen/unlocked": { "parent": "/obj/machinery/power/shieldwallgen" },
    "/obj/machinery/power/shieldwallgen/unlocked/anchored": {
        "parent": "/obj/machinery/power/shieldwallgen/unlocked", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/shieldwall": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "e.dmi" }, "icon_state": "shieldwall", "density": 1, "use_power": 0 } },
    "/obj/machinery/shieldgen": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "sg.dmi" }, "icon_state": "shieldoff", "density": 1, "anchored": 0,
        "use_power": 0, "max_integrity": 100 } },
    "/obj/structure/emergency_shield": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "e.dmi" }, "icon_state": "shield-old", "density": 1, "anchored": 1,
        "max_integrity": 200 } },
"##;

/// Row 3 (y = 3): an RTG on a cable run through two shield wall generators (x = 4 and 9).
/// Row 2: a projector at x = 3 and three space tiles at x = 5..7.
const SHIELD_MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"R" = (/obj/structure/cable,/obj/machinery/power/rtg,/turf/open/floor,/area/station)
"c" = (/obj/structure/cable,/turf/open/floor,/area/station)
"G" = (/obj/structure/cable,/obj/machinery/power/shieldwallgen/unlocked/anchored,/turf/open/floor,/area/station)
"P" = (/obj/machinery/shieldgen,/turf/open/floor,/area/station)
"x" = (/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwwwwwwwwww
wRcGccccGffw
wfPfxxxffffw
wsfffffffffw
wwwwwwwwwwww
"}
"##;

const GEN_A: TilePos = TilePos::new(4, 4, 1);
const GEN_B: TilePos = TilePos::new(9, 4, 1);
const PROJECTOR: TilePos = TilePos::new(3, 3, 1);

fn shield_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("power_machines_fixture")
        .join(test);
    let types = super::power::TYPES.replacen(
        "    \"/mob\": {",
        &format!("{SHIELD_TYPES}    \"/mob\": {{"),
        1,
    );
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/power/rtg",
        "/obj/machinery/power/shieldwallgen/unlocked/anchored",
        "/obj/machinery/shieldwall",
        "/obj/machinery/shieldgen",
        "/obj/structure/emergency_shield",
        "/obj/structure/cable",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    let states = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("t.dmi", ""),
        ("l.dmi", "x2"),
        ("l.dmi", "Assistant"),
        ("h.dmi", "human_basic"),
        ("c.dmi", "l2-1-2-4-8-node"),
        ("smes.dmi", "rtg"),
        ("sg.dmi", "shield_wall_gen"),
        ("sg.dmi", "shield_wall_gen_on"),
        ("sg.dmi", "shieldoff"),
        ("sg.dmi", "shieldon"),
        ("e.dmi", "shieldwall"),
        ("e.dmi", "shield-old"),
    ];
    write_fixture(&dir, &types, &used, &states, &[("map.dmm", SHIELD_MAP)])
}

#[test]
fn shield_wall_generators_and_the_projector() {
    let assets = shield_assets("walls");
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |app| {
        app.add_plugins(ss13_atmos::AtmosPlugin);
    });
    app.update();
    run(&mut app, 3.0);
    let a = find::<ShieldWallGen>(&app, GEN_A);
    let b = find::<ShieldWallGen>(&app, GEN_B);
    {
        let grid = app.world().resource::<Grid>();
        assert!(grid.node_net(a).is_some() && grid.node_net(a) == grid.node_net(b));
    }
    let mob = app.world_mut().spawn(TilePos::new(5, 2, 1)).id();
    let client = Entity::PLACEHOLDER;

    // Both on: the four tiles between them get walls on the next machine ticks.
    assert!(shields::interact(app.world_mut(), client, mob, a));
    assert!(shields::interact(app.world_mut(), client, mob, b));
    run(&mut app, 5.0);
    assert!(app.world().get::<ShieldWallGen>(a).unwrap().on());
    let walls = {
        let world = app.world_mut();
        let mut q = world.query::<(&ShieldWall, &TilePos)>();
        let mut xs: Vec<i32> = q.iter(world).map(|(_, p)| p.x).collect();
        xs.sort_unstable();
        xs
    };
    assert_eq!(walls, vec![5, 6, 7, 8], "walls between the generators");
    // Not with it on: no wrench, no crowbar.
    assert!(shields::crowbar_blocked(app.world(), a));

    // One off: the walls go.
    assert!(shields::interact(app.world_mut(), client, mob, a));
    run(&mut app, 3.0);
    assert_eq!(count::<ShieldWall>(&mut app), 0);
    assert!(!app.world().get::<ShieldWallGen>(a).unwrap().on());

    // The projector: loose, it refuses; anchored, it shields the three space tiles in range.
    let p = find::<ShieldGen>(&app, PROJECTOR);
    assert!(shields::interact(app.world_mut(), client, mob, p));
    assert!(!app.world().get::<ShieldGen>(p).unwrap().active);
    ss13_game::obj_damage::set_anchored(app.world_mut(), p, true);
    assert!(shields::interact(app.world_mut(), client, mob, p));
    assert!(app.world().get::<ShieldGen>(p).unwrap().active);
    assert_eq!(count::<EmergencyShield>(&mut app), 3);
    // Again: nothing doubles up on a second switch-on.
    shields::shields_up(app.world_mut(), p);
    assert_eq!(count::<EmergencyShield>(&mut app), 3);
    assert!(shields::interact(app.world_mut(), client, mob, p));
    assert_eq!(count::<EmergencyShield>(&mut app), 0);
    // An emag shorts the lock for good.
    app.world_mut().get_mut::<ShieldGen>(p).unwrap().locked = true;
    assert!(shields::emag_act(app.world_mut(), client, p));
    assert!(!app.world().get::<ShieldGen>(p).unwrap().locked);
    assert!(!shields::emag_act(app.world_mut(), client, p));
}
