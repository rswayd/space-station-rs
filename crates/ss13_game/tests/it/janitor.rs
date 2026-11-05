//! Blood, cleaning and wet floors over real WebTransport on 127.0.0.1: a bleeding mob leaves
//! drips on its tile; water poured into a mop bucket wets a mop, which mops the drips away and
//! leaves the floor wet; walking over the wet floor is safe, running onto it slips (knockdown);
//! a space cleaner spray washes a pool of blood off a tile; soap scrubs one away.

use std::path::{Path, PathBuf};

use bevy::prelude::*;

use ss13_game::chem::Reagents;
use ss13_game::combat::NextMove;
use ss13_game::health::BodyPosition;
use ss13_game::inventory::Inventory;
use ss13_game::janitor::{BloodSplat, Soap, TURF_WET_WATER, WetFloor, add_splatter_floor};
use ss13_game::medical::wounds::{Limbs, WoundKind, apply_wound};
use ss13_game::status_effects::{Incap, StatusEffects};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::zone::BodyZone;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmItem, DmObj, Turf};
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::{Dir, TilePos};
use ss13_world::SpatialIndex;

/// tg's vars for what the test uses, trimmed to the ones read.
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
    "/datum/reagent/space_cleaner": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Space Cleaner", "color": "#A5F0EE", "chemical_flags": 400 } },
    "/datum/chemical_reaction": { "parent": "/datum" },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/decal": { "parent": "/obj/effect" },
    "/obj/effect/decal/cleanable": { "parent": "/obj/effect/decal" },
    "/obj/effect/decal/cleanable/blood": { "parent": "/obj/effect/decal/cleanable",
        "declared_vars": { "name": "pool of blood", "color": "#AA1010",
        "icon": { "resource": "icons/effects/blood.dmi" }, "icon_state": "floor1" } },
    "/obj/effect/decal/cleanable/blood/drip": { "parent": "/obj/effect/decal/cleanable/blood",
        "declared_vars": { "name": "drop of blood", "icon_state": "drip5" } },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "density": 1 } },
    "/obj/structure/mop_bucket": { "parent": "/obj/structure", "declared_vars": {
        "name": "mop bucket", "icon": { "resource": "icons/obj/service/janitor.dmi" },
        "icon_state": "mopbucket" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/mop": { "parent": "/obj/item", "declared_vars": { "name": "mop",
        "icon": { "resource": "icons/obj/service/janitor.dmi" }, "icon_state": "mop",
        "force": 8 } },
    "/obj/item/soap": { "parent": "/obj/item", "declared_vars": { "name": "soap" } },
    "/obj/item/reagent_containers": { "parent": "/obj/item", "declared_vars": {
        "name": "Container", "icon": { "resource": "c.dmi" },
        "amount_per_transfer_from_this": 5, "has_variable_transfer_amount": 1,
        "possible_transfer_amounts": { "list": [5, 10, 15, 20, 25, 30] }, "volume": 30,
        "initial_reagent_flags": 0, "list_reagents": null, "list_reagents_purity": null,
        "fill_icon_thresholds": null, "fill_icon_state": null,
        "fill_icon": { "resource": "f.dmi" } } },
    "/obj/item/reagent_containers/cup": { "parent": "/obj/item/reagent_containers",
        "declared_vars": { "amount_per_transfer_from_this": 10,
        "possible_transfer_amounts": { "list": [5, 10, 15, 20, 25, 30, 50] }, "volume": 50,
        "initial_reagent_flags": 60, "gulp_size": 5, "icon_state": "bottle" } },
    "/obj/item/reagent_containers/cup/bottle": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "name": "bottle" } },
    "/obj/item/reagent_containers/cup/bottle/water": {
        "parent": "/obj/item/reagent_containers/cup/bottle", "declared_vars": {
        "name": "water bottle",
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/water" }, 30] }] } } },
    "/obj/item/reagent_containers/spray": { "parent": "/obj/item/reagent_containers",
        "declared_vars": { "name": "spray bottle", "volume": 250,
        "possible_transfer_amounts": { "list": [5, 10] }, "initial_reagent_flags": 7,
        "icon_state": "sprayer" } },
    "/obj/item/reagent_containers/spray/cleaner": {
        "parent": "/obj/item/reagent_containers/spray", "declared_vars": {
        "name": "space cleaner", "volume": 100, "amount_per_transfer_from_this": 2,
        "possible_transfer_amounts": { "list": [2, 5] },
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/space_cleaner" }, 100] }] } } },
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
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The mob spawns on `s` (3,3); a water bottle `W`
/// north-west, a mop bucket `K` west, a mop `M` east, a space cleaner spray `C` south, soap `P`
/// south-east.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"W" = (/obj/item/reagent_containers/cup/bottle/water,/turf/open/floor,/area/space)
"K" = (/obj/structure/mop_bucket,/turf/open/floor,/area/space)
"M" = (/obj/item/mop,/turf/open/floor,/area/space)
"C" = (/obj/item/reagent_containers/spray/cleaner,/turf/open/floor,/area/space)
"P" = (/obj/item/soap,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffffffw
wWfffffw
wKsMfffw
wfCPfffw
wwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const WATER_BOTTLE: TilePos = TilePos::new(2, 4, 1);
const BUCKET: TilePos = TilePos::new(2, 3, 1);
const MOP: TilePos = TilePos::new(4, 3, 1);
const SPRAY: TilePos = TilePos::new(3, 2, 1);
const SOAP: TilePos = TilePos::new(4, 2, 1);
const NORTH_EAST: TilePos = TilePos::new(4, 4, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 22] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("c.dmi", "bottle"),
    ("c.dmi", "sprayer"),
    ("i.dmi", "toy"),
    ("icons/obj/service/janitor.dmi", "mop"),
    ("icons/obj/service/janitor.dmi", "mopbucket"),
    ("icons/obj/service/janitor.dmi", "mopbucket_water"),
    ("icons/effects/water.dmi", "wet_floor_static"),
    ("icons/effects/blood.dmi", "floor1"),
    ("icons/effects/blood.dmi", "floor2"),
    ("icons/effects/blood.dmi", "floor3"),
    ("icons/effects/blood.dmi", "floor4"),
    ("icons/effects/blood.dmi", "floor5"),
    ("icons/effects/blood.dmi", "floor6"),
    ("icons/effects/blood.dmi", "floor7"),
    ("icons/effects/blood.dmi", "drip1"),
    ("icons/effects/blood.dmi", "drip2"),
    ("icons/effects/blood.dmi", "drip3"),
    ("icons/effects/blood.dmi", "drip5"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("janitor_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/decal/cleanable/blood",
        "/obj/effect/decal/cleanable/blood/drip",
        "/obj/item/reagent_containers/cup/bottle/water",
        "/obj/item/reagent_containers/spray/cleaner",
        "/obj/structure/mop_bucket",
        "/obj/item/mop",
        "/obj/item/soap",
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

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(1, |_| {});
        Self(s)
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    /// Clicks `entity` once the click cooldown is over and the client has it.
    fn click_with(&mut self, entity: Entity, modifiers: ClickModifiers) {
        let mob = self.mob();
        self.run_until("off cooldown, replicated", |s| {
            s.now() >= s.get::<NextMove>(mob).0 && s.net_id(0, entity).is_some()
        });
        self.0.click_with(0, entity, modifiers);
    }

    fn click(&mut self, entity: Entity) {
        self.click_with(entity, ClickModifiers::default());
    }

    fn on_tile(&self, pos: TilePos, keep: impl Fn(&World, Entity) -> bool) -> Vec<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| keep(world, e))
            .collect()
    }

    fn item_on(&self, pos: TilePos) -> Entity {
        self.on_tile(pos, |w, e| w.get::<DmItem>(e).is_some())[0]
    }

    fn turf(&self, pos: TilePos) -> Entity {
        self.on_tile(pos, |w, e| w.get::<Turf>(e).is_some())[0]
    }

    fn blood_on(&self, pos: TilePos) -> usize {
        self.on_tile(pos, |w, e| w.get::<BloodSplat>(e).is_some())
            .len()
    }

    fn wet(&self, pos: TilePos) -> u8 {
        self.try_get::<WetFloor>(self.turf(pos))
            .map_or(0, WetFloor::highest)
    }

    fn volume(&self, e: Entity) -> f32 {
        self.get::<Reagents>(e).0.total_volume
    }

    fn take(&mut self, item: Entity) {
        let mob = self.mob();
        self.click(item);
        self.run_until("picked up", |s| {
            s.get::<Inventory>(mob).active_item() == Some(item)
        });
    }

    fn drop_active(&mut self) {
        let mob = self.mob();
        self.send(0, ClientIntent::DropActive);
        self.run_until("dropped", |s| {
            s.get::<Inventory>(mob).active_item().is_none()
        });
    }

    fn step_to(&mut self, dir: Dir, to: TilePos) {
        let mob = self.mob();
        self.run_until("stepped", |s| {
            s.try_step(0, dir);
            *s.get::<TilePos>(mob) == to
        });
    }

    fn knocked_down(&self) -> bool {
        self.try_get::<StatusEffects>(self.mob())
            .is_some_and(|e| e.remaining(Incap::Knockdown) > 0.0)
    }

    fn splatter(&mut self, pos: TilePos) -> Entity {
        add_splatter_floor(self.0.server.world_mut(), pos, false).expect("blood on the floor")
    }
}

#[test]
fn bleed_mop_slip_spray_soap() {
    let mut s = Station::start("bleed_mop_slip_spray_soap");
    s.0.without_mood();
    let a = s.mob();
    assert_eq!(*s.get::<TilePos>(a), SPAWN);

    // A severe slash on the chest bleeds onto the floor (`add_splatter_floor`, drips).
    {
        let world = s.0.server.world_mut();
        let mut limbs = world.get_mut::<Limbs>(a).unwrap();
        apply_wound(limbs.limb_mut(BodyZone::Chest), WoundKind::SlashSevere);
    }
    s.run_until("blood under the bleeding mob", |s| s.blood_on(SPAWN) > 0);
    {
        let world = s.0.server.world_mut();
        world
            .get_mut::<Limbs>(a)
            .unwrap()
            .limb_mut(BodyZone::Chest)
            .wounds
            .clear();
    }
    s.run_for(2.5);
    let drips = s.blood_on(SPAWN);
    assert!(drips > 0);

    // Water into the bucket, the bucket wets the mop (right click).
    let (bottle, bucket, mop) = (
        s.item_on(WATER_BOTTLE),
        s.on_tile(BUCKET, |w, e| w.get::<DmObj>(e).is_some())[0],
        s.item_on(MOP),
    );
    s.take(bottle);
    s.click(bucket);
    s.run_until("bucket filled", |s| s.volume(bucket) == 10.0);
    s.drop_active();
    s.take(mop);
    let right = ClickModifiers {
        right: true,
        ..Default::default()
    };
    s.click_with(bucket, right);
    s.run_until("mop wet", |s| s.volume(mop) == 10.0);
    assert_eq!(s.volume(bucket), 0.0);

    // Mopping the tile under the mob: 1.5 s, the blood is gone, the floor is wet, 1 u used.
    let spawn_turf = s.turf(SPAWN);
    s.click(spawn_turf);
    let started = s.now();
    s.run_until("blood mopped", |s| s.blood_on(SPAWN) == 0);
    assert!(s.now() - started >= 1.4, "mopspeed 1.5 s");
    assert_eq!(s.wet(SPAWN), TURF_WET_WATER);
    assert_eq!(s.volume(mop), 9.0);

    // Walking onto the wet tile is safe; running onto it slips.
    s.send(0, ClientIntent::ToggleMoveIntent);
    s.step_to(Dir::East, MOP);
    s.step_to(Dir::West, SPAWN);
    s.hold(1.0, "a walking mob does not slip", |s| !s.knocked_down());
    s.step_to(Dir::East, MOP);
    s.send(0, ClientIntent::ToggleMoveIntent);
    s.run_for(0.5);
    assert_eq!(s.wet(SPAWN), TURF_WET_WATER, "still wet");
    s.step_to(Dir::West, SPAWN);
    s.run_until("a running mob slips", |s| s.knocked_down());
    s.run_until("lying", |s| s.get::<BodyPosition>(a).lying());
    s.run_until("up again", |s| {
        !s.knocked_down() && !s.get::<BodyPosition>(a).lying()
    });

    // A space cleaner spray washes a pool of blood off an adjacent tile.
    s.drop_active();
    let spray = s.item_on(SPRAY);
    s.take(spray);
    s.splatter(NORTH_EAST);
    assert_eq!(s.blood_on(NORTH_EAST), 1);
    let ne_turf = s.turf(NORTH_EAST);
    s.click(ne_turf);
    s.run_until("sprayed clean", |s| s.blood_on(NORTH_EAST) == 0);
    assert_eq!(s.volume(spray), 98.0);

    // Soap scrubs the blood decal itself away after 3.5 s and loses a use.
    s.drop_active();
    let soap = s.item_on(SOAP);
    s.take(soap);
    let pool = s.splatter(NORTH_EAST);
    s.click(pool);
    let started = s.now();
    s.run_until("scrubbed", |s| s.blood_on(NORTH_EAST) == 0);
    assert!(s.now() - started >= 3.4, "cleanspeed 3.5 s");
    assert_eq!(s.get::<Soap>(soap).uses, 99);
}

/// tg `/atom/movable/screen/mov_intent`: a click on the HUD button toggles the move intent, and
/// the server's HUD data turns its icon into `walking` and back.
#[test]
fn move_intent_hud_button() {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Pointer, PointerClick};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    use ss13_game::hud::InteractHudData;
    use ss13_game::interact_client::{InteractHud, InteractHudPlugin};
    use ss13_game::janitor::Walking;

    let assets = fixture_assets("move_intent_hud_button");
    let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
    s.join_all(1, |client| {
        client
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ss13_render::MapInsets>()
            .add_plugins(InteractHudPlugin);
    });
    let mut s = Station(s);
    let mob = s.mob();
    let walking = |s: &Station| s.server.world().get::<Walking>(mob).is_some_and(|w| w.0);
    let client_walking = |s: &Station| {
        let hud = s.players[0].app.world().resource::<InteractHud>();
        hud.window.is_some().then_some(hud.data.walking)
    };
    s.run_until("HUD open, running", |s| client_walking(s) == Some(false));

    let click = |s: &mut Station| {
        let world = s.players[0].app.world_mut();
        let button = world
            .query::<(Entity, &Name)>()
            .iter(world)
            .find(|(_, n)| n.as_str() == "move intent")
            .map(|(e, _)| e)
            .expect("move intent button");
        let location = Location {
            target: NormalizedRenderTarget::None {
                width: 1,
                height: 1,
            },
            position: Vec2::ZERO,
        };
        world.trigger(PointerClick {
            entity: button,
            pointer: Pointer::new(PointerId::Mouse, location),
            button: PointerButton::Primary,
            hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
            duration: default(),
            count: 1,
        });
    };
    click(&mut s);
    s.run_until("walking", |s| walking(s));
    s.run_until("HUD shows walking", |s| client_walking(s) == Some(true));
    assert!(s.get::<InteractHudData>(mob).walking, "server HUD data");
    click(&mut s);
    s.run_until("running again", |s| !walking(s));
    s.run_until("HUD shows running", |s| client_walking(s) == Some(false));
}
