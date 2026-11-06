//! Misc UI over a loopback server: A's screentip over B names B with the carbon contexts (hug or
//! attack, shove, pull); B leaves and its body gets the SSD `zzz_glow` and falls asleep; Esc's
//! menu opens the Who window with both keys.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::CombatMode;
use ss13_game::escape_menu::{
    EscapeButtonUi, EscapeMenuData, EscapeMenuUi, OPEN, WHO, WhoData, WhoUi,
};
use ss13_game::screentips::Screentip;
use ss13_game::ssd::{SSD_DMI, SSD_STATE, Ssd};
use ss13_game::status_effects::{Incap, StatusEffects};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::IconIndex;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::{UiInterface, decode};

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

/// A and B are moved to (2,3) and (3,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfffw
wsfsw
wfffw
wwwww
"}
"##;

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
    (SSD_DMI, SSD_STATE),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("misc_ui_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// The windows opened for a client: id, interface, data.
#[derive(Resource, Default)]
struct Opened(Vec<(u32, String, Vec<u8>)>);

fn record(mut ui: MessageReader<UiMessageReceived>, mut opened: ResMut<Opened>) {
    for UiMessageReceived(message) in ui.read() {
        if let UiServerMessage::Open {
            window,
            interface,
            data,
            ..
        } = message
        {
            opened.0.push((*window, interface.clone(), data.clone()));
        }
    }
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(2, |app| {
            app.init_resource::<Opened>().add_systems(Update, record);
        });
        let (a, b) = (s.players[0].mob, s.players[1].mob);
        let world = s.server.world_mut();
        world.entity_mut(a).insert(TilePos::new(2, 3, 1));
        world.entity_mut(b).insert(TilePos::new(3, 3, 1));
        Self(s)
    }

    /// The last window of `interface` opened for player `p`, with its first data.
    fn window(&self, p: usize, interface: &str) -> Option<(u32, Vec<u8>)> {
        self.players[p]
            .app
            .world()
            .resource::<Opened>()
            .0
            .iter()
            .rev()
            .find(|(_, i, _)| i == interface)
            .map(|(w, _, d)| (*w, d.clone()))
    }

    fn tip(&self) -> (String, Vec<String>) {
        let tip = &self.get::<Screentip>(self.players[0].mob).0;
        (tip.name.clone(), tip.lines.clone())
    }
}

#[test]
fn screentip_names_the_hovered_human_with_its_contexts() {
    let mut s = Station::start("screentip_names_the_hovered_human_with_its_contexts");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    s.run_until("B is replicated to A", |s| s.net_id(0, b).is_some());
    let id = s.net_id(0, b).unwrap();
    s.send(0, ClientIntent::Hover(Some(id)));
    s.run_until("A's tip names B", |s| !s.tip().0.is_empty());
    let (name, lines) = s.tip();
    assert_eq!(name, s.get::<Name>(b).as_str());
    assert_eq!(
        lines,
        ["LMB: Обнять | RMB: Толкнуть", "Ctrl-LMB: Потащить"],
        "{lines:?}"
    );

    // Combat mode: the left button attacks.
    s.server.world_mut().entity_mut(a).insert(CombatMode(true));
    s.send(0, ClientIntent::Hover(None));
    s.run_until("the tip clears", |s| s.tip().0.is_empty());
    s.send(0, ClientIntent::Hover(Some(id)));
    s.run_until("A's tip attacks", |s| {
        s.tip()
            .1
            .first()
            .is_some_and(|l| l == "LMB: Атаковать | RMB: Толкнуть")
    });
    assert!(
        s.window(0, ss13_game::screentips::ScreentipUi::NAME)
            .is_some()
    );
}

#[test]
fn a_left_body_shows_zzz_and_sleeps() {
    let mut s = Station::start("a_left_body_shows_zzz_and_sleeps");
    let b = s.players[1].mob;
    let zzz = s
        .server
        .world()
        .resource::<IconIndex>()
        .get(SSD_DMI, SSD_STATE)
        .unwrap();
    let drawn = |s: &Station| {
        s.get::<Appearance>(b)
            .overlays
            .iter()
            .any(|o| o.icon_state == zzz)
    };
    assert!(!drawn(&s));
    drop(s.players.remove(1));
    s.run_until("B's body is SSD with zzz", |s| {
        s.try_get::<Ssd>(b).is_some() && drawn(s)
    });
    s.run_until("B's body falls asleep", |s| {
        s.try_get::<StatusEffects>(b)
            .is_some_and(|e| e.remaining(Incap::Sleeping) > 0.0)
    });
}

#[test]
fn escape_opens_the_menu_and_who() {
    let mut s = Station::start("escape_opens_the_menu_and_who");
    s.run_until("A has the menu host", |s| {
        s.window(0, EscapeButtonUi::NAME).is_some()
    });
    let (host, _) = s.window(0, EscapeButtonUi::NAME).unwrap();
    s.act(0, host, OPEN, &[]);
    s.run_until("the menu opens", |s| {
        s.window(0, EscapeMenuUi::NAME).is_some()
    });
    let (menu, data) = s.window(0, EscapeMenuUi::NAME).unwrap();
    assert_eq!(decode::<EscapeMenuData>(&data).unwrap().station_name, "map");
    s.run_for(1.5);
    s.act(0, menu, WHO, &[]);
    s.run_until("Who opens", |s| s.window(0, WhoUi::NAME).is_some());
    let (_, data) = s.window(0, WhoUi::NAME).unwrap();
    let who = decode::<WhoData>(&data).unwrap();
    assert_eq!(who.user, "p0");
    assert_eq!(who.clients, ["p0", "p1"]);
}
