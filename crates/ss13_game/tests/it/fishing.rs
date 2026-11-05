//! Fishing over a loopback server: a goldfish goes into an aquarium, a rod cast at the
//! aquarium hooks it after the bite, an instant win is refused and a win after the minimum
//! time puts the fish in hand; a cast at water starts fishing, a click while waiting is a
//! miss that delays the bite, and using the rod stops.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::fishing::FishingMinigameUi;
use ss13_game::fishing::fish::Fish;
use ss13_game::fishing::server::{Fishing, Phase};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_tgui::UiInterface;
use ss13_tgui::server::UiWindow;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/fish": { "parent": "/obj/item", "declared_vars": {
        "average_size": 50, "average_weight": 1000, "fishing_difficulty_modifier": 0,
        "fish_traits": { "list": [] }, "required_fluid_type": "Freshwater",
        "required_temperature_min": 273.15, "required_temperature_max": 373.15,
        "feeding_frequency": 3000, "fish_movement_type": { "path": "/datum/fish_movement" },
        "stable_population": 1, "icon_state_dead": null, "death_text": "%SRC dies." } },
    "/obj/item/fish/goldfish": { "parent": "/obj/item/fish", "declared_vars": {
        "name": "goldfish", "average_size": 20, "average_weight": 200,
        "required_temperature_min": 291.15, "required_temperature_max": 299.15 } },
    "/obj/item/fishing_rod": { "parent": "/obj/item", "declared_vars": {
        "cast_range": 3, "difficulty_modifier": 0, "completion_speed_mult": 1,
        "bait_speed_mult": 1, "deceleration_mult": 1, "bounciness_mult": 1, "gravity_mult": 1,
        "bait_height_mult": 1, "line": { "path": "/obj/item/fishing_line" },
        "hook": { "path": "/obj/item/fishing_hook" }, "w_class": 5 } },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "density": 1, "anchored": 1 } },
    "/obj/structure/aquarium": { "parent": "/obj/structure", "declared_vars": {
        "name": "aquarium", "icon": { "resource": "i.dmi" }, "icon_state": "card_grey" } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/water": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "fishing_datum": { "path": "/datum/fish_source/river" } } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// The player starts at (2,3), the water north of it (2,4) and (3,4); the aquarium at (5,4),
/// the rod and the goldfish east of the player.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"~" = (/turf/open/water,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"a" = (/obj/structure/aquarium,/turf/open/floor,/area/space)
"r" = (/obj/item/fishing_rod,/turf/open/floor,/area/space)
"g" = (/obj/item/fish/goldfish,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
w~~faww
wsfrgfw
wfffffw
wwwwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

const ROD: &str = "/obj/item/fishing_rod";
const GOLDFISH: &str = "/obj/item/fish/goldfish";
const AQUARIUM: &str = "/obj/structure/aquarium";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("fishing_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        ROD,
        GOLDFISH,
        AQUARIUM,
        "/turf/open/floor",
        "/turf/open/water",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str, at: TilePos) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(1, |_| {});
        let a = s.players[0].mob;
        s.server.world_mut().entity_mut(a).insert(at);
        Self(s)
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn item(&mut self, path: &str) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmItem)>();
        q.iter(world)
            .find(|(_, i)| i.path == path)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("{path} on the map"))
    }

    fn aquarium(&mut self) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &ss13_mapload::DmObj)>();
        q.iter(world)
            .find(|(_, o)| o.path == AQUARIUM)
            .map(|(e, _)| e)
            .expect("the aquarium")
    }

    fn turf(&mut self, pos: TilePos) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query_filtered::<(Entity, &TilePos), With<ss13_mapload::Turf>>();
        q.iter(world)
            .find(|(_, p)| **p == pos)
            .map(|(e, _)| e)
            .expect("a turf there")
    }

    /// `item` into the active hand, whatever was there dropped on the floor.
    fn put_in_hand(&mut self, item: Entity) {
        let mob = self.mob();
        let world = self.server.world_mut();
        let hand = world.get::<Inventory>(mob).unwrap().active;
        if let Some(old) = world.get::<Inventory>(mob).unwrap().hands[hand] {
            world
                .entity_mut(old)
                .remove::<CarriedBy>()
                .insert(TilePos::new(6, 2, 1));
        }
        world
            .entity_mut(item)
            .remove::<(TilePos, StoredIn)>()
            .insert(CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = Some(item);
    }

    fn fishing(&self) -> Option<&Fishing> {
        self.try_get::<Fishing>(self.mob())
    }

    /// The bite comes now.
    fn hurry_bite(&mut self) {
        let mob = self.mob();
        self.server
            .world_mut()
            .get_mut::<Fishing>(mob)
            .expect("fishing")
            .next_at = 0.0;
    }

    fn window(&mut self, interface: &str) -> Option<u32> {
        let link = self.players[0].link;
        let world = self.server.world_mut();
        let mut q = world.query::<&UiWindow>();
        q.iter(world)
            .filter(|w| w.client == link && w.interface == interface)
            .map(|w| w.id)
            .max()
    }

    fn active_slot(&self) -> InventorySlot {
        if self.get::<Inventory>(self.mob()).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        }
    }
}

#[test]
fn a_rod_catches_the_aquarium_fish_after_the_minigame() {
    let mut s = Station::start(
        "a_rod_catches_the_aquarium_fish_after_the_minigame",
        TilePos::new(4, 4, 1),
    );
    let mob = s.mob();
    let (aquarium, goldfish, rod) = (s.aquarium(), s.item(GOLDFISH), s.item(ROD));
    s.run_until("the goldfish is a fish", |s| {
        s.try_get::<Fish>(goldfish).is_some()
    });
    assert_eq!(s.get::<Fish>(goldfish).size, 20.0);

    s.put_in_hand(goldfish);
    s.click(0, aquarium);
    s.run_until("the goldfish is in the aquarium", |s| {
        s.try_get::<StoredIn>(goldfish)
            .is_some_and(|i| i.0 == aquarium)
    });

    s.put_in_hand(rod);
    s.click(0, aquarium);
    s.run_until("the line is cast", |s| s.fishing().is_some());
    assert_eq!(s.fishing().unwrap().spot, aquarium);
    s.hurry_bite();
    s.run_until("something bites", |s| {
        s.fishing().is_some_and(|f| f.phase == Phase::Biting)
    });
    s.click(0, aquarium);
    s.run_until("the minigame opens", |s| {
        s.window(FishingMinigameUi::NAME).is_some()
    });
    let window = s.window(FishingMinigameUi::NAME).unwrap();
    // Too soon: the completion cannot have filled yet.
    s.act(0, window, "win", &[]);
    for _ in 0..testing::steps(1.0) {
        s.tick();
    }
    assert!(s.fishing().is_some_and(|f| f.phase == Phase::Minigame));
    for _ in 0..testing::steps(12.0) {
        s.tick();
    }
    s.act(0, window, "win", &[]);
    // The rod keeps the active hand: the catch goes to the other one.
    s.run_until("the goldfish is in hand", |s| {
        s.get::<Inventory>(mob).hands.contains(&Some(goldfish))
    });
    assert!(s.fishing().is_none());
    assert!(s.try_get::<StoredIn>(goldfish).is_none());
    assert!(s.get::<Fish>(goldfish).alive);
}

#[test]
fn a_cast_at_water_waits_misses_and_stops() {
    let mut s = Station::start(
        "a_cast_at_water_waits_misses_and_stops",
        TilePos::new(2, 3, 1),
    );
    let rod = s.item(ROD);
    let water = s.turf(TilePos::new(2, 4, 1));
    s.put_in_hand(rod);
    s.click(0, water);
    s.run_until("the line is cast", |s| s.fishing().is_some());
    let fishing = s.fishing().unwrap().clone();
    assert_eq!(fishing.spot, water);
    assert_eq!(fishing.source.path, "/datum/fish_source/river");
    assert_eq!(fishing.phase, Phase::Waiting);

    for _ in 0..testing::steps(0.5) {
        s.tick();
    }
    s.click(0, water);
    s.run_until("the miss delays the bite", |s| {
        s.fishing().is_some_and(|f| f.next_at > fishing.next_at)
    });

    let slot = s.active_slot();
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("fishing stops", |s| s.fishing().is_none());
}
