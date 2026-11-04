//! The heretic over real WebTransport on 127.0.0.1: four clients in the lobby, one opts in to
//! heretics; an admin-queued "Roundstart Heretics" ruleset (forced, deterministic) makes it a
//! heretic at round start. It gets the greeting, the "pick a path" objective and the research
//! window; five influences appear on the station, replicated to it and never to the crew; it
//! drains one with a right click (a point; the crew then sees the pierced reality), researches
//! the Path of Ash, re-awakens its living heart on a rune, grasps and marks a crewman, draws a
//! rune with a pen and the grasp in the other hand, sacrifices its target (the objective is
//! done, the victim comes back), researches the path to its end and ascends on three husks:
//! CentCom announces the Ash Lord and the round-end report says so.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::actions::{ActionButtonsData, ActionButtonsUi, TRIGGER};
use ss13_game::antag::dynamic::{DevTraitor, DynamicConfig, HERETIC_RULESET};
use ss13_game::antag::heretic::server::{
    AshMark, Heretic, HereticObjective, MansusFist, RealityTracker, Sacrificed, TransmutationRune,
    VisibleInfluence, spawn_rune,
};
use ss13_game::antag::heretic::{AntagInfoHereticUi, HereticInfo, HereticStatic, ROLE_HERETIC};
use ss13_game::antag::objectives::RoundEnd;
use ss13_game::health::{DamageType, Health, Stat, apply_damage};
use ss13_game::human::{ASSISTANT_SHOES, ASSISTANT_UNIFORM, HAIR_STYLES, HumanLook, parts};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::jobs::fallback_jobs;
use ss13_game::lobby::{
    CHARACTER_SETUP, GameState, JobPrefsUi, LobbyData, LobbyUi, SET_ANTAGS, SET_JOB_PREFERENCE,
    TOGGLE_READY,
};
use ss13_game::round::{RoundConfig, Ticker};
use ss13_game::server::Players;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::antag::{ANTAG_FILE, AntagData};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::JOBS_FILE;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_world::SpatialIndex;

const ASH_START: &str = "/datum/heretic_knowledge/limited_amount/starting/base_ash";
const ASH_TREE: [&str; 7] = [
    "/datum/heretic_knowledge/spell/ash_passage",
    "/datum/heretic_knowledge/spell/fire_blast",
    "/datum/heretic_knowledge/armor/ash",
    "/datum/heretic_knowledge/nightwatchers_lantern",
    "/datum/heretic_knowledge/blade_upgrade/ash",
    "/datum/heretic_knowledge/spell/flame_birth",
    "/datum/heretic_knowledge/ultimate/ash_final",
];

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
    "/obj/effect/heretic_influence": { "parent": "/obj/effect", "declared_vars": {
        "name": "reality smash", "icon": { "resource": "icons/effects/eldritch.dmi" },
        "anchored": 1, "invisibility": 60 } },
    "/obj/effect/visible_heretic_influence": { "parent": "/obj/effect", "declared_vars": {
        "name": "pierced reality", "icon": { "resource": "icons/effects/eldritch.dmi" },
        "icon_state": "pierced_illusion", "anchored": 1 } },
    "/obj/effect/heretic_rune": { "parent": "/obj/effect", "declared_vars": {
        "name": "transmutation rune", "icon": { "resource": "rune.dmi" }, "icon_state": "main1",
        "anchored": 1, "plane": -6 } },
    "/obj/effect/heretic_rune/big": { "parent": "/obj/effect/heretic_rune", "declared_vars": {
        "icon": { "resource": "icons/effects/96x96.dmi" }, "icon_state": "transmutation_rune" } },
    "/obj/effect/decal": { "parent": "/obj/effect" },
    "/obj/effect/decal/cleanable": { "parent": "/obj/effect/decal" },
    "/obj/effect/decal/cleanable/blood": { "parent": "/obj/effect/decal/cleanable",
        "declared_vars": { "name": "blood", "icon": { "resource": "rune.dmi" },
        "icon_state": "main1" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/pen": { "parent": "/obj/item", "declared_vars": { "name": "pen",
        "w_class": 1 } },
    "/obj/item/food": { "parent": "/obj/item" },
    "/obj/item/food/grown": { "parent": "/obj/item/food" },
    "/obj/item/food/grown/flower": { "parent": "/obj/item/food/grown" },
    "/obj/item/food/grown/flower/poppy": { "parent": "/obj/item/food/grown/flower",
        "declared_vars": { "name": "poppy" } },
    "/obj/item/melee": { "parent": "/obj/item" },
    "/obj/item/melee/touch_attack": { "parent": "/obj/item/melee" },
    "/obj/item/melee/touch_attack/mansus_fist": { "parent": "/obj/item/melee/touch_attack",
        "declared_vars": { "name": "Хватка Мансуса", "icon_state": "render" } },
    "/obj/item/melee/sickly_blade": { "parent": "/obj/item/melee", "declared_vars": {
        "name": "sickly blade", "force": 20 } },
    "/obj/item/melee/sickly_blade/ash": { "parent": "/obj/item/melee/sickly_blade",
        "declared_vars": { "name": "ashen blade" } },
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
    "/area": { "parent": "/atom", "declared_vars": { "area_flags": 131, "requires_power": 1 } },
    "/area/space": { "parent": "/area", "declared_vars": { "area_flags": 2,
        "requires_power": 0 } },
    "/area/station": { "parent": "/area" },
    "/area/station/command": { "parent": "/area/station" },
    "/area/station/command/bridge": { "parent": "/area/station/command",
        "vars": { "name": "Bridge" }, "declared_vars": {
        "name": "Bridge", "requires_power": 0 } }
  }
}"##;

/// Assistant starts `s`, latejoin `L`; all of it the bridge, in sight from anywhere.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/command/bridge)
"f" = (/turf/open/floor,/area/station/command/bridge)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/command/bridge)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/station/command/bridge)

(1,1,1) = {"
wwwwwwwwwwwwww
wffffffffffffw
wffffffffffffw
wffffffffffffw
wffffffffffLfw
wffffffffffffw
wffffffffffffw
wssssssssssssw
wwwwwwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "render"),
    ("rune.dmi", "main1"),
    ("icons/effects/eldritch.dmi", "reality_smash"),
    ("icons/effects/eldritch.dmi", "pierced_illusion"),
    ("icons/effects/96x96.dmi", "transmutation_rune"),
];

fn human_states() -> Vec<(String, String)> {
    let mut out = vec![("icons/mob/human/human.dmi".to_owned(), "blank".to_owned())];
    for female in [false, true] {
        for hair in HAIR_STYLES {
            let look = HumanLook {
                female,
                skin_tone: "albino".into(),
                hair_style: Some(hair.into()),
                hair_color: [0, 0, 0],
                eye_color: [0, 0, 0],
                uniform: Some(ASSISTANT_UNIFORM.into()),
                shoes: Some(ASSISTANT_SHOES.into()),
                husk: false,
                missing: 0,
                eyeless: false,
            };
            for part in parts(&look).unwrap() {
                out.push((part.dmi.to_owned(), part.state));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("heretic_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = fallback_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/latejoin",
        "/obj/effect/heretic_influence",
        "/obj/effect/visible_heretic_influence",
        "/obj/effect/heretic_rune/big",
        "/obj/effect/decal/cleanable/blood",
        "/obj/item/pen",
        "/obj/item/food/grown/flower/poppy",
        "/obj/item/melee/touch_attack/mansus_fist",
        "/obj/item/melee/sickly_blade/ash",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station/command/bridge",
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
    let antag = AntagData::default().to_ron();
    write_fixture(
        &dir,
        TYPES,
        &used,
        &states,
        &[("map.dmm", MAP), (JOBS_FILE, &jobs), (ANTAG_FILE, &antag)],
    )
}

#[derive(Resource, Default)]
struct Received {
    windows: Vec<(u32, String, Vec<u8>)>,
    data: Vec<(u32, Vec<u8>)>,
    lobby: Option<(u32, LobbyData)>,
    chat: Vec<String>,
}

fn record(
    mut ui: MessageReader<UiMessageReceived>,
    mut chat: MessageReader<ChatReceived>,
    mut received: ResMut<Received>,
) {
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                static_data,
                data,
                ..
            } => {
                if interface == LobbyUi::NAME {
                    received.lobby = Some((*window, ss13_tgui::decode(data).unwrap()));
                }
                received
                    .windows
                    .push((*window, interface.clone(), static_data.clone()));
                received.data.push((*window, data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some((id, lobby)) = &mut received.lobby
                    && id == window
                {
                    *lobby = ss13_tgui::decode(data).unwrap();
                }
                received.data.push((*window, data.clone()));
            }
            UiServerMessage::Update { .. } => {}
            UiServerMessage::Close { window } => {
                if received.lobby.as_ref().is_some_and(|(id, _)| id == window) {
                    received.lobby = None;
                }
            }
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
        Self(testing::Station::start(
            fixture_map(&assets, &["map.dmm"]),
            |app| {
                app.insert_resource(RoundConfig {
                    lobby_secs: 600.0,
                    round_end_secs: 600.0,
                    ..RoundConfig::tg()
                })
                .insert_resource(DynamicConfig {
                    forced_rulesets: Some(vec![HERETIC_RULESET]),
                    protect_roles: true,
                    seed: Some(7),
                    dev_traitor: DevTraitor::Off,
                })
                .add_plugins(ChatServerPlugin);
            },
        ))
    }

    fn connect(&mut self) -> usize {
        let token = format!("p{}", self.players.len());
        let i = self.0.connect(&token, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        self.run_until("in the lobby", |s| s.received(i).lobby.is_some());
        i
    }

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn lobby_act(&mut self, p: usize, action: &str) {
        let window = self.received(p).lobby.as_ref().expect("in the lobby").0;
        self.act(p, window, action, &[]);
    }

    fn window(&self, p: usize, interface: &str) -> Option<u32> {
        self.received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i, _)| i == interface)
            .map(|(id, ..)| *id)
    }

    fn windows_of(&self, p: usize, interface: &str) -> usize {
        self.received(p)
            .windows
            .iter()
            .filter(|(_, i, _)| i == interface)
            .count()
    }

    fn wait_window(&mut self, p: usize, interface: &str) -> (u32, Vec<u8>) {
        self.run_until(&format!("{interface} opens"), |s| {
            s.received(p).windows.iter().any(|(_, i, _)| i == interface)
        });
        let (id, _, st) = self
            .received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i, _)| i == interface)
            .unwrap();
        (*id, st.clone())
    }

    fn latest_data<T: serde::de::DeserializeOwned>(&self, p: usize, window: u32) -> T {
        let (_, data) = self
            .received(p)
            .data
            .iter()
            .rev()
            .find(|(w, _)| *w == window)
            .unwrap();
        ss13_tgui::decode(data).unwrap()
    }

    fn wait_chat(&mut self, p: usize, part: &str) {
        self.run_until(&format!("p{p} reads {part:?}"), |s| {
            s.received(p).chat.iter().any(|n| n.contains(part))
        });
    }

    fn mob(&self, p: usize) -> Entity {
        self.server
            .world()
            .resource::<Players>()
            .mob(self.players[p].link)
            .unwrap()
    }

    fn heretic(&self, mob: Entity) -> &Heretic {
        self.server.world().get::<Heretic>(mob).unwrap()
    }

    fn heretic_mut(&mut self, mob: Entity) -> Mut<'_, Heretic> {
        self.server.world_mut().get_mut::<Heretic>(mob).unwrap()
    }

    fn put(&mut self, mob: Entity, pos: TilePos) {
        self.server.world_mut().entity_mut(mob).insert(pos);
    }

    fn trigger(&mut self, p: usize, id: &str) {
        let window = self.window(p, ActionButtonsUi::NAME).expect("buttons");
        self.act(p, window, TRIGGER, &[("id", id)]);
    }

    fn sees(&mut self, p: usize, e: Entity) {
        self.run_until("seen", |s| s.net_id(p, e).is_some());
    }

    fn research(&mut self, p: usize, path: &str, category: &str) {
        let window = self
            .window(p, AntagInfoHereticUi::NAME)
            .expect("research window");
        self.act(
            p,
            window,
            "research",
            &[("path", path), ("category", category)],
        );
    }

    /// Clicks the rune and picks `ritual` from the list.
    fn ritual(&mut self, p: usize, rune: Entity, ritual: &str) {
        let before = self.windows_of(p, ListInputUi::NAME);
        self.sees(p, rune);
        self.click(p, rune);
        self.run_until("the ritual list opens", |s| {
            s.windows_of(p, ListInputUi::NAME) > before
        });
        let list = self.window(p, ListInputUi::NAME).unwrap();
        let data: ListInputData = self.latest_data(p, list);
        assert!(
            data.items.iter().any(|i| i == ritual),
            "{ritual} in {:?}",
            data.items
        );
        self.act(p, list, "submit", &[("entry", ritual)]);
    }

    fn spawn(&mut self, path: &str, pos: TilePos) -> Entity {
        ss13_game::antag::heretic::server::spawn_obj_or_rune(self.server.world_mut(), path, pos)
            .unwrap_or_else(|| panic!("{path} spawns"))
    }

    fn exists(&self, e: Entity) -> bool {
        self.server.world().get_entity(e).is_ok()
    }

    fn crit(&mut self, mob: Entity, amount: f32) {
        let world = self.server.world_mut();
        let mut health = world.get::<Health>(mob).unwrap().clone();
        let mut stat = *world.get::<Stat>(mob).unwrap();
        apply_damage(&mut health, &mut stat, DamageType::Brute, amount);
        world.entity_mut(mob).insert((health, stat));
    }

    fn info(&self, p: usize) -> HereticInfo {
        let window = self.window(p, AntagInfoHereticUi::NAME).unwrap();
        self.latest_data(p, window)
    }
}

#[test]
fn heretic_drains_researches_sacrifices_and_ascends() {
    let mut s = Station::start("round");
    let players: Vec<usize> = (0..4).map(|_| s.connect()).collect();
    let h = players[0];
    for &p in &players {
        s.lobby_act(p, CHARACTER_SETUP);
        let (prefs, _) = s.wait_window(p, JobPrefsUi::NAME);
        s.act(
            p,
            prefs,
            SET_JOB_PREFERENCE,
            &[("job", "Assistant"), ("level", "3")],
        );
        if p == h {
            s.act(
                p,
                prefs,
                SET_ANTAGS,
                &[("antags", ROLE_HERETIC), ("toggled", "1")],
            );
        }
    }
    s.run_until("preferences stored", |s| {
        s.server
            .world()
            .get::<ss13_game::antag::dynamic::AntagPreferences>(s.players[h].link)
            .is_some_and(|prefs| prefs.wants(ROLE_HERETIC))
            && players.iter().all(|&p| {
                s.server
                    .world()
                    .get::<ss13_game::round::JobPreferences>(s.players[p].link)
                    .is_some_and(|prefs| !prefs.levels.is_empty())
            })
    });
    for &p in &players {
        s.lobby_act(p, TOGGLE_READY);
    }
    s.run_until("all ready", |s| {
        players
            .iter()
            .all(|&p| s.received(p).lobby.as_ref().is_some_and(|(_, l)| l.ready))
    });
    {
        let world = s.server.world_mut();
        world.resource_mut::<RoundConfig>().lobby_secs = 0.5;
        world.insert_resource(Ticker::default());
    }
    s.run_until("the round starts", |s| {
        s.server.world().resource::<Ticker>().state() >= GameState::Playing
    });

    // The opted-in player is the only heretic: greeting, objective, window, buttons.
    let mobs: Vec<Entity> = players.iter().map(|&p| s.mob(p)).collect();
    let m = mobs[0];
    for (i, &mob) in mobs.iter().enumerate() {
        assert_eq!(
            s.server.world().get::<Heretic>(mob).is_some(),
            i == 0,
            "player {i}"
        );
    }
    s.wait_chat(h, "Вы - Еретик.");
    s.wait_chat(h, "Задача #1: Pick a path to pursue.");
    let (_, st) = s.wait_window(h, AntagInfoHereticUi::NAME);
    let paths: HereticStatic = ss13_tgui::decode(&st).unwrap();
    assert_eq!(paths.paths.len(), 8);
    let ash = paths
        .paths
        .iter()
        .find(|p| p.route == "Путь Пепла")
        .unwrap();
    assert_eq!(ash.starting_knowledge.path, ASH_START);
    assert_eq!(ash.preview_abilities.len(), 4);
    assert_eq!(s.info(h).charges, 1);
    assert_eq!(s.heretic(m).researched.len(), 4, "the start knowledge");
    s.wait_window(h, ActionButtonsUi::NAME);
    let buttons: ActionButtonsData = {
        let w = s.window(h, ActionButtonsUi::NAME).unwrap();
        s.latest_data(h, w)
    };
    let ids: Vec<&str> = buttons.buttons.iter().map(|b| b.id.as_str()).collect();
    assert!(
        ids.contains(&"heretic_spell:/datum/heretic_knowledge/spell/basic"),
        "{ids:?}"
    );
    assert!(ids.contains(&"heretic_living_heart"), "{ids:?}");

    // Five influences on the station: the heretic's client gets them, the crew's never.
    let smashes = s
        .server
        .world()
        .resource::<RealityTracker>()
        .smashes
        .clone();
    assert_eq!(smashes.len(), 5);
    s.run_until("the heretic sees an influence", |s| {
        smashes.iter().any(|&e| s.net_id(h, e).is_some())
    });
    s.hold(1.0, "the crew never receives an influence", |s| {
        players[1..]
            .iter()
            .all(|&p| smashes.iter().all(|&e| s.net_id(p, e).is_none()))
    });

    // A right click with an empty hand drains one: a point, a pierced reality for everyone.
    let influence = smashes[0];
    let at = *s.get::<TilePos>(influence);
    let next_to = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .map(|(dx, dy)| TilePos::new(at.x + dx, at.y + dy, at.z))
        .find(|p| {
            let world = s.server.world();
            ss13_game::construction::turf_at(world, *p).is_some_and(|t| {
                world
                    .get::<ss13_mapload::DmTurfBuild>(t)
                    .is_some_and(|b| b.path == "/turf/open/floor")
            }) && world
                .resource::<SpatialIndex>()
                .dense_on_tile(*p)
                .is_empty()
        })
        .expect("a floor next to the influence");
    s.put(m, next_to);
    s.sees(h, influence);
    s.click_with(
        h,
        influence,
        ClickModifiers {
            right: true,
            ..Default::default()
        },
    );
    s.run_until("drained", |s| !s.exists(influence));
    assert_eq!(s.heretic(m).points, 2);
    s.wait_chat(h, "влияние добыто");
    assert_eq!(s.server.world().resource::<RealityTracker>().num_drained, 1);
    let pierced = {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(at)
            .iter()
            .copied()
            .find(|&e| world.get::<VisibleInfluence>(e).is_some())
            .expect("a pierced reality")
    };
    s.sees(players[1], pierced);

    // The Path of Ash: the start shop's knowledge, its objectives.
    s.research(h, ASH_START, "start");
    s.run_until("the path is taken", |s| {
        s.heretic(m).route == Some("Путь Пепла")
    });
    assert_eq!(s.heretic(m).points, 1);
    s.wait_chat(h, "Задача #1: Изучите как минимум");
    s.wait_chat(
        h,
        "Задача #2: Принесите в жертву не менее 5 членов экипажа.",
    );
    s.run_until("the window shows the ash tier", |s| {
        s.info(h)
            .knowledge_tiers
            .iter()
            .flatten()
            .any(|n| n.path == ASH_TREE[0] && !n.done)
    });
    // Too poor for Ash Passage (2): refused.
    s.research(h, ASH_TREE[0], "tree");
    s.hold(0.5, "not bought", |s| !s.heretic(m).has(ASH_TREE[0]));

    // The living heart, lost, is woken on a rune with a pool of blood and a poppy.
    let rune_at = TilePos::new(6, 5, at.z);
    let rune = spawn_rune(s.server.world_mut(), rune_at, Some("Путь Пепла")).unwrap();
    s.put(m, TilePos::new(5, 5, at.z));
    let blood = s.spawn("/obj/effect/decal/cleanable/blood", rune_at);
    let poppy = s.spawn(
        "/obj/item/food/grown/flower/poppy",
        TilePos::new(7, 5, at.z),
    );
    s.heretic_mut(m).living_heart = false;
    s.ritual(h, rune, "Живое сердце");
    s.run_until("the heart beats again", |s| s.heretic(m).living_heart);
    assert!(
        !s.exists(blood) && !s.exists(poppy),
        "the ingredients are used"
    );
    assert!(
        !s.server
            .world()
            .get::<TransmutationRune>(rune)
            .unwrap()
            .in_use
    );

    // The grasp: a hand, then a crewman hit is knocked down and marked.
    let victim = mobs[2];
    s.put(victim, TilePos::new(5, 4, at.z));
    s.trigger(h, "heretic_spell:/datum/heretic_knowledge/spell/basic");
    s.run_until("the grasp is in hand", |s| {
        let world = s.server.world();
        world
            .get::<Inventory>(m)
            .unwrap()
            .hands
            .iter()
            .flatten()
            .any(|&e| world.get::<MansusFist>(e).is_some())
    });
    s.sees(h, victim);
    s.click(h, victim);
    s.run_until("the crewman is marked", |s| {
        s.server.world().get::<AshMark>(victim).is_some()
    });
    s.wait_chat(players[2], "Яркий зеленый свет ужасно жжет глаза!");
    assert_eq!(
        s.heretic(m)
            .get("/datum/heretic_knowledge/spell/basic")
            .unwrap()
            .charges,
        7
    );

    // A pen in the active hand and the grasp in the other draw a rune on a floor.
    s.run_for(10.5);
    s.trigger(h, "heretic_spell:/datum/heretic_knowledge/spell/basic");
    s.run_until("the grasp is back", |s| {
        let world = s.server.world();
        let inv = world.get::<Inventory>(m).unwrap();
        inv.hands[inv.active].is_some_and(|e| world.get::<MansusFist>(e).is_some())
    });
    s.send(h, ClientIntent::SwapHands);
    s.run_until("swapped", |s| {
        let world = s.server.world();
        let inv = world.get::<Inventory>(m).unwrap();
        inv.hands[inv.active].is_none()
    });
    let pen = s.spawn("/obj/item/pen", TilePos::new(5, 5, at.z));
    {
        let world = s.server.world_mut();
        world
            .entity_mut(pen)
            .remove::<TilePos>()
            .insert(CarriedBy(m));
        let mut inv = world.get_mut::<Inventory>(m).unwrap();
        let active = inv.active;
        inv.hands[active] = Some(pen);
    }
    let floor_at = TilePos::new(10, 3, at.z);
    let floor = ss13_game::construction::turf_at(s.server.world(), floor_at).unwrap();
    s.put(m, TilePos::new(10, 4, at.z));
    s.sees(h, floor);
    s.click(h, floor);
    s.wait_chat(h, "создание руны...");
    s.run_until_for(25.0, "the rune is drawn", |s| {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(floor_at)
            .iter()
            .any(|&e| world.get::<TransmutationRune>(e).is_some())
    });
    s.wait_chat(h, "руна создана");
    s.send(h, ClientIntent::DropActive);
    s.run_until("the pen is down", |s| {
        let world = s.server.world();
        let inv = world.get::<Inventory>(m).unwrap();
        inv.hands[inv.active].is_none()
    });

    // The sacrifice: the target in crit on the rune; the objective is done, the victim returns.
    let target = mobs[1];
    s.heretic_mut(m).sac_targets = vec![target];
    s.heretic_mut(m).objectives = vec![HereticObjective::MinorSacrifice { target: 1 }];
    s.put(m, TilePos::new(5, 5, at.z));
    // Around the rune, not on it: a body on the rune's tile blocks the reach (`index.adjacent`).
    s.put(target, TilePos::new(7, 5, at.z));
    s.crit(target, 120.0);
    assert!(*s.get::<Stat>(target) >= Stat::SoftCrit);
    let points = s.heretic(m).points;
    s.ritual(h, rune, "Сердцебиение Мансуса");
    s.run_until("sacrificed", |s| s.heretic(m).total_sacrifices == 1);
    assert_eq!(s.heretic(m).points, points + 2);
    assert!(s.heretic(m).sac_targets.is_empty());
    assert!(s.server.world().get::<Sacrificed>(target).is_some());
    s.wait_chat(h, "Твои покровители принимают твоё подношение.");
    s.run_until("the objective shows done", |s| {
        s.info(h).objectives.iter().all(|o| o.complete)
    });
    s.run_until_for(15.0, "the victim comes back", |s| {
        s.server.world().get::<Sacrificed>(target).is_none()
    });
    assert_ne!(*s.get::<Stat>(target), Stat::Dead);
    s.wait_chat(h, "была возвращена на станцию");

    // The whole path, the final spell confirmed, then the ascension's knowledge.
    s.heretic_mut(m).points = 20;
    for path in ASH_TREE {
        let category = "tree";
        s.research(h, path, category);
        if path == "/datum/heretic_knowledge/spell/flame_birth" {
            let before = s.windows_of(h, ListInputUi::NAME);
            s.run_until("the final spell asks", |s| {
                s.windows_of(h, ListInputUi::NAME) > before
            });
            let list = s.window(h, ListInputUi::NAME).unwrap();
            s.act(h, list, "submit", &[("entry", "Да")]);
        }
        s.run_until(&format!("{path} researched"), |s| s.heretic(m).has(path));
    }
    assert!(s.heretic(m).unlimited_blades);

    // Three husks on the rune: the Ash Lord rises.
    for &body in &mobs[1..4] {
        let world = s.server.world_mut();
        world.entity_mut(body).insert(Stat::Dead);
        world.get_mut::<HumanLook>(body).unwrap().husk = true;
    }
    s.put(mobs[1], TilePos::new(7, 4, at.z));
    s.put(mobs[2], TilePos::new(7, 6, at.z));
    s.put(mobs[3], TilePos::new(6, 6, at.z));
    s.ritual(h, rune, "Ритуал Пепельного Лорда");
    s.run_until("ascended", |s| s.heretic(m).ascended);
    s.wait_chat(h, "Бойтесь пламени, ибо Пепельный Лорд");
    assert!(
        mobs[1..4].iter().all(|&b| !s.exists(b)),
        "the husks are consumed"
    );
    let report = ss13_game::antag::heretic::server::roundend_report(
        s.server.world_mut(),
        RoundEnd {
            force_ending: false,
        },
    );
    let text: Vec<String> = report.iter().map(|m| m.plain_text()).collect();
    assert!(
        text.iter().any(|l| l.contains("ЕРЕТИК ВОЗНЕССЯ!")),
        "{text:?}"
    );
    assert!(
        text.iter()
            .any(|l| l.contains("Жертвоприношений сделано: 1")),
        "{text:?}"
    );
}
