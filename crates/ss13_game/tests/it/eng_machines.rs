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
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
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
        "/obj/item/stack/sheet/iron",
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

// Manufactorio (`ss13_game::eng_machines::manufactorio`): machines on a powered cable, their
// procs called directly.

mod manufactorio {
    use super::*;
    use ss13_game::eng_machines::manufactorio::{
        self as manu, FilterKind, Manufacturing, SortFilter,
    };
    use ss13_game::obj_damage::Anchored;
    use ss13_game::power::Grid;
    use ss13_game::science::lathe::MaterialStore;
    use ss13_game::science::techweb::ResearchDb;
    use ss13_mapload::DmOre;
    use ss13_mapload::research::{DesignDef, MaterialDef, ResearchData, build_type};
    use ss13_shared::{Dir, Facing};

    const IRON: &str = "/obj/item/stack/sheet/iron";
    const ORE: &str = "/obj/item/stack/ore/iron";

    /// A machine of `path` facing `dir` on a cable whose net has plenty of surplus.
    fn machine(app: &mut App, path: &str, pos: TilePos, dir: Dir) -> Entity {
        let world = app.world_mut();
        let cable = world.spawn_empty().id();
        let net = world.resource_mut::<Grid>().add_cable(cable, pos, 2);
        let e = world
            .spawn((
                pos,
                DmObj {
                    path: path.into(),
                    anchored: true,
                    ..default()
                },
                Anchored(true),
                Facing(dir),
            ))
            .id();
        manu::setup(world, e);
        power(app, e);
        let _ = net;
        e
    }

    /// Refills the machine's net (the power tick resets it).
    fn power(app: &mut App, e: Entity) {
        let world = app.world_mut();
        let mut grid = world.resource_mut::<Grid>();
        if let Some(id) = grid.node_net(e)
            && let Some(net) = grid.net_mut(id)
        {
            net.avail = 1e9;
            net.load = 0.0;
        }
    }

    fn stack(app: &mut App, path: &str, pos: TilePos, amount: u32) -> Entity {
        app.world_mut()
            .spawn((
                pos,
                DmItem {
                    path: path.into(),
                    ..default()
                },
                Stack { amount },
            ))
            .id()
    }

    fn amount_on(app: &mut App, path: &str, pos: TilePos) -> u32 {
        let world = app.world_mut();
        let mut q = world.query::<(&DmItem, &TilePos, &Stack)>();
        q.iter(world)
            .filter(|(i, p, _)| i.path == path && **p == pos)
            .map(|(_, _, s)| s.amount)
            .sum()
    }

    fn amount_inside(app: &mut App, machine: Entity) -> u32 {
        let world = app.world_mut();
        manu::contents(world, machine)
            .iter()
            .map(|&e| ss13_game::tools::stack_amount(world, e))
            .sum()
    }

    #[test]
    fn smelter_refines_ore_into_the_storage_unit_next_to_it() {
        let mut app = fixture("manufactorio_smelter");
        let smelter = machine(&mut app, manu::SMELTER, t(3, 2), Dir::East);
        let storage = machine(&mut app, manu::STORAGE, t(4, 2), Dir::South);
        let ore = stack(&mut app, ORE, t(2, 2), 7);
        app.world_mut().entity_mut(ore).insert(DmOre {
            refined_type: IRON.into(),
            ..default()
        });
        let world = app.world_mut();
        // Only from behind (its input side is west when it outputs east).
        assert!(!manu::receive_resource(
            world,
            smelter,
            ore,
            None,
            Dir::East
        ));
        assert!(manu::receive_resource(world, smelter, ore, None, Dir::West));
        assert_eq!(manu::contents(world, smelter), vec![ore]);
        manu::process(world, smelter);
        assert_eq!(amount_inside(&mut app, storage), 5, "5 ore a tick");
        power(&mut app, smelter);
        manu::process(app.world_mut(), smelter);
        assert_eq!(amount_inside(&mut app, storage), 7);
        assert!(app.world().get_entity(ore).is_err(), "the ore is used up");
        assert!(manu::contents(app.world(), smelter).is_empty());
    }

    #[test]
    fn crusher_keeps_fractions_and_sends_whole_sheets_forward() {
        let mut app = fixture("manufactorio_crusher");
        let crusher = machine(&mut app, manu::CRUSHER, t(3, 2), Dir::East);
        let world = app.world_mut();
        world
            .get_mut::<Manufacturing>(crusher)
            .unwrap()
            .held_mats
            .push((IRON.into(), 1.5));
        manu::process(world, crusher);
        // The whole sheet is made this tick and goes out on the next.
        manu::process(world, crusher);
        assert_eq!(amount_on(&mut app, IRON, t(4, 2)), 1);
        let held = app
            .world()
            .get::<Manufacturing>(crusher)
            .unwrap()
            .held_mats
            .clone();
        assert_eq!(held.len(), 1);
        assert!((held[0].1 - 0.5).abs() < 1e-6, "{held:?}");
        // Ore is refused; an item without materials is crushed to nothing.
        let ore = stack(&mut app, ORE, t(2, 2), 1);
        let junk = stack(&mut app, "/obj/item/stack/sheet/meat", t(2, 2), 1);
        let world = app.world_mut();
        assert!(!manu::receive_resource(
            world,
            crusher,
            ore,
            None,
            Dir::West
        ));
        assert!(manu::receive_resource(
            world,
            crusher,
            junk,
            None,
            Dir::West
        ));
        manu::process(world, crusher);
        assert!(world.get_entity(junk).is_err());
    }

    #[test]
    fn router_splits_a_stack_over_its_three_other_sides() {
        let mut app = fixture("manufactorio_router");
        let router = machine(&mut app, manu::ROUTER, t(3, 2), Dir::South);
        let sheets = stack(&mut app, IRON, t(2, 2), 3);
        let world = app.world_mut();
        // One sheet per offer: the rest stays with the sender until the last one.
        assert!(!manu::receive_resource(
            world,
            router,
            sheets,
            None,
            Dir::West
        ));
        assert!(!manu::receive_resource(
            world,
            router,
            sheets,
            None,
            Dir::West
        ));
        assert!(manu::receive_resource(
            world,
            router,
            sheets,
            None,
            Dir::West
        ));
        for pos in [t(3, 3), t(3, 1), t(4, 2)] {
            assert_eq!(amount_on(&mut app, IRON, pos), 1, "{pos:?}");
        }
        assert_eq!(amount_on(&mut app, IRON, t(2, 2)), 0, "never back west");
        // An output toggled off is skipped.
        let world = app.world_mut();
        world.get_mut::<Manufacturing>(router).unwrap().disabled = vec![Dir::North, Dir::South];
        let one = stack(&mut app, IRON, t(2, 2), 1);
        assert!(manu::receive_resource(
            app.world_mut(),
            router,
            one,
            None,
            Dir::West
        ));
        assert_eq!(amount_on(&mut app, IRON, t(4, 2)), 2);
    }

    #[test]
    fn sorter_pushes_by_its_filters() {
        let mut app = fixture("manufactorio_sorter");
        let sorter = machine(&mut app, manu::SORTER, t(3, 2), Dir::East);
        app.world_mut()
            .get_mut::<Manufacturing>(sorter)
            .unwrap()
            .filters
            .push(SortFilter {
                id: 0,
                name: "input is ore".into(),
                kind: FilterKind::IsOre,
                dir: Dir::North,
                inverted: false,
            });
        let ore = stack(&mut app, ORE, t(2, 2), 2);
        let sheet = stack(&mut app, IRON, t(2, 2), 2);
        let world = app.world_mut();
        assert!(manu::receive_resource(world, sorter, ore, None, Dir::West));
        assert!(manu::receive_resource(
            world,
            sorter,
            sheet,
            None,
            Dir::West
        ));
        manu::process(world, sorter);
        assert_eq!(amount_on(&mut app, ORE, t(3, 3)), 2, "ore north");
        assert_eq!(amount_on(&mut app, IRON, t(4, 2)), 2, "the rest east");
    }

    const IRON_MAT: &str = "/datum/material/iron";
    const BOARD: &str = "/obj/item/circuitboard/machine/sheetifier";

    fn research() -> ResearchDb {
        ResearchDb::new(ResearchData {
            materials: vec![MaterialDef {
                id: IRON_MAT.into(),
                name: "iron".into(),
                sheet_type: Some(IRON.into()),
                color: None,
            }],
            stacks: vec![
                (IRON.into(), vec![(IRON_MAT.into(), 100.0)]),
                ("/obj/item/stack/rods".into(), vec![(IRON_MAT.into(), 50.0)]),
            ],
            designs: vec![DesignDef {
                id: "board".into(),
                name: "board".into(),
                build_type: build_type::AUTOLATHE,
                build_path: Some(BOARD.into()),
                materials: vec![(IRON_MAT.into(), 50.0)],
                category: vec!["initial".into()],
                construction_time: 10.0,
                lathe_time_factor: 1.0,
                ..default()
            }],
            ..default()
        })
    }

    #[test]
    fn lathe_prints_its_design_from_the_sheets_it_takes() {
        let mut app = fixture("manufactorio_lathe");
        app.world_mut().insert_resource(research());
        let lathe = machine(&mut app, manu::LATHE, t(3, 2), Dir::East);
        let sheets = stack(&mut app, IRON, t(2, 2), 2);
        let world = app.world_mut();
        assert_eq!(manu::lathe_designs(world.resource::<ResearchDb>()), vec![0]);
        assert!(manu::receive_resource(
            world,
            lathe,
            sheets,
            None,
            Dir::West
        ));
        assert_eq!(
            world.get::<MaterialStore>(lathe).unwrap().amounts,
            vec![200]
        );
        world.get_mut::<Manufacturing>(lathe).unwrap().design = Some(0);
        manu::process(world, lathe);
        let busy = world.get::<Manufacturing>(lathe).unwrap().busy_until;
        assert!(busy.is_some(), "the print started");
        // Its time is up.
        world.get_mut::<Manufacturing>(lathe).unwrap().busy_until = Some(0.0);
        power(&mut app, lathe);
        manu::process(app.world_mut(), lathe);
        let world = app.world_mut();
        assert_eq!(
            world.get::<MaterialStore>(lathe).unwrap().amounts,
            vec![150]
        );
        let mut q = world.query::<(&DmItem, &TilePos)>();
        assert!(
            q.iter(world).any(|(i, p)| i.path == BOARD && *p == t(4, 2)),
            "the print came out forward"
        );
    }

    #[test]
    fn crusher_turns_rods_into_iron_sheets() {
        let mut app = fixture("manufactorio_crusher_rods");
        app.world_mut().insert_resource(research());
        let crusher = machine(&mut app, manu::CRUSHER, t(3, 2), Dir::East);
        let rods = stack(&mut app, "/obj/item/stack/rods", t(2, 2), 3);
        let world = app.world_mut();
        assert!(manu::receive_resource(
            world,
            crusher,
            rods,
            None,
            Dir::West
        ));
        manu::process(world, crusher);
        // 3 rods are 1.5 sheets: one goes out, half a sheet is kept.
        assert_eq!(amount_on(&mut app, IRON, t(4, 2)), 1);
        let held = app
            .world()
            .get::<Manufacturing>(crusher)
            .unwrap()
            .held_mats
            .clone();
        assert!((held[0].1 - 0.5).abs() < 1e-6, "{held:?}");
    }

    #[test]
    fn sorter_window_adds_turns_moves_and_deletes_filters() {
        use ss13_game::eng_machines::manufactorio::{Asks, SorterAct};
        use ss13_tgui::prompt::PromptAnswer;
        let mut app = fixture("manufactorio_sorter_window");
        let sorter = machine(&mut app, manu::SORTER, t(3, 2), Dir::East);
        let client = app.world_mut().spawn_empty().id();
        let world = app.world_mut();
        // "input is ore" and "input is stack" from the new filter list.
        for i in [1, 4] {
            manu::apply_answer(
                world,
                client,
                sorter,
                Asks::NewFilter,
                PromptAnswer::Item(i),
            );
        }
        let names = |w: &World| -> Vec<String> {
            w.get::<Manufacturing>(sorter)
                .unwrap()
                .filters
                .iter()
                .map(|f| f.name.clone())
                .collect()
        };
        assert_eq!(names(world), ["input is ore", "input is stack"]);
        manu::apply_sorter_act(world, client, client, sorter, SorterAct::Rotate(0));
        manu::apply_sorter_act(world, client, client, sorter, SorterAct::Shift(0, 1));
        assert_eq!(names(world), ["input is stack", "input is ore"]);
        manu::apply_sorter_act(world, client, client, sorter, SorterAct::RotateUnmet);
        let m = world.get::<Manufacturing>(sorter).unwrap();
        assert_eq!(m.filters[1].dir, Dir::South, "north turns to south");
        assert_eq!(m.unmet, Dir::West, "east turns to west");
        manu::apply_sorter_act(world, client, client, sorter, SorterAct::Delete(1));
        assert_eq!(names(world), ["input is ore"]);
        // A bogus id does nothing.
        manu::apply_sorter_act(world, client, client, sorter, SorterAct::Delete(7));
        assert_eq!(names(world).len(), 1);
    }

    #[test]
    fn unloader_empties_an_ore_box_forward_and_the_box_aside() {
        let mut app = fixture("manufactorio_unloader");
        let unloader = machine(&mut app, manu::UNLOADER, t(3, 2), Dir::East);
        let storage = machine(&mut app, manu::STORAGE, t(4, 2), Dir::South);
        let world = app.world_mut();
        let ore_box = world
            .spawn((
                t(2, 2),
                DmObj {
                    path: "/obj/structure/ore_box".into(),
                    ..default()
                },
            ))
            .id();
        let ore = stack(&mut app, ORE, t(2, 2), 4);
        let world = app.world_mut();
        ss13_game::storage::put_inside(world, ore, ore_box);
        assert!(manu::receive_resource(
            world,
            unloader,
            ore_box,
            None,
            Dir::West
        ));
        manu::process(world, unloader);
        assert_eq!(manu::contents(app.world(), storage), vec![ore]);
        // Right of east is south.
        assert_eq!(app.world().get::<TilePos>(ore_box), Some(&t(3, 1)));
        // The storage unit dumps onto its tile.
        manu::dump(app.world_mut(), storage);
        assert_eq!(amount_on(&mut app, ORE, t(4, 2)), 4);
    }
}
