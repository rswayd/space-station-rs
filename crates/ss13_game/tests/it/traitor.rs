//! Traitors over real WebTransport on 127.0.0.1: two clients in the lobby, one opts in to traitor
//! and wants to be a Security Officer; an admin-queued traitor ruleset (forced, deterministic)
//! makes them the traitor at round start, which bars security, so they spawn as an assistant.
//! The traitor gets the greeting, the info panel with objectives and the uplink code, says the
//! code on `:z` (the other player hears nothing), buys a toolbox (TC 20 -> 19, the toolbox in
//! hand), and the round-end report lists the traitor's objectives and purchase.

use std::path::{Path, PathBuf};

use crate::harness::{self, Client};
use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::antag::dynamic::{DevTraitor, DynamicConfig, TRAITOR_RULESET};
use ss13_game::antag::traitor::Traitor;
use ss13_game::antag::uplink::Uplink;
use ss13_game::antag::{
    AntagInfoTraitorUi, BUY, ROLE_TRAITOR, TraitorInfo, UplinkData, UplinkStatic, UplinkUi,
};
use ss13_game::inventory::Inventory;
use ss13_game::jobs::{Jobs, PlayerJob, fallback_jobs};
use ss13_game::lobby::{
    CHARACTER_SETUP, GameState, JobPrefsUi, SET_ANTAGS, SET_JOB_PREFERENCE, TOGGLE_READY,
};
use ss13_game::round::{RoundConfig, Ticker};
use ss13_game::server::Players;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_mapload::antag::{ANTAG_FILE, AntagData, TraitorFlavor, UPLINK_TRAITORS, UplinkItemDef};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, JobsData, OutfitSlot};
use ss13_shared::protocol::ClientIntent;
use ss13_tgui::UiInterface;

const TOOLBOX: &str = "/obj/item/storage/toolbox/syndicate";
const TOOLBOX_ITEM: &str = "/datum/uplink_item/device_tools/toolbox";
const OFFICER: &str = "Security Officer";

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
    "/obj/effect/landmark/start/security_officer": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Security Officer" }, "declared_vars": { "name": "Security Officer" } },
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

/// Assistant starts `s` (2,2) and (8,2), the officer start `S` (5,3), latejoin `L`.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"S" = (/obj/effect/landmark/start/security_officer,/turf/open/floor,/area/space)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wfffSfffLw
wsffffffsw
wwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
];

/// The assistant and a protected Security Officer, both with a plain headset.
fn fixture_jobs() -> JobsData {
    let mut data = fallback_jobs();
    data.jobs[0]
        .outfit
        .as_mut()
        .unwrap()
        .items
        .push((OutfitSlot::Ears, "/obj/item/radio/headset".into()));
    let mut officer = data.jobs[0].clone();
    officer.path = "/datum/job/security_officer".into();
    officer.title = OFFICER.into();
    officer.total_positions = 5;
    officer.spawn_positions = 5;
    officer.antag_protected = true;
    data.jobs.push(officer);
    data.departments[0].jobs = vec![0, 1];
    data
}

fn fixture_antag() -> AntagData {
    let flavor = TraitorFlavor {
        introduction: "Вы — агент.".into(),
        roundend_report: "был агентом.".into(),
        uplink: "У вас есть аплинк.".into(),
        ..Default::default()
    };
    AntagData {
        uplink_categories: vec!["Misc. Gadgets".into()],
        uplink_items: vec![UplinkItemDef {
            path: TOOLBOX_ITEM.into(),
            name: "Full Syndicate Toolbox".into(),
            desc: "Tools.".into(),
            item: TOOLBOX.into(),
            cost: 1,
            category: "Misc. Gadgets".into(),
            purchasable_from: UPLINK_TRAITORS,
            limited_stock: -1,
            ..Default::default()
        }],
        steal_items: Vec::new(),
        // Every employer reads the same here.
        traitor_flavor: [
            "Cybersun Industries",
            "Donk Corporation",
            "Gorlex Marauders",
            "MI13",
            "Tiger Cooperative Occultist",
            "Waffle Corporation Terrorist",
            "Waffle Corporation",
            "S.E.L.F. Activist",
            "Corporate Climber",
            "Legal Trouble",
        ]
        .map(|e| (e.to_owned(), flavor.clone()))
        .to_vec(),
        phonetic_alphabet: ["Альфа", "Браво", "Чарли"].map(str::to_owned).to_vec(),
        ..Default::default()
    }
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("traitor_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = fixture_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/start/security_officer",
        "/obj/effect/landmark/latejoin",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        TOOLBOX,
    ];
    used.extend(
        data.outfit_items()
            .into_iter()
            .filter(|p| types.types.contains_key(*p)),
    );
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    std::fs::write(dir.join(JOBS_FILE), data.to_ron()).unwrap();
    std::fs::write(dir.join(ANTAG_FILE), fixture_antag().to_ron()).unwrap();
    dir
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
                app.insert_resource(RoundConfig {
                    lobby_secs: 600.0,
                    round_end_secs: 600.0,
                    ..RoundConfig::tg()
                })
                // An admin queued one traitor ruleset: no tier roll.
                .insert_resource(DynamicConfig {
                    forced_rulesets: Some(vec![TRAITOR_RULESET]),
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

#[test]
fn roundstart_traitor_buys_from_the_uplink() {
    let mut s = Station::start("round");
    let (a, b) = (s.connect(), s.connect());
    // The lobby's antagonist toggle is on by default (BandaStation `antag_roll`).
    assert!(s.received(a).lobby.as_ref().unwrap().1.antag_roll);

    // A: security officer at high, traitor on; B: assistant, no antagonist preference.
    s.lobby_act(a, CHARACTER_SETUP);
    let (prefs_a, _) = s.wait_window(a, JobPrefsUi::NAME);
    s.act(
        a,
        prefs_a,
        SET_JOB_PREFERENCE,
        &[("job", OFFICER), ("level", "3")],
    );
    s.act(
        a,
        prefs_a,
        SET_ANTAGS,
        &[("antags", ROLE_TRAITOR), ("toggled", "1")],
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
            .is_some_and(|p| p.wants(ROLE_TRAITOR))
            && [a, b].iter().all(|&p| {
                world
                    .get::<ss13_game::round::JobPreferences>(s.players[p].link)
                    .is_some_and(|prefs| !prefs.levels.is_empty())
            })
    });
    s.lobby_act(a, TOGGLE_READY);
    s.lobby_act(b, TOGGLE_READY);
    s.run_until("everyone is ready", |s| {
        [a, b]
            .iter()
            .all(|&p| s.received(p).lobby.as_ref().is_some_and(|(_, l)| l.ready))
    });
    s.start_now();
    s.run_until("the round starts", |s| {
        s.server.world().resource::<Ticker>().state() >= GameState::Playing
    });

    // A is the traitor; the protected job went elsewhere.
    let traitor = s.mob(a).unwrap();
    assert!(s.server.world().get::<Traitor>(traitor).is_some());
    assert!(s.server.world().get::<Traitor>(s.mob(b).unwrap()).is_none());
    assert_eq!(s.job(a), "Assistant");
    s.wait_chat(a, "Вы - Предатель.");
    s.wait_chat(a, "Для большей информации - прочтите панель.");
    let (_, info) = s.wait_window(a, AntagInfoTraitorUi::NAME);
    let info: TraitorInfo = ss13_tgui::decode(&info).unwrap();
    assert_eq!(info.intro, "Вы — агент.");
    // Two generic objectives and the ending one (escape or martyr).
    assert_eq!(info.objectives.len(), 3, "{:?}", info.objectives);
    let code = info.code.clone().expect("an uplink code");
    assert_eq!(code.split(' ').count(), 3);
    assert!(info.uplink_unlock_info.contains(&code));
    assert!(s.received(b).chat.iter().all(|l| !l.contains("Предатель")));

    // The code on the uplink channel opens the uplink; nobody hears it on the radio.
    s.send(a, ClientIntent::Say(format!(":z {code}")));
    let (uplink_window, st) = s.wait_window(a, UplinkUi::NAME);
    let st: UplinkStatic = ss13_tgui::decode(&st).unwrap();
    assert_eq!(st.categories, ["Misc. Gadgets"]);
    assert_eq!(st.items[0].id, TOOLBOX_ITEM);
    let data: UplinkData = s.latest_data(a, uplink_window);
    assert_eq!(data.telecrystals, 20);
    assert_eq!(data.primary_objectives.len(), 3);
    s.run_for(0.3);
    assert!(
        s.received(b).chat.iter().all(|l| !l.contains(&code)),
        "{:?}",
        s.received(b).chat
    );

    // Buying: TC 20 -> 19, the toolbox in the traitor's hand.
    s.act(a, uplink_window, BUY, &[("path", TOOLBOX_ITEM)]);
    s.run_until("the toolbox is in hand", |s| {
        let world = s.server.world();
        world.get::<Inventory>(traitor).is_some_and(|inv| {
            inv.hands
                .iter()
                .flatten()
                .any(|&e| world.get::<DmItem>(e).is_some_and(|dm| dm.path == TOOLBOX))
        })
    });
    s.wait_chat(a, "материализуется в вашей руке!");
    s.run_until("the window shows 19 TC", |s| {
        s.latest_data::<UplinkData>(a, uplink_window).telecrystals == 19
    });
    let device = s
        .server
        .world()
        .get::<Traitor>(traitor)
        .unwrap()
        .uplink
        .unwrap();
    assert_eq!(
        s.server.world().get::<Uplink>(device).unwrap().telecrystals,
        19
    );
    // A forged act for an item the uplink does not sell changes nothing.
    s.act(
        a,
        uplink_window,
        BUY,
        &[("path", "/datum/uplink_item/dangerous/sword")],
    );
    s.run_for(0.3);
    assert_eq!(
        s.server.world().get::<Uplink>(device).unwrap().telecrystals,
        19
    );

    // An admin ends the round: the report lists the traitor with objectives and the purchase.
    s.server.world_mut().resource_mut::<Ticker>().force_ending = true;
    s.wait_chat(b, "Предатели были:");
    s.wait_chat(b, "p0 был ");
    s.wait_chat(b, "(использовал 1 ТК) [Full Syndicate Toolbox]");
    s.wait_chat(b, "Задача #1: ");
    s.wait_chat(b, "Задача #3: ");
    s.wait_chat(b, "Раунд закончился.");
}
