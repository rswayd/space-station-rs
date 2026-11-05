//! Mining over real WebTransport on 127.0.0.1: a pickaxe digs an iron rock (4 s) into floor
//! and 3 iron ore, which lands on the ore redemption machine's input tile and is smelted into
//! the ore silo (3 points); ore by hand goes in too; the points are claimed onto the ID's
//! account and spent at the mining order console (express to the buyer's tile, cargo
//! shipments wait for the supply shuttle); a gibtonite rock starts its chain reaction when
//! dug, a mining scanner stops it and digging again gives the gibtonite ore; ore goes into an
//! ore box and is emptied out of it.

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
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/ore": { "parent": "/obj/item/stack", "declared_vars": { "points": 0,
        "refined_type": null, "name": "rock" } },
    "/obj/item/stack/ore/iron": { "parent": "/obj/item/stack/ore", "declared_vars": {
        "name": "iron ore", "points": 1, "merge_type": { "path": "/obj/item/stack/ore/iron" },
        "refined_type": { "path": "/obj/item/stack/sheet/iron" } } },
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
/// ore from (3,3), north of it.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"I" = (/turf/closed/mineral/iron,/area/space)
"G" = (/turf/closed/mineral/gibtonite,/area/space)
"B" = (/obj/structure/ore_box,/turf/open/floor,/area/space)
"M" = (/obj/machinery/mineral/ore_redemption,/turf/open/floor,/area/space)
"O" = (/obj/machinery/ore_silo,/turf/open/floor,/area/space)
"C" = (/obj/machinery/computer/order_console/mining,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwww
wGffffOfw
wsIffffff
wBMfffCfw
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
