//! Hydroponics over real WebTransport on 127.0.0.1: a tomato seed planted in a tray, watered
//! and fed from containers, grows through its stages (hydroponics timers sped up with tg's
//! ratios kept), is harvested by hand into tomatoes whose nutriment follows their potency, is
//! scanned with a plant analyzer, and a tomato in the seed extractor gives seeds with its stats.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::botany::{HydroSpeed, PlantStatus, Seed, Tray};
use ss13_game::chem::{Chem, Reagents};
use ss13_game::combat::NextMove;
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmItem, MapLoadPlugin};
use ss13_net::chat::client::ChatReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::{Appearance, TilePos};
use ss13_world::SpatialIndex;

const EQUIPMENT: &str = "icons/obj/service/hydroponics/equipment.dmi";
const GROWING: &str = "icons/obj/service/hydroponics/growing_fruits.dmi";

/// tg's vars for what the test uses, trimmed to the ones read.
const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1, "inverse_chem": null,
        "inverse_chem_val": 0.25, "purge_multiplier": 1 } },
    "/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Water", "color": "#AAAAAA77" } },
    "/datum/reagent/consumable": { "parent": "/datum/reagent", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent/consumable" }, "nutriment_factor": 1 } },
    "/datum/reagent/consumable/nutriment": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Nutriment", "nutriment_factor": 15 } },
    "/datum/reagent/consumable/nutriment/vitamin": {
        "parent": "/datum/reagent/consumable/nutriment", "declared_vars": { "name": "Vitamin" } },
    "/datum/reagent/plantnutriment": { "parent": "/datum/reagent", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent/plantnutriment" } } },
    "/datum/reagent/plantnutriment/eznutriment": { "parent": "/datum/reagent/plantnutriment",
        "declared_vars": { "name": "E-Z-Nutrient" } },
    "/datum/plant_gene": { "parent": "/datum", "declared_vars": {
        "name": null, "description": "", "mutability_flags": null } },
    "/datum/plant_gene/trait": { "parent": "/datum/plant_gene", "declared_vars": {
        "rate": 0.05, "trait_ids": null, "trait_flags": 0, "seed_blacklist": null } },
    "/datum/plant_gene/trait/repeated_harvest": { "parent": "/datum/plant_gene/trait",
        "declared_vars": { "name": "Perennial Growth", "mutability_flags": 7 } },
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
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "use_power": 0,
        "idle_power_usage": 0, "active_power_usage": 0, "power_channel": 1 } },
    "/obj/machinery/hydroponics": { "parent": "/obj/machinery", "vars": { "name": "hydroponics tray" },
        "declared_vars": { "name": "hydroponics tray",
        "icon": { "resource": "icons/obj/service/hydroponics/equipment.dmi" },
        "icon_state": "hydrotray", "density": 1, "anchored": 1, "maxwater": 100,
        "maxnutri": 20, "rating": 1, "tray_flags": 1, "plant_offset_y": 0, "alt_tray": 0 } },
    "/obj/machinery/hydroponics/constructable": { "parent": "/obj/machinery/hydroponics",
        "declared_vars": { "icon_state": "hydrotray3" } },
    "/obj/machinery/seed_extractor": { "parent": "/obj/machinery", "vars": { "name": "seed extractor" },
        "declared_vars": { "name": "seed extractor",
        "icon": { "resource": "icons/obj/service/hydroponics/equipment.dmi" },
        "icon_state": "sextractor", "density": 1, "anchored": 1 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "tool_behaviour": null,
        "toolspeed": 1 } },
    "/obj/item/plant_analyzer": { "parent": "/obj/item", "vars": { "name": "plant analyzer" },
        "declared_vars": { "name": "plant analyzer" } },
    "/obj/item/seeds": { "parent": "/obj/item", "declared_vars": {
        "abstract_type": { "path": "/obj/item/seeds" }, "plantname": "Plants", "product": null,
        "species": "", "growing_icon": { "resource": "icons/obj/service/hydroponics/growing.dmi" },
        "icon_grow": null, "icon_dead": null, "icon_harvest": null, "plant_icon_offset": 8,
        "lifespan": 25, "endurance": 15, "maturation": 6, "production": 6, "yield": 3,
        "potency": 10, "growthstages": 6, "instability": 5, "rarity": 0, "mutatelist": null,
        "genes": { "list": [] }, "reagents_add": null, "weed_rate": 1, "weed_chance": 5,
        "seed_flags": 1 } },
    "/obj/item/seeds/tomato": { "parent": "/obj/item/seeds", "vars": { "name": "tomato seed pack" },
        "declared_vars": { "name": "tomato seed pack", "plantname": "Tomato Plants",
        "species": "tomato", "product": { "path": "/obj/item/food/grown/tomato" },
        "growing_icon": { "resource": "icons/obj/service/hydroponics/growing_fruits.dmi" },
        "maturation": 8, "instability": 25, "icon_grow": "tomato-grow",
        "icon_dead": "tomato-dead",
        "genes": { "list": [{ "path": "/datum/plant_gene/trait/repeated_harvest" }] },
        "reagents_add": { "list": [
            { "assoc": [{ "path": "/datum/reagent/consumable/nutriment/vitamin" }, 0.04] },
            { "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 0.1] }] } } },
    "/obj/item/food": { "parent": "/obj/item", "declared_vars": {
        "food_reagents": null, "max_volume": null, "bite_consumption": null,
        "eat_time": null, "tastes": null, "eatverbs": null, "trash_type": null,
        "junkiness": null, "food_flags": null, "starting_reagent_purity": null } },
    "/obj/item/food/grown": { "parent": "/obj/item/food", "declared_vars": {
        "abstract_type": { "path": "/obj/item/food/grown" }, "max_volume": 100,
        "bite_consumption_mod": 1, "seed": null } },
    "/obj/item/food/grown/tomato": { "parent": "/obj/item/food/grown", "vars": { "name": "tomato" },
        "declared_vars": { "name": "tomato", "seed": { "path": "/obj/item/seeds/tomato" } } },
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
    "/obj/item/reagent_containers/cup/watering_can": {
        "parent": "/obj/item/reagent_containers/cup", "vars": { "name": "watering can" },
        "declared_vars": { "name": "watering can", "volume": 100,
        "amount_per_transfer_from_this": 50,
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/water" }, 100] }] } } },
    "/obj/item/reagent_containers/cup/bottle": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "name": "bottle" } },
    "/obj/item/reagent_containers/cup/bottle/nutrient": {
        "parent": "/obj/item/reagent_containers/cup/bottle" },
    "/obj/item/reagent_containers/cup/bottle/nutrient/ez": {
        "parent": "/obj/item/reagent_containers/cup/bottle/nutrient",
        "vars": { "name": "E-Z-Nutrient bottle" }, "declared_vars": { "name": "E-Z-Nutrient bottle",
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/plantnutriment/eznutriment" }, 50] }] } } },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0, "static_lighting": 0 } },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (3,3); the seed extractor `X` west, the
/// tray `T` east; a watering can `W`, a tomato seed pack `S` and an E-Z-Nutrient bottle `N` on
/// the row below, a plant analyzer `A` north.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"X" = (/obj/machinery/seed_extractor,/turf/open/floor,/area/space)
"T" = (/obj/machinery/hydroponics/constructable,/turf/open/floor,/area/space)
"W" = (/obj/item/reagent_containers/cup/watering_can,/turf/open/floor,/area/space)
"S" = (/obj/item/seeds/tomato,/turf/open/floor,/area/space)
"N" = (/obj/item/reagent_containers/cup/bottle/nutrient/ez,/turf/open/floor,/area/space)
"A" = (/obj/item/plant_analyzer,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wfAfffw
wXsTffw
wWSNffw
wwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const EXTRACTOR: TilePos = TilePos::new(2, 3, 1);
const TRAY: TilePos = TilePos::new(4, 3, 1);
const CAN: TilePos = TilePos::new(2, 2, 1);
const SEEDS: TilePos = TilePos::new(3, 2, 1);
const BOTTLE: TilePos = TilePos::new(4, 2, 1);
const ANALYZER: TilePos = TilePos::new(3, 4, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 21] = [
    ("t.dmi", "floor"),
    ("c.dmi", "bottle"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    (EQUIPMENT, "hydrotray3"),
    (EQUIPMENT, "sextractor"),
    (EQUIPMENT, "over_lowwater3"),
    (EQUIPMENT, "over_lownutri3"),
    (EQUIPMENT, "over_lowhealth3"),
    (EQUIPMENT, "over_alert3"),
    (EQUIPMENT, "over_harvest3"),
    (GROWING, "tomato-grow1"),
    (GROWING, "tomato-grow2"),
    (GROWING, "tomato-grow3"),
    (GROWING, "tomato-grow4"),
    (GROWING, "tomato-grow5"),
    (GROWING, "tomato-grow6"),
    (GROWING, "tomato-harvest"),
    (GROWING, "tomato-dead"),
];

fn state_id(dmi: &str, state: &str) -> u32 {
    STATES
        .iter()
        .position(|&(d, s)| d == dmi && s == state)
        .unwrap() as u32
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("botany_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/seed_extractor",
        "/obj/machinery/hydroponics/constructable",
        "/obj/item/reagent_containers/cup/watering_can",
        "/obj/item/reagent_containers/cup/bottle/nutrient/ez",
        "/obj/item/seeds/tomato",
        "/obj/item/plant_analyzer",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let ron = std::fs::read_to_string(dir.join("prototypes/dm_types.ron")).unwrap();
    assert!(
        ron.contains("\"/obj/item/food/grown/tomato\""),
        "the plant closure brings the seed's product"
    );
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    let plants = ss13_mapload::plants::resolve(&types);
    std::fs::write(dir.join(ss13_mapload::plants::PLANTS_FILE), plants.to_ron()).unwrap();
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

impl Deref for Station {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.0
    }
}

impl DerefMut for Station {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.0
    }
}

impl Drive for Station {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.0
    }

    fn context(&self) -> String {
        let chat: Vec<_> = self
            .players
            .iter()
            .map(|p| &p.app.world().resource::<Received>().0)
            .collect();
        format!("; chat: {chat:?}")
    }
}

impl Station {
    fn start(test: &str, clients: usize) -> Self {
        let assets = fixture_assets(test);
        Self::start_on(fixture_map(&assets, &["map.dmm"]), clients)
    }

    fn start_on(map: MapLoadPlugin, clients: usize) -> Self {
        let mut s = testing::Station::start(map, |_| {});
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

    fn items_on(&self, pos: TilePos, path: &str) -> Vec<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
            .collect()
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
    fn tray(&self) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(TRAY)
            .iter()
            .copied()
            .find(|&e| world.get::<Tray>(e).is_some())
            .expect("a tray")
    }

    fn plant_overlay(&self, tray: Entity) -> Option<u32> {
        self.get::<Appearance>(tray)
            .overlays
            .iter()
            .map(|o| o.icon_state.0)
            .find(|&id| id >= state_id(GROWING, "tomato-grow1"))
    }
}

#[test]
fn plant_water_feed_grow_harvest_and_extract() {
    let mut s = Station::start("plant_water_feed_grow_harvest_and_extract", 1);
    let a = s.players[0].mob;
    assert_eq!(*s.get::<TilePos>(a), SPAWN);
    let tray = s.tray();
    // A map-placed tray starts full of water with 10u of E-Z-Nutrient; empty it for the test.
    assert_eq!(s.get::<Tray>(tray).waterlevel, 100.0);
    assert_eq!(
        s.amount(tray, "/datum/reagent/plantnutriment/eznutriment"),
        10.0
    );
    {
        let world = s.server.world_mut();
        world.get_mut::<Tray>(tray).unwrap().waterlevel = 0.0;
        world.get_mut::<Reagents>(tray).unwrap().0.clear();
    }

    // Plant the tomato seed pack.
    let seeds = s.item_on(SEEDS);
    assert_eq!(s.get::<Seed>(seeds).potency, 10.0, "the type's seed");
    s.take(0, seeds);
    s.click(0, tray);
    s.run_until("planted", |s| {
        s.get::<Tray>(tray).plant_status == PlantStatus::Growing
    });
    s.run_until("A reads it", |s| s.said(0, "You plant tomato seed pack."));
    assert!(
        s.server.world().get_entity(seeds).is_err(),
        "the packet is in the tray"
    );
    assert_eq!(s.get::<Tray>(tray).plant_health, 15.0, "endurance");
    s.run_until("the seedling is drawn", |s| {
        s.plant_overlay(tray) == Some(state_id(GROWING, "tomato-grow1"))
    });

    // Water it: 50 units per pour, twice.
    let can = s.item_on(CAN);
    s.take(0, can);
    s.click(0, tray);
    s.run_until("watered", |s| s.get::<Tray>(tray).waterlevel == 50.0);
    assert_eq!(s.amount(can, "/datum/reagent/water"), 50.0);
    s.click(0, tray);
    s.run_until("watered again", |s| s.get::<Tray>(tray).waterlevel == 100.0);
    assert_eq!(
        s.get::<Reagents>(tray).0.total_volume,
        0.0,
        "water is not a nutrient"
    );
    s.drop_active(0);

    // Feed it: 10 units of E-Z-Nutrient per pour up to the tray's 20.
    let bottle = s.item_on(BOTTLE);
    s.take(0, bottle);
    s.click(0, tray);
    s.run_until("fed", |s| {
        s.amount(tray, "/datum/reagent/plantnutriment/eznutriment") == 10.0
    });
    s.click(0, tray);
    s.run_until("fed again", |s| {
        s.amount(tray, "/datum/reagent/plantnutriment/eznutriment") == 20.0
    });
    s.drop_active(0);

    // The analyzer reads the tray and its plant.
    let analyzer = s.item_on(ANALYZER);
    s.take(0, analyzer);
    s.click(0, tray);
    s.run_until("scanned", |s| {
        s.said(0, "Water: 100/100 | Nutrients: 20/20")
            && s.said(0, "Potency: 10 | Yield: 3")
            && s.said(0, "Reagents: ")
    });
    assert!(s.said(0, "Reagents: Vitamin 4%, Nutriment 10%"));
    s.drop_active(0);

    // Grow: every hydroponics timer 100 times faster (a cycle in 0.2 s), tg's ratios kept.
    s.server.world_mut().insert_resource(HydroSpeed(100.0));
    let mut stages = vec![s.plant_overlay(tray).unwrap()];
    s.run_until_for(60.0, "harvestable", |s| {
        let id = s.plant_overlay(tray);
        if let Some(id) = id
            && stages.last() != Some(&id)
        {
            stages.push(id);
        }
        s.get::<Tray>(tray).plant_status == PlantStatus::Harvestable
    });
    let state = s.get::<Tray>(tray).clone();
    let production = state.myseed.as_ref().unwrap().production;
    // `lastproduce` follows the age up to maturation - 1 (7); harvestable once the age is past
    // it by more than `production` (6, unless a mutation moved it).
    assert_eq!(state.lastproduce, 7.0);
    assert!(
        state.age - state.lastproduce > production,
        "age {}",
        state.age
    );
    s.step();
    assert_eq!(
        s.plant_overlay(tray),
        Some(state_id(GROWING, "tomato-harvest"))
    );
    assert!(
        stages.len() >= 4,
        "it went through its growth stages: {stages:?}"
    );
    assert!(
        s.get::<Appearance>(tray)
            .overlays
            .iter()
            .any(|o| o.icon_state.0 == state_id(EQUIPMENT, "over_harvest3")),
        "the harvest light"
    );
    s.server.world_mut().insert_resource(HydroSpeed(1.0));

    // Harvest by hand: `getYield()` tomatoes at the harvester's feet, each with the plant's seed.
    let plant = state.myseed.clone().unwrap();
    s.click(0, tray);
    s.run_until("harvested", |s| s.said(0, "You harvest "));
    let tomatoes = s.items_on(SPAWN, "/obj/item/food/grown/tomato");
    assert_eq!(tomatoes.len() as f32, plant.yield_.floor(), "yield");
    assert!(!tomatoes.is_empty());
    for &tomato in &tomatoes {
        let potency = s.get::<Seed>(tomato).potency;
        // `prepare_result`: max(1, round(100 * potency% * rate)).
        let nutriment = s.amount(tomato, "/datum/reagent/consumable/nutriment");
        let vitamin = s.amount(tomato, "/datum/reagent/consumable/nutriment/vitamin");
        assert_eq!(
            nutriment,
            (potency * 0.1).floor().max(1.0),
            "potency {potency}"
        );
        assert_eq!(
            vitamin,
            (potency * 0.04).floor().max(1.0),
            "potency {potency}"
        );
    }
    // Perennial: it grows again.
    assert_eq!(s.get::<Tray>(tray).plant_status, PlantStatus::Growing);

    // A tomato in the seed extractor: rand(1, 4) + 1 packets with its stats.
    let tomato = tomatoes[0];
    let stats = s.get::<Seed>(tomato).clone();
    s.take(0, tomato);
    let extractor = s
        .server
        .world()
        .resource::<SpatialIndex>()
        .on_tile(EXTRACTOR)
        .iter()
        .copied()
        .find(|&e| s.server.world().get::<ss13_mapload::DmObj>(e).is_some())
        .unwrap();
    s.click(0, extractor);
    s.run_until("extracted", |s| s.said(0, "You extract some seeds."));
    let packets = s.items_on(EXTRACTOR, "/obj/item/seeds/tomato");
    assert!((2..=5).contains(&packets.len()), "{}", packets.len());
    for p in packets {
        let seed = s.get::<Seed>(p);
        assert_eq!(
            (seed.potency, seed.yield_, seed.endurance, seed.instability),
            (
                stats.potency,
                stats.yield_,
                stats.endurance,
                stats.instability
            )
        );
    }
    assert!(s.server.world().get_entity(tomato).is_err());
}

/// MetaStation's hydroponics with the generated data: its trays, vendors and extractors, the
/// light at each tray, and a tomato from the MegaSeed Servitor's stock planted, grown and
/// harvested in a real tray (real sprites).
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_hydroponics() {
    use ss13_game::storage::take_out;
    use ss13_mapload::{DmObj, DmVending, IconIndex};

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
    let mut s = Station::start_on(map, 1);
    let a = s.players[0].mob;
    let world = s.server.world_mut();
    let mut q = world.query::<(Entity, &DmObj, &TilePos, &Tray)>();
    let trays: Vec<(Entity, String, TilePos)> = q
        .iter(world)
        .map(|(e, o, p, _)| (e, o.path.clone(), *p))
        .collect();
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    for (_, path, _) in &trays {
        *kinds.entry(path.clone()).or_default() += 1;
    }
    println!("trays: {kinds:?}");

    assert!(
        kinds
            .get("/obj/machinery/hydroponics/constructable")
            .is_some_and(|&n| n >= 6)
    );
    for (_, path, pos) in &trays {
        println!(
            "  {path} at {pos:?}: light {:?}",
            ss13_game::botany::lumcount(world, *pos)
        );
    }
    let mut q = world.query::<(&DmObj, &DmVending)>();
    let seeds_vendor = q
        .iter(world)
        .find(|(o, _)| o.path == "/obj/machinery/vending/hydroseeds")
        .map(|(_, v)| v.clone())
        .expect("a MegaSeed Servitor");
    assert!(
        seeds_vendor
            .products
            .iter()
            .any(|p| p.path == "/obj/item/seeds/tomato")
    );
    let mut q = world.query::<&DmObj>();
    let extractors = q
        .iter(world)
        .filter(|o| o.path == "/obj/machinery/seed_extractor")
        .count();
    println!("seed extractors: {extractors}");
    assert!(extractors >= 1);

    // The constructable tray with the most light; stand next to it with a tomato seed pack.
    let (tray, _, tray_pos) = trays
        .iter()
        .filter(|(_, p, _)| p == "/obj/machinery/hydroponics/constructable")
        .max_by(|x, y| {
            let l = |pos| ss13_game::botany::lumcount(world, pos).unwrap_or(1.0);
            l(x.2).total_cmp(&l(y.2))
        })
        .cloned()
        .unwrap();
    let free = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
    ]
    .into_iter()
    .map(|(dx, dy)| TilePos::new(tray_pos.x + dx, tray_pos.y + dy, tray_pos.z))
    .find(|&p| {
        world.resource::<SpatialIndex>().dense_on_tile(p).is_empty()
            && world
                .resource::<SpatialIndex>()
                .on_tile(p)
                .iter()
                .any(|&e| world.get::<ss13_mapload::Turf>(e).is_some())
    })
    .expect("a free tile by the tray");
    world.entity_mut(a).insert(free);
    let seeds = ss13_game::construction::spawn_obj(world, "/obj/item/seeds/tomato", free).unwrap();
    take_out(world, seeds, free);
    s.run_until("the seeds are replicated", |s| s.net_id(0, seeds).is_some());
    s.take(0, seeds);
    s.click(0, tray);
    s.run_until("planted", |s| {
        s.get::<Tray>(tray).plant_status == PlantStatus::Growing
    });
    let icons = s.server.world().resource::<IconIndex>();
    let grow1 = icons
        .get(GROWING, "tomato-grow1")
        .expect("the real growing sprite");
    s.run_until("drawn", |s| {
        s.get::<Appearance>(tray)
            .overlays
            .iter()
            .any(|o| o.icon_state == grow1)
    });
    // The mapped tray's 10u of E-Z-Nutrient last 10 cycles; top it up from a bottle as a
    // botanist does (tg starves a plant without nutrients).
    let world = s.server.world_mut();
    let bottle = ss13_game::construction::spawn_obj(
        world,
        "/obj/item/reagent_containers/cup/bottle/nutrient/ez",
        free,
    )
    .unwrap();
    take_out(world, bottle, free);
    s.run_until("the bottle is replicated", |s| {
        s.net_id(0, bottle).is_some()
    });
    s.take(0, bottle);
    s.click(0, tray);
    s.run_until("fed", |s| s.get::<Reagents>(tray).0.total_volume >= 20.0);
    s.drop_active(0);
    s.server.world_mut().insert_resource(HydroSpeed(100.0));
    s.run_until_for(60.0, "harvestable", |s| {
        s.get::<Tray>(tray).plant_status != PlantStatus::Growing
    });
    let state = s.get::<Tray>(tray).clone();
    println!(
        "after {} cycles: {:?}, health {}, water {}, seed {:?}",
        state.age, state.plant_status, state.plant_health, state.waterlevel, state.myseed
    );
    assert_eq!(state.plant_status, PlantStatus::Harvestable);
    s.server.world_mut().insert_resource(HydroSpeed(1.0));
    s.click(0, tray);
    s.run_until("harvested", |s| s.said(0, "You harvest "));
    let tomatoes = s.items_on(free, "/obj/item/food/grown/tomato");
    println!("harvested {} tomatoes", tomatoes.len());
    for t in tomatoes {
        let seed = s.get::<Seed>(t);
        println!(
            "  potency {}: nutriment {}, vitamin {}",
            seed.potency,
            s.amount(t, "/datum/reagent/consumable/nutriment"),
            s.amount(t, "/datum/reagent/consumable/nutriment/vitamin")
        );
    }
}
