//! Engineering tools on the power fixture room (`super::power`) without clients.
//!
//! T-ray scanner (`ss13_game::t_scanner`): switched on in a human's hand, it replicates the
//! under-tile objects within 3 tiles once a second for 0.8 s, only to its holder and at half
//! alpha; one 5 tiles away stays hidden; switched off, nothing shows any more.
//!
//! Inducer (`ss13_game::inducer`): clicked on a battery next to its holder, it moves the
//! target's rating (1 MJ for a 10 MJ battery) once a second until its own 3 MJ are gone.

use bevy::prelude::*;
use ss13_game::click_handlers::{self, ClickCtx, ClickPhase};
use ss13_game::health::{Health, Stat};
use ss13_game::human::HumanLook;
use ss13_game::inducer::Inducer;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::mecha::core::ItemCell;
use ss13_game::power::Cell;
use ss13_game::t_scanner::{TRayShown, TScanner};
use ss13_game::testing::{self, fixture_map, steps};
use ss13_game::tools::DoAfter;
use ss13_mapload::{DmItem, HiddenUnderTile};
use ss13_net::server::{AltAppearances, Invisibility, Networked, VisibleToMobs};
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, IconStateId, MouseOpacity, TilePos};

fn server(test: &str) -> App {
    let assets = super::power::fixture_assets(&format!("eng_tools_{test}"));
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn at(x: i32) -> TilePos {
    TilePos::new(x, 2, 1)
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn human(world: &mut World, pos: TilePos) -> Entity {
    world
        .spawn((
            pos,
            HumanLook::random_assistant(1),
            Health::default(),
            Stat::Conscious,
            Inventory::default(),
        ))
        .id()
}

fn hidden_cable(world: &mut World, pos: TilePos) -> Entity {
    world
        .spawn((
            pos,
            Appearance {
                icon_state: IconStateId(1),
                layer: ss13_shared::Layer::default(),
                plane: 0,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                overlays: Vec::new(),
                mouse_opacity: ss13_shared::MouseOpacity::default(),
            },
            HiddenUnderTile,
        ))
        .id()
}

#[test]
fn t_ray_shows_hidden_objects_to_its_holder_only() {
    let mut app = server("t_ray");
    let world = app.world_mut();
    let mob = human(world, at(3));
    let near = hidden_cable(world, at(5));
    let far = hidden_cable(world, at(8));
    let scanner = world
        .spawn(DmItem {
            path: "/obj/item/t_scanner".into(),
            ..default()
        })
        .id();
    world.entity_mut(scanner).insert(CarriedBy(mob));
    world.get_mut::<Inventory>(mob).unwrap().hands[0] = Some(scanner);
    app.update();
    assert_eq!(
        app.world().get::<TScanner>(scanner),
        Some(&TScanner::default())
    );
    // Off: nothing shows.
    run(&mut app, 1.5);
    assert!(app.world().get::<Networked>(near).is_none());

    app.world_mut().get_mut::<TScanner>(scanner).unwrap().on = true;
    // The next pulse of the shared 1 s clock.
    for _ in 0..steps(1.1) {
        app.update();
        if app.world().get::<Networked>(near).is_some() {
            break;
        }
    }
    let world = app.world();
    assert!(world.get::<Networked>(near).is_some(), "within 3 tiles");
    assert!(world.get::<TRayShown>(near).is_some());
    assert_eq!(
        world.get::<Invisibility>(near),
        Some(&Invisibility(u8::MAX))
    );
    assert_eq!(
        world.get::<VisibleToMobs>(near).map(|v| v.0.clone()),
        Some(vec![mob])
    );
    let alt = &world.get::<AltAppearances>(near).unwrap().0[0];
    assert_eq!(alt.seers.mobs, [mob]);
    assert_eq!(alt.appearance.color, Some([255, 255, 255, 128]));
    assert_eq!(alt.appearance.mouse_opacity, MouseOpacity::Transparent);
    assert!(world.get::<Networked>(far).is_none(), "5 tiles away");

    // Switched off: the last flick ends and no new one starts.
    app.world_mut().get_mut::<TScanner>(scanner).unwrap().on = false;
    run(&mut app, 2.0);
    let world = app.world();
    assert!(world.get::<Networked>(near).is_none());
    assert!(world.get::<Invisibility>(near).is_none());
    assert!(world.get::<AltAppearances>(near).is_none());
}

fn battery(world: &mut World, charge: f64) -> Entity {
    world
        .spawn((
            DmItem {
                path: "/obj/item/stock_parts/power_store/battery/upgraded".into(),
                ..default()
            },
            ItemCell(Cell {
                charge,
                maxcharge: 10_000_000.0,
                chargerate: 0.0,
            }),
        ))
        .id()
}

#[test]
fn inducer_charges_a_battery_by_its_rating() {
    let mut app = server("inducer");
    let world = app.world_mut();
    let mob = human(world, at(3));
    let inducer = world
        .spawn(DmItem {
            path: "/obj/item/inducer/empty".into(),
            ..default()
        })
        .id();
    world.entity_mut(inducer).insert(CarriedBy(mob));
    world.get_mut::<Inventory>(mob).unwrap().hands[0] = Some(inducer);
    let own = battery(world, 3_000_000.0);
    let target = battery(world, 0.0);
    world.entity_mut(target).insert(at(4));
    app.update();
    let world = app.world_mut();
    ss13_game::storage::put_inside(world, own, inducer);
    {
        let mut state = world.get_mut::<Inducer>(inducer).expect("an inducer");
        state.cell = Some(own);
        state.opened = false;
    }
    let ctx = ClickCtx {
        client: mob,
        mob,
        mob_pos: at(3),
        held: Some(inducer),
        target,
        pos: at(4),
        modifiers: ClickModifiers::default(),
        active_hand: 0,
    };
    assert!(click_handlers::run(world, ClickPhase::HeldOnTarget, &ctx));
    assert!(world.get::<DoAfter>(mob).is_some());
    let charge = |app: &App, e: Entity| app.world().get::<ItemCell>(e).unwrap().0.charge;
    run(&mut app, 1.1);
    assert_eq!(charge(&app, target), 1_000_000.0);
    assert_eq!(charge(&app, own), 2_000_000.0);
    run(&mut app, 2.5);
    assert_eq!(charge(&app, target), 3_000_000.0);
    assert_eq!(charge(&app, own), 0.0);
    assert!(app.world().get::<DoAfter>(mob).is_none(), "out of charge");
}

fn held(world: &mut World, mob: Entity, path: &str, tool: Option<(&str, f32)>) -> Entity {
    let item = world
        .spawn(DmItem {
            path: path.into(),
            ..default()
        })
        .id();
    if let Some((behaviour, speed)) = tool {
        world.entity_mut(item).insert(ss13_mapload::DmTool {
            behaviour: behaviour.into(),
            speed,
            ..default()
        });
    }
    world.entity_mut(item).insert(CarriedBy(mob));
    world.get_mut::<Inventory>(mob).unwrap().hands[0] = Some(item);
    item
}

/// Nuke core theft (`ss13_game::antag::nukeop::core_theft`): the ultra-thin screwdriver
/// (toolspeed 0.5) unscrews the panel in 3 s, a plain one does nothing; a crowbar pries the
/// panel; prying the cut plate exposes the pulsing core; a container takes it in 5 s and seals
/// 5 s later, which stops the radiation.
#[test]
fn nuke_core_is_stolen_into_a_sealed_container() {
    use ss13_game::antag::nukeop::bomb::NuclearBomb;
    use ss13_game::antag::nukeop::core_theft::{
        CoreContainer, Decon, NUKE_CORE, NUKE_SCREWDRIVER, NukeInterior,
    };
    use ss13_game::radiation::RadioactiveEmitter;

    let mut app = server("nuke_core");
    let world = app.world_mut();
    let mob = human(world, at(3));
    let core = world
        .spawn(DmItem {
            path: NUKE_CORE.into(),
            ..default()
        })
        .id();
    let bomb = world
        .spawn((
            at(4),
            NuclearBomb {
                state: default(),
                disk: None,
                syndicate: false,
                actually_explode_at: None,
                previous_level: default(),
            },
            NukeInterior {
                state: Decon::Intact,
                core: Some(core),
            },
        ))
        .id();
    ss13_game::storage::put_inside(world, core, bomb);
    let ctx = |held: Entity| ClickCtx {
        client: mob,
        mob,
        mob_pos: at(3),
        held: Some(held),
        target: bomb,
        pos: at(4),
        modifiers: ClickModifiers::default(),
        active_hand: 0,
    };
    let state = |app: &App| app.world().get::<NukeInterior>(bomb).unwrap().state;

    let plain = held(
        world,
        mob,
        "/obj/item/screwdriver",
        Some(("screwdriver", 1.0)),
    );
    assert!(!click_handlers::run(
        world,
        ClickPhase::HeldOnTarget,
        &ctx(plain)
    ));
    world.despawn(plain);

    let screwdriver = held(world, mob, NUKE_SCREWDRIVER, Some(("screwdriver", 0.5)));
    assert!(click_handlers::run(
        world,
        ClickPhase::HeldOnTarget,
        &ctx(screwdriver)
    ));
    run(&mut app, 3.2);
    assert_eq!(state(&app), Decon::Unscrewed);

    let world = app.world_mut();
    world.despawn(screwdriver);
    let crowbar = held(world, mob, "/obj/item/crowbar", Some(("crowbar", 1.0)));
    assert!(click_handlers::run(
        world,
        ClickPhase::HeldOnTarget,
        &ctx(crowbar)
    ));
    run(&mut app, 3.2);
    assert_eq!(state(&app), Decon::PanelRemoved);

    // The welder's cut is the same kind of step; skip to the cut plate.
    let world = app.world_mut();
    world.get_mut::<NukeInterior>(bomb).unwrap().state = Decon::Welded;
    assert!(click_handlers::run(
        world,
        ClickPhase::HeldOnTarget,
        &ctx(crowbar)
    ));
    run(&mut app, 3.2);
    assert_eq!(state(&app), Decon::CoreExposed);
    assert!(
        app.world().get::<RadioactiveEmitter>(core).is_some(),
        "exposed: pulsing"
    );

    let world = app.world_mut();
    world.despawn(crowbar);
    let container = held(world, mob, "/obj/item/nuke_core_container", None);
    assert!(click_handlers::run(
        world,
        ClickPhase::HeldOnTarget,
        &ctx(container)
    ));
    run(&mut app, 5.2);
    assert_eq!(state(&app), Decon::CoreRemoved);
    assert_eq!(app.world().get::<NukeInterior>(bomb).unwrap().core, None);
    let loaded = *app.world().get::<CoreContainer>(container).unwrap();
    assert_eq!(loaded.core, Some(core));
    assert!(!loaded.sealed);
    assert!(app.world().get::<RadioactiveEmitter>(core).is_some());
    run(&mut app, 5.2);
    assert!(app.world().get::<CoreContainer>(container).unwrap().sealed);
    assert!(
        app.world().get::<RadioactiveEmitter>(core).is_none(),
        "sealed"
    );
}
