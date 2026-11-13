//! Hearing over real WebTransport on 127.0.0.1 (tg `get_hearers_in_view`): A speaks; B in the
//! same room hears it, C as close behind a wall does not, even a whisper's "too far" line; dead,
//! C hears through the wall (tg adds the dead within 7 tiles).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::health::{DamageType, Health, Stat, apply_damage};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_net::chat::client::ChatReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::ClientIntent;

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

/// A room of three by three, a wall column, another room.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwww
wfffwfffw
wsfswsffw
wfffwfffw
wwwwwwwww
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
        .join("hearing_fixture")
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

#[derive(Resource, Default)]
struct Received(Vec<String>);

fn record(mut chat: MessageReader<ChatReceived>, mut received: ResMut<Received>) {
    for ChatReceived(message) in chat.read() {
        received.0.push(message.plain_text());
    }
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn heard(&self, p: usize, text: &str) -> bool {
        self.players[p]
            .app
            .world()
            .resource::<Received>()
            .0
            .iter()
            .any(|line| line.contains(text))
    }
}

#[test]
fn walls_stop_speech_but_not_the_dead() {
    let assets = fixture_assets("walls_stop_speech_but_not_the_dead");
    let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
        app.add_plugins(ss13_chat::server::ChatServerPlugin);
    });
    s.join_all(3, |app| {
        app.init_resource::<Received>().add_systems(Update, record);
    });
    let (a, b, c) = (s.players[0].mob, s.players[1].mob, s.players[2].mob);
    {
        let world = s.server.world_mut();
        world.entity_mut(a).insert(TilePos::new(2, 3, 1));
        world.entity_mut(b).insert(TilePos::new(4, 3, 1));
        // 4 tiles from A, behind the wall at x = 5.
        world.entity_mut(c).insert(TilePos::new(6, 3, 1));
    }
    let mut s = Station(s);
    s.send(0, ClientIntent::Say("Слышно меня".into()));
    s.run_until("B hears A", |s| s.heard(1, "Слышно меня"));
    s.send(0, ClientIntent::Say("#тихо тихо".into()));
    s.run_until("B hears A whisper", |s| s.heard(1, "шепчет"));
    s.run_for(1.0);
    assert!(!s.heard(2, "Слышно меня"), "the wall stops speech");
    assert!(!s.heard(2, "шепчет"), "and the whisper's \"too far\" line");

    // Dead, C hears through the wall.
    {
        let world = s.server.world_mut();
        let mut health = world.get::<Health>(c).unwrap().clone();
        let mut stat = *world.get::<Stat>(c).unwrap();
        apply_damage(&mut health, &mut stat, DamageType::Brute, 250.0);
        assert_eq!(stat, Stat::Dead);
        world.entity_mut(c).insert((health, stat));
    }
    s.run_until("C is dead for speech", |s| {
        s.try_get::<ss13_chat::speech::SpeakerStat>(c)
            == Some(&ss13_chat::speech::SpeakerStat::Dead)
    });
    s.send(0, ClientIntent::Say("Покойся с миром".into()));
    s.run_until("dead C hears A", |s| s.heard(2, "Покойся с миром"));
}

#[test]
fn saymodes_reach_their_listeners_only() {
    use ss13_chat::speech::AlienHivemind;
    let assets = fixture_assets("saymodes_reach_their_listeners_only");
    let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
        app.add_plugins(ss13_chat::server::ChatServerPlugin);
    });
    s.join_all(3, |app| {
        app.init_resource::<Received>().add_systems(Update, record);
    });
    let (a, b, c) = (s.players[0].mob, s.players[1].mob, s.players[2].mob);
    {
        let world = s.server.world_mut();
        world.entity_mut(a).insert(TilePos::new(2, 3, 1));
        world.entity_mut(b).insert(TilePos::new(3, 3, 1));
        world.entity_mut(c).insert(TilePos::new(6, 3, 1));
        // A and C in the xenomorph hive, B next to A outside it.
        world.entity_mut(a).insert(AlienHivemind);
        world.entity_mut(c).insert(AlienHivemind);
    }
    let mut s = Station(s);
    s.run_until("A's body is known to emotes", |s| {
        s.try_get::<ss13_chat::emote::EmoteUser>(a).is_some()
    });
    s.send(0, ClientIntent::Say(":ф охота начинается".into()));
    s.run_until("C reads the hive through the wall", |s| {
        s.heard(2, "Связь улья") && s.heard(2, "Охота начинается")
    });
    // tg's vocal cords saymode: a human without voice cords says nothing.
    s.send(0, ClientIntent::Say(":ч тайные слова".into()));
    s.send(0, ClientIntent::Say("Обычные слова".into()));
    s.run_until("B hears the plain words", |s| s.heard(1, "Обычные слова"));
    assert!(!s.heard(1, "Охота"), "the hive is not said aloud");
    assert!(!s.heard(1, "Тайные"), "nor the vocal cords' words");
}
