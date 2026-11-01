//! The vampire over real WebTransport on 127.0.0.1: two clients in the lobby, one opts in to the
//! vampire and wants to be the chaplain; an admin-queued "Roundstart Vampire" ruleset (forced,
//! deterministic) makes them the vampire at round start, which bars the chaplain (the ruleset's
//! `blacklisted_roles`), so they spawn as an assistant. The vampire gets the greeting, the info
//! panel, the action buttons and the blood counter; bites the other player in the head (combat
//! mode, empty hand): the victim's blood drops 25 per bite and the vampire's usable blood rises
//! 10; glares at the victim (stamina, a charge spent); at 150 blood unlocks the specialization,
//! picks umbrae in the panel and casts the cloak; the round-end report lists it with its blood.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::actions::{ActionButtonsData, ActionButtonsUi, TRIGGER};
use ss13_game::antag::dynamic::{DevTraitor, DynamicConfig, VAMPIRE_RULESET};
use ss13_game::antag::vampire::server::{Power, Subclass, Vampire};
use ss13_game::antag::vampire::{
    AntagInfoVampireUi, ROLE_VAMPIRE, VampireHudData, VampireHudUi, VampireInfoData,
    VampireInfoStatic,
};
use ss13_game::hud::HealthHudUi;
use ss13_game::jobs::{Jobs, PlayerJob, fallback_jobs};
use ss13_game::lobby::{
    CHARACTER_SETUP, GameState, JobPrefsUi, LobbyData, LobbyUi, SET_ANTAGS, SET_JOB_PREFERENCE,
    TOGGLE_READY,
};
use ss13_game::medical::blood::Blood;
use ss13_game::round::{RoundConfig, Ticker};
use ss13_game::server::Players;
use ss13_game::status_effects::StatusEffects;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::antag::{ANTAG_FILE, AntagData};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, JobsData, OutfitSlot};
use ss13_net::chat::client::ChatReceived;
use ss13_net::server::Interest;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;

const CHAPLAIN: &str = "Chaplain";
const COFFIN: &str = "/obj/structure/closet/crate/coffin";

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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "density": 1, "anchored": 1 } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "name": "closet", "icon": { "resource": "c.dmi" }, "icon_state": "coffin" } },
    "/obj/structure/closet/crate": { "parent": "/obj/structure/closet" },
    "/obj/structure/closet/crate/coffin": { "parent": "/obj/structure/closet/crate",
        "declared_vars": { "name": "coffin" } },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark",
        "vars": { "name": "start" }, "declared_vars": { "name": "start" } },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Assistant" }, "declared_vars": { "name": "Assistant" } },
    "/obj/effect/landmark/start/chaplain": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Chaplain" }, "declared_vars": { "name": "Chaplain" } },
    "/obj/effect/landmark/latejoin": { "parent": "/obj/effect/landmark" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/toolbox": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "toolbox" } },
    "/obj/item/storage/toolbox/syndicate": { "parent": "/obj/item/storage/toolbox",
        "declared_vars": { "name": "suspicious looking toolbox" } },
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
    "/obj/item/radio": { "parent": "/obj/item", "declared_vars": { "name": "station bounced radio",
        "on": 1, "frequency": 1459, "broadcasting": 0, "listening": 1, "canhear_range": 3,
        "subspace_transmission": 0, "freerange": 0, "keyslot": null, "special_channels": 0,
        "slot_flags": 512 } },
    "/obj/item/radio/headset": { "parent": "/obj/item/radio", "declared_vars": {
        "name": "radio headset", "canhear_range": 0, "subspace_transmission": 1,
        "keyslot2": null, "slot_flags": 16 } },
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

/// Assistant starts `s` (2,2) and (8,2), the chaplain start `C` (5,5), latejoin `L`, a coffin
/// `K` (5,3) with floor all around it.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"C" = (/obj/effect/landmark/start/chaplain,/turf/open/floor,/area/space)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/space)
"K" = (/obj/structure/closet/crate/coffin,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wfffCfffLw
wffffffffw
wfffKffffw
wsffffffsw
wwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 6] = [
    ("c.dmi", "coffin"),
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
];

/// The assistant and the chaplain (not protected: only the vampire ruleset bars it).
fn fixture_jobs() -> JobsData {
    let mut data = fallback_jobs();
    data.jobs[0]
        .outfit
        .as_mut()
        .unwrap()
        .items
        .push((OutfitSlot::Ears, "/obj/item/radio/headset".into()));
    let mut chaplain = data.jobs[0].clone();
    chaplain.path = "/datum/job/chaplain".into();
    chaplain.title = CHAPLAIN.into();
    chaplain.total_positions = 1;
    chaplain.spawn_positions = 1;
    data.jobs.push(chaplain);
    data.departments[0].jobs = vec![0, 1];
    data
}

fn fixture_antag() -> AntagData {
    AntagData {
        phonetic_alphabet: ["Альфа", "Браво", "Чарли"].map(str::to_owned).to_vec(),
        ..Default::default()
    }
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vampire_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = fixture_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/start/chaplain",
        "/obj/effect/landmark/latejoin",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        COFFIN,
        "/area/space",
    ];
    used.extend(
        data.outfit_items()
            .into_iter()
            .filter(|p| types.types.contains_key(*p)),
    );
    let jobs = data.to_ron();
    let antag = fixture_antag().to_ron();
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), (JOBS_FILE, &jobs), (ANTAG_FILE, &antag)],
    )
}

/// What a client received: windows opened (id, interface, static data), the latest data per
/// window, the lobby, chat lines.
#[derive(Resource, Default)]
struct Received {
    windows: Vec<(u32, String, Vec<u8>)>,
    data: Vec<(u32, Vec<u8>)>,
    lobby: Option<(u32, LobbyData)>,
    chat: Vec<String>,
    closed: Vec<u32>,
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
                received.closed.push(*window);
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

impl core::ops::Deref for Station {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.0
    }
}

impl core::ops::DerefMut for Station {
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
                // The lobby waits for `start_now` (an admin's "start now"): a countdown in game
                // time could run out while the clients are still setting up over the network.
                app.insert_resource(RoundConfig {
                    lobby_secs: 600.0,
                    round_end_secs: 600.0,
                    ..RoundConfig::tg()
                })
                // An admin queued one vampire ruleset: no tier roll.
                .insert_resource(DynamicConfig {
                    forced_rulesets: Some(vec![VAMPIRE_RULESET]),
                    protect_roles: true,
                    seed: Some(7),
                    dev_traitor: DevTraitor::Off,
                })
                .add_plugins(ChatServerPlugin);
            },
        ))
    }

    /// An admin's "start now": the countdown restarts short.
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
}

impl Station {
    fn window(&self, p: usize, interface: &str) -> Option<u32> {
        self.received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i, _)| i == interface)
            .map(|(id, ..)| *id)
    }

    fn vampire(&self, mob: Entity) -> Vampire {
        self.server.world().get::<Vampire>(mob).unwrap().clone()
    }

    fn blood(&self, mob: Entity) -> f32 {
        self.server.world().get::<Blood>(mob).unwrap().volume
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
}

#[test]
fn roundstart_vampire_drinks_unlocks_and_casts() {
    let mut s = Station::start("round");
    let (a, b) = (s.connect(), s.connect());

    // A: chaplain at high, vampire on; B: assistant, no antagonist preference.
    s.lobby_act(a, CHARACTER_SETUP);
    let (prefs_a, _) = s.wait_window(a, JobPrefsUi::NAME);
    s.act(
        a,
        prefs_a,
        SET_JOB_PREFERENCE,
        &[("job", CHAPLAIN), ("level", "3")],
    );
    s.act(
        a,
        prefs_a,
        SET_ANTAGS,
        &[("antags", ROLE_VAMPIRE), ("toggled", "1")],
    );
    s.lobby_act(b, CHARACTER_SETUP);
    let (prefs_b, _) = s.wait_window(b, JobPrefsUi::NAME);
    s.act(
        b,
        prefs_b,
        SET_JOB_PREFERENCE,
        &[("job", "Assistant"), ("level", "3")],
    );
    s.run_until("preferences stored", |s| {
        let world = s.server.world();
        world
            .get::<ss13_game::antag::dynamic::AntagPreferences>(s.players[a].link)
            .is_some_and(|p| p.wants(ROLE_VAMPIRE))
            && [a, b].iter().all(|&p| {
                world
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

    // A is the vampire; the ruleset's blacklisted chaplain job went elsewhere.
    let (vampire, victim) = (s.mob(a).unwrap(), s.mob(b).unwrap());
    assert!(s.server.world().get::<Vampire>(victim).is_none());
    assert_eq!(s.job(a), "Assistant");
    s.wait_chat(a, "Вы - Вампир.");
    s.wait_chat(a, "Вы — вампир!");
    s.wait_chat(a, "Чтобы укусить кого-то, выберите голову");
    s.wait_chat(a, "Задача #1: Наберите в общей сложности не менее");
    let (info_window, info) = s.wait_window(a, AntagInfoVampireUi::NAME);
    let info: VampireInfoStatic = ss13_tgui::decode(&info).unwrap();
    let ids: Vec<&str> = info.subclasses.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["umbrae", "hemomancer", "gargantua", "dantalion"]);
    let data: VampireInfoData = s.latest_data(a, info_window);
    // Blood, specialization / steal / protect, assassinate, lair, escape / survive.
    assert_eq!(data.objectives.len(), 5, "{:?}", data.objectives);
    assert!(!data.can_select_subclass);
    assert!(s.received(b).chat.iter().all(|l| !l.contains("вампир")));
    // Rejuvenate and glare at 0 blood, on the HUD.
    s.wait_window(a, ActionButtonsUi::NAME);
    s.wait_window(a, VampireHudUi::NAME);
    let ids: Vec<String> = s.buttons(a).buttons.iter().map(|b| b.id.clone()).collect();
    assert_eq!(ids, ["vampire_rejuvenate", "vampire_glare"]);
    assert_eq!(s.buttons(a).buttons[1].maptext.as_deref(), Some("2"));

    // B next to A, facing; A in combat mode aims at the head.
    let at = *s.server.world().get::<TilePos>(vampire).unwrap();
    s.server
        .world_mut()
        .entity_mut(victim)
        .insert(TilePos::new(at.x + 1, at.y, at.z));
    s.server
        .world_mut()
        .entity_mut(vampire)
        .insert(ss13_shared::Facing(ss13_shared::Dir::East));
    s.send(a, ClientIntent::ToggleCombatMode);
    let health = s.window(a, HealthHudUi::NAME).expect("the health HUD");
    s.act(a, health, "zone", &[("zone", "head")]);
    // A bite every 5 s: ten times faster.
    s.server
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_relative_speed(10.0);
    let victim_id = |s: &Station| {
        s.server
            .world()
            .resource::<Interest>()
            .net_id(s.players[a].link, victim)
    };
    s.run_until("B is seen by A", |s| victim_id(s).is_some());
    s.run_until("combat mode and the head", |s| {
        let world = s.server.world();
        world
            .get::<ss13_game::combat::CombatMode>(vampire)
            .is_some_and(|c| c.0)
            && world
                .get::<ss13_game::health::ZoneSelected>(vampire)
                .is_some_and(|z| z.0 == ss13_game::zone::TargetZone::Head)
    });
    let before = s.blood(victim);
    let target = victim_id(&s).unwrap();
    s.send(
        a,
        ClientIntent::Click {
            target,
            modifiers: ClickModifiers::default(),
        },
    );
    s.wait_chat(a, "Вы вонзаете клыки в ");
    s.wait_chat(b, "грубо хватает ");
    s.wait_chat(
        a,
        "Вы накопили 10 ед. крови; для использования осталось 10.",
    );
    assert_eq!(s.blood(victim), before - 25.0);
    let v = s.vampire(vampire);
    assert_eq!((v.bloodtotal, v.bloodusable), (10.0, 10.0));
    s.run_until("the counter shows 10", |s| {
        let window = s.window(a, VampireHudUi::NAME).unwrap();
        s.latest_data::<VampireHudData>(a, window).usable == 10.0
    });

    // The glare: B in front takes the full glare; a charge is spent.
    s.trigger(a, "vampire_glare");
    s.wait_chat(b, "Вас ослепляет взгляд ");
    assert!(
        s.server
            .world()
            .get::<StatusEffects>(victim)
            .is_some_and(|e| e.stamina >= 70.0 || e.stamcrit)
    );
    s.run_until("one glare charge left", |s| {
        s.buttons(a).buttons[1].maptext.as_deref() == Some("1")
    });

    // The bite goes on: 150 blood unlocks vision, the specialization and the lair.
    s.run_until("150 blood", |s| s.vampire(vampire).bloodtotal >= 150.0);
    s.wait_chat(
        a,
        "Теперь вы можете выбрать вампирскую специализацию для развития.",
    );
    let v = s.vampire(vampire);
    assert!(v.has(Power::UnlockSpecialization) && v.has(Power::Lair) && v.has(Power::Vision));
    // 15 bites of 25: the victim lost 375 units (minus what it regrew meanwhile), the vampire got
    // 10 from each.
    assert!(s.blood(victim) < before - 300.0, "{}", s.blood(victim));
    s.run_until("the lair button", |s| {
        s.buttons(a).buttons.iter().any(|b| b.id == "vampire_lair")
    });
    // Stop biting: out of combat mode the next drain does not happen.
    s.send(a, ClientIntent::ToggleCombatMode);
    s.wait_chat(a, "Вы прекращаете высасывать кровь из ");

    // The lair: the button aims, a click on the coffin next to the vampire starts the 10 s
    // ritual, after which the coffin is the vampire's and the spell is gone.
    s.server
        .world_mut()
        .entity_mut(vampire)
        .insert(TilePos::new(5, 2, at.z));
    let coffin = {
        let world = s.server.world_mut();
        let mut q = world.query::<(Entity, &ss13_mapload::DmObj)>();
        q.iter(world)
            .find(|(_, o)| o.path == COFFIN)
            .map(|(e, _)| e)
            .expect("the coffin")
    };
    let coffin_id = |s: &Station| {
        s.server
            .world()
            .resource::<Interest>()
            .net_id(s.players[a].link, coffin)
    };
    s.run_until("the coffin is seen", |s| coffin_id(s).is_some());
    s.trigger(a, "vampire_lair");
    s.run_until("aiming", |s| {
        s.buttons(a)
            .buttons
            .iter()
            .any(|b| b.id == "vampire_lair" && b.active)
    });
    let target = coffin_id(&s).unwrap();
    s.send(
        a,
        ClientIntent::Click {
            target,
            modifiers: ClickModifiers::default(),
        },
    );
    s.wait_chat(a, "Вы начинаете помечать ");
    s.run_until("the lair is made", |s| s.vampire(vampire).has_lair);
    assert!(
        s.server
            .world()
            .get::<ss13_game::antag::vampire::server::VampireCoffin>(coffin)
            .is_some()
    );
    s.run_until("the lair button is gone", |s| {
        s.buttons(a).buttons.iter().all(|b| b.id != "vampire_lair")
    });
    s.server
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_relative_speed(1.0);

    // The panel's pick: umbrae, whose cloak needs 150.
    s.run_until("the panel allows a pick", |s| {
        s.latest_data::<VampireInfoData>(a, info_window)
            .can_select_subclass
    });
    // A forged pick of a nonexistent specialization does nothing.
    s.act(a, info_window, "ancient", &[]);
    s.act(a, info_window, "umbrae", &[]);
    s.run_until("umbrae chosen", |s| {
        s.vampire(vampire).subclass == Some(Subclass::Umbrae)
    });
    s.wait_chat(a, "Вы обрели способность «Покров тьмы»");
    // A second pick is refused.
    s.act(a, info_window, "gargantua", &[]);
    s.run_for(0.3);
    assert_eq!(s.vampire(vampire).subclass, Some(Subclass::Umbrae));
    s.run_until("the cloak button", |s| {
        s.buttons(a).buttons.iter().any(|b| b.id == "vampire_cloak")
    });
    s.trigger(a, "vampire_cloak");
    s.wait_chat(a, "Теперь во тьме вас будут не замечать.");
    assert!(s.vampire(vampire).cloaking);
    // The fixture's lit area has no light sources: dark, so the cloak fades the vampire to 40
    // and makes it faster.
    s.run_until("the cloak fades the vampire", |s| {
        s.server
            .world()
            .get::<Appearance>(vampire)
            .is_some_and(|ap| ap.color == Some([255, 255, 255, 40]))
    });
    assert_eq!(
        s.server
            .world()
            .get::<ss13_game::health::MoveSpeedModifier>(vampire)
            .map(|m| m.0),
        Some(-0.25)
    );

    // An admin ends the round: the report lists the vampire, its blood and objectives.
    s.server.world_mut().resource_mut::<Ticker>().force_ending = true;
    s.wait_chat(b, "Вампиры были:");
    s.wait_chat(b, "p0 был ");
    s.wait_chat(b, "Класс: Умбра | Всего крови: 1");
    s.wait_chat(b, "Задача #5: ");
    s.wait_chat(b, "Раунд закончился.");
}

/// A ghost works none of its body's windows (tg `SStgui.on_logout`): the vampire's buttons and
/// blood counter close when it ghosts, a click on the old buttons casts nothing, and back in the
/// body the windows open again.
#[test]
fn a_ghost_does_not_keep_its_vampire_body_windows() {
    let assets = fixture_assets("ghost_windows");
    let mut s = Station(testing::Station::start(
        fixture_map(&assets, &["map.dmm"]),
        |app| {
            app.add_plugins(ChatServerPlugin);
        },
    ));
    let a = s.0.join("p0", |app| {
        app.init_resource::<Received>().add_systems(Update, record);
    });
    let body = s.mob(a).unwrap();
    assert!(ss13_game::antag::vampire::server::add_vampire(
        s.server.world_mut(),
        body
    ));
    let (buttons, _) = s.wait_window(a, ActionButtonsUi::NAME);
    let (hud, _) = s.wait_window(a, VampireHudUi::NAME);

    let link = s.players[a].link;
    let ghost = ss13_game::ghost::ghostize_client(s.server.world_mut(), link).unwrap();
    s.run_until("the body's windows close", |s| {
        let closed = &s.received(a).closed;
        closed.contains(&buttons) && closed.contains(&hud)
    });
    let before = s
        .server
        .world()
        .get::<ss13_game::actions::ActionButtons>(body)
        .cloned();
    s.act(a, buttons, TRIGGER, &[("id", "vampire_rejuvenate")]);
    s.run_for(0.3);
    assert_eq!(
        s.server
            .world()
            .get::<ss13_game::actions::ActionButtons>(body)
            .cloned(),
        before,
        "nothing was cast"
    );
    assert_eq!(s.mob(a), Some(ghost));

    let opened = s.received(a).windows.len();
    ss13_game::ghost::return_to_body(s.server.world_mut(), body);
    s.run_until("the windows open again", |s| {
        let new = &s.received(a).windows[opened..];
        [ActionButtonsUi::NAME, VampireHudUi::NAME]
            .iter()
            .all(|name| new.iter().any(|(_, i, _)| i == name))
    });
}
