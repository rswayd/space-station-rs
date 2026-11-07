//! Xenobiology without clients (`ss13_game::science::xenobio`): a mapped slime starts as a
//! grey baby, grows into an adult and splits into four; a dead one gives its core up; a grey
//! extract injected with plasma makes a baby slime and is gone, one given water makes
//! epinephrine and is spent; the slime console stores, places and doses slimes; a sepia floor
//! slows the mob on it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::{Chem, Reagents};
use ss13_game::closets::InCloset;
use ss13_game::health::Stat;
use ss13_game::science::xenobio::{
    Slime, SlimeExtract, TurfSlowdown, complete_core_removal, turf_slowdown,
};
use ss13_game::science::xenobio_console::{
    XenoConsole, feed_potion, feed_slime, slime_pickup, slime_place, valid_turf,
};
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_mapload::DmItem;
use ss13_mapload::convert::DmTypes;
use ss13_shared::TilePos;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1, "inverse_chem": null,
        "inverse_chem_val": 0.25, "purge_multiplier": 1 } },
    "/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": { "name": "Water" } },
    "/datum/reagent/toxin": { "parent": "/datum/reagent", "declared_vars": { "name": "Toxin" } },
    "/datum/reagent/toxin/plasma": { "parent": "/datum/reagent/toxin", "declared_vars": {
        "name": "Plasma" } },
    "/datum/reagent/medicine": { "parent": "/datum/reagent" },
    "/datum/reagent/medicine/epinephrine": { "parent": "/datum/reagent/medicine",
        "declared_vars": { "name": "Epinephrine" } },
    "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
        "H_ion_release": 0.01, "determin_ph_range": 4, "is_cold_recipe": 0,
        "mix_message": "The solution begins to bubble.", "mob_react": 1, "optimal_ph_max": 9,
        "optimal_ph_min": 5, "optimal_temp": 500, "overheat_temp": 900, "ph_exponent_factor": 2,
        "purity_min": 0.15, "rate_up_lim": 30, "reaction_flags": 0,
        "required_catalysts": { "list": [] }, "required_container": null,
        "required_container_accepts_subtypes": 0, "required_reagents": { "list": [] },
        "required_temp": 0, "results": { "list": [] }, "temp_exponent_factor": 2,
        "thermic_constant": 0 } },
    "/datum/chemical_reaction/slime": { "parent": "/datum/chemical_reaction",
        "declared_vars": { "reaction_flags": 2 },
        "procs": ["pre_reaction_other_checks", "on_reaction"] },
    "/datum/chemical_reaction/slime/slimespawn": { "parent": "/datum/chemical_reaction/slime",
        "declared_vars": {
        "required_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/toxin/plasma" }, 1] }] },
        "required_container": { "path": "/obj/item/slime_extract/grey" } },
        "procs": ["on_reaction"] },
    "/datum/chemical_reaction/slime/slimeinaprov": { "parent": "/datum/chemical_reaction/slime",
        "declared_vars": {
        "results": { "list": [{ "assoc": [{ "path": "/datum/reagent/medicine/epinephrine" }, 3] }] },
        "required_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/water" }, 5] }] },
        "required_container": { "path": "/obj/item/slime_extract/grey" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "attack_speed": 8 } },
    "/obj/item/slime_extract": { "parent": "/obj/item", "declared_vars": {
        "name": "slime extract", "icon": { "resource": "icons/mob/simple/slimes.dmi" },
        "icon_state": "grey-core" } },
    "/obj/item/slime_extract/grey": { "parent": "/obj/item/slime_extract", "declared_vars": {
        "name": "grey slime extract" } },
    "/obj/item/slimepotion": { "parent": "/obj/item" },
    "/obj/item/slimepotion/slime": { "parent": "/obj/item/slimepotion" },
    "/obj/item/slimepotion/slime/steroid": { "parent": "/obj/item/slimepotion/slime",
        "declared_vars": { "name": "slime steroid" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1,
        "icon": { "resource": "i.dmi" }, "icon_state": "toy" } },
    "/obj/machinery/computer": { "parent": "/obj/machinery" },
    "/obj/machinery/computer/camera_advanced": { "parent": "/obj/machinery/computer" },
    "/obj/machinery/computer/camera_advanced/xenobio": {
        "parent": "/obj/machinery/computer/camera_advanced",
        "declared_vars": { "name": "Slime management console" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "access": { "list": [] } } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1,
        "faction": { "expr": "list(FACTION_NEUTRAL)" } } },
    "/mob/living": { "parent": "/mob", "declared_vars": { "maxHealth": 100, "health": 100 } },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/mob/living/basic": { "parent": "/mob/living", "declared_vars": {
        "maxHealth": 20, "health": 20, "speed": 1, "melee_attack_cooldown": 20,
        "attack_verb_continuous": "атакует", "attack_verb_simple": "атакует",
        "habitable_atmos": null, "unsuitable_atmos_damage": 0,
        "minimum_survivable_temperature": 0, "maximum_survivable_temperature": 1500,
        "unsuitable_cold_damage": 0, "unsuitable_heat_damage": 0, "melee_damage_type": "brute",
        "icon": { "resource": "icons/mob/simple/slimes.dmi" } } },
    "/mob/living/basic/slime": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "grey baby slime (123)", "maxHealth": 150, "health": 150,
        "melee_damage_lower": 7, "melee_damage_upper": 17,
        "icon_state": "grey-baby", "icon_living": "grey-baby", "icon_dead": "grey-baby-dead" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/floor/sepia": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "sepia" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1: a slime `S` at (3,3) in a closed room.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"S" = (/mob/living/basic/slime,/turf/open/floor,/area/space)
"p" = (/turf/open/floor/sepia,/area/space)
"C" = (/obj/machinery/computer/camera_advanced/xenobio,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wffffCw
wfSpffw
wfffffw
wwwwwww
"}
"##;

const SLIME_AT: TilePos = TilePos::new(3, 3, 1);
const BENCH: TilePos = TilePos::new(5, 2, 1);
const GREY_EXTRACT: &str = "/obj/item/slime_extract/grey";
const STEROID: &str = "/obj/item/slimepotion/slime/steroid";
const SEPIA_AT: TilePos = TilePos::new(4, 3, 1);
const CONSOLE_AT: TilePos = TilePos::new(6, 4, 1);

const STATES: [(&str, &str); 11] = [
    ("t.dmi", "floor"),
    ("t.dmi", "sepia"),
    ("t.dmi", "wall"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("icons/mob/simple/slimes.dmi", "grey-baby"),
    ("icons/mob/simple/slimes.dmi", "grey-baby-dead"),
    ("icons/mob/simple/slimes.dmi", "grey-adult"),
    ("icons/mob/simple/slimes.dmi", "grey-adult-dead"),
    ("icons/mob/simple/slimes.dmi", "grey-cut"),
    ("icons/mob/simple/slimes.dmi", "grey-core"),
];

fn fixture(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("xenobio_fixture")
        .join(test);
    let used = [
        "/mob/living/basic/slime",
        GREY_EXTRACT,
        STEROID,
        "/obj/machinery/computer/camera_advanced/xenobio",
        "/turf/open/floor",
        "/turf/open/floor/sepia",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let dir = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    let defines: std::collections::HashMap<String, String> =
        [("FACTION_NEUTRAL".to_owned(), "neutral".to_owned())].into();
    let mobs = ss13_mapload::mobs::resolve(&types, &Default::default(), &defines);
    std::fs::write(dir.join(ss13_mapload::mobs::MOBS_FILE), mobs.to_ron()).unwrap();
    dir
}

fn server(test: &str) -> App {
    let assets = fixture(test);
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn slimes(app: &mut App) -> Vec<Entity> {
    let world = app.world_mut();
    let mut q = world.query_filtered::<Entity, With<Slime>>();
    q.iter(world).collect()
}

fn the_slime(app: &mut App) -> Entity {
    let all = slimes(app);
    assert_eq!(all.len(), 1, "one slime");
    all[0]
}

/// Adds `amount` of `reagent` to `e`'s holder, letting it react (tg `add_reagent`).
fn inject(app: &mut App, e: Entity, reagent: &str, amount: f32) {
    let world = app.world_mut();
    let chem = world.resource::<Chem>().0.clone();
    let id = chem.id(reagent).expect("fixture reagent");
    world.get_mut::<Reagents>(e).unwrap().0.add_reagent(
        &chem,
        id,
        amount,
        ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
        None,
        None,
        false,
        false,
    );
}

fn extract(app: &mut App) -> Entity {
    let world = app.world_mut();
    let e = ss13_game::construction::spawn_obj(world, GREY_EXTRACT, BENCH).unwrap();
    world.flush();
    assert!(
        world.get::<Reagents>(e).is_some(),
        "an extract holds reagents"
    );
    assert_eq!(world.get::<SlimeExtract>(e).map(|x| x.uses), Some(1));
    e
}

/// A mapped slime is a grey baby; fed past its growth it evolves (100 nutrition), then the
/// grown adult splits into four babies on the next `SSmobs` fire.
#[test]
fn slime_grows_evolves_and_splits() {
    let mut app = server("grow");
    let slime = the_slime(&mut app);
    {
        let s = app.world().get::<Slime>(slime).unwrap();
        assert_eq!((s.colour, s.adult, s.cores), ("grey", false, 1));
    }
    let world = app.world_mut();
    {
        let mut s = world.get_mut::<Slime>(slime).unwrap();
        s.nutrition = 200.0;
        s.amount_grown = 10;
    }
    run(&mut app, 2.5);
    let s = app.world().get::<Slime>(slime).unwrap().clone();
    assert!(s.adult, "evolved");
    assert_eq!(s.amount_grown, 0);
    assert!(s.nutrition < 101.0, "evolving costs 100: {}", s.nutrition);

    app.world_mut()
        .get_mut::<Slime>(slime)
        .unwrap()
        .amount_grown = 10;
    run(&mut app, 2.5);
    assert_eq!(slimes(&mut app).len(), 4, "split into four");
    let s = app.world().get::<Slime>(slime).unwrap();
    assert!(!s.adult, "the parent is a baby again");
}

/// A dead baby's core comes out as a grey extract; a second try finds none.
#[test]
fn dead_slime_gives_its_core() {
    let mut app = server("core");
    let slime = the_slime(&mut app);
    *app.world_mut().get_mut::<Stat>(slime).unwrap() = Stat::Dead;
    app.update();
    let action = DoAfter {
        client: slime,
        target: slime,
        target_pos: SLIME_AT,
        user_pos: SLIME_AT,
        item: None,
        amount: 0,
        ends_at: 0.0,
        job: ss13_game::construction::Job::SlimeCores,
    };
    complete_core_removal(app.world_mut(), &action);
    let world = app.world_mut();
    let mut q = world.query::<(&DmItem, &TilePos)>();
    let extracts = q
        .iter(world)
        .filter(|(i, p)| i.path == GREY_EXTRACT && **p == SLIME_AT)
        .count();
    assert_eq!(extracts, 1);
    assert_eq!(world.get::<Slime>(slime).unwrap().cores, 0);
}

/// Plasma in a grey extract: a new baby slime, and the extract is gone (no reagent results).
#[test]
fn grey_extract_with_plasma_makes_a_slime() {
    let mut app = server("plasma");
    let e = extract(&mut app);
    inject(&mut app, e, "/datum/reagent/toxin/plasma", 1.0);
    app.update();
    app.update();
    assert_eq!(slimes(&mut app).len(), 2, "a baby slime emerged");
    assert!(app.world().get_entity(e).is_err(), "extract used up");
}

/// Water in a grey extract makes epinephrine; the spent extract stays and reacts no more.
#[test]
fn grey_extract_with_water_is_spent() {
    let mut app = server("water");
    let e = extract(&mut app);
    inject(&mut app, e, "/datum/reagent/water", 5.0);
    app.update();
    app.update();
    let world = app.world();
    let chem = world.resource::<Chem>().0.clone();
    let epi = chem.id("/datum/reagent/medicine/epinephrine").unwrap();
    let held = &world.get::<Reagents>(e).expect("still there").0;
    assert_eq!(held.amount(epi), 3.0);
    assert_eq!(world.get::<SlimeExtract>(e).unwrap().uses, 0);
    assert_eq!(
        world
            .get::<Name>(e)
            .map(|n| n.as_str().to_owned())
            .as_deref(),
        Some("used slime extract")
    );
    inject(&mut app, e, "/datum/reagent/toxin/plasma", 1.0);
    app.update();
    assert_eq!(slimes(&mut app).len(), 1, "a spent extract does not react");
}

/// The slime console sucks a slime up (hidden on the console's tile, in stasis), spits it out
/// onto a floor of its area, doses it with the loaded potion and places no monkey without a
/// cube.
#[test]
fn slime_console_stores_places_and_doses_slimes() {
    let mut app = server("console");
    let slime = the_slime(&mut app);
    let world = app.world_mut();
    let console = {
        let mut q = world.query_filtered::<Entity, With<XenoConsole>>();
        q.single(world).expect("the mapped console")
    };
    assert!(valid_turf(world, console, BENCH));
    // `tell` only queues chat: any entity stands in for the client.
    slime_pickup(world, slime, console, slime);
    world.flush();
    assert_eq!(
        world.get::<XenoConsole>(console).unwrap().slimes,
        vec![slime]
    );
    assert!(
        world.get::<InCloset>(slime).is_some(),
        "hidden in the console"
    );
    assert_eq!(world.get::<TilePos>(slime), Some(&CONSOLE_AT));
    run(&mut app, 2.5);
    assert!(
        app.world().get::<Slime>(slime).unwrap().stasis,
        "stored in stasis"
    );

    let world = app.world_mut();
    slime_place(world, console, BENCH);
    world.flush();
    assert!(world.get::<XenoConsole>(console).unwrap().slimes.is_empty());
    assert!(world.get::<InCloset>(slime).is_none());
    assert_eq!(world.get::<TilePos>(slime), Some(&BENCH));

    world.get_mut::<XenoConsole>(console).unwrap().potion = Some(STEROID.to_owned());
    feed_potion(world, slime, slime, console, slime);
    world.flush();
    assert_eq!(
        world.get::<Slime>(slime).unwrap().cores,
        2,
        "steroid applied"
    );
    assert_eq!(world.get::<XenoConsole>(console).unwrap().potion, None);

    feed_slime(world, slime, console, BENCH);
    assert_eq!(world.get::<XenoConsole>(console).unwrap().monkeys, 0.0);
}

/// A mob stepping onto a sepia floor gets its `slowdown = 2`, and loses it stepping off.
#[test]
fn sepia_floor_slows_the_mob_on_it() {
    let mut app = server("sepia");
    let slime = the_slime(&mut app);
    assert_eq!(turf_slowdown(app.world(), SEPIA_AT), 2.0);
    assert_eq!(turf_slowdown(app.world(), SLIME_AT), 0.0);
    app.world_mut().write_message(ss13_world::MoveRequest {
        entity: slime,
        dir: ss13_shared::Dir::East,
    });
    run(&mut app, 1.0);
    assert_eq!(app.world().get::<TilePos>(slime), Some(&SEPIA_AT));
    assert_eq!(
        app.world().get::<TurfSlowdown>(slime),
        Some(&TurfSlowdown(2.0))
    );
    app.world_mut().write_message(ss13_world::MoveRequest {
        entity: slime,
        dir: ss13_shared::Dir::West,
    });
    run(&mut app, 2.0);
    assert_eq!(app.world().get::<TilePos>(slime), Some(&SLIME_AT));
    assert_eq!(
        app.world().get::<TurfSlowdown>(slime),
        Some(&TurfSlowdown(0.0))
    );
}
