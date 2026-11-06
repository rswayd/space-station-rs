//! Engineering machines of `ss13_game::eng_machines` on a fixture floor, server side: machines
//! and items are spawned directly and the modules' procs called with a stand-in client.

use std::path::Path;

use bevy::prelude::*;
use ss13_game::eng_machines::atmos_shield::{self, AtmosShield, AtmosShieldGen, GenState};
use ss13_game::eng_machines::sheetifier::{self, MEAT_SHEET, PROCESS_SECS, Sheetifier};
use ss13_game::testing::{fixture_map, server_app, steps, write_fixture};
use ss13_game::tools::Stack;
use ss13_mapload::{DmFood, DmItem, DmObj};
use ss13_shared::TilePos;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "flags_1": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": { "interaction_flags_item": 1 } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/meat": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "meat", "merge_type": { "path": "/obj/item/stack/sheet/meat" } } },
    "/obj/item/circuitboard": { "parent": "/obj/item", "declared_vars": { "build_path": null } },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard",
        "declared_vars": { "needs_anchored": 1, "req_components": null,
        "def_components": null } },
    "/obj/item/circuitboard/machine/sheetifier": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "name": "Sheet-meister 2000",
        "build_path": { "path": "/obj/machinery/sheetifier" }, "needs_anchored": 0 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "density": 1 } },
    "/obj/machinery/sheetifier": { "parent": "/obj/machinery", "declared_vars": {
        "name": "Sheet-meister 2000",
        "circuit": { "path": "/obj/item/circuitboard/machine/sheetifier" } } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing" },
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
    "/area": { "parent": "/atom", "declared_vars": { "area_flags": 0, "requires_power": 0 } },
    "/area/space": { "parent": "/area" }
  }
}"##;

const MAP: &str = r##""f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
ffffff
ffffff
sfffff
"}
"##;

const STATES: [(&str, &str); 3] = [
    ("t.dmi", "floor"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
];

fn fixture(test: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("eng_machines_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/stack/sheet/meat",
        "/obj/item/circuitboard/machine/sheetifier",
        "/obj/machinery/sheetifier",
        "/turf/open/floor",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    server_app(fixture_map(&assets, &["map.dmm"]), |_| {})
}

fn t(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y, 1)
}

fn obj(app: &mut App, path: &str, pos: TilePos) -> Entity {
    app.world_mut()
        .spawn((
            pos,
            DmObj {
                path: path.into(),
                anchored: true,
                ..default()
            },
        ))
        .id()
}

fn meat(app: &mut App, path: &str, sheets: f32) -> Entity {
    app.world_mut()
        .spawn((
            DmItem {
                path: path.into(),
                ..default()
            },
            DmFood {
                meat_sheets: sheets,
                ..default()
            },
        ))
        .id()
}

fn meat_sheets_on(app: &mut App, pos: TilePos) -> u32 {
    let world = app.world_mut();
    let mut q = world.query::<(&DmItem, &TilePos, &Stack)>();
    q.iter(world)
        .filter(|(i, p, _)| i.path == MEAT_SHEET && **p == pos)
        .map(|(_, _, s)| s.amount)
        .sum()
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

#[test]
fn sheetifier_turns_meat_into_sheets_after_processing() {
    let mut app = fixture("sheetifier");
    let machine = obj(&mut app, sheetifier::SHEETIFIER, t(3, 2));
    sheetifier::setup(app.world_mut(), machine);
    let client = app.world_mut().spawn_empty().id();
    let slab = meat(&mut app, "/obj/item/food/meat/slab", 4.0);
    let cutlet = meat(&mut app, "/obj/item/food/meat/cutlet", 4.0 / 3.0);
    let plant = meat(&mut app, "/obj/item/food/meat/slab/human/mutant/plant", 0.0);
    let burger = meat(&mut app, "/obj/item/food/burger", 1.0);

    // A left click with a non-stack is not an insert (`on_item_insert`).
    let w = app.world_mut();
    assert!(!sheetifier::item_interaction(
        w, client, client, slab, machine, false
    ));
    // Not meat: rejected and kept; meat without meat material: kept.
    assert!(sheetifier::item_interaction(
        w, client, client, burger, machine, true
    ));
    assert!(sheetifier::item_interaction(
        w, client, client, plant, machine, true
    ));
    assert!(w.get_entity(burger).is_ok() && w.get_entity(plant).is_ok());

    assert!(sheetifier::item_interaction(
        w, client, client, slab, machine, true
    ));
    assert!(w.get_entity(slab).is_err(), "the slab is consumed");
    let s = *w.get::<Sheetifier>(machine).unwrap();
    assert_eq!(s.meat, 4.0);
    assert!(s.busy_until.is_some());
    // Busy: the cutlet is blocked and stays.
    assert!(sheetifier::item_interaction(
        w, client, client, cutlet, machine, true
    ));
    assert!(w.get_entity(cutlet).is_ok());

    run(&mut app, PROCESS_SECS - 0.5);
    assert_eq!(meat_sheets_on(&mut app, t(3, 2)), 0, "still processing");
    run(&mut app, 1.0);
    assert_eq!(meat_sheets_on(&mut app, t(3, 2)), 4);
    let s = *app.world().get::<Sheetifier>(machine).unwrap();
    assert!(s.busy_until.is_none() && s.meat == 0.0);

    // Four thirds of a sheet: one comes out, the third stays inside.
    let w = app.world_mut();
    assert!(sheetifier::item_interaction(
        w, client, client, cutlet, machine, true
    ));
    run(&mut app, PROCESS_SECS + 0.5);
    assert_eq!(meat_sheets_on(&mut app, t(3, 2)), 5);
    let left = app.world().get::<Sheetifier>(machine).unwrap().meat;
    assert!((left - 1.0 / 3.0).abs() < 1e-3, "{left}");
}

fn generator(app: &mut App, path: &str, pos: TilePos, dir: ss13_shared::Dir) -> Entity {
    let e = obj(app, path, pos);
    app.world_mut().entity_mut(e).insert((
        ss13_shared::Facing(dir),
        ss13_game::obj_damage::Anchored(true),
    ));
    atmos_shield::setup(app.world_mut(), e);
    e
}

fn gen_state(app: &App, e: Entity) -> AtmosShieldGen {
    app.world().get::<AtmosShieldGen>(e).unwrap().clone()
}

#[test]
fn atmos_shield_generators_span_an_airtight_field_between_them() {
    use ss13_shared::Dir;
    let mut app = fixture("atmos_shield");
    let a = generator(&mut app, atmos_shield::GENERATOR, t(1, 2), Dir::East);
    let b = generator(&mut app, atmos_shield::GENERATOR, t(3, 2), Dir::West);
    let mapped = generator(
        &mut app,
        "/obj/machinery/atmos_shield_gen/active",
        t(6, 1),
        Dir::North,
    );
    let client = app.world_mut().spawn_empty().id();
    // `/active` maps in waiting for power, locked.
    let m = gen_state(&app, mapped);
    assert_eq!((m.on, m.locked), (GenState::WantPower, true));
    // No parts: tier 1 laser and capacitor.
    assert_eq!(atmos_shield::ratings(app.world(), a), (3, 1000.0));

    // No access: the lock stays.
    assert!(atmos_shield::interact(
        app.world_mut(),
        client,
        client,
        b,
        true
    ));
    assert!(!gen_state(&app, b).locked);

    assert!(atmos_shield::interact(
        app.world_mut(),
        client,
        client,
        a,
        false
    ));
    assert_eq!(gen_state(&app, a).on, GenState::WantPower);
    atmos_shield::process_early(app.world_mut(), a);
    let (sa, sb) = (gen_state(&app, a), gen_state(&app, b));
    assert_eq!((sa.on, sb.on), (GenState::Active, GenState::Active));
    assert_eq!(sb.master, Some(a));
    // Both ends and the tile between.
    assert_eq!(sa.fields.len(), 3);
    let mut tiles: Vec<TilePos> = sa
        .fields
        .iter()
        .map(|&f| *app.world().get::<TilePos>(f).unwrap())
        .collect();
    tiles.sort_by_key(|p| p.x);
    assert_eq!(tiles, vec![t(1, 2), t(2, 2), t(3, 2)]);
    for &f in &sa.fields {
        assert_eq!(
            app.world().get::<ss13_atmos::AtmosBlocker>(f).map(|b| b.0),
            Some(ss13_atmos::AtmosPass::No)
        );
        assert!(
            app.world()
                .get::<AtmosShield>(f)
                .is_some_and(|s| s.owner == a)
        );
    }
    // Active with fields: no new search, the fields stay.
    atmos_shield::process_early(app.world_mut(), a);
    assert_eq!(gen_state(&app, a).fields, sa.fields);

    // Power lost: waiting again, the fields go on the next tick.
    app.world_mut()
        .entity_mut(a)
        .insert(ss13_game::power::Unpowered);
    atmos_shield::process_early(app.world_mut(), a);
    assert_eq!(gen_state(&app, a).on, GenState::WantPower);
    atmos_shield::process_early(app.world_mut(), a);
    assert!(gen_state(&app, a).fields.is_empty());
    assert!(
        sa.fields
            .iter()
            .all(|&f| app.world().get_entity(f).is_err())
    );
    // The slave follows its inactive master off.
    atmos_shield::process_early(app.world_mut(), b);
    let sb = gen_state(&app, b);
    assert_eq!((sb.on, sb.master), (GenState::Off, None));

    // Power back: the pair forms again; turned off by hand, the fields go at once.
    app.world_mut()
        .entity_mut(a)
        .remove::<ss13_game::power::Unpowered>();
    atmos_shield::process_early(app.world_mut(), a);
    assert_eq!(gen_state(&app, a).fields.len(), 3);
    atmos_shield::toggle(app.world_mut(), a);
    assert_eq!(gen_state(&app, a).on, GenState::Off);
    assert!(gen_state(&app, a).fields.is_empty());

    // Facing away: no partner, it keeps waiting.
    app.world_mut()
        .entity_mut(b)
        .insert(ss13_shared::Facing(Dir::North));
    atmos_shield::process_early(app.world_mut(), b);
    atmos_shield::toggle(app.world_mut(), a);
    atmos_shield::process_early(app.world_mut(), a);
    let sa = gen_state(&app, a);
    assert_eq!((sa.on, sa.fields.len()), (GenState::WantPower, 0));
}

#[test]
fn flatpacker_packs_a_board_and_the_flatpack_deploys() {
    use ss13_game::eng_machines::flatpacker::{self, Flatpack, Flatpacker};
    use ss13_game::science::lathe::MaterialStore;
    use ss13_game::science::techweb::ResearchDb;
    use ss13_mapload::research::{MaterialDef, ResearchData};

    let mut app = fixture("flatpacker");
    let mat = |id: &str| MaterialDef {
        id: id.into(),
        name: id.rsplit('/').next().unwrap().into(),
        ..default()
    };
    app.world_mut()
        .insert_resource(ResearchDb::new(ResearchData {
            materials: vec![mat("/datum/material/iron"), mat("/datum/material/glass")],
            ..default()
        }));
    let machine = obj(&mut app, flatpacker::FLATPACKER, t(3, 2));
    flatpacker::setup(app.world_mut(), machine);
    let client = app.world_mut().spawn_empty().id();
    let board = ss13_game::construction::spawn_obj(
        app.world_mut(),
        "/obj/item/circuitboard/machine/sheetifier",
        t(2, 2),
    )
    .expect("board prototype");

    let w = app.world_mut();
    flatpacker::insert_board(w, client, client, board, machine);
    let f = w.get::<Flatpacker>(machine).unwrap().clone();
    assert_eq!(f.board, Some(board));
    // No parts: only the frame and cable (iron 5.05 sheets, glass 0.05), at efficiency 2.
    assert_eq!(f.needed, vec![(0, 505.0), (1, 5.0)]);
    assert_eq!(f.efficiency, 2.0);
    assert!(!flatpacker::start_build(w, machine), "no materials");

    w.get_mut::<MaterialStore>(machine).unwrap().amounts = vec![2000, 100];
    assert!(flatpacker::start_build(w, machine));
    assert!(!flatpacker::start_build(w, machine), "busy");
    run(&mut app, flatpacker::BASE_SECS);
    let world = app.world_mut();
    assert!(
        world
            .get::<Flatpacker>(machine)
            .unwrap()
            .busy_until
            .is_none()
    );
    assert_eq!(
        world.get::<MaterialStore>(machine).unwrap().amounts,
        vec![990, 90]
    );
    let packs: Vec<(Entity, Flatpack)> = world
        .query::<(Entity, &Flatpack)>()
        .iter(world)
        .map(|(e, f)| (e, *f))
        .collect();
    assert_eq!(packs.len(), 1);
    let (pack, packed) = packs[0];
    assert_eq!(packed.board, board);
    assert!(world.get::<Flatpacker>(machine).unwrap().board.is_none());

    // Moved off the packer, a multitool deploys it into the board's machine.
    ss13_game::storage::take_out(world, pack, t(5, 2));
    assert!(flatpacker::deploy(world, client, pack));
    assert!(world.get_entity(pack).is_err());
    let built: Vec<TilePos> = world
        .query::<(&Sheetifier, &TilePos)>()
        .iter(world)
        .map(|(_, p)| *p)
        .collect();
    assert_eq!(built, vec![t(5, 2)]);
}

#[test]
fn modular_shield_charges_raises_a_ring_and_fails_when_drained() {
    use ss13_game::eng_machines::modular_shield::{
        self, ModularShield, ShieldGenerator, ShieldModule,
    };
    use ss13_shared::{Dir, Facing};

    let mut app = fixture("modular_shield");
    let g = obj(&mut app, modular_shield::GENERATOR, t(3, 2));
    modular_shield::setup(app.world_mut(), g);
    let state = |app: &App| app.world().get::<ShieldGenerator>(g).unwrap().clone();
    // No parts: 40 strength, 3 regen, radius max(3, 3 / 2); recovering from 0 at a quarter.
    let s = state(&app);
    assert_eq!(
        (s.max_strength, s.max_regeneration, s.max_radius),
        (40.0, 3.0, 3.0)
    );
    assert_eq!((s.stored_strength, s.current_regeneration), (0.0, 0.75));
    assert!(s.recovering);
    modular_shield::toggle(app.world_mut(), g);
    assert!(!state(&app).active, "recovering: it does not start");

    modular_shield::process(app.world_mut(), g, 60.0);
    let s = state(&app);
    assert_eq!(s.stored_strength, 40.0);
    assert!(!s.recovering);
    assert_eq!(s.current_regeneration, 3.0);

    // Radius 1: the four neighbours.
    modular_shield::set_radius(app.world_mut(), g, 1.0);
    modular_shield::toggle(app.world_mut(), g);
    let s = state(&app);
    assert!(s.active && s.initiating_until.is_some());
    let mut tiles: Vec<TilePos> = s
        .shields
        .iter()
        .map(|&f| *app.world().get::<TilePos>(f).unwrap())
        .collect();
    tiles.sort_by_key(|p| (p.x, p.y));
    assert_eq!(tiles, vec![t(2, 2), t(3, 1), t(3, 3), t(4, 2)]);
    let field = s.shields[0];
    assert!(app.world().get::<ss13_world::Density>(field).is_none());
    assert_eq!(
        app.world()
            .get::<ss13_atmos::AtmosBlocker>(field)
            .map(|b| b.0),
        Some(ss13_atmos::AtmosPass::No)
    );
    // Active: 3 / (0.5 + 2 * 1 / 3) * 0.5.
    assert!((s.current_regeneration - 3.0 / (0.5 + 2.0 / 3.0) * 0.5).abs() < 1e-4);
    // Radius is locked while active; toggling again while initiating does nothing.
    modular_shield::set_radius(app.world_mut(), g, 2.0);
    modular_shield::toggle(app.world_mut(), g);
    assert_eq!((state(&app).radius, state(&app).active), (1.0, true));
    run(&mut app, modular_shield::INITIATE_SECS + 0.2);
    assert!(app.world().get::<ss13_world::Density>(field).is_some());
    assert!(state(&app).initiating_until.is_none());

    // Damage drains the generator, not the field; under 5 it fails and recovers.
    let w = app.world_mut();
    assert!(modular_shield::absorb(w, field, 20.0, true));
    assert_eq!(w.get::<ShieldGenerator>(g).unwrap().stored_strength, 20.0);
    assert!(w.get::<ModularShield>(field).is_some());
    assert!(modular_shield::absorb(w, field, 16.0, true));
    let s = w.get::<ShieldGenerator>(g).unwrap().clone();
    assert!(!s.active && s.recovering && s.shields.is_empty());
    assert_eq!(s.stored_strength, 0.0);
    assert!(w.get_entity(field).is_err());

    // Modules: a node facing the generator, a relay facing the node.
    let node = obj(
        &mut app,
        "/obj/machinery/modular_shield/module/node",
        t(2, 2),
    );
    let relay = obj(
        &mut app,
        "/obj/machinery/modular_shield/module/relay",
        t(1, 2),
    );
    let w = app.world_mut();
    w.entity_mut(node).insert(Facing(Dir::East));
    w.entity_mut(relay).insert(Facing(Dir::East));
    modular_shield::setup(w, node);
    modular_shield::setup(w, relay);
    let (n, r) = (
        w.get::<ShieldModule>(node).unwrap().clone(),
        w.get::<ShieldModule>(relay).unwrap().clone(),
    );
    assert_eq!(n.generator, Some(g));
    assert_eq!((r.node, r.generator), (Some(node), Some(g)));
    assert!(n.through_us.contains(&relay));
    let s = w.get::<ShieldGenerator>(g).unwrap().clone();
    assert!(s.modules.contains(&node) && s.modules.contains(&relay));
    // The node goes: the relay loses the generator through it.
    modular_shield::on_destroy(w, node);
    w.despawn(node);
    let r = w.get::<ShieldModule>(relay).unwrap().clone();
    assert_eq!((r.node, r.generator), (None, None));
    assert!(
        !w.get::<ShieldGenerator>(g)
            .unwrap()
            .modules
            .contains(&relay)
    );
}
