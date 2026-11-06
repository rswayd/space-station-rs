//! Mining over real WebTransport on 127.0.0.1: a pickaxe digs an iron rock (4 s) into floor
//! and 3 iron ore, which lands on the ore redemption machine's input tile and is smelted into
//! the ore silo (3 points); ore by hand goes in too; the points are claimed onto the ID's
//! account and spent at the mining order console (express by pod to the buyer's tile, cargo
//! shipments wait for the supply shuttle); a gibtonite rock starts its chain reaction when
//! dug, a mining scanner stops it and digging again gives the gibtonite ore; ore goes into an
//! ore box and is emptied out of it. A second test: modkits in a kinetic accelerator, the
//! point transfer card, a mining voucher, gibtonite ore primed and defused, the ORM's
//! rotation, the satchel's ore-only rule and the stacking machine.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::cargo::economy::{BankAccountId, Economy};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::mining::console::{ConsoleAct, MiningShipments, OrderConsole, apply};
use ss13_game::mining::orm::{OreRedemption, claim};
use ss13_game::mining::rocks::{GibStage, Gibtonite};
use ss13_game::science::lathe::{MaterialStore, OreSilo};
use ss13_game::science::techweb::ResearchDb;
use ss13_game::storage::Stored;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::Stack;
use ss13_mapload::research::{MaterialDef, ResearchData};
use ss13_mapload::{DmItem, DmMineral, DmObj};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/orderable_item": { "parent": "/datum", "declared_vars": { "category_index": 0,
        "cost_per_order": 10, "max_per_order": null, "name": null, "desc": null,
        "purchase_path": null } },
    "/datum/orderable_item/mining": { "parent": "/datum/orderable_item", "declared_vars": {
        "category_index": "Mining" } },
    "/datum/orderable_item/mining/pick": { "parent": "/datum/orderable_item/mining",
        "declared_vars": { "purchase_path": { "path": "/obj/item/pickaxe" },
        "cost_per_order": 300 } },
    "/datum/voucher_set": { "parent": "/datum", "declared_vars": { "name": null,
        "description": null, "set_items": null } },
    "/datum/voucher_set/mining": { "parent": "/datum/voucher_set" },
    "/datum/voucher_set/mining/picks": { "parent": "/datum/voucher_set/mining",
        "declared_vars": { "name": "Pick Kit", "description": "Two picks.",
        "set_items": { "list": [ { "path": "/obj/item/pickaxe" },
            { "path": "/obj/item/mining_scanner" } ] } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0, "pass_flags_self": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "max_integrity": 100,
        "req_access": null } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3, "tool_behaviour": null,
        "toolspeed": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/pickaxe": { "parent": "/obj/item", "declared_vars": { "name": "pickaxe",
        "tool_behaviour": "mining", "toolspeed": 1 } },
    "/obj/item/mining_scanner": { "parent": "/obj/item", "declared_vars": {
        "name": "manual mining scanner" } },
    "/obj/item/gibtonite": { "parent": "/obj/item", "declared_vars": {
        "name": "gibtonite ore", "icon_state": "gibtonite" } },
    "/obj/item/mining_voucher": { "parent": "/obj/item", "declared_vars": {
        "name": "mining voucher" } },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/bag": { "parent": "/obj/item/storage" },
    "/obj/item/storage/bag/ore": { "parent": "/obj/item/storage/bag", "declared_vars": {
        "name": "mining satchel" } },
    "/obj/item/borg": { "parent": "/obj/item" },
    "/obj/item/borg/upgrade": { "parent": "/obj/item/borg" },
    "/obj/item/borg/upgrade/modkit": { "parent": "/obj/item/borg/upgrade", "declared_vars": {
        "name": "modkit", "cost": 30, "modifier": 1, "denied_type": null,
        "maximum_of_type": 0, "minebot_upgrade": 1, "minebot_exclusive": 0, "removable": 1 } },
    "/obj/item/borg/upgrade/modkit/damage": { "parent": "/obj/item/borg/upgrade/modkit",
        "declared_vars": { "name": "damage increase", "modifier": 10 } },
    "/obj/item/borg/upgrade/modkit/range": { "parent": "/obj/item/borg/upgrade/modkit",
        "declared_vars": { "name": "range increase", "modifier": 1, "cost": 25 } },
    "/obj/item/gun": { "parent": "/obj/item" },
    "/obj/item/gun/energy": { "parent": "/obj/item/gun" },
    "/obj/item/gun/energy/recharge": { "parent": "/obj/item/gun/energy" },
    "/obj/item/gun/energy/recharge/kinetic_accelerator": {
        "parent": "/obj/item/gun/energy/recharge", "declared_vars": {
        "name": "proto-kinetic accelerator" } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/sheet/glass": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "glass", "merge_type": { "path": "/obj/item/stack/sheet/glass" } } },
    "/obj/item/stack/ore": { "parent": "/obj/item/stack", "declared_vars": { "points": 0,
        "refined_type": null, "name": "rock" } },
    "/obj/item/stack/ore/iron": { "parent": "/obj/item/stack/ore", "declared_vars": {
        "name": "iron ore", "points": 1, "merge_type": { "path": "/obj/item/stack/ore/iron" },
        "refined_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/card/mining_point_card": { "parent": "/obj/item/card", "declared_vars": {
        "name": "mining point transfer card" } },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/ore_box": { "parent": "/obj/structure", "declared_vars": {
        "name": "ore box", "icon": { "resource": "i.dmi" }, "icon_state": "orebox",
        "density": 1 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1,
        "icon": { "resource": "m.dmi" }, "icon_state": "machine" } },
    "/obj/machinery/ore_silo": { "parent": "/obj/machinery", "declared_vars": {
        "name": "ore silo" } },
    "/obj/machinery/mineral": { "parent": "/obj/machinery", "declared_vars": {
        "input_dir": 1, "output_dir": 2 } },
    "/obj/machinery/mineral/ore_redemption": { "parent": "/obj/machinery/mineral",
        "declared_vars": { "name": "ore redemption machine", "ore_multiplier": 1,
        "point_upgrade": 1, "req_access": { "list": [ "mineral_storeroom" ] },
        "ore_values": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 1 ] } ] } } },
    "/obj/machinery/mineral/stacking_machine": { "parent": "/obj/machinery/mineral",
        "declared_vars": { "name": "stacking machine", "input_dir": 4, "output_dir": 8,
        "stack_amt": 50 } },
    "/obj/machinery/mineral/stacking_unit_console": { "parent": "/obj/machinery/mineral",
        "declared_vars": { "name": "stacking machine console", "density": 0 } },
    "/obj/machinery/computer": { "parent": "/obj/machinery" },
    "/obj/machinery/computer/order_console": { "parent": "/obj/machinery/computer",
        "declared_vars": { "order_categories": { "list": [] }, "cargo_cost_multiplier": 1,
        "express_cost_multiplier": 2, "credit_type": "cr", "cooldown_time": 600,
        "forced_express": 0 } },
    "/obj/machinery/computer/order_console/mining": {
        "parent": "/obj/machinery/computer/order_console", "declared_vars": {
        "name": "mining equipment order console",
        "order_categories": { "list": [ "Mining" ] }, "credit_type": "mp",
        "cargo_cost_multiplier": 0.65, "express_cost_multiplier": 1, "cooldown_time": 100 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/misc": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/misc/asteroid": { "parent": "/turf/open/misc" },
    "/turf/open/misc/asteroid/airless": { "parent": "/turf/open/misc/asteroid" },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/turf/closed/mineral": { "parent": "/turf/closed", "declared_vars": {
        "name": "rock", "icon_state": "rock",
        "baseturfs": { "path": "/turf/open/misc/asteroid/airless" },
        "turf_type": { "path": "/turf/open/misc/asteroid/airless" },
        "mineral_type": null, "mineral_amt": 3, "tool_mine_speed": 40,
        "hand_mine_speed": 150, "weak_turf": 0 } },
    "/turf/closed/mineral/iron": { "parent": "/turf/closed/mineral", "declared_vars": {
        "mineral_type": { "path": "/obj/item/stack/ore/iron" } } },
    "/turf/closed/mineral/gibtonite": { "parent": "/turf/closed/mineral", "declared_vars": {
        "mineral_amt": 1, "det_time": 8, "stage": 0 } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The player spawns on (2,3) between the iron rock
/// `I` (3,3), the gibtonite rock `G` (2,4) and the ore box `B` (2,2); the ORM `M` (3,2) takes
/// ore from (3,3), north of it. The stacking machine `S` (5,4) takes from (6,4) and puts out
/// on (4,4); its console `K` is on (5,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"I" = (/turf/closed/mineral/iron,/area/space)
"G" = (/turf/closed/mineral/gibtonite,/area/space)
"B" = (/obj/structure/ore_box,/turf/open/floor,/area/space)
"M" = (/obj/machinery/mineral/ore_redemption,/turf/open/floor,/area/space)
"O" = (/obj/machinery/ore_silo,/turf/open/floor,/area/space)
"C" = (/obj/machinery/computer/order_console/mining,/turf/open/floor,/area/space)
"S" = (/obj/machinery/mineral/stacking_machine,/turf/open/floor,/area/space)
"K" = (/obj/machinery/mineral/stacking_unit_console,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwww
wGffSfOfw
wsIffffff
wBMfKfCfw
wwwwwwwww
"}
"##;

const PLAYER: TilePos = TilePos::new(2, 3, 1);
const IRON_ROCK: TilePos = TilePos::new(3, 3, 1);
const GIBTONITE: TilePos = TilePos::new(2, 4, 1);
const ORE_BOX: TilePos = TilePos::new(2, 2, 1);
const ORM: TilePos = TilePos::new(3, 2, 1);
const CONSOLE: TilePos = TilePos::new(7, 2, 1);
const IRON: &str = "/datum/material/iron";

const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("t.dmi", "rock"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "orebox"),
    ("i.dmi", "gibtonite"),
    ("m.dmi", "machine"),
    ("l.dmi", "toy"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mining_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/pickaxe",
        "/obj/item/mining_scanner",
        "/obj/item/stack/ore/iron",
        "/obj/structure/ore_box",
        "/obj/machinery/ore_silo",
        "/obj/machinery/mineral/ore_redemption",
        "/obj/machinery/computer/order_console/mining",
        "/obj/machinery/mineral/stacking_machine",
        "/obj/machinery/mineral/stacking_unit_console",
        "/obj/item/mining_voucher",
        "/obj/item/card/mining_point_card",
        "/obj/item/storage/bag/ore",
        "/obj/item/borg/upgrade/modkit/damage",
        "/obj/item/borg/upgrade/modkit/range",
        "/obj/item/gun/energy/recharge/kinetic_accelerator",
        "/obj/item/gibtonite",
        "/obj/item/stack/sheet/glass",
        "/obj/item/stack/sheet/iron",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/closed/mineral/iron",
        "/turf/closed/mineral/gibtonite",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

fn research() -> ResearchDb {
    ResearchDb::new(ResearchData {
        materials: vec![MaterialDef {
            id: IRON.into(),
            name: "iron".into(),
            sheet_type: Some("/obj/item/stack/sheet/iron".into()),
            color: None,
        }],
        stacks: vec![
            (
                "/obj/item/stack/ore/iron".into(),
                vec![(IRON.into(), 100.0)],
            ),
            (
                "/obj/item/stack/sheet/iron".into(),
                vec![(IRON.into(), 100.0)],
            ),
        ],
        ..default()
    })
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start() -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.insert_resource(research());
        });
        s.join_all(1, |_| {});
        s.without_mood();
        Self(s)
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn on(&self, pos: TilePos, filter: impl Fn(EntityRef) -> bool) -> Option<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| filter(world.entity(e)))
    }

    fn obj(&self, pos: TilePos, path: &str) -> Entity {
        self.on(pos, |e| e.get::<DmObj>().is_some_and(|o| o.path == path))
            .unwrap_or_else(|| panic!("no {path} on {pos:?}"))
    }

    fn item(&self, pos: TilePos, path: &str) -> Option<Entity> {
        self.on(pos, |e| e.get::<DmItem>().is_some_and(|i| i.path == path))
    }

    fn rock(&self, pos: TilePos) -> Option<Entity> {
        self.on(pos, |e| e.contains::<DmMineral>())
    }

    /// A new `path` item straight into the active hand.
    fn give(&mut self, path: &str) -> Entity {
        let mob = self.mob();
        let world = self.server.world_mut();
        let item = ss13_game::construction::spawn_obj(world, path, TilePos::new(5, 3, 1)).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let old = {
            let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
            let hand = inventory.active;
            inventory.hands[hand].take()
        };
        if let Some(old) = old {
            world.despawn(old);
        }
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
        item
    }

    fn silo_iron(&self) -> u32 {
        let world = self.server.world();
        let db = world.resource::<ResearchDb>();
        let index = db.material_index[IRON];
        let silo = self.obj(TilePos::new(7, 4, 1), "/obj/machinery/ore_silo");
        assert!(world.get::<OreSilo>(silo).is_some());
        world.get::<MaterialStore>(silo).unwrap().amounts[index]
    }

    /// The player's ID's account (one is opened if the job gave none).
    fn account(&mut self) -> u32 {
        let mob = self.mob();
        let world = self.server.world_mut();
        let card = world
            .get::<Inventory>(mob)
            .and_then(|i| i.id_card(world))
            .expect("an ID");
        if let Some(id) = world.get::<BankAccountId>(card) {
            return id.0;
        }
        let id = world.resource_mut::<Economy>().open_account("Miner", 0, "");
        world.entity_mut(card).insert(BankAccountId(id));
        id
    }

    fn mining_points(&mut self) -> i64 {
        let id = self.account();
        self.server
            .world()
            .resource::<Economy>()
            .account(id)
            .unwrap()
            .mining_points
    }
}

#[test]
fn dig_smelt_claim_buy_gibtonite_and_ore_box() {
    let mut s = Station::start();
    assert_eq!(*s.server.world().get::<TilePos>(s.mob()).unwrap(), PLAYER);
    let orm = s.obj(ORM, "/obj/machinery/mineral/ore_redemption");
    assert!(
        s.server
            .world()
            .get::<OreRedemption>(orm)
            .is_some_and(|o| o.silo.is_some())
    );
    let gib = s.rock(GIBTONITE).expect("gibtonite rock");
    let det = s.server.world().get::<Gibtonite>(gib).unwrap().det_time;
    assert!((8..=10).contains(&det), "det_time {det}");

    // A pickaxe digs the iron rock: 4 s, then floor; its 3 ore land on the ORM's input tile
    // and are smelted into the silo for 3 points.
    s.give("/obj/item/pickaxe");
    let rock = s.rock(IRON_ROCK).expect("iron rock");
    s.click(0, rock);
    s.hold(3.5, "still digging", |s| s.rock(IRON_ROCK).is_some());
    s.run_until("dug", |s| s.rock(IRON_ROCK).is_none());
    s.run_until("smelted", |s| s.silo_iron() == 300);
    assert!(s.item(IRON_ROCK, "/obj/item/stack/ore/iron").is_none());
    let points = s.server.world().get::<OreRedemption>(orm).unwrap().points;
    assert_eq!(points, 3.0);

    // Ore by hand: 2 more units, 2 more points.
    let ore = s.give("/obj/item/stack/ore/iron");
    s.server
        .world_mut()
        .entity_mut(ore)
        .insert(Stack { amount: 2 });
    s.click(0, orm);
    s.run_until("ore by hand", |s| s.silo_iron() == 500);
    assert_eq!(
        s.server.world().get::<OreRedemption>(orm).unwrap().points,
        5.0
    );

    // Claimed onto the ID's account.
    s.account();
    let mob = s.mob();
    assert!(claim(s.server.world_mut(), orm, mob));
    assert_eq!(s.mining_points(), 5);
    assert_eq!(
        s.server.world().get::<OreRedemption>(orm).unwrap().points,
        0.0
    );
    assert!(
        !claim(s.server.world_mut(), orm, mob),
        "nothing left to claim"
    );

    // The order console: a pickaxe (300 mp) by express lands on the buyer; not enough points
    // first.
    let console = s.obj(CONSOLE, "/obj/machinery/computer/order_console/mining");
    assert!(s.server.world().get::<OrderConsole>(console).is_some());
    apply(s.server.world_mut(), console, mob, ConsoleAct::AddOne(0));
    apply(s.server.world_mut(), console, mob, ConsoleAct::Express);
    assert!(
        s.item(PLAYER, "/obj/item/pickaxe").is_none(),
        "5 mp is not enough"
    );
    let id = s.account();
    s.server
        .world_mut()
        .resource_mut::<Economy>()
        .account_mut(id)
        .unwrap()
        .mining_points = 1000;
    apply(s.server.world_mut(), console, mob, ConsoleAct::Express);
    assert!(
        s.item(PLAYER, "/obj/item/pickaxe").is_none(),
        "the supply pod is on its way"
    );
    // The teleporter pod lands after 3.4 s and opens 3 s later.
    s.run_for(6.6);
    assert!(s.item(PLAYER, "/obj/item/pickaxe").is_some(), "delivered");
    assert_eq!(s.mining_points(), 700);
    assert!(
        s.server
            .world()
            .get::<OrderConsole>(console)
            .unwrap()
            .cart
            .is_empty()
    );
    // On cooldown (10 s): the cargo order waits.
    apply(s.server.world_mut(), console, mob, ConsoleAct::AddOne(0));
    apply(s.server.world_mut(), console, mob, ConsoleAct::Purchase);
    assert!(s.server.world().resource::<MiningShipments>().0.is_empty());
    s.run_for(10.5);
    apply(s.server.world_mut(), console, mob, ConsoleAct::Purchase);
    assert_eq!(
        s.server.world().resource::<MiningShipments>().0,
        vec![vec!["/obj/item/pickaxe".to_owned()]],
        "shipped with the next supply shuttle"
    );
    // 300 * 0.65.
    assert_eq!(s.mining_points(), 505);

    // Gibtonite: digging starts the reaction, a mining scanner stops it, digging again gives
    // the ore.
    s.give("/obj/item/pickaxe");
    s.click(0, gib);
    s.run_until("reacting", |s| {
        s.server
            .world()
            .get::<Gibtonite>(gib)
            .is_some_and(|g| g.stage == GibStage::Active)
    });
    s.give("/obj/item/mining_scanner");
    s.click(0, gib);
    s.run_until("defused", |s| {
        s.server
            .world()
            .get::<Gibtonite>(gib)
            .is_some_and(|g| g.stage == GibStage::Stable)
    });
    s.give("/obj/item/pickaxe");
    s.click(0, gib);
    s.run_until("dug out", |s| s.rock(GIBTONITE).is_none());
    assert!(s.item(GIBTONITE, "/obj/item/gibtonite").is_some());

    // The ore box takes ore by hand and dumps it.
    let ore_box = s.obj(ORE_BOX, "/obj/structure/ore_box");
    let ore = s.give("/obj/item/stack/ore/iron");
    s.click(0, ore_box);
    s.run_until("ore in the box", |s| {
        s.server
            .world()
            .get::<Stored>(ore_box)
            .is_some_and(|st| st.items().contains(&ore))
    });
    ss13_game::mining::rocks::dump_box(s.server.world_mut(), ore_box);
    assert_eq!(s.item(ORE_BOX, "/obj/item/stack/ore/iron"), Some(ore));
}

const STACKER: TilePos = TilePos::new(5, 4, 1);
const STACK_IN: TilePos = TilePos::new(6, 4, 1);
const STACK_OUT: TilePos = TilePos::new(4, 4, 1);

impl Station {
    fn client(&self) -> Entity {
        self.server
            .world()
            .get::<ss13_game::server::PlayerMob>(self.mob())
            .unwrap()
            .client
    }

    fn spawn(&mut self, path: &str, pos: TilePos) -> Entity {
        ss13_game::construction::spawn_obj(self.server.world_mut(), path, pos).unwrap()
    }

    /// Glass sheets lying on `pos`.
    fn glass_on(&self, pos: TilePos) -> u32 {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .filter(|&&e| {
                world
                    .get::<DmItem>(e)
                    .is_some_and(|i| i.path == "/obj/item/stack/sheet/glass")
            })
            .map(|&e| world.get::<Stack>(e).map_or(1, |s| s.amount))
            .sum()
    }
}

#[test]
fn modkits_cards_vouchers_gibtonite_ore_and_stacking() {
    use ss13_game::mining::equipment::{PointCard, redeem, transfer};
    use ss13_game::mining::modkits::{Modkits, install, modify_projectile, remaining_capacity};
    use ss13_game::mining::rocks::{GibPrimed, GibtoniteQuality, item_ex_act, satchel_refuses};
    use ss13_game::mining::stacking::{
        StackingAct, StackingConsole, StackingMachine, apply as stack_apply,
    };

    let mut s = Station::start();
    let (mob, client) = (s.mob(), s.client());

    // Modkits: three damage kits fill 90% of the accelerator; a fourth and a range kit (25%)
    // do not fit. The shot gets +30 damage.
    let gun = s.spawn(
        "/obj/item/gun/energy/recharge/kinetic_accelerator",
        TilePos::new(4, 3, 1),
    );
    for n in 0..3 {
        let kit = s.spawn(
            "/obj/item/borg/upgrade/modkit/damage",
            TilePos::new(4, 3, 1),
        );
        assert!(
            install(s.server.world_mut(), kit, gun, None),
            "damage kit {n}"
        );
    }
    let extra = s.spawn(
        "/obj/item/borg/upgrade/modkit/damage",
        TilePos::new(4, 3, 1),
    );
    assert!(
        !install(s.server.world_mut(), extra, gun, None),
        "no room for a fourth"
    );
    let range = s.spawn("/obj/item/borg/upgrade/modkit/range", TilePos::new(4, 3, 1));
    assert!(
        !install(s.server.world_mut(), range, gun, None),
        "10% left, 25% needed"
    );
    assert_eq!(remaining_capacity(s.server.world(), gun), 10.0);
    assert_eq!(
        s.server.world().get::<Modkits>(gun).unwrap().kits().len(),
        3
    );
    let mut data = ss13_mapload::ProjectileData {
        path: "/obj/projectile/kinetic".into(),
        damage: 40.0,
        range: 3.0,
        ..default()
    };
    modify_projectile(s.server.world(), gun, &mut data);
    assert_eq!((data.damage, data.range), (70.0, 3.0));
    // A held kit goes in from the hand.
    let held = s.give("/obj/item/borg/upgrade/modkit/damage");
    assert!(ss13_game::mining::modkits::item_interaction(
        s.server.world_mut(),
        client,
        mob,
        held,
        gun
    ));
    assert_eq!(
        s.server.world().get::<Modkits>(gun).unwrap().kits().len(),
        3,
        "still full"
    );

    // The point card: 30 points onto the card, then everything back (only 30 move).
    let id = s.account();
    s.server
        .world_mut()
        .resource_mut::<Economy>()
        .account_mut(id)
        .unwrap()
        .mining_points = 100;
    let card = s.spawn("/obj/item/card/mining_point_card", TilePos::new(4, 3, 1));
    let id_card = s
        .server
        .world()
        .get::<Inventory>(mob)
        .and_then(|i| i.id_card(s.server.world()))
        .unwrap();
    transfer(s.server.world_mut(), client, card, id_card, false, 30.0);
    assert_eq!(s.mining_points(), 70);
    assert_eq!(s.server.world().get::<PointCard>(card).unwrap().points, 30);
    transfer(s.server.world_mut(), client, card, id_card, true, 500.0);
    assert_eq!(s.mining_points(), 100);
    assert_eq!(s.server.world().get::<PointCard>(card).unwrap().points, 0);

    // A voucher on the mining console: the set appears on its tile, the voucher is gone.
    let console = s.obj(CONSOLE, "/obj/machinery/computer/order_console/mining");
    let voucher = s.give("/obj/item/mining_voucher");
    assert!(ss13_game::mining::equipment::item_interaction(
        s.server.world_mut(),
        client,
        mob,
        voucher,
        console
    ));
    // The console is out of reach from the spawn: nothing yet.
    assert!(!redeem(
        s.server.world_mut(),
        client,
        mob,
        voucher,
        console,
        0
    ));
    // Next to it (7,3).
    s.server
        .world_mut()
        .entity_mut(mob)
        .insert(TilePos::new(7, 3, 1));
    s.run_for(0.2);
    assert!(redeem(
        s.server.world_mut(),
        client,
        mob,
        voucher,
        console,
        0
    ));
    assert!(s.server.world().get_entity(voucher).is_err());
    assert!(s.item(CONSOLE, "/obj/item/pickaxe").is_some());
    assert!(s.item(CONSOLE, "/obj/item/mining_scanner").is_some());
    assert!(
        !redeem(s.server.world_mut(), client, mob, voucher, console, 0),
        "used up"
    );
    // Back, away from the gibtonite blast below.
    s.server.world_mut().entity_mut(mob).insert(PLAYER);

    // Gibtonite ore: a pickaxe primes it, a scanner stops it at the lowest quality, a blast
    // primes it again and it blows 10 s later.
    let ore = s.spawn("/obj/item/gibtonite", TilePos::new(8, 3, 1));
    s.server
        .world_mut()
        .entity_mut(ore)
        .insert(GibtoniteQuality(3));
    let pick = s.give("/obj/item/pickaxe");
    assert!(ss13_game::mining::hand_interaction(
        s.server.world_mut(),
        client,
        mob,
        pick,
        ore
    ));
    assert!(s.server.world().get::<GibPrimed>(ore).is_some());
    let scanner = s.give("/obj/item/mining_scanner");
    assert!(ss13_game::mining::hand_interaction(
        s.server.world_mut(),
        client,
        mob,
        scanner,
        ore
    ));
    assert!(s.server.world().get::<GibPrimed>(ore).is_none());
    assert_eq!(
        s.server.world().get::<GibtoniteQuality>(ore),
        Some(&GibtoniteQuality(1))
    );

    // The satchel takes only ore.
    let bag = s.spawn("/obj/item/storage/bag/ore", TilePos::new(4, 3, 1));
    let iron_ore = s.spawn("/obj/item/stack/ore/iron", TilePos::new(4, 3, 1));
    assert!(satchel_refuses(s.server.world(), bag, card));
    assert!(!satchel_refuses(s.server.world(), bag, iron_ore));

    // The ORM turns its input clockwise with the panel open only.
    let orm = s.obj(ORM, "/obj/machinery/mineral/ore_redemption");
    assert!(ss13_game::mining::orm::click_alt(
        s.server.world_mut(),
        client,
        orm
    ));
    let input = |s: &Station| {
        s.server
            .world()
            .get::<ss13_mapload::DmOreRedemption>(orm)
            .unwrap()
            .input_dir
    };
    assert_eq!(input(&s), 1, "closed: unchanged");
    s.server
        .world_mut()
        .entity_mut(orm)
        .insert(ss13_game::construction::PanelOpen(true));
    ss13_game::mining::orm::click_alt(s.server.world_mut(), client, orm);
    assert_eq!(input(&s), 4, "north -> east");

    // The stacking machine: its console found it; iron sheets go to the silo, 60 glass make a
    // stack of 50 on the output and 10 stay inside until released.
    let stacker = s.obj(STACKER, "/obj/machinery/mineral/stacking_machine");
    let stack_console = s.obj(
        TilePos::new(5, 2, 1),
        "/obj/machinery/mineral/stacking_unit_console",
    );
    assert_eq!(
        s.server
            .world()
            .get::<StackingConsole>(stack_console)
            .unwrap()
            .machine,
        Some(stacker)
    );
    let iron_before = s.silo_iron();
    ss13_game::construction::spawn_stack(
        s.server.world_mut(),
        "/obj/item/stack/sheet/iron",
        STACK_IN,
        2,
    );
    s.run_until("iron in the silo", |s| s.silo_iron() == iron_before + 200);
    ss13_game::construction::spawn_stack(
        s.server.world_mut(),
        "/obj/item/stack/sheet/glass",
        STACK_IN,
        50,
    );
    ss13_game::construction::spawn_stack(
        s.server.world_mut(),
        "/obj/item/stack/sheet/glass",
        STACK_IN,
        10,
    );
    s.run_until("a stack of glass out", |s| s.glass_on(STACK_OUT) == 50);
    let stored = |s: &Station| {
        s.server
            .world()
            .get::<StackingMachine>(stacker)
            .unwrap()
            .stored
            .iter()
            .map(|st| st.amount)
            .sum::<u32>()
    };
    assert_eq!(stored(&s), 10);
    stack_apply(
        s.server.world_mut(),
        stacker,
        StackingAct::Release("/obj/item/stack/sheet/glass".into()),
    );
    assert_eq!(s.glass_on(STACK_OUT), 60);
    assert_eq!(stored(&s), 0);

    // The blast: a primed ore (quality 1 now) blows after 10 s.
    item_ex_act(
        s.server.world_mut(),
        ore,
        ss13_game::explosions::Severity::Light,
    );
    assert!(s.server.world().get::<GibPrimed>(ore).is_some());
    s.hold(9.5, "still primed", |s| {
        s.server.world().get_entity(ore).is_ok()
    });
    s.run_until("blown", |s| s.server.world().get_entity(ore).is_err());
}
