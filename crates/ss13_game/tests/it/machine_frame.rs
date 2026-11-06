//! Machine and computer frames over real WebTransport on a fixture room
//! (`ss13_game::machine_frame`): a recharger taken apart with a screwdriver and a crowbar and
//! built back from its frame, board and capacitor; a supply console unscrewed into its glassed
//! frame, its glass pried out and put back, and screwed back into a working console; a machine
//! frame built from iron sheets with a board in the other hand, wired and given its board.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::cargo::console::CargoConsole;
use ss13_game::combat::NextMove;
use ss13_game::construction::PanelOpen;
use ss13_game::inventory::Inventory;
use ss13_game::machine_frame::Frame;
use ss13_game::obj_damage::{Anchored, ObjState};
use ss13_game::power::PowerMachine;
use ss13_game::power::chargers::Recharger;
use ss13_game::stock_parts::ComponentParts;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::{DoAfter, Stack};
use ss13_mapload::{DmItem, DmObj, Turf};
use ss13_net::chat::client::ChatReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, NetId};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
    "/datum/stock_part": { "parent": "/datum" },
    "/datum/stock_part/capacitor": { "parent": "/datum/stock_part", "declared_vars": {
        "physical_object_type": { "path": "/obj/item/stock_parts/capacitor" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1,
        "flags_1": 0, "max_integrity": 500, "armor_type": { "expr": "/datum/armor" },
        "integrity_failure": 0, "damage_deflection": 0, "resistance_flags": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
    "/obj/item/crowbar": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "crowbar", "tool_behaviour": "crowbar", "force": 5 } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "screwdriver", "tool_behaviour": "screwdriver", "force": 5 } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "wrench", "tool_behaviour": "wrench", "force": 5 } },
    "/obj/item/shard": { "parent": "/obj/item", "declared_vars": { "icon_state": "large",
        "name": "shard" } },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/capacitor": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "capacitor", "icon_state": "capacitor" } },
    "/obj/item/circuitboard": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "circuit_map", "build_path": null } },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard",
        "declared_vars": { "needs_anchored": 1, "req_components": null,
        "def_components": null } },
    "/obj/item/circuitboard/machine/recharger": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "name": "Weapon Recharger",
        "build_path": { "path": "/obj/machinery/recharger" }, "needs_anchored": 0,
        "req_components": { "list": [
            { "assoc": [{ "path": "/datum/stock_part/capacitor" }, 1] } ] } } },
    "/obj/item/circuitboard/computer": { "parent": "/obj/item/circuitboard" },
    "/obj/item/circuitboard/computer/cargo": { "parent": "/obj/item/circuitboard/computer",
        "declared_vars": { "name": "Supply Console",
        "build_path": { "path": "/obj/machinery/computer/cargo" } } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/cable_coil": { "parent": "/obj/item/stack", "declared_vars": {
        "name": "cable coil", "icon_state": "coil",
        "merge_type": { "path": "/obj/item/stack/cable_coil" } } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/sheet/glass": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "glass", "icon_state": "sheet-glass",
        "merge_type": { "path": "/obj/item/stack/sheet/glass" } } },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "max_integrity": 300, "icon": { "resource": "s.dmi" } } },
    "/obj/structure/frame": { "parent": "/obj/structure", "declared_vars": {
        "name": "frame", "icon_state": "box_0", "density": 1, "anchored": 0, "state": 0,
        "max_integrity": 250 } },
    "/obj/structure/frame/machine": { "parent": "/obj/structure/frame", "declared_vars": {
        "name": "machine frame" } },
    "/obj/structure/frame/computer": { "parent": "/obj/structure/frame", "declared_vars": {
        "name": "computer frame", "icon_state": "0" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1,
        "icon": { "resource": "m.dmi" }, "max_integrity": 200, "use_power": 1,
        "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "circuit": null } },
    "/obj/machinery/recharger": { "parent": "/obj/machinery", "declared_vars": {
        "name": "recharger", "icon_state": "recharger", "density": 0,
        "circuit": { "path": "/obj/item/circuitboard/machine/recharger" } } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "name": "computer", "icon_state": "computer" } },
    "/obj/machinery/computer/cargo": { "parent": "/obj/machinery/computer", "declared_vars": {
        "name": "supply console",
        "circuit": { "path": "/obj/item/circuitboard/computer/cargo" } } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15, "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "baseturfs": { "path": "/turf/open/floor/plating" }, "overfloor_placed": 1,
        "floor_tile": null } },
    "/turf/open/floor/iron": { "parent": "/turf/open/floor" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The player spawns at `A` (3,3). Around it: a
/// screwdriver (2,4), a crowbar (3,4), ten cable (4,4), ten iron and a recharger board (2,3), a
/// recharger `M` (4,3), a wrench and ten glass (2,2), a supply console `K` (3,2); (4,2) is free.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor/iron,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/turf/open/floor/iron,/area/space)
"S" = (/obj/item/screwdriver,/turf/open/floor/iron,/area/space)
"C" = (/obj/item/crowbar,/turf/open/floor/iron,/area/space)
"L" = (/obj/item/stack/cable_coil{amount = 10},/turf/open/floor/iron,/area/space)
"I" = (/obj/item/stack/sheet/iron{amount = 10},/obj/item/circuitboard/machine/recharger,/turf/open/floor/iron,/area/space)
"M" = (/obj/machinery/recharger,/turf/open/floor/iron,/area/space)
"W" = (/obj/item/wrench,/obj/item/stack/sheet/glass{amount = 10},/turf/open/floor/iron,/area/space)
"K" = (/obj/machinery/computer/cargo,/turf/open/floor/iron,/area/space)

(1,1,1) = {"
wwwwwwwww
wSCLffffw
wIAMffffw
wWKfffffw
wwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const SCREWDRIVER_AT: TilePos = TilePos::new(2, 4, 1);
const CROWBAR_AT: TilePos = TilePos::new(3, 4, 1);
const CABLE_AT: TilePos = TilePos::new(4, 4, 1);
const IRON_AT: TilePos = TilePos::new(2, 3, 1);
const RECHARGER_AT: TilePos = TilePos::new(4, 3, 1);
const GLASS_AT: TilePos = TilePos::new(2, 2, 1);
const CONSOLE_AT: TilePos = TilePos::new(3, 2, 1);
const BUILD_AT: TilePos = TilePos::new(4, 2, 1);

const RECHARGER: &str = "/obj/machinery/recharger";
const RECHARGER_BOARD: &str = "/obj/item/circuitboard/machine/recharger";
const CAPACITOR: &str = "/obj/item/stock_parts/capacitor";
const CONSOLE: &str = "/obj/machinery/computer/cargo";
const MACHINE_FRAME: &str = "/obj/structure/frame/machine";
const COMPUTER_FRAME: &str = "/obj/structure/frame/computer";

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 25] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "crowbar"),
    ("i.dmi", "screwdriver"),
    ("i.dmi", "wrench"),
    ("i.dmi", "large"),
    ("i.dmi", "capacitor"),
    ("i.dmi", "circuit_map"),
    ("i.dmi", "coil"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "sheet-glass"),
    ("l.dmi", "card-id"),
    ("s.dmi", "box_0"),
    ("s.dmi", "box_1"),
    ("s.dmi", "box_2"),
    ("s.dmi", "0"),
    ("s.dmi", "1"),
    ("s.dmi", "2"),
    ("s.dmi", "3"),
    ("s.dmi", "4"),
    ("m.dmi", "recharger"),
    ("m.dmi", "computer"),
    ("i.dmi", "toy"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("machine_frame_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/screwdriver",
        "/obj/item/crowbar",
        "/obj/item/wrench",
        "/obj/item/shard",
        "/obj/item/stack/cable_coil",
        "/obj/item/stack/sheet/iron",
        "/obj/item/stack/sheet/glass",
        "/obj/item/circuitboard/machine/recharger",
        "/obj/structure/frame/machine",
        "/obj/structure/frame/computer",
        "/obj/machinery/recharger",
        "/obj/machinery/computer/cargo",
        "/turf/open/floor/iron",
        "/turf/closed/wall",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

#[derive(Resource, Default)]
struct Chat(Vec<String>);

fn record(mut chat: MessageReader<ChatReceived>, mut out: ResMut<Chat>) {
    for ChatReceived(message) in chat.read() {
        out.0.push(message.plain_text());
    }
}

struct Station(testing::Station);

impl Deref for Station {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.0
    }
}

impl DerefMut for Station {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.0
    }
}

impl Drive for Station {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.0
    }

    fn context(&self) -> String {
        format!("; chat: {:?}", self.chat())
    }
}

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join("builder", |app| {
            app.init_resource::<Chat>().add_systems(Update, record);
        });
        let s = Self(s);
        assert_eq!(s.pos(s.mob()), SPAWN);
        s
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn world(&self) -> &World {
        self.server.world()
    }

    fn pos(&self, e: Entity) -> TilePos {
        *self.world().get::<TilePos>(e).unwrap()
    }

    fn chat(&self) -> Vec<String> {
        self.players[0].app.world().resource::<Chat>().0.clone()
    }

    fn net_id(&self, e: Entity) -> Option<NetId> {
        self.0.net_id(0, e)
    }

    fn click_with(&mut self, e: Entity, right: bool) {
        let mob = self.mob();
        self.run_until("off click cooldown", |s| {
            s.now() >= s.world().get::<NextMove>(mob).unwrap().0
        });
        self.run_until("the target is replicated", |s| s.net_id(e).is_some());
        self.0
            .click_with(0, e, ClickModifiers { right, ..default() });
    }

    fn click(&mut self, e: Entity) {
        self.click_with(e, false);
    }

    fn active(&self) -> Option<Entity> {
        self.world()
            .get::<Inventory>(self.mob())
            .unwrap()
            .active_item()
    }

    fn find(&self, pos: TilePos, f: impl Fn(&World, Entity) -> bool) -> Option<Entity> {
        let world = self.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| f(world, e))
    }

    fn item_at(&self, pos: TilePos, path: &str) -> Option<Entity> {
        self.find(pos, |w, e| {
            w.get::<DmItem>(e).is_some_and(|i| i.path == path)
        })
    }

    fn obj_at(&self, pos: TilePos, path: &str) -> Option<Entity> {
        self.find(pos, |w, e| {
            w.get::<DmObj>(e).is_some_and(|o| o.path == path)
        })
    }

    fn state(&self, e: Entity) -> i32 {
        self.world().get::<ObjState>(e).unwrap().0
    }

    fn amount(&self, item: Entity) -> u32 {
        self.world().get::<Stack>(item).map_or(0, |s| s.amount)
    }

    /// The active hand empties onto the floor, then takes `item`.
    fn take(&mut self, item: Entity) {
        if self.active().is_some() {
            self.0.send(0, ClientIntent::DropActive);
            self.run_until("the hand is empty", |s| s.active().is_none());
        }
        self.click(item);
        self.run_until("picked up", |s| s.active() == Some(item));
    }

    fn swap_hands(&mut self) {
        let before = self.world().get::<Inventory>(self.mob()).unwrap().active;
        self.0.send(0, ClientIntent::SwapHands);
        self.run_until("hands swapped", |s| {
            s.world().get::<Inventory>(s.mob()).unwrap().active != before
        });
    }

    fn wait_do_after(&mut self, secs: f64, what: &str) {
        let mob = self.mob();
        self.run_until(&format!("{what} starts"), |s| {
            s.world().get::<DoAfter>(mob).is_some()
        });
        self.run_until_for(secs, &format!("{what} finishes"), |s| {
            s.world().get::<DoAfter>(mob).is_none()
        });
    }
}

#[test]
fn recharger_comes_apart_and_back() {
    let mut s = Station::start("recharger_comes_apart_and_back");
    let recharger = s.obj_at(RECHARGER_AT, RECHARGER).unwrap();
    assert!(s.world().get::<Recharger>(recharger).is_some());
    let screwdriver = s.item_at(SCREWDRIVER_AT, "/obj/item/screwdriver").unwrap();
    let crowbar = s.item_at(CROWBAR_AT, "/obj/item/crowbar").unwrap();

    // A crowbar does nothing with the panel closed.
    s.take(crowbar);
    s.click(recharger);
    s.hold(1.0, "still there", |s| {
        s.world().get_entity(recharger).is_ok()
    });

    s.swap_hands();
    s.take(screwdriver);
    s.click(recharger);
    s.run_until("the panel opens", |s| {
        s.world().get::<PanelOpen>(recharger) == Some(&PanelOpen(true))
    });
    s.swap_hands();
    s.click(recharger);
    s.run_until("taken apart", |s| s.world().get_entity(recharger).is_err());
    let frame = s.obj_at(RECHARGER_AT, MACHINE_FRAME).expect("a frame");
    assert_eq!(s.state(frame), 1, "wired");
    assert_eq!(
        s.world().get::<Anchored>(frame),
        Some(&Anchored(true)),
        "as anchored as the machine"
    );
    let board = s.item_at(RECHARGER_AT, RECHARGER_BOARD).expect("its board");
    let capacitor = s.item_at(RECHARGER_AT, CAPACITOR).expect("its capacitor");

    // Back together: board, capacitor, screwdriver.
    s.take(board);
    s.click(frame);
    s.run_until("the board is in", |s| s.state(frame) == 2);
    assert!(
        s.world().get::<TilePos>(board).is_none(),
        "inside the frame"
    );
    s.take(capacitor);
    // A screwdriver with a part missing refuses.
    s.swap_hands();
    s.click(frame);
    s.run_until("missing parts", |s| {
        s.chat().iter().any(|l| l.contains("Не хватает деталей"))
    });
    s.swap_hands();
    s.click(frame);
    s.run_until("the capacitor is in", |s| {
        s.world()
            .get::<Frame>(frame)
            .is_some_and(|f| f.needed.iter().all(|n| n.amount == 0))
    });
    s.swap_hands();
    s.click(frame);
    s.run_until("built", |s| s.world().get_entity(frame).is_err());
    let built = s.obj_at(RECHARGER_AT, RECHARGER).expect("a recharger");
    let world = s.world();
    assert!(world.get::<Recharger>(built).is_some(), "its live state");
    assert!(
        world.get::<PowerMachine>(built).is_some(),
        "on its area's power"
    );
    assert_eq!(world.get::<Anchored>(built), Some(&Anchored(true)));
    let parts: Vec<Entity> = world.get::<ComponentParts>(built).unwrap().parts().to_vec();
    assert_eq!(
        parts,
        [board, capacitor],
        "the board and capacitor it was built with"
    );
}

#[test]
fn console_unscrews_into_its_frame_and_back() {
    let mut s = Station::start("console_unscrews_into_its_frame_and_back");
    let console = s.obj_at(CONSOLE_AT, CONSOLE).unwrap();
    assert!(s.world().get::<CargoConsole>(console).is_some());
    let screwdriver = s.item_at(SCREWDRIVER_AT, "/obj/item/screwdriver").unwrap();
    let crowbar = s.item_at(CROWBAR_AT, "/obj/item/crowbar").unwrap();
    let glass = s.item_at(GLASS_AT, "/obj/item/stack/sheet/glass").unwrap();

    s.take(screwdriver);
    s.click(console);
    let started = s.now();
    s.wait_do_after(5.0, "unscrewing the monitor");
    assert!(s.now() - started >= 1.9, "2 s");
    assert!(s.world().get_entity(console).is_err());
    let frame = s
        .obj_at(CONSOLE_AT, COMPUTER_FRAME)
        .expect("a computer frame");
    assert_eq!(s.state(frame), 4, "glassed");
    assert_eq!(s.world().get::<Anchored>(frame), Some(&Anchored(true)));
    assert_eq!(
        s.world().get::<Frame>(frame).unwrap().board.as_deref(),
        Some("/obj/item/circuitboard/computer/cargo"),
        "the board stays in the frame"
    );

    s.swap_hands();
    s.take(crowbar);
    s.click(frame);
    s.run_until("the glass is out", |s| s.state(frame) == 3);
    let dropped = s
        .item_at(CONSOLE_AT, "/obj/item/stack/sheet/glass")
        .unwrap();
    assert_eq!(s.amount(dropped), 2);

    s.take(glass);
    s.click(frame);
    s.wait_do_after(5.0, "the glass goes in");
    assert_eq!(s.state(frame), 4);
    assert_eq!(s.amount(glass), 8);

    s.swap_hands();
    s.click(frame);
    s.run_until("the console is back", |s| {
        s.world().get_entity(frame).is_err()
    });
    let built = s.obj_at(CONSOLE_AT, CONSOLE).expect("a console");
    assert!(
        s.world().get::<CargoConsole>(built).is_some(),
        "its live state"
    );
    assert!(s.world().get::<PowerMachine>(built).is_some());
}

#[test]
fn frame_from_sheets_with_a_board_in_hand() {
    let mut s = Station::start("frame_from_sheets_with_a_board_in_hand");
    let iron = s.item_at(IRON_AT, "/obj/item/stack/sheet/iron").unwrap();
    let board = s.item_at(IRON_AT, RECHARGER_BOARD).unwrap();
    let cable = s.item_at(CABLE_AT, "/obj/item/stack/cable_coil").unwrap();
    s.take(board);
    s.swap_hands();
    s.take(iron);
    let floor = s.find(BUILD_AT, |w, e| w.get::<Turf>(e).is_some()).unwrap();
    s.click_with(floor, true);
    let started = s.now();
    s.wait_do_after(5.0, "the frame");
    assert!(s.now() - started >= 2.4, "2.5 s");
    let frame = s.obj_at(BUILD_AT, MACHINE_FRAME).expect("a machine frame");
    assert_eq!(s.amount(iron), 5, "5 sheets");
    assert_eq!(s.state(frame), 0);
    assert_eq!(s.world().get::<Anchored>(frame), Some(&Anchored(false)));

    // The board needs a wired frame.
    s.swap_hands();
    s.click(frame);
    s.run_until("refused", |s| {
        s.chat()
            .iter()
            .any(|l| l.contains("Сначала проложите кабели"))
    });
    s.swap_hands();
    s.take(cable);
    s.click(frame);
    s.wait_do_after(5.0, "wiring");
    assert_eq!(s.state(frame), 1);
    assert_eq!(s.amount(cable), 5);
    s.swap_hands();
    s.click(frame);
    s.run_until("the board is in", |s| s.state(frame) == 2);
}

fn metastation_map() -> ss13_mapload::MapLoadPlugin {
    ss13_mapload::MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..ss13_mapload::MapLoadPlugin::from_env().unwrap_or(ss13_mapload::MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    }
}

/// Every covered family on MetaStation: a mapped machine comes apart into a frame (or a
/// computer frame), its board and its default parts, and the board's machine built back gets
/// its family's live state; a thermomachine leaves the pipe graph and joins it again.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_families() {
    use ss13_atmos::pipenet::Pipenet;
    use ss13_game::atmos_machinery::AtmosDevice;
    use ss13_game::machine_frame::{Family, build_machine, buildable_machine, family};
    use ss13_mapload::{DmCircuit, DmMachine};
    use ss13_proto::Prototypes;

    let mut server = testing::server_app(metastation_map(), |app| {
        app.add_plugins(ss13_atmos::AtmosPlugin);
    });
    let world = server.world_mut();
    let mut q = world.query::<(Entity, &DmObj, &TilePos)>();
    let mut picked: Vec<(Family, Entity, String, TilePos)> = Vec::new();
    for (e, obj, &pos) in q.iter(world) {
        // Vendors keep their brand's canister, not the board's default parts: `vending::`.
        if let Some(f) = family(&obj.path)
            && f != Family::Vending
            && !picked.iter().any(|p| p.0 == f)
            && buildable_machine(world, e).is_some()
        {
            picked.push((f, e, obj.path.clone(), pos));
        }
    }
    println!(
        "covered families on MetaStation: {:?}",
        picked.iter().map(|p| p.0).collect::<Vec<_>>()
    );
    assert_eq!(picked.len(), 20, "every family is mapped: {picked:?}");
    for (fam, machine, path, pos) in picked {
        let board = world.get::<DmMachine>(machine).unwrap().circuit.clone();
        let def = world
            .resource::<Prototypes>()
            .get(&board)
            .and_then(|p| p.get::<DmCircuit>())
            .cloned()
            .unwrap();
        let device = world.get::<AtmosDevice>(machine).copied();
        ss13_game::obj_damage::deconstruct(world, machine, true);
        assert!(world.get_entity(machine).is_err(), "{path}");
        if let Some(device) = device {
            assert!(
                world.resource::<Pipenet>().device(device.id).is_none(),
                "{path} left the pipe graph"
            );
        }
        let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
        let has = |p: &str| {
            on_tile.iter().any(|&e| {
                world.get::<DmItem>(e).is_some_and(|i| i.path == p)
                    || world.get::<DmObj>(e).is_some_and(|o| o.path == p)
            })
        };
        if def.computer {
            assert!(has(COMPUTER_FRAME), "{path}: a computer frame");
        } else {
            // A frame on a blocked tile (a microwave on a table) falls apart into sheets.
            assert!(
                has(&board) && (has(MACHINE_FRAME) || has("/obj/item/stack/sheet/iron")),
                "{path}: frame and board"
            );
            for part in &def.default_parts {
                assert!(has(&part.path), "{path}: {}", part.path);
            }
        }
        let paths: Vec<String> = std::iter::once(board.clone())
            .chain(
                def.default_parts
                    .iter()
                    .flat_map(|p| std::iter::repeat_n(p.path.clone(), p.amount as usize)),
            )
            .collect();
        let parts = paths
            .iter()
            .filter_map(|p| ss13_game::construction::spawn_obj(world, p, pos))
            .collect();
        let built = build_machine(world, pos, board.clone(), parts, true, None)
            .unwrap_or_else(|| panic!("{path}: {} builds", def.build_path));
        let e = world.entity(built);
        let live = match fam {
            Family::Charger => {
                e.contains::<Recharger>() || e.contains::<ss13_game::power::chargers::CellCharger>()
            }
            Family::Kitchen => {
                e.contains::<ss13_game::kitchen::Microwave>()
                    || e.contains::<ss13_game::kitchen::Grinder>()
                    || e.contains::<ss13_game::kitchen::Processor>()
                    || e.contains::<ss13_game::cooking::Oven>()
                    || e.contains::<ss13_game::cooking::Griddle>()
                    || e.contains::<ss13_game::cooking::DeepFryer>()
            }
            Family::Tray => e.contains::<ss13_game::botany::tray::Tray>(),
            Family::Station => e.contains::<ss13_game::silicon::cyborg::RechargeStation>(),
            Family::Analyzer => e.contains::<ss13_game::science::analyzer::Analyzer>(),
            Family::Thermo => e
                .get::<AtmosDevice>()
                .is_some_and(|d| world.resource::<Pipenet>().device(d.id).is_some()),
            Family::RdConsole => e.contains::<ss13_game::science::console::RdConsole>(),
            Family::CargoConsole => e.contains::<CargoConsole>(),
            Family::AtmosAlert => {
                e.contains::<ss13_game::atmos_machinery::consoles::AtmosAlertConsole>()
            }
            Family::Autolathe => e.contains::<ss13_game::science::autolathe::Autolathe>(),
            Family::Vending => e.contains::<ss13_game::vending::Vendor>(),
            Family::ChemDispenser => e
                .get::<ss13_game::chem_dispenser::ChemDispenser>()
                .is_some_and(|d| d.cell.is_some() && !d.dispensable.is_empty()),
            Family::SmartFridge => e.contains::<ss13_game::smartfridge::SmartFridge>(),
            // The buffer is the volume of the two beakers among its parts.
            Family::ChemMaster => {
                e.get::<ss13_game::chem::Reagents>()
                    .is_some_and(|r| r.0.maximum_volume == 100.0)
                    && e.contains::<ss13_game::chem_master::ChemMaster>()
            }
            Family::ChemHeater => e
                .get::<ss13_game::chem_heater::ChemHeater>()
                .is_some_and(|h| h.heater_coefficient == 0.1),
            Family::DestructiveScanner => {
                e.contains::<ss13_game::science::experi_scanner::DestructiveScanner>()
            }
            Family::ServerControl => {
                e.contains::<ss13_game::science::server_control::ServerControl>()
            }
            Family::Pacman => e.contains::<ss13_game::power::pacman::Pacman>(),
            Family::Smes => e.contains::<ss13_game::power::Smes>(),
            Family::ShieldWallGen => e.contains::<ss13_game::power::shields::ShieldWallGen>(),
            Family::Orm => e.contains::<ss13_game::mining::orm::OreRedemption>(),
            Family::MailSorter => e.contains::<ss13_game::cargo::mail_sorter::MailSorter>(),
            Family::MaterialsMarket => {
                e.contains::<ss13_game::cargo::materials_market::MaterialsMarket>()
            }
        };
        assert!(live, "{fam:?}: {} has its live state", def.build_path);
        println!("{fam:?}: {path} -> {board} -> {}", def.build_path);
    }
}
