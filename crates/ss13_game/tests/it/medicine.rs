//! Advanced medicine over real WebTransport on 127.0.0.1: B cuts its own arm with a scalpel and
//! bleeds; A takes a blood sample with a syringe; A hangs a blood pack on an IV drip and drags
//! the drip onto B, whose blood volume rises; on B's prepared, opened chest A's hemostat offers
//! two operations through the `ListInputWindow` and the picked tend-wounds heals; B dies and A's
//! defibrillator paddles bring it back to crit.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::{Chem, Reagents};
use ss13_game::combat::{CombatMode, NextMove};
use ss13_game::health::{DamageType, Health, Stat, ZoneSelected};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::medical::blood::{Blood, IvDrip};
use ss13_game::medical::defib::Defib;
use ss13_game::medical::wounds::{Limbs, Series, state};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_game::zone::{BodyZone, TargetZone};
use ss13_mapload::DmItem;
use ss13_mapload::convert::DmTypes;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd, InventorySlot, NetId};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_world::SpatialIndex;

/// tg's vars for what the test uses, trimmed to the ones read.
const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1,
        "inverse_chem": null, "inverse_chem_val": 0, "purge_multiplier": 1 } },
    "/datum/reagent/blood": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Blood", "color": "#C80000", "metabolization_rate": 2.5, "ph": 7.4 } },
    "/datum/reagent/toxin": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Toxin", "color": "#CF3600" } },
    "/datum/reagent/iron": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Iron", "color": "#606060" } },
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
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/iv_drip": { "parent": "/obj/machinery", "declared_vars": {
        "name": "IV drip", "icon": { "resource": "iv.dmi" }, "icon_state": "iv_drip",
        "anchored": 0 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "sharpness": 0, "item_flags": 0,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/scalpel": { "parent": "/obj/item", "declared_vars": { "name": "scalpel",
        "icon_state": "scalpel", "force": 10, "sharpness": 1, "wound_bonus": 10,
        "exposed_wound_bonus": 15, "item_flags": 1024, "tool_behaviour": "scalpel" } },
    "/obj/item/hemostat": { "parent": "/obj/item", "declared_vars": { "name": "hemostat",
        "icon_state": "hemostat", "item_flags": 1024, "tool_behaviour": "hemostat" } },
    "/obj/item/defibrillator": { "parent": "/obj/item", "declared_vars": {
        "name": "defibrillator", "icon_state": "defibunit", "slot_flags": 1024, "combat": 0,
        "cooldown_duration": 50, "paddle_type": { "path": "/obj/item/shockpaddles" } } },
    "/obj/item/defibrillator/loaded": { "parent": "/obj/item/defibrillator" },
    "/obj/item/shockpaddles": { "parent": "/obj/item", "declared_vars": {
        "name": "defibrillator paddles", "icon_state": "defibpaddles0" } },
    "/obj/item/reagent_containers": { "parent": "/obj/item", "declared_vars": {
        "name": "Container", "icon": { "resource": "c.dmi" },
        "amount_per_transfer_from_this": 5, "has_variable_transfer_amount": 1,
        "possible_transfer_amounts": { "list": [5, 10, 15] }, "volume": 30,
        "initial_reagent_flags": 0, "list_reagents": null, "list_reagents_purity": null,
        "fill_icon_thresholds": null, "fill_icon_state": null, "blood_type": null,
        "fill_icon": { "resource": "f.dmi" } } },
    "/obj/item/reagent_containers/syringe": { "parent": "/obj/item/reagent_containers",
        "declared_vars": { "name": "syringe", "icon": { "resource": "s.dmi" },
        "icon_state": "syringe_0", "volume": 15, "initial_reagent_flags": 32 } },
    "/obj/item/reagent_containers/blood": { "parent": "/obj/item/reagent_containers",
        "declared_vars": { "name": "blood pack", "icon_state": "bloodpack", "volume": 200 } },
    "/obj/item/reagent_containers/blood/o_minus": {
        "parent": "/obj/item/reagent_containers/blood", "declared_vars": {
        "blood_type": { "path": "/datum/blood_type/human/o_minus" } } },
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

/// Rows top-down, the last line is y = 1. Spawns `s` (3,3) for A then (3,4) for B; around A a
/// scalpel `X`, a syringe `S`, a blood pack `P`, an IV drip `I`, a hemostat `H` and a
/// defibrillator `D`.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"X" = (/obj/item/scalpel,/turf/open/floor,/area/space)
"S" = (/obj/item/reagent_containers/syringe,/turf/open/floor,/area/space)
"P" = (/obj/item/reagent_containers/blood/o_minus,/turf/open/floor,/area/space)
"I" = (/obj/machinery/iv_drip,/turf/open/floor,/area/space)
"H" = (/obj/item/hemostat,/turf/open/floor,/area/space)
"D" = (/obj/item/defibrillator/loaded,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffXfffw
wHsIfffw
wDsPfffw
wfSffffw
wwwwwwww
"}
"##;

const SCALPEL: TilePos = TilePos::new(4, 5, 1);
const SYRINGE: TilePos = TilePos::new(3, 2, 1);
const PACK: TilePos = TilePos::new(4, 3, 1);
const DRIP: TilePos = TilePos::new(4, 4, 1);
const HEMOSTAT: TilePos = TilePos::new(2, 4, 1);
const DEFIB: TilePos = TilePos::new(2, 3, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 11] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "scalpel"),
    ("i.dmi", "hemostat"),
    ("i.dmi", "defibunit"),
    ("i.dmi", "defibpaddles0"),
    ("s.dmi", "syringe_0"),
    ("c.dmi", "bloodpack"),
    ("iv.dmi", "iv_drip"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("medicine_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/scalpel",
        "/obj/item/hemostat",
        "/obj/item/defibrillator/loaded",
        "/obj/item/shockpaddles",
        "/obj/item/reagent_containers/syringe",
        "/obj/item/reagent_containers/blood/o_minus",
        "/obj/machinery/iv_drip",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    dir
}

/// What a client received: chat lines and the last `ListInputWindow` (id, data).
#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    list: Option<(u32, ListInputData)>,
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
            window,
            interface,
            data,
            ..
        } = message
            && interface == ListInputUi::NAME
        {
            received.list = Some((*window, ss13_tgui::decode(data).unwrap()));
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

    /// `entity`'s id for `p`; it must be replicated.
    fn net_id(&self, p: usize, entity: Entity) -> NetId {
        self.0.net_id(p, entity).expect("replicated")
    }

    fn click_with(&mut self, p: usize, entity: Entity, modifiers: ClickModifiers) {
        let mob = self.players[p].mob;
        self.run_until("off cooldown", |s| {
            s.server.world().resource::<Time>().elapsed_secs_f64() >= s.get::<NextMove>(mob).0
        });
        let target = self.net_id(p, entity);
        self.send(p, ClientIntent::Click { target, modifiers });
    }

    fn click(&mut self, p: usize, entity: Entity) {
        self.click_with(p, entity, ClickModifiers::default());
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

    fn item_on(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some() || world.get::<IvDrip>(e).is_some())
            .unwrap()
    }

    /// Picks up the item at `pos` into `p`'s active hand (after dropping what it holds).
    fn take(&mut self, p: usize, pos: TilePos) -> Entity {
        let mob = self.players[p].mob;
        if self.get::<Inventory>(mob).active_item().is_some() {
            self.send(p, ClientIntent::DropActive);
            self.run_until("the hand is empty", |s| {
                s.get::<Inventory>(mob).active_item().is_none()
            });
        }
        let item = self.item_on(pos);
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

    fn blood(&self, p: usize) -> f32 {
        self.get::<Blood>(self.players[p].mob).volume
    }
}

#[test]
fn bleeding_blood_iv_surgery_and_defib() {
    let mut s = Station::start("bleeding_blood_iv_surgery_and_defib", 2);
    let (a, b) = (s.players[0].mob, s.players[1].mob);

    // B cuts its own left arm: a scalpel (10, +10, +15 exposed) always cuts an arm.
    s.take(1, SCALPEL);
    s.zone(1, TargetZone::LArm);
    s.send(1, ClientIntent::ToggleCombatMode);
    s.run_until("combat mode", |s| s.get::<CombatMode>(b).0);
    s.click(1, b);
    s.run_until("a cut", |s| {
        s.get::<Limbs>(b)
            .limb(BodyZone::LArm)
            .wound(Series::Slash)
            .is_some()
    });
    s.run_until("the victim reads the wound", |s| {
        s.said(1, "Ваша левая рука")
    });
    let before = s.blood(1);
    s.run_until("B bleeds", |s| s.blood(1) < before - 1.0);
    s.send(1, ClientIntent::ToggleCombatMode);
    s.run_until("combat mode off", |s| !s.get::<CombatMode>(b).0);

    // A takes a blood sample: 3 s, then 15 units with B's type.
    let syringe = s.take(0, SYRINGE);
    s.zone(0, TargetZone::LArm);
    let before = s.blood(1);
    s.click_with(
        0,
        b,
        ClickModifiers {
            right: true,
            ..default()
        },
    );
    s.run_until("the sample starts", |s| {
        s.server.world().get::<DoAfter>(a).is_some()
    });
    s.run_until("the syringe is full", |s| {
        s.get::<Reagents>(syringe).0.total_volume >= 15.0
    });
    assert!(s.blood(1) <= before - 14.0, "{before} -> {}", s.blood(1));
    {
        let world = s.server.world();
        let chem = world.resource::<Chem>();
        let id = chem.0.id("/datum/reagent/blood").unwrap();
        let reagent = world
            .get::<Reagents>(syringe)
            .unwrap()
            .0
            .has(id)
            .unwrap()
            .clone();
        let blood_type = world.get::<Blood>(b).unwrap().blood_type;
        assert_eq!(reagent.data.as_deref(), Some(blood_type.path()));
    }
    s.run_until("A reads the sample", |s| s.said(0, "takes a blood sample"));

    // The cut stops bleeding for now; B loses blood to 400 and gets an IV of O-.
    {
        let world = s.server.world_mut();
        let mut limbs = world.get::<Limbs>(b).unwrap().clone();
        limbs.limb_mut(BodyZone::LArm).wounds.clear();
        world.entity_mut(b).insert(limbs);
        world.get_mut::<Blood>(b).unwrap().volume = 400.0;
    }
    let pack = s.take(0, PACK);
    let drip = s.item_on(DRIP);
    s.click(0, drip);
    s.run_until("the pack hangs on the drip", |s| {
        s.get::<IvDrip>(drip).container == Some(pack)
    });
    let (from, to) = (s.net_id(0, drip), s.net_id(0, b));
    let drag = ClientIntent::Drag {
        from: DragEnd::Atom(from),
        to: DragEnd::Atom(to),
        modifiers: ClickModifiers::default(),
    };
    // Cuffed hands attach nothing (`interaction_flags_mouse_drop = NEED_HANDS`).
    let cuffs = s.server.world_mut().spawn_empty().id();
    s.server
        .world_mut()
        .entity_mut(a)
        .insert(ss13_game::restraints::Handcuffed(cuffs));
    s.run_until("A's hands are blocked", |s| {
        s.get::<ss13_game::status_effects::Incapacitation>(a)
            .hands_blocked
    });
    s.send(0, drag.clone());
    s.hold(0.5, "cuffed A starts no attaching", |s| {
        s.server.world().get::<DoAfter>(a).is_none()
    });
    s.server
        .world_mut()
        .entity_mut(a)
        .remove::<ss13_game::restraints::Handcuffed>();
    s.server.world_mut().despawn(cuffs);
    s.send(0, drag);
    s.run_until("the drip is attached", |s| {
        s.get::<IvDrip>(drip).attached == Some(b)
    });
    s.run_until_for(10.0, "blood flows in", |s| s.blood(1) > 405.0);
    assert!(s.get::<Reagents>(pack).0.total_volume < 200.0);

    // Surgery: B lies on the floor with a prepared, opened chest and some bruises.
    {
        let world = s.server.world_mut();
        let mut drip_state = world.get::<IvDrip>(drip).unwrap().clone();
        drip_state.attached = None;
        world.entity_mut(drip).insert(drip_state);
        let mut limbs = world.get::<Limbs>(b).unwrap().clone();
        let chest = limbs.limb_mut(BodyZone::Chest);
        chest.prepped = true;
        chest.surgery = state::SKIN_OPEN | state::VESSELS_UNCLAMPED;
        world.entity_mut(b).insert(limbs);
        let mut health = world.get::<Health>(b).unwrap().clone();
        health.damage_zone(BodyZone::Chest, DamageType::Brute, 30.0);
        world.entity_mut(b).insert(health);
        ss13_game::status_effects::apply(
            world,
            b,
            ss13_game::status_effects::Incap::Paralyze,
            60.0,
        );
    }
    s.run_until("B lies", |s| {
        s.get::<ss13_game::health::BodyPosition>(b).lying()
    });
    s.take(0, HEMOSTAT);
    s.zone(0, TargetZone::Chest);
    s.click(0, b);
    s.run_until("the operation list opens", |s| {
        s.players[0]
            .app
            .world()
            .resource::<Received>()
            .list
            .is_some()
    });
    let (window, list) = s.players[0]
        .app
        .world()
        .resource::<Received>()
        .list
        .clone()
        .unwrap();
    assert_eq!(list.items, ["Зажать сосуды", "Обработка ушибов"]);
    // A surgeon stunned while the list is open picks nothing, and the patient feels nothing.
    ss13_game::status_effects::apply(
        s.server.world_mut(),
        a,
        ss13_game::status_effects::Incap::Stun,
        3.0,
    );
    s.send(
        0,
        ClientIntent::UiAct {
            window,
            action: "submit".into(),
            params: vec![("entry".into(), "Обработка ушибов".into())],
        },
    );
    s.hold(1.0, "a stunned surgeon starts no operation", |s| {
        s.server.world().get::<DoAfter>(a).is_none() && !s.said(1, "острую боль")
    });
    s.run_until("A can act again", |s| {
        !s.get::<ss13_game::status_effects::Incapacitation>(a)
            .incapacitated
    });
    s.players[0].app.world_mut().resource_mut::<Received>().list = None;
    s.click(0, b);
    s.run_until("the operation list opens again", |s| {
        s.players[0]
            .app
            .world()
            .resource::<Received>()
            .list
            .is_some()
    });
    let (window, _) = s.players[0]
        .app
        .world()
        .resource::<Received>()
        .list
        .clone()
        .unwrap();
    let brute = s.get::<Health>(b).brute;
    s.send(
        0,
        ClientIntent::UiAct {
            window,
            action: "submit".into(),
            params: vec![("entry".into(), "Обработка ушибов".into())],
        },
    );
    s.run_until("tending starts", |s| {
        s.server.world().get::<DoAfter>(a).is_some()
    });
    // A conscious patient: 45% of the steps fail (BandaStation), and a failure hurts. Waits for
    // one healing step, not for a total below the start: a run of failures can keep the total
    // above it for the whole paralysis (~1% of runs at 20 steps; 0.45^20 for no success at all).
    let mut last = brute;
    s.run_until_for(60.0, "a tending step heals the bruises", |s| {
        let now = s.get::<Health>(b).brute;
        let healed = now < last;
        last = now;
        healed
    });
    // A stray entry is ignored: the pick is gone once used.
    s.send(
        0,
        ClientIntent::UiAct {
            window,
            action: "submit".into(),
            params: vec![("entry".into(), "Зажать сосуды".into())],
        },
    );
    for _ in 0..20 {
        s.step();
    }
    assert!(
        s.get::<Limbs>(b)
            .limb(BodyZone::Chest)
            .has_state(state::VESSELS_UNCLAMPED)
    );

    // B dies of suffocation; A's defibrillator (on its back) revives it into crit.
    {
        let world = s.server.world_mut();
        world.entity_mut(a).remove::<DoAfter>();
        let mut health = world.get::<Health>(b).unwrap().clone();
        health.adjust(DamageType::Oxy, 200.0);
        world.entity_mut(b).insert((health, Stat::Dead));
    }
    let defib = s.take(0, DEFIB);
    s.send(0, ClientIntent::ClickSlot(InventorySlot::Back));
    s.run_until("the defibrillator is worn", |s| {
        s.get::<Inventory>(a).slot(InventorySlot::Back) == Some(defib)
    });
    s.send(0, ClientIntent::ClickSlot(InventorySlot::Back));
    s.run_until("the paddles are out", |s| {
        s.get::<Defib>(defib).paddles.is_some()
    });
    let paddles = s.get::<Defib>(defib).paddles.unwrap();
    assert_eq!(s.get::<Inventory>(a).active_item(), Some(paddles));
    assert!(s.server.world().get::<CarriedBy>(paddles).is_some());
    // Something taken into the other hand mid-shock unwields the paddles: the shock stops
    // (tg `extra_checks = is_wielded` on every stage).
    s.click(0, b);
    s.run_until("the shock starts", |s| {
        s.server.world().get::<DoAfter>(a).is_some()
    });
    let other = 1 - s.get::<Inventory>(a).active;
    let junk = s.server.world_mut().spawn_empty().id();
    s.server.world_mut().get_mut::<Inventory>(a).unwrap().hands[other] = Some(junk);
    s.run_until("the shock stops", |s| {
        s.server.world().get::<DoAfter>(a).is_none()
    });
    assert_eq!(*s.get::<Stat>(b), Stat::Dead);
    s.server.world_mut().get_mut::<Inventory>(a).unwrap().hands[other] = None;
    s.server.world_mut().despawn(junk);
    s.click(0, b);
    s.run_until_for(10.0, "B is alive again", |s| {
        *s.get::<Stat>(b) != Stat::Dead
    });
    let health = s.get::<Health>(b).health();
    assert!((health + 50.0).abs() < 0.5, "halfway to death: {health}");
    assert_eq!(*s.get::<Stat>(b), Stat::HardCrit);
    s.run_until("A reads the ping", |s| {
        s.said(0, "Resuscitation successful")
    });
    let charge = s.get::<Defib>(defib).cell.unwrap().0;
    assert_eq!(charge, 9000.0, "one shock used");
}
