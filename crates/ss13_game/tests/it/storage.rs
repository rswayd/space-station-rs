//! Closets and storage over real WebTransport on 127.0.0.1: a headless client on a fixture
//! station opens a locker, picks up what `PopulateContents` put in it, puts it into the backpack
//! it spawned with, opens the backpack's storage view and takes the item out again.
//!
//! `metastation_closet_contents` (ignored, needs generated assets) reports how many closet and
//! storage types on MetaStation resolve their contents.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::closets::Closet;
use ss13_game::hud::{StorageData, StorageUi};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::storage::{Storage, Stored, StoredIn};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, MapLoadPlugin};
use ss13_net::server::Interest;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{IconStateId, TilePos};
use ss13_tgui::UiInterface;
use ss13_world::{Density, SpatialIndex};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14 } },
    "/datum/storage/backpack": { "parent": "/datum/storage", "declared_vars": {
        "max_slots": 21, "max_total_storage": 21 } },
    "/datum/storage/box": { "parent": "/datum/storage", "declared_vars": {
        "max_specific_storage": 2 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "req_access": null } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": { "w_class": 1 } },
    "/obj/item/bulky": { "parent": "/obj/item", "declared_vars": { "w_class": 4 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/card/id/advanced/mining": { "parent": "/obj/item/card/id/advanced",
        "declared_vars": { "access": { "list": ["mining"] } } },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/backpack": { "parent": "/obj/item/storage", "declared_vars": {
        "icon_state": "backpack", "slot_flags": 1, "w_class": 4,
        "storage_type": { "path": "/datum/storage/backpack" } } },
    "/obj/item/storage/box": { "parent": "/obj/item/storage", "declared_vars": {
        "icon_state": "box", "w_class": 2, "storage_type": { "path": "/datum/storage/box" } } },
    "/obj/item/storage/box/survival": { "parent": "/obj/item/storage/box" },
    "/obj/item/storage/box/ids": { "parent": "/obj/item/storage/box" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "generic", "density": 1,
        "opened": 0, "locked": 0, "welded": 0, "secure": 0, "icon_door": null,
        "icon_welded": "welded", "enable_door_overlay": 1, "has_opened_overlay": 1,
        "has_closed_overlay": 1, "dense_when_open": 0, "wall_mounted": 0, "horizontal": 0,
        "storage_capacity": 30 } },
    "/obj/structure/closet/secure_closet": { "parent": "/obj/structure/closet", "declared_vars": {
        "locked": 1, "secure": 1, "req_access": { "list": ["captain"] } } },
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
  },
  "populate": {
    "/obj/structure/closet": { "PopulateContents": ["return"] },
    "/obj/structure/closet/secure_closet": { "PopulateContents": [
        "parent", { "new": "/obj/item/toy" }, { "new": "/obj/item/bulky" },
        { "new": "/obj/item/card/id/advanced/mining" }] },
    "/obj/item/storage/box/ids": { "PopulateContents": [
        { "repeat": { "count": 2, "body": [{ "new": "/obj/item/card/id/advanced" }] } }] },
    "/obj/item/storage/box/survival": { "PopulateContents": [
        { "repeat": { "count": 2, "body": [{ "new": "/obj/item/toy" }] } }] }
  }
}"##;

/// Spawn `s` (3,3). `L` (4,3): a captain's locker, unlocked by a map edit, holding the toy, the
/// bulky item and a mining ID from `PopulateContents`. `C` (3,2): a plain closet mapped closed
/// over a toy. `B` (5,2): a plain closet over a box of IDs; `I` (2,4): a box of IDs on the floor.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"L" = (/obj/structure/closet/secure_closet{locked = 0},/turf/open/floor,/area/space)
"C" = (/obj/structure/closet,/obj/item/toy,/turf/open/floor,/area/space)
"B" = (/obj/structure/closet,/obj/item/storage/box/ids,/turf/open/floor,/area/space)
"I" = (/obj/item/storage/box/ids,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wIfffw
wfsLfw
wfCfBw
wwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const LOCKER: TilePos = TilePos::new(4, 3, 1);
const CLOSET: TilePos = TilePos::new(3, 2, 1);
const IDS_CLOSET: TilePos = TilePos::new(5, 2, 1);
const IDS_BOX: TilePos = TilePos::new(2, 4, 1);

const STATES: [(&str, &str); 14] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "card_grey"),
    ("i.dmi", "backpack"),
    ("i.dmi", "box"),
    ("c.dmi", "generic"),
    ("c.dmi", "generic_open"),
    ("c.dmi", "generic_door"),
    ("c.dmi", "locked"),
    ("c.dmi", "unlocked"),
    ("c.dmi", "secure"),
];
const TOY_ICON: IconStateId = IconStateId(4);
const BOX_ICON: IconStateId = IconStateId(7);
const DOOR_OPEN: IconStateId = IconStateId(9);
const DOOR_CLOSED: IconStateId = IconStateId(10);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("storage_fixture")
        .join(test);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/closet",
        "/obj/structure/closet/secure_closet",
        "/obj/item/toy",
        "/obj/item/storage/box/ids",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    used.extend(ss13_mapload::RUNTIME_CONTAINER_TYPES);
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// The storage window as the client sees it (what `storage_client` draws).
#[derive(Resource, Default)]
struct LastStorage {
    window: Option<u32>,
    data: Option<StorageData>,
}

fn record_storage(mut received: MessageReader<UiMessageReceived>, mut last: ResMut<LastStorage>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == StorageUi::NAME => {
                last.window = Some(*window);
                last.data = Some(ss13_tgui::decode(data).unwrap());
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if last.window == Some(*window) => {
                last.data = Some(ss13_tgui::decode(data).unwrap());
            }
            UiServerMessage::Close { window } if last.window == Some(*window) => {
                *last = LastStorage::default();
            }
            _ => {}
        }
    }
}

/// One client (`p0`): its connection and mob.
struct Station {
    st: testing::Station,
    link: Entity,
    mob: Entity,
}

ss13_game::drive_station!(Station, st);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        Self::start_on(fixture_map(&assets, &["map.dmm"]))
    }

    fn start_on(map: MapLoadPlugin) -> Self {
        let mut st = testing::Station::start(map, |_| {});
        st.join("p0", |app| {
            app.init_resource::<LastStorage>()
                .add_systems(Update, record_storage);
        });
        let (link, mob) = (st.players[0].link, st.players[0].mob);
        Self { st, link, mob }
    }

    fn client(&self) -> &App {
        &self.players[0].app
    }

    fn send(&mut self, intent: ClientIntent) {
        self.st.send(0, intent);
    }

    fn click(&mut self, entity: Entity) {
        self.st.click(0, entity);
    }

    fn inventory(&self) -> Inventory {
        self.server
            .world()
            .get::<Inventory>(self.mob)
            .unwrap()
            .clone()
    }

    fn on(&self, pos: TilePos, filter: impl Fn(EntityRef) -> bool) -> Option<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| filter(world.entity(e)))
    }

    fn stored(&self, container: Entity) -> Vec<Entity> {
        self.server
            .world()
            .get::<Stored>(container)
            .map(|s| s.items().to_vec())
            .unwrap_or_default()
    }

    fn storage_view(&self) -> Option<StorageData> {
        self.client().world().resource::<LastStorage>().data.clone()
    }
}

#[test]
fn locker_to_backpack_and_back() {
    let mut s = Station::start("locker_to_backpack_and_back");
    let world = s.server.world();
    assert_eq!(world.get::<TilePos>(s.mob), Some(&SPAWN));
    let locker = s.on(LOCKER, |e| e.contains::<Closet>()).unwrap();
    let closet = s.on(CLOSET, |e| e.contains::<Closet>()).unwrap();

    // The plain closet took the toy mapped on its tile at startup; the locker is closed, its
    // `PopulateContents` has not run yet (tg fills on the first opening).
    assert_eq!(s.stored(closet).len(), 1);
    assert!(s.on(CLOSET, |e| e.contains::<DmItem>()).is_none());
    assert!(s.stored(locker).is_empty());
    let look = world.get::<ss13_shared::Appearance>(locker).unwrap();
    assert!(look.overlays.iter().any(|o| o.icon_state == DOOR_CLOSED));
    assert!(world.entity(locker).contains::<Density>());

    // Spawned with a backpack holding the survival box and its two toys.
    let inv = s.inventory();
    let backpack = inv.back.expect("a backpack on the back");
    assert_eq!(world.get::<CarriedBy>(backpack).unwrap().0, s.mob);
    let contents = s.stored(backpack);
    assert_eq!(contents.len(), 1, "the job box");
    assert_eq!(s.stored(contents[0]).len(), 2, "the box filled itself");
    assert_eq!(
        world
            .get::<Storage>(backpack)
            .map(|s| (s.max_slots, s.max_total_storage)),
        Some((21, 21))
    );

    // Open the locker: its contents spawn and land on its tile, visible to the client.
    s.click(locker);
    s.run_until("the locker opens", |s| {
        s.server.world().get::<Closet>(locker).unwrap().opened
    });
    let world = s.server.world();
    assert!(!world.entity(locker).contains::<Density>());
    let look = world.get::<ss13_shared::Appearance>(locker).unwrap();
    assert!(look.overlays.iter().any(|o| o.icon_state == DOOR_OPEN));
    let toy = s
        .on(LOCKER, |e| {
            e.get::<DmItem>().is_some_and(|i| i.path == "/obj/item/toy")
        })
        .expect("the toy from PopulateContents");
    let bulky = s
        .on(LOCKER, |e| {
            e.get::<DmItem>()
                .is_some_and(|i| i.path == "/obj/item/bulky")
        })
        .expect("the bulky item from PopulateContents");

    // Pick up the toy, click the worn backpack with it: it goes in.
    s.run_until("the toy replicates", |s| {
        s.server
            .world()
            .resource::<Interest>()
            .net_id(s.link, toy)
            .is_some()
    });
    s.click(toy);
    s.run_until("the toy is in hand", |s| {
        s.inventory().hands[0] == Some(toy)
    });
    s.send(ClientIntent::ClickSlot(InventorySlot::Back));
    s.run_until("the toy is in the backpack", |s| {
        s.server
            .world()
            .get::<StoredIn>(toy)
            .is_some_and(|i| i.0 == backpack)
    });
    assert_eq!(s.inventory().hands, [None, None]);

    // A bulky item does not fit into a backpack (w_class 4 > max_specific_storage 3).
    s.click(bulky);
    s.run_until("the bulky item is in hand", |s| {
        s.inventory().hands[0] == Some(bulky)
    });
    s.send(ClientIntent::ClickSlot(InventorySlot::Back));
    for _ in 0..20 {
        s.step();
    }
    assert_eq!(
        s.inventory().hands[0],
        Some(bulky),
        "refused, still in hand"
    );
    s.send(ClientIntent::SwapHands);
    s.run_until("the right hand is active", |s| s.inventory().active == 1);

    // Empty active hand on the backpack: its storage view opens with the box and the toy.
    s.send(ClientIntent::ClickSlot(InventorySlot::Back));
    s.run_until("the storage view shows both items", |s| {
        s.storage_view().is_some_and(|d| {
            d.items.len() == 2
                && d.items[0].icon.is_some_and(|i| i.icon_state == BOX_ICON)
                && d.items[1].icon.is_some_and(|i| i.icon_state == TOY_ICON)
        })
    });
    assert_eq!(s.storage_view().unwrap().max_slots, 21);

    // Take the toy (slot 1, named by its key) back into the empty active hand.
    let window = s.client().world().resource::<LastStorage>().window.unwrap();
    let key = s.storage_view().unwrap().items[1].key;
    s.send(ClientIntent::UiAct {
        window,
        action: "take".into(),
        params: vec![("key".into(), key.to_string())],
    });
    s.run_until("the toy is back in hand", |s| {
        s.inventory().hands[1] == Some(toy)
    });
    assert!(s.server.world().get::<StoredIn>(toy).is_none());
    s.run_until("the view shows only the box", |s| {
        s.storage_view().is_some_and(|d| d.items.len() == 1)
    });

    // Stale or hostile keys do nothing: the toy taken already, an item in another container
    // (the closet's toy), garbage.
    let foreign = s.stored(closet)[0];
    for key in [key.to_string(), foreign.to_bits().to_string(), "x".into()] {
        s.send(ClientIntent::UiAct {
            window,
            action: "take".into(),
            params: vec![("key".into(), key)],
        });
    }
    for _ in 0..10 {
        s.step();
    }
    assert_eq!(s.stored(backpack).len(), 1);
    assert_eq!(s.stored(closet), [foreign]);

    // Close the locker: it takes what lies on its tile (nothing but the locker now) again.
    s.send(ClientIntent::SwapHands);
    s.send(ClientIntent::DropActive);
    s.run_until("the bulky item is dropped", |s| {
        s.inventory().hands[0].is_none()
    });
    s.send(ClientIntent::SwapHands);
    s.run_until("the toy hand is active", |s| s.inventory().active == 1);
    s.send(ClientIntent::DropActive);
    s.run_until("hands are empty", |s| s.inventory().hands == [None, None]);
    s.click(locker);
    s.run_until("the locker closes", |s| {
        !s.server.world().get::<Closet>(locker).unwrap().opened
    });
    assert!(s.server.world().entity(locker).contains::<Density>());
}

/// Every way an item comes into existence gives it what tg's `Initialize` does: ID cards in
/// boxes mapped on the floor or inside a closed closet, and an ID a locker's `PopulateContents`
/// spawns on the first opening, all carry their access.
#[test]
fn created_items_get_access_and_storage() {
    use ss13_game::inventory::{IdCard, check_access};

    let mut s = Station::start("created_items_get_access_and_storage");
    let access = |s: &Station, card: Entity| {
        s.server
            .world()
            .get::<IdCard>(card)
            .map(|c| c.access.clone())
    };
    let floor_box = s
        .on(IDS_BOX, |e| e.contains::<Storage>())
        .expect("the box of IDs on the floor is a storage");
    let closet = s.on(IDS_CLOSET, |e| e.contains::<Closet>()).unwrap();
    let closet_box = s.stored(closet)[0];
    assert!(s.server.world().get::<Storage>(closet_box).is_some());
    for ids in [floor_box, closet_box] {
        let cards = s.stored(ids);
        assert_eq!(cards.len(), 2, "the box filled itself");
        for card in cards {
            assert_eq!(access(&s, card), Some(vec![]), "a startup card is an ID");
        }
    }

    let locker = s.on(LOCKER, |e| e.contains::<Closet>()).unwrap();
    s.click(locker);
    s.run_until("the locker opens", |s| {
        s.server.world().get::<Closet>(locker).unwrap().opened
    });
    let card = s
        .on(LOCKER, |e| {
            e.get::<DmItem>()
                .is_some_and(|i| i.path == "/obj/item/card/id/advanced/mining")
        })
        .expect("the mining ID from PopulateContents");
    let access = access(&s, card).expect("the mining ID is an ID card");
    assert!(check_access(&["mining".into()], &[], &access));
}

/// Two `take` acts in one tick each take the item they name, though the first one shifts the
/// contents; a close and a reopen of the storage arriving in one tick leave it open.
#[test]
fn storage_acts_name_items_and_survive_reopen() {
    use ss13_game::storage::StorageView;
    use ss13_net::server::IntentReceived;
    use ss13_net::ui::server::UiMessageReceived as ServerUiReceived;
    use ss13_shared::ui::UiClientMessage;

    let mut s = Station::start("storage_acts_name_items_and_survive_reopen");
    let backpack = s.inventory().back.unwrap();
    let job_box = s.stored(backpack)[0];
    let toys = s.stored(job_box);
    assert_eq!(toys.len(), 2);

    // Backpack open; take the box into the left hand.
    s.send(ClientIntent::ClickSlot(InventorySlot::Back));
    s.run_until("the backpack view shows the box", |s| {
        s.storage_view().is_some_and(|d| d.items.len() == 1)
    });
    let backpack_window = s.client().world().resource::<LastStorage>().window.unwrap();
    let key = s.storage_view().unwrap().items[0].key;
    let take = ClientIntent::UiAct {
        window: backpack_window,
        action: "take".into(),
        params: vec![("key".into(), key.to_string())],
    };
    // Paralyzed, the window stays but takes nothing (tg screen clicks need an able mob).
    let mob = s.mob;
    let paralyze = |s: &mut Station, secs: f32| {
        let world = s.st.server.world_mut();
        ss13_game::status_effects::effects_mut(world, mob)
            .unwrap()
            .paralyze = secs;
    };
    let incapacitated = |s: &Station| {
        ss13_game::status_effects::incapacitation(s.server.world(), mob).incapacitated
    };
    paralyze(&mut s, 60.0);
    s.run_until("paralyzed", |s| incapacitated(s));
    s.send(take.clone());
    s.hold(0.5, "nothing taken while paralyzed", |s| {
        s.stored(backpack) == [job_box]
    });
    paralyze(&mut s, 0.0);
    s.run_until("able again", |s| !incapacitated(s));
    s.send(take);
    s.run_until("the box is in hand", |s| {
        s.inventory().hands[0] == Some(job_box)
    });
    s.send(ClientIntent::SwapHands);
    s.run_until("the right hand is active", |s| s.inventory().active == 1);

    // The client closes the backpack's window and clicks the held box: both reach the server in
    // the same tick. The box's view is open afterwards.
    let world = s.st.server.world_mut();
    world.write_message(ServerUiReceived {
        client: s.link,
        message: UiClientMessage::Close {
            window: backpack_window,
        },
    });
    world.write_message(IntentReceived {
        client: s.link,
        intent: ClientIntent::ClickSlot(InventorySlot::LeftHand),
    });
    for _ in 0..5 {
        s.step();
    }
    assert_eq!(
        s.server
            .world()
            .get::<StorageView>(s.mob)
            .and_then(|v| v.storage),
        Some(job_box),
        "the box stays open"
    );
    s.run_until("the client shows the box's two toys", |s| {
        s.storage_view().is_some_and(|d| d.items.len() == 2)
    });

    // Both toys in one tick, by key: the first goes to the free hand, the second to the floor.
    let window = s.client().world().resource::<LastStorage>().window.unwrap();
    let keys: Vec<u64> = s
        .storage_view()
        .unwrap()
        .items
        .iter()
        .map(|i| i.key)
        .collect();
    for key in keys {
        s.send(ClientIntent::UiAct {
            window,
            action: "take".into(),
            params: vec![("key".into(), key.to_string())],
        });
    }
    s.run_until("both toys are out of the box", |s| {
        s.stored(job_box).is_empty()
    });
    let world = s.server.world();
    assert!(toys.iter().all(|&t| world.get::<StoredIn>(t).is_none()));
    assert!(s.inventory().hands[1].is_some_and(|h| toys.contains(&h)));
}

/// MetaStation: how many closet and storage types placed on the map resolve their contents from
/// `PopulateContents`, what the server saw at startup, and the HoP's and captain's lockers open
/// with their real contents (ID boxes, the captain's spare equipment).
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_closet_contents() {
    use std::collections::{BTreeMap, BTreeSet};

    use ss13_assets::dmm::DmmMap;
    use ss13_game::closets::{ClosetReport, open};
    use ss13_mapload::contents::{CLOSET_ROOT, STORAGE_ROOT, coverage};
    use ss13_mapload::convert::DmTypes;
    use ss13_mapload::fidelity::is_under;

    let map = MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    };
    let types = DmTypes::read(&map.assets.join("dm_types.json")).unwrap();
    let dmm = DmmMap::from_file(&map.map_paths()[0]).unwrap();
    let placed: BTreeSet<&str> = dmm
        .prefabs
        .iter()
        .flat_map(|p| p.objects.iter().map(|o| o.path.as_str()))
        .collect();
    for (label, root) in [("closet", CLOSET_ROOT), ("storage item", STORAGE_ROOT)] {
        let paths: Vec<&str> = placed
            .iter()
            .copied()
            .filter(|p| is_under(p, root))
            .collect();
        let cov = coverage(&types, paths.iter().copied());
        println!(
            "{label} types on MetaStation: {} -> {} fully resolved, {} partially, {} without \
             contents",
            paths.len(),
            cov.full,
            cov.partial.len(),
            cov.empty
        );
        for (path, missing) in &cov.partial {
            println!("  partial: {path} ({missing} statements not understood)");
        }
    }

    let mut s = Station::start_on(map);
    let report = s.server.world().resource::<ClosetReport>().clone();
    println!("server: {report:?}");
    assert!(report.closets > 150, "{report:?}");

    let world = s.st.server.world_mut();
    let mut q = world.query::<(Entity, &Closet, &TilePos)>();
    let lockers: Vec<(Entity, Vec<String>, TilePos)> = q
        .iter(world)
        .map(|(e, c, p)| (e, c.req_access.clone(), *p))
        .collect();
    let mut opened = BTreeMap::new();
    for access in ["hop", "captain"] {
        let (locker, _, pos) = lockers
            .iter()
            .find(|(_, a, _)| a.iter().any(|x| x == access))
            .unwrap_or_else(|| panic!("a locker with {access} access"));
        assert!(open(s.st.server.world_mut(), *locker));
        let world = s.server.world();
        let paths: Vec<String> = world
            .resource::<SpatialIndex>()
            .on_tile(*pos)
            .iter()
            .filter_map(|&e| world.get::<DmItem>(e).map(|i| i.path.clone()))
            .collect();
        println!("{access} locker at {pos:?}: {paths:?}");
        opened.insert(access, paths);
    }
    let hop = &opened["hop"];
    assert!(
        hop.iter().any(|p| p == "/obj/item/storage/box/ids"),
        "{hop:?}"
    );
    assert!(
        hop.iter().any(|p| p == "/obj/item/storage/box/silver_ids"),
        "{hop:?}"
    );
    let ids = s
        .server
        .world()
        .resource::<SpatialIndex>()
        .on_tile(
            lockers
                .iter()
                .find(|(_, a, _)| a.iter().any(|x| x == "hop"))
                .unwrap()
                .2,
        )
        .iter()
        .copied()
        .find(|&e| {
            s.server
                .world()
                .get::<DmItem>(e)
                .is_some_and(|i| i.path == "/obj/item/storage/box/ids")
        })
        .unwrap();
    let cards = s.stored(ids);
    println!("box of IDs holds {} cards", cards.len());
    assert!(!cards.is_empty());
    assert!(
        cards.iter().all(|&c| s
            .server
            .world()
            .get::<ss13_game::inventory::IdCard>(c)
            .is_some()),
        "every card in the box is an ID"
    );
    assert!(
        opened["captain"]
            .iter()
            .any(|p| p.starts_with("/obj/item/door_remote"))
    );
}

/// Clicks that reach the server in the same tick are all handled, in order (they used to collapse
/// into the newest one): the same closet clicked twice opens and closes again; a toy and a floor
/// tile clicked in one tick: the toy is picked up, then put on that tile.
#[test]
fn clicks_in_one_tick_all_apply() {
    use ss13_net::server::IntentReceived;

    let mut s = Station::start("clicks_in_one_tick_all_apply");
    let closet = s.on(CLOSET, |e| e.contains::<Closet>()).unwrap();
    let click = |s: &mut Station, entity: Entity| {
        let target = s
            .server
            .world()
            .resource::<Interest>()
            .net_id(s.link, entity)
            .expect("replicated to the clicker");
        s.st.server.world_mut().write_message(IntentReceived {
            client: s.link,
            intent: ClientIntent::Click {
                target,
                modifiers: ClickModifiers::default(),
            },
        });
    };
    click(&mut s, closet);
    click(&mut s, closet);
    let mut seen = Vec::new();
    for _ in 0..5 {
        s.step();
        seen.push(s.server.world().get::<Closet>(closet).unwrap().opened);
    }
    seen.dedup();
    assert_eq!(seen, [true, false], "opened, then closed again");

    // Open it again: the toy mapped under it is on the floor. The toy and then a free floor
    // tile are clicked in one tick: the toy is taken, then put down on that tile.
    click(&mut s, closet);
    s.run_until("the closet opens", |s| {
        s.server.world().get::<Closet>(closet).unwrap().opened
    });
    let toy = s
        .on(CLOSET, |e| {
            e.get::<DmItem>().is_some_and(|i| i.path == "/obj/item/toy")
        })
        .expect("the toy on the floor");
    let free = TilePos::new(2, 3, 1);
    let floor = s.on(free, |e| e.contains::<ss13_mapload::Turf>()).unwrap();
    s.run_until("the toy replicates", |s| {
        s.server
            .world()
            .resource::<Interest>()
            .net_id(s.link, toy)
            .is_some()
    });
    click(&mut s, toy);
    click(&mut s, floor);
    s.run_until("the toy is on the free tile", |s| {
        s.server.world().get::<TilePos>(toy) == Some(&free)
    });
    assert_eq!(s.inventory().hands[0], None);
}
