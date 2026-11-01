//! Security gear over real WebTransport on 127.0.0.1: officer A batons B into stamina crit (B
//! lies and cannot move), cuffs B (B cannot pick things up), drags the cuffed B, takes the cuffs
//! off; B steps away from a second cuffing, is cuffed with tg's fake handcuffs and resists out
//! after their `breakouttime`. A flash knocks down a mob facing it, not one in sunglasses; a
//! shove pushes a mob away.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::{CombatRng, NextMove};
use ss13_game::health::BodyPosition;
use ss13_game::hud::{HealthHudData, HealthHudUi};
use ss13_game::inventory::Inventory;
use ss13_game::pull::Pulling;
use ss13_game::restraints::Handcuffed;
use ss13_game::security::Baton;
use ss13_game::server::MoveQueue;
use ss13_game::status_effects::{Incapacitation, StatusEffects};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_mapload::DmItem;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::UiInterface;
use ss13_world::SpatialIndex;

/// tg values of the gear (`baton.dm`, `handcuffs.dm`, `cell.dm`, `glasses.dm`).
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
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "breakouttime": 0,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/melee": { "parent": "/obj/item" },
    "/obj/item/melee/baton": { "parent": "/obj/item/melee", "declared_vars": {
        "name": "police baton", "icon_state": "classic_baton", "force": 12, "active": 1,
        "cooldown": 40, "knockdown_time": 15, "stamina_damage": 55, "stun_on_harmbaton": 0 } },
    "/obj/item/melee/baton/security": { "parent": "/obj/item/melee/baton", "declared_vars": {
        "name": "stun baton", "icon_state": "stunbaton", "inhand_icon_state": "stunbaton",
        "force": 10, "active": 0, "cooldown": 25, "knockdown_time": 50, "stamina_damage": 60,
        "stun_on_harmbaton": 1, "cell_hit_cost": 10000, "preload_cell_type": null } },
    "/obj/item/melee/baton/security/loaded": { "parent": "/obj/item/melee/baton/security",
        "declared_vars": { "preload_cell_type": {
            "path": "/obj/item/stock_parts/power_store/cell/high" } } },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item/stock_parts" },
    "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item/stock_parts/power_store",
        "declared_vars": { "maxcharge": 10000 } },
    "/obj/item/stock_parts/power_store/cell/high": {
        "parent": "/obj/item/stock_parts/power_store/cell", "declared_vars": { "maxcharge": 100000 } },
    "/obj/item/restraints": { "parent": "/obj/item", "declared_vars": { "breakouttime": 600 } },
    "/obj/item/restraints/handcuffs": { "parent": "/obj/item/restraints", "declared_vars": {
        "name": "handcuffs", "icon_state": "handcuff", "breakouttime": 600, "handcuff_time": 40,
        "used": 0 } },
    "/obj/item/restraints/handcuffs/fake": { "parent": "/obj/item/restraints/handcuffs",
        "declared_vars": { "name": "fake handcuffs", "breakouttime": 10 } },
    "/obj/item/assembly": { "parent": "/obj/item" },
    "/obj/item/assembly/flash": { "parent": "/obj/item/assembly", "declared_vars": {
        "name": "flash", "icon_state": "flash" } },
    "/obj/item/assembly/flash/handheld": { "parent": "/obj/item/assembly/flash" },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item", "declared_vars": { "flash_protect": 0 } },
    "/obj/item/clothing/glasses": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 8 } },
    "/obj/item/clothing/glasses/sunglasses": { "parent": "/obj/item/clothing/glasses",
        "declared_vars": { "name": "sunglasses", "icon_state": "sun", "flash_protect": 1 } },
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

const PREFABS: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"b" = (/obj/item/melee/baton/security/loaded,/turf/open/floor,/area/space)
"c" = (/obj/item/restraints/handcuffs,/turf/open/floor,/area/space)
"k" = (/obj/item/restraints/handcuffs/fake,/turf/open/floor,/area/space)
"l" = (/obj/item/assembly/flash/handheld,/turf/open/floor,/area/space)
"g" = (/obj/item/clothing/glasses/sunglasses,/turf/open/floor,/area/space)
"##;

/// Rows top-down, the last line is y = 1: A at (3,3), B at (3,4); a loaded stun baton (4,3),
/// handcuffs (2,3), fake handcuffs (5,3).
const CUFFS_MAP: &str = r##"
(1,1,1) = {"
wwwwwwww
wffffffw
wfsffffw
wcsbkffw
wffffffw
wwwwwwww
"}
"##;

/// Players at (3,3), (3,4), (4,4); a flash (2,3), sunglasses (5,4).
const FLASH_MAP: &str = r##"
(1,1,1) = {"
wwwwwwww
wffffffw
wfssgffw
wlsffffw
wffffffw
wwwwwwww
"}
"##;

const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "stunbaton"),
    ("i.dmi", "stunbaton_active"),
    ("i.dmi", "handcuff"),
    ("i.dmi", "flash"),
    ("i.dmi", "sun"),
    ("icons/mob/simple/mob.dmi", "handcuff1"),
];

fn fixture_assets(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("security_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/melee/baton/security/loaded",
        "/obj/item/restraints/handcuffs",
        "/obj/item/restraints/handcuffs/fake",
        "/obj/item/assembly/flash/handheld",
        "/obj/item/clothing/glasses/sunglasses",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let map = format!("{PREFABS}{map}");
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", &map)])
}

#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    hud: Option<HealthHudData>,
    hud_window: Option<u32>,
    strip_window: Option<u32>,
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
        if let UiServerMessage::Open {
            window, interface, ..
        } = message
            && interface == ss13_game::panels::StripMenuUi::NAME
        {
            received.strip_window = Some(*window);
        }
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == HealthHudUi::NAME => {
                received.hud_window = Some(*window);
                data
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if received.hud_window == Some(*window) => data,
            _ => continue,
        };
        if !data.is_empty() {
            received.hud = Some(ss13_tgui::decode(data).unwrap());
        }
    }
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str, map: &str, clients: usize) -> Self {
        let assets = fixture_assets(test, map);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        // Fixed rolls: the flash's burnout chance and the lying side (after `finish`, which
        // seeds the rngs from the map seed).
        s.server.insert_resource(CombatRng(7));
        s.join_all(clients, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    fn pos_of(&self, e: Entity) -> TilePos {
        *self.get::<TilePos>(e)
    }

    fn traits(&self, e: Entity) -> Incapacitation {
        *self.get::<Incapacitation>(e)
    }

    fn effects(&self, e: Entity) -> StatusEffects {
        self.try_get::<StatusEffects>(e)
            .cloned()
            .unwrap_or_default()
    }

    fn held(&self, e: Entity) -> Option<Entity> {
        self.get::<Inventory>(e).active_item()
    }

    fn lying(&self, e: Entity) -> bool {
        self.get::<BodyPosition>(e).lying()
    }

    /// Steps `p` once and waits until the server applied it.
    fn step_once(&mut self, p: usize, dir: Dir) {
        self.try_step(p, dir);
        let (mob, seq) = (self.players[p].mob, self.players[p].seq);
        self.run_until("the step is applied", |s| {
            s.get::<MoveQueue>(mob).applied() == seq
        });
    }

    /// A click once `entity` is replicated to `p`.
    fn click_with(&mut self, p: usize, entity: Entity, modifiers: ClickModifiers) {
        self.run_until("replicated to the clicker", |s| {
            s.net_id(p, entity).is_some()
        });
        self.0.click_with(p, entity, modifiers);
    }

    /// A left click once `p` is off its click cooldown.
    fn click(&mut self, p: usize, entity: Entity) {
        let mob = self.players[p].mob;
        self.run_until("off the click cooldown", |s| {
            s.now() >= s.get::<NextMove>(mob).0
        });
        self.click_with(p, entity, ClickModifiers::default());
    }

    fn item_at(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some())
            .unwrap_or_else(|| panic!("no item on {pos:?}"))
    }

    fn pick_up(&mut self, p: usize, item: Entity) {
        let mob = self.players[p].mob;
        self.click(p, item);
        self.run_until("picked up", |s| s.held(mob) == Some(item));
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

    fn hud(&self, p: usize) -> Option<HealthHudData> {
        self.players[p]
            .app
            .world()
            .resource::<Received>()
            .hud
            .clone()
    }
}

#[test]
fn baton_cuff_drag_strip_and_resist() {
    let mut s = Station::start("baton_cuff_drag_strip_and_resist", CUFFS_MAP, 2);
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert_eq!(
        (s.pos_of(a), s.pos_of(b)),
        (TilePos::new(3, 3, 1), TilePos::new(3, 4, 1))
    );

    // A loaded stun baton spawns off with a full high-capacity cell (10 hits).
    let baton = s.item_at(TilePos::new(4, 3, 1));
    let state = s.get::<Baton>(baton).clone();
    assert!(!state.active);
    assert_eq!(state.charge, Some(100_000.0));
    s.pick_up(0, baton);
    s.send(0, ClientIntent::ClickSlot(InventorySlot::LeftHand));
    s.run_until("the baton turns on", |s| s.get::<Baton>(baton).active);
    assert_eq!(s.get::<DmItem>(baton).inhand_icon_state, "stunbaton_active");

    // First hit, out of combat mode: 60 stamina, no damage; the knockdown follows 2 s later.
    s.click(0, b);
    s.run_until("B takes stamina damage", |s| s.effects(b).stamina == 60.0);
    assert_eq!(s.get::<Baton>(baton).charge, Some(90_000.0));
    assert!(!s.lying(b), "the stun baton knocks down only after 2 s");
    assert_eq!(s.get::<ss13_game::health::Health>(b).brute, 0.0);
    s.run_until("B reads the stun", |s| s.said(1, "оглушает вас с помощью"));
    s.run_until_for(5.0, "B falls 2 s later", |s| s.lying(b));
    assert!(s.effects(b).knockdown > 4.0);

    // The baton's cooldown (2.5 s) refuses a stun; after it the second hit stamcrits B.
    let ready = s.get::<Baton>(baton).ready_at;
    s.run_until("the baton is ready", |s| s.now() >= ready);
    s.click(0, b);
    s.run_until("B is in stamina crit", |s| s.effects(b).stamcrit);
    assert_eq!(s.effects(b).stamina, 120.0);
    s.run_until("B's traits follow", |s| {
        let t = s.traits(b);
        t.incapacitated && t.immobilized && t.floored
    });
    s.run_until("B's HUD says it cannot move", |s| {
        s.hud(1).is_some_and(|h| h.immobile)
    });
    s.step_once(1, Dir::North);
    s.hold(0.3, "B cannot crawl away in stamcrit", |s| {
        s.pos_of(b) == TilePos::new(3, 4, 1)
    });
    let stamcrit_at = s.now();

    // A cuffs B: 4 s of do_after, then the cuffs leave A's hand onto B.
    s.send(0, ClientIntent::DropActive);
    s.run_until("baton dropped", |s| s.held(a).is_none());
    let cuffs = s.item_at(TilePos::new(2, 3, 1));
    s.pick_up(0, cuffs);
    s.click(0, b);
    s.run_until("cuffing starts", |s| s.try_get::<DoAfter>(a).is_some());
    let started = s.now();
    s.run_until("B reads the attempt", |s| {
        s.said(1, "пытается надеть handcuffs на вас")
    });
    s.run_until_for(10.0, "B is cuffed", |s| {
        s.try_get::<Handcuffed>(b).is_some()
    });
    assert!(s.now() - started >= 3.9, "tg handcuff_time is 4 s");
    assert_eq!(s.get::<Handcuffed>(b).0, cuffs);
    assert!(s.held(a).is_none(), "the cuffs left A's hand");
    assert!(s.traits(b).restrained && s.traits(b).hands_blocked);

    // Out of stamcrit (10 s after the last hit), B still cannot use its hands.
    s.run_until_for(15.0, "B's stamina resets", |s| !s.effects(b).stamcrit);
    assert!(s.now() - stamcrit_at >= 9.0);
    assert!(!s.traits(b).incapacitated);
    let on_floor = baton;
    s.click(1, on_floor);
    s.hold(0.5, "a cuffed B picks nothing up", |s| s.held(b).is_none());

    // A drags the cuffed B: B's own steps are refused, it follows A.
    s.click_with(
        0,
        b,
        ClickModifiers {
            ctrl: true,
            ..default()
        },
    );
    s.run_until("A pulls B", |s| {
        s.try_get::<Pulling>(a).map(|p| p.0) == Some(b)
    });
    s.step_once(1, Dir::North);
    s.run_until("B reads that it is restrained", |s| {
        s.said(1, "Вы сдержаны! Вы не можете двигаться!")
    });
    assert_eq!(s.pos_of(b), TilePos::new(3, 4, 1));
    s.step_once(0, Dir::East);
    s.run_until("B follows A", |s| s.pos_of(b) == TilePos::new(3, 3, 1));
    assert_eq!(s.pos_of(a), TilePos::new(4, 3, 1));
    s.send(0, ClientIntent::StopPulling);
    s.run_until("A lets go", |s| s.try_get::<Pulling>(a).is_none());

    // A takes the cuffs off through the strip menu (B dragged onto A, the handcuffs entry:
    // strip_delay 4 s); they drop at B's feet.
    let ids = [b, a].map(|e| s.net_id(0, e).unwrap());
    s.send(
        0,
        ClientIntent::Drag {
            from: ss13_shared::protocol::DragEnd::Atom(ids[0]),
            to: ss13_shared::protocol::DragEnd::Atom(ids[1]),
            modifiers: ClickModifiers::default(),
        },
    );
    s.run_until("the strip menu opens", |s| {
        s.players[0]
            .app
            .world()
            .resource::<Received>()
            .strip_window
            .is_some()
    });
    let window = s.players[0]
        .app
        .world()
        .resource::<Received>()
        .strip_window
        .unwrap();
    s.send(
        0,
        ClientIntent::UiAct {
            window,
            action: "use".into(),
            params: vec![("key".into(), "handcuffs".into())],
        },
    );
    s.run_until("stripping starts", |s| s.try_get::<DoAfter>(a).is_some());
    s.run_until_for(10.0, "B is free", |s| s.try_get::<Handcuffed>(b).is_none());
    assert_eq!(s.pos_of(cuffs), TilePos::new(3, 3, 1));
    assert!(!s.traits(b).hands_blocked);

    // B steps away from a cuffing: interrupted.
    let fake = s.item_at(TilePos::new(5, 3, 1));
    s.pick_up(0, fake);
    s.click(0, b);
    s.run_until("cuffing starts", |s| s.try_get::<DoAfter>(a).is_some());
    s.step_once(1, Dir::North);
    s.run_until("B stepped away", |s| s.pos_of(b) == TilePos::new(3, 4, 1));
    s.run_until("the cuffing is interrupted", |s| {
        s.try_get::<DoAfter>(a).is_none()
    });
    s.hold(0.3, "B stays free", |s| {
        s.try_get::<Handcuffed>(b).is_none()
    });
    assert_eq!(s.held(a), Some(fake));
    s.step_once(1, Dir::South);

    // tg's fake handcuffs (breakouttime 1 s): B resists out of them.
    s.click(0, b);
    s.run_until_for(10.0, "B is cuffed with the fakes", |s| {
        s.try_get::<Handcuffed>(b).map(|c| c.0) == Some(fake)
    });
    s.send(1, ClientIntent::Resist);
    s.run_until("B starts breaking out", |s| {
        s.try_get::<DoAfter>(b).is_some()
    });
    let resist_started = s.now();
    s.run_until("B reads how long", |s| {
        s.said(1, "Это займет примерно 1 секунда")
    });
    s.run_until_for(5.0, "B breaks out", |s| {
        s.try_get::<Handcuffed>(b).is_none()
    });
    assert!(s.now() - resist_started >= 0.95);
    s.run_until("B reads it", |s| s.said(1, "Вы успешно снимаете"));
    assert_eq!(s.pos_of(fake), s.pos_of(b));
}

#[test]
fn flash_and_shove() {
    let mut s = Station::start("flash_and_shove", FLASH_MAP, 3);
    let at = |s: &Station, x, y| {
        let pos = TilePos::new(x, y, 1);
        (0..3).find(|&i| s.pos_of(s.players[i].mob) == pos).unwrap()
    };
    let (pa, pb, pc) = (at(&s, 3, 3), at(&s, 3, 4), at(&s, 4, 4));
    let (a, b, c) = (s.players[pa].mob, s.players[pb].mob, s.players[pc].mob);

    // C puts on sunglasses (flash_protect 1).
    let glasses = s.item_at(TilePos::new(5, 4, 1));
    s.pick_up(pc, glasses);
    s.send(pc, ClientIntent::ClickSlot(InventorySlot::Eyes));
    s.run_until("C wears sunglasses", |s| {
        s.get::<Inventory>(c).eyes == Some(glasses)
    });

    // B faces south, towards A: flashed, it gets confusion, stamina damage and falls.
    let flash = s.item_at(TilePos::new(2, 3, 1));
    s.pick_up(pa, flash);
    s.click(pa, b);
    s.run_until("B is knocked down", |s| s.effects(b).knockdown > 0.0);
    let e = s.effects(b);
    assert!(e.stamina >= 80.0 && e.confusion > 0.0, "{e:?}");
    s.run_until("B lies", |s| s.lying(b));
    s.run_until("B reads it", |s| s.said(pb, "ослепляет вас вспышкой"));

    // C faces A too, but the sunglasses block the flash.
    s.click(pa, c);
    s.run_until("A reads the failure", |s| s.said(pa, "не удаётся ослепить"));
    s.hold(0.3, "C is not affected", |s| {
        let e = s.effects(c);
        e.knockdown == 0.0 && e.stamina == 0.0 && e.confusion == 0.0 && !s.lying(c)
    });

    // A shoves C (right click, empty hand): pushed one tile away, staggered.
    s.send(pa, ClientIntent::DropActive);
    s.run_until("flash dropped", |s| s.held(a).is_none());
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0);
    s.click_with(
        pa,
        c,
        ClickModifiers {
            right: true,
            ..default()
        },
    );
    s.run_until("C is pushed away", |s| s.pos_of(c) == TilePos::new(5, 5, 1));
    assert!(s.effects(c).staggered > 0.0);
    s.run_until("C reads the shove", |s| s.said(pc, "толкает вас"));
}
