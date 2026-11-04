//! Surgery over real WebTransport on 127.0.0.1: the operating computer next to an operating
//! table (its window, a surgery disk unlocking the locked tend wounds operations for a patient on
//! the table only), and the operations on organs and wounds: coronary bypass, brain surgery,
//! debridement, joining and sealing punctured vessels, revival by a shock to the brain.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::NextMove;
use ss13_game::health::{BodyPosition, DamageType, Health, Stat, ZoneSelected};
use ss13_game::inventory::Inventory;
use ss13_game::medical::defib::{Defib, OrganDecay};
use ss13_game::medical::operating::OperatingComputer;
use ss13_game::medical::wounds::{Limbs, Series, WoundKind, apply_wound, state};
use ss13_game::operating::{OperatingComputerData, OperatingComputerUi};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_game::zone::{BodyZone, TargetZone};
use ss13_mapload::{DmItem, DmObj};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot, NetId};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/table": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "t.dmi" }, "icon_state": "table", "density": 1, "layer": 2.8,
        "pass_flags_self": 65, "deconstruction_ready": 1, "can_flip": 1,
        "base_icon_state": "table", "slam_gently": 0 } },
    "/obj/structure/table/optable": { "parent": "/obj/structure/table", "declared_vars": {
        "name": "operating table", "icon_state": "surgery_table", "can_flip": 0,
        "slam_gently": 1 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "m.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/operating": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "operating computer" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "sharpness": 0, "item_flags": 0,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/hemostat": { "parent": "/obj/item", "declared_vars": { "name": "hemostat",
        "icon_state": "hemostat", "item_flags": 1024, "tool_behaviour": "hemostat" } },
    "/obj/item/cautery": { "parent": "/obj/item", "declared_vars": { "name": "cautery",
        "icon_state": "cautery", "item_flags": 1024, "tool_behaviour": "cautery",
        "heat": 500 } },
    "/obj/item/disk": { "parent": "/obj/item", "declared_vars": { "icon_state": "datadisk1" } },
    "/obj/item/disk/surgery": { "parent": "/obj/item/disk", "declared_vars": {
        "name": "диск хирургических процедур" } },
    "/obj/item/disk/surgery/debug": { "parent": "/obj/item/disk/surgery" },
    "/obj/item/defibrillator": { "parent": "/obj/item", "declared_vars": {
        "name": "defibrillator", "icon_state": "defibunit", "slot_flags": 1024, "combat": 0,
        "cooldown_duration": 50, "paddle_type": { "path": "/obj/item/shockpaddles" } } },
    "/obj/item/defibrillator/loaded": { "parent": "/obj/item/defibrillator" },
    "/obj/item/shockpaddles": { "parent": "/obj/item", "declared_vars": {
        "name": "defibrillator paddles", "icon_state": "defibpaddles0" } },
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
    "/area/space": { "parent": "/area", "declared_vars": { "static_lighting": 0,
        "base_lighting_alpha": 255 } }
  }
}"##;

/// Rows top-down, the last line is y = 1. A stands at (3,3) with the operating table `T` north
/// of it, the operating computer `O` west of the table and around it a hemostat `H`, a
/// defibrillator `D`, a cautery `K` and a debug surgery disk `X`; B starts at (5,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"H" = (/obj/item/hemostat,/turf/open/floor,/area/space)
"D" = (/obj/item/defibrillator/loaded,/turf/open/floor,/area/space)
"K" = (/obj/item/cautery,/turf/open/floor,/area/space)
"X" = (/obj/item/disk/surgery/debug,/turf/open/floor,/area/space)
"O" = (/obj/machinery/computer/operating,/turf/open/floor,/area/space)
"T" = (/obj/structure/table/optable,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffffffw
wOTHfffw
wDsfsffw
wXfKfffw
wwwwwwww
"}
"##;

const HEMOSTAT: TilePos = TilePos::new(4, 4, 1);
const DEFIB: TilePos = TilePos::new(2, 3, 1);
const CAUTERY: TilePos = TilePos::new(4, 2, 1);
const DISK: TilePos = TilePos::new(2, 2, 1);
const COMPUTER: TilePos = TilePos::new(2, 4, 1);
const TABLE: TilePos = TilePos::new(3, 4, 1);
const FLOOR: TilePos = TilePos::new(4, 3, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 11] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("t.dmi", "surgery_table"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "hemostat"),
    ("i.dmi", "cautery"),
    ("i.dmi", "datadisk1"),
    ("i.dmi", "defibunit"),
    ("i.dmi", "defibpaddles0"),
    ("m.dmi", "computer"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("surgery_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/table/optable",
        "/obj/item/hemostat",
        "/obj/item/cautery",
        "/obj/item/disk/surgery/debug",
        "/obj/item/defibrillator/loaded",
        "/obj/item/shockpaddles",
        "/obj/machinery/computer/operating",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    dir
}

/// What a client received: chat lines, the last `ListInputWindow` and `OperatingComputer` data.
#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    list: Option<(u32, ListInputData)>,
    computer: Option<(u32, OperatingComputerData)>,
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
            } if interface == ListInputUi::NAME => {
                received.list = Some((*window, ss13_tgui::decode(data).unwrap()));
            }
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == OperatingComputerUi::NAME && !data.is_empty() => {
                received.computer = Some((*window, ss13_tgui::decode(data).unwrap()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if received.computer.as_ref().is_some_and(|(w, _)| w == window) => {
                received.computer = Some((*window, ss13_tgui::decode(data).unwrap()));
            }
            _ => {}
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
            app.init_resource::<Received>().add_systems(Update, record);
        });
        let (a, b) = (s.players[0].mob, s.players[1].mob);
        let world = s.server.world_mut();
        world.entity_mut(a).insert(TilePos::new(3, 3, 1));
        world.entity_mut(b).insert(TilePos::new(5, 3, 1));
        Self(s)
    }

    fn net_id(&self, p: usize, entity: Entity) -> NetId {
        self.0.net_id(p, entity).expect("replicated")
    }

    fn click(&mut self, p: usize, entity: Entity) {
        let mob = self.players[p].mob;
        self.run_until("off cooldown", |s| {
            s.server.world().resource::<Time>().elapsed_secs_f64() >= s.get::<NextMove>(mob).0
        });
        let target = self.net_id(p, entity);
        self.send(
            p,
            ClientIntent::Click {
                target,
                modifiers: ClickModifiers::default(),
            },
        );
    }

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn said(&self, p: usize, text: &str) -> bool {
        self.received(p).chat.iter().any(|l| l.contains(text))
    }

    fn on_tile(&self, pos: TilePos, root: &str) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| {
                world
                    .get::<DmItem>(e)
                    .map(|i| i.path.as_str())
                    .or_else(|| world.get::<DmObj>(e).map(|o| o.path.as_str()))
                    .is_some_and(|p| p.starts_with(root))
            })
            .unwrap()
    }

    /// Picks up the item at `pos` into `p`'s active hand (after dropping what it holds).
    fn take(&mut self, p: usize, pos: TilePos, root: &str) -> Entity {
        let mob = self.players[p].mob;
        if self.get::<Inventory>(mob).active_item().is_some() {
            self.send(p, ClientIntent::DropActive);
            self.run_until("the hand is empty", |s| {
                s.get::<Inventory>(mob).active_item().is_none()
            });
        }
        let item = self.on_tile(pos, root);
        self.click(p, item);
        self.run_until("the item is held", |s| {
            s.get::<Inventory>(mob).active_item() == Some(item)
        });
        item
    }

    fn zone(&mut self, p: usize, zone: TargetZone) {
        let mob = self.players[p].mob;
        self.server
            .world_mut()
            .entity_mut(mob)
            .insert(ZoneSelected(zone));
    }

    /// B lies paralyzed at `pos`.
    fn lay_down(&mut self, pos: TilePos) {
        let b = self.players[1].mob;
        let world = self.server.world_mut();
        world.entity_mut(b).insert(pos);
        ss13_game::status_effects::apply(
            world,
            b,
            ss13_game::status_effects::Incap::Paralyze,
            600.0,
        );
        self.run_until("B lies", |s| s.get::<BodyPosition>(b).lying());
    }

    fn limbs(&mut self, f: impl FnOnce(&mut Limbs)) {
        let b = self.players[1].mob;
        let world = self.server.world_mut();
        let mut limbs = world.get::<Limbs>(b).unwrap().clone();
        f(&mut limbs);
        world.entity_mut(b).insert(limbs);
    }

    /// A clicks B with what it holds and the operation list opens: its entries.
    fn list(&mut self) -> (u32, Vec<String>) {
        let b = self.players[1].mob;
        self.players[0]
            .app
            .world_mut()
            .resource_mut::<Received>()
            .list = None;
        self.click(0, b);
        self.run_until("the operation list opens", |s| s.received(0).list.is_some());
        let (window, data) = self.received(0).list.clone().unwrap();
        (window, data.items)
    }

    /// B passes out from oxygen loss (unless dead).
    fn knock_out(&mut self) {
        let b = self.players[1].mob;
        let world = self.server.world_mut();
        if world.get::<Stat>(b) == Some(&Stat::Dead) {
            return;
        }
        let mut health = world.get::<Health>(b).unwrap().clone();
        if health.oxy < 60.0 {
            health.adjust(DamageType::Oxy, 60.0 - health.oxy);
            world.entity_mut(b).insert(health);
        }
    }

    /// A starts `entry` on B (from the list, or at once when it is the only one) and repeats it
    /// until `done`: a failed step (5% with basic tools) leaves the state as it was.
    fn operate(&mut self, entry: &str, done: impl Fn(&Self) -> bool) {
        let (a, b) = (self.players[0].mob, self.players[1].mob);
        for _ in 0..10 {
            self.players[0]
                .app
                .world_mut()
                .resource_mut::<Received>()
                .list = None;
            self.click(0, b);
            self.run_until("the operation starts or a list opens", |s| {
                s.server.world().get::<DoAfter>(a).is_some() || s.received(0).list.is_some()
            });
            if let Some((window, _)) = self.received(0).list.clone() {
                self.send(
                    0,
                    ClientIntent::UiAct {
                        window,
                        action: "submit".into(),
                        params: vec![("entry".into(), entry.into())],
                    },
                );
                self.run_until("the picked operation starts", |s| {
                    s.server.world().get::<DoAfter>(a).is_some()
                });
            }
            // Steps on, keeping B out cold (a conscious patient adds 40% to the failure chance).
            for _ in 0..600 {
                self.knock_out();
                self.step();
                if self.server.world().get::<DoAfter>(a).is_none() || done(self) {
                    break;
                }
            }
            if done(self) {
                self.server.world_mut().entity_mut(a).remove::<DoAfter>();
                return;
            }
        }
        panic!("{entry} never succeeded");
    }
}

#[test]
fn operating_computer_unlocks_operations() {
    let mut s = Station::start("operating_computer_unlocks_operations");
    let b = s.players[1].mob;
    let computer = s.on_tile(COMPUTER, "/obj/machinery/computer/operating");

    // B lies on the table with a prepared, opened chest, bruised and burnt.
    s.lay_down(TABLE);
    s.limbs(|l| {
        let chest = l.limb_mut(BodyZone::Chest);
        chest.prepped = true;
        chest.surgery = state::SKIN_OPEN | state::VESSELS_UNCLAMPED;
    });
    {
        let world = s.server.world_mut();
        let mut health = world.get::<Health>(b).unwrap().clone();
        health.damage_zone(BodyZone::Chest, DamageType::Brute, 30.0);
        health.damage_zone(BodyZone::Chest, DamageType::Burn, 20.0);
        world.entity_mut(b).insert(health);
    }

    // An empty hand opens the window: the patient, its chest, what it is ready for.
    s.zone(0, TargetZone::Chest);
    s.click(0, computer);
    s.run_until("the window opens", |s| {
        s.received(0)
            .computer
            .as_ref()
            .is_some_and(|(_, d)| d.patient.is_some())
    });
    let data = s.received(0).computer.clone().unwrap().1;
    assert!(data.has_table);
    assert_eq!(data.target_zone, "chest");
    let patient = data.patient.unwrap();
    assert_eq!(patient.brute_loss, 30.0);
    assert!(
        patient.surgery_state.contains(&"Кожа раскрыта".to_owned()),
        "{:?}",
        patient.surgery_state
    );
    let next: Vec<&str> = data
        .surgeries
        .iter()
        .filter(|o| o.show_as_next)
        .map(|o| o.name.as_str())
        .collect();
    assert!(next.contains(&"Обработка ран"), "{next:?}");
    assert!(next.contains(&"Зажать сосуды"), "{next:?}");
    assert!(
        !data
            .surgeries
            .iter()
            .any(|o| o.name.starts_with("Улучшенная")),
        "locked operations stay out of the catalog"
    );

    // "change_zone" with a made-up zone is dropped; a real one moves the window and the selector.
    let window = s.received(0).computer.as_ref().unwrap().0;
    let a = s.players[0].mob;
    for zone in ["tail", "l_arm"] {
        s.send(
            0,
            ClientIntent::UiAct {
                window,
                action: "change_zone".into(),
                params: vec![("new_zone".into(), zone.into())],
            },
        );
    }
    s.run_until("the zone follows", |s| {
        s.get::<ZoneSelected>(a).0 == TargetZone::LArm
            && s.received(0)
                .computer
                .as_ref()
                .is_some_and(|(_, d)| d.target_zone == "l_arm")
    });
    s.zone(0, TargetZone::Chest);

    // The debug disk loads after a second; the computer then offers every operation.
    s.take(0, DISK, "/obj/item/disk/surgery");
    s.click(0, computer);
    s.run_until("the disk loads", |s| s.said(0, "операции загружены"));
    assert!(!s.get::<OperatingComputer>(computer).disks.is_empty());

    // On the table the best tending (the combo master) replaces the others.
    s.take(0, HEMOSTAT, "/obj/item/hemostat");
    let (window, items) = s.list();
    assert!(
        items.contains(&"Обработайте ушибы и ожоги".to_owned()),
        "{items:?}"
    );
    assert!(!items.contains(&"Обработка ушибов".to_owned()), "{items:?}");
    s.send(
        0,
        ClientIntent::UiAct {
            window,
            action: "cancel".into(),
            params: Vec::new(),
        },
    );
    s.run_until("the list closes", |s| {
        s.server
            .world()
            .get::<ss13_game::medical::surgery::SurgeryChoice>(s.players[0].mob)
            .is_none()
    });
    for _ in 0..5 {
        s.step();
    }

    // Off the table the computer is not consulted.
    s.lay_down(FLOOR);
    let (_, items) = s.list();
    assert!(items.contains(&"Обработка ушибов".to_owned()), "{items:?}");
    assert!(
        !items.contains(&"Обработайте ушибы и ожоги".to_owned()),
        "{items:?}"
    );
}

#[test]
fn organ_and_wound_operations() {
    let mut s = Station::start("organ_and_wound_operations");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    s.lay_down(TABLE);

    // Coronary bypass: a heart at 80 comes back to 60, once.
    s.limbs(|l| {
        let chest = l.limb_mut(BodyZone::Chest);
        chest.prepped = true;
        chest.surgery =
            state::SKIN_OPEN | state::ORGANS_CUT | state::BONE_SAWED | state::VESSELS_CLAMPED;
    });
    s.server.world_mut().get_mut::<OrganDecay>(b).unwrap().heart = 80.0;
    s.take(0, HEMOSTAT, "/obj/item/hemostat");
    s.zone(0, TargetZone::Chest);
    s.operate("Коронарное шунтирование", |s| {
        s.get::<OrganDecay>(s.players[1].mob).heart_operated
    });
    assert!((s.get::<OrganDecay>(b).heart - 60.0).abs() < 0.01);

    // Brain surgery loops, 50 a step, while the surgeon keeps at it.
    s.limbs(|l| {
        let head = l.limb_mut(BodyZone::Head);
        head.prepped = true;
        head.surgery = state::SKIN_OPEN | state::BONE_SAWED | state::VESSELS_CLAMPED;
    });
    s.server.world_mut().get_mut::<OrganDecay>(b).unwrap().brain = 120.0;
    s.zone(0, TargetZone::Head);
    s.operate("Операция на мозге", |s| {
        s.get::<OrganDecay>(s.players[1].mob).brain <= 20.0
    });

    // Debridement: the infection of a burn comes off 4 a step.
    s.limbs(|l| {
        let arm = l.limb_mut(BodyZone::LArm);
        arm.prepped = true;
        apply_wound(arm, WoundKind::BurnSevere);
        let burn = arm
            .wounds
            .iter_mut()
            .find(|w| w.kind == WoundKind::BurnSevere);
        burn.unwrap().infection = 6.0;
    });
    s.zone(0, TargetZone::LArm);
    s.operate("Обработка зараженной плоти", |s| {
        s.get::<Limbs>(s.players[1].mob)
            .limb(BodyZone::LArm)
            .wound(Series::Burn)
            .is_none_or(|w| w.infection <= 0.0)
    });

    // A puncture: the vessels joined with a hemostat, then sealed with a lit cautery.
    s.limbs(|l| {
        let arm = l.limb_mut(BodyZone::RArm);
        arm.prepped = true;
        apply_wound(arm, WoundKind::PierceModerate);
        arm.surgery |= state::SKIN_OPEN | state::ORGANS_CUT | state::VESSELS_CLAMPED;
    });
    s.zone(0, TargetZone::RArm);
    let flow = s
        .get::<Limbs>(b)
        .limb(BodyZone::RArm)
        .wound(Series::Pierce)
        .unwrap()
        .blood_flow;
    s.operate("Соединение сосудов", |s| {
        s.get::<Limbs>(s.players[1].mob)
            .limb(BodyZone::RArm)
            .wound(Series::Pierce)
            .is_none_or(|w| w.mend_state)
    });
    if let Some(w) = s.get::<Limbs>(b).limb(BodyZone::RArm).wound(Series::Pierce) {
        assert!(w.blood_flow < flow);
        s.take(0, CAUTERY, "/obj/item/cautery");
        s.operate("Прижигание сосудов", |s| {
            s.get::<Limbs>(s.players[1].mob)
                .limb(BodyZone::RArm)
                .wound(Series::Pierce)
                .is_none_or(|w| !w.mend_state)
        });
    }

    // Revival: a dead B with an opened skull, a shock from wielded paddles.
    {
        let world = s.server.world_mut();
        world.entity_mut(a).remove::<DoAfter>();
        let mut health = world.get::<Health>(b).unwrap().clone();
        health.adjust(DamageType::Oxy, 200.0 - health.oxy);
        world.entity_mut(b).insert((health, Stat::Dead));
    }
    s.limbs(|l| {
        let head = l.limb_mut(BodyZone::Head);
        head.surgery = state::SKIN_OPEN | state::BONE_SAWED | state::VESSELS_CLAMPED;
    });
    let brain = s.get::<OrganDecay>(b).brain;
    let defib = s.take(0, DEFIB, "/obj/item/defibrillator");
    s.send(0, ClientIntent::ClickSlot(InventorySlot::Back));
    s.run_until("the defibrillator is worn", |s| {
        s.get::<Inventory>(a).slot(InventorySlot::Back) == Some(defib)
    });
    s.send(0, ClientIntent::ClickSlot(InventorySlot::Back));
    s.run_until("the paddles are out", |s| {
        s.get::<Defib>(defib).paddles.is_some()
    });
    s.zone(0, TargetZone::Head);
    s.operate("Дефибрилляция мозга", |s| {
        *s.get::<Stat>(s.players[1].mob) != Stat::Dead
    });
    assert!(s.said(0, "искру жизни"));
    let after = s.get::<OrganDecay>(b).brain;
    assert!(
        (after - (brain + 15.0)).abs() < 3.0,
        "15 brain damage: {brain} -> {after}"
    );
}
