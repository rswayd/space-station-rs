//! Doors over real WebTransport on 127.0.0.1: a fixture station with airlocks (one fast-closing,
//! two bolted), a firelock and an open shutter; a headless client bumps, walks through, clicks.
//!
//! `cargo test -p ss13_game --all-features --test it doors::metastation -- --ignored` walks out of
//! the MetaStation spawn room through a door (needs generated assets in `SS13_ASSETS`).

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::doors::{Door, DoorReport, DoorState};
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::server::MoveQueue;
use ss13_game::testing::{self, Drive, steps, write_fixture};
use ss13_mapload::{DmAirlockHelper, DoorKind, MapLoadPlugin, Opaque};
use ss13_net::client::NetEntity;
use ss13_net::server::{Interest, Viewer};
use ss13_shared::protocol::{ClickModifiers, ClientIntent, NetId};
use ss13_shared::{Appearance, Dir, IconStateId, TilePos};
use ss13_world::{Density, SpatialIndex};

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
    "/obj/item": { "parent": "/obj", "declared_vars": { "interaction_flags_item": 1 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
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

/// Rows top-down: the last line is y = 1. Column 5 separates the spawn room from the east hall:
/// `B` (5,5) bolted by a var edit, `L` (5,4) bolted by a helper, `D` (5,3) a fast airlock
/// (`normalspeed = 0`: autoclose 1.5 s; needs maintenance access, which assistants have), `P` (5,2) an open shutter. `F` (7,3) is a firelock.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"D" = (/obj/machinery/door/airlock{normalspeed = 0; req_access = list("maint_tunnels")},/turf/open/floor,/area/space)
"L" = (/obj/machinery/door/airlock,/obj/effect/mapping_helpers/airlock/locked,/turf/open/floor,/area/space)
"B" = (/obj/machinery/door/airlock{locked = 1},/turf/open/floor,/area/space)
"P" = (/obj/machinery/door/poddoor/shutters/preopen,/turf/open/floor,/area/space)
"F" = (/obj/machinery/door/firedoor,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwww
wfffBffffffw
wfffLffffffw
wfsfDfFffffw
wfffPffffffw
wwwwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const D: TilePos = TilePos::new(5, 3, 1);
const L: TilePos = TilePos::new(5, 4, 1);
const B: TilePos = TilePos::new(5, 5, 1);
const P: TilePos = TilePos::new(5, 2, 1);
const F: TilePos = TilePos::new(7, 3, 1);

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
const AIRLOCK_CLOSED: IconStateId = IconStateId(6);
const AIRLOCK_OPEN: IconStateId = IconStateId(8);
const FILL_CLOSED: IconStateId = IconStateId(10);
const BOLT_LIGHTS: IconStateId = IconStateId(14);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("doors_fixture")
        .join(test);
    // The player mob and the assistant kit (`RUNTIME_TYPES`), whose ID opens `D`.
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/mapping_helpers/airlock/locked",
        "/obj/machinery/door/airlock",
        "/obj/machinery/door/firedoor",
        "/obj/machinery/door/poddoor/shutters/preopen",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// Every icon state the client applied to each replicated entity, in order: transient states
/// (a door's `open` between autoclose timers) are checked here instead of being polled for.
#[derive(Resource, Default)]
struct ClientLooks(Vec<(NetId, IconStateId)>);

fn record_look(
    insert: On<Insert<Appearance>>,
    entities: Query<(&NetEntity, &Appearance)>,
    mut looks: ResMut<ClientLooks>,
) {
    if let Ok((id, look)) = entities.get(insert.entity) {
        looks.0.push((id.0, look.icon_state));
    }
}

/// A started server and one joined client, with the client's connection entity and mob. Every
/// frame is exactly one tick on both sides, however slow the machine: the door timers and the
/// waits below count game time.
struct Session {
    st: testing::Station,
    link: Entity,
    mob: Entity,
}

ss13_game::drive_station!(Session, st);

impl Session {
    fn start(map: MapLoadPlugin) -> Self {
        let mut st = testing::Station::start(map, |_| {});
        let p = st.join("a", |client| {
            client
                .init_resource::<ClientLooks>()
                .add_observer(record_look);
        });
        let (link, mob) = (st.players[p].link, st.players[p].mob);
        Self { st, link, mob }
    }

    fn client(&self) -> &App {
        &self.players[0].app
    }

    fn pos(&self) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob).unwrap()
    }

    fn send(&mut self, intent: ClientIntent) {
        self.st.send(0, intent);
    }

    /// A held key: the next step goes out once the server applied the previous one.
    fn press(&mut self, dir: Dir) {
        if self.in_flight() == 0 {
            self.players[0].seq += 1;
            let seq = self.players[0].seq;
            self.send(ClientIntent::Move { dir, seq });
        }
    }

    /// Steps the mob one tile and waits until it arrived, holding the key.
    fn walk(&mut self, dir: Dir) {
        self.run_until("steps in flight", |s| s.in_flight() == 0);
        let to = self.pos().step(dir);
        self.walk_to(dir, to);
    }

    /// Holds `dir` until the mob is on `to`; the last step pressed is the one that got there.
    fn walk_to(&mut self, dir: Dir, to: TilePos) {
        self.run_until(&format!("walk {dir:?} to {to:?}"), |s| {
            if s.pos() == to {
                return true;
            }
            s.press(dir);
            false
        });
    }

    /// Steps sent and not applied by the server yet.
    fn in_flight(&self) -> u32 {
        let queue = self.server.world().get::<MoveQueue>(self.mob).unwrap();
        self.players[0].seq - queue.applied()
    }

    /// Holds `dir` for a few step delays; the mob must not move (it bumps every step).
    fn bump(&mut self, dir: Dir) {
        let at = self.pos();
        for _ in 0..steps(3.0 * f64::from(ss13_game::RUN_DELAY_SECS)) {
            self.press(dir);
            self.step();
            assert_eq!(self.pos(), at, "bumping {dir:?} moved the mob");
        }
    }

    /// The door on `pos`; a closed one first when a firelock shares the tile with an airlock.
    fn door(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        let doors: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| world.get::<Door>(e).is_some())
            .collect();
        doors
            .iter()
            .copied()
            .find(|&e| self.dense(e))
            .or(doors.first().copied())
            .unwrap_or_else(|| panic!("no door at {pos:?}"))
    }

    fn state(&self, door: Entity) -> DoorState {
        self.server.world().get::<Door>(door).unwrap().state
    }

    fn dense(&self, door: Entity) -> bool {
        self.server.world().get::<Density>(door).is_some()
    }

    fn net_id(&self, door: Entity) -> NetId {
        self.server
            .world()
            .resource::<Interest>()
            .net_id(self.link, door)
            .expect("the door was replicated")
    }

    fn click(&mut self, door: Entity) {
        let target = self.net_id(door);
        self.send(ClientIntent::Click {
            target,
            modifiers: ClickModifiers::default(),
        });
    }

    /// Whether the client ever drew `door` in `state`.
    fn client_drew(&self, door: NetId, state: IconStateId) -> bool {
        self.client()
            .world()
            .resource::<ClientLooks>()
            .0
            .contains(&(door, state))
    }

    fn client_sees(&mut self, door: NetId) -> bool {
        let world = self.players[0].app.world_mut();
        let mut q = world.query::<&NetEntity>();
        q.iter(world).any(|n| n.0 == door)
    }
}

fn is_open(state: DoorState) -> bool {
    state == DoorState::Open
}

#[test]
fn bump_pass_autoclose_click_bolts() {
    let assets = fixture_assets("all");
    let mut s = Session::start(MapLoadPlugin {
        source: assets.clone(),
        maps: vec!["map.dmm".into()],
        assets,
    });
    assert_eq!(s.pos(), SPAWN);
    let report = s.server.world().resource::<DoorReport>();
    assert_eq!(report.by_kind.get(&DoorKind::Airlock), Some(&3));
    assert_eq!(report.by_kind.get(&DoorKind::Firedoor), Some(&1));
    assert_eq!(report.by_kind.get(&DoorKind::Poddoor), Some(&1));
    assert_eq!((report.helpers, report.sealed), (1, 2));

    let (d, l, b, p, f) = (s.door(D), s.door(L), s.door(B), s.door(P), s.door(F));
    {
        let door = s.server.world().get::<Door>(d).unwrap();
        // The assistant's ID has it: bumps and clicks below open D.
        assert_eq!(
            door.req_access,
            ["maint_tunnels"],
            "read from the map var edit"
        );
        assert!(!door.locked);
    }
    // The helper is gone, its airlock is bolted and shows the bolt lights.
    let world = s.server.world();
    let on_l = world.resource::<SpatialIndex>().on_tile(L);
    assert!(
        on_l.iter()
            .all(|&e| world.get::<DmAirlockHelper>(e).is_none())
    );
    assert!(s.server.world().get::<Door>(l).unwrap().locked);
    assert!(s.server.world().get::<Door>(b).unwrap().locked);
    let look = s.server.world().get::<Appearance>(l).unwrap();
    assert_eq!(look.icon_state, AIRLOCK_CLOSED);
    let overlays: Vec<_> = look.overlays.iter().map(|o| o.icon_state).collect();
    assert_eq!(overlays, [FILL_CLOSED, BOLT_LIGHTS]);
    // Doors that start open are open and passable; the firelock and shutter are left alone.
    for open_door in [p, f] {
        assert!(is_open(s.state(open_door)));
        assert!(!s.dense(open_door));
        assert!(s.server.world().get::<Opaque>(open_door).is_none());
    }
    assert!(s.dense(d) && s.server.world().get::<Opaque>(d).is_some());

    // 1. Bump: the closed airlock opens after tg's 0.5 s, the mob passes, it closes by itself.
    s.walk(Dir::East);
    let bumped_at = s.now();
    s.run_until("D starts opening", |s| {
        s.press(Dir::East);
        s.state(d) != DoorState::Closed
    });
    assert!(matches!(s.state(d), DoorState::Opening { .. }));
    s.run_until("D becomes passable", |s| !s.dense(d));
    let passable_after = s.now() - bumped_at;
    assert!(
        (0.5..1.0).contains(&passable_after),
        "passable after {passable_after:.2}s"
    );
    let d_id = s.net_id(d);
    s.run_until("the client sees D open", |s| {
        s.client_drew(d_id, AIRLOCK_OPEN)
    });
    s.walk(Dir::East);
    assert_eq!(s.pos(), D);
    s.walk(Dir::East);
    s.run_until("D closes behind the mob", |s| {
        s.state(d) == DoorState::Closed && s.dense(d)
    });
    let closed_after = s.now() - bumped_at;
    // normalspeed = 0: autoclose 1.5 s after opening started, then 0.6 s of closing.
    assert!(
        (2.0..3.5).contains(&closed_after),
        "closed after {closed_after:.2}s"
    );
    assert!(s.server.world().get::<Opaque>(d).is_some());

    // 2. A mob in the doorway keeps it open; it closes once the doorway is free.
    // The key stays held: the step that bumped it open walks in once the door is passable.
    s.walk_to(Dir::West, D);
    s.hold(3.0, "D stays open while occupied", |s| is_open(s.state(d)));
    let retry = s.server.world().get::<Door>(d).unwrap().close_at();
    assert!(retry.is_some(), "a blocked autoclose is retried");
    s.walk(Dir::East);
    s.run_until("D closes after the retry", |s| {
        s.state(d) == DoorState::Closed
    });

    // 3. Clicks: from two tiles away nothing happens, adjacent toggles.
    s.walk(Dir::East);
    assert_eq!(s.pos(), F, "the open firelock lets the mob through");
    s.click(d);
    s.hold(0.5, "a click from 2 tiles is rejected", |s| {
        s.state(d) == DoorState::Closed
    });
    s.walk(Dir::West);
    s.click(d);
    s.run_until("an adjacent click opens D", |s| is_open(s.state(d)));
    s.click(d);
    s.run_until("a second click closes D", |s| {
        s.state(d) == DoorState::Closed
    });

    // 4. A click on a door the client does not see is rejected, even next to it. Edge hysteresis
    // keeps entities near the viewer, so hide D by moving the view to a level that does not exist.
    let stale_id = s.net_id(d);
    let home = s.server.world().get::<Viewer>(s.link).unwrap().center;
    s.st.server
        .world_mut()
        .get_mut::<Viewer>(s.link)
        .unwrap()
        .center
        .z = u16::MAX;
    s.run_until("the client loses D", |s| !s.client_sees(d_id));
    s.send(ClientIntent::Click {
        target: stale_id,
        modifiers: ClickModifiers::default(),
    });
    // Shorter than the NetId reuse delay (200 server updates) at this loop's unthrottled frame
    // rate, or D would come back under its old id.
    s.hold(0.2, "a click on an unseen door is rejected", |s| {
        s.state(d) == DoorState::Closed
    });
    s.st.server
        .world_mut()
        .get_mut::<Viewer>(s.link)
        .unwrap()
        .center = home;
    // D was despawned on the client above, so seeing it again means a new spawn (its id may
    // or may not be the old one, depending on how long the client was away).
    s.run_until("the client sees D again", |s| {
        let id = s.server.world().resource::<Interest>().net_id(s.link, d);
        id.is_some_and(|id| s.client_sees(id))
    });

    // 5. The open shutter ignores hands; bolted airlocks ignore bumps and clicks.
    s.click(p);
    s.hold(0.5, "the shutter stays open", |s| {
        is_open(s.state(p)) && !s.dense(p)
    });
    s.walk(Dir::North);
    s.bump(Dir::West);
    s.click(l);
    s.hold(1.0, "the helper-bolted airlock stays shut", |s| {
        s.state(l) == DoorState::Closed && s.dense(l)
    });
    s.walk(Dir::North);
    s.bump(Dir::West);
    s.click(b);
    s.hold(1.0, "the var-edit-bolted airlock stays shut", |s| {
        s.state(b) == DoorState::Closed && s.dense(b)
    });
}

/// Walks out of the MetaStation spawn room: BFS over free tiles from the spawn (closed doors
/// block it), then through the nearest openable door to a tile the room does not reach.
fn metastation_map() -> MapLoadPlugin {
    MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    }
}

/// The access of the ID the player spawned with.
fn spawn_access(s: &Session) -> Vec<String> {
    let world = s.server.world();
    let id = world
        .get::<Inventory>(s.mob)
        .unwrap()
        .id
        .expect("spawned with an ID");
    world.get::<IdCard>(id).unwrap().access.clone()
}

#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation() {
    let mut s = Session::start(metastation_map());
    let report = s.server.world().resource::<DoorReport>();
    println!(
        "MetaStation doors: {:?}, {} open and {} sealed at start, {} helpers applied, {} orphan",
        report.by_kind, report.open, report.sealed, report.helpers, report.orphan_helpers
    );
    let start = s.pos();
    let (room, door, through) = plan_exit(&s, start);
    println!(
        "spawn {start:?}: room of {} tiles, leaving through {:?} {door:?}",
        room.len(),
        s.server.world().get::<Door>(s.door(door)).unwrap().kind
    );
    // Walk to the tile before the door along the BFS path.
    for dir in path(&s, start, through[0], &room) {
        s.walk(dir);
    }
    let into = dir_between(through[0], door);
    let out = dir_between(door, through[1]);
    let d = s.door(door);
    s.walk_to(into, door);
    assert!(!s.dense(d), "the door opened on the bump");
    s.walk(out);
    assert!(
        !room.contains(&s.pos()),
        "{:?} is still in the room",
        s.pos()
    );
    println!("left the spawn room at {:?}", s.pos());
}

/// The department of an access, after the sections of tg `code/__DEFINES/access.dm`.
fn department(access: &str) -> &'static str {
    match access {
        "maint_tunnels" => "maintenance",
        "command"
        | "ai_upload"
        | "teleporter"
        | "eva"
        | "rc_announce"
        | "keycard_auth"
        | "minisat"
        | "network"
        | "gateway"
        | "all_personal_lockers"
        | "change_ids"
        | "department_budget"
        | "captain"
        | "hop"
        | "blueshield"
        | "nanotrasen_representative"
        | "magistrate" => "command",
        "security" | "brig_entrance" | "brig" | "armory" | "court" | "weapons" | "hos"
        | "detective" => "security",
        "engineering" | "atmospherics" | "engine_equip" | "atmos_equip" | "construction"
        | "tech_storage" | "tcomms" | "aux_base" | "external airlocks" | "ce" => "engineering",
        "medical" | "morgue" | "morgue_secure" | "pharmacy" | "surgery" | "plumbing"
        | "virology" | "psychology" | "cmo" | "paramedic" => "medical",
        "cargo" | "shipping" | "mineral_storeroom" | "mining" | "mining_station" | "vault"
        | "qm" | "bit_den" => "supply",
        "science" | "research" | "ordnance" | "ordnance_storage" | "genetics" | "robotics"
        | "xenobiology" | "rd" => "science",
        "service" | "theatre" | "chapel_office" | "crematorium" | "library" | "bar" | "kitchen"
        | "hydroponics" | "janitor" | "lawyer" => "service",
        _ => "other",
    }
}

/// Teleports the mob next to `door` (a free cardinal neighbour that is not an unrestricted
/// side), clicks it and returns its state 0.3 s later and whether it showed the deny animation.
fn click_from_beside(s: &mut Session, door: Entity) -> (DoorState, bool) {
    let pos = *s.server.world().get::<TilePos>(door).unwrap();
    let beside = Dir::CARDINALS
        .into_iter()
        .map(|d| pos.step(d))
        .find(|&p| {
            free(s, p)
                && !s
                    .server
                    .world()
                    .get::<Door>(door)
                    .unwrap()
                    .unrestricted_from(pos, p)
        })
        .unwrap_or_else(|| panic!("no free tile beside {pos:?}"));
    s.st.server.world_mut().entity_mut(s.mob).insert(beside);
    s.run_until("the door is replicated", |s| {
        s.server
            .world()
            .resource::<Interest>()
            .net_id(s.link, door)
            .is_some()
    });
    s.click(door);
    let mut denied = false;
    for _ in 0..steps(0.3) {
        s.step();
        denied |= s.server.world().get::<Door>(door).unwrap().denied();
    }
    (s.state(door), denied)
}

#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_access() {
    let mut s = Session::start(metastation_map());
    let access = spawn_access(&s);
    let world = s.server.world_mut();
    let mut q = world.query::<(Entity, &Door)>();
    let mut by_department: std::collections::BTreeMap<&str, usize> = Default::default();
    let mut public = std::collections::BTreeMap::<DoorKind, usize>::new();
    let mut unres = 0;
    let mut pick: std::collections::HashMap<&str, Entity> = Default::default();
    for (entity, door) in q.iter(world) {
        unres += usize::from(door.unres_sides != 0);
        let Some(first) = door.req_access.first().or(door.req_one_access.first()) else {
            *public.entry(door.kind).or_default() += 1;
            continue;
        };
        let dept = if door.req_one_access.iter().any(|a| a == "maint_tunnels") {
            "maintenance"
        } else {
            department(first)
        };
        *by_department.entry(dept).or_default() += 1;
        let usable = door.kind == DoorKind::Airlock
            && door.state == DoorState::Closed
            && !door.locked
            && !door.welded
            && door.unres_sides == 0;
        let key = match dept {
            "command" | "security" if usable && !door.allows(Some(&access)) => dept,
            "maintenance" if usable && door.allows(Some(&access)) => dept,
            _ => continue,
        };
        // The lowest entity of each, so reruns pick the same door.
        let slot = pick.entry(key).or_insert(entity);
        *slot = (*slot).min(entity);
    }
    let with_access: usize = by_department.values().sum();
    println!("assistant access: {access:?}");
    println!(
        "MetaStation doors with access by department: {by_department:?} ({with_access} total)"
    );
    println!("public doors by kind: {public:?}; airlocks with unrestricted sides: {unres}");
    assert!(with_access > 300, "{with_access} doors with access");
    for dept in [
        "command",
        "security",
        "engineering",
        "medical",
        "science",
        "supply",
        "service",
    ] {
        assert!(
            by_department.get(dept).copied().unwrap_or(0) >= 5,
            "few {dept} doors"
        );
    }
    assert!(by_department["maintenance"] >= 50);

    for dept in ["command", "security"] {
        let door = pick[dept];
        let (state, denied) = click_from_beside(&mut s, door);
        let d = s.server.world().get::<Door>(door).unwrap();
        println!(
            "{dept} door {:?} {:?}: {state:?}, denied {denied}",
            d.req_access, d.req_one_access
        );
        assert_eq!(
            state,
            DoorState::Closed,
            "the assistant opened a {dept} door"
        );
        assert!(denied && s.dense(door));
    }
    let door = pick["maintenance"];
    let (state, denied) = click_from_beside(&mut s, door);
    println!("maintenance door: {state:?}, denied {denied}");
    assert!(
        matches!(state, DoorState::Opening { .. } | DoorState::Open) && !denied,
        "the assistant could not open maintenance"
    );
}

fn free(s: &Session, pos: TilePos) -> bool {
    let index = s.server.world().resource::<SpatialIndex>();
    index.in_bounds(pos) && index.dense_on_tile(pos).iter().all(|&e| e == s.mob)
}

/// The free tiles reachable from `start`, the nearest door on the room's edge that a hand opens,
/// and the tiles before and after it.
fn plan_exit(s: &Session, start: TilePos) -> (HashSet<TilePos>, TilePos, [TilePos; 2]) {
    let world = s.server.world();
    let access = spawn_access(s);
    let mut room = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    let mut exit = None;
    while let Some(at) = queue.pop_front() {
        for dir in Dir::CARDINALS {
            let next = at.step(dir);
            if room.contains(&next) {
                continue;
            }
            if free(s, next) {
                room.insert(next);
                queue.push_back(next);
                continue;
            }
            let beyond = next.step(dir);
            let openable = world
                .resource::<SpatialIndex>()
                .on_tile(next)
                .iter()
                .filter_map(|&e| world.get::<Door>(e))
                .any(|d| {
                    d.state == DoorState::Closed
                        && !d.locked
                        && !d.welded
                        && (d.allows(Some(&access)) || d.unrestricted_from(next, at))
                        && matches!(
                            d.kind,
                            DoorKind::Airlock | DoorKind::Basic | DoorKind::Windoor
                        )
                });
            let only_door = world
                .resource::<SpatialIndex>()
                .dense_on_tile(next)
                .iter()
                .all(|&e| world.get::<Door>(e).is_some());
            if exit.is_none() && openable && only_door && free(s, beyond) {
                exit = Some((next, [at, beyond]));
            }
        }
    }
    let (door, through) = exit.expect("no openable door on the spawn room's edge");
    assert!(!room.contains(&through[1]), "the far side is the same room");
    (room, door, through)
}

fn path(s: &Session, from: TilePos, to: TilePos, room: &HashSet<TilePos>) -> Vec<Dir> {
    let mut prev = std::collections::HashMap::from([(from, None)]);
    let mut queue = VecDeque::from([from]);
    while let Some(at) = queue.pop_front() {
        if at == to {
            break;
        }
        for dir in Dir::CARDINALS {
            let next = at.step(dir);
            if room.contains(&next) && free(s, next) && !prev.contains_key(&next) {
                prev.insert(next, Some((at, dir)));
                queue.push_back(next);
            }
        }
    }
    let mut dirs = Vec::new();
    let mut at = to;
    while let Some(Some((back, dir))) = prev.get(&at) {
        dirs.push(*dir);
        at = *back;
    }
    dirs.reverse();
    dirs
}

fn dir_between(a: TilePos, b: TilePos) -> Dir {
    Dir::CARDINALS
        .into_iter()
        .find(|&d| a.step(d) == b)
        .expect("adjacent")
}

/// An unsafe airlock (`safe = 0`) closes on the mob in its doorway and crushes it (tg
/// `crush()`: 20 brute to the chest, 10 s paralysis for a human); a bare right click on a
/// closed door knocks (BandaStation `knock_on`: the click cooldown, the door stays shut).
#[test]
fn unsafe_airlock_crushes_and_a_right_click_knocks() {
    use ss13_game::combat::NextMove;
    use ss13_game::health::Health;
    use ss13_game::status_effects::{Incap, StatusEffects};
    use ss13_game::zone::BodyZone;
    let assets = fixture_assets("crush");
    let mut s = Session::start(MapLoadPlugin {
        source: assets.clone(),
        maps: vec!["map.dmm".into()],
        assets,
    });
    let (d, b) = (s.door(D), s.door(B));
    s.server.world_mut().get_mut::<Door>(d).unwrap().safe = false;
    s.walk(Dir::East);
    s.walk_to(Dir::East, D);
    let chest = |s: &Session| {
        s.server
            .world()
            .get::<Health>(s.mob)
            .unwrap()
            .part(BodyZone::Chest)
            .brute
    };
    assert_eq!(chest(&s), 0.0);
    s.run_until("D closes on the mob", |s| s.state(d) == DoorState::Closed);
    assert!(chest(&s) >= 20.0, "crushed: {} brute", chest(&s));
    let paralyzed = s
        .server
        .world()
        .get::<StatusEffects>(s.mob)
        .map_or(0.0, |e| e.remaining(Incap::Paralyze));
    assert!(paralyzed > 5.0, "paralyzed for {paralyzed}");
    assert_eq!(s.pos(), D, "the crushed mob stays in the doorway");

    // Knock on the bolted B from beside it, once the paralysis is over.
    s.run_until("the paralysis wears off", |s| {
        s.server
            .world()
            .get::<StatusEffects>(s.mob)
            .is_none_or(|e| e.remaining(Incap::Paralyze) <= 0.0)
    });
    s.server.world_mut().get_mut::<Door>(d).unwrap().safe = true;
    s.walk(Dir::West);
    s.walk(Dir::North);
    s.walk(Dir::North);
    let target = s.net_id(b);
    let before = s.now();
    s.send(ClientIntent::Click {
        target,
        modifiers: ClickModifiers {
            right: true,
            ..default()
        },
    });
    s.run_until("the knock sets the click cooldown", |s| {
        s.server
            .world()
            .get::<NextMove>(s.mob)
            .is_some_and(|n| n.0 > before)
    });
    assert_eq!(s.state(b), DoorState::Closed);
}
