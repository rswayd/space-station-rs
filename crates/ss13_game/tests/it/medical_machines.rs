//! Medical machines over real WebTransport on 127.0.0.1: the occupant of a syndicate sleeper
//! opens its window from inside (a plain sleeper's occupant cannot); the medical kiosk opens for
//! its patient and forgets them; the limb grower prints a left arm from synthflesh.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::{Chem, Reagents};
use ss13_game::closets::InCloset;
use ss13_game::combat::NextMove;
use ss13_game::medical::kiosk::MedicalKiosk;
use ss13_game::medical::limbgrower::Limbgrower;
use ss13_game::medical::sleeper::Sleeper;
use ss13_game::science::techweb::ResearchDb;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::research::{DesignDef, ResearchData, build_type};
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::limbgrower::LimbgrowerUi;
use ss13_tgui::interfaces::medical_kiosk::{MedicalKioskData, MedicalKioskUi};
use ss13_tgui::interfaces::sleeper::SleeperUi;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1, "inverse_chem": null,
        "inverse_chem_val": 0.25, "purge_multiplier": 1, "default_container": null } },
    "/datum/reagent/medicine": { "parent": "/datum/reagent" },
    "/datum/reagent/medicine/c2": { "parent": "/datum/reagent/medicine" },
    "/datum/reagent/medicine/c2/synthflesh": { "parent": "/datum/reagent/medicine/c2",
        "declared_vars": { "name": "Synthflesh" } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "use_power": 1,
        "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "density": 1, "anchored": 1 } },
    "/obj/machinery/sleeper": { "parent": "/obj/machinery", "vars": { "name": "sleeper" },
        "declared_vars": { "name": "sleeper", "icon": { "resource": "m.dmi" },
        "icon_state": "sleeper", "density": 0 } },
    "/obj/machinery/sleeper/syndie": { "parent": "/obj/machinery/sleeper",
        "declared_vars": { "icon_state": "sleeper_s" } },
    "/obj/machinery/medical_kiosk": { "parent": "/obj/machinery", "vars": {
        "name": "medical kiosk" }, "declared_vars": { "name": "medical kiosk",
        "icon": { "resource": "m.dmi" }, "icon_state": "kiosk" } },
    "/obj/machinery/limbgrower": { "parent": "/obj/machinery", "vars": {
        "name": "limb grower" }, "declared_vars": { "name": "limb grower",
        "icon": { "resource": "m.dmi" }, "icon_state": "limbgrower_idleoff" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "tool_behaviour": null,
        "toolspeed": 1 } },
    "/obj/item/bodypart": { "parent": "/obj/item" },
    "/obj/item/bodypart/arm": { "parent": "/obj/item/bodypart" },
    "/obj/item/bodypart/arm/left": { "parent": "/obj/item/bodypart/arm", "vars": {
        "name": "left arm" }, "declared_vars": { "name": "left arm" } },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0 } },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (3,3) between the kiosk (west) and the
/// limb grower (east); the syndicate sleeper south of it, a plain one south-west.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"K" = (/obj/machinery/medical_kiosk,/turf/open/floor,/area/space)
"L" = (/obj/machinery/limbgrower,/turf/open/floor,/area/space)
"S" = (/obj/machinery/sleeper/syndie,/turf/open/floor,/area/space)
"P" = (/obj/machinery/sleeper,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wKsLw
wPSfw
wwwww
"}
"##;

const KIOSK: TilePos = TilePos::new(2, 3, 1);
const GROWER: TilePos = TilePos::new(4, 3, 1);
const SYNDIE: TilePos = TilePos::new(3, 2, 1);
const PLAIN: TilePos = TilePos::new(2, 2, 1);
const SYNTHFLESH: &str = "/datum/reagent/medicine/c2/synthflesh";
const ARM: &str = "/obj/item/bodypart/arm/left";

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 9] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("m.dmi", "sleeper"),
    ("m.dmi", "sleeper_s"),
    ("m.dmi", "kiosk"),
    ("m.dmi", "limbgrower_idleoff"),
    ("i.dmi", "toy"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("medical_machines_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/medical_kiosk",
        "/obj/machinery/limbgrower",
        "/obj/machinery/sleeper",
        "/obj/machinery/sleeper/syndie",
        ARM,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    let mecha = ss13_mapload::mecha::resolve(&types);
    std::fs::write(dir.join(ss13_mapload::mecha::MECHA_FILE), mecha.to_ron()).unwrap();
    dir
}

#[derive(Resource, Default)]
struct Received {
    windows: HashMap<u32, (String, Vec<u8>)>,
}

fn record(mut ui: MessageReader<UiMessageReceived>, mut received: ResMut<Received>) {
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } => {
                received
                    .windows
                    .insert(*window, (interface.clone(), data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some(w) = received.windows.get_mut(window) {
                    w.1 = data.clone();
                }
            }
            UiServerMessage::Close { window } => {
                received.windows.remove(window);
            }
            _ => {}
        }
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
}

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(1, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    fn click(&mut self, entity: Entity) {
        let mob = self.players[0].mob;
        self.run_until("off cooldown", |s| {
            s.try_get::<NextMove>(mob).is_none_or(|n| s.now() >= n.0)
        });
        self.0.click(0, entity);
    }

    fn window_id(&self, name: &str) -> Option<u32> {
        let received = self.players[0].app.world().resource::<Received>();
        received
            .windows
            .iter()
            .filter(|(_, (n, _))| n == name)
            .map(|(id, _)| *id)
            .max()
    }

    fn data<T: UiInterface>(&self) -> Option<T::Data> {
        let received = self.players[0].app.world().resource::<Received>();
        let id = self.window_id(T::NAME)?;
        received
            .windows
            .get(&id)
            .map(|(_, data)| ss13_tgui::decode(data).unwrap())
    }

    fn machine<C: Component>(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<C>(e).is_some())
            .expect("the machine")
    }
}

#[test]
fn a_syndicate_sleeper_is_used_from_inside() {
    let mut s = Station::start("a_syndicate_sleeper_is_used_from_inside");
    s.without_mood();
    let mob = s.players[0].mob;
    // A plain sleeper first: its occupant's click is dropped (`notcontained_state`, no
    // `controls_inside`).
    let plain = s.machine::<Sleeper>(PLAIN);
    ss13_game::medical::sleeper::close(s.server.world_mut(), plain, Some(mob));
    s.run_until("inside the plain sleeper", |s| {
        s.try_get::<InCloset>(mob)
            .is_some_and(|c| c.closet == plain)
    });
    s.click(plain);
    s.run_for(1.0);
    assert_eq!(s.window_id(SleeperUi::NAME), None, "no window from inside");
    ss13_game::medical::sleeper::open(s.server.world_mut(), plain);

    let syndie = s.machine::<Sleeper>(SYNDIE);
    ss13_game::medical::sleeper::close(s.server.world_mut(), syndie, Some(mob));
    s.run_until("inside the syndicate sleeper", |s| {
        s.try_get::<InCloset>(mob)
            .is_some_and(|c| c.closet == syndie)
    });
    s.click(syndie);
    s.run_until("the window from inside", |s| {
        s.window_id(SleeperUi::NAME).is_some()
    });
}

#[test]
fn the_kiosk_scans_its_user_until_reset() {
    let mut s = Station::start("the_kiosk_scans_its_user_until_reset");
    let mob = s.players[0].mob;
    let kiosk = s.machine::<MedicalKiosk>(KIOSK);
    s.click(kiosk);
    s.run_until("the kiosk window", |s| {
        s.data::<MedicalKioskUi>()
            .is_some_and(|d: MedicalKioskData| !d.patient_name.is_empty())
    });
    assert_eq!(s.get::<MedicalKiosk>(kiosk).patient, Some(mob));
    let window = s.window_id(MedicalKioskUi::NAME).unwrap();
    s.act(0, window, "clearTarget", &[]);
    s.run_until("the patient forgotten", |s| {
        s.get::<MedicalKiosk>(kiosk).patient.is_none()
    });
}

#[test]
fn the_limb_grower_prints_an_arm_from_synthflesh() {
    let mut s = Station::start("the_limb_grower_prints_an_arm_from_synthflesh");
    let grower = s.machine::<Limbgrower>(GROWER);
    {
        let world = s.server.world_mut();
        world.insert_resource(ResearchDb::new(ResearchData {
            designs: vec![DesignDef {
                id: "/datum/design/leftarm".into(),
                name: "Left Arm".into(),
                build_type: build_type::LIMBGROWER,
                build_path: Some(ARM.into()),
                reagents: vec![(SYNTHFLESH.into(), 25.0)],
                category: vec!["initial".into(), "human".into()],
                ..default()
            }],
            ..default()
        }));
        let chem = world.resource::<Chem>().0.clone();
        let id = chem.id(SYNTHFLESH).unwrap();
        let _ = world
            .get_mut::<Reagents>(grower)
            .unwrap()
            .0
            .add_reagent(&chem, id, 40.0, 300.0, None, None, false, false);
    }
    s.click(grower);
    s.run_until("the grower window", |s| {
        s.window_id(LimbgrowerUi::NAME).is_some()
    });
    let window = s.window_id(LimbgrowerUi::NAME).unwrap();
    s.act(
        0,
        window,
        "make_limb",
        &[
            ("design_path", "/datum/design/leftarm"),
            ("active_tab", "human"),
        ],
    );
    s.run_until("busy", |s| s.get::<Limbgrower>(grower).busy);
    s.run_until("the arm on the grower's tile", |s| {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(GROWER)
            .iter()
            .any(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == ARM))
    });
    let chem = s.server.world().resource::<Chem>().0.clone();
    let left = s
        .get::<Reagents>(grower)
        .0
        .amount(chem.id(SYNTHFLESH).unwrap());
    assert!((left - 15.0).abs() < 1e-3, "25 u used: {left}");
    assert!(!s.get::<Limbgrower>(grower).busy);
}
