//! The ID console and crew records over real WebTransport on 127.0.0.1.
//!
//! A head of personnel puts her ID into the identification console, logs in to Plexagon Access
//! Management and gives the assistant's card (in the secondary slot) security access: the card
//! opens a security door. With the card in the assistant's PDA, an access change to the card
//! reaches the PDA without reinserting it. A security officer logs into the security records
//! console and adds a crime to the assistant's record (Arrest): the officer's security HUD
//! glasses show the wanted icon over the assistant, and the crew list reads the records.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::doors::{Door, DoorState};
use ss13_game::id_access::CardAccess;
use ss13_game::inventory::{IdCard, Inventory, get_id};
use ss13_game::jobs::fallback_jobs;
use ss13_game::modpc::pda::ModularComputer;
use ss13_game::modpc::{
    CardModData, NtosData, NtosScreen, NtosUi, PC_RUNPROGRAM, PRG_ACCESS, PRG_AUTHENTICATE,
    PRG_INSERT_ALT_ID, PRG_REMOVE_ALT_ID,
};
use ss13_game::records::manifest::Manifest;
use ss13_game::records::{
    ADD_CRIME, DataHudData, DataHudUi, LOGIN, SecurityRecordsData, SecurityRecordsUi, WANTED_ARREST,
};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmName;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, Job, JobsData, OutfitSlot};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::UiInterface;
use ss13_world::SpatialIndex;

const PDA: &str = "/obj/item/modular_computer/pda/crew";
const SECHUD: &str = "/obj/item/clothing/glasses/hud/security/sunglasses";
const ID_CONSOLE: &str = "/obj/machinery/modular_computer/preset/id";
const RECORDS_CONSOLE: &str = "/obj/machinery/computer/records/security";

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
    "/obj/item/card/id/advanced/platinum": { "parent": "/obj/item/card/id/advanced" },
    "/obj/item/modular_computer": { "parent": "/obj/item", "declared_vars": {
        "name": "modular microcomputer" } },
    "/obj/item/modular_computer/pda": { "parent": "/obj/item/modular_computer", "declared_vars": {
        "name": "pda", "slot_flags": 768 } },
    "/obj/item/modular_computer/pda/crew": { "parent": "/obj/item/modular_computer/pda" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/glasses": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 64 } },
    "/obj/item/clothing/glasses/hud": { "parent": "/obj/item/clothing/glasses" },
    "/obj/item/clothing/glasses/hud/security": { "parent": "/obj/item/clothing/glasses/hud" },
    "/obj/item/clothing/glasses/hud/security/sunglasses": { "parent": "/obj/item/clothing/glasses/hud/security" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1 } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer" } },
    "/obj/machinery/computer/records": { "parent": "/obj/machinery/computer" },
    "/obj/machinery/computer/records/security": { "parent": "/obj/machinery/computer/records",
        "declared_vars": { "name": "security records console" } },
    "/obj/machinery/modular_computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer" } },
    "/obj/machinery/modular_computer/preset": { "parent": "/obj/machinery/modular_computer" },
    "/obj/machinery/modular_computer/preset/id": { "parent": "/obj/machinery/modular_computer/preset",
        "declared_vars": { "name": "identification console" } },
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

/// The ID console `I`, the records console `R`, a security door `S` and a medical door `M`.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"I" = (/obj/machinery/modular_computer/preset/id,/turf/open/floor,/area/space)
"R" = (/obj/machinery/computer/records/security,/turf/open/floor,/area/space)
"S" = (/obj/machinery/door/airlock{req_access = list("security")},/turf/open/floor,/area/space)
"M" = (/obj/machinery/door/airlock{req_access = list("medical")},/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwww
wffffffffffw
wffIfffRfffw
wfffffffffSw
wsfffffffffw
wffffffffffw
wfffffffffMw
wwwwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 12] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("a.dmi", "closed"),
    ("c.dmi", "computer"),
    ("icons/mob/huds/hud.dmi", "hudno_id"),
    ("icons/mob/huds/hud.dmi", "hudassistant"),
    ("icons/mob/huds/hud.dmi", "hudsecurityofficer"),
    ("icons/mob/huds/hud.dmi", "hudheadofpersonnel"),
    ("icons/mob/huds/hud.dmi", "hudwanted"),
];

const ASSISTANT: usize = 0;
const HOP: usize = 1;
const OFFICER: usize = 2;

/// The assistant (PDA on the belt), a head of personnel and a security officer with security
/// HUD glasses.
fn fixture_jobs() -> JobsData {
    let mut data = fallback_jobs();
    let assistant = data.jobs[0].clone();
    data.jobs[0]
        .outfit
        .as_mut()
        .unwrap()
        .items
        .push((OutfitSlot::Belt, PDA.into()));
    let job = |title: &str, assignment: &str, access: &[&str], sechud: &str| -> Job {
        let mut job = assistant.clone();
        job.title = title.into();
        job.path = format!("/datum/job/{}", title.to_lowercase().replace(' ', "_"));
        job.assignment = assignment.into();
        job.access = access.iter().map(|a| (*a).to_owned()).collect();
        job.sechud_icon_state = sechud.into();
        job
    };
    let mut hop = job(
        "Head of Personnel",
        "Глава персонала",
        &["change_ids", "hop", "command", "maint_tunnels"],
        "hudheadofpersonnel",
    );
    hop.template_access = vec!["captain".into(), "change_ids".into()];
    if let Some(outfit) = hop.outfit.as_mut() {
        for (slot, item) in &mut outfit.items {
            if *slot == OutfitSlot::Id {
                *item = "/obj/item/card/id/advanced/platinum".into();
            }
        }
    }
    let mut officer = job(
        "Security Officer",
        "Офицер",
        &["security", "brig", "maint_tunnels"],
        "hudsecurityofficer",
    );
    officer.template_access = vec!["captain".into(), "change_ids".into(), "hos".into()];
    officer
        .outfit
        .as_mut()
        .unwrap()
        .items
        .push((OutfitSlot::Glasses, SECHUD.into()));
    data.jobs.push(hop);
    data.jobs.push(officer);
    data.departments[0].jobs = vec![0, 1, 2];
    data
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("records_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = fixture_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/door/airlock",
        ID_CONSOLE,
        RECORDS_CONSOLE,
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
    dir
}

/// Windows opened (id, interface), the latest data per window, chat lines.
#[derive(Resource, Default)]
struct Received {
    windows: Vec<(u32, String)>,
    data: Vec<(u32, Vec<u8>)>,
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
                data,
                ..
            } => {
                received.windows.push((*window, interface.clone()));
                received.data.push((*window, data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => received.data.push((*window, data.clone())),
            _ => {}
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
    fn start(test: &str, clients: usize) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(clients, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    /// Gives player `p` a body of `job` on `pos` (tg `equip_rank` after a latejoin).
    fn join_as(&mut self, p: usize, job: usize, pos: TilePos) {
        let link = self.players[p].link;
        let world = self.server.world_mut();
        let mob = ss13_game::jobs::join(world, link, job, true);
        world.entity_mut(mob).insert(pos);
        self.players[p].mob = mob;
        self.players[p].seq = 0;
        self.run_for(0.3);
    }

    fn slot(&mut self, p: usize, slot: InventorySlot) {
        self.send(p, ClientIntent::ClickSlot(slot));
    }

    /// A click once `entity` is replicated to `p`.
    fn click(&mut self, p: usize, entity: Entity) {
        self.run_until("replicated to the player", |s| {
            s.net_id(p, entity).is_some()
        });
        self.0.click(p, entity);
    }

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn wait_window(&mut self, p: usize, interface: &str) -> u32 {
        self.run_until(&format!("{interface} opens"), |s| {
            s.received(p).windows.iter().any(|(_, i)| i == interface)
        });
        self.received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i)| i == interface)
            .unwrap()
            .0
    }

    fn latest<T: serde::de::DeserializeOwned>(&self, p: usize, window: u32) -> Option<T> {
        let (_, data) = self
            .received(p)
            .data
            .iter()
            .rev()
            .find(|(w, d)| *w == window && !d.is_empty())?;
        ss13_tgui::decode(data).ok()
    }

    fn card_screen(&self, p: usize, window: u32) -> Option<CardModData> {
        match self.latest::<NtosData>(p, window)?.screen {
            NtosScreen::Card(c) => Some(*c),
            _ => None,
        }
    }

    fn inventory(&self, p: usize) -> Inventory {
        self.server
            .world()
            .get::<Inventory>(self.players[p].mob)
            .unwrap()
            .clone()
    }

    fn find(&self, path: &str) -> Entity {
        let world = self.server.world();
        let mut q = world
            .try_query::<(Entity, &ss13_mapload::construct::DmObj)>()
            .unwrap();
        q.iter(world).find(|(_, o)| o.path == path).unwrap().0
    }

    fn pos_of(&self, e: Entity) -> TilePos {
        *self.server.world().get::<TilePos>(e).unwrap()
    }

    fn door_at(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Door>(e).is_some())
            .unwrap()
    }

    fn door_state(&self, door: Entity) -> DoorState {
        self.server.world().get::<Door>(door).unwrap().state
    }

    /// Takes the worn ID into the empty active hand.
    fn id_to_hand(&mut self, p: usize) -> Entity {
        let card = self.inventory(p).id.expect("a worn ID");
        self.slot(p, InventorySlot::Id);
        self.run_until("the ID is in hand", |s| {
            s.inventory(p).hands.contains(&Some(card))
        });
        card
    }
}

#[test]
fn hop_gives_access_and_security_sets_arrest() {
    let mut s = Station::start("id_console", 3);
    let (a, h, o) = (0, 1, 2);
    let console = s.find(ID_CONSOLE);
    let records = s.find(RECORDS_CONSOLE);
    let at = s.pos_of(console);
    // The bodies of the first join go; each player's record comes from the job below.
    s.server
        .world_mut()
        .resource_mut::<Manifest>()
        .general
        .clear();
    s.join_as(h, HOP, at.step(Dir::West));
    s.join_as(a, ASSISTANT, at.step(Dir::East));
    let rec = s.pos_of(records);
    s.join_as(o, OFFICER, rec.step(Dir::South));

    // Every crew member has a record; the crew list reads them.
    let manifest = s.server.world().resource::<Manifest>();
    assert_eq!(manifest.general.len(), 3, "one record per joined body");
    let assistant_name = s
        .server
        .world()
        .get::<Name>(s.players[a].mob)
        .unwrap()
        .to_string();
    let record = manifest
        .find(&assistant_name)
        .expect("the assistant's record");
    assert_eq!(record.rank, "Assistant");
    assert_eq!(record.fingerprint.len(), 32);

    // The HoP: her ID into the console (left click with it), then the window with an empty hand.
    let hop_card = s.id_to_hand(h);
    s.click(h, console);
    s.run_until("the HoP's ID is in the main slot", |s| {
        s.server
            .world()
            .get::<ModularComputer>(console)
            .unwrap()
            .stored_id
            == Some(hop_card)
    });
    s.click(h, console);
    let hop_window = s.wait_window(h, NtosUi::NAME);
    s.act(
        h,
        hop_window,
        PC_RUNPROGRAM,
        &[("name", "plexagonidwriter")],
    );
    s.run_until("the ID writer runs", |s| {
        s.card_screen(h, hop_window).is_some()
    });
    s.act(h, hop_window, PRG_AUTHENTICATE, &[]);
    s.run_until("the HoP is logged in", |s| {
        s.card_screen(h, hop_window)
            .is_some_and(|c| c.authed_user.is_some() && !c.regions.is_empty())
    });

    // The assistant: the window first, then the ID into the secondary slot from the hand.
    s.click(a, console);
    let a_window = s.wait_window(a, NtosUi::NAME);
    let card = s.id_to_hand(a);
    s.act(a, a_window, PRG_INSERT_ALT_ID, &[]);
    s.run_until("the assistant's ID is in the secondary slot", |s| {
        s.server
            .world()
            .get::<ModularComputer>(console)
            .unwrap()
            .alt_stored_id
            == Some(card)
    });

    // Access outside every region is refused; security access goes on in a common slot.
    s.act(
        h,
        hop_window,
        PRG_ACCESS,
        &[("access_target", "cent_captain")],
    );
    s.act(h, hop_window, PRG_ACCESS, &[("access_target", "security")]);
    s.run_until("the card has security access", |s| {
        s.server
            .world()
            .get::<IdCard>(card)
            .unwrap()
            .access
            .contains(&"security".to_owned())
    });
    let world = s.server.world();
    assert!(
        !world
            .get::<IdCard>(card)
            .unwrap()
            .access
            .contains(&"cent_captain".to_owned())
    );
    let data = world.get::<CardAccess>(card).unwrap();
    assert_eq!(data.wildcard_slots[0].usage, ["security"]);
    s.run_until("the window shows it", |s| {
        s.card_screen(a, a_window).is_some_and(|c| {
            c.modified_card
                .is_some_and(|m| m.access_on_card.contains(&"security".to_owned()))
        })
    });

    // Back in the assistant's hand; the security door opens for it.
    s.act(a, a_window, PRG_REMOVE_ALT_ID, &[]);
    s.run_until("the ID is back in hand", |s| {
        s.inventory(a).hands.contains(&Some(card))
    });
    let security_door = s.door_at(s.find_door_pos("security"));
    let door_pos = s.pos_of(security_door);
    {
        let mob = s.players[a].mob;
        s.server
            .world_mut()
            .entity_mut(mob)
            .insert(door_pos.step(Dir::West));
    }
    s.run_until("the security door opens", |s| {
        s.try_step(a, Dir::East);
        s.door_state(security_door) == DoorState::Open
    });

    // The card into the PDA, the PDA into the ID slot: a later change to the card reaches the
    // PDA at once (no copy to refresh, no reinserting).
    let pda = s
        .inventory(a)
        .belt
        .filter(|&b| s.server.world().get::<ModularComputer>(b).is_some())
        .expect("the PDA on the belt");
    s.slot(a, InventorySlot::Belt);
    s.run_until("the ID is in the PDA", |s| {
        s.server
            .world()
            .get::<ModularComputer>(pda)
            .unwrap()
            .stored_id
            == Some(card)
    });
    s.slot(a, InventorySlot::Belt);
    s.run_until("the PDA is in hand", |s| {
        s.inventory(a).hands.contains(&Some(pda))
    });
    s.slot(a, InventorySlot::Id);
    s.run_until("the PDA is in the ID slot", |s| {
        s.inventory(a).id == Some(pda)
    });
    let medical_door = s.door_at(s.find_door_pos("medical"));
    let medical_pos = s.pos_of(medical_door);
    {
        let mob = s.players[a].mob;
        s.server
            .world_mut()
            .entity_mut(mob)
            .insert(medical_pos.step(Dir::West));
    }
    for _ in 0..testing::steps(1.0) {
        s.try_step(a, Dir::East);
        s.step();
    }
    assert_ne!(
        s.door_state(medical_door),
        DoorState::Open,
        "no medical access yet"
    );
    s.server
        .world_mut()
        .get_mut::<IdCard>(card)
        .unwrap()
        .access
        .push("medical".into());
    assert_eq!(get_id(s.server.world(), pda), Some(card));
    s.run_until("the medical door opens through the PDA", |s| {
        s.try_step(a, Dir::East);
        s.door_state(medical_door) == DoorState::Open
    });

    // The officer logs into the security records console and adds a crime: Arrest.
    let hud_window = s.wait_window(o, DataHudUi::NAME);
    s.click(o, records);
    let sec_window = s.wait_window(o, SecurityRecordsUi::NAME);
    s.act(o, sec_window, LOGIN, &[]);
    s.run_until("logged in", |s| {
        s.latest::<SecurityRecordsData>(o, sec_window)
            .is_some_and(|d| d.authenticated && d.records.len() == 3)
    });
    let crew_ref = s
        .latest::<SecurityRecordsData>(o, sec_window)
        .unwrap()
        .records
        .iter()
        .find(|r| r.name == assistant_name)
        .unwrap()
        .crew_ref
        .clone();
    s.act(
        o,
        sec_window,
        ADD_CRIME,
        &[
            ("crew_ref", &crew_ref),
            ("name", "Кража"),
            ("details", "Украл ID"),
            ("fine", "0"),
        ],
    );
    s.run_until("the record says Arrest", |s| {
        s.server
            .world()
            .resource::<Manifest>()
            .find(&assistant_name)
            .is_some_and(|r| r.wanted_status == WANTED_ARREST && r.crimes.len() == 1)
    });
    let crime = s
        .server
        .world()
        .resource::<Manifest>()
        .find(&assistant_name)
        .unwrap()
        .crimes[0]
        .clone();
    assert_eq!(
        crime.author,
        s.server
            .world()
            .get::<Name>(s.players[o].mob)
            .unwrap()
            .to_string()
    );

    // The officer's glasses show the wanted icon over the assistant, by her net id.
    let wanted = s
        .server
        .world()
        .resource::<ss13_mapload::fidelity::IconIndex>()
        .get("icons/mob/huds/hud.dmi", "hudwanted")
        .unwrap();
    let a_mob = s.players[a].mob;
    s.run_until("the sechud shows the wanted icon", |s| {
        let Some(net) = s.net_id(o, a_mob) else {
            return false;
        };
        s.latest::<DataHudData>(o, hud_window).is_some_and(|d| {
            d.entries
                .iter()
                .any(|e| e.net_id == net.0 && e.icons.iter().any(|i| i.icon_state == wanted))
        })
    });
    // The assistant wears no HUD glasses: no images for her.
    assert!(
        s.received(a)
            .windows
            .iter()
            .all(|(_, i)| i != DataHudUi::NAME)
    );
    let _ = s.server.world().get::<DmName>(console);
}

impl Station {
    /// The tile of the door whose `req_access` is `access`.
    fn find_door_pos(&self, access: &str) -> TilePos {
        let world = self.server.world();
        let mut q = world.try_query::<(Entity, &Door, &TilePos)>().unwrap();
        q.iter(world)
            .find(|(_, d, _)| d.req_access.iter().any(|a| a == access))
            .map(|(_, _, p)| *p)
            .unwrap()
    }
}
