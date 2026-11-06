//! MODsuits (`ss13_game::modsuit`) without clients: a pre-equipped control unit spawned on a
//! fixture floor gets its theme's parts, core, cell and storage module; worn, it deploys its
//! parts, seals them one second apart on activation (pressure sealing, the theme's slowdown),
//! drains `charge_drain` from its cell, burns its wearer on an EMP, and unseals on
//! deactivation; it cannot come off with parts out. Module installs refuse over the theme's
//! complexity and on incompatible modules. Sealing hides what the skin's sealed
//! `flags_inv` say; an active ion jetpack lets a weightless wearer step off nothing, paying
//! for each thrust.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::emp::EmpHit;
use ss13_game::health::{Health, Stat};
use ss13_game::inventory::{CarriedBy, Inventory, NoDrop};
use ss13_game::mecha::core::ItemCell;
use ss13_game::modsuit::modules::ModModule;
use ss13_game::modsuit::server::{
    ModCore, ModDb, ModSuit, SuitSlowdown, install, quick_deploy, toggle_activate,
};
use ss13_game::power::Cell;
use ss13_game::storage::Storage;
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_mapload::DmClothing;
use ss13_mapload::modsuit::{
    ModData, ModStorageDef, ModuleDef, PartDef, SkinDef, SuitDef, ThemeDef,
};
use ss13_shared::TilePos;

const CONTROL: &str = "/obj/item/mod/control/pre_equipped/standard";
const HELMET: &str = "/obj/item/clothing/head/mod";
const CHESTPLATE: &str = "/obj/item/clothing/suit/mod";
const STORAGE: &str = "/obj/item/mod/module/storage";
const BIG: &str = "/obj/item/mod/module/big";
const JETPACK: &str = "/obj/item/mod/module/jetpack";
/// `HIDEEARS`, `HIDEHAIR`.
const HIDEEARS: u32 = 1 << 5;
const HIDEHAIR: u32 = 1 << 8;
const CORE: &str = "/obj/item/mod/core/standard";
const CELL: &str = "/obj/item/stock_parts/power_store/cell/super";
const STOPSPRESSUREDAMAGE: u32 = 1 << 1;
const THICKMATERIAL: u32 = 1 << 4;
const FLOOR: TilePos = TilePos::new(2, 2, 1);

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "thing", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/clothing/head": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 64 } },
    "/obj/item/clothing/head/mod": { "parent": "/obj/item/clothing/head", "declared_vars": {
        "name": "MOD helmet", "body_parts_covered": 4, "heat_protection": 4,
        "cold_protection": 4 } },
    "/obj/item/clothing/suit": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 1 } },
    "/obj/item/clothing/suit/mod": { "parent": "/obj/item/clothing/suit", "declared_vars": {
        "name": "MOD chestplate", "body_parts_covered": 3, "heat_protection": 3,
        "cold_protection": 3 } },
    "/obj/item/mod": { "parent": "/obj/item" },
    "/obj/item/mod/control": { "parent": "/obj/item/mod", "declared_vars": {
        "name": "MOD control unit", "slot_flags": 1024, "w_class": 4 } },
    "/obj/item/mod/control/pre_equipped": { "parent": "/obj/item/mod/control" },
    "/obj/item/mod/control/pre_equipped/standard": { "parent": "/obj/item/mod/control/pre_equipped" },
    "/obj/item/mod/core": { "parent": "/obj/item/mod" },
    "/obj/item/mod/core/standard": { "parent": "/obj/item/mod/core" },
    "/obj/item/mod/module": { "parent": "/obj/item/mod" },
    "/obj/item/mod/module/storage": { "parent": "/obj/item/mod/module", "declared_vars": {
        "name": "MOD storage module" } },
    "/obj/item/mod/module/big": { "parent": "/obj/item/mod/module" },
    "/obj/item/mod/module/jetpack": { "parent": "/obj/item/mod/module" },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item/stock_parts" },
    "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item/stock_parts/power_store" },
    "/obj/item/stock_parts/power_store/cell/super": { "parent": "/obj/item/stock_parts/power_store/cell" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "icon_state": "space" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwwsssssss
wffwsssssss
wffwsssssss
wwwwsssssss
"}
"##;

/// Open space with nothing within a tile to push off.
const SPACE: TilePos = TilePos::new(8, 2, 1);

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "space"),
    ("t.dmi", "wall"),
    ("i.dmi", "thing"),
    ("h.dmi", "human_basic"),
    ("l.dmi", "thing"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("modsuit_fixture")
        .join(test);
    let used = [
        CONTROL,
        HELMET,
        CHESTPLATE,
        STORAGE,
        BIG,
        JETPACK,
        CORE,
        CELL,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// The standard theme with two parts, the pre-equipped unit with a storage module, and a
/// module over the complexity limit.
fn data() -> ModData {
    let part = |path: &str, sealed: u32| PartDef {
        path: path.into(),
        unsealed_clothing: THICKMATERIAL,
        sealed_clothing: sealed,
        sealed_message: "seals".into(),
        unsealed_message: "unseals".into(),
        ..Default::default()
    };
    ModData {
        themes: vec![ThemeDef {
            path: "/datum/mod_theme/standard".into(),
            name: "standard".into(),
            default_skin: "standard".into(),
            slot_flags: 1 << 10,
            armor: ss13_mapload::DmArmor {
                melee: 10.0,
                bio: 100.0,
                ..Default::default()
            },
            max_heat_protection_temperature: 1500.0,
            min_cold_protection_temperature: 2.0,
            siemens_coefficient: 0.5,
            complexity_max: 15,
            charge_drain: 50.0,
            slowdown_deployed: 0.75,
            activation_step_time: 10.0,
            skins: vec![SkinDef {
                name: "standard".into(),
                parts: vec![
                    PartDef {
                        unsealed_inv: HIDEEARS,
                        sealed_inv: HIDEHAIR,
                        ..part(HELMET, STOPSPRESSUREDAMAGE)
                    },
                    part(CHESTPLATE, STOPSPRESSUREDAMAGE),
                ],
                ..Default::default()
            }],
            ..Default::default()
        }],
        suits: vec![SuitDef {
            path: CONTROL.into(),
            theme: "/datum/mod_theme/standard".into(),
            core: CORE.into(),
            cell: CELL.into(),
            modules: vec![STORAGE.into()],
            ..Default::default()
        }],
        modules: vec![
            ModuleDef {
                path: STORAGE.into(),
                name: "MOD storage module".into(),
                complexity: 1,
                required_slots: vec![1 << 10],
                removable: true,
                incompatible: vec![STORAGE.into()],
                storage: Some(ModStorageDef {
                    max_slots: 7,
                    max_specific_storage: 3,
                    max_total_storage: 15,
                }),
                ..Default::default()
            },
            ModuleDef {
                path: BIG.into(),
                complexity: 15,
                removable: true,
                ..Default::default()
            },
            // `/obj/item/mod/module/jetpack`.
            ModuleDef {
                path: JETPACK.into(),
                module_type: ss13_game::modsuit::MODULE_TOGGLE,
                complexity: 3,
                active_power_cost: 2.5,
                use_energy_cost: 50.0,
                required_slots: vec![1 << 10],
                removable: true,
                incompatible: vec![JETPACK.into()],
                ..Default::default()
            },
        ],
        cores: vec![CORE.into()],
    }
}

/// A server on the fixture with a pre-equipped unit on the floor and a human next to it; the
/// cell charged to 10 kJ.
fn setup(test: &str) -> (App, Entity, Entity) {
    let assets = fixture_assets(test);
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |app| {
        app.insert_resource(ModDb::new(data()));
    });
    app.update();
    let control =
        ss13_game::construction::spawn_obj(app.world_mut(), CONTROL, FLOOR).expect("prototype");
    app.update();
    let world = app.world_mut();
    let core = world
        .get::<ModSuit>(control)
        .and_then(|s| s.core)
        .expect("a core");
    let cell = world
        .get::<ModCore>(core)
        .and_then(|c| c.cell)
        .expect("a cell");
    world.entity_mut(cell).insert(ItemCell(Cell {
        charge: 10_000.0,
        maxcharge: 10_000.0,
        chargerate: 1000.0,
    }));
    let mob = world
        .spawn((
            TilePos::new(3, 2, 1),
            Health::default(),
            Stat::Conscious,
            Inventory::default(),
        ))
        .id();
    (app, control, mob)
}

fn wear(app: &mut App, control: Entity, mob: Entity) {
    let world = app.world_mut();
    world
        .entity_mut(control)
        .remove::<TilePos>()
        .insert(CarriedBy(mob));
    world.get_mut::<Inventory>(mob).unwrap().back = Some(control);
    app.update();
}

fn charge(app: &App, control: Entity) -> f64 {
    ss13_game::modsuit::server::charge(app.world(), control)
}

fn clothing(app: &App, part: Entity) -> DmClothing {
    app.world().get::<DmClothing>(part).cloned().unwrap()
}

#[test]
fn spawns_with_parts_core_and_storage() {
    let (app, control, _) = setup("spawn");
    let suit = app.world().get::<ModSuit>(control).unwrap();
    assert_eq!(suit.parts.len(), 2);
    assert_eq!(suit.modules.len(), 1);
    assert_eq!(suit.complexity, 1);
    assert!(
        app.world().get::<Storage>(control).is_some(),
        "the storage module makes the unit a storage"
    );
    let helmet = suit.parts[0].item;
    let c = clothing(&app, helmet);
    assert_eq!(c.clothing_flags, THICKMATERIAL, "unsealed flags");
    assert_eq!(c.heat_protection, 0, "no heat protection unsealed");
    assert_eq!(c.siemens_coefficient, 0.5);
    assert_eq!(
        app.world()
            .get::<ss13_mapload::DmArmor>(helmet)
            .unwrap()
            .melee,
        10.0
    );
    assert!(
        app.world().get::<CarriedBy>(helmet).is_none()
            && app.world().get::<TilePos>(helmet).is_none(),
        "retracted parts are nowhere"
    );
    assert_eq!(charge(&app, control), 10_000.0);
}

#[test]
fn deploys_seals_drains_and_unseals() {
    let (mut app, control, mob) = setup("cycle");
    wear(&mut app, control, mob);
    assert_eq!(
        app.world().get::<ModSuit>(control).unwrap().wearer,
        Some(mob)
    );

    assert!(quick_deploy(app.world_mut(), control, mob));
    app.update();
    let (helmet, chest) = {
        let s = app.world().get::<ModSuit>(control).unwrap();
        (s.parts[0].item, s.parts[1].item)
    };
    let inventory = app.world().get::<Inventory>(mob).unwrap().clone();
    assert_eq!(inventory.head, Some(helmet));
    assert_eq!(inventory.suit, Some(chest));
    assert!(
        app.world().get::<NoDrop>(control).is_some(),
        "parts out: stuck on"
    );
    // 0.75 split over three parts, all worn.
    let slowdown = app.world().get::<SuitSlowdown>(mob).unwrap().0;
    assert!((slowdown - 0.75).abs() < 1e-4, "{slowdown}");

    assert!(toggle_activate(app.world_mut(), control, Some(mob), false));
    // One second per part, then the unit.
    for _ in 0..steps(1.2) {
        app.update();
    }
    assert!(app.world().get::<ModSuit>(control).unwrap().parts[0].sealed);
    assert!(!app.world().get::<ModSuit>(control).unwrap().active);
    for _ in 0..steps(2.2) {
        app.update();
    }
    let suit = app.world().get::<ModSuit>(control).unwrap().clone();
    assert!(suit.active && !suit.activating);
    assert!(suit.parts.iter().all(|p| p.sealed));
    let c = clothing(&app, helmet);
    assert_eq!(c.clothing_flags, THICKMATERIAL | STOPSPRESSUREDAMAGE);
    assert_eq!(c.heat_protection, 4, "initial heat protection back");
    assert_eq!(
        c.flags_inv,
        HIDEEARS | HIDEHAIR,
        "sealed: the sealed flags_inv too"
    );

    let before = charge(&app, control);
    for _ in 0..steps(10.0) {
        app.update();
    }
    let used = before - charge(&app, control);
    assert!((used - 500.0).abs() < 30.0, "50 W for 10 s: {used}");

    // A heavy EMP on the active unit burns its wearer 5.
    app.world_mut().write_message(EmpHit {
        target: control,
        severity: 1,
    });
    app.update();
    let burn = app.world().get::<Health>(mob).unwrap().burn;
    assert!((burn - 5.0).abs() < 1e-3, "{burn}");

    assert!(toggle_activate(app.world_mut(), control, Some(mob), false));
    for _ in 0..steps(3.5) {
        app.update();
    }
    let suit = app.world().get::<ModSuit>(control).unwrap().clone();
    assert!(!suit.active && !suit.activating);
    assert!(suit.parts.iter().all(|p| !p.sealed));
    assert_eq!(clothing(&app, helmet).clothing_flags, THICKMATERIAL);
    assert_eq!(
        clothing(&app, helmet).flags_inv,
        HIDEEARS,
        "unsealed flags_inv"
    );

    assert!(quick_deploy(app.world_mut(), control, mob));
    app.update();
    let inventory = app.world().get::<Inventory>(mob).unwrap().clone();
    assert!(inventory.head.is_none() && inventory.suit.is_none());
    assert!(
        app.world().get::<NoDrop>(control).is_none(),
        "off and retracted: comes off"
    );
}

#[test]
fn installs_check_complexity_and_incompatibility() {
    let (mut app, control, _) = setup("install");
    let world = app.world_mut();
    let big = ss13_game::construction::spawn_obj(world, BIG, FLOOR).unwrap();
    let storage = ss13_game::construction::spawn_obj(world, STORAGE, FLOOR).unwrap();
    ss13_game::modsuit::modules::init_module(world, big);
    ss13_game::modsuit::modules::init_module(world, storage);
    assert!(world.get::<ModModule>(big).is_some());
    assert!(!install(world, control, big, None), "1 + 15 > 15");
    assert!(!install(world, control, storage, None), "a second storage");
    assert_eq!(world.get::<ModSuit>(control).unwrap().modules.len(), 1);
}

#[test]
fn jetpack_thrusts_a_weightless_wearer() {
    use ss13_game::modsuit::modules::{self, Jetpack};
    use ss13_game::spacemove::process_spacemove;
    use ss13_shared::Dir;

    let (mut app, control, mob) = setup("jetpack");
    wear(&mut app, control, mob);
    assert!(quick_deploy(app.world_mut(), control, mob));
    app.update();
    assert!(toggle_activate(app.world_mut(), control, Some(mob), false));
    for _ in 0..steps(3.5) {
        app.update();
    }
    let world = app.world_mut();
    let jet = ss13_game::construction::spawn_obj(world, JETPACK, FLOOR).unwrap();
    modules::init_module(world, jet);
    assert!(install(world, control, jet, None));

    world.entity_mut(mob).insert(SPACE);
    app.update();
    let world = app.world_mut();
    assert!(
        !process_spacemove(world, mob, Some(Dir::East), false),
        "no backup, no jetpack: no step"
    );

    assert!(modules::activate(world, control, jet, Some(mob)));
    assert!(world.get::<Jetpack>(mob).is_some_and(|j| j.stabilize));
    let before = ss13_game::modsuit::server::charge(world, control);
    assert!(
        process_spacemove(world, mob, Some(Dir::East), false),
        "the jetpack thrusts"
    );
    let used = before - ss13_game::modsuit::server::charge(world, control);
    assert!(
        (used - 50.0).abs() < 1e-6,
        "a thrust costs use_energy_cost: {used}"
    );
    assert!(
        process_spacemove(world, mob, Some(Dir::East), true),
        "stabilized: the drift stops"
    );

    // Without stabilizers the wearer drifts on.
    modules::configure_edit(world, control, jet, "stabilizers", "0");
    assert!(world.get::<Jetpack>(mob).is_some_and(|j| !j.stabilize));
    assert!(!process_spacemove(world, mob, Some(Dir::East), true));
    assert!(process_spacemove(world, mob, Some(Dir::East), false));

    assert!(modules::deactivate(world, control, jet, false));
    assert!(world.get::<Jetpack>(mob).is_none());
    assert!(!process_spacemove(world, mob, Some(Dir::East), false));
}
