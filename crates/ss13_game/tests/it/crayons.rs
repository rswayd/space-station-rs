//! Crayons and spray cans on a fixture map: a crayon draws its stencil on the floor after 5 s in
//! its colour, for a charge, and writes text letter by letter (Cyrillic letters by their
//! `u<code>` states); the decal washes off only by scrubbing; a used-up crayon is gone. A capped
//! spray can does nothing, uncapped it draws on a wall at once and paints an item.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::crayons::{Crayon, CrayonArt};
use ss13_game::gifts::Gift;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::janitor::{CLEAN_TYPE_HARD_DECAL, Cleanable};
use ss13_game::stickers::Stickers;
use ss13_game::syndie_spraycan::{RuneStage, TraitorCan, TraitorRune};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, Turf};
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot};
use ss13_shared::{Appearance, TilePos};
use ss13_world::SpatialIndex;

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
    "/obj/effect/decal": { "parent": "/obj/effect" },
    "/obj/effect/decal/cleanable": { "parent": "/obj/effect/decal" },
    "/obj/effect/decal/cleanable/crayon": { "parent": "/obj/effect/decal/cleanable",
        "declared_vars": { "name": "rune", "icon": { "resource": "icons/effects/crayondecal.dmi" },
        "icon_state": "rune1" } },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": { "name": "toy" } },
    "/obj/item/toy/crayon": { "parent": "/obj/item/toy", "declared_vars": { "name": "crayon" } },
    "/obj/item/toy/crayon/red": { "parent": "/obj/item/toy/crayon" },
    "/obj/item/toy/crayon/spraycan": { "parent": "/obj/item/toy/crayon" },
    "/obj/item/soap": { "parent": "/obj/item", "declared_vars": { "name": "soap" } },
    "/obj/item/sticker": { "parent": "/obj/item", "declared_vars": { "name": "sticker",
        "icon": { "resource": "icons/obj/toys/stickers.dmi" }, "icon_state": "smile" } },
    "/obj/item/sticker/skub": { "parent": "/obj/item/sticker", "declared_vars": {
        "icon_state": "skub" } },
    "/obj/item/gift": { "parent": "/obj/item", "declared_vars": { "name": "gift",
        "icon": { "resource": "icons/obj/storage/wrapping.dmi" },
        "icon_state": "giftdeliverypackage3" } },
    "/obj/item/traitor_spraycan": { "parent": "/obj/item", "declared_vars": {
        "name": "seditious spraycan" } },
    "/obj/effect/decal/cleanable/traitor_rune": { "parent": "/obj/effect/decal/cleanable",
        "declared_vars": { "name": "syndicate graffiti",
        "icon": { "resource": "icons/effects/96x96.dmi" }, "icon_state": "traitor_rune_outline",
        "pixel_x": -32, "pixel_y": -32 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (3,2), a toy `T` east of it.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"T" = (/obj/item/toy,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfsTw
wwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 2, 1);
const WEST: TilePos = TilePos::new(2, 2, 1);
const TOY: TilePos = TilePos::new(4, 2, 1);
const NORTH_WALL: TilePos = TilePos::new(3, 3, 1);
/// `BIG`: the spawn (3,3) in a 5x3 room; the tile east of it has a clear 3x3 around it.
const BIG: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wfffffw
wfsfffw
wfffffw
wwwwwww
"}
"##;
const BIG_EAST: TilePos = TilePos::new(4, 3, 1);
const RUNE_DMI: &str = "icons/effects/96x96.dmi";
const DMI: &str = "icons/effects/crayondecal.dmi";

const STATES: [(&str, &str); 16] = [
    ("icons/obj/toys/stickers.dmi", "smile"),
    ("icons/obj/toys/stickers.dmi", "skub"),
    ("icons/obj/storage/wrapping.dmi", "giftdeliverypackage1"),
    ("icons/obj/storage/wrapping.dmi", "giftdeliverypackage3"),
    (RUNE_DMI, "traitor_rune_outline"),
    (RUNE_DMI, "traitor_rune_done"),
    (RUNE_DMI, "traitor_rune_sheen"),
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    (DMI, "rune1"),
    (DMI, "rune2"),
    (DMI, "u1072"),
    (DMI, "skull"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("crayons_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/decal/cleanable/crayon",
        "/obj/item/toy",
        "/obj/item/toy/crayon/red",
        "/obj/item/toy/crayon/spraycan",
        "/obj/item/soap",
        "/obj/item/sticker/skub",
        "/obj/item/gift",
        "/obj/item/traitor_spraycan",
        "/obj/effect/decal/cleanable/traitor_rune",
        "/obj/item/card/id",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), ("big.dmm", BIG)],
    )
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start() -> Self {
        Self::start_on("map.dmm")
    }

    fn start_on(map: &str) -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &[map]), |_| {});
        s.join_all(1, |_| {});
        s.without_mood();
        Self(s)
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn turf(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Turf>(e).is_some())
            .expect("a turf")
    }

    fn art_on(&self, pos: TilePos) -> Vec<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| world.get::<CrayonArt>(e).is_some())
            .collect()
    }

    fn state(&self, dmi: &str, state: &str) -> ss13_shared::IconStateId {
        self.server
            .world()
            .resource::<ss13_mapload::IconIndex>()
            .get(dmi, state)
            .expect("fixture state")
    }

    /// A new `path` item straight into the active hand, the old one gone.
    fn give_new(&mut self, path: &str) -> Entity {
        let mob = self.mob();
        let world = self.server.world_mut();
        let old = {
            let inventory = world.get::<Inventory>(mob).unwrap();
            inventory.hands[inventory.active]
        };
        if let Some(old) = old {
            world.despawn(old);
        }
        let item = ss13_game::construction::spawn_obj(world, path, SPAWN).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
        item
    }

    fn crayon(&mut self, item: Entity) -> Mut<'_, Crayon> {
        self.server.world_mut().get_mut::<Crayon>(item).unwrap()
    }
}

#[test]
fn a_crayon_draws_and_writes_on_the_floor() {
    let mut s = Station::start();
    let crayon = s.give_new("/obj/item/toy/crayon/red");
    assert_eq!(s.get::<Crayon>(crayon).charges_left, 30.0);
    s.crayon(crayon).drawtype = "rune2".into();
    s.run_for(1.0);
    let floor = s.turf(WEST);
    s.click(0, floor);
    let started = s.now();
    s.run_until("a rune drawn", |s| !s.art_on(WEST).is_empty());
    assert!(s.now() - started >= 4.9, "DRAW_TIME 5 s");
    let rune = s.art_on(WEST)[0];
    assert_eq!(s.get::<CrayonArt>(rune).drawing, "rune2");
    let look = s.get::<Appearance>(rune).clone();
    assert_eq!(look.icon_state, s.state(DMI, "rune2"));
    assert_eq!(
        look.color,
        Some([0xDA, 0x00, 0x00, 0xFF]),
        "COLOR_CRAYON_RED"
    );
    assert_eq!(s.get::<Cleanable>(rune).clean_type, CLEAN_TYPE_HARD_DECAL);
    assert_eq!(s.get::<Crayon>(crayon).charges_left, 29.0);
    // Mopping (`CLEAN_WASH`) leaves it; scrubbing takes it off.
    assert!(!ss13_game::janitor::wash(
        s.server.world_mut(),
        rune,
        ss13_game::janitor::CLEAN_WASH
    ));
    assert!(ss13_game::janitor::wash(
        s.server.world_mut(),
        rune,
        ss13_game::janitor::CLEAN_SCRUB
    ));

    // Text goes down letter by letter; a Cyrillic letter is its `u<code>` state.
    {
        let mut c = s.crayon(crayon);
        c.text = ss13_game::crayons::strip_text("Аб!");
        c.drawtype = "a".into();
    }
    s.run_for(0.5);
    let floor = s.turf(WEST);
    s.click(0, floor);
    s.run_until("a letter written", |s| !s.art_on(WEST).is_empty());
    let letter = s.art_on(WEST)[0];
    assert_eq!(s.get::<CrayonArt>(letter).drawing, "u1072");
    assert_eq!(
        s.get::<Appearance>(letter).icon_state,
        s.state(DMI, "u1072")
    );
    assert_eq!(s.get::<Crayon>(crayon).text, "б!");

    // The last charge is spent and the crayon is used up.
    {
        let mut c = s.crayon(crayon);
        c.text.clear();
        c.drawtype = "skull".into();
        c.charges_left = 1.0;
    }
    s.run_for(0.5);
    s.click(0, floor);
    s.run_until("the crayon used up", |s| {
        s.server.world().get_entity(crayon).is_err()
    });
    assert_eq!(s.art_on(WEST).len(), 2);
    let inventory = s.get::<Inventory>(s.mob());
    assert!(inventory.hands[inventory.active].is_none());
}

#[test]
fn a_spray_can_sprays_walls_at_once_and_paints_items() {
    let mut s = Station::start();
    let can = s.give_new("/obj/item/toy/crayon/spraycan");
    assert!(s.get::<Crayon>(can).capped);
    s.crayon(can).drawtype = "rune1".into();
    s.run_for(1.0);
    let wall = s.turf(NORTH_WALL);
    s.click(0, wall);
    s.run_for(1.0);
    assert!(s.art_on(NORTH_WALL).is_empty(), "capped");
    s.crayon(can).capped = false;
    s.click(0, wall);
    s.run_until("wall graffiti", |s| !s.art_on(NORTH_WALL).is_empty());
    let color = s.get::<Crayon>(can).color;
    assert_eq!(s.get::<Crayon>(can).charges_left, 29.0);
    // An item on the floor is painted for 2 charges.
    s.run_for(1.0);
    let toy = {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(TOY)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some())
            .unwrap()
    };
    s.click(0, toy);
    s.run_until("toy painted", |s| {
        s.get::<Appearance>(toy).color == Some(color)
    });
    assert_eq!(s.get::<Crayon>(can).charges_left, 27.0);
    assert!(
        s.try_get::<CarriedBy>(toy).is_none(),
        "painted, not picked up"
    );
}

#[test]
fn the_seditious_can_draws_a_rune_that_resists_cleaning_for_a_while() {
    let mut s = Station::start_on("big.dmm");
    let can = s.give_new("/obj/item/traitor_spraycan");
    s.run_for(1.0);
    let floor = s.turf(BIG_EAST);
    s.click(0, floor);
    let started = s.now();
    let rune_on = |s: &Station| {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(BIG_EAST)
            .iter()
            .copied()
            .find(|&e| world.get::<TraitorRune>(e).is_some())
    };
    s.run_until("the rune finished", |s| {
        rune_on(s).is_some_and(|r| s.get::<TraitorRune>(r).stage == RuneStage::Complete)
    });
    assert!(s.now() - started >= 8.9, "three steps of 3 s");
    let rune = rune_on(&s).unwrap();
    assert_eq!(
        s.get::<Appearance>(rune).icon_state,
        s.state(RUNE_DMI, "traitor_rune_sheen")
    );
    assert!(s.get::<TraitorCan>(can).expended);
    let world = s.server.world_mut();
    assert!(
        !ss13_game::janitor::wash(world, rune, ss13_game::janitor::CLEAN_SCRUB),
        "clean-proof"
    );
    // Five minutes later it is an ordinary hard decal.
    let now = s.now();
    s.server
        .world_mut()
        .get_mut::<TraitorRune>(rune)
        .unwrap()
        .removable_at = now;
    s.run_for(0.2);
    assert_eq!(s.get::<TraitorRune>(rune).stage, RuneStage::Removable);
    assert!(ss13_game::janitor::wash(
        s.server.world_mut(),
        rune,
        ss13_game::janitor::CLEAN_SCRUB
    ));
}

#[test]
fn a_sticker_sticks_where_clicked_and_washes_off() {
    let mut s = Station::start_on("big.dmm");
    let sticker = s.give_new("/obj/item/sticker/skub");
    s.run_for(1.0);
    let floor = s.turf(BIG_EAST);
    let at = ClickModifiers {
        pixel: Some([5, -3]),
        ..Default::default()
    };
    s.click_with(0, floor, at);
    s.run_until("stuck", |s| s.try_get::<Stickers>(floor).is_some());
    assert!(s.try_get::<TilePos>(sticker).is_none(), "out of the world");
    let inventory = s.get::<Inventory>(s.mob());
    assert!(inventory.hands[inventory.active].is_none());
    let overlay = &s.get::<Stickers>(floor).0[0].overlay;
    assert_eq!(overlay.pixel_offset, [5, -3]);
    assert_eq!(
        overlay.icon_state,
        s.state("icons/obj/toys/stickers.dmi", "skub")
    );
    assert!(s.get::<Appearance>(floor).overlays.contains(overlay));
    let mut lines = Vec::new();
    ss13_game::stickers::examine_lines(s.server.world(), floor, &mut lines);
    assert_eq!(lines.len(), 1, "skub's examine text");
    // Mopping the floor peels it off onto the tile.
    let world = s.server.world_mut();
    assert!(ss13_game::janitor::wash_turf(
        world,
        BIG_EAST,
        ss13_game::janitor::CLEAN_WASH,
        false
    ));
    assert_eq!(s.try_get::<TilePos>(sticker), Some(&BIG_EAST));
    assert!(s.try_get::<Stickers>(floor).is_none());
    assert!(s.get::<Appearance>(floor).overlays.is_empty());
}

#[test]
fn a_gift_opens_into_a_present_in_hand() {
    let mut s = Station::start_on("big.dmm");
    let gift = s.give_new("/obj/item/gift");
    s.run_for(0.2);
    s.server.world_mut().get_mut::<Gift>(gift).unwrap().contains =
        Some("/obj/item/toy/crayon/red".into());
    let offset = s.get::<Appearance>(gift).pixel_offset;
    assert!(offset.iter().all(|o| (-10..=10).contains(o)));
    s.run_for(1.0);
    let hand = if s.get::<Inventory>(s.mob()).active == 0 {
        InventorySlot::LeftHand
    } else {
        InventorySlot::RightHand
    };
    s.send(0, ClientIntent::ClickSlot(hand));
    s.run_until("opened", |s| s.server.world().get_entity(gift).is_err());
    let inventory = s.get::<Inventory>(s.mob());
    let present = inventory.hands[inventory.active].expect("the present in hand");
    assert_eq!(s.get::<DmItem>(present).path, "/obj/item/toy/crayon/red");
}
