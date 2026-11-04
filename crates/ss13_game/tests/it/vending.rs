//! Vending over real WebTransport on 127.0.0.1: a headless client on a fixture station clicks a
//! vending machine, gets the `Vending` window, vends and pays from its ID's account, is refused
//! for foreign and sold-out records, for spam, without funds and underage; puts items back,
//! restocks from a canister, tips the vendor over and rights it, takes it apart and builds it
//! back; runs a custom vendor as its owner and as a buyer; hears a slogan.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::cargo::economy::{BankAccountId, Economy};
use ss13_game::construction::PanelOpen;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::vending::custom::CustomVendor;
use ss13_game::vending::refill::{RefillStock, VendorBoard};
use ss13_game::vending::tilt::{Crit, PinnedUnder, Tilted};
use ss13_game::vending::{VendingInflation, Vendor};
use ss13_mapload::DmItem;
use ss13_net::chat::client::ChatReceived;
use ss13_net::server::Interest;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Appearance, Dir, IconStateId, TilePos};
use ss13_tgui::interfaces::vending::{VendingData, VendingStatic, VendingUi};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 2, "max_total_storage": 14 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "flags_1": 0,
        "max_integrity": 500, "armor_type": { "expr": "/datum/armor" },
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
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3, "attack_speed": 8,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "tool_behaviour": null, "toolspeed": 1,
        "heat": 0, "age_restricted": 0 } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": {
        "name": "toy", "custom_price": 15, "w_class": 1 } },
    "/obj/item/toy/gold": { "parent": "/obj/item/toy", "declared_vars": {
        "name": "gold toy", "custom_price": 0 } },
    "/obj/item/toolbox": { "parent": "/obj/item", "declared_vars": {
        "name": "toolbox", "icon_state": "toolbox", "force": 12 } },
    "/obj/item/crowbar": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "crowbar", "tool_behaviour": "crowbar", "force": 5 } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "screwdriver", "tool_behaviour": "screwdriver", "force": 5 } },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/fancy": { "parent": "/obj/item/storage" },
    "/obj/item/storage/fancy/cigarettes": { "parent": "/obj/item/storage/fancy",
        "declared_vars": { "name": "cigarettes", "icon_state": "cigs", "w_class": 1,
        "age_restricted": 1 } },
    "/obj/item/toy/secret": { "parent": "/obj/item/toy" },
    "/obj/item/vending_refill": { "parent": "/obj/item", "declared_vars": {
        "name": "resupply canister", "icon_state": "refill", "machine_name": "Generic",
        "w_class": 4 } },
    "/obj/item/vending_refill/test": { "parent": "/obj/item/vending_refill",
        "declared_vars": { "machine_name": "Vendomat" } },
    "/obj/item/vending_refill/custom": { "parent": "/obj/item/vending_refill",
        "declared_vars": { "machine_name": "Custom Vendor" } },
    "/obj/item/circuitboard": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "circuit_map", "build_path": null } },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard",
        "declared_vars": { "needs_anchored": 1, "req_components": null,
        "def_components": null } },
    "/obj/item/circuitboard/machine/vendor": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "name": "Custom Vendor",
        "build_path": { "path": "/obj/machinery/vending/custom" },
        "req_components": { "list": [
            { "assoc": [{ "path": "/obj/item/vending_refill/custom" }, 1] } ] } } },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "max_integrity": 300, "icon": { "resource": "s.dmi" } } },
    "/obj/structure/frame": { "parent": "/obj/structure", "declared_vars": {
        "name": "frame", "icon_state": "box_0", "density": 1, "anchored": 0, "state": 0,
        "max_integrity": 250 } },
    "/obj/structure/frame/machine": { "parent": "/obj/structure/frame", "declared_vars": {
        "name": "machine frame" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "circuit": null } },
    "/obj/machinery/vending": { "parent": "/obj/machinery", "declared_vars": {
        "name": "Vendomat", "icon": { "resource": "v.dmi" }, "icon_state": "generic",
        "density": 1, "default_price": 25, "extra_price": 50, "payment_department": "SRV",
        "product_ads": "Buy!", "product_slogans": "Slogan!", "light_mask": "generic-mask",
        "max_integrity": 300, "integrity_failure": 0.33, "tiltable": 1, "squish_damage": 75,
        "crit_chance": 15, "age_restrictions": 1, "onstation": 1, "allow_custom": 1,
        "max_loaded_items": 0, "req_access": null,
        "circuit": { "path": "/obj/item/circuitboard/machine/vendor" },
        "refill_canister": { "path": "/obj/item/vending_refill/test" },
        "products": { "list": [ { "assoc": [ { "path": "/obj/item/toy" }, 5 ] },
            { "assoc": [ { "path": "/obj/item/storage/fancy/cigarettes" }, 2 ] } ] },
        "contraband": { "list": [ { "assoc": [ { "path": "/obj/item/toy/secret" }, 1 ] } ] },
        "premium": { "list": [ { "assoc": [ { "path": "/obj/item/toy/gold" }, 1 ] } ] } } },
    "/obj/machinery/vending/custom": { "parent": "/obj/machinery/vending", "declared_vars": {
        "name": "Custom Vendor", "icon_state": "custom", "payment_department": "commies go home",
        "max_loaded_items": 20, "products": { "list": [] }, "contraband": { "list": [] },
        "premium": { "list": [] },
        "refill_canister": { "path": "/obj/item/vending_refill/custom" } } },
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

/// Spawn `s` (2,3) next to the vendor `V` (3,3) and the custom vendor `U` (2,4); a second
/// vendor `X` (5,3) out of reach.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"V" = (/obj/machinery/vending,/turf/open/floor,/area/space)
"X" = (/obj/machinery/vending,/turf/open/floor,/area/space)
"U" = (/obj/machinery/vending/custom,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwwwww
wUfffffffffffw
wsVfXffffffffw
wffffffffffffw
wwwwwwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 3, 1);
const VENDOR: TilePos = TilePos::new(3, 3, 1);
const FAR_VENDOR: TilePos = TilePos::new(5, 3, 1);
const CUSTOM: TilePos = TilePos::new(2, 4, 1);

const TOY: &str = "/obj/item/toy";
const TOOLBOX: &str = "/obj/item/toolbox";
const CROWBAR: &str = "/obj/item/crowbar";
const CANISTER: &str = "/obj/item/vending_refill/test";
const VENDOR_BOARD: &str = "/obj/item/circuitboard/machine/vendor";

const STATES: [(&str, &str); 18] = [
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
    ("v.dmi", "custom"),
    ("v.dmi", "generic-broken"),
    ("i.dmi", "toolbox"),
    ("i.dmi", "crowbar"),
    ("i.dmi", "screwdriver"),
    ("i.dmi", "refill"),
    ("i.dmi", "circuit_map"),
    ("s.dmi", "box_1"),
];
const TOY_ICON: IconStateId = IconStateId(4);
const VENDOR_ON: IconStateId = IconStateId(6);
const VENDOR_OFF: IconStateId = IconStateId(7);
const VENDOR_MASK: IconStateId = IconStateId(8);

/// Records: products toy (0), cigarettes (1); premium gold toy (2); contraband secret (3).
const R_TOY: usize = 0;
const R_CIGS: usize = 1;
const R_GOLD: usize = 2;
const R_SECRET: usize = 3;

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vending_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/vending",
        "/obj/machinery/vending/custom",
        TOOLBOX,
        CROWBAR,
        "/obj/item/screwdriver",
        CANISTER,
        "/obj/item/vending_refill/custom",
        VENDOR_BOARD,
        "/obj/structure/frame/machine",
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

#[derive(Resource, Default)]
struct Chat(Vec<String>);

/// Other windows the server opened: (interface, window id).
#[derive(Resource, Default)]
struct Prompts(Vec<(String, u32)>);

fn record_prompts(mut received: MessageReader<UiMessageReceived>, mut out: ResMut<Prompts>) {
    for UiMessageReceived(message) in received.read() {
        if let UiServerMessage::Open {
            window, interface, ..
        } = message
            && interface != VendingUi::NAME
        {
            out.0.push((interface.clone(), *window));
        }
    }
}

fn record_chat(mut chat: MessageReader<ChatReceived>, mut out: ResMut<Chat>) {
    for ChatReceived(message) in chat.read() {
        out.0.push(message.plain_text());
    }
}

/// One client (`p0`): its connection, mob and bank account (100 credits, civilian).
struct Station {
    st: testing::Station,
    link: Entity,
    mob: Entity,
    account: u32,
}

ss13_game::drive_station!(Station, st);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut st = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        st.join("p0", |app| {
            app.init_resource::<VendWindow>()
                .init_resource::<Chat>()
                .init_resource::<Prompts>()
                .add_systems(Update, (record_window, record_chat, record_prompts));
        });
        let (link, mob) = (st.players[0].link, st.players[0].mob);
        let mut s = Self {
            st,
            link,
            mob,
            account: 0,
        };
        s.account = s.open_account("p0", "CIV", 100);
        let card = s.card();
        let account = s.account;
        s.st.server
            .world_mut()
            .entity_mut(card)
            .insert(BankAccountId(account));
        s
    }

    fn open_account(&mut self, holder: &str, department: &str, balance: i64) -> u32 {
        let world = self.st.server.world_mut();
        let mut economy = world.resource_mut::<Economy>();
        let id = economy.open_account(holder, 50, department);
        let account = economy.account_mut(id).unwrap();
        account.balance = balance;
        account.job = "Assistant".into();
        id
    }

    fn card(&self) -> Entity {
        self.inventory().id.expect("the assistant has an ID")
    }

    fn balance(&self, account: u32) -> i64 {
        self.server
            .world()
            .resource::<Economy>()
            .account(account)
            .unwrap()
            .balance
    }

    fn budget(&self, department: &str) -> i64 {
        self.server
            .world()
            .resource::<Economy>()
            .department(department)
            .unwrap()
            .balance
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

    fn machine(&self, vendor: Entity) -> &Vendor {
        self.server.world().get::<Vendor>(vendor).unwrap()
    }

    fn stock(&self, vendor: Entity, record: usize) -> u32 {
        self.machine(vendor)
            .records
            .get(record)
            .map_or(0, |r| r.amount)
    }

    fn replicated(&mut self, e: Entity) {
        self.run_until("replicated", |s| {
            s.server
                .world()
                .resource::<Interest>()
                .net_id(s.link, e)
                .is_some()
        });
    }

    fn click(&mut self, entity: Entity) {
        let mob = self.mob;
        self.run_until("off click cooldown", |s| {
            s.st.now()
                >= s.server
                    .world()
                    .get::<ss13_game::combat::NextMove>(mob)
                    .map_or(0.0, |n| n.0)
        });
        self.replicated(entity);
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

    fn open(&mut self, vendor: Entity) {
        self.players[0]
            .app
            .world_mut()
            .insert_resource(VendWindow::default());
        self.click(vendor);
        self.run_until("the Vending window opens", |s| s.window().id.is_some());
    }

    fn window(&self) -> &VendWindow {
        self.players[0].app.world().resource::<VendWindow>()
    }

    fn chat(&self) -> Vec<String> {
        self.players[0].app.world().resource::<Chat>().0.clone()
    }

    fn inventory(&self) -> Inventory {
        self.server
            .world()
            .get::<Inventory>(self.mob)
            .unwrap()
            .clone()
    }

    /// A new `path` item in the empty active hand.
    fn hold_item(&mut self, path: &str) -> Entity {
        let mob = self.mob;
        let pos = self.pos();
        let world = self.st.server.world_mut();
        let item = ss13_game::construction::spawn_obj(world, path, pos).unwrap();
        let mut inventory = world.get::<Inventory>(mob).unwrap().clone();
        assert!(
            inventory.active_item().is_none(),
            "the active hand is empty"
        );
        inventory.hands[inventory.active] = Some(item);
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        world.entity_mut(mob).insert(inventory);
        item
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

    fn find_on(&self, pos: TilePos, path: &str) -> Option<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
    }

    fn said(&self, text: &str) -> bool {
        self.chat().iter().any(|l| l.contains(text))
    }

    /// Answers the newest `interface` prompt.
    fn answer(&mut self, interface: &str, action: &str, key: &str, value: &str) {
        self.run_until("the prompt opens", |s| {
            s.players[0]
                .app
                .world()
                .resource::<Prompts>()
                .0
                .iter()
                .any(|(i, _)| i == interface)
        });
        let window = self.players[0]
            .app
            .world()
            .resource::<Prompts>()
            .0
            .iter()
            .rev()
            .find(|(i, _)| i == interface)
            .unwrap()
            .1;
        self.send(ClientIntent::UiAct {
            window,
            action: action.into(),
            params: vec![(key.into(), value.into())],
        });
    }
}

#[test]
fn click_open_vend_refuse_and_close() {
    let mut s = Station::start("click_open_vend_refuse_and_close");
    assert_eq!(s.pos(), SPAWN);
    let vendor = s.vendor(VENDOR);
    let far = s.vendor(FAR_VENDOR);

    // Out of reach: no window.
    s.replicated(vendor);
    s.replicated(far);
    s.st.click(0, far);
    s.hold(0.3, "no window for a far vendor", |s| {
        s.window().id.is_none()
    });

    // Adjacent, empty hand: the window opens with the records, stock and the account.
    s.open(vendor);
    let st = s.window().static_data.clone().unwrap();
    assert_eq!(st.product_records.len(), 2);
    let toy = &st.product_records[0];
    assert_eq!(
        (toy.name.as_str(), toy.price, toy.path.as_str()),
        ("Toy", 15, "toy")
    );
    assert_eq!(toy.icon.map(|i| i.icon_state), Some(TOY_ICON));
    assert_eq!(st.coin_records[0].price, 50, "premium: extra_price");
    assert_eq!(
        st.hidden_records.len(),
        1,
        "contraband is listed, hidden by the client"
    );
    assert_eq!(st.ad, "Buy!");
    assert_eq!(st.displayed_currency_name, " ¢");
    let data = s.window().data.clone().unwrap();
    assert_eq!(data.stock[R_TOY].amount, 5);
    let user = data.user.expect("the ID has an account");
    assert_eq!(
        (user.job.as_str(), user.cash, user.department.as_str()),
        ("Assistant", Some(100), "CIV")
    );

    // Foreign records: not in this vendor, contraband of an unhacked vendor, garbage.
    let srv = s.budget("SRV");
    for reference in ["7", "3", "-1", "x"] {
        s.vend(reference);
    }
    s.hold(0.3, "foreign records sell nothing", |s| {
        s.stock(vendor, R_TOY) == 5
            && s.stock(vendor, R_SECRET) == 1
            && s.inventory().hands == [None, None]
    });

    // One vend: into the empty active hand, paid from the account into the service budget.
    s.vend("0");
    s.run_until("the toy is in hand and the stock re-sent", |s| {
        s.inventory().hands[0].is_some()
            && s.window()
                .data
                .as_ref()
                .is_some_and(|d| d.stock[R_TOY].amount == 4)
    });
    assert_eq!(s.stock(vendor, R_TOY), 4);
    assert_eq!(s.balance(s.account), 85);
    assert_eq!(s.budget("SRV"), srv + 15);
    s.run_until("the vendor thanks", |s| {
        s.said("Благодарим за использование")
    });

    // Spam: 30 acts in one tick sell what is left (4), never more; the rest is sold out or
    // rate limited. The second item fills the other hand, the rest lands on the vendor's tile.
    for _ in 0..30 {
        s.vend("0");
    }
    s.run_until("the stock runs out", |s| s.stock(vendor, R_TOY) == 0);
    s.hold(0.5, "nothing more is sold", |s| {
        s.stock(vendor, R_TOY) == 0 && s.items_on(VENDOR) == 3
    });
    assert!(s.inventory().hands.iter().all(Option::is_some));
    assert_eq!(s.balance(s.account), 25, "five toys paid, once each");
    s.run_until("the client sees it sold out", |s| {
        s.window()
            .data
            .as_ref()
            .is_some_and(|d| d.stock[R_TOY].amount == 0)
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
        let a = s.server.world().get::<Appearance>(vendor).unwrap();
        let mask = a
            .overlays
            .iter()
            .any(|o| o.icon_state == VENDOR_MASK && o.plane == Some(EMISSIVE_PLANE));
        (a.icon_state, mask)
    };
    assert_eq!(look(&s), (VENDOR_ON, true), "powered: the light mask glows");
    s.replicated(vendor);

    // No power: dark sprite, a click opens nothing.
    s.st.server.world_mut().entity_mut(vendor).insert(Unpowered);
    s.tick();
    assert_eq!(look(&s), (VENDOR_OFF, false));
    s.st.click(0, vendor);
    s.hold(0.3, "an unpowered vendor opens no window", |s| {
        s.window().id.is_none()
    });

    // Power back: lit again, the window opens.
    s.st.server
        .world_mut()
        .entity_mut(vendor)
        .remove::<Unpowered>();
    s.tick();
    assert_eq!(look(&s), (VENDOR_ON, true));
    s.open(vendor);

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
    let toys = s.stock(vendor, R_TOY);
    s.st.server.world_mut().entity_mut(vendor).insert(Unpowered);
    s.vend("0");
    s.hold(0.5, "an unpowered vendor sells nothing", |s| {
        s.stock(vendor, R_TOY) == toys && !s.window().closed
    });
}

/// `proceed_payment`: the vendor's own department pays 20% (premium never), an account without
/// the money is refused and keeps it, an ID under 20 buys no cigarettes, a free vendor charges
/// nothing; a market crash inflates the prices in the open window.
#[test]
fn payment_discount_funds_age_and_inflation() {
    let mut s = Station::start("payment_discount_funds_age_and_inflation");
    let vendor = s.vendor(VENDOR);
    let account = s.account;
    let set_balance = |s: &mut Station, balance: i64| {
        s.st.server
            .world_mut()
            .resource_mut::<Economy>()
            .account_mut(account)
            .unwrap()
            .balance = balance;
    };
    s.st.server
        .world_mut()
        .resource_mut::<Economy>()
        .account_mut(account)
        .unwrap()
        .paycheck_department = "SRV".into();
    s.open(vendor);
    let srv = s.budget("SRV");

    // Own department: 15 * 0.2 = 3.
    s.vend("0");
    s.run_until("the discounted toy is paid", |s| s.balance(account) == 97);
    assert_eq!(s.budget("SRV"), srv + 3);
    // Premium: full price.
    s.vend(&R_GOLD.to_string());
    s.run_until("the gold toy is paid in full", |s| s.balance(account) == 47);
    assert_eq!(s.stock(vendor, R_GOLD), 0);

    // Not enough money: refused, nothing taken.
    set_balance(&mut s, 2);
    s.vend(&R_CIGS.to_string());
    s.hold(0.5, "no funds, no cigarettes", |s| {
        s.stock(vendor, R_CIGS) == 2 && s.balance(account) == 2
    });
    s.run_until("the vendor says so", |s| s.said("недостаточно средств"));

    // Underage: cigarettes are age-restricted.
    set_balance(&mut s, 100);
    let card = s.card();
    s.st.server
        .world_mut()
        .get_mut::<ss13_game::id_access::CardAccess>(card)
        .unwrap()
        .registered_age = 18;
    s.vend(&R_CIGS.to_string());
    s.hold(0.5, "an 18 year old buys no cigarettes", |s| {
        s.stock(vendor, R_CIGS) == 2 && s.balance(account) == 100
    });
    s.run_until("the vendor says why", |s| s.said("недостаточно взрослы"));
    // A vendor without the age check (`age_restrictions` off) sells them.
    s.st.server
        .world_mut()
        .get_mut::<Vendor>(vendor)
        .unwrap()
        .age_restrictions = false;
    s.vend(&R_CIGS.to_string());
    s.run_until("cigarettes sold", |s| s.stock(vendor, R_CIGS) == 1);
    assert_eq!(s.balance(account), 95, "25 * 0.2");

    // A free vendor charges nothing.
    s.st.server
        .world_mut()
        .get_mut::<Vendor>(vendor)
        .unwrap()
        .all_products_free = true;
    s.vend("0");
    s.run_until("a free toy", |s| s.stock(vendor, R_TOY) == 3);
    assert_eq!(s.balance(account), 95);

    // Market crash: the open window gets the inflated prices.
    let world = s.st.server.world_mut();
    world.insert_resource(VendingInflation(2.0));
    ss13_game::vending::update_prices(world);
    s.run_until("the window shows doubled prices", |s| {
        s.window()
            .static_data
            .as_ref()
            .is_some_and(|d| d.product_records[0].price == 30)
    });
    assert_eq!(s.machine(vendor).records[R_TOY].price, 30);
}

/// `loadingAttempt`: an item of the vendor's type goes back in and comes out first, free; a
/// canister restocks with the panel open and pays the collected credits.
#[test]
fn items_go_back_and_canisters_restock() {
    use ss13_game::storage::StoredIn;

    let mut s = Station::start("items_go_back_and_canisters_restock");
    let vendor = s.vendor(VENDOR);
    let account = s.account;
    s.open(vendor);
    s.vend("0");
    s.run_until("a toy in hand", |s| s.inventory().hands[0].is_some());
    let toy = s.inventory().hands[0].unwrap();
    assert_eq!(s.balance(account), 85);
    assert_eq!(s.machine(vendor).credits_contained, 3, "20% of 15");

    // Back in: stock 5 again, the toy inside, shown as free.
    s.click(vendor);
    s.run_until("the toy is back in", |s| {
        s.stock(vendor, R_TOY) == 5
            && s.server
                .world()
                .get::<StoredIn>(toy)
                .is_some_and(|i| i.0 == vendor)
    });
    assert_eq!(s.machine(vendor).records[R_TOY].returned, [toy]);
    s.run_until("the window shows it free", |s| {
        s.window()
            .data
            .as_ref()
            .is_some_and(|d| d.stock[R_TOY].free)
    });

    // Vended again: the same toy, for nothing.
    s.vend("0");
    s.run_until("the returned toy comes out", |s| {
        s.inventory().hands[0] == Some(toy)
    });
    assert_eq!(s.balance(account), 85);
    assert!(s.machine(vendor).records[R_TOY].returned.is_empty());

    // Not one of its products: refused, still in hand.
    let mob = s.mob;
    s.st.server
        .world_mut()
        .get_mut::<Inventory>(mob)
        .unwrap()
        .hands[0] = None;
    let toolbox = s.hold_item(TOOLBOX);
    s.click(vendor);
    s.run_until("a toolbox is refused", |s| s.said("не принимает"));
    assert_eq!(s.inventory().hands[0], Some(toolbox));
    s.st.server
        .world_mut()
        .get_mut::<Inventory>(mob)
        .unwrap()
        .hands[0] = None;

    // Restock: the canister needs the open panel; it fills every record and pays the credits.
    s.st.server
        .world_mut()
        .get_mut::<Vendor>(vendor)
        .unwrap()
        .records[R_TOY]
        .amount = 1;
    let cargo = s.budget("CAR");
    let canister = s.hold_item(CANISTER);
    s.click(vendor);
    s.run_until("refused with the panel closed", |s| {
        s.said("панель обслуживания")
    });
    assert_eq!(s.stock(vendor, R_TOY), 1);
    s.st.server
        .world_mut()
        .entity_mut(vendor)
        .insert(PanelOpen(true));
    s.click(vendor);
    s.run_until("restocked", |s| s.stock(vendor, R_TOY) == 5);
    assert_eq!(s.balance(account), 85 + 3, "the collected credits");
    assert_eq!(s.budget("CAR"), cargo + 2, "round(3 * 0.5)");
    assert_eq!(s.machine(vendor).credits_contained, 0);
    let left = s
        .server
        .world()
        .get::<RefillStock>(canister)
        .cloned()
        .unwrap()
        .0
        .unwrap();
    assert_eq!(
        left[0],
        [
            ("/obj/item/toy".to_owned(), 1),
            ("/obj/item/storage/fancy/cigarettes".to_owned(), 2)
        ],
        "a fresh canister holds the full lists less what went in"
    );
}

/// `tilt`: the vendor falls onto the mob next to it (damage, paralysis, the vendor on its tile,
/// turned, above mobs); an empty hand rights it in 5 s; a pin holds the mob until then. Hits
/// with a toolbox in combat mode end up tipping it.
#[test]
fn tilt_crush_pin_and_right() {
    use ss13_game::health::Health;
    use ss13_game::status_effects::{Incap, StatusEffects};

    let mut s = Station::start("tilt_crush_pin_and_right");
    let vendor = s.vendor(VENDOR);
    let mob = s.mob;
    s.replicated(vendor);
    let paralyzed = |s: &Station| {
        s.server
            .world()
            .get::<StatusEffects>(mob)
            .map_or(0.0, |e| e.remaining(Incap::Paralyze))
    };
    let heal = |s: &mut Station| {
        let world = s.st.server.world_mut();
        let healed = world.get::<Health>(mob).unwrap().healed();
        world
            .entity_mut(mob)
            .insert((healed, ss13_game::health::Stat::Conscious));
    };
    let world = s.st.server.world_mut();
    assert!(ss13_game::vending::tilt::tilt(
        world, vendor, mob, 0.0, None
    ));
    let world = s.server.world();
    assert!(
        world.get::<Health>(mob).unwrap().health() < 100.0,
        "crushed"
    );
    assert!(paralyzed(&s) > 5.0);
    assert_eq!(*world.get::<TilePos>(vendor).unwrap(), SPAWN, "on the mob");
    let a = world.get::<Appearance>(vendor).unwrap();
    assert!(matches!(a.rotation, 90 | 270));
    assert_eq!(a.layer.0, 4.1);
    assert!(world.get::<Tilted>(vendor).is_some());

    // Righting: paralyzed first, then 5 s with an empty hand.
    s.run_until_for(10.0, "the paralysis ends", |s| paralyzed(s) <= 0.0);
    s.click(vendor);
    s.run_until("righting started", |s| {
        s.server
            .world()
            .get::<ss13_game::tools::DoAfter>(mob)
            .is_some()
    });
    s.hold(4.5, "still lying", |s| {
        s.server.world().get::<Tilted>(vendor).is_some()
    });
    s.run_until("upright", |s| {
        s.server.world().get::<Tilted>(vendor).is_none()
    });
    assert_eq!(
        s.server.world().get::<Appearance>(vendor).unwrap().rotation,
        0
    );

    // A pin holds the mob down until the vendor is righted.
    heal(&mut s);
    let world = s.st.server.world_mut();
    assert!(ss13_game::vending::tilt::tilt(
        world,
        vendor,
        mob,
        100.0,
        Some(Crit::Pin)
    ));
    assert_eq!(
        s.server.world().get::<PinnedUnder>(mob),
        Some(&PinnedUnder(vendor))
    );
    s.run_until_for(10.0, "the paralysis ends", |s| paralyzed(s) <= 0.0);
    s.hold(1.0, "pinned: immobilized", |s| {
        s.server
            .world()
            .get::<StatusEffects>(mob)
            .is_some_and(|e| e.remaining(Incap::Immobilize) > 0.0)
    });
    // The pinned mob cannot right it.
    s.click(vendor);
    s.hold(0.5, "no righting from under it", |s| {
        s.server
            .world()
            .get::<ss13_game::tools::DoAfter>(mob)
            .is_none()
    });
    let world = s.st.server.world_mut();
    ss13_game::vending::tilt::untilt(world, vendor, None);
    s.run_until("free", |s| {
        s.server.world().get::<PinnedUnder>(mob).is_none()
    });

    // Hits: damage, and sooner or later a tilt.
    heal(&mut s);
    s.st.server
        .world_mut()
        .entity_mut(mob)
        .insert(ss13_game::combat::CombatMode(true));
    s.hold_item(TOOLBOX);
    let integrity = |s: &Station| {
        s.server
            .world()
            .get::<ss13_game::obj_damage::Integrity>(vendor)
            .unwrap()
            .current
    };
    let full = integrity(&s);
    s.run_until_for(60.0, "a hit tips it over", |s| {
        if s.server.world().get::<Tilted>(vendor).is_some() {
            return true;
        }
        let next = s
            .server
            .world()
            .get::<ss13_game::combat::NextMove>(mob)
            .map_or(0.0, |n| n.0);
        if s.st.now() >= next && paralyzed(s) <= 0.0 {
            s.st.click(0, vendor);
        }
        false
    });
    assert!(integrity(&s) < full);
}

/// The crowbar with the panel open: the vendor board with its brand and the canister with the
/// stock (a returned item falls out); the board and canister build the same vendor with the
/// same stock. A screwdriver turns a board's brand dial.
#[test]
fn taken_apart_and_built_back() {
    use ss13_game::stock_parts::ComponentParts;

    let mut s = Station::start("taken_apart_and_built_back");
    let vendor = s.vendor(VENDOR);
    s.replicated(vendor);
    let world = s.st.server.world_mut();
    {
        let mut machine = world.get_mut::<Vendor>(vendor).unwrap();
        machine.records[R_TOY].amount = 3;
        machine.records[R_GOLD].amount = 0;
    }
    let toy = ss13_game::construction::spawn_obj(world, TOY, SPAWN).unwrap();
    ss13_game::vending::load_item(world, vendor, toy).unwrap();
    world.entity_mut(vendor).insert(PanelOpen(true));
    s.hold_item(CROWBAR);
    s.click(vendor);
    s.run_until("taken apart", |s| {
        s.server.world().get_entity(vendor).is_err()
    });
    let world = s.server.world();
    let board = s.find_on(VENDOR, VENDOR_BOARD).expect("the board");
    assert_eq!(
        world.get::<VendorBoard>(board).unwrap().build_path,
        "/obj/machinery/vending"
    );
    let canister = s.find_on(VENDOR, CANISTER).expect("the canister");
    let lists = world
        .get::<RefillStock>(canister)
        .unwrap()
        .0
        .clone()
        .unwrap();
    assert_eq!(
        lists,
        [
            vec![
                ("/obj/item/toy".to_owned(), 3),
                ("/obj/item/storage/fancy/cigarettes".to_owned(), 2)
            ],
            vec![("/obj/item/toy/secret".to_owned(), 1)],
            vec![],
        ]
    );
    assert_eq!(
        world.get::<TilePos>(toy),
        Some(&VENDOR),
        "the returned toy fell out"
    );

    let world = s.st.server.world_mut();
    let built = ss13_game::machine_frame::build_machine(
        world,
        VENDOR,
        VENDOR_BOARD.into(),
        vec![board, canister],
        true,
        None,
    )
    .expect("built");
    let machine = world.get::<Vendor>(built).expect("a vendor again");
    let amounts: Vec<u32> = machine.records.iter().map(|r| r.amount).collect();
    assert_eq!(amounts, [3, 2, 0, 1]);
    assert_eq!(world.get::<ComponentParts>(built).unwrap().parts().len(), 2);
    assert_eq!(
        world.get::<RefillStock>(canister).unwrap().0,
        Some([vec![], vec![], vec![]])
    );

    // The brand dial: a screwdriver on a board on the floor picks another brand.
    let board = ss13_game::construction::spawn_obj(world, VENDOR_BOARD, SPAWN).unwrap();
    let mob = s.mob;
    s.st.server
        .world_mut()
        .get_mut::<Inventory>(mob)
        .unwrap()
        .hands = [None, None];
    s.hold_item("/obj/item/screwdriver");
    s.click(board);
    // The fixture names types after their path.
    s.answer("ListInputWindow", "submit", "entry", "custom");
    s.run_until("the board is a custom vendor's", |s| {
        s.server
            .world()
            .get::<VendorBoard>(board)
            .is_some_and(|b| b.refill == "/obj/item/vending_refill/custom")
    });
    assert_eq!(
        s.server
            .world()
            .get::<VendorBoard>(board)
            .unwrap()
            .build_path,
        "/obj/machinery/vending/custom"
    );
}

/// The custom vendor: no window without an owner; an ID swipe links the account; the owner
/// loads priced toys and takes one for free; a buyer pays the owner; the crowbar while linked
/// asks, and "Yes" self-destructs it.
#[test]
fn custom_vendor_owner_and_buyer() {
    let mut s = Station::start("custom_vendor_owner_and_buyer");
    let custom = s.vendor(CUSTOM);
    let owner = s.account;
    let mob = s.mob;
    s.replicated(custom);
    s.st.click(0, custom);
    s.run_until("no owner", |s| s.said("нет зарегистрированного владельца"));
    assert!(s.window().id.is_none());

    // The ID from its slot into the hand, swiped, back.
    let card = s.card();
    let move_card = |s: &mut Station, to_hand: bool| {
        let world = s.st.server.world_mut();
        let mut inventory = world.get::<Inventory>(mob).unwrap().clone();
        let active = inventory.active;
        if to_hand {
            inventory.id = None;
            inventory.hands[active] = Some(card);
        } else {
            inventory.id = Some(card);
            inventory.hands[active] = None;
        }
        world.entity_mut(mob).insert(inventory);
    };
    move_card(&mut s, true);
    s.click(custom);
    s.run_until("linked", |s| {
        s.server.world().get::<CustomVendor>(custom).unwrap().linked == Some(owner)
    });
    move_card(&mut s, false);

    // Loading: priced toys go in, one row.
    let toy = s.hold_item(TOY);
    s.click(custom);
    s.run_until("the toy is in", |s| s.stock(custom, 0) == 1);
    let toy2 = s.hold_item(TOY);
    s.click(custom);
    s.run_until("two toys, one row", |s| s.stock(custom, 0) == 2);
    assert_eq!(s.machine(custom).records.len(), 1);

    // The owner's window: one row, free.
    s.open(custom);
    let st = s.window().static_data.clone().unwrap();
    assert_eq!(st.product_records.len(), 1);
    assert_eq!(
        (
            st.product_records[0].name.as_str(),
            st.product_records[0].price
        ),
        ("toy", 15)
    );
    assert!(s.window().data.as_ref().unwrap().stock[0].free);
    s.vend("0");
    s.run_until("the owner takes a toy", |s| s.stock(custom, 0) == 1);
    assert_eq!(s.balance(owner), 100);
    assert_eq!(s.inventory().hands[0], Some(toy2));

    // A buyer: the same card on another account pays the owner.
    let buyer = s.open_account("buyer", "CIV", 40);
    s.st.server
        .world_mut()
        .entity_mut(card)
        .insert(BankAccountId(buyer));
    s.send(ClientIntent::SwapHands);
    s.run_until("the right hand is active", |s| s.inventory().active == 1);
    s.vend("0");
    s.run_until("the buyer gets the toy", |s| {
        s.inventory().hands[1] == Some(toy)
    });
    assert_eq!((s.balance(buyer), s.balance(owner)), (25, 115));
    s.run_until("sold out: the row is gone", |s| {
        s.window()
            .static_data
            .as_ref()
            .is_some_and(|d| d.product_records.is_empty())
    });

    // The crowbar while linked: a warning and an alert; "Yes" self-destructs it, stock and
    // canister lost.
    s.st.server
        .world_mut()
        .entity_mut(custom)
        .insert(PanelOpen(true));
    s.send(ClientIntent::SwapHands);
    s.run_until("the left hand is active", |s| s.inventory().active == 0);
    s.st.server
        .world_mut()
        .get_mut::<Inventory>(mob)
        .unwrap()
        .hands[0] = None;
    s.hold_item(CROWBAR);
    s.click(custom);
    s.run_until("warned", |s| s.said("Несанкционированная разборка"));
    assert!(
        s.server.world().get_entity(custom).is_ok(),
        "not before the answer"
    );
    s.answer("AlertModal", "choose", "choice", "Yes");
    s.run_until("self-destructed", |s| {
        s.server.world().get_entity(custom).is_err()
    });
    assert!(
        s.find_on(CUSTOM, "/obj/item/vending_refill/custom")
            .is_none(),
        "the canister went with it"
    );
}

/// `process`: once `slogan_delay` passed, a powered vendor says one of its slogans.
#[test]
fn slogans() {
    let mut s = Station::start("slogans");
    let vendor = s.vendor(VENDOR);
    let now = s.st.now();
    s.st.server
        .world_mut()
        .get_mut::<Vendor>(vendor)
        .unwrap()
        .last_slogan = now - 1000.0;
    s.run_until_for(300.0, "a slogan", |s| s.said("Slogan!"));
}

/// Every vendor with a canister on MetaStation: taken apart, its canister (named after the
/// vendor) holds its stock, and the board and canister build it back with the same stock.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_vendors_come_apart_and_back() {
    use ss13_game::vending::refill::installed_canister;
    use ss13_mapload::{DmName, DmObj};

    let map = ss13_mapload::MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..ss13_mapload::MapLoadPlugin::from_env().unwrap_or(ss13_mapload::MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    };
    let mut server = testing::server_app(map, |_| {});
    let world = server.world_mut();
    let mut q = world.query::<(Entity, &Vendor, &DmObj, &TilePos)>();
    let vendors: Vec<(Entity, String, TilePos, Vec<u32>)> = q
        .iter(world)
        .filter(|(_, v, ..)| !v.refill_canister.is_empty())
        .map(|(e, v, o, p)| {
            (
                e,
                o.path.clone(),
                *p,
                v.records.iter().map(|r| r.amount).collect(),
            )
        })
        .collect();
    assert!(
        vendors.len() > 20,
        "{} vendors with canisters",
        vendors.len()
    );
    let mut done = 0;
    for (vendor, path, pos, stock) in vendors {
        if path.starts_with(ss13_game::vending::custom::CUSTOM_ROOT) {
            continue;
        }
        assert!(
            ss13_game::stock_parts::ensure_parts(world, vendor),
            "{path}"
        );
        let canister = installed_canister(world, vendor).unwrap();
        let name = world.get::<DmName>(canister).unwrap().name.clone();
        assert!(name.ends_with("restocking unit"), "{path}: {name}");
        let parts = world
            .get::<ss13_game::stock_parts::ComponentParts>(vendor)
            .unwrap()
            .parts()
            .to_vec();
        ss13_game::obj_damage::deconstruct(world, vendor, true);
        assert!(world.get_entity(vendor).is_err(), "{path}");
        let built = ss13_game::machine_frame::build_machine(
            world,
            pos,
            VENDOR_BOARD.into(),
            parts,
            true,
            None,
        )
        .unwrap_or_else(|| panic!("{path} builds"));
        assert_eq!(world.get::<DmObj>(built).unwrap().path, path);
        let again: Vec<u32> = world
            .get::<Vendor>(built)
            .unwrap()
            .records
            .iter()
            .map(|r| r.amount)
            .collect();
        assert_eq!(again, stock, "{path}");
        done += 1;
    }
    println!("{done} vendors taken apart and built back");
}
