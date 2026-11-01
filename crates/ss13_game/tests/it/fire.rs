//! Fire and space wind over real WebTransport on 127.0.0.1, on a fixture room:
//! - plasma in the room's air lit by a welder in the player's hand burns: the air heats up,
//!   plasma turns into CO2, the client draws the fire, the player catches fire and burns, a
//!   wooden table burns down; resisting rolls on the floor;
//! - a window to space breaks: the space wind pushes an item and the player toward the breach.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::{Atmos, AtmosPlugin, GasId, HotspotEffect};
use ss13_game::breath::{AlertCategory, Alerts, Respiration};
use ss13_game::combat::NextMove;
use ss13_game::fire::{Burning, FireStacks, Rolling};
use ss13_game::health::{BodyPosition, Health};
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::Welder;
use ss13_mapload::{DmItem, DmObj};
use ss13_net::chat::client::ChatReceived;
use ss13_shared::protocol::{ClientIntent, InventorySlot, NetId};
use ss13_shared::{Appearance, IconStateId, Optics, TilePos};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
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
    "/obj/item/weldingtool": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "welder", "inhand_icon_state": "welder", "tool_behaviour": "welder",
        "max_fuel": 20, "heat": 3800, "force": 3 } },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/toolbox": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "toolbox", "icon_state": "toolbox", "force": 13 } },
    "/obj/item/shard": { "parent": "/obj/item", "declared_vars": { "icon_state": "large",
        "name": "shard" } },
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
    "/obj/structure/table": { "parent": "/obj/structure", "declared_vars": {
        "name": "table", "icon_state": "table", "density": 1, "max_integrity": 100 } },
    "/obj/structure/table/wood": { "parent": "/obj/structure/table", "declared_vars": {
        "name": "wooden table", "resistance_flags": 4, "max_integrity": 70 } },
    "/obj/structure/window": { "parent": "/obj/structure", "declared_vars": {
        "name": "window", "icon_state": "window", "density": 1, "can_atmos_pass": -1,
        "flags_1": 2, "max_integrity": 50, "state": 0, "reinf": 0, "fulltile": 0,
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
    "/turf/open/floor/iron": { "parent": "/turf/open/floor" },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "overfloor_placed": 0,
        "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall",
        "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The player spawns at `A` (4,3) on a welder, a
/// wooden table `T` (6,4) and a toolbox `B` (4,4) stand in the room, the full-tile window `W`
/// (4,2) separates it from space (y = 1).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor/iron,/area/space)
"s" = (/turf/open/space/basic,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/obj/item/weldingtool,/turf/open/floor/iron,/area/space)
"B" = (/obj/item/storage/toolbox,/turf/open/floor/iron,/area/space)
"T" = (/obj/structure/table/wood,/turf/open/floor/iron,/area/space)
"W" = (/obj/structure/window/fulltile,/turf/open/floor/plating,/area/space)

(1,1,1) = {"
wwwwwwwww
wfffffffw
wffBfTffw
wffAffffw
wwwWwwwww
sssssssss
"}
"##;

const SPAWN: TilePos = TilePos::new(4, 3, 1);
const TOOLBOX_AT: TilePos = TilePos::new(4, 4, 1);
const TABLE_AT: TilePos = TilePos::new(6, 4, 1);
const WINDOW_AT: TilePos = TilePos::new(4, 2, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 18] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toolbox"),
    ("i.dmi", "welder"),
    ("i.dmi", "welder100"),
    ("i.dmi", "welder-on"),
    ("i.dmi", "large"),
    ("s.dmi", "table"),
    ("s.dmi", "window"),
    ("l.dmi", "card-id"),
    ("icons/effects/fire.dmi", "light"),
    ("icons/effects/fire.dmi", "medium"),
    ("icons/effects/fire.dmi", "heavy"),
    ("icons/mob/effects/onfire.dmi", "human_small_fire"),
    ("icons/mob/effects/onfire.dmi", "human_big_fire"),
];

const SMALL_FIRE_ICON: IconStateId = IconStateId(16);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("fire_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/storage/toolbox",
        "/obj/item/weldingtool",
        "/obj/item/shard",
        "/obj/structure/table/wood",
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

    /// Fires and their spread take a while of game time.
    fn run_until(&mut self, what: &str, done: impl FnMut(&mut Self) -> bool) {
        self.run_until_for(60.0, what, done);
    }
}

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(AtmosPlugin);
        });
        s.join("firefighter", |app| {
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

    fn click(&mut self, e: Entity) {
        let mob = self.mob();
        self.run_until("off click cooldown", |s| {
            s.now() >= s.world().get::<NextMove>(mob).unwrap().0
        });
        self.run_until("the target is replicated", |s| s.net_id(e).is_some());
        self.0.click(0, e);
    }

    fn inventory(&self) -> Inventory {
        self.world().get::<Inventory>(self.mob()).unwrap().clone()
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

    fn atmos(&self) -> &Atmos {
        self.world().resource::<Atmos>()
    }

    fn fire(&self) -> Option<FireStacks> {
        self.world().get::<FireStacks>(self.mob()).cloned()
    }
}

#[test]
fn a_welder_lights_plasma_and_the_room_burns() {
    let mut s = Station::start("a_welder_lights_plasma_and_the_room_burns");
    // Plasma in every room tile (tg `atmos_spawn_air`), at room temperature: nothing reacts.
    let room: Vec<TilePos> = (2..=8)
        .flat_map(|x| (3..=5).map(move |y| TilePos::new(x, y, 1)))
        .collect();
    {
        let mut atmos = s.server.world_mut().resource_mut::<Atmos>();
        for &pos in &room {
            assert!(atmos.spawn_air(pos, "plasma=15;TEMP=293.15").unwrap());
        }
    }
    let plasma = |s: &Station| -> f64 {
        room.iter()
            .map(|&p| s.atmos().moles(p, GasId::PLASMA).unwrap())
            .sum()
    };
    let start_plasma = plasma(&s);
    let table = s.obj_at(TABLE_AT, "/obj/structure/table/wood").unwrap();
    for _ in 0..40 {
        s.step();
    }
    assert_eq!(s.atmos().hotspots(), 0, "plasma alone does not burn");
    assert!((plasma(&s) - start_plasma).abs() < 1e-6);

    // Pick up the welder under the player and light it (its hand slot).
    let welder = s.item_at(SPAWN, "/obj/item/weldingtool").unwrap();
    s.click(welder);
    s.run_until("picked up", |s| s.inventory().active_item() == Some(welder));
    let slot = if s.inventory().active == 0 {
        InventorySlot::LeftHand
    } else {
        InventorySlot::RightHand
    };
    s.send(ClientIntent::ClickSlot(slot));
    s.run_until("lit", |s| s.world().get::<Welder>(welder).unwrap().on);
    let lit_at = s.now();
    // `open_flame()` within a second lights the player's tile.
    s.run_until("a fire starts", |s| s.atmos().hotspot(SPAWN).is_some());
    assert!(s.now() - lit_at <= 1.1, "{}", s.now() - lit_at);
    s.run_until("the hotspot entity", |s| {
        s.find(SPAWN, |w, e| w.get::<HotspotEffect>(e).is_some())
            .is_some()
    });
    let effect = s
        .find(SPAWN, |w, e| w.get::<HotspotEffect>(e).is_some())
        .unwrap();
    assert!(s.world().get::<Optics>(effect).unwrap().light.is_some());
    s.run_until("the client draws the fire", |s| {
        s.client_copy(effect)
            .is_some_and(|c| s.players[0].app.world().get::<Appearance>(c).is_some())
    });

    // The player stands in the fire: fire stacks, burning, the alert, burn damage.
    s.run_until("the player catches fire", |s| {
        s.fire().is_some_and(|f| f.on_fire)
    });
    s.run_until("the message", |s| s.said("Вы загорелись!"));
    let alerts = s.world().get::<Alerts>(s.mob()).unwrap();
    assert!(alerts.get(AlertCategory::Fire).is_some());
    s.run_until("the room heats up", |s| {
        s.atmos().temperature(TilePos::new(6, 3, 1)).unwrap() > 1000.0
    });
    s.run_until("burns", |s| {
        s.world().get::<Health>(s.mob()).unwrap().burn > 5.0
    });
    eprintln!("fire: burn damage 5 after {:.1} s", s.now() - lit_at);
    let body = s
        .world()
        .get::<Respiration>(s.mob())
        .unwrap()
        .bodytemperature;
    assert!(body > 340.0, "the fire heats the body: {body}");
    s.run_until("plasma burns into CO2", |s| plasma(s) < start_plasma * 0.5);
    let co2: f64 = room
        .iter()
        .map(|&p| s.atmos().moles(p, GasId::CO2).unwrap())
        .sum();
    assert!(co2 > 10.0, "{co2}");
    assert!(s.atmos().hotspots() > 1, "the fire spread");

    // The wooden table caught fire and burns down (70 integrity, 10/s plus exposures).
    s.run_until("the table burns", |s| {
        s.world().get_entity(table).is_err() || s.world().get::<Burning>(table).is_some()
    });
    s.run_until("the table is gone", |s| {
        s.world().get_entity(table).is_err()
    });
    eprintln!(
        "fire: {:.1} s after lighting: plasma {:.1} -> {:.1} mol, {} hotspots, (6,3) at {:.0} K, \
         burn {:.1}, body {:.0} K",
        s.now() - lit_at,
        start_plasma,
        plasma(&s),
        s.atmos().hotspots(),
        s.atmos().temperature(TilePos::new(6, 3, 1)).unwrap(),
        s.world().get::<Health>(s.mob()).unwrap().burn,
        body
    );
}

#[test]
fn stop_drop_and_roll_puts_the_fire_out() {
    let mut s = Station::start("stop_drop_and_roll_puts_the_fire_out");
    let mob = s.mob();
    // Set alight in station air (tg `fire_act`: 3 stacks, `ignite_mob`).
    ss13_game::fire::fire_act(s.server.world_mut(), mob, 1000.0);
    assert!(s.fire().is_some_and(|f| f.on_fire && f.stacks == 3.0));
    s.run_until("the player's fire overlay", |s| {
        s.client_copy(mob).is_some_and(|c| {
            s.players[0]
                .app
                .world()
                .get::<Appearance>(c)
                .is_some_and(|a| a.overlays.iter().any(|o| o.icon_state == SMALL_FIRE_ICON))
        })
    });
    assert!(
        s.world().get::<Optics>(mob).unwrap().light.is_some(),
        "it glows"
    );
    s.send(ClientIntent::Resist);
    s.run_until("rolling", |s| s.world().get::<Rolling>(mob).is_some());
    s.run_until("lies down", |s| {
        s.world().get::<BodyPosition>(mob).unwrap().lying()
    });
    s.run_until("the roll message", |s| {
        s.said("Вы останавливаетесь, падаете и катаетесь!")
    });
    // 3 stacks: 0.25 at once, then 1 per 0.8 s (and 0.1 per 2 s of decay): out in ~2.4 s.
    let started = s.now();
    s.run_until("put out", |s| s.fire().is_none());
    let took = s.now() - started;
    assert!((1.5..3.0).contains(&took), "{took}");
    s.run_until("the success message", |s| s.said("Вы потушили себя."));
    assert!(s.world().get::<Optics>(mob).is_none(), "no more glow");
    let alerts = s.world().get::<Alerts>(mob).unwrap();
    assert!(alerts.get(AlertCategory::Fire).is_none());
    s.run_until("stands up", |s| {
        !s.world().get::<BodyPosition>(mob).unwrap().lying()
    });
}

#[test]
fn a_breach_blows_things_toward_space() {
    let mut s = Station::start("a_breach_blows_things_toward_space");
    let toolbox = s.item_at(TOOLBOX_AT, "/obj/item/storage/toolbox").unwrap();
    let window = s
        .obj_at(WINDOW_AT, "/obj/structure/window/fulltile")
        .unwrap();
    // The window breaks (tg `deconstruct` of a destroyed window: shards, the tile opens).
    ss13_game::obj_damage::deconstruct(s.server.world_mut(), window, false);
    // The space wind takes the player and the toolbox south, through the breach.
    s.run_until("the player is blown toward space", |s| {
        s.pos(s.mob()).y < SPAWN.y
    });
    s.run_until("the toolbox is blown toward space", |s| {
        s.world()
            .get::<TilePos>(toolbox)
            .is_none_or(|p| p.y < TOOLBOX_AT.y)
    });
}
