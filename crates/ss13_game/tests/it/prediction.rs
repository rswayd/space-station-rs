//! Own-mob prediction over real WebTransport on 127.0.0.1 with 50 ms of simulated latency on
//! each side (100 ms round trip): holding a direction, walking into a wall (never predicted into
//! it: walls replicate as blockers), through a door that opens on the bump (no bounce), and key
//! spam. Every frame the predicted tile may lead the server's by at
//! most one step, and after each phase prediction, ack and server agree.
//!
//! `cargo test -p ss13_game --all-features --test it prediction:: -- --nocapture` prints the
//! predicted / acked / server tile log.

use core::time::Duration;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_game::RUN_DELAY_SECS;
use ss13_game::prediction::{MoveInput, Prediction, PredictionPlugin};
use ss13_game::testing::{Drive, Station, fixture_map};
use ss13_mapload::MapLoadPlugin;
use ss13_mapload::convert::{DmTypes, convert};
use ss13_net::SimLatency;
use ss13_net::client::NetEntityMap;
use ss13_shared::{Dir, TilePos};

const ONE_WAY: Duration = Duration::from_millis(50);

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "declared_vars": { "icon_state": "Assistant" } },
    "/obj/effect/mapping_helpers": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "m.dmi" }, "icon_state": "", "layer": 5 } },
    "/obj/effect/mapping_helpers/airlock": { "parent": "/obj/effect/mapping_helpers" },
    "/obj/effect/mapping_helpers/airlock/locked": { "parent": "/obj/effect/mapping_helpers/airlock",
        "declared_vars": { "icon_state": "airlock_locked_helper" } },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "autoclose": 1,
        "base_icon_state": null, "normalspeed": 1, "overlays_file": { "resource": "o.dmi" },
        "airlock_material": null } },
    "/obj/machinery/door/firedoor": { "parent": "/obj/machinery/door", "declared_vars": {
        "can_open_with_hands": 0, "closingLayer": 3.11, "density": 0, "glass": 1,
        "icon": { "resource": "f.dmi" }, "icon_state": "door_open", "layer": 2.6, "opacity": 0,
        "safe": 0 } },
    "/obj/machinery/door/poddoor": { "parent": "/obj/machinery/door", "declared_vars": {
        "can_open_with_hands": 0, "closingLayer": 3.3, "icon": { "resource": "s.dmi" },
        "icon_state": "closed", "layer": 2.65, "safe": 0 } },
    "/obj/machinery/door/poddoor/shutters": { "parent": "/obj/machinery/door/poddoor" },
    "/obj/machinery/door/poddoor/shutters/preopen": { "parent": "/obj/machinery/door/poddoor/shutters",
        "declared_vars": { "density": 0, "icon_state": "open", "opacity": 0 } },
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

/// Rows top-down: the last line is y = 1. The spawn row runs east into a wall at x = 17; the
/// shaft at x = 16 leads down to the lower row, which has an airlock at x = 12.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"D" = (/obj/machinery/door/airlock,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwwwwwwwww
wsffffffffffffffww
wwwwwwwwwwwwwwwfww
wffffffffffDffffww
wwwwwwwwwwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 4, 1);
const EAST_END: TilePos = TilePos::new(16, 4, 1);
const SHAFT_BOTTOM: TilePos = TilePos::new(16, 2, 1);
const DOOR_X: i32 = 12;

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 22] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("t.dmi", ""),
    ("l.dmi", "x2"),
    ("l.dmi", "Assistant"),
    ("h.dmi", "human_basic"),
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("a.dmi", "closing"),
    ("a.dmi", "fill_closed"),
    ("a.dmi", "fill_opening"),
    ("a.dmi", "fill_open"),
    ("a.dmi", "fill_closing"),
    ("o.dmi", "lights_bolts"),
    ("o.dmi", "lights_opening"),
    ("o.dmi", "lights_closing"),
    ("o.dmi", "welded"),
    ("s.dmi", "open"),
    ("s.dmi", "closed"),
    ("f.dmi", "door_open"),
    ("m.dmi", "airlock_locked_helper"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("prediction_fixture")
        .join(test);
    std::fs::create_dir_all(dir.join("prototypes")).unwrap();
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/mapping_helpers/airlock/locked",
        "/obj/machinery/door/airlock",
        "/obj/machinery/door/firedoor",
        "/obj/machinery/door/poddoor/shutters/preopen",
        "/mob/living/carbon/human",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let conversion = convert(&types, used).unwrap();
    std::fs::write(dir.join("prototypes/dm_types.ron"), conversion.ron).unwrap();
    let manifest = AssetManifest {
        atlases: vec![],
        states: STATES
            .iter()
            .map(|&(dmi, state)| IconStateInfo {
                dmi: dmi.into(),
                state: state.into(),
                atlas: 0,
                width: 32,
                height: 32,
                dirs: 1,
                rects: vec![[0, 0, 32, 32]],
                delays: vec![],
                loop_count: 0,
                rewind: false,
                movement: false,
            })
            .collect(),
    };
    std::fs::write(
        dir.join(ss13_assets::import::MANIFEST_FILE),
        postcard::to_stdvec(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("map.dmm"), MAP).unwrap();
    dir
}

/// A started server and one joined, predicting client. Unlike the other harnesses this one runs
/// on real time: the simulated latency is real time in the transport, and what is checked is how
/// prediction copes with it frame by frame.
struct Session {
    st: Station,
    mob: Entity,
    /// `(predicted, acked, server)` at the last logged change.
    logged: Option<(TilePos, TilePos, TilePos)>,
    start: Instant,
    max_lead: u32,
}

impl Session {
    fn start(map: MapLoadPlugin) -> Self {
        let real_time = |app: &mut App| {
            app.insert_resource(TimeUpdateStrategy::Automatic)
                .insert_resource(SimLatency(ONE_WAY));
        };
        let mut st = Station::start(map, real_time);
        let p = st.join("a", |client| {
            real_time(client);
            client.add_plugins(PredictionPlugin);
        });
        st.run_until("the client predicts", |s| {
            s.players[p]
                .app
                .world()
                .resource::<Prediction>()
                .predicted()
                .is_some()
        });
        let mob = st.players[p].mob;
        Self {
            st,
            mob,
            logged: None,
            start: Instant::now(),
            max_lead: 0,
        }
    }

    fn client(&self) -> &App {
        &self.st.players[0].app
    }

    fn prediction(&self) -> &Prediction {
        self.client().world().resource::<Prediction>()
    }

    fn predicted(&self) -> TilePos {
        self.prediction().predicted().unwrap()
    }

    fn acked(&self) -> TilePos {
        self.prediction().authoritative().unwrap().pos
    }

    fn server_pos(&self) -> TilePos {
        *self.st.get::<TilePos>(self.mob)
    }

    fn hold(&mut self, dir: Option<Dir>) {
        self.st.players[0]
            .app
            .world_mut()
            .resource_mut::<MoveInput>()
            .0 = dir;
    }

    /// One frame of both apps; checks the invariants and logs every change of the three tiles.
    fn step(&mut self) {
        self.st.tick();
        let (predicted, acked, server) = (self.predicted(), self.acked(), self.server_pos());
        let lead = predicted.chebyshev(server).unwrap();
        self.max_lead = self.max_lead.max(lead);
        assert!(
            lead <= 1,
            "prediction {predicted:?} ran {lead} steps ahead of the server {server:?}"
        );
        let own = self
            .client()
            .world()
            .resource::<NetEntityMap>()
            .controlled();
        let drawn = own.and_then(|e| self.client().world().get::<TilePos>(e).copied());
        assert_eq!(
            drawn,
            Some(predicted),
            "the own mob is drawn where predicted"
        );
        if self.logged != Some((predicted, acked, server)) {
            self.logged = Some((predicted, acked, server));
            println!(
                "{:7.3}s predicted ({:2},{}) acked ({:2},{}) server ({:2},{}) unacked {}",
                self.start.elapsed().as_secs_f64(),
                predicted.x,
                predicted.y,
                acked.x,
                acked.y,
                server.x,
                server.y,
                self.prediction().unacked()
            );
        }
    }

    fn run_for(&mut self, secs: f64, mut each: impl FnMut(&mut Self)) {
        let start = Instant::now();
        while start.elapsed().as_secs_f64() < secs {
            each(self);
            self.step();
        }
    }

    fn run_until(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let start = Instant::now();
        while !done(self) {
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "timed out: {what}"
            );
            self.step();
        }
    }

    /// Released key: everything sent is acked and all three tiles agree.
    fn settle(&mut self, what: &str) -> TilePos {
        self.hold(None);
        self.run_until(what, |s| {
            s.prediction().unacked() == 0 && s.predicted() == s.server_pos()
        });
        // Nothing left to arrive changes it.
        self.run_for(0.3, |_| {});
        assert_eq!(self.predicted(), self.server_pos(), "{what}");
        assert_eq!(self.acked(), self.server_pos(), "{what}");
        println!("-- {what}: {:?}", self.predicted());
        self.predicted()
    }
}

#[test]
fn prediction_under_latency() {
    let assets = fixture_assets("all");
    let mut s = Session::start(fixture_map(&assets, &["map.dmm"]));
    assert_eq!(s.predicted(), SPAWN);

    // The step shows at once, its ack after the simulated round trip.
    s.hold(Some(Dir::East));
    s.step();
    let sent = Instant::now();
    assert_eq!(
        s.predicted(),
        SPAWN.step(Dir::East),
        "predicted without waiting"
    );
    s.run_until("the first ack", |s| s.acked() != SPAWN);
    let rtt = sent.elapsed().as_secs_f64();
    println!("-- first step acked after {:.0} ms", rtt * 1000.0);
    assert!(rtt >= 0.09, "simulated latency is not applied ({rtt:.3}s)");

    // 1. Held for 2 s: prediction and server walk the same path, nothing corrected.
    s.run_for(2.0, |s| s.hold(Some(Dir::East)));
    let held = s.settle("held east for 2 s");
    let steps = held.x - SPAWN.x;
    let allowed = (2.0 / f64::from(RUN_DELAY_SECS)).ceil() as i32 + 1;
    assert!((8..=allowed).contains(&steps), "{steps} steps in 2 s");
    assert_eq!(s.prediction().corrections(), 0);

    // 2. Into the wall: the replicated wall stops the prediction, no step into it, nothing
    //    corrected.
    s.hold(Some(Dir::East));
    s.run_until("the wall-adjacent tile", |s| s.server_pos() == EAST_END);
    let before_wall = s.prediction().corrections();
    let mut changes = 0;
    let mut last = s.predicted();
    s.run_for(1.5, |s| {
        s.hold(Some(Dir::East));
        if s.predicted() != last {
            changes += 1;
            last = s.predicted();
        }
        assert!(s.predicted().x <= EAST_END.x, "predicted into the wall");
    });
    assert_eq!(s.settle("held into the wall"), EAST_END);
    assert_eq!(changes, 0, "predicted moves while held into the wall");
    assert_eq!(
        s.prediction().corrections(),
        before_wall,
        "bounced off the wall"
    );

    // 3. Down the shaft, then west through the airlock that opens on the bump.
    s.hold(Some(Dir::South));
    s.run_until("the shaft bottom", |s| s.server_pos() == SHAFT_BOTTOM);
    s.settle("down the shaft");
    let before_door = s.prediction().corrections();
    s.hold(Some(Dir::West));
    s.run_until("past the door", |s| s.server_pos().x < DOOR_X - 2);
    let past = s.settle("through the door");
    assert!(past.x < DOOR_X, "{past:?} is not past the door");
    // The closed door is not walked into: it opens on the bump, then the step is taken.
    assert_eq!(
        s.prediction().corrections(),
        before_door,
        "bounced off the closed door"
    );

    // 4. Key spam (a new press every other frame) walks no faster than holding and never
    //    outruns the server.
    let before = s.prediction().corrections();
    let from = s.predicted();
    let mut frame = 0u32;
    s.run_for(1.0, |s| {
        frame += 1;
        s.hold(frame.is_multiple_of(2).then_some(Dir::West));
    });
    let spammed = s.settle("spammed west for 1 s");
    let steps = from.x - spammed.x;
    let allowed = (1.0 / f64::from(RUN_DELAY_SECS)).ceil() as i32 + 1;
    assert!(
        (1..=allowed).contains(&steps),
        "{steps} steps of spam in 1 s"
    );
    assert_eq!(s.prediction().corrections(), before);
    println!("-- max lead over the server: {} tile(s)", s.max_lead);
}
