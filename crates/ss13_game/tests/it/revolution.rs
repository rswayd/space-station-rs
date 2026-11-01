//! The revolution and dynamic's midround and latejoin rulesets over real WebTransport on
//! 127.0.0.1.
//!
//! - Dynamic: a lobby round with no roundstart rulesets; an admin forces the midround traitor
//!   ruleset and the one crew member who opted in to sleeper agent becomes a traitor; a player who
//!   opted in to infiltrator latejoins while a latejoin traitor ruleset is queued and becomes one.
//! - The revolution: a head revolutionary (admin-made) gets a flash, flashes a crew member who
//!   becomes a revolutionary (both antag HUDs show each other with their own icons); the captain
//!   implants a mindshield into the revolutionary (deconverted, and a second flash fails); the
//!   captain dies and the revolution wins (BandaStation: the round goes on), the round-end report
//!   lists the revolution.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::antag::dynamic::{DevTraitor, DynamicConfig};
use ss13_game::antag::midround::{LATEJOIN_TRAITOR, MIDROUND_TRAITOR, Midround, force_midround};
use ss13_game::antag::objectives::RoundEnd;
use ss13_game::antag::revolution::server::{
    MINDSHIELD_IMPLANTER, Mindshield, REV_FLASH, RevResult, RevolutionHandler, Revolutionary,
    add_revolutionary, roundend_report,
};
use ss13_game::antag::traitor::Traitor;
use ss13_game::antag::{AntagHudData, AntagHudUi, ROLE_SLEEPER_AGENT, ROLE_SYNDICATE_INFILTRATOR};
use ss13_game::health::{DamageType, Health, Stat, apply_damage};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::jobs::{Jobs, PlayerJob, fallback_jobs};
use ss13_game::lobby::{
    CHARACTER_SETUP, GameState, JobPrefsUi, LobbyData, LobbyUi, SET_ANTAGS, SET_JOB_PREFERENCE,
    TOGGLE_READY,
};
use ss13_game::round::{RoundConfig, Ticker};
use ss13_game::server::Players;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, JobsData};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::job_selection::{JobSelectionUi, SELECT_JOB};

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
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/assembly": { "parent": "/obj/item", "declared_vars": { "w_class": 2 } },
    "/obj/item/assembly/flash": { "parent": "/obj/item/assembly", "declared_vars": {
        "name": "flash" } },
    "/obj/item/assembly/flash/handheld": { "parent": "/obj/item/assembly/flash" },
    "/obj/item/implanter": { "parent": "/obj/item", "declared_vars": {
        "name": "implanter" } },
    "/obj/item/implanter/mindshield": { "parent": "/obj/item/implanter", "declared_vars": {
        "name": "implanter (mindshield)" } },
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

/// Assistant starts `s`, latejoin `L`.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wffffffffw
wsffsfffLw
wffffffsfw
wwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 7] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("icons/mob/huds/antag_hud.dmi", "rev"),
    ("icons/mob/huds/antag_hud.dmi", "rev_head"),
];

/// The assistant and a captain (a head of staff with a mindshield).
fn fixture_jobs() -> JobsData {
    let mut data = fallback_jobs();
    data.jobs[0].total_positions = 10;
    data.jobs[0].spawn_positions = 10;
    let mut captain = data.jobs[0].clone();
    captain.title = "Captain".into();
    captain.path = "/datum/job/captain".into();
    captain.total_positions = 1;
    captain.spawn_positions = 1;
    data.jobs.push(captain);
    data.departments[0].jobs.push(1);
    data
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("revolution_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = fixture_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/latejoin",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        REV_FLASH,
        MINDSHIELD_IMPLANTER,
    ];
    used.extend(
        data.outfit_items()
            .into_iter()
            .filter(|p| types.types.contains_key(*p)),
    );
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    std::fs::write(dir.join(JOBS_FILE), data.to_ron()).unwrap();
    dir
}

/// What a client received: windows opened (id, interface, static data), the latest data per
/// window, the lobby, chat lines.
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

    fn context(&self) -> String {
        let chat: Vec<_> = self
            .players
            .iter()
            .map(|p| &p.app.world().resource::<Received>().chat)
            .collect();
        format!("; chat {chat:?}")
    }
}

impl Station {
    fn start(test: &str, round: RoundConfig) -> Self {
        let assets = fixture_assets(test);
        Self(testing::Station::start(
            fixture_map(&assets, &["map.dmm"]),
            |app| {
                app.insert_resource(round)
                    // An admin queued no roundstart ruleset.
                    .insert_resource(DynamicConfig {
                        forced_rulesets: Some(Vec::new()),
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
        self.0.connect(&token, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        })
    }

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn lobby_act(&mut self, p: usize, action: &str) {
        let window = self.received(p).lobby.as_ref().expect("in the lobby").0;
        self.act(p, window, action, &[]);
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

    fn wait_chat(&mut self, p: usize, part: &str) {
        self.run_until(&format!("a chat line with {part:?}"), |s| {
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

    /// The jobs page: assistant, and the antagonist roles.
    fn set_prefs(&mut self, p: usize, antags: &[&str]) {
        self.lobby_act(p, CHARACTER_SETUP);
        let (prefs, _) = self.wait_window(p, JobPrefsUi::NAME);
        self.act(
            p,
            prefs,
            SET_JOB_PREFERENCE,
            &[("job", "Assistant"), ("level", "3")],
        );
        for role in antags {
            self.act(p, prefs, SET_ANTAGS, &[("antags", role), ("toggled", "1")]);
        }
        let link = self.players[p].link;
        let roles: Vec<String> = antags.iter().map(|r| (*r).to_owned()).collect();
        self.run_until("preferences stored", move |s| {
            let world = s.server.world();
            world
                .get::<ss13_game::antag::dynamic::AntagPreferences>(link)
                .is_some_and(|p| roles.iter().all(|r| p.wants(r)))
                && world
                    .get::<ss13_game::round::JobPreferences>(link)
                    .is_some_and(|prefs| !prefs.levels.is_empty())
        });
    }

    fn hud(&self, p: usize) -> Option<AntagHudData> {
        let (window, ..) = self
            .received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i, _)| i == AntagHudUi::NAME)?;
        let (_, data) = self
            .received(p)
            .data
            .iter()
            .rev()
            .find(|(w, _)| w == window)?;
        ss13_tgui::decode(data).ok()
    }

    /// `item` into `mob`'s active hand.
    fn hold(&mut self, mob: Entity, item: Entity) {
        let world = self.server.world_mut();
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        for pocket in &mut inventory.pockets {
            if *pocket == Some(item) {
                *pocket = None;
            }
        }
        let active = inventory.active;
        inventory.hands[active] = Some(item);
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
    }

    fn click(&mut self, p: usize, target: Entity) {
        self.run_until("the target is replicated", |s| {
            s.net_id(p, target).is_some()
        });
        let target = self.net_id(p, target).unwrap();
        self.send(
            p,
            ClientIntent::Click {
                target,
                modifiers: ClickModifiers::default(),
            },
        );
    }
}

#[test]
fn dynamic_spawns_midround_and_latejoin_traitors() {
    let mut s = Station::start(
        "dynamic",
        RoundConfig {
            lobby_secs: 600.0,
            round_end_secs: 600.0,
            ..RoundConfig::tg()
        },
    );
    let (a, b) = (s.connect(), s.connect());
    s.run_until("in the lobby", |s| {
        [a, b].iter().all(|&p| s.received(p).lobby.is_some())
    });
    s.set_prefs(a, &[ROLE_SLEEPER_AGENT]);
    s.set_prefs(b, &[]);
    for p in [a, b] {
        s.lobby_act(p, TOGGLE_READY);
    }
    s.run_until("everyone is ready", |s| {
        [a, b]
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
    let (mob_a, mob_b) = (s.mob(a), s.mob(b));
    assert!(s.server.world().resource::<Midround>().started);
    assert!(s.server.world().get::<Traitor>(mob_a).is_none());

    // The admin forces a midround traitor: only A opted in to sleeper agent.
    let picked = force_midround(s.server.world_mut(), MIDROUND_TRAITOR).unwrap();
    assert_eq!(picked, [mob_a]);
    assert!(s.server.world().get::<Traitor>(mob_a).is_some());
    assert!(s.server.world().get::<Traitor>(mob_b).is_none());
    assert!(
        s.server
            .world()
            .resource::<Midround>()
            .executed
            .contains(&MIDROUND_TRAITOR)
    );
    // Nobody left who wants it: the next forced one fails.
    assert!(force_midround(s.server.world_mut(), MIDROUND_TRAITOR).is_err());

    // C latejoins with a queued latejoin traitor ruleset and opted in to infiltrator.
    let c = s.connect();
    s.run_until("C in the lobby", |s| s.received(c).lobby.is_some());
    s.set_prefs(c, &[ROLE_SYNDICATE_INFILTRATOR]);
    s.server
        .world_mut()
        .resource_mut::<Midround>()
        .queued_latejoin
        .push(LATEJOIN_TRAITOR);
    s.send(c, ClientIntent::LateJoinMenu);
    let (menu, _) = s.wait_window(c, JobSelectionUi::NAME);
    s.act(c, menu, SELECT_JOB, &[("job", "Assistant")]);
    let link = s.players[c].link;
    s.run_until("C latejoins as a traitor", move |s| {
        let world = s.server.world();
        world
            .resource::<Players>()
            .mob(link)
            .is_some_and(|m| world.get::<Traitor>(m).is_some())
    });
    assert!(
        s.server
            .world()
            .resource::<Midround>()
            .queued_latejoin
            .is_empty()
    );
}

#[test]
fn a_head_revolutionary_converts_and_the_revolution_wins() {
    let mut s = Station::start("revolution", RoundConfig::skip_lobby());
    for token in ["p0", "p1", "p2"] {
        s.join(token, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
    }
    let (head, crew, captain) = (s.mob(0), s.mob(1), s.mob(2));
    {
        let world = s.server.world_mut();
        let captain_job = world.resource::<Jobs>().data.job("Captain").unwrap();
        world.entity_mut(captain).insert(PlayerJob(captain_job));
        world.entity_mut(head).insert(TilePos::new(4, 3, 1));
        world.entity_mut(crew).insert(TilePos::new(5, 3, 1));
        world.entity_mut(captain).insert(TilePos::new(6, 3, 1));
        assert!(add_revolutionary(world, head, true));
        // A head of staff cannot be one.
        assert!(!add_revolutionary(world, captain, false));
    }
    s.wait_chat(0, "Вы - Глава Революционеров.");
    let world = s.server.world();
    assert_eq!(world.resource::<RevolutionHandler>().targets, [captain]);
    let flash = world
        .get::<Inventory>(head)
        .unwrap()
        .pockets
        .iter()
        .flatten()
        .copied()
        .find(|&e| world.get::<DmItem>(e).is_some_and(|d| d.path == REV_FLASH))
        .expect("a flash in a pocket");
    s.hold(head, flash);

    // The flash converts the crew member.
    s.click(0, crew);
    s.run_until("the crew member is a revolutionary", |s| {
        s.server
            .world()
            .get::<Revolutionary>(crew)
            .is_some_and(|r| !r.head)
    });
    s.wait_chat(1, "Вы - Революционер.");
    let crew_on_head = s.net_id(0, crew).unwrap();
    s.run_until("the head's HUD shows the revolutionary", |s| {
        s.hud(0)
            .is_some_and(|h| h.team_icons.iter().any(|(id, _)| *id == crew_on_head))
    });
    s.run_until("the head is replicated to the crew member", |s| {
        s.net_id(1, head).is_some()
    });
    let head_on_crew = s.net_id(1, head).unwrap();
    s.run_until("the revolutionary's HUD shows the head", |s| {
        s.hud(1).is_some_and(|h| {
            h.hud_icon.is_some()
                && h.team_icons
                    .iter()
                    .any(|(id, icon)| *id == head_on_crew && Some(*icon) != h.hud_icon)
        })
    });

    // The captain implants a mindshield: deconverted, and a flash no longer works.
    let implanter = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        MINDSHIELD_IMPLANTER,
        TilePos::new(6, 3, 1),
    )
    .expect("an implanter");
    s.hold(captain, implanter);
    s.click(2, crew);
    s.run_until("the revolutionary is deconverted", |s| {
        let world = s.server.world();
        world.get::<Revolutionary>(crew).is_none() && world.get::<Mindshield>(crew).is_some()
    });
    s.wait_chat(1, "Вы больше не революционер");
    s.run_until("the head's HUD drops the revolutionary", |s| {
        s.hud(0).is_some_and(|h| h.team_icons.is_empty())
    });
    // Let the flash recharge and the stun pass, then flash again: refused.
    s.run_for(12.0);
    s.click(0, crew);
    s.wait_chat(0, "кажется устойчивым к вспышке");
    assert!(s.server.world().get::<Revolutionary>(crew).is_none());

    // The captain dies: every mutiny target is dead, the revolution wins; the round goes on.
    {
        let world = s.server.world_mut();
        let mut health = world.get::<Health>(captain).unwrap().clone();
        let mut stat = *world.get::<Stat>(captain).unwrap();
        apply_damage(&mut health, &mut stat, DamageType::Brute, 500.0);
        world.entity_mut(captain).insert((health, stat));
    }
    s.run_until("the revolution wins", |s| {
        s.server.world().resource::<RevolutionHandler>().result == Some(RevResult::Revolution)
    });
    s.wait_chat(1, "Похоже, восстание одержало победу");
    s.run_for(1.0);
    assert_eq!(
        s.server.world().resource::<Ticker>().state(),
        GameState::Playing
    );
    let report = roundend_report(s.server.world_mut(), RoundEnd::default());
    let text: Vec<String> = report.iter().map(|m| m.plain_text()).collect();
    assert!(
        text.iter().any(|t| t == "Главными революционерами были:"),
        "{text:?}"
    );
    assert!(text.iter().any(|t| t == "Командованием станции были:"));
}
