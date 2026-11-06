//! Offering and surrender over real WebTransport on 127.0.0.1: A offers its card to B with
//! ctrl+shift, B clicks the offer alert and holds the card; B's `*surrender` paralyzes B.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::breath::{AlertCategory, Alerts};
use ss13_game::buffs::Offering;
use ss13_game::hud::{ALERT_CLICK, AlertHudData, AlertHudUi, alert_click_name};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::status_effects::StatusEffects;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1,
        "inverse_chem": null, "inverse_chem_val": 0.25, "purge_multiplier": 1 } },
    "/datum/reagent/medicine": { "parent": "/datum/reagent", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent/medicine" } } },
    "/datum/reagent/medicine/mannitol": { "parent": "/datum/reagent/medicine",
        "declared_vars": { "name": "Mannitol" } },
    "/datum/reagent/medicine/neurine": { "parent": "/datum/reagent/medicine",
        "declared_vars": { "name": "Neurine" } },
    "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
        "H_ion_release": 0.01, "determin_ph_range": 4, "is_cold_recipe": 0,
        "mix_message": "The solution begins to bubble.", "mob_react": 1, "optimal_ph_max": 9,
        "optimal_ph_min": 5, "optimal_temp": 500, "overheat_temp": 900, "ph_exponent_factor": 2,
        "purity_min": 0.15, "rate_up_lim": 30, "reaction_flags": 0,
        "required_catalysts": { "list": [] }, "required_container": null,
        "required_container_accepts_subtypes": 0, "required_reagents": { "list": [] },
        "required_temp": 100, "results": { "list": [] }, "temp_exponent_factor": 2,
        "thermic_constant": 0 } },
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
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
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

/// A and B spawn side by side.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfffw
wsfsw
wfffw
wwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("buffs_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let dir = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    dir
}

#[derive(Resource, Default)]
struct Received {
    alert_window: Option<u32>,
}

fn record(mut ui: MessageReader<UiMessageReceived>, mut received: ResMut<Received>) {
    for UiMessageReceived(message) in ui.read() {
        if let UiServerMessage::Open {
            window, interface, ..
        } = message
            && interface == AlertHudUi::NAME
        {
            received.alert_window = Some(*window);
        }
    }
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(2, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        let (a, b) = (s.players[0].mob, s.players[1].mob);
        let world = s.server.world_mut();
        world.entity_mut(a).insert(TilePos::new(2, 3, 1));
        world.entity_mut(b).insert(TilePos::new(3, 3, 1));
        Self(s)
    }

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn holds(&self, mob: Entity, item: Entity) -> bool {
        self.get::<Inventory>(mob).hands.contains(&Some(item))
    }
}

#[test]
fn offer_take_and_surrender() {
    let mut s = Station::start("offer_take_and_surrender");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    // A holds a card in its active hand.
    let card = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        "/obj/item/card/id/advanced",
        TilePos::new(2, 2, 1),
    )
    .unwrap();
    {
        let world = s.server.world_mut();
        world
            .entity_mut(card)
            .remove::<TilePos>()
            .insert(CarriedBy(a));
        let mut inventory = world.get_mut::<Inventory>(a).unwrap();
        inventory.active = 0;
        inventory.hands = [Some(card), None];
    }
    s.tick();

    // Ctrl+shift on B offers it: B gets the alert.
    let give = ClickModifiers {
        ctrl: true,
        shift: true,
        ..default()
    };
    s.click_with(0, b, give);
    s.run_until("B sees the offer", |s| {
        s.received(1).alert_window.is_some()
            && s.try_get::<Offering>(a).is_some()
            && s.get::<AlertHudData>(b)
                .alerts
                .iter()
                .any(|x| x.name.contains("предлагает"))
    });
    // Only B may take it.
    assert_eq!(s.get::<Offering>(a).takers, vec![b]);
    let index = s
        .get::<Alerts>(b)
        .0
        .iter()
        .position(|x| x.category == AlertCategory::Offer(a))
        .unwrap();
    let name = s.get::<AlertHudData>(b).alerts[index].name.clone();

    // A stale click (another name) does nothing.
    let window = s.received(1).alert_window.unwrap();
    let at = index.to_string();
    s.act(
        1,
        window,
        ALERT_CLICK,
        &[("index", at.as_str()), ("name", "что-то другое")],
    );
    for _ in 0..5 {
        s.tick();
    }
    assert!(s.holds(a, card));

    // The real click: B takes the card, the offer and its alert end.
    let window = s.received(1).alert_window.unwrap();
    s.act(
        1,
        window,
        ALERT_CLICK,
        &[("index", at.as_str()), ("name", alert_click_name(&name))],
    );
    s.run_until("B takes the card", |s| s.holds(b, card));
    assert!(!s.holds(a, card));
    assert_eq!(s.get::<CarriedBy>(card).0, b);
    s.run_until("the offer ends", |s| {
        s.try_get::<Offering>(a).is_none()
            && s.get::<Alerts>(b).get(AlertCategory::Offer(a)).is_none()
    });

    // B surrenders: 20 s of paralysis.
    s.send(1, ClientIntent::Say("*surrender".into()));
    s.run_until("B lies paralyzed", |s| {
        s.try_get::<StatusEffects>(b)
            .is_some_and(|e| e.paralyze > 15.0)
    });
}
