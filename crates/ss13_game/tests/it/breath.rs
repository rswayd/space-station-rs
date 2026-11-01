//! Breathing over real WebTransport on 127.0.0.1: two players spawn in space. A breathes
//! vacuum: suffocation oxyloss, low pressure brute damage and the alerts on A's client. B picks
//! up a breath mask and an emergency oxygen tank, wears the mask (HUD slot click), alt+clicks
//! the tank in hand: internals open and B takes no more oxyloss, only pressure damage.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::AtmosPlugin;
use ss13_game::breath::{AlertCategory, Alerts};
use ss13_game::health::{Health, Stat};
use ss13_game::hud::{AlertHudData, AlertHudUi};
use ss13_game::internals::{Internals, TankAir};
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "w_class": 3,
        "body_parts_covered": 0, "heat_protection": 0, "cold_protection": 0,
        "min_cold_protection_temperature": 0, "max_heat_protection_temperature": 0,
        "allowed": null } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item", "declared_vars": { "clothing_flags": 0 } },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 2, "body_parts_covered": 411 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/clothing/mask": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 32, "body_parts_covered": 4 } },
    "/obj/item/clothing/mask/breath": { "parent": "/obj/item/clothing/mask", "declared_vars": {
        "name": "breath mask", "icon_state": "breath", "body_parts_covered": 0,
        "clothing_flags": 8, "w_class": 2 } },
    "/obj/item/tank": { "parent": "/obj/item", "declared_vars": { "slot_flags": 1024,
        "volume": 70, "distribute_pressure": 101.325 } },
    "/obj/item/tank/internals": { "parent": "/obj/item/tank" },
    "/obj/item/tank/internals/emergency_oxygen": { "parent": "/obj/item/tank/internals",
        "declared_vars": { "name": "emergency oxygen tank", "icon_state": "emergency",
        "slot_flags": 512, "volume": 3, "distribute_pressure": 16, "w_class": 2 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15" } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "icon_state": "space",
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1: space inside walls, the two spawns `A` (3,3) and
/// (3,4), a breath mask `M` (4,3) and an emergency oxygen tank `T` (4,4) next to both.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"s" = (/turf/open/space/basic,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/turf/open/space/basic,/area/space)
"M" = (/obj/item/clothing/mask/breath,/turf/open/space/basic,/area/space)
"T" = (/obj/item/tank/internals/emergency_oxygen,/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwwwww
wsssssw
wsATssw
wsAMssw
wsssssw
wwwwwww
"}
"##;

/// Items without a sprite in the manifest are not drawn, so not replicated.
const STATES: [(&str, &str); 6] = [
    ("t.dmi", "space"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "breath"),
    ("i.dmi", "emergency"),
];

const MASK: TilePos = TilePos::new(4, 3, 1);
const TANK: TilePos = TilePos::new(4, 4, 1);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("breath_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/clothing/mask/breath",
        "/obj/item/tank/internals/emergency_oxygen",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// What a client received: chat lines and its last alert HUD data.
#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    alerts: Option<AlertHudData>,
    window: Option<u32>,
}

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReader<UiMessageReceived>,
    mut received: ResMut<Received>,
) {
    for ChatReceived(message) in chat.read() {
        received.chat.push(message.plain_text());
    }
    for UiMessageReceived(message) in ui.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == AlertHudUi::NAME => {
                received.window = Some(*window);
                data
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if received.window == Some(*window) => data,
            _ => continue,
        };
        if !data.is_empty() {
            received.alerts = Some(ss13_tgui::decode(data).unwrap());
        }
    }
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    /// Every frame is one tick of game time on every side: Life() fires every 40 frames however
    /// slow the machine is.
    fn start(test: &str, players: usize) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(AtmosPlugin);
        });
        s.join_all(players, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    fn mob(&self, p: usize) -> Entity {
        self.players[p].mob
    }

    fn health(&self, p: usize) -> &Health {
        self.get::<Health>(self.mob(p))
    }

    fn inventory(&self, p: usize) -> &Inventory {
        self.get::<Inventory>(self.mob(p))
    }

    fn item_at(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<ss13_mapload::DmItem>(e).is_some())
            .unwrap()
    }

    /// Clicks `entity` once it is replicated to `p`.
    fn click(&mut self, p: usize, entity: Entity) {
        self.run_until("replicated to the clicker", |s| {
            s.net_id(p, entity).is_some()
        });
        self.0.click(p, entity);
    }

    fn said(&self, p: usize, text: &str) -> bool {
        self.players[p]
            .app
            .world()
            .resource::<Received>()
            .chat
            .iter()
            .any(|l| l.contains(text))
    }

    fn alert_names(&self, p: usize) -> Vec<String> {
        self.players[p]
            .app
            .world()
            .resource::<Received>()
            .alerts
            .as_ref()
            .map(|a| a.alerts.iter().map(|a| a.name.clone()).collect())
            .unwrap_or_default()
    }
}

#[test]
fn vacuum_suffocates_and_internals_save_the_breath() {
    let mut s = Station::start("vacuum_suffocates_and_internals_save_the_breath", 2);
    let (a, b) = (0, 1);
    let (mask, tank) = (s.item_at(MASK), s.item_at(TANK));
    assert!(s.server.world().get::<TankAir>(tank).is_some());

    // B: the mask from the floor into the empty active hand, then onto the face.
    s.click(b, mask);
    s.run_until("B holds the mask", |s| {
        s.inventory(b).active_item() == Some(mask)
    });
    s.send(b, ClientIntent::ClickSlot(InventorySlot::Mask));
    s.run_until("B wears the mask", |s| s.inventory(b).mask == Some(mask));
    // The tank into the hand, alt+click on it: internals open.
    s.click(b, tank);
    s.run_until("B holds the tank", |s| {
        s.inventory(b).active_item() == Some(tank)
    });
    let hand = if s.inventory(b).active == 0 {
        InventorySlot::LeftHand
    } else {
        InventorySlot::RightHand
    };
    s.send(b, ClientIntent::AltClickSlot(hand));
    s.run_until("B's internals open", |s| {
        s.get::<Internals>(s.mob(b)).0 == Some(tank)
    });
    s.run_for(2.0);
    assert!(
        s.said(b, "Вы открываете клапан на emergency oxygen"),
        "{:?}",
        s.players[b].app.world().resource::<Received>().chat
    );
    let tank_moles = s.get::<TankAir>(tank).air.total_moles();

    let (a_oxy, a_brute) = (s.health(a).oxy, s.health(a).brute);
    let mut b_oxy = s.health(b).oxy;
    let b_brute = s.health(b).brute;
    // 20 s of game time: 10 Life() fires.
    let start = s.now();
    while s.now() - start < 20.0 {
        s.step();
        let oxy = s.health(b).oxy;
        assert!(
            oxy <= b_oxy,
            "B on internals took oxyloss: {b_oxy} -> {oxy}"
        );
        b_oxy = oxy;
    }
    // A: a failed breath every fire (3 oxy each) and 4 brute per fire from the vacuum.
    let a_health = s.health(a).clone();
    assert!(
        a_health.oxy - a_oxy >= 3.0 * 8.0,
        "A's oxyloss {a_oxy} -> {}",
        a_health.oxy
    );
    assert!(
        a_health.brute - a_brute >= 4.0 * 9.0,
        "A's brute {a_brute} -> {}",
        a_health.brute
    );
    let alerts = s.get::<Alerts>(s.mob(a)).clone();
    assert!(alerts.get(AlertCategory::NotEnoughOxygen).is_some());
    assert_eq!(
        alerts.get(AlertCategory::Pressure).map(|a| a.icon_state()),
        Some("lowpressure2".to_owned())
    );
    s.run_until("A's client shows the alerts", |s| {
        let names = s.alert_names(a);
        names.iter().any(|n| n == "Удушье (Нехватка O2)")
            && names.iter().any(|n| n == "Низкое давление")
    });
    // B: the vacuum still crushes, but it breathes the tank.
    assert!(s.health(b).brute - b_brute >= 4.0 * 9.0);
    assert!(
        s.get::<Alerts>(s.mob(b))
            .get(AlertCategory::NotEnoughOxygen)
            .is_none()
    );
    assert!(s.get::<TankAir>(tank).air.total_moles() < tank_moles);
    assert_eq!(*s.get::<Stat>(s.mob(b)), Stat::Conscious);
}
