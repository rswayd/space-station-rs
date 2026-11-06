//! Grooming over real WebTransport on 127.0.0.1: a razor shaves a beard and a head and restyles
//! someone's hair; lipstick goes on and comes off with paper and washing; the dye spray colors a
//! beard; a mirror changes a beard and a frame hangs on a wall; a magic mirror changes the eyes,
//! the name and the species. Prompt answers are written straight to the server (the questions
//! themselves are `ss13_tgui::prompt` modals).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::CombatMode;
use ss13_game::decor::Mirror;
use ss13_game::health::ZoneSelected;
use ss13_game::human::{FACIAL_HAIR_STYLES, HAIR_STYLES, HumanLook, SKINHEAD};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::service::barber::{BarberAsk, Lipstick};
use ss13_game::species::SpeciesId;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::zone::TargetZone;
use ss13_mapload::{DmObj, DmTurfBuild};
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::prompt::{PromptAnswer, PromptAnswered};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2, "max_integrity": 100 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/mirror": { "parent": "/obj/structure", "declared_vars": {
        "name": "mirror", "icon": { "resource": "w.dmi" }, "icon_state": "mirror",
        "max_integrity": 200 } },
    "/obj/structure/mirror/magic": { "parent": "/obj/structure/mirror", "declared_vars": {
        "name": "magic mirror", "icon_state": "magic_mirror" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/razor": { "parent": "/obj/item", "declared_vars": {
        "name": "electric razor", "icon": { "resource": "c.dmi" }, "icon_state": "razor" } },
    "/obj/item/lipstick": { "parent": "/obj/item", "declared_vars": {
        "name": "red lipstick", "icon": { "resource": "c.dmi" }, "icon_state": "lipstick" } },
    "/obj/item/lipstick/purple": { "parent": "/obj/item/lipstick", "declared_vars": {
        "name": "purple lipstick" } },
    "/obj/item/dyespray": { "parent": "/obj/item", "declared_vars": {
        "name": "hair dye spray", "icon": { "resource": "c.dmi" }, "icon_state": "dyespray" } },
    "/obj/item/paper": { "parent": "/obj/item", "declared_vars": { "name": "paper" } },
    "/obj/item/wallframe": { "parent": "/obj/item" },
    "/obj/item/wallframe/mirror": { "parent": "/obj/item/wallframe", "declared_vars": {
        "name": "mirror", "icon": { "resource": "w.dmi" }, "icon_state": "mirror" } },
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

/// Rows top-down, the last line is y = 1. Spawns `s` at (3,2) and (4,2); a mirror `M` (3,3)
/// and a magic mirror `G` (4,3) north of them.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"M" = (/obj/structure/mirror,/turf/open/floor,/area/space)
"G" = (/obj/structure/mirror/magic,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wfMGfw
wfssfw
wwwwww
"}
"##;

const MIRROR: TilePos = TilePos::new(3, 3, 1);
const MAGIC: TilePos = TilePos::new(4, 3, 1);

const STATES: [(&str, &str); 13] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("c.dmi", "razor"),
    ("c.dmi", "lipstick"),
    ("c.dmi", "lipstick_uncap"),
    ("c.dmi", "lipstick_uncap_color"),
    ("c.dmi", "dyespray"),
    ("w.dmi", "mirror"),
    ("w.dmi", "mirror_broke"),
    ("w.dmi", "magic_mirror"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("barber_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/mirror",
        "/obj/structure/mirror/magic",
        "/obj/item/razor",
        "/obj/item/lipstick",
        "/obj/item/lipstick/purple",
        "/obj/item/dyespray",
        "/obj/item/paper",
        "/obj/item/wallframe/mirror",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(players: usize) -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(players, |_| {});
        Self(s)
    }

    fn mob(&self, p: usize) -> Entity {
        self.players[p].mob
    }

    fn look(&self, p: usize) -> HumanLook {
        self.get::<HumanLook>(self.mob(p)).clone()
    }

    fn set_look(&mut self, p: usize, change: impl FnOnce(&mut HumanLook)) {
        let mob = self.mob(p);
        let mut look = self.server.world_mut().get_mut::<HumanLook>(mob).unwrap();
        change(&mut look);
    }

    fn obj_on(&self, pos: TilePos, path: &str) -> Option<Entity> {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied())
        .find(|&e| world.get::<DmObj>(e).is_some_and(|o| o.path == path))
    }

    fn turf_at(&mut self, pos: TilePos) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &TilePos, &DmTurfBuild)>();
        q.iter(world)
            .find(|(_, p, _)| **p == pos)
            .map(|(e, ..)| e)
            .unwrap()
    }

    /// A new `path` item straight into `p`'s active hand, the old one gone.
    fn give_new(&mut self, p: usize, path: &str) -> Entity {
        let mob = self.mob(p);
        let world = self.server.world_mut();
        let old = world.get::<Inventory>(mob).and_then(|i| i.hands[i.active]);
        if let Some(old) = old {
            world.despawn(old);
        }
        let item = ss13_game::construction::spawn_obj(world, path, TilePos::new(5, 2, 1)).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
        item
    }

    fn empty_hand(&mut self, p: usize) {
        let mob = self.mob(p);
        let world = self.server.world_mut();
        let old = world.get::<Inventory>(mob).and_then(|i| i.hands[i.active]);
        if let Some(old) = old {
            world.despawn(old);
        }
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = None;
    }

    fn aim(&mut self, p: usize, zone: TargetZone, combat: bool) {
        let mob = self.mob(p);
        self.server
            .world_mut()
            .entity_mut(mob)
            .insert((ZoneSelected(zone), CombatMode(combat)));
    }

    /// The grooming question waiting for `p`, if any.
    fn asked(&mut self, p: usize) -> Option<(Entity, BarberAsk)> {
        let mob = self.mob(p);
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &BarberAsk, &ss13_tgui::prompt::Prompt)>();
        q.iter(world)
            .find(|(_, _, prompt)| prompt.actor == mob)
            .map(|(e, ask, _)| (e, ask.clone()))
    }

    /// Answers `p`'s grooming question as its player would, then closes the prompt.
    fn answer(&mut self, p: usize, answer: PromptAnswer) -> BarberAsk {
        self.run_until("a grooming question", |s| s.asked(p).is_some());
        let (prompt, ask) = self.asked(p).unwrap();
        let (client, actor) = (self.players[p].link, self.mob(p));
        self.server.world_mut().write_message(PromptAnswered {
            prompt,
            client,
            actor,
            answer: Some(answer),
        });
        self.tick();
        if let Ok(e) = self.server.world_mut().get_entity_mut(prompt) {
            e.despawn();
        }
        ask
    }
}

#[test]
fn razor_shaves_and_restyles() {
    let mut s = Station::start(2);
    s.set_look(1, |l| {
        l.female = false;
        l.facial_hair_style = Some(FACIAL_HAIR_STYLES[3].into());
        l.hair_style = Some(HAIR_STYLES[0].into());
    });
    let victim = s.mob(1);
    s.give_new(0, "/obj/item/razor");
    s.run_for(1.0);

    // Combat mode on the mouth: the beard comes off.
    s.aim(0, TargetZone::Mouth, true);
    s.click(0, victim);
    s.run_until("shaved", |s| s.look(1).facial_hair_style.is_none());
    assert_eq!(s.look(1).hair_style.as_deref(), Some(HAIR_STYLES[0]));

    // On the head: a skinhead.
    s.aim(0, TargetZone::Head, true);
    s.run_for(1.0);
    s.click(0, victim);
    s.run_until("head shaved", |s| {
        s.look(1).hair_style.as_deref() == Some(SKINHEAD)
    });

    // Out of combat mode a barber picks a new style: entry 2 is the second hairstyle.
    s.aim(0, TargetZone::Head, false);
    s.run_for(1.0);
    s.click(0, victim);
    let ask = s.answer(0, PromptAnswer::Item(2));
    assert!(matches!(ask, BarberAsk::RazorStyle { facial: false, .. }));
    s.run_until("restyled", |s| {
        s.look(1).hair_style.as_deref() == Some(HAIR_STYLES[1])
    });

    // Styling your own hair needs a mirror.
    let me = s.mob(0);
    s.run_for(1.0);
    s.click(0, me);
    s.run_for(0.5);
    assert!(s.asked(0).is_none());
}

#[test]
fn lipstick_goes_on_and_comes_off() {
    let mut s = Station::start(2);
    let (me, other) = (s.mob(0), s.mob(1));
    let lipstick = s.give_new(0, "/obj/item/lipstick/purple");
    s.run_for(1.0);
    assert_eq!(s.get::<Lipstick>(lipstick).color, [0x80, 0, 0x80]);

    // Closed, it does nothing.
    s.click(0, me);
    s.run_for(0.5);
    assert!(s.look(0).lip_style.is_none());

    let link = s.players[0].link;
    ss13_game::service::barber::attack_self(s.server.world_mut(), link, me, lipstick);
    assert!(s.get::<Lipstick>(lipstick).open);
    s.click(0, me);
    s.run_until("painted", |s| s.look(0).lip_style.is_some());
    let look = s.look(0);
    assert_eq!(look.lip_style.as_deref(), Some("lipstick"));
    assert_eq!(look.lip_color, [0x80, 0, 0x80]);

    // Paper on the mouth wipes it off.
    s.give_new(0, "/obj/item/paper");
    s.aim(0, TargetZone::Mouth, false);
    s.run_for(1.0);
    s.click(0, me);
    s.run_until("wiped", |s| s.look(0).lip_style.is_none());

    // On someone else it takes 2 s; washing takes it off.
    let lipstick = s.give_new(0, "/obj/item/lipstick");
    ss13_game::service::barber::attack_self(s.server.world_mut(), link, me, lipstick);
    s.run_for(1.0);
    s.click(0, other);
    s.run_for(0.5);
    assert!(s.look(1).lip_style.is_none(), "still applying");
    s.run_until("painted someone", |s| s.look(1).lip_style.is_some());
    assert_eq!(s.look(1).lip_color, [0xff, 0, 0]);
    assert!(ss13_game::janitor::wash(
        s.server.world_mut(),
        other,
        u32::MAX
    ));
    assert!(s.look(1).lip_style.is_none());
}

#[test]
fn dye_spray_colors_a_beard() {
    let mut s = Station::start(1);
    s.set_look(0, |l| {
        l.female = false;
        l.facial_hair_style = Some(FACIAL_HAIR_STYLES[0].into());
    });
    let me = s.mob(0);
    let spray = s.give_new(0, "/obj/item/dyespray");
    s.run_for(1.0);
    let link = s.players[0].link;
    assert!(ss13_game::service::barber::attack_self(
        s.server.world_mut(),
        link,
        me,
        spray
    ));
    // "Волосы", "Борода": the beard.
    let ask = s.answer(0, PromptAnswer::Button(1));
    assert!(matches!(ask, BarberAsk::DyeWhat { .. }));
    let ask = s.answer(0, PromptAnswer::Color("#112233".into()));
    assert!(matches!(ask, BarberAsk::DyeColor { facial: true, .. }));
    s.run_until("dyed", |s| {
        s.look(0).facial_hair_color == [0x11, 0x22, 0x33]
    });
}

#[test]
fn mirror_changes_the_beard_and_a_frame_hangs() {
    let mut s = Station::start(1);
    s.set_look(0, |l| l.female = false);
    let mirror = s.obj_on(MIRROR, "/obj/structure/mirror").unwrap();
    s.empty_hand(0);
    s.run_for(1.0);
    s.click(0, mirror);
    // "Сменить причёску", "Сменить бороду".
    let ask = s.answer(0, PromptAnswer::Item(1));
    assert!(matches!(ask, BarberAsk::Mirror { .. }));
    s.answer(0, PromptAnswer::Item(3));
    assert_eq!(
        s.look(0).facial_hair_style.as_deref(),
        Some(FACIAL_HAIR_STYLES[2])
    );
    // The menu comes back; the hair list then: entry 0 is bald.
    let ask = s.answer(0, PromptAnswer::Item(0));
    assert!(matches!(ask, BarberAsk::Mirror { .. }));
    s.answer(0, PromptAnswer::Item(0));
    assert!(s.look(0).hair_style.is_none());

    // A broken mirror offers nothing.
    s.server
        .world_mut()
        .entity_mut(mirror)
        .insert(Mirror { broken: true });
    while let Some((prompt, _)) = s.asked(0) {
        s.server.world_mut().despawn(prompt);
    }
    s.run_for(1.0);
    s.click(0, mirror);
    s.run_for(0.5);
    assert!(s.asked(0).is_none());

    // A frame on the south wall: a mirror on the user's tile, shifted onto that wall.
    let at = *s.get::<TilePos>(s.mob(0));
    let frame = s.give_new(0, "/obj/item/wallframe/mirror");
    let wall = s.turf_at(TilePos::new(at.x, at.y - 1, 1));
    s.run_for(1.0);
    s.click(0, wall);
    s.run_until("hung", |s| s.obj_on(at, "/obj/structure/mirror").is_some());
    let hung = s.obj_on(at, "/obj/structure/mirror").unwrap();
    assert_eq!(s.get::<Appearance>(hung).pixel_offset, [0, -28]);
    assert!(s.try_get::<Mirror>(hung).is_some_and(|m| !m.broken));
    assert!(s.server.world().get_entity(frame).is_err());
}

#[test]
fn magic_mirror_changes_eyes_name_and_species() {
    let mut s = Station::start(1);
    let magic = s.obj_on(MAGIC, "/obj/structure/mirror/magic").unwrap();
    s.empty_hand(0);
    s.run_for(1.0);
    s.click(0, magic);
    // Hair, beard, race, sex, eyes, name.
    s.answer(0, PromptAnswer::Item(4));
    s.answer(0, PromptAnswer::Color("#00ff00".into()));
    assert_eq!(s.look(0).eye_color, [0, 0xff, 0]);

    s.answer(0, PromptAnswer::Item(5));
    s.answer(0, PromptAnswer::Text("  Мерлин  ".into()));
    assert_eq!(s.get::<Name>(s.mob(0)).as_str(), "Мерлин");

    // The hair's color (a magic mirror asks "style or color" first).
    s.answer(0, PromptAnswer::Item(0));
    s.answer(0, PromptAnswer::Button(1));
    s.answer(0, PromptAnswer::Color("#abcdef".into()));
    assert_eq!(s.look(0).hair_color, [0xab, 0xcd, 0xef]);
    if !s.look(0).female {
        s.answer(0, PromptAnswer::Color("#010203".into()));
        assert_eq!(s.look(0).facial_hair_color, [1, 2, 3]);
    }

    // Another roundstart species, confirmed.
    let species: Vec<SpeciesId> = SpeciesId::roundstart().collect();
    let other = species
        .iter()
        .position(|&sp| sp != s.look(0).species)
        .expect("more than one roundstart species");
    s.answer(0, PromptAnswer::Item(2));
    s.answer(0, PromptAnswer::Item(other));
    let ask = s.answer(0, PromptAnswer::Button(0));
    assert!(matches!(ask, BarberAsk::MirrorStep { .. }));
    s.run_for(0.2);
    assert_eq!(s.look(0).species, species[other]);
}
