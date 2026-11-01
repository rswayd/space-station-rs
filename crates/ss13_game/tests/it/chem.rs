//! Chemistry over real WebTransport on 127.0.0.1: pour from a bottle into a beaker, mix
//! hydrogen and nitrogen into ammonia (tg's step reaction, with its mix message), drink water,
//! cycle a transfer amount, and inject libital with a syringe into a hurt mob, whose brute
//! damage then goes down at tg's rate (3 per `SSmobs` tick).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::{Chem, MobReagents, Reacting, Reagents, TransferAmount, blood_amount};
use ss13_game::combat::NextMove;
use ss13_game::health::{DamageType, Health};
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_game::zone::BodyZone;
use ss13_mapload::DmItem;
use ss13_mapload::convert::DmTypes;
use ss13_net::chat::client::ChatReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot};
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_world::SpatialIndex;

/// tg's vars for what the test uses (`code/modules/reagents`), trimmed to the ones read.
const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1,
        "inverse_chem": { "path": "/datum/reagent/inverse" }, "inverse_chem_val": 0.25,
        "purge_multiplier": 1 } },
    "/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Water", "color": "#AAAAAA77", "chemical_flags": 144 } },
    "/datum/reagent/hydrogen": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Hydrogen", "color": "#808080", "ph": 0.1, "chemical_flags": 16 } },
    "/datum/reagent/nitrogen": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Nitrogen", "color": "#808080", "chemical_flags": 16 } },
    "/datum/reagent/ammonia": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Ammonia", "color": "#404030", "ph": 11.6, "chemical_flags": 16 } },
    "/datum/reagent/medicine": { "parent": "/datum/reagent", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent/medicine" } } },
    "/datum/reagent/medicine/c2": { "parent": "/datum/reagent/medicine", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent/medicine/c2" }, "metabolization_rate": 0.1,
        "creation_purity": 0.75, "purity": 0.75, "inverse_chem_val": 0, "inverse_chem": null,
        "chemical_flags": 128 } },
    "/datum/reagent/medicine/c2/libital": { "parent": "/datum/reagent/medicine/c2",
        "declared_vars": { "name": "Libital", "color": "#ECEC8D", "ph": 8.2,
        "inverse_chem": { "path": "/datum/reagent/inverse/libitoil" }, "inverse_chem_val": 0.3,
        "chemical_flags": 16 } },
    "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
        "H_ion_release": 0.01, "determin_ph_range": 4, "is_cold_recipe": 0,
        "mix_message": "The solution begins to bubble.", "mob_react": 1, "optimal_ph_max": 9,
        "optimal_ph_min": 5, "optimal_temp": 500, "overheat_temp": 900, "ph_exponent_factor": 2,
        "purity_min": 0.15, "rate_up_lim": 30, "reaction_flags": 0,
        "required_catalysts": { "list": [] }, "required_container": null,
        "required_container_accepts_subtypes": 0, "required_reagents": { "list": [] },
        "required_temp": 100, "results": { "list": [] }, "temp_exponent_factor": 2,
        "thermic_constant": 0 } },
    "/datum/chemical_reaction/ammonia": { "parent": "/datum/chemical_reaction",
        "declared_vars": {
        "results": { "list": [{ "assoc": [{ "path": "/datum/reagent/ammonia" }, 3] }] },
        "required_reagents": { "list": [
            { "assoc": [{ "path": "/datum/reagent/hydrogen" }, 3] },
            { "assoc": [{ "path": "/datum/reagent/nitrogen" }, 1] }] } } },
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
    "/obj/item/reagent_containers": { "parent": "/obj/item", "declared_vars": {
        "name": "Container", "icon": { "resource": "c.dmi" },
        "amount_per_transfer_from_this": 5, "has_variable_transfer_amount": 1,
        "possible_transfer_amounts": { "list": [5, 10, 15, 20, 25, 30] }, "volume": 30,
        "initial_reagent_flags": 0, "list_reagents": null, "list_reagents_purity": null,
        "fill_icon_thresholds": null, "fill_icon_state": null,
        "fill_icon": { "resource": "f.dmi" } } },
    "/obj/item/reagent_containers/cup": { "parent": "/obj/item/reagent_containers",
        "declared_vars": { "amount_per_transfer_from_this": 10,
        "possible_transfer_amounts": { "list": [5, 10, 15, 20, 25, 30, 50] }, "volume": 50,
        "initial_reagent_flags": 60, "gulp_size": 5, "icon_state": "bottle" } },
    "/obj/item/reagent_containers/cup/beaker": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "name": "beaker", "icon_state": "beaker",
        "fill_icon_thresholds": { "list": [0, 1, 20, 40, 60, 80, 100] } } },
    "/obj/item/reagent_containers/cup/bottle": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "name": "bottle", "fill_icon_state": "bottle",
        "fill_icon_thresholds": { "list": [0, 1, 20, 40, 60, 80, 100] },
        "possible_transfer_amounts": { "list": [5, 10, 15, 25, 50] } } },
    "/obj/item/reagent_containers/cup/bottle/hydrogen": {
        "parent": "/obj/item/reagent_containers/cup/bottle", "declared_vars": {
        "name": "hydrogen bottle",
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/hydrogen" }, 30] }] } } },
    "/obj/item/reagent_containers/cup/bottle/nitrogen": {
        "parent": "/obj/item/reagent_containers/cup/bottle", "declared_vars": {
        "name": "nitrogen bottle",
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/nitrogen" }, 30] }] } } },
    "/obj/item/reagent_containers/cup/bottle/libital": {
        "parent": "/obj/item/reagent_containers/cup/bottle", "declared_vars": {
        "name": "libital bottle",
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/medicine/c2/libital" }, 30] }] } } },
    "/obj/item/reagent_containers/cup/glass": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "icon_state": "glass_empty" } },
    "/obj/item/reagent_containers/cup/glass/drinkingglass": {
        "parent": "/obj/item/reagent_containers/cup/glass", "declared_vars": {
        "name": "drinking glass", "fill_icon_state": "drinking_glass",
        "fill_icon_thresholds": { "list": [0] } } },
    "/obj/item/reagent_containers/cup/glass/drinkingglass/filled": {
        "parent": "/obj/item/reagent_containers/cup/glass/drinkingglass" },
    "/obj/item/reagent_containers/cup/glass/drinkingglass/filled/half_full": {
        "parent": "/obj/item/reagent_containers/cup/glass/drinkingglass/filled", "declared_vars": {
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/water" }, 25] }] } } },
    "/obj/item/reagent_containers/syringe": { "parent": "/obj/item/reagent_containers",
        "declared_vars": { "name": "syringe", "icon": { "resource": "s.dmi" },
        "icon_state": "syringe_0", "amount_per_transfer_from_this": 5,
        "possible_transfer_amounts": { "list": [5, 10, 15] }, "volume": 15,
        "initial_reagent_flags": 32 } },
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

/// Rows top-down, the last line is y = 1. Spawns `s`: (3,3) for A, (3,4) for B. Around A:
/// hydrogen `H` east, nitrogen `N` west, beaker `B` south, water glass `W` south-east; a
/// syringe `S` north-east and a libital bottle `L` north-west.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"H" = (/obj/item/reagent_containers/cup/bottle/hydrogen,/turf/open/floor,/area/space)
"N" = (/obj/item/reagent_containers/cup/bottle/nitrogen,/turf/open/floor,/area/space)
"B" = (/obj/item/reagent_containers/cup/beaker,/turf/open/floor,/area/space)
"W" = (/obj/item/reagent_containers/cup/glass/drinkingglass/filled/half_full,/turf/open/floor,/area/space)
"S" = (/obj/item/reagent_containers/syringe,/turf/open/floor,/area/space)
"L" = (/obj/item/reagent_containers/cup/bottle/libital,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffffffw
wLsSfffw
wNsHfffw
wfBWfffw
wwwwwwww
"}
"##;

const SPAWN_A: TilePos = TilePos::new(3, 3, 1);
const SPAWN_B: TilePos = TilePos::new(3, 4, 1);
const HYDROGEN: TilePos = TilePos::new(4, 3, 1);
const NITROGEN: TilePos = TilePos::new(2, 3, 1);
const BEAKER: TilePos = TilePos::new(3, 2, 1);
const GLASS: TilePos = TilePos::new(4, 2, 1);
const SYRINGE: TilePos = TilePos::new(4, 4, 1);
const LIBITAL: TilePos = TilePos::new(2, 4, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 11] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("c.dmi", "beaker"),
    ("c.dmi", "bottle"),
    ("f.dmi", "beaker20"),
    ("s.dmi", "syringe_0"),
    ("s.dmi", "syringe_15"),
    ("f.dmi", "syringe15"),
    ("c.dmi", "glass_empty"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("chem_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/reagent_containers/cup/bottle/hydrogen",
        "/obj/item/reagent_containers/cup/bottle/nitrogen",
        "/obj/item/reagent_containers/cup/bottle/libital",
        "/obj/item/reagent_containers/cup/beaker",
        "/obj/item/reagent_containers/cup/glass/drinkingglass/filled/half_full",
        "/obj/item/reagent_containers/syringe",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let dir = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    dir
}

#[derive(Resource, Default)]
struct Received(Vec<String>);

fn record(mut chat: MessageReader<ChatReceived>, mut received: ResMut<Received>) {
    for ChatReceived(message) in chat.read() {
        received.0.push(message.plain_text());
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

    /// Clicks `entity` once `p`'s click cooldown is over.
    fn click_with(&mut self, p: usize, entity: Entity, modifiers: ClickModifiers) {
        let mob = self.players[p].mob;
        self.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(mob).0);
        self.0.click_with(p, entity, modifiers);
    }

    fn click(&mut self, p: usize, entity: Entity) {
        self.click_with(p, entity, ClickModifiers::default());
    }

    /// The active hand's slot: a click on it is the held item's `attack_self`.
    fn click_active_slot(&mut self, p: usize) {
        let slot = if self.get::<Inventory>(self.players[p].mob).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        };
        self.send(p, ClientIntent::ClickSlot(slot));
    }

    fn said(&self, p: usize, text: &str) -> bool {
        self.players[p]
            .app
            .world()
            .resource::<Received>()
            .0
            .iter()
            .any(|line| line.contains(text))
    }

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

    fn amount(&self, container: Entity, path: &str) -> f32 {
        let id = self.server.world().resource::<Chem>().0.id(path).unwrap();
        self.get::<Reagents>(container).0.amount(id)
    }

    fn take(&mut self, p: usize, item: Entity) {
        let mob = self.players[p].mob;
        self.click(p, item);
        self.run_until("picked up", |s| {
            s.get::<Inventory>(mob).active_item() == Some(item)
        });
    }

    fn drop_active(&mut self, p: usize) {
        let mob = self.players[p].mob;
        self.send(p, ClientIntent::DropActive);
        self.run_until("dropped", |s| {
            s.get::<Inventory>(mob).active_item().is_none()
        });
    }
}

#[test]
fn pour_react_and_drink() {
    let mut s = Station::start("pour_react_and_drink", 1);
    let a = s.players[0].mob;
    assert_eq!(*s.get::<TilePos>(a), SPAWN_A);
    let (hydrogen, nitrogen, beaker, glass) = (
        s.item_on(HYDROGEN),
        s.item_on(NITROGEN),
        s.item_on(BEAKER),
        s.item_on(GLASS),
    );
    assert_eq!(
        s.amount(hydrogen, "/datum/reagent/hydrogen"),
        30.0,
        "list_reagents"
    );
    assert_eq!(s.get::<Reagents>(beaker).0.maximum_volume, 50.0);
    assert!(
        s.get::<Appearance>(beaker).overlays.is_empty(),
        "empty: no fill"
    );

    // Pour 10 hydrogen (a bottle's `amount_per_transfer_from_this`) into the beaker.
    s.take(0, hydrogen);
    s.click(0, beaker);
    s.run_until("poured", |s| {
        s.amount(beaker, "/datum/reagent/hydrogen") == 10.0
    });
    assert_eq!(s.amount(hydrogen, "/datum/reagent/hydrogen"), 20.0);
    s.run_until("A reads it", |s| {
        s.said(0, "Вы переливаете 10 ед. раствора в beaker.")
    });
    // 10 of 50 units = 20%: `beaker20`, tinted like hydrogen.
    s.run_until("the fill overlay", |s| {
        s.get::<Appearance>(beaker).overlays.len() == 1
    });
    let overlay = s.get::<Appearance>(beaker).overlays[0].clone();
    assert_eq!(overlay.icon_state, IconStateId(6), "beaker20");
    assert_eq!(overlay.color, Some([0x80, 0x80, 0x80, 255]));

    // `attack_self` cycles the bottle's transfer amount: 10 -> 15.
    s.click_active_slot(0);
    s.run_until("15 per transfer", |s| {
        s.get::<TransferAmount>(hydrogen).0 == 15.0
    });
    s.run_until("A reads it", |s| s.said(0, "Переливание по 15 ед."));
    s.drop_active(0);

    // 10 nitrogen in: 3 hydrogen + 1 nitrogen -> 3 ammonia over tg's reaction steps.
    s.take(0, nitrogen);
    s.click(0, beaker);
    s.run_until("ammonia forms", |s| {
        s.amount(beaker, "/datum/reagent/ammonia") > 0.0
    });
    s.run_until("the beaker reacts", |s| {
        s.server.world().get::<Reacting>(beaker).is_some()
    });
    s.run_until_for(20.0, "the reaction ends", |s| {
        s.server.world().get::<Reacting>(beaker).is_none()
    });
    // Hydrogen is the limit: all 10 become 10 ammonia; 10 - 10/3 nitrogen is left.
    let ammonia = s.amount(beaker, "/datum/reagent/ammonia");
    assert!((ammonia - 10.0).abs() < 0.05, "{ammonia}");
    assert!(s.amount(beaker, "/datum/reagent/hydrogen") < 0.05);
    let left = s.amount(beaker, "/datum/reagent/nitrogen");
    assert!((left - 6.67).abs() < 0.05, "{left}");
    s.run_until("the mix message is heard", |s| {
        s.said(0, "The solution begins to bubble.")
    });
    s.drop_active(0);

    // Drink water: a gulp of 5 into the stomach, then into the blood over Life ticks.
    s.take(0, glass);
    s.click(0, a);
    s.run_until("a gulp", |s| {
        s.amount(glass, "/datum/reagent/water") == 20.0
    });
    s.run_until("A reads it", |s| s.said(0, "Вы делаете глоток из "));
    let water = |s: &Station| {
        let id = s
            .server
            .world()
            .resource::<Chem>()
            .0
            .id("/datum/reagent/water")
            .unwrap();
        let m = s.get::<MobReagents>(a);
        (m.stomach.amount(id), m.blood.amount(id))
    };
    let (stomach, _) = water(&s);
    assert!(stomach > 0.0 && stomach <= 5.0, "{stomach}");
    s.run_until_for(10.0, "water reaches the blood", |s| water(s).1 > 0.0);
}

#[test]
fn syringe_injects_libital_and_heals() {
    let mut s = Station::start("syringe_injects_libital_and_heals", 2);
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert_eq!(
        (*s.get::<TilePos>(a), *s.get::<TilePos>(b)),
        (SPAWN_A, SPAWN_B)
    );
    {
        let world = s.server.world_mut();
        let mut health = world.get::<Health>(b).unwrap().clone();
        health.damage_zone(BodyZone::Chest, DamageType::Brute, 30.0);
        world.entity_mut(b).insert(health);
    }
    let (syringe, bottle) = (s.item_on(SYRINGE), s.item_on(LIBITAL));
    s.take(0, syringe);
    // Draw 15 (cycle 5 -> 10 -> 15).
    s.click_active_slot(0);
    s.run_until("10", |s| s.get::<TransferAmount>(syringe).0 == 10.0);
    s.click_active_slot(0);
    s.run_until("15", |s| s.get::<TransferAmount>(syringe).0 == 15.0);
    s.click_with(
        0,
        bottle,
        ClickModifiers {
            right: true,
            ..default()
        },
    );
    s.run_until("drawn", |s| {
        s.amount(syringe, "/datum/reagent/medicine/c2/libital") == 15.0
    });
    s.run_until("A reads it", |s| {
        s.said(
            0,
            "Вы наполняете syringe 15 ед. раствора. Теперь в нём 15 ед.",
        )
    });
    s.run_until("the syringe looks full", |s| {
        s.get::<Appearance>(syringe).icon_state == IconStateId(8)
    });

    // Inject B: 3 s, then 15 units of libital in B's blood.
    s.click(0, b);
    s.run_until("the injection starts", |s| {
        s.server.world().get::<DoAfter>(a).is_some()
    });
    let started = s.now();
    let libital =
        |s: &Station| blood_amount(s.server.world(), b, "/datum/reagent/medicine/c2/libital");
    s.run_until("B is injected", |s| libital(s) > 0.0);
    assert!(s.now() - started >= 2.9, "after 3 s: {}", s.now() - started);
    s.run_until("B reads it", |s| s.said(1, "делает укол"));
    assert_eq!(s.amount(syringe, "/datum/reagent/medicine/c2/libital"), 0.0);

    // Libital: 0.2 units and 3 brute per SSmobs tick (2 s).
    let brute = |s: &Station| s.get::<Health>(b).brute;
    let before = brute(&s);
    s.run_until_for(10.0, "the first tick heals", |s| brute(s) < before);
    let (first, units) = (brute(&s), libital(&s));
    s.run_until_for(10.0, "the next tick heals", |s| brute(s) < first);
    assert!(
        (first - brute(&s) - 3.0).abs() < 0.01,
        "{first} -> {}",
        brute(&s)
    );
    assert!(
        (units - libital(&s) - 0.2).abs() < 1e-3,
        "{units} -> {}",
        libital(&s)
    );
}
