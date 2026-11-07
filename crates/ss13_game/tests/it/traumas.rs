//! Brain traumas over real WebTransport on 127.0.0.1: a mute A cannot speak (B hears nothing),
//! a stuttering A's words reach B stuttered, a pacified A's punch is refused, neurine cures a
//! mild trauma and mannitol heals the brain.

use std::path::{Path, PathBuf};

use crate::harness::chat::{Received, record};
use bevy::prelude::*;
use ss13_game::chem::{Chem, MobReagents};
use ss13_game::combat::{CombatMode, NextMove};
use ss13_game::health::Health;
use ss13_game::medical::organs::{self, Slot};
use ss13_game::medical::traumas::{self, Kind, MANNITOL, NEURINE};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1,
        "inverse_chem": null, "inverse_chem_val": 0.25, "purge_multiplier": 1 } },
    "/datum/reagent/medicine": { "parent": "/datum/reagent", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent/medicine" } } },
    "/datum/reagent/medicine/mannitol": { "parent": "/datum/reagent/medicine",
        "declared_vars": { "name": "Mannitol" } },
    "/datum/reagent/medicine/neurine": { "parent": "/datum/reagent/medicine",
        "declared_vars": { "name": "Neurine" } },
    "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
        "H_ion_release": 0.01, "determin_ph_range": 4, "is_cold_recipe": 0,
        "mix_message": "The solution begins to bubble.", "mob_react": 1, "optimal_ph_max": 9,
        "optimal_ph_min": 5, "optimal_temp": 500, "overheat_temp": 900, "ph_exponent_factor": 2,
        "purity_min": 0.15, "rate_up_lim": 30, "reaction_flags": 0,
        "required_catalysts": { "list": [] }, "required_container": null,
        "required_container_accepts_subtypes": 0, "required_reagents": { "list": [] },
        "required_temp": 100, "results": { "list": [] }, "temp_exponent_factor": 2,
        "thermic_constant": 0 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
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

/// A and B spawn side by side.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfffw
wsfsw
wfffw
wwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("traumas_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let dir = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    dir
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(2, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        let (a, b) = (s.players[0].mob, s.players[1].mob);
        let world = s.server.world_mut();
        world.entity_mut(a).insert(TilePos::new(2, 3, 1));
        world.entity_mut(b).insert(TilePos::new(3, 3, 1));
        Self(s)
    }

    fn heard(&self, p: usize, text: &str) -> bool {
        self.players[p]
            .app
            .world()
            .resource::<Received>()
            .0
            .iter()
            .any(|l| l.contains(text))
    }

    fn gain(&mut self, p: usize, kind: Kind) {
        let mob = self.players[p].mob;
        let world = self.server.world_mut();
        let brain = organs::organ(world, mob, Slot::Brain).unwrap();
        assert!(traumas::gain(world, brain, kind, None));
    }

    fn inject(&mut self, p: usize, reagent: &str, amount: f32) {
        let mob = self.players[p].mob;
        let world = self.0.server.world_mut();
        let chem = world.resource::<Chem>().0.clone();
        let id = chem.id(reagent).unwrap();
        world
            .get_mut::<MobReagents>(mob)
            .unwrap()
            .blood
            .add_reagent(&chem, id, amount, 310.0, None, None, true, false);
    }
}

#[test]
fn speech_pacifism_and_brain_medicine() {
    let mut s = Station::start("speech_pacifism_and_brain_medicine");
    let (a, b) = (s.players[0].mob, s.players[1].mob);

    // Mute: the say is refused, B hears nothing.
    s.gain(0, Kind::Mute);
    s.run_until("A learns it", |s| s.heard(0, "Вы разучились говорить!"));
    s.send(0, ClientIntent::Say("молчание золото".into()));
    s.run_until("the say is refused", |s| {
        s.heard(0, "Вы осознаете, что не можете говорить!")
    });
    s.hold(1.0, "B hears nothing", |s| !s.heard(1, "молчание"));

    // A brain surgery's cure gives the voice back; a stutter then reaches B.
    {
        let world = s.server.world_mut();
        let brain = organs::organ(world, a, Slot::Brain).unwrap();
        traumas::cure_all(world, brain, traumas::Resilience::Surgery);
    }
    s.gain(0, Kind::Stuttering);
    s.send(0, ClientIntent::Say("ну давай же попробуем снова".into()));
    s.run_until("B hears a stutter", |s| s.heard(1, "-"));

    // Pacifism: a punch in combat mode is refused and B is unhurt.
    s.gain(0, Kind::Pacifism);
    s.server.world_mut().entity_mut(a).insert(CombatMode(true));
    s.run_until("off cooldown", |s| {
        s.server.world().resource::<Time>().elapsed_secs_f64() >= s.get::<NextMove>(a).0
    });
    let target = s.0.net_id(0, b).unwrap();
    s.send(
        0,
        ClientIntent::Click {
            target,
            modifiers: ClickModifiers::default(),
        },
    );
    s.run_until("the punch is refused", |s| {
        if s.heard(0, "Вы не хотите причинить вред") {
            return true;
        }
        eprintln!("{:?}", s.players[0].app.world().resource::<Received>().0);
        false
    });
    assert_eq!(s.get::<Health>(b).brute, 0.0);

    // Neurine cures the stutter (a mild trauma), not the severe pacifism.
    s.inject(0, NEURINE, 30.0);
    s.run_until_for(120.0, "neurine cures the stutter", |s| {
        !traumas::has(s.server.world(), a, Kind::Stuttering)
    });
    assert!(traumas::has(s.server.world(), a, Kind::Pacifism));

    // Mannitol heals the brain.
    organs::set_organ_damage(s.server.world_mut(), b, Slot::Brain, 10.0);
    s.inject(1, MANNITOL, 10.0);
    s.run_until_for(30.0, "mannitol heals the brain", |s| {
        organs::state(s.server.world(), b, Slot::Brain)
            .unwrap()
            .damage
            == 0.0
    });
}

/// Speech status effects (`crate::status_effects`): silence refuses the say like a mute, a
/// sleeping A is knocked out (floored, blind) and says nothing until woken.
#[test]
fn silence_and_sleep_stop_speech() {
    use ss13_game::status_effects::{self, Incap, Incapacitation, Timed};
    let mut s = Station::start("silence_and_sleep_stop_speech");
    let a = s.players[0].mob;

    status_effects::set_timed(s.server.world_mut(), a, Timed::Silence, 30.0, true);
    s.run_until("the chat sees the silence", |s| {
        s.server
            .world()
            .get::<ss13_chat::speech::SpeechEffects>(a)
            .is_some_and(|e| e.silenced)
    });
    s.send(0, ClientIntent::Say("тишина вокруг".into()));
    s.run_until("the say is refused", |s| {
        s.heard(0, "Вы осознаете, что не можете говорить!")
    });
    s.hold(1.0, "B hears nothing", |s| !s.heard(1, "тишина"));
    status_effects::set_timed(s.server.world_mut(), a, Timed::Silence, 0.0, false);

    status_effects::apply(s.server.world_mut(), a, Incap::Sleeping, 60.0);
    s.run_until("A falls asleep", |s| {
        let traits = s.get::<Incapacitation>(a);
        traits.knocked_out && traits.floored && traits.hands_blocked
    });
    assert!(ss13_game::vision::is_blind(s.server.world(), a));
    s.send(0, ClientIntent::Say("храп храп".into()));
    s.hold(1.0, "B hears no sleep talk", |s| !s.heard(1, "храп"));

    status_effects::set(s.server.world_mut(), a, Incap::Sleeping, 0.0);
    s.run_until("A wakes", |s| !s.get::<Incapacitation>(a).knocked_out);
    s.send(0, ClientIntent::Say("доброе утро".into()));
    s.run_until("B hears A", |s| s.heard(1, "оброе утро"));
}

/// A phobia (`ss13_game::medical::fear`): A, afraid of spiders, hears B say the word and freaks
/// out; terror builds up.
#[test]
fn a_phobia_hears_its_word() {
    use ss13_game::medical::fear::{self, Fearful};
    let mut s = Station::start("a_phobia_hears_its_word");
    let a = s.players[0].mob;
    let spiders = fear::phobia_index("spiders").unwrap();
    s.gain(0, Kind::Phobia(spiders));
    s.run_until("A learns it", |s| {
        s.heard(0, "Вы начинаете находить пауков очень нервирующим")
    });
    assert!(s.server.world().get::<Fearful>(a).is_some());
    s.send(1, ClientIntent::Say("look out, a spider".into()));
    s.run_until("B's words reach A", |s| s.heard(0, "spider"));
    s.run_until("A freaks out", |s| s.heard(0, "Hearing spider"));
    assert!(s.server.world().get::<Fearful>(a).unwrap().buildup > 0.0);
    // The cure takes the fear away.
    {
        let world = s.server.world_mut();
        let brain = organs::organ(world, a, Slot::Brain).unwrap();
        traumas::cure_all(world, brain, traumas::Resilience::Basic);
    }
    assert!(s.server.world().get::<Fearful>(a).is_none());
}
