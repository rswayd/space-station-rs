//! The changeling over real WebTransport on 127.0.0.1: two clients in the lobby, one opts in to
//! the changeling; an admin-queued "Roundstart Changeling" ruleset (forced, deterministic) makes
//! them the changeling at round start. The changeling gets the greeting, the info panel, the
//! chemical counter and its buttons; speaks in the hivemind (the other player does not read it);
//! grabs the other player aggressively and absorbs them in three stages (the victim dies husked
//! and bloodless, its profile is stored); transforms into them (the name others hear, the body);
//! fakes death and revives; buys the arm blade in the cellular emporium, grows it and readapts;
//! the round-end report lists it with its extracted genes.

use std::path::{Path, PathBuf};

use crate::harness::{self, Client};
use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::actions::{ActionButtonsData, ActionButtonsUi, TRIGGER};
use ss13_game::antag::changeling::server::{Changeling, Stasis};
use ss13_game::antag::changeling::{
    AntagInfoChangelingUi, CellularEmporiumUi, ChangelingHudData, ChangelingHudUi, ChangelingInfo,
    EmporiumData, EmporiumStatic, ROLE_CHANGELING,
};
use ss13_game::antag::dynamic::{CHANGELING_RULESET, DevTraitor, DynamicConfig};
use ss13_game::health::{DeathComa, Health, Stat};
use ss13_game::human::{ASSISTANT_SHOES, ASSISTANT_UNIFORM, HAIR_STYLES, HumanLook, Husk, parts};
use ss13_game::inventory::Inventory;
use ss13_game::jobs::{Jobs, PlayerJob, fallback_jobs};
use ss13_game::lobby::{
    CHARACTER_SETUP, GameState, JobPrefsUi, SET_ANTAGS, SET_JOB_PREFERENCE, TOGGLE_READY,
};
use ss13_game::medical::blood::Blood;
use ss13_game::pull::GrabState;
use ss13_game::round::{RoundConfig, Ticker};
use ss13_game::server::Players;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::antag::{ANTAG_FILE, AntagData};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, JobsData};
use ss13_net::server::Interest;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::ListInputUi;

const ARM_BLADE: &str = "/obj/item/melee/arm_blade";

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark",
        "vars": { "name": "start" }, "declared_vars": { "name": "start" } },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Assistant" }, "declared_vars": { "name": "Assistant" } },
    "/obj/effect/landmark/latejoin": { "parent": "/obj/effect/landmark" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/melee": { "parent": "/obj/item" },
    "/obj/item/melee/arm_blade": { "parent": "/obj/item/melee", "declared_vars": {
        "name": "arm blade", "icon_state": "arm_blade", "force": 25 } },
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

/// Assistant starts `s` (2,2) and (8,2), latejoin `L`.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wffffffffw
wfffffffLw
wffffffffw
wsffffffsw
wwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "arm_blade"),
];

/// Every sprite a human body can use here (both bodies, every hair style, husked or not), so
/// player mobs are drawn from their look and a transform changes the drawn body.
fn human_states() -> Vec<(String, String)> {
    let mut out = vec![("icons/mob/human/human.dmi".to_owned(), "blank".to_owned())];
    for female in [false, true] {
        for hair in HAIR_STYLES {
            for husk in [false, true] {
                let look = HumanLook {
                    female,
                    species: Default::default(),
                    mutant_color: [0, 0, 0],
                    skin_tone: "albino".into(),
                    hair_style: Some(hair.into()),
                    hair_color: [0, 0, 0],
                    facial_hair_style: None,
                    facial_hair_color: [0, 0, 0],
                    lip_style: None,
                    lip_color: [0, 0, 0],
                    eye_color: [0, 0, 0],
                    uniform: Some(ASSISTANT_UNIFORM.into()),
                    shoes: Some(ASSISTANT_SHOES.into()),
                    husk,
                    hulk: false,
                    missing: 0,
                    robotic: 0,
                    eyeless: false,
                    tail: None,
                };
                for part in parts(&look).unwrap() {
                    out.push((part.dmi.to_owned(), part.state));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn fixture_antag() -> AntagData {
    AntagData {
        phonetic_alphabet: ["Альфа", "Браво", "Чарли"].map(str::to_owned).to_vec(),
        ..Default::default()
    }
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("changeling_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data: JobsData = fallback_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/latejoin",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        ARM_BLADE,
    ];
    used.extend(
        data.outfit_items()
            .into_iter()
            .filter(|p| types.types.contains_key(*p)),
    );
    let humans = human_states();
    let mut states: Vec<(&str, &str)> = STATES.to_vec();
    states.extend(humans.iter().map(|(d, s)| (d.as_str(), s.as_str())));
    let jobs = data.to_ron();
    let antag = fixture_antag().to_ron();
    write_fixture(
        &dir,
        TYPES,
        &used,
        &states,
        &[("map.dmm", MAP), (JOBS_FILE, &jobs), (ANTAG_FILE, &antag)],
    )
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0, context = harness::chat_context);
impl Client for Station {}

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        Self(testing::Station::start(
            fixture_map(&assets, &["map.dmm"]),
            |app| {
                // The lobby waits for `start_now` (an admin's "start now"): a countdown in game
                // time could run out while the clients are still setting up over the network.
                app.insert_resource(RoundConfig {
                    lobby_secs: 600.0,
                    round_end_secs: 600.0,
                    ..RoundConfig::tg()
                })
                // An admin queued one changeling ruleset: no tier roll.
                .insert_resource(DynamicConfig {
                    forced_rulesets: Some(vec![CHANGELING_RULESET]),
                    protect_roles: true,
                    seed: Some(7),
                    dev_traitor: DevTraitor::Off,
                })
                .add_plugins(ChatServerPlugin);
            },
        ))
    }

    fn mob(&self, p: usize) -> Option<Entity> {
        self.server
            .world()
            .resource::<Players>()
            .mob(self.players[p].link)
    }

    fn job(&self, p: usize) -> String {
        let world = self.server.world();
        let job = world.get::<PlayerJob>(self.mob(p).unwrap()).unwrap().0;
        world.resource::<Jobs>().data.jobs[job].title.clone()
    }
}

impl Station {
    fn ling(&self, mob: Entity) -> Changeling {
        self.server.world().get::<Changeling>(mob).unwrap().clone()
    }

    fn buttons(&self, p: usize) -> ActionButtonsData {
        let window = self
            .window(p, ActionButtonsUi::NAME)
            .expect("action buttons");
        self.latest_data(p, window)
    }

    fn trigger(&mut self, p: usize, id: &str) {
        let window = self
            .window(p, ActionButtonsUi::NAME)
            .expect("action buttons");
        self.act(p, window, TRIGGER, &[("id", id)]);
    }

    fn say(&mut self, p: usize, text: &str) {
        self.send(p, ClientIntent::Say(text.into()));
    }
}

#[test]
fn roundstart_changeling_absorbs_transforms_and_evolves() {
    let mut s = Station::start("round");
    let (a, b) = (s.connect(), s.connect());

    // A: assistant, changeling on; B: assistant, no antagonist preference.
    for p in [a, b] {
        s.lobby_act(p, CHARACTER_SETUP);
        let (prefs, _) = s.wait_window(p, JobPrefsUi::NAME);
        s.act(
            p,
            prefs,
            SET_JOB_PREFERENCE,
            &[("job", "Assistant"), ("level", "3")],
        );
        if p == a {
            s.act(
                p,
                prefs,
                SET_ANTAGS,
                &[("antags", ROLE_CHANGELING), ("toggled", "1")],
            );
        }
    }
    s.run_until("preferences stored", |s| {
        s.server
            .world()
            .get::<ss13_game::antag::dynamic::AntagPreferences>(s.players[a].link)
            .is_some_and(|p| p.wants(ROLE_CHANGELING))
            && [a, b].iter().all(|&p| {
                s.server
                    .world()
                    .get::<ss13_game::round::JobPreferences>(s.players[p].link)
                    .is_some_and(|prefs| !prefs.levels.is_empty())
            })
    });
    s.lobby_act(a, TOGGLE_READY);
    s.lobby_act(b, TOGGLE_READY);
    s.run_until("both ready", |s| {
        [a, b]
            .iter()
            .all(|&p| s.received(p).lobby.as_ref().is_some_and(|(_, l)| l.ready))
    });
    s.start_now();
    s.run_until("the round starts", |s| {
        s.server.world().resource::<Ticker>().state() >= GameState::Playing
    });

    // A is the changeling: greeting, panel, counter, buttons.
    let (ling, victim) = (s.mob(a).unwrap(), s.mob(b).unwrap());
    assert!(s.server.world().get::<Changeling>(victim).is_none());
    assert_eq!(s.job(a), "Assistant");
    s.wait_chat(a, "Вы - Генокрад.");
    s.wait_chat(a, "Задача #1: Извлеките ");
    let (info_window, _) = s.wait_window(a, AntagInfoChangelingUi::NAME);
    let info: ChangelingInfo = s.latest_data(a, info_window);
    let id_name = s.ling(ling).id_name;
    assert_eq!(info.true_name, id_name);
    assert!(info.hive_name.ends_with("-hive"), "{}", info.hive_name);
    assert_eq!(info.absorbed_dna, 1);
    assert!(info.objectives.len() >= 2, "{:?}", info.objectives);
    let (hud_window, _) = s.wait_window(a, ChangelingHudUi::NAME);
    let hud: ChangelingHudData = s.latest_data(a, hud_window);
    assert_eq!(hud.chem_max, 75);
    s.wait_window(a, ActionButtonsUi::NAME);
    let ids: Vec<String> = s.buttons(a).buttons.iter().map(|b| b.id.clone()).collect();
    assert_eq!(
        ids,
        [
            "cellular_emporium",
            "absorb_dna",
            "transform",
            "fakedeath",
            "regenerate"
        ]
    );
    assert!(s.received(b).chat.iter().all(|l| !l.contains("Генокрад")));

    // The hivemind: the changeling reads its own message, the crew does not hear it.
    s.say(a, ":g Улей, слушай.");
    s.wait_chat(a, &format!("{id_name}: Улей, слушай."));
    s.run_for(0.2);
    assert!(s.received(b).chat.iter().all(|l| !l.contains("Улей")));

    // B next to A; two ctrl-clicks: a passive grab, then an aggressive one.
    let at = *s.server.world().get::<TilePos>(ling).unwrap();
    s.server
        .world_mut()
        .entity_mut(victim)
        .insert(TilePos::new(at.x + 1, at.y, at.z));
    s.run_until("B is seen by A", |s| {
        s.server
            .world()
            .resource::<Interest>()
            .net_id(s.players[a].link, victim)
            .is_some()
    });
    let ctrl = ClickModifiers {
        ctrl: true,
        ..default()
    };
    // Absorbing without a grab is refused.
    s.trigger(a, "absorb_dna");
    s.wait_chat(a, "нужно взять в захват!");
    s.click_with(a, victim, ctrl);
    s.run_until("a passive grab", |s| {
        s.server.world().get::<GrabState>(ling) == Some(&GrabState::Passive)
    });
    s.run_for(1.1);
    s.click_with(a, victim, ctrl);
    s.run_until("an aggressive grab", |s| {
        s.server.world().get::<GrabState>(ling) == Some(&GrabState::Aggressive)
    });
    let victim_name = s
        .server
        .world()
        .get::<Name>(victim)
        .unwrap()
        .as_str()
        .to_owned();
    let victim_look = s.server.world().get::<HumanLook>(victim).unwrap().clone();
    let ling_appearance = s.server.world().get::<Appearance>(ling).unwrap().clone();
    let ling_look = s.server.world().get::<HumanLook>(ling).unwrap().clone();

    // Three 15 s stages, ten times faster.
    s.server
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_relative_speed(10.0);
    s.0.without_mood();
    let started = s.server.world().resource::<Time>().elapsed_secs_f64();
    s.trigger(a, "absorb_dna");
    s.wait_chat(a, "Это существо совместимо. Мы должны стоять неподвижно...");
    s.wait_chat(b, "вытягивает жало-хоботок!");
    s.wait_chat(b, "Вы чувствуете острую колющую боль!");
    s.run_until("B is husked", |s| {
        s.server.world().get::<Husk>(victim).is_some()
    });
    let took = s.server.world().resource::<Time>().elapsed_secs_f64() - started;
    assert!((45.0..47.0).contains(&took), "absorbing took {took} s");
    s.wait_chat(a, &format!("We have absorbed {victim_name}."));
    let world = s.server.world();
    assert_eq!(world.get::<Stat>(victim), Some(&Stat::Dead));
    assert_eq!(world.get::<Blood>(victim).unwrap().volume, 0.0);
    assert!(world.get::<HumanLook>(victim).unwrap().husk);
    let c = s.ling(ling);
    let names: Vec<&str> = c.profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(names[1], victim_name);
    assert_eq!((c.absorbed_count, c.true_absorbs, c.can_respec), (2, 1, 1));
    // Absorbing it again is refused (tg checks the stored DNA before the drained one).
    s.trigger(a, "absorb_dna");
    s.wait_chat(a, "Мы уже имеем это ДНК в нашем хранилище!");

    // Transform: the pick list, then B's name and body.
    s.trigger(a, "transform");
    let (list, _) = s.wait_window(a, ListInputUi::NAME);
    let data: ss13_tgui::interfaces::list_input::ListInputData = s.latest_data(a, list);
    assert!(data.items.contains(&victim_name), "{:?}", data.items);
    s.act(a, list, "submit", &[("entry", &victim_name)]);
    s.run_until("A carries B's name", |s| {
        s.server
            .world()
            .get::<Name>(ling)
            .is_some_and(|n| n.as_str() == victim_name)
    });
    let look = s.server.world().get::<HumanLook>(ling).unwrap().clone();
    assert_eq!(
        (
            look.female,
            &look.skin_tone,
            &look.hair_style,
            look.hair_color,
            look.eye_color
        ),
        (
            victim_look.female,
            &victim_look.skin_tone,
            &victim_look.hair_style,
            victim_look.hair_color,
            victim_look.eye_color
        )
    );
    // The body is drawn anew (unless both random bodies happen to be alike).
    let alike = HumanLook {
        uniform: None,
        shoes: None,
        ..ling_look
    } == HumanLook {
        uniform: None,
        shoes: None,
        ..victim_look.clone()
    };
    assert!(!ling_appearance.overlays.is_empty(), "drawn from its look");
    if !alike {
        s.run_until("A is drawn with B's body", |s| {
            s.server.world().get::<Appearance>(ling) != Some(&ling_appearance)
        });
    }
    // Others hear the stolen name.
    s.say(a, "Всё в порядке.");
    s.wait_chat(b, &format!("{victim_name} говорит, \"Всё в порядке.\""));

    // Reviving stasis: dead-looking for 40 s (4 s here), then the revive button.
    s.trigger(a, "fakedeath");
    s.wait_chat(a, "Мы начинаем стазис");
    let world = s.server.world();
    assert_eq!(world.get::<Stat>(ling), Some(&Stat::Dead));
    assert!(world.get::<DeathComa>(ling).is_some());
    s.wait_chat(a, "Мы готовы возродиться.");
    s.run_until("the revive button", |s| {
        s.buttons(a).buttons.iter().any(|b| b.name == "Revive")
    });
    s.server
        .world_mut()
        .entity_mut(ling)
        .get_mut::<Health>()
        .unwrap()
        .adjust(ss13_game::health::DamageType::Brute, 30.0);
    s.trigger(a, "fakedeath");
    s.wait_chat(a, "Мы возродили себя.");
    let world = s.server.world();
    assert_eq!(world.get::<Stat>(ling), Some(&Stat::Conscious));
    assert!(world.get::<DeathComa>(ling).is_none());
    assert_eq!(world.get::<Health>(ling).unwrap().brute, 0.0);
    assert_eq!(s.ling(ling).stasis, Stasis::None);

    // The emporium: buy the arm blade (2 points), grow it, readapt.
    s.trigger(a, "cellular_emporium");
    let (shop, st) = s.wait_window(a, CellularEmporiumUi::NAME);
    let st: EmporiumStatic = ss13_tgui::decode(&st).unwrap();
    let names: Vec<&str> = st.abilities.iter().map(|x| x.name.as_str()).collect();
    for power in [
        "Arm Blade",
        "Chitinous Armor",
        "Fleshmend",
        "Gene Stim",
        "Blind Sting",
    ] {
        assert!(names.contains(&power), "{power} in {names:?}");
    }
    let data: EmporiumData = s.latest_data(a, shop);
    assert_eq!((data.genetic_points_count, data.can_readapt), (10, 1));
    // A forged path buys nothing.
    s.act(
        a,
        shop,
        "evolve",
        &[("path", "/datum/action/changeling/headcrab")],
    );
    s.act(
        a,
        shop,
        "evolve",
        &[("path", "/datum/action/changeling/weapon/arm_blade")],
    );
    s.run_until("8 points left", |s| {
        s.latest_data::<EmporiumData>(a, shop).genetic_points_count == 8
    });
    s.run_until("the blade button", |s| {
        s.buttons(a).buttons.iter().any(|b| b.id == "arm_blade")
    });
    s.run_until("20 chemicals", |s| s.ling(ling).chem_charges >= 20.0);
    s.trigger(a, "arm_blade");
    s.wait_chat(a, "превращаясь в смертоносный клинок");
    let held = |s: &Station| {
        let world = s.server.world();
        world
            .get::<Inventory>(ling)
            .and_then(Inventory::active_item)
            .and_then(|i| world.get::<ss13_mapload::DmItem>(i))
            .map(|d| d.path.clone())
    };
    assert_eq!(held(&s).as_deref(), Some(ARM_BLADE));
    // It cannot be dropped.
    s.send(a, ClientIntent::DropActive);
    s.run_for(0.3);
    assert_eq!(held(&s).as_deref(), Some(ARM_BLADE));
    s.act(a, shop, "readapt", &[]);
    s.wait_chat(a, "готовы к повторной адаптации");
    s.run_until("the points are back", |s| {
        let c = s.ling(ling);
        c.genetic_points == 10 && c.purchased.is_empty() && c.can_respec == 0
    });
    assert_eq!(held(&s), None);
    s.server
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_relative_speed(1.0);

    // An admin ends the round: the report lists the changeling and its genes.
    s.server.world_mut().resource_mut::<Ticker>().force_ending = true;
    s.wait_chat(b, "Генокрады были:");
    s.wait_chat(b, "p0 был ");
    s.wait_chat(b, "Извлеченных генов: 2");
    s.wait_chat(b, "Задача #1: Извлеките ");
    s.wait_chat(b, "Раунд закончился.");
}
