//! Integrated circuits on a fixture station: a radio component and a cell go into a circuit,
//! the circuit into a compact remote; the remote's button, wired to the radio's `Send`, signals
//! an assembly signaler that starts its holder's timer. A screwdriver takes the circuit back
//! out of the shell (off, without the shell's component).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::assemblies::server::{self as asm, Assembly, AssemblyHolder, Device};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::mecha::core::ItemCell;
use ss13_game::science::quantum_pad::QuantumPad;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::wiremod::circuit::Kind;
use ss13_game::wiremod::devices::{self as dv, UsbAttached, UsbCable};
use ss13_game::wiremod::server::{self as wm, IntegratedCircuit, Shell};
use ss13_shared::TilePos;

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
        "slot_flags": 0, "inhand_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "screwdriver" } },
    "/obj/item/integrated_circuit": { "parent": "/obj/item" },
    "/obj/item/circuit_component": { "parent": "/obj/item" },
    "/obj/item/circuit_component/radio": { "parent": "/obj/item/circuit_component" },
    "/obj/item/compact_remote": { "parent": "/obj/item" },
    "/obj/item/usb_cable": { "parent": "/obj/item" },
    "/obj/structure": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy" } },
    "/obj/structure/bot": { "parent": "/obj/structure" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "icon": { "resource": "i.dmi" }, "icon_state": "toy" } },
    "/obj/machinery/quantumpad": { "parent": "/obj/machinery" },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item/stock_parts" },
    "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item/stock_parts/power_store" },
    "/obj/item/assembly": { "parent": "/obj/item" },
    "/obj/item/assembly/signaler": { "parent": "/obj/item/assembly" },
    "/obj/item/assembly/timer": { "parent": "/obj/item/assembly" },
    "/obj/item/assembly_holder": { "parent": "/obj/item" },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256,
        "access": { "list": [] } } },
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
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wffffw
wfsffw
wffffw
wwwwww
"}
"##;

const CIRCUIT: &str = "/obj/item/integrated_circuit";
const RADIO: &str = "/obj/item/circuit_component/radio";
const REMOTE: &str = "/obj/item/compact_remote";
const CELL: &str = "/obj/item/stock_parts/power_store/cell";
const SIGNALER: &str = "/obj/item/assembly/signaler";
const TIMER: &str = "/obj/item/assembly/timer";
const SCREWDRIVER: &str = "/obj/item/screwdriver";
const CABLE: &str = "/obj/item/usb_cable";
const BOT: &str = "/obj/structure/bot";
const PAD: &str = "/obj/machinery/quantumpad";

const STATES: [(&str, &str); 4] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
];

fn fixture(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("wiremod_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        CIRCUIT,
        RADIO,
        REMOTE,
        CELL,
        SIGNALER,
        TIMER,
        SCREWDRIVER,
        CABLE,
        BOT,
        PAD,
        "/obj/item/assembly_holder",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> (Self, Entity, Entity) {
        let assets = fixture(test);
        let mut s = Self(testing::Station::start(
            fixture_map(&assets, &["map.dmm"]),
            |_| {},
        ));
        let p = s.0.join("p0", |_| {});
        let (link, mob) = (s.players[p].link, s.players[p].mob);
        (s, link, mob)
    }

    fn spawn(&mut self, path: &str, pos: TilePos) -> Entity {
        let world = self.0.server.world_mut();
        let e = ss13_game::construction::spawn_obj(world, path, pos).unwrap();
        world.flush();
        e
    }

    /// `path` into the mob's active hand (emptied first).
    fn give(&mut self, mob: Entity, path: &str) -> Entity {
        let pos = *self.get::<TilePos>(mob);
        let item = self.spawn(path, pos);
        let world = self.0.server.world_mut();
        let hands = world.get::<Inventory>(mob).unwrap().hands;
        for held in hands.into_iter().flatten() {
            world.despawn(held);
        }
        let hand = world.get::<Inventory>(mob).unwrap().active;
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = Some(item);
        item
    }

    fn world(&mut self) -> &mut World {
        self.0.server.world_mut()
    }

    fn timing(&self, a: Entity) -> bool {
        matches!(
            self.get::<Assembly>(a).device,
            Device::Timer { timing: true, .. }
        )
    }
}

#[test]
fn a_compact_remote_radios_a_signaler() {
    let (mut s, client, mob) = Station::start("remote");
    let pos = *s.get::<TilePos>(mob);
    let circuit = s.spawn(CIRCUIT, pos);
    let radio = s.spawn(RADIO, pos);
    let cell = s.spawn(CELL, pos);
    let remote = s.spawn(REMOTE, pos);
    let world = s.world();
    world
        .entity_mut(cell)
        .insert(ItemCell(ss13_game::power::Cell {
            charge: 1000.0,
            maxcharge: 1000.0,
            chargerate: 0.0,
        }));

    // The radio and the cell into the circuit, the circuit into the remote.
    assert!(wm::item_interaction(world, client, mob, radio, circuit));
    assert!(wm::item_interaction(world, client, mob, cell, circuit));
    assert!(wm::item_interaction(world, client, mob, circuit, remote));
    world.flush();
    assert_eq!(world.get::<Shell>(remote).unwrap().circuit, Some(circuit));
    let ic = world.get::<IntegratedCircuit>(circuit).unwrap();
    assert!(ic.circuit.on);
    let kinds: Vec<Kind> = ic.circuit.nodes.iter().map(|n| n.kind).collect();
    assert_eq!(kinds, vec![Kind::Radio, Kind::CompactRemote]);
    assert_eq!(ic.circuit.size, 1);
    // Turning on ran the radio once: it listens on the signaler frequency.
    assert_eq!(ic.circuit.nodes[0].state.radio_freq, Some(1457));
    // The remote's Signal into the radio's Send.
    assert_eq!(
        wm::with_circuit(world, circuit, |c, ctx| c.connect((0, 3), (1, 1), ctx)),
        Some(true)
    );

    // A signaler-timer holder on the default frequency and code.
    let receiver = s.spawn(SIGNALER, pos);
    let timer = s.spawn(TIMER, pos);
    let driver = s.give(mob, SCREWDRIVER);
    let world = s.world();
    assert!(asm::item_interaction(world, client, mob, driver, receiver));
    assert!(asm::item_interaction(world, client, mob, driver, timer));
    assert!(asm::item_interaction(world, client, mob, timer, receiver));
    world.flush();
    assert!(
        world
            .query_filtered::<Entity, With<AssemblyHolder>>()
            .single(world)
            .is_ok()
    );

    // The button: the radio sends on the next circuit step, the signaler pulses the timer.
    assert!(wm::attack_self(world, client, mob, remote));
    s.run_until("the timer started", |s| s.timing(timer));
    let charge = s.get::<ItemCell>(cell).0.charge;
    assert!(charge < 1000.0, "runs draw from the cell");

    // A screwdriver takes the circuit out: off, without the remote's component.
    let world = s.world();
    assert!(wm::item_interaction(world, client, mob, driver, remote));
    world.flush();
    assert_eq!(world.get::<Shell>(remote).unwrap().circuit, None);
    let ic = world.get::<IntegratedCircuit>(circuit).unwrap();
    assert!(!ic.circuit.on);
    assert_eq!(ic.circuit.nodes.len(), 1);
    assert_eq!(world.get::<TilePos>(circuit), Some(&pos));
}

#[test]
fn a_usb_cable_connects_a_bot_to_a_quantum_pad() {
    let (mut s, client, mob) = Station::start("usb");
    let pos = *s.get::<TilePos>(mob);
    let circuit = s.spawn(CIRCUIT, pos);
    let cell = s.spawn(CELL, pos);
    let bot = s.spawn(BOT, pos);
    let remote = s.spawn(REMOTE, pos);
    let cable = s.spawn(CABLE, pos);
    let pad = s.spawn(PAD, pos);
    let other = s.spawn(PAD, pos);
    let world = s.world();
    world
        .entity_mut(cell)
        .insert(ItemCell(ss13_game::power::Cell {
            charge: 1000.0,
            maxcharge: 1000.0,
            chargerate: 0.0,
        }));
    // The fixture has no power grid.
    for p in [pad, other] {
        world.entity_mut(p).remove::<ss13_game::power::Unpowered>();
    }
    world.entity_mut(pad).insert(QuantumPad {
        linked: Some(other),
        ..default()
    });

    // A remote has no USB port; the bot does.
    assert!(dv::cable_interaction(world, client, mob, cable, remote));
    assert_eq!(world.get::<UsbCable>(cable).unwrap().circuit, None);
    assert!(wm::item_interaction(world, client, mob, cell, circuit));
    assert!(wm::item_interaction(world, client, mob, circuit, bot));
    world.flush();
    assert_eq!(world.get::<Shell>(bot).unwrap().circuit, Some(circuit));

    // The cable takes the bot's circuit, then the pad's port.
    assert!(dv::cable_interaction(world, client, mob, cable, bot));
    assert_eq!(world.get::<UsbCable>(cable).unwrap().circuit, Some(circuit));
    assert!(dv::cable_interaction(world, client, mob, cable, pad));
    world.flush();
    let index = {
        let ic = world.get::<IntegratedCircuit>(circuit).unwrap();
        ic.circuit
            .nodes
            .iter()
            .position(|n| n.kind == Kind::QuantumPad && n.usb == Some(pad))
            .expect("the pad's component joined the circuit")
    };
    assert_eq!(world.get::<UsbAttached>(pad), Some(&UsbAttached(circuit)));
    assert_eq!(world.get::<TilePos>(cable), None, "the cable went inside");

    // The component's Trigger teleports through the pad's link.
    assert!(
        wm::with_circuit(world, circuit, |c, ctx| c.manual_input(
            index,
            1,
            Some(""),
            ctx
        ))
        .is_some()
    );
    assert!(world.get::<QuantumPad>(pad).unwrap().charging.is_some());

    // Ejecting the component pulls the cable out at the machine.
    dv::detach(world, circuit, pad);
    world.flush();
    assert_eq!(world.get::<UsbAttached>(pad), None);
    assert_eq!(world.get::<TilePos>(cable), Some(&pos));
    let ic = world.get::<IntegratedCircuit>(circuit).unwrap();
    assert!(!ic.circuit.nodes.iter().any(|n| n.usb.is_some()));
}
