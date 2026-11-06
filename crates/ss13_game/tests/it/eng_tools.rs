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
