//! Inventory and ID access over real WebTransport on 127.0.0.1: two headless clients on a
//! fixture station pick up, drop, swap hands, equip IDs and try doors that need access.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::doors::{Door, DoorState};
use ss13_game::hud::{HudData, InventoryHudUi};
use ss13_game::inventory::{ASSISTANT_ACCESS, CarriedBy, IdCard, Inventory};
use ss13_game::server::MoveQueue;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmAtom, DmItem, Turf};
use ss13_net::client::NetEntity;
use ss13_net::server::Viewer;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot, NetId};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Appearance, Dir, IconStateId, TilePos};
use ss13_tgui::UiInterface;
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
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/toy": { "parent": "/obj/item" },
    "/obj/item/toy/bolted": { "parent": "/obj/item/toy", "declared_vars": { "anchored": 1 } },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/card/id/brig": { "parent": "/obj/item/card/id", "declared_vars": {
        "access": { "list": ["brig"] } } },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/rack": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "rack" } },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "autoclose": 1,
        "base_icon_state": null, "normalspeed": 1, "overlays_file": { "resource": "o.dmi" },
        "airlock_material": null } },
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

/// Rows top-down, the last line is y = 1. Spawns `s` (3,3) then (3,4). `T` toy (4,3), `K` an
/// anchored toy (2,3), `R` a rack (2,2), `C` a brig ID card (4,2), `F` a toy out of reach
/// (10,2). `D` (6,3) needs brig access, `P` (6,4) is public.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"T" = (/obj/item/toy,/turf/open/floor,/area/space)
"F" = (/obj/item/toy,/turf/open/floor,/area/space)
"K" = (/obj/item/toy/bolted,/turf/open/floor,/area/space)
"R" = (/obj/structure/rack,/turf/open/floor,/area/space)
"C" = (/obj/item/card/id/brig,/turf/open/floor,/area/space)
"D" = (/obj/machinery/door/airlock{req_access = list("brig")},/turf/open/floor,/area/space)
"P" = (/obj/machinery/door/airlock,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwww
wffffffffffw
wfsffPfffffw
wKsTfDfffffw
wRfCfffffFfw
wwwwwwwwwwww
"}
"##;

const SPAWN_A: TilePos = TilePos::new(3, 3, 1);
const SPAWN_B: TilePos = TilePos::new(3, 4, 1);
const TOY: TilePos = TilePos::new(4, 3, 1);
const FAR_TOY: TilePos = TilePos::new(10, 2, 1);
const BOLTED: TilePos = TilePos::new(2, 3, 1);
const RACK: TilePos = TilePos::new(2, 2, 1);
const CARD: TilePos = TilePos::new(4, 2, 1);
const D: TilePos = TilePos::new(6, 3, 1);
const P: TilePos = TilePos::new(6, 4, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 15] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "card_grey"),
    ("i.dmi", "rack"),
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("a.dmi", "closing"),
    ("o.dmi", "lights_denied"),
    ("o.dmi", "lights_opening"),
    ("o.dmi", "lights_closing"),
    ("l.dmi", "card-id"),
];
const TOY_ICON: IconStateId = IconStateId(4);
const CARD_ICON: IconStateId = IconStateId(5);
const DENIED_LIGHTS: IconStateId = IconStateId(11);

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("inventory_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/toy",
        "/obj/item/toy/bolted",
        "/obj/item/card/id/brig",
        "/obj/structure/rack",
        "/obj/machinery/door/airlock",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// The last HUD data the client received (what `inventory_client` draws).
#[derive(Resource, Default)]
struct LastHud(Option<HudData>);

fn record_hud(
    mut received: MessageReader<UiMessageReceived>,
    mut last: ResMut<LastHud>,
    mut hud_window: Local<Option<u32>>,
) {
    for UiMessageReceived(message) in received.read() {
        // Other windowless HUDs (mood) update too: only the inventory HUD's window counts.
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == InventoryHudUi::NAME => {
                *hud_window = Some(*window);
                data
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if *hud_window == Some(*window) => data,
            _ => continue,
        };
        last.0 = Some(ss13_tgui::decode(data).unwrap());
    }
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(clients: usize) -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(clients, |app| {
            app.init_resource::<LastHud>()
                .add_systems(Update, record_hud);
        });
        Self(s)
    }

    fn pos(&self, p: usize) -> TilePos {
        *self
            .server
            .world()
            .get::<TilePos>(self.players[p].mob)
            .unwrap()
    }

    fn walk(&mut self, p: usize, dir: Dir) {
        let to = self.pos(p).step(dir);
        self.run_until(&format!("walk {dir:?} to {to:?}"), |s| {
            if s.pos(p) == to {
                return true;
            }
            let applied = s
                .server
                .world()
                .get::<MoveQueue>(s.players[p].mob)
                .unwrap()
                .applied();
            if s.players[p].seq == applied {
                s.players[p].seq += 1;
                let seq = s.players[p].seq;
                s.send(p, ClientIntent::Move { dir, seq });
            }
            false
        });
    }

    fn inventory(&self, p: usize) -> Inventory {
        self.server
            .world()
            .get::<Inventory>(self.players[p].mob)
            .unwrap()
            .clone()
    }

    /// The first entity on `pos` matching `filter`.
    fn on(&self, pos: TilePos, filter: impl Fn(EntityRef) -> bool) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| filter(world.entity(e)))
            .unwrap_or_else(|| panic!("nothing matching on {pos:?}"))
    }

    fn item(&self, pos: TilePos) -> Entity {
        self.on(pos, |e| e.contains::<DmItem>())
    }

    fn door(&self, pos: TilePos) -> Entity {
        self.on(pos, |e| e.contains::<Door>())
    }

    fn sees(&mut self, p: usize, entity: Entity) -> bool {
        let Some(id) = self.net_id(p, entity) else {
            return false;
        };
        let world = self.players[p].app.world_mut();
        let mut q = world.query::<&NetEntity>();
        q.iter(world).any(|n| n.0 == id)
    }

    fn click_id(&mut self, p: usize, target: NetId) {
        self.send(
            p,
            ClientIntent::Click {
                target,
                modifiers: ClickModifiers::default(),
            },
        );
    }

    fn on_floor(&self, item: Entity) -> Option<TilePos> {
        self.server.world().get::<TilePos>(item).copied()
    }

    fn state(&self, door: Entity) -> DoorState {
        self.server.world().get::<Door>(door).unwrap().state
    }

    fn denied(&self, door: Entity) -> bool {
        self.server.world().get::<Door>(door).unwrap().denied()
    }

    fn hud(&self, p: usize) -> Option<HudData> {
        self.players[p].app.world().resource::<LastHud>().0.clone()
    }
}

#[test]
fn pickup_drop_swap_equip_and_access() {
    let mut s = Station::start(2);
    assert_eq!((s.pos(0), s.pos(1)), (SPAWN_A, SPAWN_B));
    let a_mob = s.players[0].mob;

    // Spawned with the assistant kit: an ID with assistant access, uniform, shoes, empty hands.
    let inv = s.inventory(0);
    assert_eq!((inv.hands, inv.active), ([None, None], 0));
    let assistant_id = inv.id.expect("an ID card in the ID slot");
    let card = s.server.world().get::<IdCard>(assistant_id).unwrap();
    assert_eq!(card.access, ASSISTANT_ACCESS);
    for item in [assistant_id, inv.uniform.unwrap(), inv.shoes.unwrap()] {
        assert!(s.on_floor(item).is_none(), "worn items are off the map");
        assert_eq!(s.server.world().get::<CarriedBy>(item).unwrap().0, a_mob);
    }
    s.run_until("the HUD reaches the client", |s| {
        s.hud(0).is_some_and(|h| {
            h.id.is_some_and(|i| i.icon_state == CARD_ICON) && h.hands == [None, None]
        })
    });

    let toy = s.item(TOY);
    let far_toy = s.item(FAR_TOY);
    let bolted = s.item(BOLTED);
    let rack = s.on(RACK, |e| e.contains::<DmAtom>() && !e.contains::<Turf>());
    let brig_card = s.item(CARD);
    let b_toy_id = s.net_id(1, toy).expect("B sees the toy");

    // Rejected: out of reach, an anchored item, not an item.
    for target in [far_toy, bolted, rack] {
        s.click(0, target);
        s.hold(0.3, "nothing is picked up", |s| {
            s.inventory(0).hands == [None, None]
        });
    }
    assert_eq!(s.on_floor(far_toy), Some(FAR_TOY));
    assert_eq!(s.on_floor(bolted), Some(BOLTED));

    // Picked up: off the map for everyone, in the active (left) hand, on A's HUD.
    s.click(0, toy);
    s.run_until("A holds the toy", |s| s.inventory(0).hands[0] == Some(toy));
    assert!(s.on_floor(toy).is_none());
    assert_eq!(s.server.world().get::<CarriedBy>(toy).unwrap().0, a_mob);
    s.run_until("B no longer sees the toy", |s| !s.sees(1, toy));
    s.run_until("A's HUD shows it", |s| {
        s.hud(0)
            .is_some_and(|h| h.hands[0].is_some_and(|i| i.icon_state == TOY_ICON))
    });

    // Held by someone else: B's stale click does nothing.
    s.click_id(1, b_toy_id);
    s.hold(0.3, "B cannot take A's toy", |s| {
        s.inventory(1).hands == [None, None] && s.inventory(0).hands[0] == Some(toy)
    });

    // Unseen: the card next to A, with A's view moved away (edge hysteresis keeps nearby
    // entities, so move it to a level that does not exist).
    let stale = s.net_id(0, brig_card).unwrap();
    let link = s.players[0].link;
    let home = s.server.world().get::<Viewer>(link).unwrap().center;
    s.server
        .world_mut()
        .get_mut::<Viewer>(link)
        .unwrap()
        .center
        .z = u16::MAX;
    s.run_until("A loses the card", |s| !s.sees(0, brig_card));
    s.send(0, ClientIntent::SwapHands);
    s.run_until("swapped to the right hand", |s| s.inventory(0).active == 1);
    s.click_id(0, stale);
    s.hold(0.3, "an unseen card is not picked up", |s| {
        s.on_floor(brig_card) == Some(CARD)
    });
    s.server.world_mut().get_mut::<Viewer>(link).unwrap().center = home;
    s.run_until("A sees the card again", |s| s.sees(0, brig_card));

    // Back to the left hand with its HUD slot; Q drops the toy at A's feet, B sees it again.
    s.send(0, ClientIntent::ClickSlot(InventorySlot::LeftHand));
    s.run_until("left hand active", |s| s.inventory(0).active == 0);
    s.run_until("the HUD marks it", |s| {
        s.hud(0).is_some_and(|h| h.active == 0)
    });
    s.send(0, ClientIntent::DropActive);
    s.run_until("the toy is dropped", |s| s.on_floor(toy) == Some(SPAWN_A));
    assert_eq!(s.inventory(0).hands, [None, None]);
    assert!(s.server.world().get::<CarriedBy>(toy).is_none());
    s.run_until("B sees the dropped toy", |s| s.sees(1, toy));

    // A click with an item in hand puts it on a free tile next to the mob.
    s.click(0, toy);
    s.run_until("picked up again", |s| s.inventory(0).hands[0] == Some(toy));
    let corner = TilePos::new(2, 4, 1);
    let floor = s.on(corner, |e| e.contains::<Turf>());
    s.click(0, floor);
    s.run_until("put down on (2,4)", |s| s.on_floor(toy) == Some(corner));

    // Doors, both next to (5,3). The assistant lacks brig access: denied, stays closed.
    s.walk(0, Dir::East);
    s.walk(0, Dir::East);
    assert_eq!(s.pos(0), TilePos::new(5, 3, 1));
    let (d, p) = (s.door(D), s.door(P));
    s.click(0, d);
    s.run_until("D denies", |s| s.denied(d));
    let overlays = &s.server.world().get::<Appearance>(d).unwrap().overlays;
    assert!(
        overlays.iter().any(|o| o.icon_state == DENIED_LIGHTS),
        "{overlays:?}"
    );
    s.hold(1.0, "D stays closed", |s| s.state(d) == DoorState::Closed);
    assert!(!s.denied(d), "the deny animation ends");
    s.click(0, p);
    s.run_until("the public door opens", |s| s.state(p) == DoorState::Open);

    // Equip the brig card: pick it up, the ID slot is taken, take the assistant ID out, drop it,
    // put the brig card in.
    s.walk(0, Dir::West);
    assert_eq!(s.pos(0), TOY);
    s.click(0, brig_card);
    s.run_until("A holds the brig card", |s| {
        s.inventory(0).hands[0] == Some(brig_card)
    });
    s.send(0, ClientIntent::ClickSlot(InventorySlot::Id));
    s.hold(0.3, "the slot is taken", |s| {
        s.inventory(0).id == Some(assistant_id)
    });
    s.send(0, ClientIntent::SwapHands);
    s.run_until("right hand", |s| s.inventory(0).active == 1);
    s.send(0, ClientIntent::ClickSlot(InventorySlot::Id));
    s.run_until("the assistant ID is in hand", |s| {
        let inv = s.inventory(0);
        inv.hands[1] == Some(assistant_id) && inv.id.is_none()
    });
    s.send(0, ClientIntent::DropActive);
    s.run_until("dropped", |s| s.on_floor(assistant_id).is_some());
    s.send(0, ClientIntent::SwapHands);
    s.run_until("left hand", |s| s.inventory(0).active == 0);
    s.send(0, ClientIntent::ClickSlot(InventorySlot::Id));
    s.run_until("the brig card is worn", |s| {
        s.inventory(0).id == Some(brig_card)
    });
    assert_eq!(s.inventory(0).hands, [None, None]);
    s.walk(0, Dir::East);
    s.click(0, d);
    s.run_until("D opens for the brig card", |s| {
        s.state(d) == DoorState::Open
    });
}
