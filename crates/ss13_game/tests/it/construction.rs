//! Construction over real WebTransport on 127.0.0.1, on a fixture room with tools around the
//! spawn: pry a floor tile with a crowbar and put it back; build a girder from iron sheets
//! (right click), secure it with a wrench, plate it into a wall and slice the wall back to a
//! girder with a lit welder; smash the window between the room and space with a toolbox and
//! watch the room's air leave.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::{Atmos, AtmosPlugin};
use ss13_game::combat::NextMove;
use ss13_game::construction::Baseturfs;
use ss13_game::inventory::Inventory;
use ss13_game::obj_damage::{Anchored, Integrity};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::{DoAfter, Stack, Welder};
use ss13_mapload::{DmItem, DmObj, DmTurfBuild, Turf};
use ss13_net::chat::client::ChatReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot, NetId};
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_world::{Density, SpatialIndex};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
    "/datum/armor/structure_window": { "parent": "/datum/armor", "declared_vars": {
        "melee": { "expr": "50" } } },
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
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "wrench", "tool_behaviour": "wrench", "force": 5 } },
    "/obj/item/weldingtool": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "welder", "inhand_icon_state": "welder", "tool_behaviour": "welder",
        "max_fuel": 20, "heat": 3800, "force": 3 } },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/toolbox": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "toolbox", "icon_state": "toolbox", "force": 13 } },
    "/obj/item/shard": { "parent": "/obj/item", "declared_vars": { "icon_state": "large",
        "name": "shard" } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/rods": { "parent": "/obj/item/stack", "declared_vars": {
        "icon_state": "rods", "merge_type": { "path": "/obj/item/stack/rods" } } },
    "/obj/item/stack/tile": { "parent": "/obj/item/stack", "declared_vars": {
        "max_amount": 60 } },
    "/obj/item/stack/tile/iron": { "parent": "/obj/item/stack/tile", "declared_vars": {
        "icon_state": "tile", "turf_type": { "path": "/turf/open/floor/iron" },
        "merge_type": { "path": "/obj/item/stack/tile/iron" } } },
    "/obj/item/stack/tile/iron/base": { "parent": "/obj/item/stack/tile/iron",
        "declared_vars": { "merge_type": { "path": "/obj/item/stack/tile/iron/base" } } },
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
    "/obj/structure/girder": { "parent": "/obj/structure", "declared_vars": {
        "name": "girder", "icon_state": "girder", "density": 1, "state": 0,
        "stack_type": { "path": "/obj/item/stack/sheet/iron" }, "stack_amount": 2,
        "can_displace": 1, "max_integrity": 200 } },
    "/obj/structure/girder/displaced": { "parent": "/obj/structure/girder", "declared_vars": {
        "icon_state": "displaced", "anchored": 0, "state": 3, "max_integrity": 120 } },
    "/obj/structure/window": { "parent": "/obj/structure", "declared_vars": {
        "name": "window", "icon_state": "window", "density": 1, "can_atmos_pass": -1,
        "flags_1": 2, "armor_type": { "expr": "/datum/armor/structure_window" },
        "max_integrity": 50, "state": 0, "reinf": 0, "fulltile": 0,
        "glass_type": { "path": "/obj/item/stack/sheet/glass" }, "glass_amount": 1,
        "decon_speed": 30 } },
    "/obj/structure/window/fulltile": { "parent": "/obj/structure/window",
        "vars": { "name": "full tile window" }, "declared_vars": {
        "name": "full tile window", "flags_1": 0, "fulltile": 1, "glass_amount": 2,
        "max_integrity": 100 } },
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
    "/turf/open/floor/iron": { "parent": "/turf/open/floor", "declared_vars": {
        "floor_tile": { "path": "/obj/item/stack/tile/iron/base" } } },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "overfloor_placed": 0,
        "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall",
        "baseturfs": { "path": "/turf/open/floor/plating" },
        "sheet_type": { "path": "/obj/item/stack/sheet/iron" }, "sheet_amount": 2,
        "girder_type": { "path": "/obj/structure/girder" }, "slicing_duration": 100 } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The player spawns at `A` (3,3) with a toolbox under
/// it; a welder on the iron floor (2,3), a crowbar (2,4), a wrench (3,4), ten iron sheets
/// (4,4). The full-tile window `W` (4,2) on plating separates the room from space (y = 1).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor/iron,/area/space)
"s" = (/turf/open/space/basic,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/obj/item/storage/toolbox,/turf/open/floor/iron,/area/space)
"L" = (/obj/item/weldingtool,/turf/open/floor/iron,/area/space)
"C" = (/obj/item/crowbar,/turf/open/floor/iron,/area/space)
"R" = (/obj/item/wrench,/turf/open/floor/iron,/area/space)
"I" = (/obj/item/stack/sheet/iron{amount = 10},/turf/open/floor/iron,/area/space)
"W" = (/obj/structure/window/fulltile,/turf/open/floor/plating,/area/space)

(1,1,1) = {"
wwwwwwwww
wfffffffw
wCRIffffw
wLAfffffw
wwwWwwwww
sssssssss
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const WELDER_AT: TilePos = TilePos::new(2, 3, 1);
const CROWBAR_AT: TilePos = TilePos::new(2, 4, 1);
const WRENCH_AT: TilePos = TilePos::new(3, 4, 1);
const IRON_AT: TilePos = TilePos::new(4, 4, 1);
const BUILD_AT: TilePos = TilePos::new(4, 3, 1);
const WINDOW_AT: TilePos = TilePos::new(4, 2, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 19] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toolbox"),
    ("i.dmi", "crowbar"),
    ("i.dmi", "wrench"),
    ("i.dmi", "welder"),
    ("i.dmi", "welder100"),
    ("i.dmi", "welder-on"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "tile"),
    ("i.dmi", "large"),
    ("i.dmi", "rods"),
    ("s.dmi", "girder"),
    ("s.dmi", "displaced"),
    ("s.dmi", "window"),
    ("l.dmi", "card-id"),
];
const FLOOR_ICON: IconStateId = IconStateId(0);
const PLATING_ICON: IconStateId = IconStateId(1);
const WALL_ICON: IconStateId = IconStateId(2);
const WELDER_ON_ICON: IconStateId = IconStateId(10);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("construction_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/storage/toolbox",
        "/obj/item/weldingtool",
        "/obj/item/crowbar",
        "/obj/item/wrench",
        "/obj/item/shard",
        "/obj/item/stack/sheet/iron",
        "/obj/item/stack/rods",
        "/obj/item/stack/tile/iron/base",
        "/obj/structure/girder",
        "/obj/structure/girder/displaced",
        "/obj/structure/window/fulltile",
        "/turf/open/floor/iron",
        "/turf/open/floor/plating",
        "/turf/closed/wall",
        "/turf/open/space/basic",
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
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(AtmosPlugin);
        });
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

    fn said(&self, text: &str) -> bool {
        self.chat().iter().any(|l| l.contains(text))
    }

    fn send(&mut self, intent: ClientIntent) {
        self.0.send(0, intent);
    }

    fn net_id(&self, e: Entity) -> Option<NetId> {
        self.0.net_id(0, e)
    }

    fn client_copy(&mut self, e: Entity) -> Option<Entity> {
        self.0.client_copy(0, e)
    }

    /// Waits for the click cooldown and the replication of `e`, then clicks it.
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

    fn inventory(&self) -> Inventory {
        self.world().get::<Inventory>(self.mob()).unwrap().clone()
    }

    fn active(&self) -> Option<Entity> {
        self.inventory().active_item()
    }

    /// The first entity on `pos` matching `f`.
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

    fn turf(&self, pos: TilePos) -> Entity {
        self.find(pos, |w, e| w.get::<Turf>(e).is_some()).unwrap()
    }

    fn turf_path(&self, pos: TilePos) -> String {
        let turf = self.turf(pos);
        self.world().get::<DmTurfBuild>(turf).unwrap().path.clone()
    }

    fn amount(&self, item: Entity) -> u32 {
        self.world().get::<Stack>(item).unwrap().amount
    }

    /// Picks up the item into the empty active hand.
    fn pick_up(&mut self, item: Entity) {
        assert!(self.active().is_none(), "the active hand is free");
        self.click(item);
        self.run_until("picked up", |s| s.active() == Some(item));
    }

    fn swap_hands(&mut self) {
        let before = self.inventory().active;
        self.send(ClientIntent::SwapHands);
        self.run_until("hands swapped", |s| s.inventory().active != before);
    }

    /// Waits until the mob's timed action has started and finished.
    fn wait_do_after(&mut self, secs: f64, what: &str) {
        let mob = self.mob();
        self.run_until(&format!("{what} starts"), |s| {
            s.world().get::<DoAfter>(mob).is_some()
        });
        self.run_until_for(secs, &format!("{what} finishes"), |s| {
            s.world().get::<DoAfter>(mob).is_none()
        });
    }

    fn pressure(&self, pos: TilePos) -> f64 {
        self.world().resource::<Atmos>().pressure(pos).unwrap()
    }
}

#[test]
fn pry_a_floor_tile_and_put_it_back() {
    let mut s = Station::start("pry_a_floor_tile_and_put_it_back");
    let air = s.pressure(WELDER_AT);
    let crowbar = s.item_at(CROWBAR_AT, "/obj/item/crowbar").unwrap();
    s.pick_up(crowbar);
    let floor = s.turf(WELDER_AT);
    s.click(floor);
    s.run_until("the floor is plating", |s| {
        s.turf_path(WELDER_AT) == "/turf/open/floor/plating"
    });
    // The message reaches the client a frame after the turf changed on the server.
    s.run_until("the pry message", |s| s.said("Вы снимаете плитку."));
    let plating = s.turf(WELDER_AT);
    assert_ne!(plating, floor, "a new turf took the old one's place");
    assert_eq!(
        s.world().get::<Baseturfs>(plating).unwrap().0,
        ["/turf/baseturf_bottom"]
    );
    assert!(
        s.item_at(WELDER_AT, "/obj/item/weldingtool").is_some(),
        "what lay on the tile stays"
    );
    assert_eq!(s.pressure(WELDER_AT), air, "the air is inherited");
    s.run_until("the client draws plating", |s| {
        let copy = s.client_copy(plating);
        copy.and_then(|c| s.players[0].app.world().get::<Appearance>(c))
            .is_some_and(|a| a.icon_state == PLATING_ICON)
    });
    s.run_until("the old turf is gone on the client", |s| {
        s.client_copy(floor).is_none()
    });

    // The tile it dropped, into the other hand, then onto the plating.
    let tile = s
        .item_at(WELDER_AT, "/obj/item/stack/tile/iron/base")
        .unwrap();
    assert_eq!(s.amount(tile), 1);
    s.swap_hands();
    s.pick_up(tile);
    s.click(plating);
    s.run_until("the floor is back", |s| {
        s.turf_path(WELDER_AT) == "/turf/open/floor/iron"
    });
    assert!(
        s.world().get_entity(tile).is_err(),
        "the last tile is used up"
    );
    let floor = s.turf(WELDER_AT);
    assert_eq!(
        s.world().get::<Baseturfs>(floor).unwrap().0,
        ["/turf/baseturf_bottom", "/turf/open/floor/plating"]
    );
    assert_eq!(s.pressure(WELDER_AT), air);
    s.run_until("the client draws the floor", |s| {
        let copy = s.client_copy(floor);
        copy.and_then(|c| s.players[0].app.world().get::<Appearance>(c))
            .is_some_and(|a| a.icon_state == FLOOR_ICON)
    });
}

#[test]
fn build_a_girder_and_a_wall_then_slice_it() {
    let mut s = Station::start("build_a_girder_and_a_wall_then_slice_it");
    s.0.without_mood();
    let iron = s.item_at(IRON_AT, "/obj/item/stack/sheet/iron").unwrap();
    assert_eq!(s.amount(iron), 10, "the map's `amount` edit");
    s.pick_up(iron);
    let floor = s.turf(BUILD_AT);

    // A right click with the sheets on the floor starts a displaced girder.
    s.click_with(floor, true);
    let started = s.now();
    s.wait_do_after(10.0, "the girder");
    let girder = s
        .obj_at(BUILD_AT, "/obj/structure/girder/displaced")
        .unwrap();
    assert!(s.now() - started >= 3.9, "4 s");
    assert_eq!(s.amount(iron), 8);
    assert_eq!(s.world().get::<Anchored>(girder), Some(&Anchored(false)));

    // Walking away interrupts: the wrench's 4 s do not finish when the mob moves.
    s.swap_hands();
    let wrench = s.item_at(WRENCH_AT, "/obj/item/wrench").unwrap();
    s.pick_up(wrench);
    s.click(girder);
    let mob = s.mob();
    s.run_until("the wrench starts", |s| {
        s.world().get::<DoAfter>(mob).is_some()
    });
    s.send(ClientIntent::DropActive);
    s.run_until("dropping the wrench interrupts", |s| {
        s.world().get::<DoAfter>(mob).is_none()
    });
    assert!(s.world().get_entity(girder).is_ok(), "still displaced");
    let wrench = s.item_at(SPAWN, "/obj/item/wrench").unwrap();
    s.pick_up(wrench);
    s.click(girder);
    s.wait_do_after(10.0, "securing");
    let girder = s.obj_at(BUILD_AT, "/obj/structure/girder").unwrap();
    assert_eq!(s.world().get::<Anchored>(girder), Some(&Anchored(true)));
    assert_eq!(
        s.world().get::<Integrity>(girder).unwrap().current,
        200.0,
        "replace_girder keeps the integrity ratio"
    );

    // Two sheets plate it into a wall.
    s.swap_hands();
    assert_eq!(s.active(), Some(iron));
    s.click(girder);
    s.wait_do_after(10.0, "the wall");
    assert_eq!(s.turf_path(BUILD_AT), "/turf/closed/wall");
    assert_eq!(s.amount(iron), 6);
    assert!(s.world().get_entity(girder).is_err());
    let wall = s.turf(BUILD_AT);
    assert!(s.world().get::<Density>(wall).is_some());
    assert_eq!(
        s.world().get::<Baseturfs>(wall).unwrap().0,
        [
            "/turf/baseturf_bottom",
            "/turf/open/floor/plating",
            "/turf/open/floor/iron"
        ]
    );
    assert!(
        s.world().resource::<Atmos>().air(BUILD_AT).is_none(),
        "a wall has no air"
    );
    s.run_until("the client draws the wall", |s| {
        let copy = s.client_copy(wall);
        copy.and_then(|c| s.players[0].app.world().get::<Appearance>(c))
            .is_some_and(|a| a.icon_state == WALL_ICON)
    });

    // An unlit welder refuses; lit, it slices the wall in 10 s into 2 sheets and a girder over
    // the floor it was built on.
    s.send(ClientIntent::DropActive);
    s.run_until("iron dropped", |s| s.active().is_none());
    s.swap_hands();
    s.send(ClientIntent::DropActive);
    s.run_until("wrench dropped", |s| s.active().is_none());
    let welder = s.item_at(WELDER_AT, "/obj/item/weldingtool").unwrap();
    s.pick_up(welder);
    s.click(wall);
    s.run_until("unlit refusal", |s| s.said("должен быть включён"));
    let slot = if s.inventory().active == 0 {
        InventorySlot::LeftHand
    } else {
        InventorySlot::RightHand
    };
    s.send(ClientIntent::ClickSlot(slot));
    s.run_until("lit", |s| s.world().get::<Welder>(welder).unwrap().on);
    let lit = s.world().get::<Appearance>(welder).unwrap();
    assert!(lit.overlays.iter().any(|o| o.icon_state == WELDER_ON_ICON));
    assert_eq!(s.world().get::<DmItem>(welder).unwrap().force, 15.0);
    s.click(wall);
    let started = s.now();
    s.wait_do_after(15.0, "slicing");
    assert!(s.now() - started >= 9.9, "10 s");
    assert_eq!(s.turf_path(BUILD_AT), "/turf/open/floor/iron");
    assert!(s.obj_at(BUILD_AT, "/obj/structure/girder").is_some());
    let sheets = s.item_at(BUILD_AT, "/obj/item/stack/sheet/iron").unwrap();
    assert_eq!(s.amount(sheets), 2);
    let fuel = s.world().get::<Welder>(welder).unwrap().fuel;
    assert!((18.0..20.0).contains(&fuel), "burnt while lit: {fuel}");
    assert!(
        s.world().resource::<Atmos>().air(BUILD_AT).is_some(),
        "the floor has air again"
    );
}

#[test]
fn smashing_the_window_vents_the_room() {
    let mut s = Station::start("smashing_the_window_vents_the_room");
    let room = SPAWN;
    let before = s.pressure(room);
    assert!(before > 100.0, "{before}");
    assert_eq!(s.pressure(TilePos::new(4, 1, 1)), 0.0);
    let toolbox = s.item_at(SPAWN, "/obj/item/storage/toolbox").unwrap();
    s.pick_up(toolbox);
    let window = s
        .obj_at(WINDOW_AT, "/obj/structure/window/fulltile")
        .unwrap();
    // Force 13 against melee armor 50: 6.5 per hit, 100 integrity, 16 hits.
    let mut hits = 0;
    while s.world().get_entity(window).is_ok() {
        let integrity = s.world().get::<Integrity>(window).unwrap().current;
        assert_eq!(integrity, 100.0 - 6.5 * hits as f32);
        s.click(window);
        hits += 1;
        assert!(hits <= 16, "too many hits");
        s.run_until("the hit lands", |s| {
            s.world().get_entity(window).is_err()
                || s.world().get::<Integrity>(window).unwrap().current < integrity
        });
    }
    assert_eq!(hits, 16);
    s.run_until("the hit message arrives", |s| {
        s.said("Вы атакуете full tile window с помощью toolbox.")
    });
    let shards = s
        .world()
        .resource::<SpatialIndex>()
        .on_tile(WINDOW_AT)
        .iter()
        .filter(|&&e| {
            s.world()
                .get::<DmItem>(e)
                .is_some_and(|i| i.path == "/obj/item/shard")
        })
        .count();
    assert_eq!(shards, 2, "a full-tile window leaves two shards");
    s.run_until_for(20.0, "the room loses its air", |s| {
        s.pressure(room) < before / 2.0
    });
}

/// A medium meteor (`ss13_game::events::meteors`, tg `/obj/effect/meteor/medium`) flying north
/// out of space into the bottom wall: the bump rams the wall (`ex_act(EXPLODE_HEAVY)`), which is
/// dismantled; the client sees the meteor on its way.
#[test]
fn a_meteor_breaks_the_wall() {
    use ss13_game::events::meteors::{Meteor, kind_by_path, spawn_meteor_at};
    let mut s = Station::start("a_meteor_breaks_the_wall");
    let wall = TilePos::new(7, 2, 1);
    let path = |s: &Station, pos| {
        let turf = s.turf(pos);
        s.world().get::<DmTurfBuild>(turf).map(|b| b.path.clone())
    };
    assert_eq!(path(&s, wall).as_deref(), Some("/turf/closed/wall"));
    let kind = kind_by_path("/obj/effect/meteor/medium").unwrap();
    let meteor = spawn_meteor_at(
        s.server.world_mut(),
        kind,
        TilePos::new(7, 1, 1),
        TilePos::new(7, 6, 1),
    )
    .unwrap();
    let mut seen = false;
    s.run_until("the meteor breaks the wall", |s| {
        seen |= s.net_id(meteor).is_some();
        path(s, wall).as_deref() != Some("/turf/closed/wall")
    });
    s.run_until("the meteor is spent", |s| {
        seen |= s.net_id(meteor).is_some();
        s.world().get::<Meteor>(meteor).is_none()
    });
    assert!(seen, "the client was sent the meteor");
    let now = path(&s, wall);
    assert!(
        matches!(
            now.as_deref(),
            Some("/turf/open/floor/plating" | "/turf/open/space/basic")
        ),
        "{now:?}"
    );
}
