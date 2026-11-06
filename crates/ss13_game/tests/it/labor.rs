//! Prison labor (`ss13_game::brig::labor`): sealed sheets in the labor camp collection unit
//! earn points the claim console moves onto the prisoner ID (unsealed ones earn none); a blank
//! license plate earns 10 on loading and the press makes a finished plate in 4 s; crops sent by
//! the produceporter land on the pad for 2 points each; the labor camp gate holds the prisoner
//! until the goal is met.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::brig::labor::{
    BLANK_PLATE, ClaimAct, ClaimConsole, FILLED_PLATE, GateVerdict, LaborStacker, PlatePress,
    PrisonGateState, ProducePad, Produceporter, claim_act, gate_verdict, interact,
    item_interaction, stacker_points,
};
use ss13_game::brig::prisoner::{PRISONER_CARD, PrisonerCard};
use ss13_game::construction::Manufactured;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::Stack;
use ss13_mapload::DmItem;
use ss13_shared::{Dir, TilePos};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "light_power": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "toy", "interaction_flags_item": 1 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/card/id/advanced/prisoner": { "parent": "/obj/item/card/id/advanced",
        "declared_vars": { "name": "prisoner ID card" } },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/under/rank": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/rank/prisoner": { "parent": "/obj/item/clothing/under/rank" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/clothing/shoes/sneakers/orange": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "req_access": null, "req_one_access": null } },
    "/obj/machinery/mineral": { "parent": "/obj/machinery", "declared_vars": {
        "input_dir": 1, "output_dir": 2 } },
    "/obj/machinery/mineral/stacking_machine": { "parent": "/obj/machinery/mineral",
        "declared_vars": { "input_dir": 4, "output_dir": 8, "stack_amt": 50 } },
    "/obj/machinery/mineral/stacking_machine/laborstacker": {
        "parent": "/obj/machinery/mineral/stacking_machine" },
    "/obj/machinery/mineral/labor_claim_console": { "parent": "/obj/machinery/mineral" },
    "/obj/machinery/plate_press": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "p.dmi" }, "icon_state": "offline" } },
    "/obj/machinery/produceporter": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "p.dmi" }, "icon_state": "produceporter" } },
    "/obj/machinery/prisongate": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "p.dmi" }, "icon_state": "prisongate_on" } },
    "/obj/machinery/prisongate/labour": { "parent": "/obj/machinery/prisongate" },
    "/obj/item/producepad": { "parent": "/obj/item" },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null } },
    "/obj/item/stack/license_plates": { "parent": "/obj/item/stack" },
    "/obj/item/stack/license_plates/empty": { "parent": "/obj/item/stack/license_plates" },
    "/obj/item/stack/license_plates/filled": { "parent": "/obj/item/stack/license_plates" },
    "/obj/machinery/gulag_teleporter": { "parent": "/obj/machinery", "declared_vars": {
        "name": "labor camp teleporter", "icon": { "resource": "ic.dmi" },
        "icon_state": "implantchair", "density": 1 } },
    "/obj/machinery/gulag_item_reclaimer": { "parent": "/obj/machinery", "declared_vars": {
        "name": "equipment reclaimer station", "icon": { "resource": "wm.dmi" },
        "icon_state": "gulag_off" } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/prisoner": { "parent": "/obj/machinery/computer" },
    "/obj/machinery/computer/prisoner/gulag_teleporter_computer": {
        "parent": "/obj/machinery/computer/prisoner", "declared_vars": {
        "name": "labor camp teleporter console", "req_access": { "list": ["brig"] } } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/gulag_beacon": { "parent": "/obj/structure", "declared_vars": {
        "name": "labor camp bluespace beacon", "icon": { "resource": "t.dmi" },
        "icon_state": "floor" } },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0, "always_unpowered": 0,
        "power_equip": 1, "power_light": 1, "power_environ": 1, "lightswitch": 1,
        "static_lighting": 1 } },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" }
  }
}"##;
/// The player at (2,3), the plate press at (3,3), the produceporter at (5,3), the labor camp
/// gate at (5,4), the collection unit at (3,2), the claim console at (4,2), the pad at (5,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"P" = (/obj/machinery/plate_press,/turf/open/floor,/area/station)
"O" = (/obj/machinery/produceporter,/turf/open/floor,/area/station)
"G" = (/obj/machinery/prisongate/labour,/turf/open/floor,/area/station)
"K" = (/obj/machinery/mineral/stacking_machine/laborstacker,/turf/open/floor,/area/station)
"C" = (/obj/machinery/mineral/labor_claim_console,/turf/open/floor,/area/station)
"d" = (/obj/item/producepad,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwww
wfffGfw
wsPfOfw
wfKCdfw
wwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 3, 1);
const PRESS: TilePos = TilePos::new(3, 3, 1);
const PORTER: TilePos = TilePos::new(5, 3, 1);
const GATE: TilePos = TilePos::new(5, 4, 1);
const STACKER: TilePos = TilePos::new(3, 2, 1);
const CONSOLE: TilePos = TilePos::new(4, 2, 1);
const PAD: TilePos = TilePos::new(5, 2, 1);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("labor_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/plate_press",
        "/obj/machinery/produceporter",
        "/obj/machinery/prisongate/labour",
        "/obj/machinery/mineral/stacking_machine/laborstacker",
        "/obj/machinery/mineral/labor_claim_console",
        "/obj/item/producepad",
        PRISONER_CARD,
        FILLED_PLATE,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    let states = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "toy"),
        ("p.dmi", "offline"),
        ("p.dmi", "online"),
        ("p.dmi", "online_loaded"),
        ("p.dmi", "loop"),
        ("p.dmi", "produceporter"),
        ("p.dmi", "prisongate_on"),
    ];
    write_fixture(&dir, TYPES, &used, &states, &[("map.dmm", MAP)])
}

fn find<T: Component>(s: &testing::Station, pos: TilePos) -> Entity {
    let world = s.server.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<T>(e).is_some())
        .unwrap_or_else(|| panic!("nothing with {} on {pos:?}", std::any::type_name::<T>()))
}

fn item(world: &mut World, path: &str) -> Entity {
    world
        .spawn(DmItem {
            path: path.to_owned(),
            ..default()
        })
        .id()
}

fn points(s: &testing::Station, card: Entity) -> u32 {
    s.get::<PrisonerCard>(card).points
}

#[test]
fn prison_labor_earns_points_and_opens_the_gate() {
    let mut s =
        testing::Station::start(fixture_map(&fixture_assets("labor"), &["map.dmm"]), |_| {});
    s.join("p0", |_| {});
    let (mob, client) = (s.players[0].mob, s.players[0].link);
    assert_eq!(*s.get::<TilePos>(mob), SPAWN);
    let press = find::<PlatePress>(&s, PRESS);
    let porter = find::<Produceporter>(&s, PORTER);
    let stacker = find::<LaborStacker>(&s, STACKER);
    let console = find::<ClaimConsole>(&s, CONSOLE);
    find::<PrisonGateState>(&s, GATE);
    find::<ProducePad>(&s, PAD);
    // A prisoner ID in the free hand.
    let card = {
        let world = s.server.world_mut();
        let card = ss13_game::construction::spawn_obj(world, PRISONER_CARD, SPAWN)
            .expect("the prisoner ID prototype");
        world.flush();
        world
            .entity_mut(card)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().hands[1] = Some(card);
        world.get_mut::<PrisonerCard>(card).unwrap().goal = 200;
        card
    };
    s.run_until("the claim console links the unit", |s| {
        s.get::<ClaimConsole>(console).stacker == Some(stacker)
    });

    // Sealed sheets earn points, others do not; the console moves them onto the ID.
    {
        let world = s.server.world_mut();
        let sealed = item(world, "/obj/item/stack/sheet/iron");
        world.entity_mut(sealed).insert(Manufactured);
        let plain = item(world, "/obj/item/stack/sheet/iron");
        stacker_points(world, stacker, sealed, "/obj/item/stack/sheet/iron", 10);
        stacker_points(world, stacker, plain, "/obj/item/stack/sheet/iron", 10);
        assert_eq!(world.get::<LaborStacker>(stacker).unwrap().points, 50);
        claim_act(world, mob, console, ClaimAct::Claim);
        assert_eq!(world.get::<LaborStacker>(stacker).unwrap().points, 0);
    }
    assert_eq!(points(&s, card), 50);

    // A blank plate: 10 points on loading, a finished plate after 4 s of pressing.
    {
        let world = s.server.world_mut();
        let blank = item(world, BLANK_PLATE);
        world.entity_mut(blank).insert(Stack { amount: 2 });
        assert!(item_interaction(world, client, mob, blank, press));
        assert!(world.get::<PlatePress>(press).unwrap().loaded);
        assert_eq!(world.get::<Stack>(blank).unwrap().amount, 1);
        assert!(interact(world, client, mob, press, false));
    }
    assert_eq!(points(&s, card), 60);
    s.run_until("the plate is pressed", |s| {
        !s.get::<PlatePress>(press).loaded
    });
    let filled = {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(PRESS)
            .iter()
            .any(|&e| {
                world
                    .get::<DmItem>(e)
                    .is_some_and(|i| i.path == FILLED_PLATE)
            })
    };
    assert!(filled, "a finished plate lies under the press");

    // Two crops sent to the pad: 2 points each.
    let crops: Vec<Entity> = {
        let world = s.server.world_mut();
        let crops = vec![
            item(world, "/obj/item/food/grown/apple"),
            item(world, "/obj/item/food/grown/apple"),
        ];
        for &c in &crops {
            assert!(item_interaction(world, client, mob, c, porter));
        }
        assert_eq!(world.get::<Produceporter>(porter).unwrap().produce.len(), 2);
        assert!(interact(world, client, mob, porter, true));
        crops
    };
    for c in crops {
        assert_eq!(*s.get::<TilePos>(c), PAD);
    }
    assert_eq!(points(&s, card), 64);

    // The labor camp gate holds a prisoner short of the goal and lets them out once it is met.
    let before = GATE.step(Dir::West);
    assert_eq!(
        gate_verdict(s.server.world(), mob, find::<PrisonGateState>(&s, GATE)),
        GateVerdict::Remaining(136)
    );
    for (dir, to) in [
        (Dir::North, TilePos::new(2, 4, 1)),
        (Dir::East, TilePos::new(3, 4, 1)),
        (Dir::East, before),
    ] {
        s.run_until("walking to the gate", |s| {
            if *s.get::<TilePos>(mob) != to {
                s.try_step(0, dir);
            }
            *s.get::<TilePos>(mob) == to
        });
    }
    for _ in 0..10 {
        s.try_step(0, Dir::East);
        s.run_for(0.2);
    }
    assert_eq!(*s.get::<TilePos>(mob), before, "the gate holds");
    s.server
        .world_mut()
        .get_mut::<PrisonerCard>(card)
        .unwrap()
        .points = 200;
    s.run_until("the gate lets the prisoner out", |s| {
        if *s.get::<TilePos>(mob) != GATE {
            s.try_step(0, Dir::East);
        }
        *s.get::<TilePos>(mob) == GATE
    });
}
