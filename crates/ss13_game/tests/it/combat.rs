//! Health and combat over real WebTransport on 127.0.0.1: player A hugs B, hits B with a
//! toolbox, then punches and kicks B through crit to death; B's client becomes a ghost that only
//! B sees and that walks through walls.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::{CombatMode, NextMove};
use ss13_game::ghost::Ghost;
use ss13_game::health::{BodyPosition, Health, Stat};
use ss13_game::hud::{HealthHudData, HealthHudUi};
use ss13_game::inventory::Inventory;
use ss13_game::server::{MoveQueue, PlayerMob, Players};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_net::chat::client::ChatReceived;
use ss13_net::client::NetEntityMap;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Appearance, Dir, IconStateId, TilePos};
use ss13_tgui::UiInterface;
use ss13_world::{Density, SpatialIndex};

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
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/toy": { "parent": "/obj/item" },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/toolbox": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "toolbox", "icon_state": "toolbox", "force": 13 } },
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

/// Rows top-down, the last line is y = 1. Spawns `s` (3,3) for A then (3,4) for B; `T` a
/// toolbox (force 13) next to A.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"T" = (/obj/item/storage/toolbox,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffffffw
wfsffffw
wfsTfffw
wffffffw
wwwwwwww
"}
"##;

const SPAWN_A: TilePos = TilePos::new(3, 3, 1);
const SPAWN_B: TilePos = TilePos::new(3, 4, 1);
const TOOLBOX: TilePos = TilePos::new(4, 3, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 7] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toolbox"),
    ("icons/mob/simple/mob.dmi", "ghost"),
    ("l.dmi", "card-id"),
];
const GHOST_ICON: IconStateId = IconStateId(5);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("combat_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/storage/toolbox",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// What a client received: chat lines and its last health HUD data.
#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    hud: Option<HealthHudData>,
    hud_window: Option<u32>,
}

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReceiver,
    mut received: ResMut<Received>,
) {
    for ChatReceived(message) in chat.read() {
        received.chat.push(message.plain_text());
    }
    for UiMessageReceived(message) in ui.0.read() {
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

#[derive(bevy::ecs::system::SystemParam)]
struct MessageReceiver<'w, 's>(MessageReader<'w, 's, UiMessageReceived>);

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

    fn health(&self, p: usize) -> f32 {
        self.get::<Health>(self.players[p].mob).health()
    }

    fn stat(&self, p: usize) -> Stat {
        *self.get::<Stat>(self.players[p].mob)
    }

    fn pos_of(&self, e: Entity) -> TilePos {
        *self.get::<TilePos>(e)
    }

    /// Clicks `target` once `p`'s click cooldown is over, then waits until the click was
    /// handled (the cooldown restarted).
    fn attack(&mut self, p: usize, target: Entity) {
        let mob = self.players[p].mob;
        let before = self.get::<NextMove>(mob).0;
        self.run_until("A is off cooldown", |s| {
            s.server.world().resource::<Time>().elapsed_secs_f64() >= before
        });
        self.click(p, target);
        self.run_until("the attack is handled", |s| {
            s.get::<NextMove>(mob).0 > before
        });
    }

    fn chat(&self, p: usize) -> &[String] {
        &self.players[p].app.world().resource::<Received>().chat
    }

    fn said(&self, p: usize, text: &str) -> bool {
        self.chat(p).iter().any(|line| line.contains(text))
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
fn punch_to_crit_death_and_ghost() {
    let mut s = Station::start("punch_to_crit_death_and_ghost", 2);
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert_eq!((s.pos_of(a), s.pos_of(b)), (SPAWN_A, SPAWN_B));
    assert_eq!((s.stat(1), s.health(1)), (Stat::Conscious, 100.0));
    s.run_until("B's health HUD arrives", |s| {
        s.hud(1)
            .and_then(|h| h.health)
            .is_some_and(|r| r.health_state() == 0)
    });
    let (name_a, name_b) = (
        s.get::<Name>(a).as_str().to_owned(),
        s.get::<Name>(b).as_str().to_owned(),
    );

    // Without combat mode an empty hand hugs; the click cooldown (`CLICK_CD_MELEE`) swallows a
    // second click, even one that reaches the server in the same tick.
    let (target, client) = (s.net_id(0, b).unwrap(), s.players[0].link);
    for _ in 0..2 {
        s.server
            .world_mut()
            .write_message(ss13_net::server::IntentReceived {
                client,
                intent: ClientIntent::Click {
                    target,
                    modifiers: ClickModifiers::default(),
                },
            });
    }
    s.run_until("B is hugged", |s| {
        s.said(1, &format!("{name_a} обнимает вас"))
    });
    s.hold(0.3, "one hug per cooldown, no damage", |s| {
        s.chat(1).iter().filter(|l| l.contains("обнимает")).count() == 1 && s.health(1) == 100.0
    });
    assert!(s.said(0, &format!("Вы обнимаете {name_b}")));

    // A held item attacks whatever the combat mode: the toolbox's force 13 as brute.
    let toolbox = {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(TOOLBOX)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some())
            .unwrap()
    };
    assert_eq!(s.get::<DmItem>(toolbox).force, 13.0);
    s.run_until("off cooldown", |s| {
        s.server.world().resource::<Time>().elapsed_secs_f64() >= s.get::<NextMove>(a).0
    });
    s.click(0, toolbox);
    s.run_until("A holds the toolbox", |s| {
        s.get::<Inventory>(a).hands[0] == Some(toolbox)
    });
    s.attack(0, b);
    s.run_until("the toolbox hit lands", |s| s.health(1) == 87.0);
    s.run_until("B reads it", |s| {
        s.said(1, &format!("{name_a} атакует вас с помощью toolbox!"))
    });
    s.send(0, ClientIntent::DropActive);
    s.run_until("dropped", |s| s.get::<Inventory>(a).hands[0].is_none());

    // Combat mode, then punches (5-10, 10% misses) until B falls into crit.
    s.send(0, ClientIntent::ToggleCombatMode);
    s.run_until("combat mode on", |s| s.get::<CombatMode>(a).0);
    s.run_until("A's HUD shows combat mode", |s| {
        s.hud(0).is_some_and(|h| h.combat_mode)
    });
    let mut punches = 0;
    while s.stat(1) == Stat::Conscious {
        let before = s.health(1);
        s.attack(0, b);
        punches += 1;
        assert!(punches < 60, "B never fell");
        let dealt = before - s.health(1);
        // Less than rolled only on a limb near its `max_damage` (50).
        assert!((0.0..=10.0).contains(&dealt), "{dealt}");
    }
    s.run_until("B reads a punch", |s| {
        s.said(1, &format!("{name_a} бьет вас!"))
    });
    assert!(s.health(1) <= 0.0);

    // Crit: B lies (turned, lower, not dense) on every client, cannot move or punch back.
    s.run_until("B lies", |s| s.get::<BodyPosition>(b).lying());
    let rotation = s.get::<Appearance>(b).rotation;
    assert!(rotation == 90 || rotation == 270, "{rotation}");
    assert!(s.server.world().get::<Density>(b).is_none());
    s.run_until("A's client sees B lying", |s| {
        let copy = s.client_copy(0, b).unwrap();
        s.players[0]
            .app
            .world()
            .get::<Appearance>(copy)
            .is_some_and(|a| a.rotation == rotation && a.pixel_offset == [0, -6])
    });
    s.run_until("B's HUD shows crit", |s| {
        s.hud(1)
            .and_then(|h| h.health)
            .is_some_and(|r| r.health_state() == 6)
    });
    let health_a = s.health(0);
    s.send(1, ClientIntent::ToggleCombatMode);
    s.click(1, a);
    s.hold(0.5, "B cannot hit back from crit", |s| {
        s.health(0) == health_a
    });

    // Kicks (7-15, never miss on a lying target) until B dies.
    let mut kicks = 0;
    while s.stat(1) != Stat::Dead {
        // Brute only: crit breathing adds oxyloss between kicks, more the slower the machine.
        let (before, stat) = (s.get::<Health>(b).brute, s.stat(1));
        s.attack(0, b);
        kicks += 1;
        assert!(kicks < 40, "B never died");
        if stat != Stat::HardCrit && s.stat(1) == Stat::HardCrit {
            // A hard crit mob does not move: its step is applied as blocked (and acked).
            let at = s.pos_of(b);
            s.try_step(1, Dir::North);
            let seq = s.players[1].seq;
            s.run_until("the step is applied", |s| {
                s.get::<MoveQueue>(b).applied() == seq
            });
            s.hold(0.3, "B stays put", |s| s.pos_of(b) == at);
        }
        let dealt = s.get::<Health>(b).brute - before;
        // A kick on a maxed limb deals less (tg `receive_damage` caps at `max_damage`).
        assert!((0.0..=15.0).contains(&dealt), "kick dealt {dealt}");
    }
    assert!(s.health(1) <= -100.0);
    s.run_until("everyone near reads the death", |s| {
        s.said(0, "взгляд становится пустым и безжизненным")
            && s.said(1, "взгляд становится пустым и безжизненным")
    });
    // A corpse can still be kicked; it dies only once. The kick may land on a limb already at
    // its `max_damage` (20% go to a random zone) and deal nothing, so no strict decrease.
    let corpse = s.get::<Health>(b).brute;
    s.attack(0, b);
    let dealt = s.get::<Health>(b).brute - corpse;
    assert!((0.0..=15.0).contains(&dealt), "corpse kick dealt {dealt}");
    assert_eq!(s.stat(1), Stat::Dead);
    s.hold(0.3, "one death message", |s| {
        s.chat(0)
            .iter()
            .filter(|l| l.contains("безжизненным"))
            .count()
            == 1
    });
    s.run_until("B's HUD shows death", |s| {
        s.hud(1)
            .and_then(|h| h.health)
            .is_some_and(|r| r.dead && r.health_state() == 7)
    });

    // A dead player's step ghostizes it: a ghost on the body's tile, controlled by B, seen only
    // by B; the body stays.
    s.run_until_for(10.0, "B becomes a ghost", |s| {
        if s.controlled(1) != b {
            return true;
        }
        s.try_step(1, Dir::West);
        false
    });
    let ghost = s.controlled(1);
    assert_eq!(s.get::<Ghost>(ghost).body, Some(b));
    assert_eq!(s.pos_of(ghost), SPAWN_B);
    assert!(s.server.world().get::<PlayerMob>(b).is_none());
    assert_eq!(s.stat(1), Stat::Dead, "the body stays dead on the map");
    s.run_until("B's client controls its ghost", |s| {
        let copy = s.client_copy(1, ghost);
        copy.is_some()
            && s.players[1]
                .app
                .world()
                .resource::<NetEntityMap>()
                .controlled()
                == copy
    });
    let ghost_copy = s.client_copy(1, ghost).unwrap();
    assert_eq!(
        s.players[1]
            .app
            .world()
            .get::<Appearance>(ghost_copy)
            .unwrap()
            .icon_state,
        GHOST_ICON
    );
    s.run_until("B's HUD has no health as a ghost", |s| {
        s.hud(1).is_some_and(|h| h.health.is_none())
    });
    s.hold(0.5, "A never sees the ghost", |s| {
        s.net_id(0, ghost).is_none()
    });
    assert!(s.client_copy(0, b).is_some(), "A still sees the body");

    // The ghost walks west through the wall at x = 1 and stops at the map's edge.
    for to in [TilePos::new(2, 4, 1), TilePos::new(1, 4, 1)] {
        s.run_until(&format!("the ghost reaches {to:?}"), |s| {
            if s.pos_of(ghost) == to {
                return true;
            }
            s.try_step(1, Dir::West);
            false
        });
    }
    s.try_step(1, Dir::West);
    s.hold(0.3, "the map's edge holds the ghost", |s| {
        s.pos_of(ghost) == TilePos::new(1, 4, 1)
    });
    assert!(s.net_id(0, ghost).is_none());

    // A ghost has no hands and no fists: its clicks do nothing.
    let health_a = s.health(0);
    if let Some(id) = s.net_id(1, a) {
        s.send(
            1,
            ClientIntent::Click {
                target: id,
                modifiers: ClickModifiers::default(),
            },
        );
    }
    s.hold(0.3, "ghosts do not punch", |s| s.health(0) == health_a);
}

#[test]
fn disconnect_leaves_the_body() {
    let mut s = Station::start("disconnect_leaves_the_body", 2);
    let b = s.players[1].mob;
    let link = s.players[1].link;
    let player = s.players.pop().unwrap();
    drop(player);
    s.run_until("B's connection is gone", |s| {
        s.server.world().resource::<Players>().mob(link).is_none()
    });
    let world = s.server.world();
    assert!(world.get_entity(b).is_ok(), "the body stays");
    assert!(world.get::<PlayerMob>(b).is_none());
    assert_eq!(world.get::<Stat>(b), Some(&Stat::Conscious));
    let mut ghosts = s.server.world_mut().query::<&Ghost>();
    assert_eq!(ghosts.iter(s.server.world()).count(), 0, "no ghost");
}
