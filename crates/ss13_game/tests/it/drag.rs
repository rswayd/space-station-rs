//! Drags, alt-clicks and middle clicks over real WebTransport on 127.0.0.1 (fixture station, two
//! headless clients): the strip menu takes a cuffed mob's handcuffs off and puts an item into
//! its hand, each after tg's delay; a backpack dragged from the back slot onto the floor dumps
//! after 2 s; a mob dragged onto an open closet is stuffed in after 4 s and gets out by
//! stepping; an alt-click opens the loot panel of a tile, whose entry picks the item up; a
//! shift+middle click points, a middle click swaps hands.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::closets::{Closet, InCloset};
use ss13_game::inventory::Inventory;
use ss13_game::panels::{LootPanelData, LootPanelUi, StripMenuData, StripMenuUi};
use ss13_game::point::PointArrow;
use ss13_game::restraints::Handcuffed;
use ss13_game::server::MoveQueue;
use ss13_game::status_effects::StatusEffects;
use ss13_game::storage::Stored;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_mapload::DmItem;
use ss13_net::chat::client::ChatReceived;
use ss13_net::server::Interest;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd, InventorySlot, NetId};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Appearance, Dir, TilePos};
use ss13_tgui::UiInterface;
use ss13_world::SpatialIndex;

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
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3, "breakouttime": 0,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": { "w_class": 1 } },
    "/obj/item/pen": { "parent": "/obj/item", "declared_vars": {
        "name": "pen", "icon_state": "pen", "w_class": 1 } },
    "/obj/item/restraints": { "parent": "/obj/item", "declared_vars": { "breakouttime": 600 } },
    "/obj/item/restraints/handcuffs": { "parent": "/obj/item/restraints", "declared_vars": {
        "name": "handcuffs", "icon_state": "handcuff", "breakouttime": 600, "handcuff_time": 40,
        "used": 0 } },
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
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/backpack": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "backpack", "icon_state": "backpack", "slot_flags": 1, "w_class": 4,
        "storage_type": { "path": "/datum/storage/backpack" } } },
    "/obj/item/storage/box": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "box", "icon_state": "box", "w_class": 2,
        "storage_type": { "path": "/datum/storage/box" } } },
    "/obj/item/storage/box/survival": { "parent": "/obj/item/storage/box" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "name": "closet", "icon": { "resource": "c.dmi" }, "icon_state": "generic",
        "density": 1, "opened": 0, "locked": 0, "welded": 0, "secure": 0, "icon_door": null,
        "icon_welded": "welded", "enable_door_overlay": 1, "has_opened_overlay": 1,
        "has_closed_overlay": 1, "dense_when_open": 0, "wall_mounted": 0, "horizontal": 0,
        "storage_capacity": 30 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": {
        "name": "floor", "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  },
  "populate": {
    "/obj/structure/closet": { "PopulateContents": ["return"] },
    "/obj/item/storage/box/survival": { "PopulateContents": [
        { "repeat": { "count": 2, "body": [{ "new": "/obj/item/toy" }] } }] }
  }
}"##;

/// Rows top-down, the last line is y = 1. A spawns at (3,3), B at (3,4); an open closet (4,4),
/// a pen (4,3), handcuffs (2,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"O" = (/obj/structure/closet{opened = 1},/turf/open/floor,/area/space)
"p" = (/obj/item/pen,/turf/open/floor,/area/space)
"c" = (/obj/item/restraints/handcuffs,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffffffw
wfsOfffw
wfspfffw
wcfffffw
wwwwwwww
"}
"##;

const A_SPAWN: TilePos = TilePos::new(3, 3, 1);
const B_SPAWN: TilePos = TilePos::new(3, 4, 1);
const CLOSET: TilePos = TilePos::new(4, 4, 1);
const PEN: TilePos = TilePos::new(4, 3, 1);
const CUFFS: TilePos = TilePos::new(2, 2, 1);

const STATES: [(&str, &str); 13] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "pen"),
    ("i.dmi", "handcuff"),
    ("i.dmi", "backpack"),
    ("i.dmi", "box"),
    ("c.dmi", "generic"),
    ("c.dmi", "generic_open"),
    ("c.dmi", "generic_door"),
    ("icons/hud/screen_gen.dmi", "arrow"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("drag_fixture")
        .join(test);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/closet",
        "/obj/item/pen",
        "/obj/item/toy",
        "/obj/item/restraints/handcuffs",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    used.extend(ss13_mapload::RUNTIME_CONTAINER_TYPES);
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// Every open window of a client (interface, last data) and its chat.
#[derive(Resource, Default)]
struct Received {
    windows: HashMap<u32, (String, Vec<u8>)>,
    chat: Vec<String>,
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
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } => {
                received
                    .windows
                    .insert(*window, (interface.clone(), data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some(w) = received.windows.get_mut(window) {
                    w.1 = data.clone();
                }
            }
            UiServerMessage::Close { window } => {
                received.windows.remove(window);
            }
            _ => {}
        }
    }
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str, clients: usize) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(clients, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    fn get<T: Component>(&self, e: Entity) -> Option<&T> {
        self.server.world().get::<T>(e)
    }

    fn net_id(&mut self, p: usize, entity: Entity) -> NetId {
        let link = self.players[p].link;
        self.run_until("replicated to the player", |s| {
            s.server
                .world()
                .resource::<Interest>()
                .net_id(link, entity)
                .is_some()
        });
        self.server
            .world()
            .resource::<Interest>()
            .net_id(link, entity)
            .unwrap()
    }

    fn click_with(&mut self, p: usize, entity: Entity, modifiers: ClickModifiers) {
        let target = self.net_id(p, entity);
        self.send(p, ClientIntent::Click { target, modifiers });
    }

    fn drag(&mut self, p: usize, from: DragEnd, to: DragEnd) {
        self.send(
            p,
            ClientIntent::Drag {
                from,
                to,
                modifiers: ClickModifiers::default(),
            },
        );
    }

    fn atom(&mut self, p: usize, entity: Entity) -> DragEnd {
        DragEnd::Atom(self.net_id(p, entity))
    }

    fn step_once(&mut self, p: usize, dir: Dir) {
        let mob = self.players[p].mob;
        self.players[p].seq += 1;
        let seq = self.players[p].seq;
        self.send(p, ClientIntent::Move { dir, seq });
        self.run_until("the step is applied", |s| {
            s.get::<MoveQueue>(mob).unwrap().applied() == seq
        });
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

    fn turf(&self, pos: TilePos) -> Entity {
        ss13_game::construction::turf_at(self.server.world(), pos).unwrap()
    }

    fn inventory(&self, e: Entity) -> Inventory {
        self.get::<Inventory>(e).unwrap().clone()
    }

    /// The newest window of `interface` on `p`'s client: its id and data.
    fn window<T: UiInterface>(&self, p: usize) -> Option<(u32, T::Data)> {
        let received = self.players[p].app.world().resource::<Received>();
        received
            .windows
            .iter()
            .filter(|(_, (name, _))| name == T::NAME)
            .max_by_key(|(id, _)| **id)
            .map(|(id, (_, data))| (*id, ss13_tgui::decode(data).unwrap()))
    }

    fn said(&self, p: usize, text: &str) -> bool {
        self.players[p]
            .app
            .world()
            .resource::<Received>()
            .chat
            .iter()
            .any(|line| line.contains(text))
    }

    fn act(&mut self, p: usize, window: u32, action: &str, params: &[(&str, String)]) {
        let params = params
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        self.send(
            p,
            ClientIntent::UiAct {
                window,
                action: action.into(),
                params,
            },
        );
    }
}

#[test]
fn strip_menu_uncuffs_and_puts_an_item_on() {
    let mut s = Station::start("strip_menu_uncuffs_and_puts_an_item_on", 2);
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert_eq!(s.get::<TilePos>(a), Some(&A_SPAWN));
    assert_eq!(s.get::<TilePos>(b), Some(&B_SPAWN));
    let cuffs = s.on(CUFFS, |e| e.contains::<DmItem>()).unwrap();
    ss13_game::restraints::apply_cuffs(s.server.world_mut(), cuffs, b);

    // A drags B onto itself: the strip menu, with B's cuffs and backpack.
    let (from, to) = (s.atom(0, b), s.atom(0, a));
    s.drag(0, from, to);
    s.run_until("the strip menu opens", |s| {
        s.window::<StripMenuUi>(0).is_some()
    });
    let (window, data): (u32, StripMenuData) = s.window::<StripMenuUi>(0).unwrap();
    let slot = |data: &StripMenuData, key: &str| {
        data.items
            .iter()
            .find(|i| i.key == key)
            .map(|i| i.item.as_ref().map(|i| i.name.clone()))
    };
    assert_eq!(slot(&data, "handcuffs"), Some(Some("handcuffs".into())));
    assert_eq!(slot(&data, "back"), Some(Some("backpack".into())));
    assert_eq!(slot(&data, "left_hand"), Some(None));

    // `use` on the handcuffs: 4 s (`strip_delay`), then they drop at B's feet.
    s.act(0, window, "use", &[("key", "handcuffs".into())]);
    s.run_until("stripping starts", |s| s.get::<DoAfter>(a).is_some());
    let started = s.now();
    s.run_until("A's menu shows the slot in use", |s| {
        s.window::<StripMenuUi>(0).is_some_and(|(_, d)| {
            d.items
                .iter()
                .any(|i| i.key == "handcuffs" && i.interacting)
        })
    });
    s.run_until("B reads the attempt", |s| {
        s.said(1, "пытается снять ваш handcuffs")
    });
    s.run_until("B is uncuffed", |s| s.get::<Handcuffed>(b).is_none());
    assert!(s.now() - started >= 3.9, "tg strip_delay is 4 s");
    assert_eq!(s.get::<TilePos>(cuffs), Some(&B_SPAWN));
    s.run_until("the menu drops the handcuffs entry", |s| {
        s.window::<StripMenuUi>(0)
            .is_some_and(|(_, d)| d.items.iter().all(|i| i.key != "handcuffs"))
    });

    // A puts the pen into B's empty left hand: 2 s (`equip_delay_other`).
    let pen = s.on(PEN, |e| e.contains::<DmItem>()).unwrap();
    s.click_with(0, pen, ClickModifiers::default());
    s.run_until("A holds the pen", |s| {
        s.inventory(a).active_item() == Some(pen)
    });
    s.act(0, window, "use", &[("key", "left_hand".into())]);
    s.run_until("equipping starts", |s| s.get::<DoAfter>(a).is_some());
    let started = s.now();
    s.run_until("B holds the pen", |s| s.inventory(b).hands[0] == Some(pen));
    assert!(s.now() - started >= 1.9, "tg equip_delay_other is 2 s");
    assert!(s.inventory(a).active_item().is_none());

    // B steps away: A's menu closes (`ui_status_user_is_adjacent`).
    s.step_once(1, Dir::North);
    s.run_until("the strip menu closes", |s| {
        s.window::<StripMenuUi>(0).is_none()
    });

    // Dragging oneself onto oneself opens nothing.
    let me = s.atom(1, b);
    s.drag(1, me, me);
    s.hold(0.3, "no strip menu for oneself", |s| {
        s.window::<StripMenuUi>(1).is_none()
    });
}

#[test]
fn backpack_dumps_and_mob_goes_into_closet() {
    let mut s = Station::start("backpack_dumps_and_mob_goes_into_closet", 2);
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    let pack = s.inventory(a).back.expect("players spawn with a backpack");
    let stored = s
        .get::<Stored>(pack)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    assert_eq!(stored.len(), 1, "the survival box");

    // The backpack from the back slot onto the floor west of A: a 2 s do_after, then its
    // contents lie there.
    let floor = TilePos::new(2, 3, 1);
    let to = s.atom(0, s.turf(floor));
    s.drag(0, DragEnd::Slot(InventorySlot::Back), to);
    s.run_until("dumping starts", |s| s.get::<DoAfter>(a).is_some());
    let started = s.now();
    s.run_until("A reads it", |s| s.said(0, "Вы начинаете вытряхивать"));
    s.run_until("the box lies on the floor", |s| {
        s.get::<TilePos>(stored[0]) == Some(&floor)
    });
    assert!(s.now() - started >= 1.9, "tg dumps onto a tile after 2 s");
    assert!(s.get::<Stored>(pack).is_none_or(|s| s.items().is_empty()));
    assert_eq!(s.inventory(a).back, Some(pack), "the backpack stays worn");

    // The backpack from the back slot onto the empty left hand goes into it.
    s.drag(
        0,
        DragEnd::Slot(InventorySlot::Back),
        DragEnd::Slot(InventorySlot::LeftHand),
    );
    s.run_until("the backpack moves to the hand", |s| {
        s.inventory(a).hands[0] == Some(pack) && s.inventory(a).back.is_none()
    });
    // The box from the floor onto the held backpack: its contents go into the backpack at
    // once (storage into storage).
    let toys = s.get::<Stored>(stored[0]).unwrap().items().to_vec();
    assert_eq!(toys.len(), 2);
    let boxed = s.atom(0, stored[0]);
    s.drag(0, boxed, DragEnd::Slot(InventorySlot::LeftHand));
    s.run_until("the toys are in the backpack", |s| {
        s.get::<Stored>(pack).is_some_and(|s| s.items() == toys)
    });
    assert_eq!(s.get::<TilePos>(stored[0]), Some(&floor), "the box stays");

    // The held backpack onto the other hand's item: after 2 s the toys lie under A (an item in
    // the hands dumps onto the user's tile).
    let junk = s
        .server
        .world_mut()
        .spawn(ss13_game::inventory::CarriedBy(a))
        .id();
    s.server.world_mut().get_mut::<Inventory>(a).unwrap().hands[1] = Some(junk);
    s.drag(
        0,
        DragEnd::Slot(InventorySlot::LeftHand),
        DragEnd::Slot(InventorySlot::RightHand),
    );
    s.run_until("dumping onto the held item starts", |s| {
        s.get::<DoAfter>(a).is_some()
    });
    s.run_until("the toys lie under A", |s| {
        toys.iter().all(|&t| s.get::<TilePos>(t) == Some(&A_SPAWN))
    });
    s.server.world_mut().get_mut::<Inventory>(a).unwrap().hands[1] = None;
    s.server.world_mut().despawn(junk);

    // A stuffs B into the open closet: 4 s, then B is inside, hidden and paralyzed, and the
    // closet is closed.
    let closet = s.on(CLOSET, |e| e.contains::<Closet>()).unwrap();
    assert!(s.get::<Closet>(closet).unwrap().opened);
    let (from, to) = (s.atom(0, b), s.atom(0, closet));
    s.drag(0, from, to);
    s.run_until("stuffing starts", |s| s.get::<DoAfter>(a).is_some());
    let started = s.now();
    s.run_until("B is in the closet", |s| s.get::<InCloset>(b).is_some());
    assert!(s.now() - started >= 3.9, "tg stuffs after 4 s");
    assert!(!s.get::<Closet>(closet).unwrap().opened);
    assert_eq!(s.get::<TilePos>(b), Some(&CLOSET));
    assert!(s.get::<StatusEffects>(b).unwrap().paralyze > 3.0);
    s.run_until("B is drawn transparent", |s| {
        s.get::<Appearance>(b)
            .is_some_and(|a| a.color.is_some_and(|c| c[3] == 0))
    });

    // B inside is no target for A (tg `CanReach`: the closet is its `loc`), whatever ref a
    // modified client sends: no strip menu through the closet, no point revealing B.
    let (from, to) = (s.atom(0, b), s.atom(0, a));
    s.drag(0, from, to);
    s.click_with(
        0,
        b,
        ClickModifiers {
            shift: true,
            middle: true,
            ..default()
        },
    );
    s.hold(0.5, "B inside is out of reach", |s| {
        s.window::<StripMenuUi>(0).is_none()
            && !s.said(1, "указывает на")
            && s.on(CLOSET, |e| e.contains::<PointArrow>()).is_none()
    });

    // B steps once paralysis is over: the unlocked closet opens and B shows again.
    s.run_until("B can move", |s| {
        s.get::<StatusEffects>(b).is_none_or(|e| e.paralyze <= 0.0)
    });
    s.step_once(1, Dir::East);
    s.run_until("the closet opens", |s| {
        s.get::<Closet>(closet).unwrap().opened
    });
    assert!(s.get::<InCloset>(b).is_none());
    assert_eq!(s.get::<TilePos>(b), Some(&CLOSET));
    assert!(
        s.get::<Appearance>(b)
            .unwrap()
            .color
            .is_none_or(|c| c[3] > 0)
    );

    // The closet is pulled away during a second stuffing: B stays on the closet's first tile
    // (tg `forceMove(T)` of the turf taken at the start) and the closet closes empty.
    let (from, to) = (s.atom(0, b), s.atom(0, closet));
    s.drag(0, from, to);
    s.run_until("stuffing starts again", |s| s.get::<DoAfter>(a).is_some());
    let away = TilePos::new(5, 4, 1);
    s.server.world_mut().entity_mut(closet).insert(away);
    s.run_until("the stuffing ends", |s| s.get::<DoAfter>(a).is_none());
    s.step();
    assert!(s.get::<InCloset>(b).is_none());
    assert_eq!(s.get::<TilePos>(b), Some(&CLOSET));
    assert!(!s.get::<Closet>(closet).unwrap().opened);
}

#[test]
fn alt_click_lists_a_tile_and_middle_clicks() {
    let mut s = Station::start("alt_click_lists_a_tile_and_middle_clicks", 2);
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    let pen = s.on(PEN, |e| e.contains::<DmItem>()).unwrap();

    // Alt-click on the pen's floor: the loot panel of that tile, the floor first.
    let floor = s.turf(PEN);
    let alt = ClickModifiers {
        alt: true,
        ..default()
    };
    s.click_with(0, floor, alt);
    s.run_until("the loot panel opens", |s| {
        s.window::<LootPanelUi>(0).is_some()
    });
    let (window, data): (u32, LootPanelData) = s.window::<LootPanelUi>(0).unwrap();
    let names: Vec<&str> = data.contents.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["floor", "pen"]);
    // Its entry clicks the pen: A picks it up, and the panel follows.
    let key = data.contents[1].key.to_string();
    s.act(0, window, "grab", &[("ref", key)]);
    s.run_until("A holds the pen", |s| {
        s.inventory(a).active_item() == Some(pen)
    });
    s.run_until("the panel lists only the floor", |s| {
        s.window::<LootPanelUi>(0)
            .is_some_and(|(_, d)| d.contents.len() == 1)
    });
    // Alt-click on another tile: the first panel closes, one panel per client.
    let other = s.turf(TilePos::new(2, 3, 1));
    s.click_with(0, other, alt);
    s.run_until("the panel moves to the other tile", |s| {
        s.players[0]
            .app
            .world()
            .resource::<Received>()
            .windows
            .values()
            .filter(|(name, _)| name == LootPanelUi::NAME)
            .count()
            == 1
            && s.window::<LootPanelUi>(0)
                .is_some_and(|(id, _)| id != window)
    });

    // Middle click: the other hand is active.
    let active = s.inventory(a).active;
    let middle = ClickModifiers {
        middle: true,
        ..default()
    };
    s.click_with(0, floor, middle);
    s.run_until("the hands swap", |s| s.inventory(a).active != active);

    // Shift+middle click on B: an arrow over B for 2.5 s, B reads it.
    let point = ClickModifiers {
        shift: true,
        middle: true,
        ..default()
    };
    s.click_with(0, b, point);
    s.run_until("B sees A point", |s| s.said(1, "указывает на"));
    s.run_until("A reads it", |s| s.said(0, "Вы указываете на"));
    let arrow = s.on(B_SPAWN, |e| e.contains::<PointArrow>()).unwrap();
    let shown = s.now();
    // The 1 s cooldown: a second point at once is ignored.
    s.click_with(0, b, point);
    s.hold(0.3, "one arrow during the cooldown", |s| {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(B_SPAWN)
            .iter()
            .filter(|&&e| world.get::<PointArrow>(e).is_some())
            .count()
            == 1
    });
    s.run_until("the arrow goes", |s| {
        s.server.world().get_entity(arrow).is_err()
    });
    assert!(s.now() - shown >= 2.4, "POINT_TIME is 2.5 s");
}
