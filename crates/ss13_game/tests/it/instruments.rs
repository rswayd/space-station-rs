//! Instruments over a loopback server: a click on a piano opens its song editor; playing a
//! two-note song sends each note to the player as its nearest sample at the note's pitch, and
//! the song stops at its end.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::instruments::{InstrumentDb, Song};
use ss13_game::instruments_ui::{InstrumentEditorData, InstrumentEditorUi};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::instruments::{InstrumentData, InstrumentHolder, Instruments};
use ss13_net::sound::client::{ChannelSoundReceived, SoundReceived};
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::SoundEvent;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::{UiInterface, decode};

const PIANO: &str = "/obj/structure/musician/piano";
const C2: &str = "sound/p/c2.ogg";
const C3: &str = "sound/p/c3.ogg";

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
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
    "/obj/item": { "parent": "/obj" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/musician": { "parent": "/obj/structure", "declared_vars": {
        "name": "Not A Piano", "icon": { "resource": "m.dmi" }, "icon_state": "piano" } },
    "/obj/structure/musician/piano": { "parent": "/obj/structure/musician", "declared_vars": {
        "name": "space piano", "density": 1, "anchored": 1 } },
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
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfffw
wsffw
wfffw
wwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("m.dmi", "piano"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("instruments_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        PIANO,
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// The windows opened for the client and the sounds it got.
#[derive(Resource, Default)]
struct Got {
    windows: Vec<(u32, String, Vec<u8>)>,
    sounds: Vec<SoundEvent>,
}

fn record(
    mut ui: MessageReader<UiMessageReceived>,
    mut sounds: MessageReader<SoundReceived>,
    mut channel_sounds: MessageReader<ChannelSoundReceived>,
    mut got: ResMut<Got>,
) {
    for UiMessageReceived(message) in ui.read() {
        if let UiServerMessage::Open {
            window,
            interface,
            data,
            ..
        } = message
        {
            got.windows.push((*window, interface.clone(), data.clone()));
        }
    }
    got.sounds.extend(sounds.read().map(|s| s.0.clone()));
    // `playsound` comes as a sound datum (`ChannelSound`).
    got.sounds
        .extend(channel_sounds.read().filter_map(|s| s.0.sound.clone()));
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn got(&self) -> &Got {
        self.players[0].app.world().resource::<Got>()
    }
}

fn db() -> InstrumentDb {
    let mut data = Instruments::default();
    data.instruments.insert(
        "r3grand".into(),
        InstrumentData {
            name: "FluidR3 Grand Piano".into(),
            category: "Piano".into(),
            samples: [(36, C2.to_owned()), (48, C3.to_owned())].into(),
            volume_multiplier: 0.33,
            ..Default::default()
        },
    );
    data.holders.insert(
        PIANO.into(),
        InstrumentHolder {
            allowed: vec!["r3grand".into()],
            range: 15.0,
            stationary: true,
            ..Default::default()
        },
    );
    InstrumentDb(data)
}

#[test]
fn a_piano_plays_its_song_at_the_notes_pitch() {
    let assets = fixture_assets("a_piano_plays_its_song_at_the_notes_pitch");
    let mut s = Station(testing::Station::start(
        fixture_map(&assets, &["map.dmm"]),
        |_| {},
    ));
    s.join_all(1, |app| {
        app.init_resource::<Got>().add_systems(Update, record);
    });
    let a = s.players[0].mob;
    let world = s.server.world_mut();
    world.entity_mut(a).insert(TilePos::new(2, 3, 1));
    world.insert_resource(db());
    let piano = ss13_game::construction::spawn_obj(world, PIANO, TilePos::new(3, 3, 1))
        .expect("piano prototype");
    s.run_until("the piano has its song", |s| {
        s.try_get::<Song>(piano).is_some()
    });
    s.run_until("A sees the piano", |s| s.net_id(0, piano).is_some());

    s.click(0, piano);
    s.run_until("the editor opens", |s| {
        s.got()
            .windows
            .iter()
            .any(|(_, i, _)| i == InstrumentEditorUi::NAME)
    });
    let (window, data) = s
        .got()
        .windows
        .iter()
        .find(|(_, i, _)| i == InstrumentEditorUi::NAME)
        .map(|(w, _, d)| (*w, d.clone()))
        .unwrap();
    let data = decode::<InstrumentEditorData>(&data).unwrap();
    assert_eq!(data.using_instrument, "FluidR3 Grand Piano");
    assert!(data.instrument_ready && !data.playing);
    assert_eq!(data.bpm, 120);

    // C3 is the c2 sample as recorded; G3 (key 43) is past the pivot (42): c3, 5 semitones down.
    s.server.world_mut().get_mut::<Song>(piano).unwrap().lines = vec!["C3,G3".into()];
    s.act(0, window, "play_music", &[]);
    s.run_until("both notes are heard", |s| s.got().sounds.len() >= 2);
    let sounds = s.got().sounds.clone();
    assert_eq!(sounds[0].file, C2);
    assert!((sounds[0].speed - 1.0).abs() < 1e-4);
    assert_eq!(sounds[1].file, C3);
    assert!((sounds[1].speed - 2f32.powf(-5.0 / 12.0)).abs() < 1e-4);
    // 35 volume × 0.33 at most (no distance falloff; the air may lower it).
    assert!(
        sounds[0].volume > 0.0 && sounds[0].volume <= 11.56,
        "{}",
        sounds[0].volume
    );
    s.run_until("the song ends", |s| !s.get::<Song>(piano).playing);
}
