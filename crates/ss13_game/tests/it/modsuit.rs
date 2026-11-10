//! MODsuits (`ss13_game::modsuit`) without clients: a pre-equipped control unit spawned on a
//! fixture floor gets its theme's parts, core, cell and storage module; worn, it deploys its
//! parts, seals them one second apart on activation (pressure sealing, the theme's slowdown),
//! drains `charge_drain` from its cell, burns its wearer on an EMP, and unseals on
//! deactivation; it cannot come off with parts out. Module installs refuse over the theme's
//! complexity and on incompatible modules. Sealing hides what the skin's sealed
//! `flags_inv` say; an active ion jetpack lets a weightless wearer step off nothing, paying
//! for each thrust. A MOD shell becomes a unit of its plating's theme with its core; skins and
//! paint apply to the parts; MODlink calls show the other wearer's hologram and end when a
//! wearer is gone; an AI moves into the unit and out, and a despawned unit leaves its AI in a
//! mini-card.

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
const SHELL: &str = "/obj/item/mod/construction/shell";
const PLATING: &str = "/obj/item/mod/construction/plating";
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
        "worn_icon": null, "worn_icon_state": null, "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "screwdriver" } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": { "tool_behaviour": "wrench" } },
    "/obj/item/crowbar": { "parent": "/obj/item", "declared_vars": { "tool_behaviour": "crowbar" } },
    "/obj/item/mod/construction": { "parent": "/obj/item/mod" },
    "/obj/item/mod/ai_minicard": { "parent": "/obj/item/mod" },
    "/obj/item/mod/construction/shell": { "parent": "/obj/item/mod/construction" },
    "/obj/item/mod/construction/helmet": { "parent": "/obj/item/mod/construction" },
    "/obj/item/mod/construction/chestplate": { "parent": "/obj/item/mod/construction" },
    "/obj/item/mod/construction/gauntlets": { "parent": "/obj/item/mod/construction" },
    "/obj/item/mod/construction/boots": { "parent": "/obj/item/mod/construction" },
    "/obj/item/mod/construction/plating": { "parent": "/obj/item/mod/construction" },
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
        "/obj/item/mod/control",
        "/obj/item/mod/ai_minicard",
        SHELL,
        PLATING,
        "/obj/item/mod/construction/helmet",
        "/obj/item/mod/construction/chestplate",
        "/obj/item/mod/construction/gauntlets",
        "/obj/item/mod/construction/boots",
        "/obj/item/screwdriver",
        "/obj/item/wrench",
        "/obj/item/crowbar",
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
        suits: vec![
            SuitDef {
                path: CONTROL.into(),
                theme: "/datum/mod_theme/standard".into(),
                core: CORE.into(),
                cell: CELL.into(),
                modules: vec![STORAGE.into()],
                starting_frequency: "NT".into(),
                ..Default::default()
            },
            SuitDef {
                path: "/obj/item/mod/control".into(),
                theme: "/datum/mod_theme/standard".into(),
                starting_frequency: "NT".into(),
                ..Default::default()
            },
        ],
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
        platings: vec![(
            "/obj/item/mod/construction/plating".into(),
            "/datum/mod_theme/standard".into(),
        )],
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

/// `item` into `mob`'s active hand.
fn hold(world: &mut World, mob: Entity, item: Entity) {
    world
        .entity_mut(item)
        .remove::<(TilePos, ss13_game::storage::StoredIn)>()
        .insert(CarriedBy(mob));
    let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
    let active = inventory.active;
    inventory.hands[active] = Some(item);
}

#[test]
fn shell_builds_a_control_unit_with_its_core() {
    use ss13_game::modsuit::construction::{ModShell, ShellStep, item_interaction};
    use ss13_game::tools::Actor;

    let (mut app, _, mob) = setup("construct");
    let world = app.world_mut();
    let client = world.spawn_empty().id();
    let shell = ss13_game::construction::spawn_obj(world, SHELL, FLOOR).unwrap();
    let use_on = |world: &mut World, path: &str| -> Entity {
        let item = ss13_game::construction::spawn_obj(world, path, FLOOR).unwrap();
        hold(world, mob, item);
        let actor = Actor {
            client,
            mob,
            item: Some(item),
        };
        assert!(item_interaction(world, actor, shell), "{path} on the shell");
        item
    };
    let step = |world: &World| world.get::<ModShell>(shell).unwrap().step;

    let core = use_on(world, CORE);
    assert_eq!(step(world), ShellStep::Core);
    use_on(world, "/obj/item/screwdriver");
    assert_eq!(step(world), ShellStep::ScrewedCore);
    let helmet = use_on(world, "/obj/item/mod/construction/helmet");
    assert_eq!(step(world), ShellStep::Helmet);
    // A crowbar takes the helmet back out.
    use_on(world, "/obj/item/crowbar");
    assert_eq!(step(world), ShellStep::ScrewedCore);
    assert_eq!(world.get::<TilePos>(helmet), Some(&FLOOR));
    hold(world, mob, helmet);
    let actor = Actor {
        client,
        mob,
        item: Some(helmet),
    };
    assert!(item_interaction(world, actor, shell));
    for part in [
        "/obj/item/mod/construction/chestplate",
        "/obj/item/mod/construction/gauntlets",
        "/obj/item/mod/construction/boots",
        "/obj/item/wrench",
        "/obj/item/screwdriver",
    ] {
        use_on(world, part);
    }
    assert_eq!(step(world), ShellStep::ScrewedAssembly);
    // The wrong part does nothing at this step.
    let stray =
        ss13_game::construction::spawn_obj(world, "/obj/item/mod/construction/helmet", FLOOR)
            .unwrap();
    hold(world, mob, stray);
    let actor = Actor {
        client,
        mob,
        item: Some(stray),
    };
    assert!(!item_interaction(world, actor, shell));

    use_on(world, PLATING);
    assert!(world.get_entity(shell).is_err(), "the shell is used up");
    let held = world.get::<Inventory>(mob).unwrap().hands;
    let control = held
        .into_iter()
        .flatten()
        .find(|&e| world.get::<ModSuit>(e).is_some())
        .expect("the unit is in the hands");
    let suit = world.get::<ModSuit>(control).unwrap();
    assert_eq!(suit.core, Some(core), "the shell's core powers the unit");
    assert_eq!(suit.parts.len(), 2, "the theme's parts");
}

#[test]
fn skins_and_colors_apply_to_inactive_units() {
    use ss13_game::modsuit::paint::{set_mod_color, set_skin};

    let (mut app, control, _) = setup("paint");
    let world = app.world_mut();
    assert!(set_skin(world, control, "standard"));
    assert!(!set_skin(world, control, "no-such-skin"));
    let red = ss13_game::modsuit::PAINT_PRESETS[0].1;
    set_mod_color(world, control, &red);
    let helmet = world.get::<ModSuit>(control).unwrap().parts[0].item;
    assert_eq!(
        world.get::<ss13_shared::Appearance>(helmet).unwrap().color,
        Some([255, 128, 128, 255])
    );
    assert_eq!(
        world.get::<ss13_shared::Appearance>(control).unwrap().color,
        Some([255, 128, 128, 255])
    );
}

#[test]
fn modlink_calls_show_holograms_and_end_with_the_wearer() {
    use ss13_game::modsuit::link::{InCall, LinkVisual, ModLink, end_call, start_call};

    let (mut app, control, mob) = setup("modlink");
    wear(&mut app, control, mob);
    let world = app.world_mut();
    let other = ss13_game::construction::spawn_obj(world, CONTROL, FLOOR).unwrap();
    let callee = world
        .spawn((
            TilePos::new(2, 3, 1),
            Health::default(),
            Stat::Conscious,
            Inventory::default(),
        ))
        .id();
    app.update();
    wear(&mut app, other, callee);
    let world = app.world_mut();
    let core = world.get::<ModSuit>(other).unwrap().core.unwrap();
    let cell = world.get::<ModCore>(core).unwrap().cell.unwrap();
    world.entity_mut(cell).insert(ItemCell(Cell {
        charge: 10_000.0,
        maxcharge: 10_000.0,
        chargerate: 1000.0,
    }));
    let (a, b) = (
        world.get::<ModLink>(control).cloned().unwrap(),
        world.get::<ModLink>(other).cloned().unwrap(),
    );
    assert_eq!(a.frequency.as_deref(), Some("NT"), "starting_frequency");
    assert_ne!(a.id, b.id, "unique ids");
    assert!(ss13_game::modsuit::link::can_call(world, control));

    start_call(world, control, other);
    let link = world.get::<ModLink>(control).cloned().unwrap();
    assert_eq!(link.call, Some(other));
    assert!(world.get::<InCall>(mob).is_some() && world.get::<InCall>(callee).is_some());
    let visual = link.visual.expect("the wearer's hologram");
    assert_eq!(
        world.get::<LinkVisual>(visual).map(|v| v.at),
        Some(other),
        "shown at the other end"
    );
    assert_eq!(world.get::<TilePos>(visual), Some(&TilePos::new(2, 3, 1)));
    let before = charge(&app, control);
    for _ in 0..steps(2.0) {
        app.update();
    }
    let used = before - charge(&app, control);
    assert!(used > 20.0 && used < 30.0, "12.5 W for 2 s: {used}");

    // The callee takes the unit off: the call ends.
    let world = app.world_mut();
    world.get_mut::<Inventory>(callee).unwrap().back = None;
    world.entity_mut(other).remove::<CarriedBy>().insert(FLOOR);
    app.update();
    app.update();
    let world = app.world_mut();
    assert!(world.get::<ModLink>(control).unwrap().call.is_none());
    assert!(world.get_entity(visual).is_err(), "the hologram is gone");
    assert!(world.get::<InCall>(mob).is_none());
    end_call(world, control);
}

#[test]
fn an_ai_rides_the_unit_and_leaves_a_minicard() {
    use ss13_game::modsuit::ai::{
        CHARGE_PER_STEP, MiniCard, ModAssistant, ai_enter_mod, ai_exit_mod, relaymove, suit_of_eye,
    };
    use ss13_game::silicon::ai::{AiCore, AiEye, AiInside};
    use ss13_game::silicon::aicard::IntelliCard;
    use ss13_shared::Dir;

    let (mut app, control, mob) = setup("ai");
    let world = app.world_mut();
    let card = world.spawn(IntelliCard::default()).id();
    let eye = world.spawn(FLOOR).id();
    let ai = world
        .spawn((
            Health::default(),
            Stat::Conscious,
            AiCore {
                eye,
                control_disabled: true,
                connected_robots: Vec::new(),
            },
            AiInside::Card(card),
        ))
        .id();
    world.entity_mut(eye).insert(AiEye { ai, blind: false });
    world.get_mut::<IntelliCard>(card).unwrap().ai = Some(ai);

    ai_enter_mod(world, control, ai, card);
    assert_eq!(world.get::<AiInside>(ai), Some(&AiInside::Mod(control)));
    assert!(world.get::<IntelliCard>(card).unwrap().ai.is_none());
    assert_eq!(suit_of_eye(world, eye), Some(control));
    assert!(
        !world.get::<AiCore>(ai).unwrap().control_disabled,
        "wireless on in the unit"
    );

    // Worn but inactive, and no boots to walk with: no step, no charge.
    wear(&mut app, control, mob);
    let world = app.world_mut();
    let before = ss13_game::modsuit::server::charge(world, control);
    assert!(!relaymove(world, control, Dir::East));
    assert_eq!(ss13_game::modsuit::server::charge(world, control), before);
    const { assert!(CHARGE_PER_STEP > 0.0) };

    ai_exit_mod(world, control, ai, card);
    assert_eq!(world.get::<AiInside>(ai), Some(&AiInside::Card(card)));
    assert_eq!(world.get::<IntelliCard>(card).unwrap().ai, Some(ai));
    assert!(world.get::<ModAssistant>(control).is_none());

    // Back in, and the unit goes: a mini-card keeps the dead AI where it was.
    world.get_mut::<IntelliCard>(card).unwrap().ai = Some(ai);
    ai_enter_mod(world, control, ai, card);
    world.get_mut::<Inventory>(mob).unwrap().back = None;
    world
        .entity_mut(control)
        .remove::<CarriedBy>()
        .insert(FLOOR);
    world.despawn(control);
    app.update();
    let world = app.world_mut();
    let Some(&AiInside::Card(minicard)) = world.get::<AiInside>(ai) else {
        panic!("the AI is in a mini-card");
    };
    assert_eq!(world.get::<MiniCard>(minicard).unwrap().ai, Some(ai));
    assert_eq!(world.get::<Stat>(ai), Some(&Stat::Dead));
}
