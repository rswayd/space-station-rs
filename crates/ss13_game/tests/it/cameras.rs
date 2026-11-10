//! Cameras on the power fixture room (`super::power`) without clients: a mob's simple bodycam
//! follows it and the network with it; a pausable bodycam is in the network only while a
//! console watches it and it is whole; an advanced console's eye moves with its user's steps
//! and lets go when the user walks away.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_game::health::Stat;
use ss13_game::server::PlayerMob;
use ss13_game::silicon::camera_advanced::{
    AdvancedConsole, RemoteEye, RemoteEyeStep, RemoteEyeUser, attack_hand,
};
use ss13_game::silicon::cameras::{Camera, CameraConsole, CameraNet, CameraPaused};
use ss13_game::silicon::mob_cameras::{
    BodycamAccessory, MobCameraKind, add_simple_bodycam, spawn_mob_camera,
};
use ss13_game::silicon::ui::CameraConsoleStatic;
use ss13_game::testing::{self, fixture_map};
use ss13_shared::{Dir, TilePos};

/// Floor tiles of the fixture's row 2 (`super::emp`).
const AT: TilePos = TilePos::new(4, 2, 1);
const NEXT: TilePos = TilePos::new(5, 2, 1);
const FAR: TilePos = TilePos::new(9, 2, 1);

fn server(test: &str) -> App {
    let assets = super::power::fixture_assets(&format!("cameras_{test}"));
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn visible(app: &App, pos: TilePos) -> bool {
    app.world().resource::<CameraNet>().visible(pos)
}

#[test]
fn simple_bodycam_follows_its_mob() {
    let mut app = server("simple");
    let mob = app.world_mut().spawn(AT).id();
    let camera = add_simple_bodycam(app.world_mut(), mob, "Avatar Bob".into(), "bitrunner", true)
        .expect("a camera");
    assert!(
        add_simple_bodycam(app.world_mut(), mob, "again".into(), "bitrunner", true).is_none(),
        "one per network"
    );
    app.update();
    assert!(visible(&app, AT));
    app.world_mut().entity_mut(mob).insert(FAR);
    app.update();
    app.update();
    assert_eq!(app.world().get::<TilePos>(camera), Some(&FAR));
    assert!(visible(&app, FAR));
    // The mob gone, its camera goes.
    app.world_mut().despawn(mob);
    app.update();
    app.update();
    assert!(app.world().get_entity(camera).is_err());
    assert!(!visible(&app, FAR));
}

#[test]
fn pausable_bodycam_is_seen_only_while_watched() {
    let mut app = server("pausable");
    let world = app.world_mut();
    let accessory = world.spawn(BodycamAccessory::default()).id();
    let mob = world.spawn(AT).id();
    let camera = spawn_mob_camera(
        world,
        mob,
        "!Bob (Нательная камера)".into(),
        vec!["ss13".into()],
        3,
        true,
        MobCameraKind::Pausable,
        Some(accessory),
        false,
    )
    .expect("a camera");
    app.update();
    assert!(app.world().get::<CameraPaused>(camera).is_some());
    assert!(!visible(&app, AT), "nobody watches");

    let console = app
        .world_mut()
        .spawn(CameraConsole {
            statics: Arc::new(CameraConsoleStatic::default()),
            active: Some(camera),
        })
        .id();
    app.update();
    app.update();
    assert!(app.world().get::<CameraPaused>(camera).is_none());
    assert!(visible(&app, AT), "watched");

    app.world_mut()
        .get_mut::<BodycamAccessory>(accessory)
        .unwrap()
        .broken = true;
    app.update();
    app.update();
    assert!(!app.world().get::<Camera>(camera).unwrap().enabled);
    assert_eq!(
        app.world().get::<CameraConsole>(console).unwrap().active,
        None,
        "the console loses it"
    );
    assert!(!visible(&app, AT));
}

#[test]
fn advanced_console_eye_moves_and_lets_go() {
    let mut app = server("advanced");
    let world = app.world_mut();
    // A camera so the console's own turf is seen: the eye starts there.
    world.spawn((
        AT,
        Camera {
            c_tag: "test".into(),
            network: vec!["ss13".into()],
            view_range: 7,
            enabled: true,
        },
    ));
    let console = world
        .spawn((
            AT,
            AdvancedConsole {
                network: vec!["ss13".into()],
                eye: None,
                user: None,
            },
        ))
        .id();
    let client = world.spawn_empty().id();
    let mob = world
        .spawn((AT, Stat::Conscious, PlayerMob { client }))
        .id();
    app.update();
    assert!(attack_hand(app.world_mut(), client, mob, console));
    let user = *app
        .world()
        .get::<RemoteEyeUser>(mob)
        .expect("looking through");
    assert_eq!(app.world().get::<RemoteEye>(user.eye).unwrap().pos, AT);

    app.world_mut().write_message(RemoteEyeStep {
        mob,
        dir: Dir::East,
    });
    app.update();
    assert_eq!(app.world().get::<RemoteEye>(user.eye).unwrap().pos, NEXT);

    // Walking away ends it; the eye stays for the next user.
    app.world_mut().entity_mut(mob).insert(FAR);
    app.update();
    assert!(app.world().get::<RemoteEyeUser>(mob).is_none());
    let state = app.world().get::<AdvancedConsole>(console).unwrap();
    assert_eq!(state.user, None);
    assert_eq!(state.eye, Some(user.eye));
}
