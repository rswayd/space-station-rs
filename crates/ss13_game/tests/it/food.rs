//! Hunger, food and the kitchen over real WebTransport on 127.0.0.1: a mob's nutrition drops
//! every `SSmobs` fire; an egg cooked in the microwave becomes a boiled egg (tg
//! `microwavable`); eating it bite by bite tastes it, leaves its trash in the hand and raises
//! nutrition through nutriment metabolism; a knife slices a bread loaf on a table into five
//! slices that split its nutriment.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use crate::harness::chat::{Received, record};
use bevy::prelude::*;
use ss13_game::breath::MobsClock;
use ss13_game::chem::{Chem, MobReagents, Reagents};
use ss13_game::combat::NextMove;
use ss13_game::hunger::Hunger;
use ss13_game::inventory::Inventory;
use ss13_game::kitchen::Microwave;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmItem, MapLoadPlugin};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
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
        "chemical_flags": 0, "purity": 1, "creation_purity": 1, "inverse_chem": null,
        "inverse_chem_val": 0.25, "purge_multiplier": 1 } },
    "/datum/reagent/consumable": { "parent": "/datum/reagent", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent/consumable" }, "taste_mult": 4,
        "nutriment_factor": 1, "creation_purity": 0.5 } },
    "/datum/reagent/consumable/nutriment": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Nutriment", "nutriment_factor": 15, "brute_heal": 1,
        "metabolization_rate": 0.4 } },
    "/datum/reagent/consumable/eggyolk": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Egg Yolk", "nutriment_factor": 15,
        "taste_description": "egg" } },
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
    "/obj/structure": { "parent": "/obj" },
    "/obj/structure/table": { "parent": "/obj/structure", "vars": { "name": "table" }, "declared_vars": {
        "name": "table", "icon": { "resource": "s.dmi" }, "icon_state": "table" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "use_power": 1,
        "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1 } },
    "/obj/machinery/microwave": { "parent": "/obj/machinery", "vars": { "name": "microwave oven" }, "declared_vars": {
        "name": "microwave oven", "icon": { "resource": "m.dmi" },
        "icon_state": "mw_complete", "density": 1, "anchored": 1 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/knife": { "parent": "/obj/item", "vars": { "name": "knife" }, "declared_vars": { "name": "knife",
        "tool_behaviour": "knife", "force": 10 } },
    "/obj/item/trash": { "parent": "/obj/item" },
    "/obj/item/trash/shell": { "parent": "/obj/item/trash", "vars": { "name": "egg shell" }, "declared_vars": {
        "name": "egg shell" } },
    "/obj/item/food": { "parent": "/obj/item", "declared_vars": {
        "food_reagents": null, "max_volume": null, "bite_consumption": null,
        "eat_time": null, "tastes": null, "eatverbs": null, "trash_type": null,
        "junkiness": null, "food_flags": null, "starting_reagent_purity": null },
        "procs": ["make_microwaveable", "make_processable"] },
    "/obj/item/food/badrecipe": { "parent": "/obj/item/food", "vars": { "name": "burned mess" }, "declared_vars": {
        "name": "burned mess" } },
    "/obj/item/food/egg": { "parent": "/obj/item/food", "vars": { "name": "egg" }, "declared_vars": { "name": "egg",
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/eggyolk" }, 2] }] } },
        "procs": ["make_microwaveable"] },
    "/obj/item/food/boiledegg": { "parent": "/obj/item/food", "vars": { "name": "boiled egg" }, "declared_vars": {
        "name": "boiled egg", "trash_type": { "path": "/obj/item/trash/shell" },
        "tastes": { "list": [{ "assoc": ["egg", 1] }] },
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 4] }] } } },
    "/obj/item/food/bread": { "parent": "/obj/item/food", "declared_vars": {
        "max_volume": 80, "eat_time": 30, "slice_type": null, "yield": 5 },
        "procs": ["make_processable"] },
    "/obj/item/food/bread/plain": { "parent": "/obj/item/food/bread", "vars": { "name": "bread" }, "declared_vars": {
        "name": "bread",
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 10] }] },
        "slice_type": { "path": "/obj/item/food/breadslice/plain" } } },
    "/obj/item/food/breadslice": { "parent": "/obj/item/food" },
    "/obj/item/food/breadslice/plain": { "parent": "/obj/item/food/breadslice",
        "vars": { "name": "bread slice" }, "declared_vars": { "name": "bread slice",
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 2] }] } } },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0 } },
    "/area/space": { "parent": "/area" }
  },
  "element_calls": {
    "/datum/element/microwavable": [
      { "type": "/obj/item/food", "proc": "make_microwaveable", "call": "AddElement",
        "line": "AddElement(/datum/element/microwavable, /obj/item/food/badrecipe, bad_recipe = TRUE)" },
      { "type": "/obj/item/food/egg", "proc": "make_microwaveable", "call": "AddElement",
        "line": "AddElement(/datum/element/microwavable, /obj/item/food/boiledegg)" }
    ],
    "/datum/element/processable": [
      { "type": "/obj/item/food/bread", "proc": "make_processable", "call": "AddElement",
        "line": "AddElement(/datum/element/processable, TOOL_KNIFE, slice_type, yield, 3 SECONDS, table_required = TRUE, screentip_verb = \"Slice\", sound_to_play = SFX_KNIFE_SLICE)" }
    ]
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (3,4); a microwave `M` west of it, an
/// egg `E` east; a bread loaf on a table `T` south-west and a knife `K` south.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"M" = (/obj/machinery/microwave,/turf/open/floor,/area/space)
"E" = (/obj/item/food/egg,/turf/open/floor,/area/space)
"T" = (/obj/structure/table,/obj/item/food/bread/plain,/turf/open/floor,/area/space)
"K" = (/obj/item/knife,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wfffffw
wMsEffw
wTKfffw
wwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const MICROWAVE: TilePos = TilePos::new(2, 3, 1);
const EGG: TilePos = TilePos::new(4, 3, 1);
const TABLE: TilePos = TilePos::new(2, 2, 1);
const KNIFE: TilePos = TilePos::new(3, 2, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("s.dmi", "table"),
    ("m.dmi", "back_off"),
    ("m.dmi", "back_on"),
    ("m.dmi", "back_open"),
    ("m.dmi", "mw_complete"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("food_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/microwave",
        "/obj/item/food/egg",
        "/obj/item/food/bread/plain",
        "/obj/structure/table",
        "/obj/item/knife",
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

    /// Picks "cook" in the microwave's radial menu.
    fn cook_from_radial(&mut self, p: usize) {
        use ss13_tgui::UiInterface;
        use ss13_tgui::interfaces::radial::RadialUi;
        use ss13_tgui::server::UiWindow;
        let link = self.players[p].link;
        let mut window = None;
        self.run_until("the radial opens", |s| {
            let world = s.server.world_mut();
            let mut q = world.query::<&UiWindow>();
            window = q
                .iter(world)
                .find(|w| w.client == link && w.interface == RadialUi::NAME)
                .map(|w| w.id);
            window.is_some()
        });
        self.act(p, window.expect("open"), "submit", &[("entry", "Готовить")]);
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

    fn nutrition(&self, p: usize) -> f32 {
        self.get::<Hunger>(self.players[p].mob).nutrition
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
}

#[test]
fn hunger_microwave_eat_and_slice() {
    let mut s = Station::start("hunger_microwave_eat_and_slice", 1);
    let a = s.players[0].mob;
    assert_eq!(*s.get::<TilePos>(a), SPAWN);

    // Nutrition starts in tg's range and drops 0.04/s at every SSmobs fire.
    let start = s.nutrition(0);
    assert!((250.0..=400.0).contains(&start), "{start}");
    let fired = s.server.world().resource::<MobsClock>().times_fired;
    s.run_until("two SSmobs fires", |s| {
        s.server.world().resource::<MobsClock>().times_fired >= fired + 2
    });
    s.step();
    let dropped = start - s.nutrition(0);
    assert!(
        (0.079..=0.25).contains(&dropped),
        "{start} -> {}",
        s.nutrition(0)
    );

    // The egg goes into the microwave; an empty-hand click cooks it into a boiled egg.
    let egg = s.item_on(EGG);
    assert_eq!(
        s.amount(egg, "/datum/reagent/consumable/eggyolk"),
        2.0,
        "food_reagents"
    );
    s.take(0, egg);
    let microwave = s
        .server
        .world()
        .resource::<SpatialIndex>()
        .on_tile(MICROWAVE)
        .iter()
        .copied()
        .find(|&e| s.server.world().get::<Microwave>(e).is_some())
        .expect("a microwave");
    s.click(0, microwave);
    s.run_until("the egg is inside", |s| {
        s.get::<Inventory>(a).active_item().is_none()
            && s.server.world().get::<TilePos>(egg).is_none()
    });
    s.run_until("A reads it", |s| s.said(0, "Вы добавляете egg в "));
    s.click(0, microwave);
    s.cook_from_radial(0);
    s.run_until("it runs", |s| {
        s.get::<Microwave>(microwave).operating.is_some()
    });
    s.run_until("it turns on", |s| s.said(0, "включается!"));
    s.run_until_for(30.0, "the boiled egg comes out", |s| {
        !s.items_on(MICROWAVE, "/obj/item/food/boiledegg").is_empty()
    });
    assert!(s.server.world().get_entity(egg).is_err(), "the egg is gone");
    let boiled = s.items_on(MICROWAVE, "/obj/item/food/boiledegg")[0];
    // `microwavable` moves the source's reagents into the result.
    assert_eq!(s.amount(boiled, "/datum/reagent/consumable/eggyolk"), 2.0);
    assert_eq!(s.amount(boiled, "/datum/reagent/consumable/nutriment"), 0.0);
    assert_eq!(s.get::<Microwave>(microwave).dirty, 1);

    // Eat it: one click, bites of 2 every second until it is gone, its shell left in hand.
    s.take(0, boiled);
    let before = s.nutrition(0);
    s.click(0, a);
    s.run_until("the first bite", |s| s.said(0, "Вы чувствуете вкус egg."));
    s.run_until("eaten up", |s| s.server.world().get_entity(boiled).is_err());
    s.run_until("nothing left", |s| {
        s.said(0, "Ничего более не осталось от boiled egg, о нет!")
    });
    let held = s
        .get::<Inventory>(a)
        .active_item()
        .expect("the trash in hand");
    assert_eq!(s.get::<DmItem>(held).path, "/obj/item/trash/shell");
    let stomach = &s.get::<MobReagents>(a).stomach;
    assert!(stomach.total_volume > 0.0 || s.get::<MobReagents>(a).blood.total_volume > 0.0);
    // The yolk is digested and metabolized into nutrition (`consumable/on_mob_life`): 2 units
    // at purity 0.5 give 15.
    s.run_until_for(40.0, "nutrition goes up", |s| s.nutrition(0) > before + 6.0);
    s.drop_active(0);

    // Slice the bread on the table with the knife: 3 s, then five slices of 2 nutriment.
    let knife = s.item_on(KNIFE);
    s.take(0, knife);
    let bread = s.items_on(TABLE, "/obj/item/food/bread/plain")[0];
    s.click(0, bread);
    s.run_until("A starts", |s| s.said(0, "You start working on bread."));
    assert!(
        s.server.world().get::<DoAfter>(a).is_some(),
        "3 s with the knife"
    );
    s.run_until_for(10.0, "five slices", |s| {
        s.items_on(TABLE, "/obj/item/food/breadslice/plain").len() == 5
    });
    assert!(s.server.world().get_entity(bread).is_err());
    for slice in s.items_on(TABLE, "/obj/item/food/breadslice/plain") {
        assert_eq!(s.amount(slice, "/datum/reagent/consumable/nutriment"), 2.0);
    }
    s.run_until("A reads it", |s| {
        s.said(0, "You manage to create 5 bread slices from bread.")
    });
}

/// The chef's meal on MetaStation's kitchen: an egg from the kitchen refrigerator's egg box
/// (tg `PopulateContents`) cooked in the kitchen microwave and eaten.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_kitchen_meal() {
    use ss13_game::closets::open;
    use ss13_game::storage::{Stored, take_out};
    use ss13_mapload::DmObj;

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
    let mut q = world.query::<(Entity, &DmObj, &TilePos)>();
    let fridges: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(_, o, _)| o.path == "/obj/structure/closet/secure_closet/freezer/fridge")
        .map(|(e, _, p)| (e, *p))
        .collect();
    let (fridge, fridge_pos) = fridges[0];
    let mut q = world.query_filtered::<(Entity, &TilePos), With<Microwave>>();
    let (microwave, mw_pos) = q
        .iter(world)
        .map(|(e, p)| (e, *p))
        .filter(|(_, p)| p.z == fridge_pos.z)
        .min_by_key(|(_, p)| (p.x - fridge_pos.x).abs() + (p.y - fridge_pos.y).abs())
        .unwrap();
    println!("kitchen fridge at {fridge_pos:?}, microwave at {mw_pos:?}");

    // The refrigerator's contents (milk, soy milk, two egg boxes) as tg spawns them.
    assert!(open(world, fridge));
    let index = world.resource::<SpatialIndex>();
    let on_fridge: Vec<Entity> = index.on_tile(fridge_pos).to_vec();
    let paths: Vec<String> = on_fridge
        .iter()
        .filter_map(|&e| world.get::<DmItem>(e).map(|i| i.path.clone()))
        .collect();
    println!("fridge holds {paths:?}");
    let egg_box = on_fridge
        .iter()
        .copied()
        .find(|&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|i| i.path == "/obj/item/storage/fancy/egg_box")
        })
        .expect("an egg box");
    let egg = world.get::<Stored>(egg_box).expect("eggs").items()[0];
    assert_eq!(world.get::<DmItem>(egg).unwrap().path, "/obj/item/food/egg");

    // Stand next to the microwave with the egg at your feet.
    let free = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .map(|(dx, dy)| TilePos::new(mw_pos.x + dx, mw_pos.y + dy, mw_pos.z))
        .find(|&p| {
            let index = world.resource::<SpatialIndex>();
            index.on_tile(p).iter().all(|&e| {
                world.get::<ss13_world::Density>(e).is_none()
                    && world.get::<ss13_game::closets::Closet>(e).is_none()
            })
        })
        .expect("a free tile by the microwave");
    world.entity_mut(a).insert(free);
    take_out(world, egg, free);
    s.run_until("the egg is replicated", |s| s.net_id(0, egg).is_some());
    s.take(0, egg);
    s.click(0, microwave);
    s.run_until("inside", |s| s.server.world().get::<TilePos>(egg).is_none());
    s.run_until("A reads it", |s| s.said(0, "Вы добавляете egg в "));
    s.click(0, microwave);
    s.cook_from_radial(0);
    s.run_until_for(30.0, "the boiled egg", |s| {
        !s.items_on(mw_pos, "/obj/item/food/boiledegg").is_empty()
    });
    let boiled = s.items_on(mw_pos, "/obj/item/food/boiledegg")[0];
    s.take(0, boiled);
    let before = s.nutrition(0);
    s.click(0, a);
    s.run_until_for(30.0, "eaten", |s| {
        s.server.world().get_entity(boiled).is_err()
    });
    s.run_until_for(60.0, "nutrition from the egg", |s| {
        s.nutrition(0) > before + 3.0
    });
    let chat = &s.players[0].app.world().resource::<Received>().0;
    println!("chat: {chat:?}");
    println!(
        "in hand after eating: {:?}",
        s.get::<Inventory>(a).active_item().and_then(|i| s
            .server
            .world()
            .get::<DmItem>(i)
            .map(|d| d.path.clone()))
    );
}
