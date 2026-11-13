//! The crafting menu over real WebTransport on 127.0.0.1: the HUD button opens the menu, the
//! cooking mode crafts a burger from a bun and a patty on the crafter's tile once a knife is in
//! reach, the ingredients go, their reagents end up in the burger, which lands in the hands.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::Reagents;
use ss13_game::crafting::{
    CraftButtonUi, MAKE, OPEN, PersonalCrafting, PersonalCraftingUi, RECIPE, TOGGLE_MODE,
};
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_mapload::convert::DmTypes;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1,
        "inverse_chem": null, "inverse_chem_val": 0.25, "purge_multiplier": 1 } },
    "/datum/reagent/consumable": { "parent": "/datum/reagent" },
    "/datum/reagent/consumable/nutriment": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Nutriment" } },
    "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
        "H_ion_release": 0.01, "determin_ph_range": 4, "is_cold_recipe": 0,
        "mix_message": "The solution begins to bubble.", "mob_react": 1, "optimal_ph_max": 9,
        "optimal_ph_min": 5, "optimal_temp": 500, "overheat_temp": 900, "ph_exponent_factor": 2,
        "purity_min": 0.15, "rate_up_lim": 30, "reaction_flags": 0,
        "required_catalysts": { "list": [] }, "required_container": null,
        "required_container_accepts_subtypes": 0, "required_reagents": { "list": [] },
        "required_temp": 100, "results": { "list": [] }, "temp_exponent_factor": 2,
        "thermic_constant": 0 } },
    "/datum/crafting_recipe": { "parent": "/datum", "declared_vars": { "name": null,
        "desc": null, "reqs": { "list": [] }, "result": null, "time": 30,
        "crafting_flags": 32, "delete_contents": 1, "mass_craftable": 0,
        "blacklist_result": null, "placement_checks": 0, "non_craftable": null,
        "category": null }, "procs": ["New", "check_requirements", "check_tools"] },
    "/datum/crafting_recipe/food": { "parent": "/datum/crafting_recipe",
        "declared_vars": { "category": "Foods", "crafting_flags": 224, "mass_craftable": 1,
        "added_foodtypes": 0, "removed_foodtypes": 0, "cuisine_category": "Terran",
        "dish_category": "Uncategorized", "meal_category": "Main Course" } },
    "/datum/crafting_recipe/food/burger": { "parent": "/datum/crafting_recipe/food",
        "declared_vars": { "name": "Plain Burger", "dish_category": "Burger", "time": 10,
        "tool_behaviors": { "list": ["knife"] },
        "reqs": { "list": [
            { "assoc": [{ "path": "/obj/item/food/patty" }, 1] },
            { "assoc": [{ "path": "/obj/item/food/bun" }, 1] } ] },
        "result": { "path": "/obj/item/food/burger" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "tool_behaviour": null,
        "toolspeed": 1 } },
    "/obj/item/knife": { "parent": "/obj/item", "vars": { "name": "knife" }, "declared_vars": {
        "name": "knife", "tool_behaviour": "knife", "force": 10 } },
    "/obj/item/food": { "parent": "/obj/item", "declared_vars": {
        "food_reagents": null, "max_volume": null, "bite_consumption": null,
        "eat_time": null, "tastes": null, "eatverbs": null, "trash_type": null,
        "junkiness": null, "food_flags": null, "starting_reagent_purity": null } },
    "/obj/item/food/bun": { "parent": "/obj/item/food", "vars": { "name": "bun" },
        "declared_vars": { "name": "bun",
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 2] }] } } },
    "/obj/item/food/patty": { "parent": "/obj/item/food", "vars": { "name": "patty" },
        "declared_vars": { "name": "patty",
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 3] }] } } },
    "/obj/item/food/burger": { "parent": "/obj/item/food", "vars": { "name": "burger" },
        "declared_vars": { "name": "burger", "crafting_complexity": 2,
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 6] }] } } },
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
  }
}"##;

const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wffffw
wfsffw
wffffw
wwwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
];

const BUN: &str = "/obj/item/food/bun";
const PATTY: &str = "/obj/item/food/patty";
const BURGER: &str = "/obj/item/food/burger";
const KNIFE: &str = "/obj/item/knife";
const BURGER_RECIPE: &str = "/datum/crafting_recipe/food/burger";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("crafting_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        BUN,
        PATTY,
        BURGER,
        KNIFE,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let dir = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    let crafting = ss13_mapload::crafting::resolve(&types);
    std::fs::write(
        dir.join(ss13_mapload::crafting::CRAFTING_FILE),
        crafting.to_ron(),
    )
    .unwrap();
    dir
}

/// Chat lines and the windows opened (id, interface).
#[derive(Resource, Default)]
struct Received(Vec<String>, Vec<(u32, String)>);

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReader<UiMessageReceived>,
    mut received: ResMut<Received>,
) {
    for ChatReceived(message) in chat.read() {
        received.0.push(message.plain_text());
    }
    for UiMessageReceived(message) in ui.read() {
        if let UiServerMessage::Open {
            window, interface, ..
        } = message
        {
            received.1.push((*window, interface.clone()));
        }
    }
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(1, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        let a = s.players[0].mob;
        s.server
            .world_mut()
            .entity_mut(a)
            .insert(TilePos::new(2, 3, 1));
        Self(s)
    }

    fn heard(&self, text: &str) -> bool {
        self.players[0]
            .app
            .world()
            .resource::<Received>()
            .0
            .iter()
            .any(|l| l.contains(text))
    }

    /// The last window of `interface` opened.
    fn window(&self, interface: &str) -> Option<u32> {
        self.players[0]
            .app
            .world()
            .resource::<Received>()
            .1
            .iter()
            .rev()
            .find(|(_, i)| i == interface)
            .map(|(w, _)| *w)
    }

    fn spawn(&mut self, path: &str, pos: TilePos) -> Entity {
        ss13_game::construction::spawn_obj(self.server.world_mut(), path, pos).unwrap()
    }

    /// The item of `path` in the crafter's hands.
    fn held(&self, path: &str) -> Option<Entity> {
        let world = self.server.world();
        let a = self.players[0].mob;
        world
            .get::<Inventory>(a)?
            .hands
            .iter()
            .flatten()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
    }
}

#[test]
fn a_burger_is_cooked_from_what_lies_around() {
    let mut s = Station::start("a_burger_is_cooked_from_what_lies_around");
    let a = s.players[0].mob;
    let here = TilePos::new(2, 3, 1);
    let bun = s.spawn(BUN, here);
    let patty = s.spawn(PATTY, here);

    s.run_until("the button", |s| s.window(CraftButtonUi::NAME).is_some());
    let button = s.window(CraftButtonUi::NAME).unwrap();
    s.act(0, button, OPEN, &[]);
    s.run_until("the menu opens", |s| {
        s.window(PersonalCraftingUi::NAME).is_some()
    });
    let menu = s.window(PersonalCraftingUi::NAME).unwrap();
    s.act(0, menu, TOGGLE_MODE, &[]);
    s.run_until("cooking mode", |s| s.get::<PersonalCrafting>(a).cooking);

    // No knife in reach: refused, nothing used.
    s.act(0, menu, MAKE, &[(RECIPE, BURGER_RECIPE)]);
    s.run_until("no tool", |s| s.heard("нет нужного инструмента"));
    assert!(s.try_get::<DmItem>(bun).is_some() && s.try_get::<DmItem>(patty).is_some());

    // A knife one tile away counts.
    s.spawn(KNIFE, TilePos::new(3, 3, 1));
    s.act(0, menu, MAKE, &[(RECIPE, BURGER_RECIPE)]);
    s.run_until("the burger is in hand", |s| s.held(BURGER).is_some());
    let burger = s.held(BURGER).unwrap();
    assert!(s.server.world().get_entity(bun).is_err(), "the bun is used");
    assert!(
        s.server.world().get_entity(patty).is_err(),
        "the patty is used"
    );
    // `CRAFT_CLEARS_REAGENTS`, then the ingredients' 2 + 3 units.
    let volume = s.get::<Reagents>(burger).0.total_volume;
    assert!((volume - 5.0).abs() < 0.01, "{volume}");
    assert!(s.try_get::<ss13_game::food::Handmade>(burger).is_some());
    // The line comes with the craft; the client may read it a tick after the burger.
    s.run_until("the done line", |s| s.heard("Plain Burger: готово."));
}
