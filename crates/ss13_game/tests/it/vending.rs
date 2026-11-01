//! Vending over real WebTransport on 127.0.0.1: a headless client on a fixture station clicks a
//! vending machine, gets the `Vending` window, vends, is refused for foreign and sold-out records
//! and for spam, and the window closes when it walks away.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::vending::Vendor;
use ss13_mapload::DmItem;
use ss13_net::server::Interest;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId, TilePos};
use ss13_tgui::interfaces::vending::{VendingData, VendingStatic, VendingUi};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 2, "max_total_storage": 14 } },
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
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": {
        "name": "toy", "custom_price": 15, "w_class": 1 } },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/fancy": { "parent": "/obj/item/storage" },
    "/obj/item/storage/fancy/cigarettes": { "parent": "/obj/item/storage/fancy",
        "declared_vars": { "name": "cigarettes", "icon_state": "cigs", "w_class": 1 } },
    "/obj/item/toy/secret": { "parent": "/obj/item/toy" },
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
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/vending": { "parent": "/obj/machinery", "declared_vars": {
        "name": "Vendomat", "icon": { "resource": "v.dmi" }, "icon_state": "generic",
        "density": 1, "default_price": 25, "extra_price": 50, "payment_department": "SRV",
        "product_ads": "Buy!", "light_mask": "generic-mask",
        "products": { "list": [ { "assoc": [ { "path": "/obj/item/toy" }, 5 ] },
            { "assoc": [ { "path": "/obj/item/storage/fancy/cigarettes" }, 2 ] } ] },
        "contraband": { "list": [ { "assoc": [ { "path": "/obj/item/toy/secret" }, 1 ] } ] },
        "premium": { "list": [] } } },
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
    "/obj/item/storage/fancy/cigarettes": { "PopulateContents": [
        { "repeat": { "count": 2, "body": [{ "new": "/obj/item/toy" }] } }] }
  }
}"##;

/// Spawn `s` (2,3) next to the vendor `V` (3,3); a second vendor `X` (5,3) out of reach.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"V" = (/obj/machinery/vending,/turf/open/floor,/area/space)
"X" = (/obj/machinery/vending,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwwwww
wffffffffffffw
wsVfXffffffffw
wffffffffffffw
wwwwwwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 3, 1);
const VENDOR: TilePos = TilePos::new(3, 3, 1);
const FAR_VENDOR: TilePos = TilePos::new(5, 3, 1);

const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "card_grey"),
    ("v.dmi", "generic"),
    ("v.dmi", "generic-off"),
    ("v.dmi", "generic-mask"),
    ("i.dmi", "cigs"),
];
const TOY_ICON: IconStateId = IconStateId(4);
const VENDOR_ON: IconStateId = IconStateId(6);
const VENDOR_OFF: IconStateId = IconStateId(7);
const VENDOR_MASK: IconStateId = IconStateId(8);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vending_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/vending",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// What the client's tgui would render: the Vending window's id, static data and last data.
#[derive(Resource, Default)]
struct VendWindow {
    id: Option<u32>,
    static_data: Option<VendingStatic>,
    data: Option<VendingData>,
    closed: bool,
}

fn record_window(mut received: MessageReader<UiMessageReceived>, mut w: ResMut<VendWindow>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                static_data,
                data,
                ..
            } if interface == VendingUi::NAME => {
                *w = VendWindow {
                    id: Some(*window),
                    static_data: Some(decode(static_data).unwrap()),
                    data: Some(decode(data).unwrap()),
                    closed: false,
                };
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if w.id == Some(*window) => w.data = Some(decode(data).unwrap()),
            UiServerMessage::Close { window } if w.id == Some(*window) => w.closed = true,
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
        let mut st = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        st.join("p0", |app| {
            app.init_resource::<VendWindow>()
                .add_systems(Update, record_window);
        });
        let (link, mob) = (st.players[0].link, st.players[0].mob);
        Self { st, link, mob }
    }

    fn send(&mut self, intent: ClientIntent) {
        self.st.send(0, intent);
    }

    fn pos(&self) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob).unwrap()
    }

    fn walk(&mut self, dir: Dir) {
        let to = self.pos().step(dir);
        self.run_until(&format!("walk {dir:?} to {to:?}"), |s| {
            if s.pos() == to {
                return true;
            }
            s.try_step(0, dir);
            false
        });
    }

    fn vendor(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Vendor>(e).is_some())
            .unwrap()
    }

    fn stock(&self, vendor: Entity, record: usize) -> u32 {
        self.server.world().get::<Vendor>(vendor).unwrap().records[record].amount
    }

    fn click(&mut self, entity: Entity) {
        self.st.click(0, entity);
    }

    fn vend(&mut self, reference: &str) {
        let window = self.window().id.expect("window open");
        self.send(ClientIntent::UiAct {
            window,
            action: "vend".into(),
            params: vec![
                ("ref".into(), reference.into()),
                ("premium".into(), "0".into()),
            ],
        });
    }

    fn window(&self) -> &VendWindow {
        self.players[0].app.world().resource::<VendWindow>()
    }

    fn inventory(&self) -> Inventory {
        self.server
            .world()
            .get::<Inventory>(self.mob)
            .unwrap()
            .clone()
    }

    /// Product items lying on `pos`.
    fn items_on(&self, pos: TilePos) -> usize {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .filter(|&&e| world.get::<DmItem>(e).is_some())
            .count()
    }
}

#[test]
fn click_open_vend_refuse_and_close() {
    let mut s = Station::start("click_open_vend_refuse_and_close");
    assert_eq!(s.pos(), SPAWN);
    let vendor = s.vendor(VENDOR);
    let far = s.vendor(FAR_VENDOR);

    // Out of reach: no window.
    s.run_until("both vendors are replicated", |s| {
        let interest = s.server.world().resource::<Interest>();
        [vendor, far]
            .iter()
            .all(|&v| interest.net_id(s.link, v).is_some())
    });
    s.click(far);
    s.hold(0.3, "no window for a far vendor", |s| {
        s.window().id.is_none()
    });

    // Adjacent, empty hand: the window opens with the records and stock.
    s.click(vendor);
    s.run_until("the Vending window opens", |s| s.window().id.is_some());
    let st = s.window().static_data.clone().unwrap();
    assert_eq!(st.product_records.len(), 2);
    let toy = &st.product_records[0];
    assert_eq!(
        (toy.name.as_str(), toy.price, toy.path.as_str()),
        ("Toy", 15, "toy")
    );
    assert_eq!(toy.icon.map(|i| i.icon_state), Some(TOY_ICON));
    assert_eq!(
        st.hidden_records.len(),
        1,
        "contraband is listed, hidden by the client"
    );
    assert_eq!(st.ad, "Buy!");
    assert_eq!(st.displayed_currency_name, " ¢");
    let data = s.window().data.clone().unwrap();
    assert_eq!(data.stock[0].amount, 5);
    let user = data.user.expect("the assistant ID is found");
    assert_eq!((user.job.as_str(), user.cash), ("Assistant", None));

    // Foreign records: not in this vendor, contraband of an unhacked vendor, garbage.
    for reference in ["7", "2", "-1", "x"] {
        s.vend(reference);
    }
    s.hold(0.3, "foreign records sell nothing", |s| {
        s.stock(vendor, 0) == 5 && s.stock(vendor, 2) == 1 && s.inventory().hands == [None, None]
    });

    // One vend: into the empty active hand, stock re-sent.
    s.vend("0");
    s.run_until("the toy is in hand and the stock re-sent", |s| {
        s.inventory().hands[0].is_some()
            && s.window()
                .data
                .as_ref()
                .is_some_and(|d| d.stock[0].amount == 4)
    });
    assert_eq!(s.stock(vendor, 0), 4);

    // Spam: 30 acts in one tick sell what is left (4), never more; the rest is sold out or
    // rate limited. The second item fills the other hand, the rest lands on the vendor's tile.
    for _ in 0..30 {
        s.vend("0");
    }
    s.run_until("the stock runs out", |s| s.stock(vendor, 0) == 0);
    s.hold(0.5, "nothing more is sold", |s| {
        s.stock(vendor, 0) == 0 && s.items_on(VENDOR) == 3
    });
    assert!(s.inventory().hands.iter().all(Option::is_some));
    s.run_until("the client sees it sold out", |s| {
        s.window()
            .data
            .as_ref()
            .is_some_and(|d| d.stock[0].amount == 0)
    });

    // Walking away: two tiles is update-only (acts refused), past five tiles the window closes.
    s.walk(Dir::South);
    s.walk(Dir::East);
    s.walk(Dir::East);
    s.walk(Dir::East);
    assert_eq!(s.pos(), TilePos::new(5, 2, 1));
    assert!(!s.window().closed);
    for _ in 0..4 {
        s.walk(Dir::East);
    }
    s.run_until("the window closes", |s| s.window().closed);
}

/// Vended items are finished like any other (a pack fills itself and takes items), and a vendor
/// without power goes dark, opens nothing and sells nothing (tg `can_interact` with `NOPOWER`).
#[test]
fn vended_storage_and_power() {
    use ss13_game::power::Unpowered;
    use ss13_game::storage::{Storage, StorageView, Stored, StoredIn};
    use ss13_shared::EMISSIVE_PLANE;
    use ss13_shared::protocol::InventorySlot;

    let mut s = Station::start("vended_storage_and_power");
    let vendor = s.vendor(VENDOR);
    let look = |s: &Station| {
        let a = s
            .server
            .world()
            .get::<ss13_shared::Appearance>(vendor)
            .unwrap();
        let mask = a
            .overlays
            .iter()
            .any(|o| o.icon_state == VENDOR_MASK && o.plane == Some(EMISSIVE_PLANE));
        (a.icon_state, mask)
    };
    assert_eq!(look(&s), (VENDOR_ON, true), "powered: the light mask glows");
    s.run_until("the vendor is replicated", |s| {
        s.server
            .world()
            .resource::<Interest>()
            .net_id(s.link, vendor)
            .is_some()
    });

    // No power: dark sprite, a click opens nothing.
    s.st.server.world_mut().entity_mut(vendor).insert(Unpowered);
    assert_eq!(look(&s), (VENDOR_OFF, false));
    s.click(vendor);
    s.hold(0.3, "an unpowered vendor opens no window", |s| {
        s.window().id.is_none()
    });

    // Power back: lit again, the window opens.
    s.server
        .world_mut()
        .entity_mut(vendor)
        .remove::<Unpowered>();
    assert_eq!(look(&s), (VENDOR_ON, true));
    s.click(vendor);
    s.run_until("the Vending window opens", |s| s.window().id.is_some());

    // A vended pack of cigarettes filled itself (PopulateContents) and is a storage.
    s.vend("1");
    s.run_until("the pack is in hand", |s| s.inventory().hands[0].is_some());
    let pack = s.inventory().hands[0].unwrap();
    let world = s.server.world();
    assert!(
        world.get::<Storage>(pack).is_some(),
        "the pack is a storage"
    );
    let stored = |s: &Station| {
        s.server
            .world()
            .get::<Stored>(pack)
            .map_or(0, |st| st.items().len())
    };
    assert_eq!(stored(&s), 2, "the pack came full");

    // It takes a toy from the other hand and opens with an empty hand.
    s.send(ClientIntent::SwapHands);
    s.run_until("the right hand is active", |s| s.inventory().active == 1);
    s.vend("0");
    s.run_until("the toy is in the right hand", |s| {
        s.inventory().hands[1].is_some()
    });
    let toy = s.inventory().hands[1].unwrap();
    s.send(ClientIntent::ClickSlot(InventorySlot::LeftHand));
    s.run_until("the toy went into the pack", |s| {
        s.server
            .world()
            .get::<StoredIn>(toy)
            .is_some_and(|i| i.0 == pack)
    });
    assert_eq!(stored(&s), 3);
    s.send(ClientIntent::ClickSlot(InventorySlot::LeftHand));
    s.run_until("the pack's storage opens", |s| {
        s.server
            .world()
            .get::<StorageView>(s.mob)
            .is_some_and(|v| v.storage == Some(pack))
    });

    // Power lost with the window open: acts are refused, the window stays (update-only).
    let toys = s.stock(vendor, 0);
    s.st.server.world_mut().entity_mut(vendor).insert(Unpowered);
    s.vend("0");
    s.hold(0.5, "an unpowered vendor sells nothing", |s| {
        s.stock(vendor, 0) == toys && !s.window().closed
    });
}
