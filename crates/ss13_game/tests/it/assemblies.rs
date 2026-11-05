//! Assemblies on a fixture station: a signal pulses a signaler-timer holder (wrong codes do
//! not), a timer runs out and pulses; a chemical grenade casing takes a timer on its wire and a
//! beaker, locks, primes when the timer pulses and goes off, staying as a used casing until a
//! screwdriver resets it; a transfer valve with two tanks and a signaler merges the tanks' gas
//! on a signal and splits it when closed; a player running onto an armed mousetrap sets it
//! off (the shoes protect).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::{GasId, GasMixture};
use ss13_game::assemblies::chem_grenade::{self, Casing, Stage};
use ss13_game::assemblies::server::{self as asm, Assembly, AssemblyHolder, Attached, Device};
use ss13_game::assemblies::ttv::{self, Valve};
use ss13_game::chem::Reagents;
use ss13_game::grenades::{Armed, Grenade};
use ss13_game::internals::TankAir;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::wires::Wires;
use ss13_reagents::{Holder, Owner};
use ss13_shared::{Dir, TilePos};

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
        "icon_state": "screwdriver", "tool_behaviour": "screwdriver" } },
    "/obj/item/assembly": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "" } },
    "/obj/item/assembly/signaler": { "parent": "/obj/item/assembly", "declared_vars": {
        "icon_state": "signaller" } },
    "/obj/item/assembly/timer": { "parent": "/obj/item/assembly", "declared_vars": {
        "icon_state": "timer" } },
    "/obj/item/assembly/mousetrap": { "parent": "/obj/item/assembly", "declared_vars": {
        "icon_state": "mousetrap" } },
    "/obj/item/assembly/mousetrap/armed": { "parent": "/obj/item/assembly/mousetrap",
        "declared_vars": { "icon_state": "mousetraparmed" } },
    "/obj/item/assembly_holder": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "assembly_holder" } },
    "/obj/item/grenade": { "parent": "/obj/item" },
    "/obj/item/grenade/chem_grenade": { "parent": "/obj/item/grenade", "declared_vars": {
        "icon": { "resource": "g.dmi" }, "icon_state": "chemg" } },
    "/obj/item/reagent_containers": { "parent": "/obj/item" },
    "/obj/item/reagent_containers/cup": { "parent": "/obj/item/reagent_containers" },
    "/obj/item/reagent_containers/cup/beaker": { "parent": "/obj/item/reagent_containers/cup" },
    "/obj/item/tank": { "parent": "/obj/item" },
    "/obj/item/transfer_valve": { "parent": "/obj/item" },
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

const SIGNALER: &str = "/obj/item/assembly/signaler";
const TIMER: &str = "/obj/item/assembly/timer";
const MOUSETRAP: &str = "/obj/item/assembly/mousetrap/armed";
const SCREWDRIVER: &str = "/obj/item/screwdriver";
const CASING: &str = "/obj/item/grenade/chem_grenade";
const BEAKER: &str = "/obj/item/reagent_containers/cup/beaker";
const TANK: &str = "/obj/item/tank";
const VALVE: &str = "/obj/item/transfer_valve";

const STATES: [(&str, &str); 9] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "screwdriver"),
    ("a.dmi", "signaller"),
    ("a.dmi", "timer"),
    ("g.dmi", "chemg"),
];

fn fixture(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("assemblies_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        SIGNALER,
        TIMER,
        MOUSETRAP,
        SCREWDRIVER,
        CASING,
        BEAKER,
        TANK,
        VALVE,
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
    /// The station with one player; returns it with the player's connection and mob.
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

    fn device(&self, a: Entity) -> Device {
        self.get::<Assembly>(a).device.clone()
    }

    fn timing(&self, a: Entity) -> bool {
        matches!(self.device(a), Device::Timer { timing: true, .. })
    }
}

#[test]
fn signals_pulse_holders() {
    let (mut s, client, mob) = Station::start("signal");
    let pos = *s.get::<TilePos>(mob);
    let sender = s.spawn(SIGNALER, pos);
    let receiver = s.spawn(SIGNALER, pos);
    let timer = s.spawn(TIMER, pos);
    let driver = s.give(mob, SCREWDRIVER);
    let world = s.world();
    assert!(asm::item_interaction(world, client, mob, driver, receiver));
    assert!(asm::item_interaction(world, client, mob, driver, timer));
    assert!(!world.get::<Assembly>(receiver).unwrap().secured);
    // The timer onto the receiver: a holder of both on the floor, both secured again.
    assert!(asm::item_interaction(world, client, mob, timer, receiver));
    world.flush();
    let holder = world
        .query_filtered::<Entity, With<AssemblyHolder>>()
        .single(world)
        .unwrap();
    assert_eq!(world.get::<TilePos>(holder), Some(&pos));
    assert_eq!(asm::parts_of(world, holder), vec![receiver, timer]);
    for part in [receiver, timer] {
        let a = world.get::<Assembly>(part).unwrap();
        assert!(a.secured);
        assert_eq!(a.attached, Attached::Holder(holder));
    }

    // The default frequency and code reach the receiver; it pulses the timer, which starts.
    asm::signal(world, sender);
    assert!(s.timing(timer));

    // Another code does nothing.
    if let Some(mut a) = s.world().get_mut::<Assembly>(sender)
        && let Device::Signaler { code, .. } = &mut a.device
    {
        *code = 5;
    }
    s.run_for(3.5);
    asm::signal(s.world(), sender);
    assert!(s.timing(timer), "a wrong code pulsed the holder");

    // The right code again, after the 3 s cooldown: the timer stops.
    if let Some(mut a) = s.world().get_mut::<Assembly>(sender)
        && let Device::Signaler { code, .. } = &mut a.device
    {
        *code = 30;
    }
    asm::signal(s.world(), sender);
    assert!(!s.timing(timer));

    // A 1 s timer runs out, beeps and is set back to its time.
    if let Some(mut a) = s.world().get_mut::<Assembly>(timer)
        && let Device::Timer {
            timing,
            time,
            saved_time,
            ..
        } = &mut a.device
    {
        (*timing, *time, *saved_time) = (true, 1.0, 1.0);
    }
    s.run_until("the timer ran out", |s| !s.timing(timer));
    assert!(matches!(s.device(timer), Device::Timer { time, .. } if time == 1.0));

    // A screwdriver takes the holder apart: both parts drop.
    let world = s.world();
    assert!(asm::item_interaction(world, client, mob, driver, holder));
    world.flush();
    assert!(world.get_entity(holder).is_err());
    assert_eq!(world.get::<TilePos>(receiver), Some(&pos));
    assert_eq!(
        world.get::<Assembly>(timer).unwrap().attached,
        Attached::None
    );
}

#[test]
fn a_timer_on_a_chemical_grenade_sets_it_off() {
    let (mut s, client, mob) = Station::start("chem_grenade");
    let pos = *s.get::<TilePos>(mob);
    let casing = s.spawn(CASING, pos);
    let timer = s.spawn(TIMER, pos);
    let beaker = s.spawn(BEAKER, pos);
    let world = s.world();
    world
        .entity_mut(beaker)
        .insert(Reagents(Holder::new(50.0, 0, Owner::default())));
    assert_eq!(
        chem_grenade::casing(world, casing).map(|c| c.stage),
        Some(Stage::Empty)
    );
    // No cable stack in the fixture: wired by hand.
    world.get_mut::<Casing>(casing).unwrap().stage = Stage::Wired;
    if let Some(mut a) = world.get_mut::<Assembly>(timer)
        && let Device::Timer {
            time, saved_time, ..
        } = &mut a.device
    {
        (*time, *saved_time) = (1.0, 1.0);
    }
    let color = world.get::<Wires>(casing).unwrap().colors[0].0;
    ss13_game::wires::attach_assembly(world, casing, color, timer);
    assert_eq!(
        world.get::<Assembly>(timer).unwrap().attached,
        Attached::Wires(casing)
    );
    // `attach_assembly`: the fuse is the timer's.
    assert_eq!(world.get::<Grenade>(casing).unwrap().det_time, 1.0);

    // A beaker goes in; a screwdriver locks it.
    assert!(asm::item_interaction(world, client, mob, beaker, casing));
    assert_eq!(chem_grenade::beakers_of(world, casing), vec![beaker]);
    let driver = s.give(mob, SCREWDRIVER);
    let world = s.world();
    assert!(asm::item_interaction(world, client, mob, driver, casing));
    assert_eq!(world.get::<Casing>(casing).unwrap().stage, Stage::Ready);

    // The timer pulses the wire: the grenade primes, then goes off and stays, used up.
    if let Some(mut a) = world.get_mut::<Assembly>(timer)
        && let Device::Timer { timing, .. } = &mut a.device
    {
        *timing = true;
    }
    s.run_until("primed", |s| s.try_get::<Armed>(casing).is_some());
    s.run_until("went off", |s| s.try_get::<Grenade>(casing).unwrap().used);
    assert!(s.try_get::<Armed>(casing).is_none());
    assert_eq!(s.get::<Casing>(casing).stage, Stage::Ready);

    // The screwdriver resets the trigger.
    let world = s.world();
    assert!(asm::item_interaction(world, client, mob, driver, casing));
    assert!(!world.get::<Grenade>(casing).unwrap().used);
}

fn tank_air(gas: GasId, moles: f64) -> TankAir {
    let mut air = GasMixture::vacuum();
    air.add(gas, moles);
    air.temperature = 293.15;
    TankAir {
        air,
        volume: 70.0,
        distribute_pressure: 16.0,
    }
}

#[test]
fn a_signal_opens_a_transfer_valve() {
    let (mut s, client, mob) = Station::start("ttv");
    let pos = *s.get::<TilePos>(mob);
    let valve = s.spawn(VALVE, pos);
    let one = s.spawn(TANK, pos);
    let two = s.spawn(TANK, pos);
    let sender = s.spawn(SIGNALER, pos);
    let trigger = s.spawn(SIGNALER, pos);
    let driver = s.give(mob, SCREWDRIVER);
    let world = s.world();
    world.entity_mut(one).insert(tank_air(GasId::PLASMA, 5.0));
    world.entity_mut(two).insert(tank_air(GasId::O2, 10.0));
    assert!(ttv::item_interaction(world, client, mob, one, valve));
    assert!(ttv::item_interaction(world, client, mob, two, valve));
    // A secured signaler does not go on; unsecured, it does (and is secured).
    assert!(ttv::item_interaction(world, client, mob, trigger, valve));
    assert_eq!(world.get::<Valve>(valve).unwrap().device, None);
    assert!(asm::item_interaction(world, client, mob, driver, trigger));
    assert!(ttv::item_interaction(world, client, mob, trigger, valve));
    let v = world.get::<Valve>(valve).unwrap().clone();
    assert_eq!(
        (v.tank_one, v.tank_two, v.device),
        (Some(one), Some(two), Some(trigger))
    );
    assert_eq!(
        world.get::<Assembly>(trigger).unwrap().attached,
        Attached::Valve(valve)
    );

    // The signal opens the valve: the primary tank's gas joins the secondary's volume.
    asm::signal(world, sender);
    assert!(world.get::<Valve>(valve).unwrap().open);
    let merged = world.get::<TankAir>(two).unwrap();
    assert_eq!(merged.volume, 140.0);
    assert!((merged.air.get(GasId::PLASMA) - 5.0).abs() < 1e-6);
    assert!((merged.air.get(GasId::O2) - 10.0).abs() < 1e-6);
    assert!(world.get::<TankAir>(one).unwrap().air.total_moles() < 1e-9);

    // Closing it splits the mixture back by volume.
    ttv::toggle_valve(world, valve);
    assert!(!world.get::<Valve>(valve).unwrap().open);
    let (a, b) = (
        world.get::<TankAir>(one).unwrap(),
        world.get::<TankAir>(two).unwrap(),
    );
    assert_eq!((a.volume, b.volume), (70.0, 70.0));
    assert!((a.air.get(GasId::O2) - 5.0).abs() < 1e-3);
    assert!((b.air.get(GasId::PLASMA) - 2.5).abs() < 1e-3);
}

#[test]
fn running_onto_a_mousetrap_sets_it_off() {
    let (mut s, _, mob) = Station::start("mousetrap");
    let pos = *s.get::<TilePos>(mob);
    let trap = s.spawn(MOUSETRAP, pos.step(Dir::East));
    assert_eq!(s.device(trap), Device::Mousetrap { armed: true });
    s.run_until("stepped on the trap", |s| {
        if *s.get::<TilePos>(mob) == pos {
            s.try_step(0, Dir::East);
        }
        s.device(trap) == Device::Mousetrap { armed: false }
    });
    assert_eq!(*s.get::<TilePos>(mob), pos.step(Dir::East));
}
