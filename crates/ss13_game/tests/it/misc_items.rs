//! Single items over real WebTransport on 127.0.0.1 (`ss13_game::misc_items`): the station
//! charter renames the station (a standard name at once, another after the admins' wait unless
//! rejected) and is used up.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::NextMove;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::misc_items::charter::{Charter, StationName, reject_proposed};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_net::chat::client::ChatReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_tgui::prompt::{Prompt, PromptAnswer, PromptAnswered};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2, "max_integrity": 100 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/station_charter": { "parent": "/obj/item", "declared_vars": {
        "name": "station charter", "icon_state": "charter" } },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wsffw
wfffw
wwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 3, 1);

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "charter"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("misc_items_fixture");
    std::fs::create_dir_all(dir.join("strings")).unwrap();
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/station_charter",
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
        &[
            ("map.dmm", MAP),
            ("strings/station_prefixes.txt", "Imperial\n"),
            ("strings/station_names.txt", "Aegis\nSpace\n"),
            ("strings/station_suffixes.txt", "Station\nOutpost\n"),
            ("strings/greek_letters.txt", "Alpha\n"),
            ("strings/phonetic_alphabet.txt", "Bravo\n"),
            ("strings/numbers_as_words.txt", "One\n"),
        ],
    )
}

#[derive(Resource, Default)]
struct Received(Vec<String>);

fn record(mut chat: MessageReader<ChatReceived>, mut received: ResMut<Received>) {
    for ChatReceived(message) in chat.read() {
        received.0.push(message.plain_text());
    }
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start() -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join("p0", |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        let s = Self(s);
        assert_eq!(*s.get::<TilePos>(s.mob()), SPAWN);
        s
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn held(&self) -> Option<Entity> {
        self.get::<Inventory>(self.mob()).active_item()
    }

    /// A new `path` item straight into the active hand, the old one gone.
    fn give_new(&mut self, path: &str) -> Entity {
        let mob = self.mob();
        let old = self.held();
        let world = self.server.world_mut();
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

    /// A click on the active hand's slot (tg `attack_self`) once off the click cooldown.
    fn use_in_hand(&mut self) {
        let mob = self.mob();
        self.run_until("off the click cooldown", |s| {
            s.now() >= s.get::<NextMove>(mob).0
        });
        let slot = if self.get::<Inventory>(mob).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        };
        self.send(0, ClientIntent::ClickSlot(slot));
    }

    fn prompt(&mut self) -> Option<Entity> {
        let mob = self.mob();
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &Prompt)>();
        q.iter(world).find(|(_, p)| p.actor == mob).map(|(e, _)| e)
    }

    /// Waits for the question and answers it with `text`.
    fn answer_text(&mut self, text: &str) {
        self.run_until("a question", |s| s.prompt().is_some());
        let prompt = self.prompt().unwrap();
        let (client, actor) = (self.players[0].link, self.mob());
        self.server.world_mut().write_message(PromptAnswered {
            prompt,
            client,
            actor,
            answer: Some(PromptAnswer::Text(text.to_owned())),
        });
        self.tick();
        if let Ok(e) = self.server.world_mut().get_entity_mut(prompt) {
            e.despawn();
        }
    }

    fn said(&self, text: &str) -> bool {
        self.players[0]
            .app
            .world()
            .resource::<Received>()
            .0
            .iter()
            .any(|l| l.contains(text))
    }

    fn station_name(&self) -> String {
        self.server.world().resource::<StationName>().0.clone()
    }
}

#[test]
fn charter_renames_the_station_once() {
    let mut s = Station::start();
    let charter = s.give_new("/obj/item/station_charter");

    // A name outside the standard format waits for the admins; one rejects it.
    s.use_in_hand();
    s.answer_text("Butt Land");
    s.run_until("sent for approval", |s| s.said("отправлено работодателям"));
    assert!(s.get::<Charter>(charter).pending.is_some());
    s.use_in_hand();
    s.run_until("still waiting", |s| s.said("всё ещё ждёте ответа"));
    assert!(reject_proposed(s.server.world_mut(), charter));
    assert!(s.get::<Charter>(charter).pending.is_none());
    assert!(!reject_proposed(s.server.world_mut(), charter));
    assert_eq!(s.station_name(), "");

    // A standard name is approved at once and uses the charter up.
    s.use_in_hand();
    s.answer_text("Imperial Aegis Station 13");
    s.run_until("renamed", |s| {
        s.station_name() == "Imperial Aegis Station 13"
    });
    assert!(s.said("одобрено автоматически"));
    assert!(s.get::<Charter>(charter).used);
    assert_eq!(
        s.get::<Name>(charter).as_str(),
        "station charter for Imperial Aegis Station 13"
    );
    s.use_in_hand();
    s.run_until("used up", |s| s.said("уже назвали станцию"));

    // Nobody rejects another charter's name: approved after the wait.
    s.give_new("/obj/item/station_charter");
    s.use_in_hand();
    s.answer_text("Butt Land");
    s.hold(50.0, "waiting for the admins", |s| {
        s.station_name() == "Imperial Aegis Station 13"
    });
    s.run_until_for(15.0, "approved by itself", |s| {
        s.station_name() == "Butt Land"
    });
}
