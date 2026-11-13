//! Area ambience (tg `SSambience`, `refresh_looping_ambience`) on a bare server app: a player
//! walking from area A into B gets B's buzz loop on `CHANNEL_BUZZ` instead of A's, a step inside
//! B restarts nothing, an area without a buzz (or without environment power, or a dead mob) stops
//! it; the first ambience sound comes 10 s after the player gets a mob, on `CHANNEL_AMBIENCE`,
//! and the next after the sound's length plus the area's cooldown.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use ss13_game::ambience::{AMBIENCE_VOLUME, AmbiencePlugin, CHANNEL_AMBIENCE, CHANNEL_BUZZ};
use ss13_game::health::Stat;
use ss13_game::power::{Apc, AreaPower, Cell};
use ss13_game::server::PlayerMob;
use ss13_mapload::{Areas, DmArea};
use ss13_net::sound::server::SendChannelSound;
use ss13_shared::TilePos;

const STEP: Duration = Duration::from_millis(200);
const HUM_A: &str = "test/hum_a.ogg";
const HUM_B: &str = "test/hum_b.ogg";
const AMB_A: &str = "test/amb_a.ogg";

#[derive(Resource, Default)]
struct Sent(Vec<SendChannelSound>);

fn record(mut sounds: MessageReader<SendChannelSound>, mut sent: ResMut<Sent>) {
    sent.0.extend(sounds.read().cloned());
}

fn powered_area(world: &mut World, dm: DmArea) -> Entity {
    let area = world.spawn(dm).id();
    let cell = Cell {
        charge: 1000.0,
        maxcharge: 1000.0,
        chargerate: 100.0,
    };
    let apc = world.spawn(Apc::new(area, Some(cell))).id();
    world.entity_mut(area).insert(AreaPower {
        requires_power: true,
        power: [true; 3],
        apc: Some(apc),
        ..default()
    });
    area
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..(secs / STEP.as_secs_f64()).round() as u32 {
        app.update();
    }
}

/// What went out on `channel` since the last call: (file or `None` for a stop, repeat, volume).
fn take(app: &mut App, channel: u16) -> Vec<Sound> {
    let mut sent = app.world_mut().resource_mut::<Sent>();
    let (mine, rest) = std::mem::take(&mut sent.0)
        .into_iter()
        .partition::<Vec<_>, _>(|s| s.sound.channel == channel);
    sent.0 = rest;
    mine.into_iter()
        .map(|s| {
            let volume = s.sound.sound.as_ref().map_or(0.0, |e| e.volume);
            (s.sound.sound.map(|e| e.file), s.sound.repeat, volume)
        })
        .collect()
}

type Sound = (Option<String>, bool, f32);
const STOP: Sound = (None, false, 0.0);

fn play(file: &str, repeat: bool, volume: f32) -> Sound {
    (Some(file.to_owned()), repeat, volume)
}

#[test]
fn area_change_changes_the_buzz_and_ambience_follows_its_cooldown() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(STEP))
        .add_message::<SendChannelSound>()
        .init_resource::<Sent>()
        .add_plugins(AmbiencePlugin)
        .add_systems(PostUpdate, record);
    let world = app.world_mut();
    let a = powered_area(
        world,
        DmArea {
            ambient_buzz: HUM_A.into(),
            ambient_buzz_vol: 35.0,
            ambientsounds: vec![AMB_A.into()],
            // rand(5 s, 5 s): the test knows when the next sound comes.
            min_ambience_cooldown: 5.0,
            max_ambience_cooldown: 5.0,
            ..default()
        },
    );
    let b = powered_area(
        world,
        DmArea {
            ambient_buzz: HUM_B.into(),
            ambient_buzz_vol: 20.0,
            ..default()
        },
    );
    let quiet = powered_area(world, DmArea::default());
    let mut areas = Areas::default();
    for area in [a, b, quiet] {
        areas.add_area(area);
    }
    assert!(areas.add_level(1, 10, 1, a));
    for x in 5..=6 {
        assert!(areas.set_area(TilePos::new(x, 1, 1), b));
    }
    assert!(areas.set_area(TilePos::new(8, 1, 1), quiet));
    world.insert_resource(areas);
    let client = world.spawn_empty().id();
    let mob = world
        .spawn((PlayerMob { client }, TilePos::new(2, 1, 1)))
        .id();

    // Entering A: its buzz loops at its volume (pref 100%).
    run(&mut app, 0.4);
    assert_eq!(take(&mut app, CHANNEL_BUZZ), vec![play(HUM_A, true, 35.0)]);

    // Into B: B's buzz replaces it; a step inside B restarts nothing.
    app.world_mut()
        .entity_mut(mob)
        .insert(TilePos::new(5, 1, 1));
    run(&mut app, 0.4);
    assert_eq!(take(&mut app, CHANNEL_BUZZ), vec![play(HUM_B, true, 20.0)]);
    app.world_mut()
        .entity_mut(mob)
        .insert(TilePos::new(6, 1, 1));
    run(&mut app, 1.2);
    assert!(take(&mut app, CHANNEL_BUZZ).is_empty());

    // An area without a buzz stops it, once.
    app.world_mut()
        .entity_mut(mob)
        .insert(TilePos::new(8, 1, 1));
    run(&mut app, 2.0);
    assert_eq!(take(&mut app, CHANNEL_BUZZ), vec![STOP]);

    // Back in A; then A's environment power goes off: silence within a second.
    app.world_mut()
        .entity_mut(mob)
        .insert(TilePos::new(2, 1, 1));
    run(&mut app, 0.4);
    assert_eq!(take(&mut app, CHANNEL_BUZZ), vec![play(HUM_A, true, 35.0)]);
    app.world_mut().get_mut::<AreaPower>(a).unwrap().power[2] = false;
    run(&mut app, 1.2);
    assert_eq!(take(&mut app, CHANNEL_BUZZ), vec![STOP]);
    app.world_mut().get_mut::<AreaPower>(a).unwrap().power[2] = true;
    run(&mut app, 1.2);
    assert_eq!(take(&mut app, CHANNEL_BUZZ), vec![play(HUM_A, true, 35.0)]);

    // The first ambience sound: 10 s after the first second's pass saw the player (~6.8 s
    // have passed), at 11-12 s.
    let first = take(&mut app, CHANNEL_AMBIENCE);
    assert!(first.is_empty(), "{first:?}");
    run(&mut app, 5.8);
    assert_eq!(
        take(&mut app, CHANNEL_AMBIENCE),
        vec![play(AMB_A, false, AMBIENCE_VOLUME)]
    );
    // The file has no length here: the next one 5 s later, at 16-17 s.
    run(&mut app, 3.0);
    assert!(take(&mut app, CHANNEL_AMBIENCE).is_empty());
    run(&mut app, 2.4);
    assert_eq!(take(&mut app, CHANNEL_AMBIENCE).len(), 1);

    // Dead: `TRAIT_DEAF` stops the buzz and the ambience.
    app.world_mut().entity_mut(mob).insert(Stat::Dead);
    run(&mut app, 1.2);
    assert_eq!(take(&mut app, CHANNEL_BUZZ), vec![STOP]);
    run(&mut app, 12.0);
    assert!(take(&mut app, CHANNEL_AMBIENCE).is_empty());
}
