//! Succumbing over real WebTransport on 127.0.0.1: A in hard crit has the succumb alert, clicks
//! it, types last words into the text prompt; B next to A reads them whispered "with its last
//! breath", A dies (oxygen damage to the death threshold) with a time of death, and the alert is
//! gone. A hard crit whisper typed into the say line is cut by the health left and kills too.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::breath::{AlertCategory, Alerts};
use ss13_game::death::TimeOfDeath;
use ss13_game::health::{DamageType, HEALTH_THRESHOLD_DEAD, Health, Stat, apply_damage};
use ss13_game::hud::{ALERT_CLICK, AlertHudData, AlertHudUi, alert_click_name};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::text_input::TextInputUi;

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
        .join("succumb_fixture")
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
struct Received {
    windows: Vec<(u32, String)>,
    chat: Vec<String>,
}

fn record(
    mut ui: MessageReader<UiMessageReceived>,
    mut chat: MessageReader<ChatReceived>,
    mut received: ResMut<Received>,
) {
    for UiMessageReceived(message) in ui.read() {
        if let UiServerMessage::Open {
            window, interface, ..
        } = message
        {
            received.windows.push((*window, interface.clone()));
        }
    }
    for ChatReceived(message) in chat.read() {
        received.chat.push(message.plain_text());
    }
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(2, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        let (a, b) = (s.players[0].mob, s.players[1].mob);
        let world = s.server.world_mut();
        world.entity_mut(a).insert(TilePos::new(2, 3, 1));
        world.entity_mut(b).insert(TilePos::new(3, 3, 1));
        Self(s)
    }

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn window(&self, p: usize, interface: &str) -> Option<u32> {
        self.received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i)| i == interface)
            .map(|(w, _)| *w)
    }
}

#[test]
fn succumb_with_last_words() {
    let mut s = Station::start("succumb_with_last_words");
    let a = s.players[0].mob;
    // A in hard crit (health -40).
    {
        let world = s.server.world_mut();
        let mut health = world.get::<Health>(a).unwrap().clone();
        let mut stat = *world.get::<Stat>(a).unwrap();
        apply_damage(&mut health, &mut stat, DamageType::Brute, 140.0);
        assert_eq!(stat, Stat::HardCrit);
        world.entity_mut(a).insert((health, stat));
    }
    s.run_until("A has the succumb alert", |s| {
        s.get::<Alerts>(a).get(AlertCategory::Succumb).is_some()
            && s.get::<AlertHudData>(a)
                .alerts
                .iter()
                .any(|x| x.name == "Сдаться")
            && s.window(0, AlertHudUi::NAME).is_some()
    });
    let index = s
        .get::<Alerts>(a)
        .0
        .iter()
        .position(|x| x.category == AlertCategory::Succumb)
        .unwrap();
    let at = index.to_string();
    let window = s.window(0, AlertHudUi::NAME).unwrap();
    s.act(
        0,
        window,
        ALERT_CLICK,
        &[
            ("index", at.as_str()),
            ("name", alert_click_name("Сдаться")),
        ],
    );
    s.run_until("the last words prompt opens", |s| {
        s.window(0, TextInputUi::NAME).is_some()
    });
    assert_ne!(s.get::<Stat>(a), &Stat::Dead, "asking does not kill");
    let prompt = s.window(0, TextInputUi::NAME).unwrap();
    s.act(0, prompt, "submit", &[("entry", "передайте маме привет")]);
    s.run_until("A succumbs", |s| s.get::<Stat>(a) == &Stat::Dead);
    assert!(s.get::<Health>(a).health() <= HEALTH_THRESHOLD_DEAD);
    s.run_until(
        "B hears the whisper, A has a time of death and no alert",
        |s| {
            s.received(1).chat.iter().any(|line| {
                // `Ellipsis` may put "..." between the words.
                line.contains("шепчет своим последним вздохом") && line.contains("ередайте")
            }) && s.try_get::<TimeOfDeath>(a).is_some()
                && s.get::<Alerts>(a).get(AlertCategory::Succumb).is_none()
        },
    );
}

#[test]
fn a_hard_crit_whisper_is_cut_short_and_kills() {
    use ss13_shared::protocol::ClientIntent;
    let mut s = Station::start("a_hard_crit_whisper_is_cut_short_and_kills");
    let a = s.players[0].mob;
    // Health -80: 20 points above death, 19 characters get out (fewer as crit drains it).
    {
        let world = s.server.world_mut();
        let mut health = world.get::<Health>(a).unwrap().clone();
        let mut stat = *world.get::<Stat>(a).unwrap();
        apply_damage(&mut health, &mut stat, DamageType::Brute, 180.0);
        assert_eq!(stat, Stat::HardCrit);
        world.entity_mut(a).insert((health, stat));
    }
    s.run_until("A's breath is known", |s| {
        s.try_get::<ss13_chat::speech::LastBreath>(a).is_some()
    });
    // A plain say from hard crit is nothing.
    s.send(0, ClientIntent::Say("помогите".into()));
    s.run_for(1.0);
    assert!(
        !s.received(1).chat.iter().any(|l| l.contains("омогите")),
        "hard crit cannot talk"
    );
    assert_ne!(s.get::<Stat>(a), &Stat::Dead);
    s.send(
        0,
        ClientIntent::Say("#это сделал главный инженер станции".into()),
    );
    s.run_until("A succumbs after its last words", |s| {
        s.get::<Stat>(a) == &Stat::Dead
    });
    s.run_until("B reads the cut words", |s| {
        s.received(1).chat.iter().any(|l| {
            l.contains("шепчет своим последним вздохом")
                && l.contains("сделал")
                && l.contains("-..")
        })
    });
    assert!(
        !s.received(1).chat.iter().any(|l| l.contains("инженер")),
        "the rest is never said"
    );
}
