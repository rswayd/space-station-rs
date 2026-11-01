//! Blood brothers over real WebTransport on 127.0.0.1: three clients in the lobby, one opts in
//! to blood brother; an admin-queued brother ruleset (forced, deterministic) makes them a brother
//! at round start with a team of their own, shared objectives (convert a brother first) and a
//! flash in the pocket. The brother flashes a second player, who joins the team: both get the
//! greeting and the `AntagInfoBrother` panel with the same objectives, each one's antag HUD lists
//! the other (the bystander has none), the flash burns out, and the round-end report lists the
//! team with its objectives.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::antag::brother::{BROTHER_FLASH, Brother, BrotherTeam};
use ss13_game::antag::dynamic::{BROTHER_RULESET, DevTraitor, DynamicConfig};
use ss13_game::antag::objectives::ObjectiveKind;
use ss13_game::antag::{AntagHudData, AntagHudUi, AntagInfoBrotherUi, BrotherInfo, ROLE_BROTHER};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::jobs::{Jobs, PlayerJob, fallback_jobs};
use ss13_game::lobby::{
    CHARACTER_SETUP, GameState, JobPrefsUi, LobbyData, LobbyUi, SET_ANTAGS, SET_JOB_PREFERENCE,
    TOGGLE_READY,
};
use ss13_game::round::{RoundConfig, Ticker};
use ss13_game::security::Flash;
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

/// Three assistant starts `s`, latejoin `L`.
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

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("icons/mob/huds/antag_hud.dmi", "brother"),
];

fn fixture_jobs() -> JobsData {
    let mut data = fallback_jobs();
    data.jobs[0].total_positions = 10;
    data.jobs[0].spawn_positions = 10;
    data
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("brothers_fixture")
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
        BROTHER_FLASH,
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
                // An admin queued one brother ruleset: no tier roll.
                .insert_resource(DynamicConfig {
                    forced_rulesets: Some(vec![BROTHER_RULESET]),
                    protect_roles: true,
                    seed: Some(7),
                    dev_traitor: DevTraitor::Off,
                })
                .add_plugins(ChatServerPlugin);
            },
        ))
    }

    /// An admin's "start now": the fixture lobby counts down for 10 minutes of game time (the
    /// clients gather on real time, so a short countdown could run out before they are ready),
    /// and once they are, the ticker restarts with a half-second countdown.
    fn start_now(&mut self) {
        let world = self.server.world_mut();
        world.resource_mut::<RoundConfig>().lobby_secs = 0.5;
        world.insert_resource(Ticker::default());
    }

    /// A new client, in the lobby once its handshake is accepted.
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
        self.run_until(&format!("a chat line with {part:?}"), |s| {
            s.received(p).chat.iter().any(|n| n.contains(part))
        });
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

    fn hud(&self, p: usize) -> Option<AntagHudData> {
        let (window, ..) = self
            .received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i, _)| i == AntagHudUi::NAME)?;
        Some(self.latest_data(p, *window))
    }

    fn pocket_flash(&self, p: usize) -> Option<Entity> {
        let world = self.server.world();
        let inventory = world.get::<Inventory>(self.mob(p)?)?;
        inventory.pockets.iter().flatten().copied().find(|&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|dm| dm.path == BROTHER_FLASH)
        })
    }
}

#[test]
fn a_brother_converts_a_crew_member_with_the_flash() {
    let mut s = Station::start("convert");
    let (a, b, c) = (s.connect(), s.connect(), s.connect());
    for p in [a, b, c] {
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
                &[("antags", ROLE_BROTHER), ("toggled", "1")],
            );
        }
    }
    s.run_until("preferences stored", |s| {
        let world = s.server.world();
        world
            .get::<ss13_game::antag::dynamic::AntagPreferences>(s.players[a].link)
            .is_some_and(|p| p.wants(ROLE_BROTHER))
            && [a, b, c].iter().all(|&p| {
                world
                    .get::<ss13_game::round::JobPreferences>(s.players[p].link)
                    .is_some_and(|prefs| !prefs.levels.is_empty())
            })
    });
    for p in [a, b, c] {
        s.lobby_act(p, TOGGLE_READY);
    }
    s.run_until("everyone is ready", |s| {
        [a, b, c]
            .iter()
            .all(|&p| s.received(p).lobby.as_ref().is_some_and(|(_, l)| l.ready))
    });
    s.start_now();
    s.run_until("the round starts", |s| {
        s.server.world().resource::<Ticker>().state() >= GameState::Playing
    });

    // A is a brother with a team of one; the others are not.
    let (mob_a, mut mob_b, mut mob_c) = (s.mob(a).unwrap(), s.mob(b).unwrap(), s.mob(c).unwrap());
    // B is the one A converts: a crew member who is not the target of a team objective (a
    // target is refused).
    let objectives = s
        .server
        .world()
        .get::<Brother>(mob_a)
        .unwrap()
        .objectives
        .clone();
    let targeted = |mob: Entity| {
        objectives.iter().any(|o| match o.kind {
            ObjectiveKind::Assassinate { target } | ObjectiveKind::Maroon { target } => {
                target == Some(mob)
            }
            _ => false,
        })
    };
    let (b, c) = if targeted(mob_b) {
        std::mem::swap(&mut mob_b, &mut mob_c);
        (c, b)
    } else {
        (b, c)
    };
    assert!(
        !targeted(mob_b),
        "both crew members are targets: {objectives:?}"
    );
    assert_eq!(s.job(a), "Assistant");
    let team = s
        .server
        .world()
        .get::<Brother>(mob_a)
        .expect("a brother")
        .team;
    assert!(s.server.world().get::<Brother>(mob_b).is_none());
    assert!(s.server.world().get::<Brother>(mob_c).is_none());
    let left = s
        .server
        .world()
        .get::<BrotherTeam>(team)
        .unwrap()
        .brothers_left;
    assert!((1..=2).contains(&left), "{left}");
    s.wait_chat(a, "Вы - кровный брат.");
    s.wait_chat(a, "Задача #1: Пробудите цель без защиты разума");
    let (_, info) = s.wait_window(a, AntagInfoBrotherUi::NAME);
    let info_a: BrotherInfo = ss13_tgui::decode(&info).unwrap();
    assert_eq!(info_a.antag_name, "Brother");
    assert!(info_a.objectives[0].explanation.starts_with("Пробудите"));
    assert!(info_a.objectives.len() >= 3, "{:?}", info_a.objectives);
    s.run_until("a's HUD shows the brother icon", |s| {
        s.hud(a)
            .is_some_and(|h| h.hud_icon.is_some() && h.team.is_empty())
    });

    // The flash is in a pocket; A takes it in hand and stands next to B.
    let flash = s.pocket_flash(a).expect("a flash in a pocket");
    {
        let world = s.server.world_mut();
        let mut inventory = world.get_mut::<Inventory>(mob_a).unwrap();
        for pocket in &mut inventory.pockets {
            if *pocket == Some(flash) {
                *pocket = None;
            }
        }
        let active = inventory.active;
        inventory.hands[active] = Some(flash);
        world.entity_mut(flash).insert(CarriedBy(mob_a));
        world.entity_mut(mob_a).insert(TilePos::new(4, 3, 1));
        world.entity_mut(mob_b).insert(TilePos::new(5, 3, 1));
    }
    s.run_until("B is replicated to A", |s| s.net_id(a, mob_b).is_some());
    let target = s.net_id(a, mob_b).unwrap();
    s.send(
        a,
        ClientIntent::Click {
            target,
            modifiers: ClickModifiers::default(),
        },
    );

    // B joins A's team: greeting, panel, the same objectives; A is told.
    s.run_until("B is a brother", |s| {
        s.server.world().get::<Brother>(mob_b).is_some()
    });
    assert_eq!(s.server.world().get::<Brother>(mob_b).unwrap().team, team);
    s.wait_chat(a, "пробужден, чтобы помогать вам как ваш кровный брат!");
    s.wait_chat(b, "Вы - кровный брат.");
    let (_, info) = s.wait_window(b, AntagInfoBrotherUi::NAME);
    let info_b: BrotherInfo = ss13_tgui::decode(&info).unwrap();
    assert_eq!(info_b.objectives, info_a.objectives);
    let t = s.server.world().get::<BrotherTeam>(team).unwrap().clone();
    assert_eq!(t.members, [mob_a, mob_b]);
    assert_eq!(t.brothers_left, left - 1);
    assert!(s.server.world().get::<Flash>(flash).unwrap().burnt_out);
    if t.brothers_left == 0 {
        s.wait_chat(a, "Вы больше не можете пробуждать кровных братьев.");
        assert!(s.pocket_flash(b).is_none());
    } else {
        s.wait_chat(b, "Синдикат возлагает на вас большие надежды");
        assert!(s.pocket_flash(b).is_some());
    }

    // Each brother's HUD lists the other (by its own id for them); C has no antag HUD.
    s.run_until("the brothers see each other's HUD icon", |s| {
        let (Some(for_a), Some(for_b)) = (s.net_id(a, mob_b), s.net_id(b, mob_a)) else {
            return false;
        };
        s.hud(a).is_some_and(|h| h.team == [for_a]) && s.hud(b).is_some_and(|h| h.team == [for_b])
    });
    assert!(s.hud(c).is_none());
    assert!(
        s.received(c)
            .chat
            .iter()
            .all(|l| !l.contains("кровный брат")),
        "{:?}",
        s.received(c).chat
    );

    // An admin ends the round: the report lists the team, its members and objectives.
    s.server.world_mut().resource_mut::<Ticker>().force_ending = true;
    assert!(t.name.starts_with("Кровные братья "), "{}", t.name);
    s.wait_chat(c, &format!("{}:", t.name));
    s.wait_chat(c, "Кровные братья были:");
    s.wait_chat(c, "p0 был ");
    s.wait_chat(c, &format!("p{b} был "));
    s.wait_chat(c, "Команда имела следующие цели:");
    s.wait_chat(c, "Цель #1: Пробудите цель без защиты разума");
    assert!(
        s.received(c)
            .chat
            .iter()
            .any(|l| l.starts_with("Цель #1:") && l.ends_with("Успех!")),
        "{:?}",
        s.received(c).chat
    );
    s.wait_chat(c, "Раунд закончился.");
}
