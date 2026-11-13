//! Storm sounds (tg `area_sound_manager` with the rain storm's playlist) on a bare server app:
//! a player in the storm's area hears the stage's loop on `CHANNEL_WEATHER` with its start
//! sound; a new stage waits for the old loop's next sound and starts with its start sound;
//! leaving the area stops it, coming back starts it without the start sound; the storm's end
//! stops it and drops the manager.

use std::collections::HashSet;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use ss13_game::looping_sound::LoopingSoundPlugin;
use ss13_game::server::PlayerMob;
use ss13_game::sound::SoundRng;
use ss13_game::weather::{
    ActiveWeather, Forced, Stage, WeatherKind, WeatherTrack, end_weather, run_weather_forced,
    weather_sounds,
};
use ss13_mapload::{Areas, DmArea};
use ss13_net::sound::server::SendChannelSound;
use ss13_shared::TilePos;
use ss13_shared::protocol::{CHANNEL_WEATHER, SoundCategory};

const STEP: Duration = Duration::from_millis(200);
const RAIN_START: &str = "sound/ambience/weather/rain/rain_start.ogg";
const RAIN_MID: &str = "sound/ambience/weather/rain/rain_mid.ogg";

#[derive(Resource, Default)]
struct Sent(Vec<SendChannelSound>);

fn record(mut sounds: MessageReader<SendChannelSound>, mut sent: ResMut<Sent>) {
    sent.0.extend(sounds.read().cloned());
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..(secs / STEP.as_secs_f64()).round() as u32 {
        app.update();
    }
}

/// The files (`None`: a stop) sent on the weather channel since the last call.
fn take(app: &mut App) -> Vec<Option<String>> {
    std::mem::take(&mut app.world_mut().resource_mut::<Sent>().0)
        .into_iter()
        .filter(|s| s.sound.channel == CHANNEL_WEATHER)
        .map(|s| {
            if let Some(e) = &s.sound.sound {
                assert_eq!(e.volume, 70.0);
                assert_eq!(s.sound.fx.category, SoundCategory::Game);
            }
            s.sound.sound.map(|e| e.file)
        })
        .collect()
}

fn play(file: &str) -> Option<String> {
    Some(file.to_owned())
}

#[test]
fn rain_follows_the_player_and_the_stages() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(STEP))
        .init_resource::<SoundRng>()
        .init_resource::<ActiveWeather>()
        .init_resource::<Sent>()
        .add_plugins(LoopingSoundPlugin)
        .add_systems(Update, weather_sounds)
        .add_systems(PostUpdate, record);
    let world = app.world_mut();
    let outside = world
        .spawn(DmArea {
            outdoors: true,
            ..default()
        })
        .id();
    let inside = world.spawn(DmArea::default()).id();
    let mut areas = Areas::default();
    areas.add_area(outside);
    areas.add_area(inside);
    assert!(areas.add_level(1, 10, 1, outside));
    for x in 6..=10 {
        assert!(areas.set_area(TilePos::new(x, 1, 1), inside));
    }
    world.insert_resource(areas);
    let client = world.spawn_empty().id();
    let mob = world
        .spawn((PlayerMob { client }, TilePos::new(2, 1, 1)))
        .id();
    run_weather_forced(
        world,
        WeatherKind::RainStorm,
        vec![1],
        Forced {
            areas: HashSet::from([outside]),
            duration: 100.0,
            end: 10.0,
            ..default()
        },
    );

    // Telegraph: `rain/start` with its start sound.
    run(&mut app, 0.4);
    assert_eq!(take(&mut app), vec![play(RAIN_START)]);
    assert!(app.world().get::<WeatherTrack>(mob).is_some());

    // The main stage: `rain/middle` waits for the old loop's timer (12.5 s from the start),
    // then plays its own start sound and, 12.5 s later, its mid.
    app.world_mut().resource_mut::<ActiveWeather>().0[0].stage = Stage::Main;
    run(&mut app, 11.0);
    assert!(take(&mut app).is_empty(), "the old sound plays out");
    run(&mut app, 2.0);
    assert_eq!(take(&mut app), vec![play(RAIN_START)]);
    run(&mut app, 13.0);
    assert_eq!(take(&mut app), vec![play(RAIN_MID)]);

    // Indoors (not impacted): stopped; back outside: the mid at once, no start sound.
    app.world_mut()
        .entity_mut(mob)
        .insert(TilePos::new(7, 1, 1));
    run(&mut app, 0.4);
    assert_eq!(take(&mut app), vec![None]);
    app.world_mut()
        .entity_mut(mob)
        .insert(TilePos::new(3, 1, 1));
    run(&mut app, 0.4);
    assert_eq!(take(&mut app), vec![play(RAIN_MID)]);

    // The storm ends: stopped, the manager is gone.
    end_weather(app.world_mut(), WeatherKind::RainStorm);
    run(&mut app, 0.4);
    assert_eq!(take(&mut app), vec![None]);
    assert!(app.world().get::<WeatherTrack>(mob).is_none());
}
