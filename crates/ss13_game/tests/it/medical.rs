//! Medicine over real WebTransport on 127.0.0.1: A punches B, selects zones on the HUD, applies
//! a bruise pack to B's chest (interrupted by a step, then for real after `other_delay`), scans B
//! with a health analyzer and gives CPR to B in crit.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::{CombatMode, NextMove};
use ss13_game::health::{Health, Stat, ZoneSelected};
use ss13_game::hud::{HealthHudData, HealthHudUi};
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::{DoAfter, Stack};
use ss13_game::zone::{BodyZone, TargetZone};
use ss13_mapload::DmItem;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
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
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/medical": { "parent": "/obj/item/stack", "declared_vars": {
        "name": "medical pack", "amount": 6, "max_amount": 6, "heal_brute": null,
        "heal_burn": null, "stop_bleeding": null, "self_delay": 50, "other_delay": 0,
        "repeating": 0, "works_on_dead": 0, "apply_verb": "обрабатываем" } },
    "/obj/item/stack/medical/bruise_pack": { "parent": "/obj/item/stack/medical",
        "declared_vars": { "name": "bruise pack", "icon_state": "brutepack", "heal_brute": 40,
        "self_delay": 40, "other_delay": 20 } },
    "/obj/item/healthanalyzer": { "parent": "/obj/item", "declared_vars": {
        "name": "health analyzer", "icon_state": "health" } },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
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

/// Rows top-down, the last line is y = 1. Spawns `s` (3,3) for A then (3,4) for B; a bruise
/// pack `P` east of A, a health analyzer `H` west of A.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"P" = (/obj/item/stack/medical/bruise_pack,/turf/open/floor,/area/space)
"H" = (/obj/item/healthanalyzer,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffffffw
wfsffffw
wHsPfffw
wffffffw
wwwwwwww
"}
"##;

const SPAWN_A: TilePos = TilePos::new(3, 3, 1);
const SPAWN_B: TilePos = TilePos::new(3, 4, 1);
const PACK: TilePos = TilePos::new(4, 3, 1);
const ANALYZER: TilePos = TilePos::new(2, 3, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "brutepack"),
    ("i.dmi", "health"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("medical_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/stack/medical/bruise_pack",
        "/obj/item/healthanalyzer",
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

impl Station {
    fn item_on(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some())
            .unwrap()
    }

    fn select_zone(&mut self, p: usize, zone: TargetZone) {
        let window = self.players[p]
            .app
            .world()
            .resource::<Received>()
            .hud_window
            .expect("the health HUD is open");
        self.send(
            p,
            ClientIntent::UiAct {
                window,
                action: "zone".into(),
                params: vec![("zone".into(), zone.dm().into())],
            },
        );
        let mob = self.players[p].mob;
        self.run_until("the zone is selected", |s| {
            s.get::<ZoneSelected>(mob).0 == zone
                && s.hud(p).is_some_and(|h| h.zone_selected == zone)
        });
    }

    fn brute(&self, p: usize, zone: BodyZone) -> f32 {
        self.get::<Health>(self.players[p].mob).part(zone).brute
    }
}

#[test]
fn bruise_pack_scan_and_cpr() {
    let mut s = Station::start("bruise_pack_scan_and_cpr", 2);
    s.0.without_mood();
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert_eq!((s.pos_of(a), s.pos_of(b)), (SPAWN_A, SPAWN_B));
    s.run_until("A's health HUD arrives", |s| {
        s.hud(0).is_some_and(|h| h.health.is_some())
    });
    assert_eq!(s.get::<ZoneSelected>(a).0, TargetZone::Chest, "tg default");

    // Zone selection goes through the HUD window; a bad zone is rejected.
    s.select_zone(0, TargetZone::LArm);
    let window = s.players[0]
        .app
        .world()
        .resource::<Received>()
        .hud_window
        .unwrap();
    s.send(
        0,
        ClientIntent::UiAct {
            window,
            action: "zone".into(),
            params: vec![("zone".into(), "tail".into())],
        },
    );
    s.hold(0.2, "an unknown zone changes nothing", |s| {
        s.get::<ZoneSelected>(a).0 == TargetZone::LArm
    });
    s.select_zone(0, TargetZone::Groin);

    // Punches at the groin (the chest bodypart) until the chest is hurt.
    s.send(0, ClientIntent::ToggleCombatMode);
    s.run_until("combat mode on", |s| s.get::<CombatMode>(a).0);
    let mut punches = 0;
    while s.brute(1, BodyZone::Chest) < 5.0 {
        s.attack(0, b);
        punches += 1;
        assert!(punches < 40, "the chest was never hit");
    }
    s.send(0, ClientIntent::ToggleCombatMode);
    s.run_until("combat mode off", |s| !s.get::<CombatMode>(a).0);
    s.select_zone(0, TargetZone::Chest);

    // A takes the bruise pack.
    let pack = s.item_on(PACK);
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0);
    s.click(0, pack);
    s.run_until("A holds the pack", |s| {
        s.get::<Inventory>(a).active_item() == Some(pack)
    });
    assert_eq!(s.get::<Stack>(pack).amount, 6);

    // A step cancels the application.
    let chest = s.brute(1, BodyZone::Chest);
    s.click(0, b);
    s.run_until("the application starts", |s| {
        s.server.world().get::<DoAfter>(a).is_some()
    });
    s.try_step(0, Dir::East);
    s.run_until("A stepped", |s| s.pos_of(a) == PACK);
    assert!(
        s.server.world().get::<DoAfter>(a).is_none(),
        "moving cancels"
    );
    s.hold(2.5, "nothing healed", |s| {
        s.brute(1, BodyZone::Chest) == chest && s.get::<Stack>(pack).amount == 6
    });
    s.try_step(0, Dir::West);
    s.run_until("A is back", |s| s.pos_of(a) == SPAWN_A);

    // For real: `other_delay` 2 s, then `heal_brute` 40 on the chest, one unit used.
    let chest = s.brute(1, BodyZone::Chest);
    let total = s.get::<Health>(b).brute;
    s.click(0, b);
    s.run_until("the application starts", |s| {
        s.server.world().get::<DoAfter>(a).is_some()
    });
    let started = s.now();
    s.run_until("the chest is treated", |s| {
        s.brute(1, BodyZone::Chest) < chest
    });
    assert!(
        s.now() - started >= 1.9,
        "after other_delay: {}",
        s.now() - started
    );
    let healed = chest - (chest - 40.0).max(0.0);
    assert_eq!(s.brute(1, BodyZone::Chest), (chest - 40.0).max(0.0));
    assert!((total - s.get::<Health>(b).brute - healed).abs() < 1e-3);
    assert_eq!(s.get::<Stack>(pack).amount, 5);
    s.run_until("A reads it", |s| {
        s.said(0, "Вы применяете bruise pack на грудь у") && s.said(0, "грудь обработана")
    });
    // The chest is clean now: another application is refused.
    s.click(0, b);
    s.run_until("refused", |s| s.said(0, "грудь не повреждена!"));
    assert!(s.server.world().get::<DoAfter>(a).is_none());

    // The health analyzer in the other hand scans B.
    s.send(0, ClientIntent::SwapHands);
    s.run_until("the other hand is active", |s| {
        s.get::<Inventory>(a).active_item().is_none()
    });
    let analyzer = s.item_on(ANALYZER);
    s.click(0, analyzer);
    s.run_until("A holds the analyzer", |s| {
        s.get::<Inventory>(a).active_item() == Some(analyzer)
    });
    let name_b = s.get::<Name>(b).as_str().to_owned();
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0);
    s.click(0, b);
    s.run_until("the scan prints", |s| {
        s.said(0, &format!("Анализ результатов для {name_b}"))
    });
    let scan = s
        .chat(0)
        .iter()
        .rev()
        .find(|l| l.contains("Анализ результатов"))
        .unwrap()
        .clone();
    let percent = (s.get::<Health>(b).health() + 0.5).floor() as i64;
    assert!(
        scan.contains(&format!("Общее состояние: {percent}% здоровья")),
        "{scan}"
    );
    assert!(scan.contains("Внутренняя температура тела: "), "{scan}");
    assert!(s.said(1, "анализирует жизненные показатели"));

    // CPR: B in crit with oxyloss; A's empty hand out of combat mode.
    s.send(0, ClientIntent::DropActive);
    s.run_until("the hand is empty", |s| {
        s.get::<Inventory>(a).active_item().is_none()
    });
    {
        let world = s.server.world_mut();
        let mut health = world.get::<Health>(b).unwrap().clone();
        health.adjust(ss13_game::health::DamageType::Brute, -1000.0);
        health.damage_zone(BodyZone::Chest, ss13_game::health::DamageType::Brute, 80.0);
        health.adjust(ss13_game::health::DamageType::Oxy, 40.0 - health.oxy);
        world.entity_mut(b).insert((health, Stat::SoftCrit));
    }
    s.run_until("B lies", |s| {
        s.server
            .world()
            .get::<ss13_game::health::BodyPosition>(b)
            .is_some_and(|p| p.lying())
    });
    let oxy = s.get::<Health>(b).oxy;
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0);
    s.click(0, b);
    s.run_until("CPR starts", |s| {
        s.server.world().get::<DoAfter>(a).is_some()
    });
    let started = s.now();
    s.run_until_for(10.0, "CPR helps", |s| s.get::<Health>(b).oxy < oxy - 1.0);
    assert!(s.now() - started >= 2.9, "after 3 s: {}", s.now() - started);
    // Up to 7 oxyloss per CPR (crit breathing may add a little meanwhile).
    let after = s.get::<Health>(b).oxy;
    assert!(
        after <= oxy - 7.0 + 2.0 && after >= oxy - 7.0,
        "{oxy} -> {after}"
    );
    s.run_until("A reads it", |s| s.said(0, "Вы проводите СЛР"));
    s.run_until("B feels it", |s| {
        s.said(1, "Вы чувствуете глоток свежего воздуха")
    });
}
