//! Grabs, carries and thrown mobs on a fixture station (`ss13_game::pull`, `pull::carry`,
//! `throw`): a human's aggressive grab upgrades to a neck grab and a kill grab after 3 s each
//! (floored, hands blocked, choked, the puller slowed, the victim drawn shifted towards it); a
//! lying human dragged onto its grabber is carried on the shoulders and follows its carrier until
//! the carrier falls; a held human climbs onto its grabber's back; a grabbed human thrown into a
//! wall is paralyzed and hurt.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::health::{BodyPosition, Health};
use ss13_game::medical::surgery::Buckled;
use ss13_game::pull::carry::{Carried, HUMAN_CARRY_SLOWDOWN, MOB_BELOW_PIGGYBACK_LAYER};
use ss13_game::pull::{GrabState, HeldOffset, PullSlowdown, Pulling, choked};
use ss13_game::status_effects::{Incap, Incapacitation, StatusEffects};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::Turf;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd};
use ss13_shared::{Dir, Facing, TilePos};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14 } },
    "/datum/storage/backpack": { "parent": "/datum/storage", "declared_vars": {
        "max_slots": 21, "max_total_storage": 21 } },
    "/datum/storage/box": { "parent": "/datum/storage", "declared_vars": {
        "max_specific_storage": 2 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "req_access": null } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3, "breakouttime": 0,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": { "w_class": 1 } },
    "/obj/item/pen": { "parent": "/obj/item", "declared_vars": {
        "name": "pen", "icon_state": "pen", "w_class": 1 } },
    "/obj/item/restraints": { "parent": "/obj/item", "declared_vars": { "breakouttime": 600 } },
    "/obj/item/restraints/handcuffs": { "parent": "/obj/item/restraints", "declared_vars": {
        "name": "handcuffs", "icon_state": "handcuff", "breakouttime": 600, "handcuff_time": 40,
        "used": 0 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/backpack": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "backpack", "icon_state": "backpack", "slot_flags": 1, "w_class": 4,
        "storage_type": { "path": "/datum/storage/backpack" } } },
    "/obj/item/storage/box": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "box", "icon_state": "box", "w_class": 2,
        "storage_type": { "path": "/datum/storage/box" } } },
    "/obj/item/storage/box/survival": { "parent": "/obj/item/storage/box" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "name": "closet", "icon": { "resource": "c.dmi" }, "icon_state": "generic", "anchored": 0,
        "density": 1, "opened": 0, "locked": 0, "welded": 0, "secure": 0, "icon_door": null,
        "icon_welded": "welded", "enable_door_overlay": 1, "has_opened_overlay": 1,
        "has_closed_overlay": 1, "dense_when_open": 0, "wall_mounted": 0, "horizontal": 0,
        "storage_capacity": 30 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": {
        "name": "floor", "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  },
  "populate": {
    "/obj/structure/closet": { "PopulateContents": ["return"] },
    "/obj/item/storage/box/survival": { "PopulateContents": [
        { "repeat": { "count": 2, "body": [{ "new": "/obj/item/toy" }] } }] }
  }
}"##;

/// Rows top-down, the last line is y = 1. Two spawns at (2,3) and (3,3); the tests place A at
/// (3,3) and B east of it at (4,3); the east wall is x = 10.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wffffffffw
wssffffffw
wffffffffw
wwwwwwwwww
"}
"##;

const A_AT: TilePos = TilePos::new(3, 3, 1);
const B_AT: TilePos = TilePos::new(4, 3, 1);
const EAST_WALL: TilePos = TilePos::new(10, 3, 1);

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("c.dmi", "generic"),
    ("c.dmi", "generic_door"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("grabs_fixture");
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    used.extend(ss13_mapload::RUNTIME_CONTAINER_TYPES);
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    /// Two joined humans, A at [`A_AT`] and B at [`B_AT`], without mood (exact `do_after`s).
    fn start() -> (Self, Entity, Entity) {
        let mut s = Station(testing::Station::start(
            fixture_map(&fixture_assets(), &["map.dmm"]),
            |_| {},
        ));
        s.join_all(2, |_| {});
        s.0.without_mood();
        let (a, b) = (s.players[0].mob, s.players[1].mob);
        let world = s.server.world_mut();
        world.entity_mut(a).insert(A_AT);
        world.entity_mut(b).insert(B_AT);
        s.run_for(0.5);
        (s, a, b)
    }

    fn pos(&self, e: Entity) -> TilePos {
        *self.get::<TilePos>(e)
    }

    /// A grabs B aggressively (as two ctrl+clicks would), off its click cooldown.
    fn grab(&mut self, a: Entity, b: Entity) {
        self.server.world_mut().entity_mut(a).insert((
            Pulling(b),
            GrabState::Aggressive,
            PullSlowdown(0.0),
        ));
        self.run_for(1.2);
    }

    fn grab_state(&self, a: Entity) -> Option<GrabState> {
        self.try_get::<GrabState>(a).copied()
    }

    fn ctrl_click(&mut self, p: usize, e: Entity) {
        let ctrl = ClickModifiers {
            ctrl: true,
            ..default()
        };
        self.click_with(p, e, ctrl);
    }

    /// `p` drags `from` onto `to`.
    fn drag(&mut self, p: usize, from: Entity, to: Entity) {
        let (from, to) = (self.net_id(p, from).unwrap(), self.net_id(p, to).unwrap());
        self.send(
            p,
            ClientIntent::Drag {
                from: DragEnd::Atom(from),
                to: DragEnd::Atom(to),
                modifiers: default(),
            },
        );
    }

    fn traits(&self, e: Entity) -> Incapacitation {
        *self.get::<Incapacitation>(e)
    }

    fn paralyzed(&self, e: Entity) -> bool {
        self.try_get::<StatusEffects>(e)
            .is_some_and(|f| f.remaining(Incap::Paralyze) > 0.0)
    }

    fn lying(&self, e: Entity) -> bool {
        self.get::<BodyPosition>(e).lying()
    }
}

#[test]
fn an_aggressive_grab_upgrades_to_a_neck_grab_and_a_kill_grab() {
    let (mut s, a, b) = Station::start();
    s.grab(a, b);
    assert_eq!(s.grab_state(a), Some(GrabState::Aggressive));
    s.run_until("the aggressive grab holds B", |s| {
        let t = s.traits(b);
        t.immobilized && t.hands_blocked && !t.floored
    });
    // Drawn 12 px towards A (west), A slowed by 3 ds.
    s.run_until("B is shifted", |s| {
        s.try_get::<HeldOffset>(b).map(|o| o.pixel) == Some([-12, 0])
    });
    assert_eq!(s.get::<PullSlowdown>(a).0, 3.0);

    // Ctrl+click again: 3 s of `do_after`, then the neck grab.
    s.ctrl_click(0, b);
    s.hold(2.5, "the upgrade takes 3 s", |s| {
        s.grab_state(a) == Some(GrabState::Aggressive)
    });
    s.run_until_for(1.5, "a neck grab", |s| {
        s.grab_state(a) == Some(GrabState::Neck)
    });
    s.run_until("B is floored", |s| s.traits(b).floored && s.lying(b));
    s.run_until("B is shifted 16 px", |s| {
        s.try_get::<HeldOffset>(b).map(|o| o.pixel) == Some([-16, 0])
    });
    assert_eq!(s.get::<PullSlowdown>(a).0, 6.0);
    assert!(!choked(s.server.world(), b), "a neck grab does not choke");

    // Once more: the kill grab chokes.
    s.run_for(1.2);
    s.ctrl_click(0, b);
    s.run_until_for(4.0, "a kill grab", |s| {
        s.grab_state(a) == Some(GrabState::Kill)
    });
    assert!(choked(s.server.world(), b));
    s.run_for(0.2);
    assert_eq!(s.get::<PullSlowdown>(a).0, 9.0);

    // Nothing past the kill grab; letting go frees B.
    s.run_for(1.2);
    s.ctrl_click(0, b);
    s.hold(3.5, "no upgrade past kill", |s| {
        s.grab_state(a) == Some(GrabState::Kill)
    });
    s.server.world_mut().entity_mut(a).remove::<Pulling>();
    s.run_until("B is free", |s| {
        let t = s.traits(b);
        !t.immobilized && !t.hands_blocked
    });
    s.run_until("B is drawn in place again", |s| {
        s.try_get::<HeldOffset>(b)
            .is_none_or(|o| *o == HeldOffset::default())
    });
    assert!(!choked(s.server.world(), b));
}

#[test]
fn a_lying_human_is_carried_on_the_shoulders_until_the_carrier_falls() {
    let (mut s, a, b) = Station::start();
    ss13_game::status_effects::apply(s.server.world_mut(), b, Incap::Paralyze, 60.0);
    s.run_until("B lies down", |s| s.lying(b));
    // Not grabbed: dragging B onto A does not carry.
    s.drag(0, b, a);
    s.run_for(6.0);
    assert!(s.try_get::<Carried>(b).is_none());

    s.grab(a, b);
    s.drag(0, b, a);
    s.hold(4.5, "lifting takes 5 s", |s| {
        s.try_get::<Carried>(b).is_none()
    });
    s.run_until_for(1.5, "B is on A's shoulders", |s| {
        s.try_get::<Carried>(b).is_some()
    });
    assert_eq!(
        *s.get::<Carried>(b),
        Carried {
            by: a,
            fireman: true
        }
    );
    assert_eq!(s.get::<Buckled>(b).0, a);
    assert!(s.try_get::<Pulling>(a).is_none(), "the grab ends");
    assert_eq!(s.pos(b), A_AT);
    s.run_until("A is slowed by the carry", |s| {
        s.get::<PullSlowdown>(a).0 == HUMAN_CARRY_SLOWDOWN
    });
    s.run_until("B lies on A's shoulders", |s| {
        s.try_get::<HeldOffset>(b)
            .is_some_and(|o| o.pixel == [0, 6] && o.layer.is_some())
            && s.get::<BodyPosition>(b).lying_angle == 90
    });

    // A walks east: B comes along and faces the same way.
    let seq = s.players[0].seq + 1;
    s.players[0].seq = seq;
    s.send(
        0,
        ClientIntent::Move {
            dir: Dir::East,
            seq,
        },
    );
    s.run_until("A stepped east", |s| s.pos(a) == TilePos::new(4, 3, 1));
    s.run_until("B follows", |s| s.pos(b) == TilePos::new(4, 3, 1));
    s.run_until("B faces east", |s| {
        s.try_get::<Facing>(b) == Some(&Facing(Dir::East))
    });

    // A falls over: B drops off, both down.
    ss13_game::status_effects::apply(s.server.world_mut(), a, Incap::Paralyze, 5.0);
    s.run_until("B falls off", |s| s.try_get::<Carried>(b).is_none());
    assert!(s.try_get::<Buckled>(b).is_none());
    s.run_until("A is no longer slowed", |s| {
        s.get::<PullSlowdown>(a).0 == 0.0
    });
}

#[test]
fn a_held_human_climbs_onto_its_grabbers_back() {
    let (mut s, a, b) = Station::start();
    s.grab(a, b);
    // B drags itself onto A: 1.5 s, then it rides piggyback.
    s.drag(1, b, a);
    s.hold(1.2, "climbing takes 1.5 s", |s| {
        s.try_get::<Carried>(b).is_none()
    });
    s.run_until_for(1.0, "B rides on A's back", |s| {
        s.try_get::<Carried>(b).is_some()
    });
    assert!(!s.get::<Carried>(b).fireman);
    assert_eq!(s.pos(b), A_AT);
    assert!(!s.lying(b), "a piggyback rider sits up");
    // A faces east (it grabbed B there): B sits 6 px behind, 8 px up, below A.
    s.server.world_mut().entity_mut(a).insert(Facing(Dir::East));
    s.run_until("B is drawn on A's back", |s| {
        s.try_get::<HeldOffset>(b).copied()
            == Some(HeldOffset {
                pixel: [-6, 8],
                layer: Some(MOB_BELOW_PIGGYBACK_LAYER),
            })
    });
    // B cannot walk off; resisting gets it down.
    assert!(s.traits(b).immobilized);
    s.send(1, ClientIntent::Resist);
    s.run_until("B got down", |s| s.try_get::<Carried>(b).is_none());
    assert!(s.try_get::<Buckled>(b).is_none());
    s.run_until("B can move again", |s| !s.traits(b).immobilized);
}

#[test]
fn a_grabbed_human_thrown_into_a_wall_is_paralyzed_and_hurt() {
    let (mut s, a, b) = Station::start();
    s.grab(a, b);
    let wall = {
        let world = s.server.world_mut();
        world
            .query_filtered::<(Entity, &TilePos), With<Turf>>()
            .iter(world)
            .find(|(_, p)| **p == EAST_WALL)
            .map(|(e, _)| e)
            .unwrap()
    };
    let before = s.get::<Health>(b).health();
    s.send(0, ClientIntent::ToggleThrowMode);
    s.run_for(0.2);
    s.click(0, wall);
    s.run_until("B is let go", |s| s.try_get::<Pulling>(a).is_none());
    s.run_until("B crashed into the wall", |s| s.paralyzed(b));
    assert_eq!(s.pos(b), TilePos::new(9, 3, 1), "B stops at the wall");
    let lost = before - s.get::<Health>(b).health();
    assert!((lost - 10.0).abs() < 0.01, "B took 10 brute, lost {lost}");
    assert!(!s.paralyzed(a), "the thrower is fine");
}
