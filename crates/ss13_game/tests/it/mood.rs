//! Mood and quirks over real WebTransport on 127.0.0.1: need events follow the mob's state,
//! seeing B die saddens A, a hug cheers B up, low sanity slows A down, the mood face's click
//! prints the mood, quirks apply their traits and personalities react.
// shortcut: Bad Back and the quirk items need backpack and item prototypes the fixture lacks.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::speech::Impediments;
use ss13_game::combat::NextMove;
use ss13_game::health::Stat;
use ss13_game::hud::{MOOD_CLICK, MoodHudData, MoodHudUi};
use ss13_game::hunger::Hunger;
use ss13_game::medical::traumas::Pacifist;
use ss13_game::mood::{Mood, SANITY_GREAT, SANITY_INSANE, SANITY_LEVEL_UNSTABLE, SanitySlowdown};
use ss13_game::quirks::{self, Quirk, Quirks};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;

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
        .join("mood_fixture")
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
    chat: Vec<String>,
    mood_window: Option<u32>,
}

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReader<UiMessageReceived>,
    mut received: ResMut<Received>,
) {
    for ChatReceived(message) in chat.read() {
        received.chat.push(message.plain_text());
    }
    for UiMessageReceived(message) in ui.read() {
        if let UiServerMessage::Open {
            window, interface, ..
        } = message
            && interface == MoodHudUi::NAME
        {
            received.mood_window = Some(*window);
        }
    }
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

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn heard(&self, p: usize, text: &str) -> bool {
        self.received(p).chat.iter().any(|l| l.contains(text))
    }

    fn mood(&self, mob: Entity) -> &Mood {
        self.get::<Mood>(mob)
    }

    fn has_event(&self, mob: Entity, category: &str) -> bool {
        self.mood(mob).get(category).is_some()
    }
}

#[test]
fn needs_deaths_hugs_and_sanity() {
    let mut s = Station::start("needs_deaths_hugs_and_sanity");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert_eq!(s.mood(a).sanity, 100.0);

    // Starving: the nutrition moodlet follows within a mood fire.
    s.server.world_mut().get_mut::<Hunger>(a).unwrap().nutrition = 100.0;
    s.run_until("A starves", |s| {
        s.mood(a)
            .get("nutrition")
            .is_some_and(|e| e.id == "starving")
    });
    // A neutral mood or worse lets sanity sink below neutral.
    s.run_until_for(10.0, "sanity sinks", |s| s.mood(a).sanity < 100.0);
    // Fed again, the moodlet changes.
    s.server.world_mut().get_mut::<Hunger>(a).unwrap().nutrition = 400.0;
    s.run_until("A is fed", |s| {
        s.mood(a).get("nutrition").is_some_and(|e| e.id == "fed")
    });

    // The face reaches A's client; a click prints the mood.
    s.run_until("A gets the mood face", |s| {
        s.received(0).mood_window.is_some()
    });
    let window = s.received(0).mood_window.unwrap();
    s.act(0, window, MOOD_CLICK, &[]);
    s.run_until("A reads its mood", |s| {
        s.heard(0, "Мое текущее ментальное состояние:") && s.heard(0, "Я недавно поел.")
    });
    assert!(!s.heard(1, "Мое текущее ментальное состояние:"));

    // A hug: B gets a hug moodlet.
    s.run_until("off cooldown", |s| {
        s.server.world().resource::<Time>().elapsed_secs_f64() >= s.get::<NextMove>(a).0
    });
    s.click(0, b);
    s.run_until("B is hugged", |s| s.has_event(b, "hug"));
    assert!(!s.has_event(a, "saw_death"));

    // B dies next to A: A saw it.
    s.server.world_mut().entity_mut(b).insert(Stat::Dead);
    s.run_until("A saw B die", |s| s.has_event(a, "saw_death"));
    let saw = s.mood(a).get("saw_death").unwrap();
    assert_eq!(saw.mood_change, -8.0);
    assert!(saw.description.contains("умер"), "{}", saw.description);

    // Low sanity slows A down and tints the face.
    s.server.world_mut().get_mut::<Mood>(a).unwrap().set_sanity(
        60.0,
        SANITY_INSANE,
        SANITY_GREAT,
        true,
    );
    s.run_until("A is slowed", |s| {
        s.get::<SanitySlowdown>(a).0 == 0.25 && s.get::<MoodHudData>(a).color == [0xdf, 0xa6, 0x5b]
    });
    assert_eq!(s.mood(a).sanity_level, SANITY_LEVEL_UNSTABLE);
    assert_eq!(s.mood(a).action_multiplier(), 1.25);
}

#[test]
fn quirks_apply_their_traits() {
    let mut s = Station::start("quirks_apply_their_traits");
    let a = s.players[0].mob;
    {
        let world = s.server.world_mut();
        for quirk in [
            Quirk::Mute,
            Quirk::Pacifist,
            Quirk::Hypersensitive,
            Quirk::Unstable,
        ] {
            assert!(quirks::add_quirk(world, a, quirk, true, false));
        }
        // Not twice.
        assert!(!quirks::add_quirk(world, a, Quirk::Mute, true, false));
    }
    assert!(s.get::<Quirks>(a).has(Quirk::Pacifist));
    assert!(s.get::<Impediments>(a).mute);
    assert!(s.try_get::<Pacifist>(a).is_some());
    assert_eq!(s.mood(a).mood_modifier, 1.5);
    assert!(s.mood(a).unstable);
    s.run_until("A hears the gain lines", |s| {
        s.heard(0, "Вы обнаруживаете, что не можете говорить!")
    });
    // Removing a quirk takes its trait away.
    {
        let world = s.server.world_mut();
        assert!(quirks::remove_quirk(world, a, Quirk::Mute));
        assert!(!quirks::remove_quirk(world, a, Quirk::Mute));
    }
    assert!(s.try_get::<Impediments>(a).is_none_or(|i| !i.mute));
    assert!(s.try_get::<Pacifist>(a).is_some());

    // Monochromacy turns the view grey (`crate::vision`).
    quirks::add_quirk(s.server.world_mut(), a, Quirk::Monochromacy, false, false);
    assert!(
        ss13_game::vision::sight(s.server.world(), a)
            .unwrap()
            .colorblind
    );
}

#[test]
fn more_quirks_and_skills_apply() {
    use ss13_chat::language::{FULL, LanguageHolder, LanguageId, understanding};
    use ss13_game::medical::wounds::Limbs;
    use ss13_game::skills::{self, Skill};

    let mut s = Station::start("more_quirks_and_skills_apply");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    {
        let world = s.server.world_mut();
        for quirk in [
            Quirk::SoftSpoken,
            Quirk::Deaf,
            Quirk::Csl,
            Quirk::Hemiplegic,
            Quirk::BodyPurist,
        ] {
            assert!(quirks::add_quirk(world, a, quirk, false, false));
        }
        assert!(quirks::add_quirk(world, b, Quirk::Foreigner, false, false));
        assert!(quirks::add_quirk(world, b, Quirk::TinMan, false, true));
        assert!(quirks::add_quirk(world, b, Quirk::BodyPurist, false, false));
    }
    assert!(s.get::<Impediments>(a).force_whisper);
    assert!(!ss13_game::sound::can_hear(s.server.world(), a));
    let common = |s: &Station, mob: Entity| {
        understanding(s.try_get::<LanguageHolder>(mob), false, LanguageId::Common)
    };
    assert_eq!(common(&s, a), 90);
    assert!(common(&s, b) < FULL);
    // One side of the body is paralyzed, not both.
    let parts = &s.get::<Limbs>(a).parts;
    assert!(parts.iter().any(|l| l.paralyzed) && !parts.iter().all(|l| l.paralyzed));
    // Tin Man's prosthetics upset the Body Purist within a quirk fire; A has none.
    s.run_until("B hates the prosthetics", |s| s.has_event(b, "body_purist"));
    assert!(s.mood(b).get("body_purist").unwrap().mood_change <= -10.0);
    assert!(!s.has_event(a, "body_purist"));
    // CSL gone: the species' Common is back.
    assert!(quirks::remove_quirk(s.server.world_mut(), a, Quirk::Csl));
    assert_eq!(common(&s, a), FULL);

    // Skills: experience levels up with the level's message; the modifier follows.
    skills::adjust_experience(s.server.world_mut(), b, Skill::Cleaning, 120.0, false);
    assert_eq!(
        skills::level(s.server.world(), b, Skill::Cleaning),
        skills::LEVEL_NOVICE
    );
    assert_eq!(
        skills::modifier(
            s.server.world(),
            b,
            Skill::Cleaning,
            skills::Modifier::Speed
        ),
        0.9
    );
    s.run_until("B hears the level up", |s| {
        s.heard(1, "I'm starting to figure out what Cleaning really is!")
    });
}

#[test]
fn personalities_react_in_the_round() {
    use ss13_game::personality::{self, Personality};
    let mut s = Station::start("personalities_react_in_the_round");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    {
        let world = s.server.world_mut();
        assert!(personality::apply_to_mob(world, a, Personality::Paranoid));
        assert!(personality::apply_to_mob(world, b, Personality::Sensitive));
    }
    assert!(s.mood(a).has_personality(Personality::Paranoid));
    assert!((s.mood(b).mood_modifier - 1.2).abs() < 1e-6);
    // SSpersonalities (3 s): A is alone with B.
    s.run_until("A fears being alone with B", |s| {
        s.mood(a)
            .get("paranoia_personality")
            .is_some_and(|e| e.id == "paranoid_one_on_one")
    });
    // Removing it clears its moodlet.
    assert!(personality::remove_from_mob(
        s.server.world_mut(),
        a,
        Personality::Paranoid
    ));
    assert!(!s.has_event(a, "paranoia_personality"));
}
