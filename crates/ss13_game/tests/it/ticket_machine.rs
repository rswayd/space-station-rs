//! The ticket machine over a loopback server: an empty hand takes ticket #1 and no second one
//! while it is held; the button's controller serves it, and serving again makes it disperse.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::Inventory;
use ss13_game::service::tickets::{Ticket, TicketMachine, serve_next};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmObj;
use ss13_shared::TilePos;

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
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/paper": { "parent": "/obj/item", "declared_vars": { "default_raw_text": null } },
    "/obj/item/paper/fluff": { "parent": "/obj/item/paper", "declared_vars": {
        "default_raw_text": "Rule one" } },
    "/obj/item/pen": { "parent": "/obj/item" },
    "/obj/item/stamp": { "parent": "/obj/item" },
    "/obj/item/stamp/denied": { "parent": "/obj/item/stamp" },
    "/obj/item/paper_bin": { "parent": "/obj/item" },
    "/obj/item/dice": { "parent": "/obj/item", "declared_vars": { "sides": 6 } },
    "/obj/item/dice/d20": { "parent": "/obj/item/dice", "declared_vars": { "sides": 20 } },
    "/obj/item/book": { "parent": "/obj/item", "declared_vars": {
        "starting_title": null, "starting_author": null, "starting_content": null } },
    "/obj/item/book/law": { "parent": "/obj/item/book", "declared_vars": {
        "starting_title": "Space Law", "starting_content": "Do not." } },
    "/obj/item/book/codex_gigas": { "parent": "/obj/item/book", "declared_vars": {
        "starting_title": "the Codex Gigas",
        "starting_author": "Forces beyond your comprehension" } },
    "/obj/item/tgui_book": { "parent": "/obj/item" },
    "/obj/item/tgui_book/manual": { "parent": "/obj/item/tgui_book" },
    "/obj/item/tgui_book/manual/dsm": { "parent": "/obj/item/tgui_book/manual" },
    "/obj/item/tgui_book/manual/idc": { "parent": "/obj/item/tgui_book/manual" },
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
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 0,
        "use_power": 0, "idle_power_usage": 0, "active_power_usage": 0, "power_channel": 1,
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey" } },
    "/obj/machinery/ticket_machine": { "parent": "/obj/machinery", "declared_vars": {
        "name": "ticket machine" } },
    "/obj/item/ticket_machine_ticket": { "parent": "/obj/item", "declared_vars": {
        "name": "ticket" } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "density": 1, "anchored": 1 } },
    "/obj/structure/bookcase": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey" } },
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

/// The player starts at (2,3), the ticket machine north of it (2,4).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"t" = (/obj/machinery/ticket_machine,/turf/open/floor,/area/space)

(1,1,1) = {"
wwww
wtfw
wsfw
wffw
wwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

const MACHINE: &str = "/obj/machinery/ticket_machine";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("ticket_machine_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        MACHINE,
        "/obj/item/ticket_machine_ticket",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(1, |_| {});
        let a = s.players[0].mob;
        s.server
            .world_mut()
            .entity_mut(a)
            .insert(TilePos::new(2, 3, 1));
        Self(s)
    }

    fn machine(&mut self) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmObj)>();
        q.iter(world)
            .find(|(_, o)| o.path == MACHINE)
            .map(|(e, _)| e)
            .expect("the ticket machine")
    }

    fn tickets(&mut self) -> Vec<Entity> {
        let world = self.server.world_mut();
        let mut q = world.query_filtered::<Entity, With<Ticket>>();
        q.iter(world).collect()
    }
}

#[test]
fn a_ticket_is_taken_served_and_dispersed() {
    let mut s = Station::start("a_ticket_is_taken_served_and_dispersed");
    let mob = s.players[0].mob;
    let machine = s.machine();
    s.run_until("the machine is set up", |s| {
        s.try_get::<TicketMachine>(machine).is_some()
    });
    s.click(0, machine);
    s.run_until("ticket #1 is in hand", |s| {
        s.get::<Inventory>(mob)
            .active_item()
            .and_then(|i| s.try_get::<Ticket>(i).copied())
            .is_some_and(|t| t.number == 1)
    });
    // One ticket per holder.
    s.click(0, machine);
    for _ in 0..5 {
        s.tick();
    }
    assert_eq!(s.tickets().len(), 1);
    assert_eq!(s.get::<TicketMachine>(machine).ticket_number, 1);

    serve_next(s.server.world_mut(), None);
    let ticket = s.tickets()[0];
    let m = s.get::<TicketMachine>(machine).clone();
    assert_eq!((m.current, m.current_number), (Some(ticket), 1));

    serve_next(s.server.world_mut(), None);
    assert!(s.tickets().is_empty(), "the served ticket disperses");
}
