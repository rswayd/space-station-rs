//! Stack crafting menus over real WebTransport on 127.0.0.1 (`ss13_game::stack_recipes`):
//! iron in hand opens its recipe list, a bed is built at once on the builder's tile, a second
//! bed on the same tile is refused (`CRAFT_ONE_PER_TURF`), iron rods ask the multiplier and come
//! out `2 × multiplier`, a girder takes its 4 s. Plasma sheets burn into plasma, rods on titanium
//! make shuttle rods (`ss13_game::sheets`).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::NextMove;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::Stack;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_tgui::prompt::{Prompt, PromptAnswer, PromptAnswered, PromptKind};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2, "max_integrity": 100 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "icon": { "resource": "st.dmi" } } },
    "/obj/structure/bed": { "parent": "/obj/structure", "declared_vars": {
        "name": "bed", "icon_state": "bed" } },
    "/obj/structure/girder": { "parent": "/obj/structure", "declared_vars": {
        "name": "girder", "icon_state": "girder", "density": 1 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/sheet/mineral": { "parent": "/obj/item/stack/sheet" },
    "/obj/item/stack/sheet/mineral/plasma": { "parent": "/obj/item/stack/sheet/mineral",
        "declared_vars": { "name": "solid plasma", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/mineral/plasma" } } },
    "/obj/item/stack/sheet/mineral/titanium": { "parent": "/obj/item/stack/sheet/mineral",
        "declared_vars": { "name": "titanium", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/mineral/titanium" } } },
    "/obj/item/knife": { "parent": "/obj/item", "declared_vars": { "name": "knife",
        "sharpness": 1 } },
    "/obj/item/stack/sheet/animalhide": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "hide", "icon_state": "sheet-metal" } },
    "/obj/item/stack/sheet/animalhide/corgi": { "parent": "/obj/item/stack/sheet/animalhide",
        "declared_vars": { "merge_type": { "path": "/obj/item/stack/sheet/animalhide/corgi" } } },
    "/obj/item/stack/sheet/hairlesshide": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "icon_state": "sheet-metal", "merge_type": { "path": "/obj/item/stack/sheet/hairlesshide" } } },
    "/obj/item/stack/sheet/wethide": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "icon_state": "sheet-metal", "merge_type": { "path": "/obj/item/stack/sheet/wethide" } } },
    "/obj/item/stack/rods/shuttle": { "parent": "/obj/item/stack/rods", "declared_vars": {
        "merge_type": { "path": "/obj/item/stack/rods/shuttle" } } },
    "/obj/item/stack/rods/shuttle/five": { "parent": "/obj/item/stack/rods/shuttle",
        "declared_vars": { "amount": 5 } },
    "/obj/item/stack/rods": { "parent": "/obj/item/stack", "declared_vars": {
        "icon_state": "rods", "merge_type": { "path": "/obj/item/stack/rods" } } },
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
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15 } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwww
wffw
wsfw
wwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 2, 1);
const IRON: &str = "/obj/item/stack/sheet/iron";

const STATES: [(&str, &str); 9] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "rods"),
    ("st.dmi", "bed"),
    ("st.dmi", "girder"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("stack_recipes_fixture");
    std::fs::create_dir_all(&dir).unwrap();
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/bed",
        "/obj/structure/girder",
        IRON,
        "/obj/item/stack/rods",
        "/obj/item/stack/rods/shuttle/five",
        "/obj/item/knife",
        "/obj/item/stack/sheet/animalhide/corgi",
        "/obj/item/stack/sheet/hairlesshide",
        "/obj/item/stack/sheet/wethide",
        "/obj/item/stack/sheet/mineral/plasma",
        "/obj/item/stack/sheet/mineral/titanium",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start() -> Self {
        Self::start_with(|_| {})
    }

    fn start_with(setup: impl FnOnce(&mut App)) -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), setup);
        s.join("p0", |_| {});
        s.without_mood();
        let s = Self(s);
        assert_eq!(*s.get::<TilePos>(s.mob()), SPAWN);
        s
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    /// `amount` iron sheets straight into the active hand.
    fn give_iron(&mut self, amount: u32) -> Entity {
        self.give(IRON, amount, 0)
    }

    /// An `amount` stack of `path` straight into the active hand (`other`: 1 for the other one).
    fn give(&mut self, path: &str, amount: u32, other: usize) -> Entity {
        let mob = self.mob();
        let world = self.server.world_mut();
        let item = ss13_game::construction::spawn_obj(world, path, SPAWN).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert((CarriedBy(mob), Stack { amount }));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = (inventory.active + other) % 2;
        inventory.hands[hand] = Some(item);
        item
    }

    fn use_in_hand(&mut self) {
        let mob = self.mob();
        self.run_until("off the click cooldown", |s| {
            s.now() >= s.get::<NextMove>(mob).0
        });
        let slot = if self.get::<Inventory>(mob).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        };
        self.send(0, ClientIntent::ClickSlot(slot));
    }

    fn prompt(&mut self) -> Option<Entity> {
        let mob = self.mob();
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &Prompt)>();
        q.iter(world).find(|(_, p)| p.actor == mob).map(|(e, _)| e)
    }

    fn answer(&mut self, prompt: Entity, answer: PromptAnswer) {
        let (client, actor) = (self.players[0].link, self.mob());
        self.server.world_mut().write_message(PromptAnswered {
            prompt,
            client,
            actor,
            answer: Some(answer),
        });
        self.tick();
        if let Ok(e) = self.server.world_mut().get_entity_mut(prompt) {
            e.despawn();
        }
    }

    /// Opens the iron menu and picks the line starting with `label`.
    fn build(&mut self, label: &str) {
        self.use_in_hand();
        self.run_until("the recipe list", |s| s.prompt().is_some());
        let prompt = self.prompt().unwrap();
        let PromptKind::List { items, .. } = &self.get::<Prompt>(prompt).kind else {
            panic!("the menu is a list");
        };
        let i = items
            .iter()
            .position(|item| item.starts_with(label))
            .unwrap_or_else(|| panic!("no {label} in {items:?}"));
        self.answer(prompt, PromptAnswer::Item(i));
    }

    fn count_at(&self, path: &str) -> usize {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(SPAWN)
            .iter()
            .filter(|&&e| world.get::<DmObj>(e).is_some_and(|o| o.path == path))
            .count()
    }

    fn stack_on_floor(&self, path: &str) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(SPAWN)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
            .unwrap_or_else(|| panic!("no {path} on the floor"))
    }

    fn amount(&self, stack: Entity) -> u32 {
        self.try_get::<Stack>(stack).map_or(0, |s| s.amount)
    }
}

#[test]
fn iron_builds_beds_girders_and_rods_from_its_menu() {
    let mut s = Station::start();
    let iron = s.give_iron(20);

    s.build("bed (2");
    assert_eq!(s.count_at("/obj/structure/bed"), 1, "a bed at once");
    assert_eq!(s.amount(iron), 18);

    // `CRAFT_ONE_PER_TURF`: no second bed here.
    s.build("bed (2");
    assert_eq!(s.count_at("/obj/structure/bed"), 1);
    assert_eq!(s.amount(iron), 18);

    // Rods: 1 sheet -> 2 rods, the multiplier asked.
    s.build("2 × iron rod");
    s.run_until("the multiplier question", |s| s.prompt().is_some());
    let prompt = s.prompt().unwrap();
    s.answer(prompt, PromptAnswer::Number(3.0));
    assert_eq!(s.amount(iron), 15);
    let mob = s.mob();
    let rods = s
        .get::<Inventory>(mob)
        .hands
        .iter()
        .flatten()
        .copied()
        .find(|&h| {
            s.try_get::<DmItem>(h)
                .is_some_and(|i| i.path == "/obj/item/stack/rods")
        })
        .expect("the rods go into the free hand");
    assert_eq!(s.amount(rods), 6);

    // A girder (the bed is not dense) takes 4 s; last, as it blocks the tile.
    s.build("wall girders");
    s.tick();
    assert_eq!(s.count_at("/obj/structure/girder"), 0, "still building");
    s.run_until("the girder", |s| s.count_at("/obj/structure/girder") == 1);
    assert_eq!(s.amount(iron), 13);
}

#[test]
fn plasma_sheets_flood_their_tile_when_burnt() {
    let mut s = Station::start_with(|app| {
        app.add_plugins(ss13_atmos::AtmosPlugin);
    });
    let world = s.server.world_mut();
    let sheets = ss13_game::construction::spawn_stack(
        world,
        "/obj/item/stack/sheet/mineral/plasma",
        SPAWN,
        4,
    )
    .unwrap();
    let before = world
        .resource::<ss13_atmos::Atmos>()
        .air(SPAWN)
        .map_or(0.0, |a| a.get(ss13_atmos::GasId::PLASMA));
    // Not hot enough for a fire: nothing.
    ss13_game::fire::fire_act(world, sheets, 300.0);
    assert!(world.get_entity(sheets).is_ok());
    ss13_game::fire::fire_act(world, sheets, 1000.0);
    assert!(world.get_entity(sheets).is_err(), "the sheets burn away");
    let after = world
        .resource::<ss13_atmos::Atmos>()
        .air(SPAWN)
        .map_or(0.0, |a| a.get(ss13_atmos::GasId::PLASMA));
    assert!(
        (after - before - 20.0).abs() < 1e-6,
        "5 moles a sheet: {after}"
    );
}

#[test]
fn five_rods_on_titanium_make_shuttle_rods() {
    let mut s = Station::start();
    let rods = s.give("/obj/item/stack/rods", 7, 0);
    let titanium = s.give("/obj/item/stack/sheet/mineral/titanium", 1, 1);
    let mob = s.mob();
    s.run_until("off the click cooldown", |s| {
        s.now() >= s.get::<NextMove>(mob).0
    });
    let slot = other_slot(&s);
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("the shuttle rods", |s| s.amount(rods) == 2);
    let world = s.server.world();
    assert!(
        world.get_entity(titanium).is_err(),
        "the titanium is used up"
    );
    let shuttle = s
        .get::<Inventory>(mob)
        .hands
        .iter()
        .flatten()
        .copied()
        .find(|&h| {
            s.try_get::<DmItem>(h)
                .is_some_and(|i| i.path == "/obj/item/stack/rods/shuttle/five")
        })
        .expect("the shuttle rods take the titanium's hand");
    assert_eq!(s.amount(shuttle), 5);
}

fn other_slot(s: &Station) -> InventorySlot {
    if s.get::<Inventory>(s.mob()).active == 0 {
        InventorySlot::RightHand
    } else {
        InventorySlot::LeftHand
    }
}

#[test]
fn a_knife_dehairs_a_hide_and_water_soaks_it() {
    let mut s = Station::start();
    let mob = s.mob();
    let knife = {
        let world = s.server.world_mut();
        let knife = ss13_game::construction::spawn_obj(world, "/obj/item/knife", SPAWN).unwrap();
        world
            .entity_mut(knife)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(knife);
        knife
    };
    let hide = s.give("/obj/item/stack/sheet/animalhide/corgi", 3, 1);
    assert!(s.try_get::<ss13_mapload::DmWounding>(knife).is_some());
    s.run_until("off the click cooldown", |s| {
        s.now() >= s.get::<NextMove>(mob).0
    });
    let slot = other_slot(&s);
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("the hair cut off (5 s)", |s| {
        s.server.world().get_entity(hide).is_err()
    });
    let hairless = s.stack_on_floor("/obj/item/stack/sheet/hairlesshide");
    assert_eq!(s.amount(hairless), 3);
    let world = s.server.world_mut();
    assert!(ss13_game::sheets::soak(world, hairless));
    let wet = s.stack_on_floor("/obj/item/stack/sheet/wethide");
    assert_eq!(s.amount(wet), 3);
    s.tick();
    assert_eq!(
        s.try_get::<ss13_game::sheets::Wetness>(wet).copied(),
        Some(ss13_game::sheets::Wetness(30))
    );
}
