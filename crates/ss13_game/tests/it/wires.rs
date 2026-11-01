//! Wires over real WebTransport on 127.0.0.1: on a fixture station powered by an APC cell, a
//! player opens an airlock's panel with a screwdriver, opens the `Wires` window with a multitool
//! (the airlock's colors), drops the bolts by pulsing their wire (the door stays shut), hacks
//! a door it has no access to (cut ID scan, pulse open), and cuts the shock wire: touching the
//! door with insulated gloves does nothing, a bare touch burns and stuns.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::doors::{Door, DoorState};
use ss13_game::health::Health;
use ss13_game::inventory::Inventory;
use ss13_game::server::MoveQueue;
use ss13_game::status_effects::StatusEffects;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::wires::{Electrified, WIRE_BOLTS, WIRE_IDSCAN, WIRE_OPEN, WIRE_SHOCK, Wires};
use ss13_mapload::DmItem;
use ss13_net::server::Interest;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::interfaces::wires::{WiresData, WiresUi};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": { "interaction_flags_item": 1,
        "icon": { "resource": "i.dmi" }, "icon_state": "tool", "slot_flags": 0,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0, "siemens_coefficient": 1 } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "name": "screwdriver", "tool_behaviour": "screwdriver" } },
    "/obj/item/wirecutters": { "parent": "/obj/item", "declared_vars": {
        "name": "wirecutters", "tool_behaviour": "cutters" } },
    "/obj/item/multitool": { "parent": "/obj/item", "declared_vars": {
        "name": "multitool", "tool_behaviour": "multitool" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/gloves": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 4 } },
    "/obj/item/clothing/gloves/color/yellow": { "parent": "/obj/item/clothing/gloves",
        "declared_vars": { "name": "insulated gloves", "siemens_coefficient": 0 } },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item", "declared_vars": {
        "maxcharge": 10000, "chargerate": 500 } },
    "/obj/item/stock_parts/power_store/battery": { "parent": "/obj/item/stock_parts/power_store",
        "declared_vars": { "maxcharge": 1000000, "chargerate": 1000000 } },
    "/obj/item/stock_parts/power_store/battery/upgraded": { "parent": "/obj/item/stock_parts/power_store/battery",
        "declared_vars": { "maxcharge": 2500000, "chargerate": 500000 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": {
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1 } },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "name": "door", "icon": { "resource": "d.dmi" }, "icon_state": "door_closed",
        "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0,
        "power_channel": 3, "idle_power_usage": 10, "active_power_usage": 200 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "airlock", "icon": { "resource": "a.dmi" }, "icon_state": "closed",
        "autoclose": 1, "base_icon_state": null, "normalspeed": 1,
        "overlays_file": { "resource": "o.dmi" }, "airlock_material": null } },
    "/obj/machinery/power": { "parent": "/obj/machinery", "declared_vars": {
        "use_power": 0, "idle_power_usage": 0, "active_power_usage": 0, "cable_layer": 2 } },
    "/obj/machinery/power/apc": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "apc.dmi" }, "icon_state": "apc0", "layer": 4,
        "cell_type": {"path": "/obj/item/stock_parts/power_store/battery/upgraded"},
        "start_charge": 90, "lighting": 3, "equipment": 3, "environ": 3, "operating": 1,
        "chargemode": 1, "areastring": null } },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 1, "always_unpowered": 0,
        "power_equip": 1, "power_light": 1, "power_environ": 1, "lightswitch": 1,
        "static_lighting": 1 } },
    "/area/space": { "parent": "/area", "declared_vars": { "always_unpowered": 1,
        "power_equip": 0, "power_light": 0, "power_environ": 0 } },
    "/area/station": { "parent": "/area" }
  }
}"##;

/// Rows top-down, y = 1 at the bottom. The player spawns on `s` (2,3) next to the airlock `D`
/// (3,3, command access only); the screwdriver `x` (2,2), wirecutters `c` (3,2), multitool `m`
/// (3,4) and insulated gloves `g` (2,4, under the APC) lie around it.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"g" = (/obj/item/clothing/gloves/color/yellow,/obj/machinery/power/apc{dir = 1; pixel_y = 25},/turf/open/floor,/area/station)
"m" = (/obj/item/multitool,/turf/open/floor,/area/station)
"x" = (/obj/item/screwdriver,/turf/open/floor,/area/station)
"c" = (/obj/item/wirecutters,/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock{req_access = list("command")},/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwww
wgmfffw
wsDfffw
wxcfffw
wwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 3, 1);
const DOOR: TilePos = TilePos::new(3, 3, 1);
const SCREWDRIVER: TilePos = TilePos::new(2, 2, 1);
const CUTTERS: TilePos = TilePos::new(3, 2, 1);
const MULTITOOL: TilePos = TilePos::new(3, 4, 1);
const GLOVES: TilePos = TilePos::new(2, 4, 1);

const STATES: [(&str, &str); 22] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "tool"),
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("a.dmi", "closing"),
    ("o.dmi", "lights_bolts"),
    ("o.dmi", "lights_denied"),
    ("o.dmi", "lights_opening"),
    ("o.dmi", "lights_closing"),
    ("o.dmi", "panel_closed"),
    ("o.dmi", "panel_opening"),
    ("o.dmi", "panel_open"),
    ("o.dmi", "panel_closing"),
    ("apc.dmi", "apc0"),
    ("apc.dmi", "apcox-0"),
    ("apc.dmi", "apcox-1"),
    ("a.dmi", "fill_closed"),
    ("a.dmi", "fill_open"),
];

fn state_id(dmi: &str, state: &str) -> ss13_shared::IconStateId {
    let i = STATES
        .iter()
        .position(|&(d, s)| d == dmi && s == state)
        .unwrap();
    ss13_shared::IconStateId(i as u32)
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("wires_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/door/airlock",
        "/obj/machinery/power/apc",
        "/obj/item/screwdriver",
        "/obj/item/wirecutters",
        "/obj/item/multitool",
        "/obj/item/clothing/gloves/color/yellow",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// What the client's tgui would render: the Wires window.
#[derive(Resource, Default)]
struct WiresWindow {
    id: Option<u32>,
    title: String,
    data: Option<WiresData>,
    closed: bool,
}

fn record_window(mut received: MessageReader<UiMessageReceived>, mut w: ResMut<WiresWindow>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                title,
                data,
                ..
            } if interface == WiresUi::NAME => {
                *w = WiresWindow {
                    id: Some(*window),
                    title: title.clone(),
                    data: Some(decode(data).unwrap()),
                    closed: false,
                };
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if w.id == Some(*window) => w.data = Some(decode(data).unwrap()),
            UiServerMessage::Close { window } if w.id == Some(*window) => w.closed = true,
            _ => {}
        }
    }
}

/// One client (`p0`): its connection and mob.
struct Station {
    st: testing::Station,
    link: Entity,
    mob: Entity,
}

ss13_game::drive_station!(Station, st);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut st = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        st.join("p0", |app| {
            app.init_resource::<WiresWindow>()
                .add_systems(Update, record_window);
        });
        let (link, mob) = (st.players[0].link, st.players[0].mob);
        Self { st, link, mob }
    }

    fn send(&mut self, intent: ClientIntent) {
        self.st.send(0, intent);
    }

    fn pos(&self) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob).unwrap()
    }

    /// One step attempt toward `dir` (a bump when blocked).
    fn try_step(&mut self, dir: Dir) {
        let applied = self
            .server
            .world()
            .get::<MoveQueue>(self.mob)
            .unwrap()
            .applied();
        self.players[0].seq = applied + 1;
        let seq = applied + 1;
        self.send(ClientIntent::Move { dir, seq });
    }

    fn door(&self) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(DOOR)
            .iter()
            .copied()
            .find(|&e| world.get::<Door>(e).is_some())
            .unwrap()
    }

    fn door_state(&self) -> (DoorState, bool, bool) {
        let d = self.server.world().get::<Door>(self.door()).unwrap();
        (d.state, d.locked, d.panel_open)
    }

    fn item(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some())
            .unwrap()
    }

    fn click(&mut self, entity: Entity) {
        self.st.click(0, entity);
    }

    fn inventory(&self) -> Inventory {
        self.server
            .world()
            .get::<Inventory>(self.mob)
            .unwrap()
            .clone()
    }

    /// Picks the item on `pos` up into the active hand.
    fn pick_up(&mut self, pos: TilePos) {
        let item = self.item(pos);
        self.pick(item);
    }

    fn pick(&mut self, item: Entity) {
        self.click(item);
        self.run_until("the item is in hand", |s| {
            s.inventory().active_item() == Some(item)
        });
    }

    fn swap_hands(&mut self) {
        let active = self.inventory().active;
        self.send(ClientIntent::SwapHands);
        self.run_until("hands swapped", |s| s.inventory().active != active);
    }

    fn wires(&self) -> Wires {
        self.server
            .world()
            .get::<Wires>(self.door())
            .unwrap()
            .clone()
    }

    fn color(&self, wire: &str) -> String {
        self.wires().color_of(wire).unwrap().to_owned()
    }

    fn act(&mut self, action: &str, wire: &str) {
        let window = self.players[0]
            .app
            .world()
            .resource::<WiresWindow>()
            .id
            .expect("window open");
        let color = self.color(wire);
        self.send(ClientIntent::UiAct {
            window,
            action: action.into(),
            params: vec![("wire".into(), color)],
        });
    }

    fn window(&self) -> &WiresWindow {
        self.players[0].app.world().resource::<WiresWindow>()
    }

    fn burn_and_stun(&self) -> (f32, f32) {
        let world = self.server.world();
        let burn = world.get::<Health>(self.mob).unwrap().burn;
        let stun = world.get::<StatusEffects>(self.mob).map_or(0.0, |e| e.stun);
        (burn, stun)
    }
}

#[test]
fn panel_window_bolts_hack_and_shock() {
    let mut s = Station::start("panel_window_bolts_hack_and_shock");
    assert_eq!(s.pos(), SPAWN);
    let cutters = s.item(CUTTERS);
    let door = s.door();
    s.run_until("everything is replicated", |s| {
        let interest = s.server.world().resource::<Interest>();
        [door, s.item(SCREWDRIVER), s.item(MULTITOOL)]
            .iter()
            .all(|&e| interest.net_id(s.link, e).is_some())
    });

    // No access (command door): a bump is denied.
    s.try_step(Dir::East);
    s.hold(1.0, "the command door stays shut", |s| {
        s.door_state().0 == DoorState::Closed && s.pos() == SPAWN
    });

    // Screwdriver: the panel opens (the panel overlay shows), the door does not move.
    s.pick_up(SCREWDRIVER);
    s.click(door);
    s.run_until("the panel opens", |s| s.door_state().2);
    let panel = state_id("o.dmi", "panel_closed");
    assert!(
        s.server
            .world()
            .get::<ss13_shared::Appearance>(door)
            .unwrap()
            .overlays
            .iter()
            .any(|o| o.icon_state == panel)
    );
    assert_eq!(s.door_state().0, DoorState::Closed);

    // Multitool in the other hand, a click: the Wires window with the airlock's colors.
    s.swap_hands();
    s.pick_up(MULTITOOL);
    s.click(door);
    s.run_until("the Wires window opens", |s| s.window().data.is_some());
    let data = s.window().data.clone().unwrap();
    assert_eq!(s.window().title, "airlock Wires");
    assert_eq!(data.proper_name.as_deref(), Some("Generic Airlock"));
    let server_colors: Vec<String> = s
        .wires()
        .colors
        .iter()
        .map(|(c, _)| (*c).to_owned())
        .collect();
    let client_colors: Vec<String> = data.wires.iter().map(|w| w.color.clone()).collect();
    assert_eq!(client_colors, server_colors);
    assert_eq!(client_colors.len(), 17);
    assert!(data.wires.iter().all(|w| w.wire.is_none() && !w.cut));
    assert_eq!(data.status[0], "The door bolts have disengaged.");
    assert_eq!(data.status[1], "The test light is on.");

    // Pulse the bolts: they drop, the status says so, the bolt lights show, a bump does nothing.
    s.act("pulse", WIRE_BOLTS);
    s.run_until("the bolts drop", |s| s.door_state().1);
    s.run_until("the status shows the bolts", |s| {
        s.window()
            .data
            .as_ref()
            .is_some_and(|d| d.status[0] == "The door bolts have engaged!")
    });
    let bolts = state_id("o.dmi", "lights_bolts");
    assert!(
        s.server
            .world()
            .get::<ss13_shared::Appearance>(door)
            .unwrap()
            .overlays
            .iter()
            .any(|o| o.icon_state == bolts)
    );
    // A pulse with the screwdriver-only hand would be refused: here both tools are held.
    s.act("pulse", WIRE_BOLTS);
    s.run_until("the bolts rise again", |s| !s.door_state().1);
    s.act("pulse", WIRE_BOLTS);
    s.run_until("the bolts drop again", |s| s.door_state().1);
    s.act("pulse", WIRE_OPEN);
    s.hold(1.0, "a bolted door does not open", |s| {
        s.door_state().0 == DoorState::Closed
    });
    s.act("pulse", WIRE_BOLTS);
    s.run_until("the bolts rise", |s| !s.door_state().1);

    // Pulsing open does nothing while the door checks IDs; cutting needs wirecutters.
    s.act("pulse", WIRE_OPEN);
    s.hold(0.5, "the ID check holds", |s| {
        s.door_state().0 == DoorState::Closed
    });
    s.act("cut", WIRE_IDSCAN);
    s.hold(0.3, "no wirecutters, no cut", |s| {
        !s.wires().is_cut(WIRE_IDSCAN)
    });

    // Wirecutters instead of the screwdriver; cut the ID scan, pulse open: the door opens.
    s.swap_hands();
    s.send(ClientIntent::DropActive);
    s.run_until("the screwdriver is dropped", |s| {
        s.inventory().active_item().is_none()
    });
    s.pick_up(CUTTERS);
    s.act("cut", WIRE_IDSCAN);
    s.run_until("the ID scan is cut", |s| s.wires().is_cut(WIRE_IDSCAN));
    s.run_until("the client sees it cut", |s| {
        let color = s.color(WIRE_IDSCAN);
        s.window()
            .data
            .as_ref()
            .is_some_and(|d| d.wires.iter().any(|w| w.color == color && w.cut))
    });
    s.act("pulse", WIRE_OPEN);
    s.run_until("the hacked door opens", |s| {
        s.door_state().0 == DoorState::Open
    });
    s.act("pulse", WIRE_OPEN);
    s.run_until("and closes again", |s| {
        s.door_state().0 == DoorState::Closed
    });

    // Insulated gloves on, then cut the shock wire: electrified, no zap for the gloved hacker;
    // the window closes (an electrified, powered airlock is not interactable).
    s.send(ClientIntent::DropActive);
    s.run_until("the wirecutters are dropped", |s| {
        s.inventory().active_item().is_none()
    });
    s.pick_up(GLOVES);
    s.send(ClientIntent::ClickSlot(InventorySlot::Gloves));
    s.run_until("the gloves are worn", |s| s.inventory().gloves.is_some());
    s.pick(cutters);
    s.act("cut", WIRE_SHOCK);
    s.run_until("the door is electrified", |s| {
        s.server.world().get::<Electrified>(door).is_some()
    });
    s.run_until("the window closes", |s| s.window().closed);
    assert_eq!(s.burn_and_stun(), (0.0, 0.0), "the gloves stopped the zap");

    // A gloved bare hand: no shock (the touch goes through: denied, the ID scan is cut).
    s.send(ClientIntent::DropActive);
    s.run_until("the wirecutters are dropped", |s| {
        s.inventory().active_item().is_none()
    });
    s.swap_hands();
    s.send(ClientIntent::DropActive);
    s.run_until("the multitool is dropped", |s| {
        s.inventory().active_item().is_none()
    });
    // The panel is still open: an empty hand reaches for the wires, which shock first; the
    // gloves stop it, and the window stays shut (an electrified airlock is not interactable).
    s.click(door);
    s.hold(1.0, "gloves: no shock", |s| s.burn_and_stun() == (0.0, 0.0));

    // Gloves off: the next bare touch burns and stuns.
    s.send(ClientIntent::ClickSlot(InventorySlot::Gloves));
    s.run_until("the gloves are off", |s| s.inventory().gloves.is_none());
    s.send(ClientIntent::DropActive);
    s.run_until("the gloves are dropped", |s| {
        s.inventory().active_item().is_none()
    });
    s.click(door);
    s.run_until("a bare touch is shocked", |s| {
        let (burn, stun) = s.burn_and_stun();
        burn > 0.0 && stun > 0.0
    });
    let (burn, _) = s.burn_and_stun();
    // ELECTROCUTE_DAMAGE of the APC cell (2.25 MJ / 10): 20 + 9 +- 5.
    assert!((24.0..=34.0).contains(&burn), "burn {burn}");
}
