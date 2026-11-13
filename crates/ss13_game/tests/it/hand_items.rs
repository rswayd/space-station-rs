//! Hand gestures over real WebTransport on 127.0.0.1 (`ss13_game::devices::hand_items`): A's
//! `*slap` offered with ctrl+shift is a high five B takes from the alert, A's `*kiss` blown at
//! B is a harmless kiss with its moodlet, A's `*noogie` on B held in an aggressive grab hurts
//! B's head after the wind-up.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::breath::{AlertCategory, Alerts};
use ss13_game::buffs::{OfferKind, Offering};
use ss13_game::devices::hand_items::{Gesture, HandItem};
use ss13_game::health::Health;
use ss13_game::hud::{ALERT_CLICK, AlertHudData, AlertHudUi, alert_click_name};
use ss13_game::inventory::Inventory;
use ss13_game::mood::Mood;
use ss13_game::pull::{GrabState, Pulling};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::zone::BodyZone;
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
    "/obj/item/hand_item": { "parent": "/obj/item" },
    "/obj/item/hand_item/slapper": { "parent": "/obj/item/hand_item", "declared_vars": { "name": "slapper" } },
    "/obj/item/hand_item/noogie": { "parent": "/obj/item/hand_item", "declared_vars": { "name": "noogie" } },
    "/obj/item/hand_item/kisser": { "parent": "/obj/item/hand_item", "declared_vars": { "name": "kiss" } },
    "/obj/projectile": { "parent": "/obj" },
    "/obj/projectile/kiss": { "parent": "/obj/projectile", "declared_vars": { "name": "kiss" } },
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
        .join("hand_items_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/obj/item/hand_item/slapper",
        "/obj/item/hand_item/noogie",
        "/obj/item/hand_item/kisser",
        "/obj/projectile/kiss",
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
    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

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

    /// The hand holding a gesture of `kind`.
    fn gesture_hand(&self, mob: Entity, kind: fn(Gesture) -> bool) -> Option<usize> {
        let hands = self.get::<Inventory>(mob).hands;
        (0..2).find(|&h| {
            hands[h].is_some_and(|i| self.try_get::<HandItem>(i).is_some_and(|g| kind(g.kind)))
        })
    }

    fn mood(&self, mob: Entity, category: &str) -> bool {
        self.get::<Mood>(mob).get(category).is_some()
    }

    /// `p` makes the gesture of `kind` with `*key` and holds it in its active hand.
    fn make(&mut self, p: usize, key: &str, kind: fn(Gesture) -> bool) {
        let mob = self.players[p].mob;
        self.send(p, ClientIntent::Say(format!("*{key}")));
        self.run_until(&format!("the {key} gesture"), |s| {
            s.gesture_hand(mob, kind).is_some()
        });
        let hand = self.gesture_hand(mob, kind).unwrap();
        self.server
            .world_mut()
            .get_mut::<Inventory>(mob)
            .unwrap()
            .active = hand;
    }
}

#[test]
fn high_five_kiss_and_noogie() {
    let mut s = Station::start("high_five_kiss_and_noogie");
    let (a, b) = (s.players[0].mob, s.players[1].mob);

    // A's slapper offered to B is a high five.
    s.make(0, "slap", |g| g == Gesture::Slapper);
    let give = ClickModifiers {
        ctrl: true,
        shift: true,
        ..default()
    };
    s.click_with(0, b, give);
    s.run_until("B sees the high five", |s| {
        s.received(1).alert_window.is_some()
            && s.try_get::<Offering>(a)
                .is_some_and(|o| o.kind == OfferKind::HighFive)
            && s.get::<AlertHudData>(b)
                .alerts
                .iter()
                .any(|x| x.name.contains("пятюню"))
    });
    let index = s
        .get::<Alerts>(b)
        .0
        .iter()
        .position(|x| x.category == AlertCategory::Offer(a))
        .unwrap();
    let name = s.get::<AlertHudData>(b).alerts[index].name.clone();
    let window = s.received(1).alert_window.unwrap();
    let at = index.to_string();
    s.act(
        1,
        window,
        ALERT_CLICK,
        &[("index", at.as_str()), ("name", alert_click_name(&name))],
    );
    s.run_until("A and B high-five", |s| {
        s.mood(a, "high-five") && s.mood(b, "high-five") && s.try_get::<Offering>(a).is_none()
    });
    // The slapper is still A's hand.
    assert!(s.gesture_hand(a, |g| g == Gesture::Slapper).is_some());

    // A blows B a kiss point blank: harmless, a moodlet naming A, the hand is spent.
    s.make(0, "kiss", |g| matches!(g, Gesture::Kisser(_)));
    s.click(0, b);
    s.run_until("B is kissed", |s| s.mood(b, "kiss"));
    assert!(
        s.gesture_hand(a, |g| matches!(g, Gesture::Kisser(_)))
            .is_none()
    );
    assert_eq!(s.get::<Health>(b).part(BodyZone::Head).brute, 0.0);

    // A noogies B held in an aggressive grab: after 1.5 s the moodlet, then head damage.
    s.make(0, "noogie", |g| g == Gesture::Noogie);
    s.server
        .world_mut()
        .entity_mut(a)
        .insert((Pulling(b), GrabState::Aggressive));
    s.click(0, b);
    s.run_until("B gets a noogie", |s| {
        s.mood(b, "noogie") && s.get::<Health>(b).part(BodyZone::Head).brute > 0.0
    });
}
