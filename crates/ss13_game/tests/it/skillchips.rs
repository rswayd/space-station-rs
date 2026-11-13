//! Skillchips over real WebTransport on 127.0.0.1: an engineer chip goes into the Skillsoft
//! station, is implanted into the occupant's brain after 15 s, activated from the window (its
//! trait, the BandaStation RCD knowledge, follows), refused a second job chip, and extracted
//! back into the occupant's hands once off cooldown; a brain leaving its body deactivates its
//! chips.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::closets::InCloset;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::medical::organs;
use ss13_game::skill_station::{self, SkillStation};
use ss13_game::skillchips::{self, ADVANCED_RCD_KNOWLEDGE, ChipIn, Skillchip};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::skill_station::{SkillStationData, SkillStationUi};
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
        "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "use_power": 1,
        "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "density": 1, "anchored": 1 } },
    "/obj/machinery/skill_station": { "parent": "/obj/machinery", "vars": {
        "name": "Skillsoft station" }, "declared_vars": { "name": "Skillsoft station",
        "icon": { "resource": "icons/obj/machines/implant_chair.dmi" },
        "icon_state": "implantchair", "density": 0 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "tool_behaviour": null,
        "toolspeed": 1 } },
    "/obj/item/skillchip": { "parent": "/obj/item", "declared_vars": {
        "name": "skillchip", "icon_state": "skillchip" } },
    "/obj/item/skillchip/job": { "parent": "/obj/item/skillchip" },
    "/obj/item/skillchip/job/engineer": { "parent": "/obj/item/skillchip/job",
        "vars": { "name": "Engineering C1-RCU-1T skillchip" },
        "declared_vars": { "name": "Engineering C1-RCU-1T skillchip" } },
    "/obj/item/skillchip/job/clown": { "parent": "/obj/item/skillchip/job",
        "vars": { "name": "B@L00NY skillchip" },
        "declared_vars": { "name": "B@L00NY skillchip" } },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0 } },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2), the station `K` east of it.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"K" = (/obj/machinery/skill_station,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wsKfw
wwwww
"}
"##;

const STATION: TilePos = TilePos::new(3, 2, 1);
const ENGINEER: &str = "/obj/item/skillchip/job/engineer";
const CLOWN: &str = "/obj/item/skillchip/job/clown";

const STATES: [(&str, &str); 9] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "skillchip"),
    ("icons/obj/machines/implant_chair.dmi", "implantchair"),
    ("icons/obj/machines/implant_chair.dmi", "implantchair_open"),
    ("icons/obj/machines/implant_chair.dmi", "working"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("skillchips_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/skill_station",
        ENGINEER,
        CLOWN,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    dir
}

#[derive(Resource, Default)]
struct Received {
    windows: HashMap<u32, (String, Vec<u8>)>,
}

fn record(mut ui: MessageReader<UiMessageReceived>, mut received: ResMut<Received>) {
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } => {
                received
                    .windows
                    .insert(*window, (interface.clone(), data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some(w) = received.windows.get_mut(window) {
                    w.1 = data.clone();
                }
            }
            UiServerMessage::Close { window } => {
                received.windows.remove(window);
            }
            _ => {}
        }
    }
}

struct Station(testing::Station);

impl Deref for Station {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.0
    }
}

impl DerefMut for Station {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.0
    }
}

impl Drive for Station {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.0
    }
}

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(1, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn window_id(&self) -> Option<u32> {
        let received = self.players[0].app.world().resource::<Received>();
        received
            .windows
            .iter()
            .filter(|(_, (n, _))| n == SkillStationUi::NAME)
            .map(|(id, _)| *id)
            .max()
    }

    fn data(&self) -> Option<SkillStationData> {
        let received = self.players[0].app.world().resource::<Received>();
        let id = self.window_id()?;
        received
            .windows
            .get(&id)
            .map(|(_, data)| ss13_tgui::decode(data).unwrap())
    }

    fn station_entity(&self) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(STATION)
            .iter()
            .copied()
            .find(|&e| world.get::<SkillStation>(e).is_some())
            .expect("the station")
    }

    /// A new chip of `path` straight into the player's active hand.
    fn give_chip(&mut self, path: &str) -> Entity {
        let mob = self.mob();
        let world = self.server.world_mut();
        let chip = ss13_game::construction::spawn_obj(world, path, TilePos::new(4, 2, 1)).unwrap();
        world
            .entity_mut(chip)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(chip);
        // The chip's `Skillchip` comes from an observer's queued command.
        self.tick();
        chip
    }

    fn brain(&self) -> Entity {
        skillchips::brain_of(self.server.world(), self.mob()).expect("a brain")
    }

    fn knows_rcd(&self) -> bool {
        skillchips::has_trait(self.server.world(), self.mob(), ADVANCED_RCD_KNOWLEDGE)
    }
}

#[test]
fn the_station_implants_toggles_and_extracts_a_chip() {
    let mut s = Station::start("the_station_implants_toggles_and_extracts_a_chip");
    s.without_mood();
    let mob = s.mob();
    let station = s.station_entity();
    assert!(s.get::<SkillStation>(station).inserted.is_none());

    // The chip goes into the station from the hand.
    let chip = s.give_chip(ENGINEER);
    assert!(s.try_get::<Skillchip>(chip).is_some(), "a skillchip item");
    assert!(skill_station::item_interaction(
        s.server.world_mut(),
        mob,
        chip,
        station
    ));
    assert_eq!(s.get::<SkillStation>(station).inserted, Some(chip));
    assert!(
        s.get::<Inventory>(mob)
            .hands
            .iter()
            .all(|h| *h != Some(chip))
    );

    // Shut in, the occupant gets the window.
    skill_station::close(s.server.world_mut(), station, Some(mob));
    s.run_until("inside the station", |s| {
        s.try_get::<InCloset>(mob)
            .is_some_and(|c| c.closet == station)
    });
    s.run_until("the window opens", |s| s.data().is_some());
    let data = s.data().unwrap();
    assert!(data.skillchip_ready && data.implantable, "{data:?}");
    assert_eq!(data.slots_max, 5);
    let window = s.window_id().unwrap();

    // Implanting takes 15 s; the chip lands inactive in the brain.
    s.act(0, window, "implant", &[]);
    s.run_until("working", |s| s.get::<SkillStation>(station).work.is_some());
    s.run_for(14.0);
    assert!(s.try_get::<ChipIn>(chip).is_none(), "not before 15 s");
    s.run_until("implanted", |s| s.try_get::<ChipIn>(chip).is_some());
    let brain = s.brain();
    assert_eq!(s.get::<ChipIn>(chip).0, brain);
    assert!(!s.get::<Skillchip>(chip).active);
    assert!(!s.knows_rcd());
    assert!(s.get::<SkillStation>(station).inserted.is_none());

    // Activation from the window grants the trait.
    let chip_ref = chip.to_bits().to_string();
    s.act(0, window, "toggle_activate", &[("ref", &chip_ref)]);
    s.run_until("active", |s| s.get::<Skillchip>(chip).active);
    assert!(s.knows_rcd());
    // On cooldown now: neither deactivated nor removed.
    s.act(0, window, "toggle_activate", &[("ref", &chip_ref)]);
    s.act(0, window, "remove", &[("ref", &chip_ref)]);
    s.run_for(1.0);
    assert!(s.get::<Skillchip>(chip).active);
    assert!(s.get::<SkillStation>(station).work.is_none());

    // A second job chip is refused.
    let clown = s.give_chip(CLOWN);
    let refused = skillchips::implant(s.server.world_mut(), mob, clown, false).unwrap_err();
    assert!(refused.contains("job chip"), "{refused}");

    // Off cooldown, the removal hands the chip back, inactive.
    s.server
        .world_mut()
        .get_mut::<Skillchip>(chip)
        .unwrap()
        .cooldown_until = 0.0;
    s.act(0, window, "remove", &[("ref", &chip_ref)]);
    s.run_until("removing", |s| {
        s.get::<SkillStation>(station).work.is_some()
    });
    s.run_until("removed", |s| s.try_get::<ChipIn>(chip).is_none());
    assert!(!s.get::<Skillchip>(chip).active);
    assert!(!s.knows_rcd());
    assert_eq!(s.get::<CarriedBy>(chip).0, mob);
}

#[test]
fn a_brain_leaving_its_body_deactivates_its_chips() {
    let mut s = Station::start("a_brain_leaving_its_body_deactivates_its_chips");
    s.without_mood();
    let mob = s.mob();
    let chip = s.give_chip(ENGINEER);
    let world = s.server.world_mut();
    skillchips::implant(world, mob, chip, false).unwrap();
    assert!(skillchips::try_activate(world, chip, true, true).is_none());
    assert!(skillchips::has_trait(world, mob, ADVANCED_RCD_KNOWLEDGE));
    let brain = skillchips::brain_of(world, mob).unwrap();
    organs::remove(world, brain, false);
    assert!(!world.get::<Skillchip>(chip).unwrap().active);
    assert_eq!(world.get::<ChipIn>(chip).map(|c| c.0), Some(brain));
    assert!(!skillchips::has_trait(world, mob, ADVANCED_RCD_KNOWLEDGE));
}
