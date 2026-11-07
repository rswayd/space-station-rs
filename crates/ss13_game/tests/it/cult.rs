//! The blood cult over real WebTransport on 127.0.0.1: five clients in the lobby, two opt in to
//! the cult; an admin-queued "Roundstart Blood Cult" ruleset (forced, deterministic) makes them
//! the cult at round start (`ceil(5 / 20) + 1` = 2). The cultists get the greeting, the panel,
//! the HUD, the dagger and their buttons; speak in Communion (the crew does not read it); draw an
//! Offer rune with the dagger from the rune list; one invoker cannot convert, two can; a
//! mindshielded crew member is refused; three sacrifice the target (the objective is done); holy
//! water deconverts the convert; the Nar'Sie rune is drawn in the summoning area and nine call
//! her: the round ends with the cult's victory in the report.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::actions::{ActionButtonsData, ActionButtonsUi, TRIGGER};
use ss13_game::antag::BrotherInfo;
use ss13_game::antag::cult::server::{CultTeam, Cultist, HOLY_WATER, Rune, RuneKind};
use ss13_game::antag::cult::{AntagInfoGenericUi, ROLE_CULTIST};
use ss13_game::antag::dynamic::{CULT_RULESET, DevTraitor, DynamicConfig};
use ss13_game::chem::{Chem, MobReagents};
use ss13_game::health::{Health, Stat};
use ss13_game::human::{ASSISTANT_SHOES, ASSISTANT_UNIFORM, HAIR_STYLES, HumanLook, parts};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::jobs::{Jobs, PlayerJob, fallback_jobs};
use ss13_game::lobby::{
    CHARACTER_SETUP, GameState, JobPrefsUi, LobbyData, LobbyUi, SET_ANTAGS, SET_JOB_PREFERENCE,
    TOGGLE_READY,
};
use ss13_game::round::{RoundConfig, Ticker};
use ss13_game::server::Players;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::antag::{ANTAG_FILE, AntagData};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, JobsData};
use ss13_net::chat::client::ChatReceived;
use ss13_net::server::Interest;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::interfaces::text_input::TextInputUi;
use ss13_world::SpatialIndex;

const DAGGER: &str = "/obj/item/melee/cultblade/dagger";

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1,
        "inverse_chem": { "path": "/datum/reagent/inverse" }, "inverse_chem_val": 0.25,
        "purge_multiplier": 1 } },
    "/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Water", "color": "#AAAAAA77", "chemical_flags": 144 } },
    "/datum/reagent/water/holywater": { "parent": "/datum/reagent/water", "declared_vars": {
        "name": "Holy Water", "color": "#E0E8EF", "chemical_flags": 4240 } },
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
    "/obj/effect/rune": { "parent": "/obj/effect", "declared_vars": {
        "name": "rune", "icon": { "resource": "rune.dmi" }, "icon_state": "1", "anchored": 1,
        "plane": -6, "color": "#FF0000" } },
    "/obj/effect/rune/convert": { "parent": "/obj/effect/rune", "declared_vars": {
        "icon_state": "3", "color": "#FFFFFF" } },
    "/obj/effect/rune/narsie": { "parent": "/obj/effect/rune", "declared_vars": {
        "icon_state": "1", "color": "#7D1717" } },
    "/obj/narsie": { "parent": "/obj", "declared_vars": {
        "name": "Nar'Sie", "icon": { "resource": "narsie.dmi" }, "icon_state": "narsie",
        "anchored": 1 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/melee": { "parent": "/obj/item" },
    "/obj/item/melee/cultblade": { "parent": "/obj/item/melee" },
    "/obj/item/melee/cultblade/dagger": { "parent": "/obj/item/melee/cultblade",
        "declared_vars": { "name": "ritual dagger", "icon_state": "render", "force": 15,
        "w_class": 2 } },
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

/// Assistant starts `s`, latejoin `L`; all of it the bridge.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/command/bridge)
"f" = (/turf/open/floor,/area/station/command/bridge)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/command/bridge)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/station/command/bridge)

(1,1,1) = {"
wwwwwwwwwwww
wffffffffffw
wffffffffffw
wffffffffLfw
wffffffffffw
wssssssssssw
wwwwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 9] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "render"),
    ("rune.dmi", "1"),
    ("rune.dmi", "3"),
    ("narsie.dmi", "narsie"),
];

fn human_states() -> Vec<(String, String)> {
    let mut out = vec![("icons/mob/human/human.dmi".to_owned(), "blank".to_owned())];
    for female in [false, true] {
        for hair in HAIR_STYLES {
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
                husk: false,
                hulk: false,
                missing: 0,
                robotic: 0,
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

/// The fallback jobs plus a security officer (a mindshield job) to move a player into.
fn jobs() -> JobsData {
    let mut data = fallback_jobs();
    let mut officer = data.jobs[0].clone();
    officer.title = "Security Officer".into();
    data.jobs.push(officer);
    data
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("cult_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/latejoin",
        "/obj/effect/rune/convert",
        "/obj/effect/rune/narsie",
        "/obj/narsie",
        DAGGER,
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
    let antag = AntagData {
        phonetic_alphabet: ["Альфа", "Браво", "Чарли"].map(str::to_owned).to_vec(),
        ..Default::default()
    }
    .to_ron();
    let dir = write_fixture(
        &dir,
        TYPES,
        &used,
        &states,
        &[("map.dmm", MAP), (JOBS_FILE, &jobs), (ANTAG_FILE, &antag)],
    );
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    dir
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
                    forced_rulesets: Some(vec![CULT_RULESET]),
                    protect_roles: true,
                    seed: Some(7),
                    dev_traitor: DevTraitor::Off,
                })
                .add_plugins(ChatServerPlugin);
            },
        ))
    }

    fn start_now(&mut self) {
        let world = self.server.world_mut();
        world.resource_mut::<RoundConfig>().lobby_secs = 0.5;
        world.insert_resource(Ticker::default());
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

    /// Waits for the `n`th window of `interface` (1-based) and returns its id.
    fn wait_nth_window(&mut self, p: usize, interface: &str, n: usize) -> u32 {
        self.run_until(&format!("{interface} #{n} opens"), |s| {
            s.windows_of(p, interface) >= n
        });
        self.window(p, interface).unwrap()
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

    fn read(&self, p: usize, part: &str) -> bool {
        self.received(p).chat.iter().any(|n| n.contains(part))
    }

    fn mob(&self, p: usize) -> Entity {
        self.server
            .world()
            .resource::<Players>()
            .mob(self.players[p].link)
            .unwrap()
    }

    fn trigger(&mut self, p: usize, id: &str) {
        let window = self.window(p, ActionButtonsUi::NAME).expect("buttons");
        self.act(p, window, TRIGGER, &[("id", id)]);
    }

    fn put(&mut self, mob: Entity, pos: TilePos) {
        self.server.world_mut().entity_mut(mob).insert(pos);
    }

    fn sees(&mut self, p: usize, e: Entity) {
        self.run_until("seen", |s| {
            s.server
                .world()
                .resource::<Interest>()
                .net_id(s.players[p].link, e)
                .is_some()
        });
    }

    fn team(&self) -> CultTeam {
        self.server.world().resource::<CultTeam>().clone()
    }

    fn rune_at(&self, pos: TilePos) -> Option<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Rune>(e).is_some())
    }

    /// The dagger from wherever it is into `mob`'s active hand.
    fn dagger_in_hand(&mut self, mob: Entity) {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &ss13_mapload::DmItem, Option<&CarriedBy>)>();
        let dagger = q
            .iter(world)
            .find(|(_, i, c)| i.path == DAGGER && c.is_some_and(|c| c.0 == mob))
            .map(|(e, ..)| e)
            .expect("the cultist carries a dagger");
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        for p in &mut inventory.pockets {
            if *p == Some(dagger) {
                *p = None;
            }
        }
        let hand = inventory.active;
        inventory.hands[hand] = Some(dagger);
    }

    /// Draws `rune` with the dagger in hand: the rune list, then the `do_after`.
    fn scribe(&mut self, p: usize, rune: &str, nth_list: usize) {
        let mob = self.mob(p);
        let hand = if self.server.world().get::<Inventory>(mob).unwrap().active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        };
        self.send(p, ClientIntent::ClickSlot(hand));
        let list = self.wait_nth_window(p, ListInputUi::NAME, nth_list);
        let data: ListInputData = self.latest_data(p, list);
        assert_eq!(data.title, "Sigils of Power");
        assert!(data.items.iter().any(|i| i == rune), "{:?}", data.items);
        self.act(p, list, "submit", &[("entry", rune)]);
    }
}

#[test]
fn roundstart_cult_converts_sacrifices_and_summons_narsie() {
    let mut s = Station::start("round");
    let players: Vec<usize> = (0..5).map(|_| s.connect()).collect();
    let (a, b) = (players[0], players[1]);
    for &p in &players {
        s.lobby_act(p, CHARACTER_SETUP);
        let (prefs, _) = s.wait_window(p, JobPrefsUi::NAME);
        s.act(
            p,
            prefs,
            SET_JOB_PREFERENCE,
            &[("job", "Assistant"), ("level", "3")],
        );
        if p == a || p == b {
            s.act(
                p,
                prefs,
                SET_ANTAGS,
                &[("antags", ROLE_CULTIST), ("toggled", "1")],
            );
        }
    }
    s.run_until("preferences stored", |s| {
        [a, b].iter().all(|&p| {
            s.server
                .world()
                .get::<ss13_game::antag::dynamic::AntagPreferences>(s.players[p].link)
                .is_some_and(|prefs| prefs.wants(ROLE_CULTIST))
        }) && players.iter().all(|&p| {
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
    s.start_now();
    s.run_until("the round starts", |s| {
        s.server.world().resource::<Ticker>().state() >= GameState::Playing
    });

    // A and B are the cult; the others are not.
    let mobs: Vec<Entity> = players.iter().map(|&p| s.mob(p)).collect();
    let (ma, mb) = (mobs[0], mobs[1]);
    for (i, &m) in mobs.iter().enumerate() {
        assert_eq!(
            s.server.world().get::<Cultist>(m).is_some(),
            i < 2,
            "player {i}"
        );
    }
    s.wait_chat(a, "Вы - Культист.");
    s.wait_chat(a, "Задача #1: Sacrifice ");
    s.wait_chat(a, "Задача #2: Summon Nar'Sie");
    s.wait_chat(a, "ritual dagger был помещен в ваш");
    let (info, st) = s.wait_window(a, AntagInfoGenericUi::NAME);
    let _ = info;
    let info: BrotherInfo = ss13_tgui::decode(&st).unwrap();
    assert_eq!(info.antag_name, "Культист");
    assert_eq!(info.objectives.len(), 2);
    let team = s.team();
    assert_eq!(team.members.len(), 2);
    assert!(team.leader.is_some());
    assert!(
        team.summon_spots().iter().any(|(_, n)| n == "Bridge"),
        "{:?}",
        team.summon_spots()
    );
    s.wait_window(a, ActionButtonsUi::NAME);
    let window = s.window(a, ActionButtonsUi::NAME).unwrap();
    let buttons: ActionButtonsData = s.latest_data(a, window);
    let ids: Vec<&str> = buttons.buttons.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids, ["cult_comm", "cult_blood_magic"]);

    // Communion: every cultist reads it, the crew does not.
    s.trigger(a, "cult_comm");
    let (text, _) = s.wait_window(a, TextInputUi::NAME);
    s.act(a, text, "submit", &[("entry", "Слава Геометру")]);
    s.wait_chat(b, ": Слава Геометру");
    s.run_for(0.3);
    // Nearby crew may hear the whisper, never the cult's channel.
    for &p in &players[2..] {
        assert!(
            !s.read(p, "Acolyte ") && !s.read(p, "Master "),
            "p{p} read the cult"
        );
    }

    // The sacrifice target, a crew member to convert, one to mindshield.
    let target = match team.objectives[0] {
        ss13_game::antag::cult::server::CultObjective::Sacrifice { target, .. } => target.unwrap(),
        _ => panic!("no sacrifice objective"),
    };
    let others: Vec<Entity> = mobs[2..].iter().copied().filter(|&m| m != target).collect();
    let (convert, shielded) = (others[0], others[1]);
    let p_convert = players[2 + mobs[2..].iter().position(|&m| m == convert).unwrap()];

    // A draws an Offer rune where she stands, B far away.
    let here = TilePos::new(5, 4, 1);
    s.put(ma, here);
    s.put(mb, TilePos::new(10, 2, 1));
    for &m in &[convert, shielded, target] {
        s.put(m, TilePos::new(2, 2, 1));
    }
    s.dagger_in_hand(ma);
    s.scribe(a, "Offer", 1);
    s.wait_chat(
        a,
        "You slice open your arm and begin drawing a sigil of the Geometer.",
    );
    s.run_until("the rune is drawn", |s| s.rune_at(here).is_some());
    let rune = s.rune_at(here).unwrap();
    assert_eq!(
        s.server.world().get::<Rune>(rune).unwrap().kind,
        RuneKind::Offer
    );
    s.wait_chat(a, "The offer rune offers a noncultist above it to Nar'Sie");
    // An empty hand invokes (the dagger would scrape the rune away).
    s.send(a, ClientIntent::SwapHands);

    // One invoker cannot convert.
    s.put(convert, here);
    s.sees(a, rune);
    s.click(a, rune);
    s.wait_chat(a, "You need at least two invokers to convert");
    assert!(s.server.world().get::<Cultist>(convert).is_none());

    // Two can.
    s.put(mb, TilePos::new(6, 4, 1));
    s.run_for(1.0);
    s.click(a, rune);
    s.run_until("converted", |s| {
        s.server.world().get::<Cultist>(convert).is_some()
    });
    assert!(s.server.world().get::<Cultist>(convert).unwrap().converted);
    s.wait_chat(p_convert, "Вы - Обращенный культист.");
    assert_eq!(s.team().members.len(), 3);

    // A mindshielded crew member is not converted: only three could sacrifice them.
    let officer = s
        .server
        .world()
        .resource::<Jobs>()
        .data
        .job("Security Officer")
        .unwrap();
    s.server
        .world_mut()
        .entity_mut(shielded)
        .insert(PlayerJob(officer));
    // One invoker short of three: A and B (the convert stands off).
    s.put(convert, TilePos::new(8, 4, 1));
    s.put(shielded, here);
    s.run_for(1.0);
    s.click(a, rune);
    s.wait_chat(
        a,
        "is too greatly linked to the world! You need three acolytes!",
    );
    assert!(s.server.world().get::<Cultist>(shielded).is_none());
    s.put(shielded, TilePos::new(2, 2, 1));

    // Three sacrifice the target: the objective is done, the body is gone.
    s.put(target, here);
    s.put(convert, TilePos::new(4, 4, 1));
    s.run_for(1.0);
    s.click(a, rune);
    s.run_until("sacrificed", |s| {
        s.server.world().get_entity(target).is_err()
    });
    assert!(s.team().sacrifice_done());
    s.wait_chat(a, "\"Yes! This is the one I desire! You have done well.\"");

    // Holy water deconverts the convert after a minute in the blood.
    {
        let world = s.server.world_mut();
        let chem = world.resource::<Chem>().0.clone();
        let id = chem.id(HOLY_WATER).unwrap();
        let mut reagents = world.get::<MobReagents>(convert).unwrap().clone();
        reagents
            .blood
            .add_reagent(&chem, id, 30.0, 310.15, None, None, false, false);
        world.entity_mut(convert).insert(reagents);
    }
    s.wait_chat(p_convert, "A vile holiness begins to spread");
    s.server
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_relative_speed(10.0);
    let started = s.now();
    s.run_until("deconverted", |s| {
        s.server.world().get::<Cultist>(convert).is_none()
    });
    let took = s.now() - started;
    assert!((56.0..66.0).contains(&took), "deconverting took {took} s");
    s.wait_chat(
        p_convert,
        "Незнакомый белый свет вспыхивает в твоем сознании",
    );
    assert_eq!(s.team().members.len(), 2);

    // Nar'Sie: her rune in the bridge (50 s, 40.1 brute), nine invokers.
    let site = TilePos::new(5, 3, 1);
    s.put(ma, site);
    s.run_for(0.5);
    let brute = s.server.world().get::<Health>(ma).unwrap().brute;
    s.send(a, ClientIntent::SwapHands);
    s.run_for(0.5);
    s.scribe(a, "Nar'Sie", 2);
    s.wait_chat(a, "Зафиксирован призыв древнего божества");
    s.run_until("the Nar'Sie rune is drawn", |s| s.rune_at(site).is_some());
    let narsie_rune = s.rune_at(site).unwrap();
    assert!(s.server.world().get::<Health>(ma).unwrap().brute - brute >= 40.0);
    // Seven more cultists around it (bodies without players).
    let offsets = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1)];
    for (dx, dy) in offsets {
        let world = s.server.world_mut();
        let body = world
            .spawn((
                TilePos::new(site.x + dx, site.y + dy, 1),
                Stat::Conscious,
                Health::default(),
                Cultist::default(),
                Name::new("Acolyte"),
            ))
            .id();
        world.resource_mut::<CultTeam>().members.push(body);
    }
    s.put(mb, TilePos::new(site.x + 1, site.y + 1, 1));
    s.send(a, ClientIntent::SwapHands);
    s.run_for(1.0);
    s.sees(a, narsie_rune);
    s.click(a, narsie_rune);
    s.wait_chat(players[3], "NAR'SIE HAS RISEN");
    assert!(s.team().victory());
    assert!(s.team().narsie.is_some());
    s.wait_chat(players[4], "ПОЛНОЕ ИСТРЕБЛЕНИЕ");
    s.run_until("the round ends", |s| {
        s.server.world().resource::<Ticker>().state() == GameState::Finished
    });
    s.wait_chat(players[4], "Культ смог призвать своего бога!");
    s.wait_chat(players[4], "Культистами были:");
}
