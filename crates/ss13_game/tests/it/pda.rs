//! PDAs over real WebTransport on 127.0.0.1: two assistants spawn with an imprinted PDA on the
//! belt. A opens it, runs the messenger and messages B; B gets the chat line and the ring, opens
//! the conversation and reads it; a second message inside the cooldown is dropped. A's ID goes
//! into the PDA: on the belt it opens nothing, in the ID slot it opens a maintenance door. A
//! traitor's uplink hides in the PDA and the ringtone code opens it.

use std::path::{Path, PathBuf};

use crate::harness::{self, Client, Received, record};
use bevy::prelude::*;
use ss13_game::antag::UplinkUi;
use ss13_game::antag::traitor::{Traitor, add_traitor};
use ss13_game::antag::uplink::Uplink;
use ss13_game::doors::{Door, DoorState};
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::jobs::fallback_jobs;
use ss13_game::modpc::pda::ModularComputer;
use ss13_game::modpc::programs::messenger::Messenger;
use ss13_game::modpc::{
    MessengerData, NtosData, NtosScreen, NtosUi, PC_RUNPROGRAM, PDA_RING_SET, PDA_SEND_MESSAGE,
    PDA_VIEW_MESSAGES,
};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmName;
use ss13_mapload::antag::{ANTAG_FILE, AntagData};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, JobsData, OutfitSlot};
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_shared::{Dir, TilePos};
use ss13_tgui::UiInterface;
use ss13_world::SpatialIndex;

const PDA: &str = "/obj/item/modular_computer/pda/crew";

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
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/modular_computer": { "parent": "/obj/item", "declared_vars": {
        "name": "modular microcomputer" } },
    "/obj/item/modular_computer/pda": { "parent": "/obj/item/modular_computer", "declared_vars": {
        "name": "pda", "slot_flags": 768 } },
    "/obj/item/modular_computer/pda/crew": { "parent": "/obj/item/modular_computer/pda" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "autoclose": 1,
        "base_icon_state": null, "normalspeed": 1, "overlays_file": { "resource": "o.dmi" },
        "airlock_material": null } },
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

/// Spawns `s` (3,3) and (3,2), each with a maintenance door `M` east of it.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"M" = (/obj/machinery/door/airlock{req_access = list("maint_tunnels")},/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffffffw
wfsMfffw
wfsMfffw
wwwwwwww
"}
"##;

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("a.dmi", "closed"),
];

/// The assistant with a PDA on the belt (tg `/datum/outfit/job` `belt`).
fn fixture_jobs() -> JobsData {
    let mut data = fallback_jobs();
    data.jobs[0]
        .outfit
        .as_mut()
        .unwrap()
        .items
        .push((OutfitSlot::Belt, PDA.into()));
    data
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("pda_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = fixture_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/door/airlock",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    used.extend(
        data.outfit_items()
            .into_iter()
            .filter(|p| types.types.contains_key(*p)),
    );
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    std::fs::write(dir.join(JOBS_FILE), data.to_ron()).unwrap();
    let antag = AntagData {
        phonetic_alphabet: ["Альфа", "Браво"].map(str::to_owned).to_vec(),
        ..Default::default()
    };
    std::fs::write(dir.join(ANTAG_FILE), antag.to_ron()).unwrap();
    dir
}

struct Station {
    st: testing::Station,
    /// Player 0's spawn.
    spawn: TilePos,
}

ss13_game::drive_station!(Station, st, context = harness::chat_context);
impl Client for Station {}

impl Station {
    fn start(test: &str, clients: usize) -> Self {
        let assets = fixture_assets(test);
        let mut st = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        st.join_all(clients, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        let spawn = *st.get::<TilePos>(st.players[0].mob);
        Self { st, spawn }
    }

    fn slot(&mut self, p: usize, slot: InventorySlot) {
        self.send(p, ClientIntent::ClickSlot(slot));
    }

    fn wait_window(&mut self, p: usize, interface: &str) -> u32 {
        Client::wait_window(self, p, interface).0
    }

    fn ntos(&self, p: usize, window: u32) -> NtosData {
        let (_, data) = self
            .received(p)
            .data
            .iter()
            .rev()
            .find(|(w, _)| *w == window)
            .unwrap();
        ss13_tgui::decode(data).unwrap()
    }

    /// Waits until the window shows the messenger and `done` holds for it.
    fn wait_messenger(
        &mut self,
        p: usize,
        window: u32,
        what: &str,
        done: impl Fn(&MessengerData) -> bool,
    ) -> MessengerData {
        self.run_until(
            what,
            |s| matches!(&s.ntos(p, window).screen, NtosScreen::Messenger(m) if done(m)),
        );
        match self.ntos(p, window).screen {
            NtosScreen::Messenger(m) => m,
            _ => unreachable!(),
        }
    }

    fn inventory(&self, p: usize) -> Inventory {
        self.server
            .world()
            .get::<Inventory>(self.players[p].mob)
            .unwrap()
            .clone()
    }

    fn pda(&self, p: usize) -> Entity {
        let world = self.server.world();
        let inv = self.inventory(p);
        inv.equipped()
            .chain(inv.hands.iter().flatten().copied())
            .find(|&e| world.get::<ModularComputer>(e).is_some())
            .expect("a PDA")
    }

    /// Takes the PDA from the belt into the active hand and uses it: the NtOS window.
    fn open_pda(&mut self, p: usize) -> u32 {
        let pda = self.pda(p);
        self.slot(p, InventorySlot::Belt);
        self.run_until("the PDA is in hand", |s| {
            s.inventory(p).hands[0] == Some(pda)
        });
        self.slot(p, InventorySlot::LeftHand);
        self.wait_window(p, NtosUi::NAME)
    }

    fn pos(&self, p: usize) -> TilePos {
        *self
            .server
            .world()
            .get::<TilePos>(self.players[p].mob)
            .unwrap()
    }

    /// The door east of player 0's spawn.
    fn door(&self) -> Entity {
        let world = self.server.world();
        let at = self.spawn.step(Dir::East);
        world
            .resource::<SpatialIndex>()
            .on_tile(at)
            .iter()
            .copied()
            .find(|&e| world.get::<Door>(e).is_some())
            .unwrap()
    }

    fn door_state(&self) -> DoorState {
        self.server.world().get::<Door>(self.door()).unwrap().state
    }

    fn name(&self, p: usize) -> String {
        self.server
            .world()
            .get::<Name>(self.players[p].mob)
            .unwrap()
            .to_string()
    }
}

#[test]
fn messenger_and_id_slot() {
    let mut s = Station::start("messenger", 2);
    let (a, b) = (0, 1);
    let (name_a, name_b) = (s.name(a), s.name(b));
    // `post_equip`: both PDAs are imprinted and named after their owner.
    let pda_b = s.pda(b);
    assert_eq!(
        s.server.world().get::<DmName>(pda_b).unwrap().name,
        format!("КПК «{name_b}» (Assistant)")
    );

    // A: the PDA turns on, NtosMain lists the programs; the messenger shows B.
    let window = s.open_pda(a);
    s.wait_chat(a, "включая");
    let NtosScreen::Main(main) = s.ntos(a, window).screen else {
        panic!("main menu first");
    };
    assert!(main.programs.iter().any(|p| p.name == "nt_messenger"));
    assert_eq!(main.login.name.as_deref(), Some(name_a.as_str()));
    s.act(a, window, PC_RUNPROGRAM, &[("name", "nt_messenger")]);
    let m = s.wait_messenger(a, window, "the messenger lists B", |m| {
        m.messengers.iter().any(|p| p.name == name_b)
    });
    assert_eq!(m.owner.as_ref().unwrap().name, name_a);
    let b_ref = m.messengers[0].messenger_ref.clone();

    // A -> B, then a second message inside the one-second cooldown, dropped.
    s.act(
        a,
        window,
        PDA_SEND_MESSAGE,
        &[("ref", &b_ref), ("message", "Привет, Боб")],
    );
    s.act(
        a,
        window,
        PDA_SEND_MESSAGE,
        &[("ref", &b_ref), ("message", "спам")],
    );
    s.wait_chat(a, "PDA message sent to");
    s.wait_chat(
        b,
        &format!("PDA message from {name_a} (Assistant), \"Привет, Боб\""),
    );
    s.wait_chat(b, "*beep*");
    s.run_for(0.3);
    let world = s.server.world();
    let chats = &world.get::<Messenger>(pda_b).unwrap().chats;
    assert_eq!(chats.len(), 1);
    assert_eq!(
        chats[0].messages.len(),
        1,
        "the spam inside the cooldown is dropped"
    );
    assert!(s.received(b).chat.iter().all(|l| !l.contains("спам")));
    // A's screen switched to the new chat.
    let m = s.wait_messenger(a, window, "A sees its chat", |m| m.open_chat.is_some());
    assert_eq!(m.saved_chats[0].messages[0].message, "Привет, Боб");

    // B opens the messenger: one unread chat; opening it shows the message.
    let window_b = s.open_pda(b);
    s.act(b, window_b, PC_RUNPROGRAM, &[("name", "nt_messenger")]);
    let m = s.wait_messenger(b, window_b, "B has a chat", |m| !m.saved_chats.is_empty());
    let chat = m.saved_chats[0].clone();
    assert_eq!(chat.unread_messages, 1);
    assert_eq!(chat.recipient.name, name_a);
    s.act(b, window_b, PDA_VIEW_MESSAGES, &[("ref", &chat.chat_ref)]);
    let m = s.wait_messenger(b, window_b, "B views the chat", |m| {
        m.open_chat.as_deref() == Some(chat.chat_ref.as_str())
    });
    let shown = m.chat(&chat.chat_ref).unwrap();
    assert_eq!(shown.messages[0].message, "Привет, Боб");
    assert!(!shown.messages[0].outgoing);
    // A forged ref to some entity is no messenger.
    s.act(b, window_b, PDA_VIEW_MESSAGES, &[("ref", "pda:1")]);
    s.run_for(0.2);
    assert_eq!(
        s.server.world().get::<Messenger>(pda_b).unwrap().viewing,
        ss13_game::modpc::programs::messenger::ViewRef::parse(&chat.chat_ref)
    );

    // A: the PDA back on the belt, the ID out of its slot and into the PDA.
    let pda_a = s.pda(a);
    s.slot(a, InventorySlot::Belt);
    s.run_until("the PDA is on the belt", |s| {
        s.inventory(a).belt == Some(pda_a)
    });
    s.slot(a, InventorySlot::Id);
    s.run_until("the ID is in hand", |s| {
        s.inventory(a).id.is_none() && s.inventory(a).hands[0].is_some()
    });
    s.slot(a, InventorySlot::Belt);
    s.run_until("the ID is in the PDA", |s| {
        s.server
            .world()
            .get::<ModularComputer>(pda_a)
            .unwrap()
            .stored_id
            .is_some()
    });
    s.wait_chat(a, "в слот карты");
    assert!(s.inventory(a).hands.iter().all(Option::is_none));
    // `GetID` resolves the PDA to the card itself, not a copy.
    let card = ss13_game::inventory::get_id(s.server.world(), pda_a).expect("the card");
    assert!(s.server.world().get::<IdCard>(pda_a).is_none());
    assert!(s.server.world().get::<IdCard>(card).is_some());

    // On the belt the PDA is no ID: the maintenance door stays shut.
    let start = s.pos(a);
    for _ in 0..testing::steps(1.5) {
        s.try_step(a, Dir::East);
        s.step();
    }
    assert_eq!(s.pos(a), start);
    assert_ne!(s.door_state(), DoorState::Open);

    // In the ID slot it is (tg `GetID`): the door opens.
    s.slot(a, InventorySlot::Belt);
    s.run_until("the PDA is in hand", |s| {
        s.inventory(a).hands[0] == Some(pda_a)
    });
    s.slot(a, InventorySlot::Id);
    s.run_until("the PDA is in the ID slot", |s| {
        s.inventory(a).id == Some(pda_a)
    });
    s.run_until("the door opens", |s| {
        s.try_step(a, Dir::East);
        s.door_state() == DoorState::Open
    });
}

#[test]
fn traitor_ringtone_opens_the_pda_uplink() {
    let mut s = Station::start("traitor", 1);
    let mob = s.players[0].mob;
    let pda = s.pda(0);
    assert!(add_traitor(s.server.world_mut(), mob));
    let world = s.server.world();
    let traitor = world.get::<Traitor>(mob).unwrap();
    // `get_uplink_location(UPLINK_PDA)`: the PDA, with a "123 Альфа" code.
    assert_eq!(traitor.uplink, Some(pda));
    let code = traitor.code.clone().unwrap();
    let (number, word) = code.split_once(' ').unwrap();
    assert!((100..=999).contains(&number.parse::<u32>().unwrap()));
    assert!(["Альфа", "Браво"].contains(&word));
    assert!(traitor.unlock_text.contains("в настройках рингтона"));

    let window = s.open_pda(0);
    s.act(0, window, PC_RUNPROGRAM, &[("name", "nt_messenger")]);
    s.wait_messenger(0, window, "the messenger opens", |_| true);
    // A wrong code is just a ringtone.
    s.act(0, window, PDA_RING_SET, &[("ringtone", "999 Зулу")]);
    s.wait_messenger(0, window, "the ringtone changes", |m| {
        m.ringtone == "999 Зулу"
    });
    assert!(s.server.world().get::<Uplink>(pda).unwrap().locked);
    // The code, in any case, opens the uplink and does not become the ringtone.
    s.act(
        0,
        window,
        PDA_RING_SET,
        &[("ringtone", &code.to_uppercase())],
    );
    s.wait_window(0, UplinkUi::NAME);
    s.wait_chat(0, "The computer softly beeps.");
    assert!(!s.server.world().get::<Uplink>(pda).unwrap().locked);
    assert_eq!(
        s.server.world().get::<Messenger>(pda).unwrap().ringtone,
        "999 Зулу"
    );
}
