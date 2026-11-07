use std::sync::Arc;

use super::*;
use crate::obj_damage::Anchored;

#[test]
fn connects_turn_with_the_machine() {
    // Drawn facing south: south stays south.
    assert_eq!(rotate(S, S), S);
    // `turn(SOUTH, 180 - dir2angle(NORTH))`.
    assert_eq!(rotate(S, N), N);
    assert_eq!(rotate(S, E), E);
    assert_eq!(rotate(W, E), S);
    for facing in CARDINALS {
        for d in CARDINALS {
            assert_eq!(unrotate(rotate(d, facing), facing), d);
        }
    }
}

fn machine(world: &mut World, path: &str, pos: TilePos, facing: Dir) -> Entity {
    let (plumber, ..) = plumber_for(path).unwrap();
    world
        .spawn((plumber, pos, Facing(facing), Anchored(true)))
        .id()
}

/// An input gate, two ducts and an output gate in a column, and an input right against an
/// output: two networks with one supplier and one demander each.
#[test]
fn ducts_join_machines_into_networks() {
    let mut world = World::new();
    let at = |x, y| TilePos { x, y, z: 0 };
    // `step(North)` is +y: the input's south supply faces the duct below it.
    let input = machine(
        &mut world,
        "/obj/machinery/plumbing/input",
        at(0, 3),
        Dir::South,
    );
    world.spawn((
        Duct {
            layer: DEFAULT_LAYER,
        },
        at(0, 2),
    ));
    world.spawn((
        Duct {
            layer: DEFAULT_LAYER,
        },
        at(0, 1),
    ));
    // A duct on another layer does not join.
    world.spawn((Duct { layer: 1 }, at(1, 1)));
    let output = machine(
        &mut world,
        "/obj/machinery/plumbing/output",
        at(0, 0),
        Dir::North,
    );
    let a = machine(
        &mut world,
        "/obj/machinery/plumbing/input",
        at(5, 1),
        Dir::South,
    );
    let b = machine(
        &mut world,
        "/obj/machinery/plumbing/output",
        at(5, 0),
        Dir::North,
    );
    // Unanchored machines stay out.
    let loose = machine(
        &mut world,
        "/obj/machinery/plumbing/output",
        at(1, 0),
        Dir::North,
    );
    world.entity_mut(loose).insert(Anchored(false));
    let nets = rebuild(&mut world);
    let ends = |e: Entity| {
        nets.iter()
            .filter(|n| {
                n.suppliers
                    .iter()
                    .chain(&n.demanders)
                    .any(|x| x.machine == e)
            })
            .count()
    };
    let piped = nets
        .iter()
        .find(|n| n.suppliers.iter().any(|s| s.machine == input))
        .expect("the input's network");
    assert_eq!(piped.demanders.len(), 1);
    assert_eq!(piped.demanders[0].machine, output);
    let direct = nets
        .iter()
        .find(|n| n.suppliers.iter().any(|s| s.machine == a))
        .expect("machine to machine");
    assert_eq!(direct.demanders[0].machine, b);
    assert_eq!(ends(loose), 0);
}

/// The chemistry of the full-cycle test: alpha + beta make gamma at once.
const CHEM_TYPES: &str = r##"{
  "schema": 3,
  "types": {
"/datum": { "parent": null },
"/datum/reagent": { "parent": "/datum", "declared_vars": {
    "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
    "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
    "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
    "chemical_flags": 0, "purity": 1, "creation_purity": 1, "inverse_chem": null,
    "inverse_chem_val": 0.25, "purge_multiplier": 1,
    "default_container": { "path": "/obj/item/reagent_containers/cup/bottle" } } },
"/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": { "name": "Water" } },
"/datum/reagent/alpha": { "parent": "/datum/reagent", "declared_vars": { "name": "Alpha" } },
"/datum/reagent/beta": { "parent": "/datum/reagent", "declared_vars": { "name": "Beta" } },
"/datum/reagent/gamma": { "parent": "/datum/reagent", "declared_vars": { "name": "Gamma" } },
"/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
    "H_ion_release": 0, "determin_ph_range": 4, "is_cold_recipe": 0,
    "mix_message": "The solution begins to bubble.", "mob_react": 1, "optimal_ph_max": 14,
    "optimal_ph_min": 0, "optimal_temp": 500, "overheat_temp": 900, "ph_exponent_factor": 2,
    "purity_min": 0.15, "rate_up_lim": 30, "reaction_flags": 0,
    "required_catalysts": { "list": [] }, "required_container": null,
    "required_container_accepts_subtypes": 0, "required_reagents": { "list": [] },
    "required_temp": 100, "results": { "list": [] }, "temp_exponent_factor": 2,
    "thermic_constant": 0 } },
"/datum/chemical_reaction/gamma": { "parent": "/datum/chemical_reaction",
    "declared_vars": { "required_temp": 0, "reaction_flags": 2,
    "results": { "list": [{ "assoc": [{ "path": "/datum/reagent/gamma" }, 2] }] },
    "required_reagents": { "list": [
        { "assoc": [{ "path": "/datum/reagent/alpha" }, 1] },
        { "assoc": [{ "path": "/datum/reagent/beta" }, 1] }] } } }
  }
}"##;

fn chem_world() -> (World, Arc<ss13_reagents::Chemistry>) {
    let types = ss13_mapload::convert::DmTypes::parse(CHEM_TYPES.as_bytes()).unwrap();
    let (data, _) = ss13_mapload::reagents::chem_data(&types);
    let chem = Arc::new(ss13_reagents::Chemistry::new(data));
    let mut world = World::new();
    world.insert_resource(Chem(chem.clone()));
    (world, chem)
}

fn with_reagents(world: &mut World, path: &str, pos: TilePos, facing: Dir) -> Entity {
    let (plumber, buffer, f) = plumber_for(path).unwrap();
    let holder = Holder::new(buffer, f, Owner::default());
    world
        .spawn((
            plumber,
            Reagents(holder),
            pos,
            Facing(facing),
            Anchored(true),
        ))
        .id()
}

fn amount(world: &World, e: Entity, chem: &ss13_reagents::Chemistry, path: &str) -> f32 {
    let id = chem.id(path).unwrap();
    world.get::<Reagents>(e).unwrap().0.amount(id)
}

/// Input gate → duct → reaction chamber (alpha 5, beta 5, water 5 as a catalyst) → output
/// gate: the chamber fills one reagent per tick, reacts, sends the gamma on and keeps its
/// catalyst water.
#[test]
fn input_duct_chamber_output_cycle() {
    let (mut world, chem) = chem_world();
    let at = |x, y| TilePos { x, y, z: 0 };
    let input = with_reagents(
        &mut world,
        "/obj/machinery/plumbing/input",
        at(0, 3),
        Dir::South,
    );
    world.spawn((
        Duct {
            layer: DEFAULT_LAYER,
        },
        at(0, 2),
    ));
    let chamber = with_reagents(
        &mut world,
        "/obj/machinery/plumbing/reaction_chamber",
        at(0, 1),
        Dir::South,
    );
    let output = with_reagents(
        &mut world,
        "/obj/machinery/plumbing/output",
        at(0, 0),
        Dir::North,
    );
    let id = |p: &str| chem.id(p).unwrap();
    {
        let mut r = world.get_mut::<Reagents>(input).unwrap();
        for (p, v) in [
            ("/datum/reagent/alpha", 20.0),
            ("/datum/reagent/beta", 20.0),
            ("/datum/reagent/water", 10.0),
        ] {
            r.0.add_reagent(&chem, id(p), v, 300.0, None, None, true, false);
        }
    }
    if let Some(mut p) = world.get_mut::<Plumber>(chamber)
        && let Kind::ReactionChamber {
            required,
            catalysts,
            ..
        } = &mut p.kind
    {
        *required = vec![
            (id("/datum/reagent/alpha"), 5.0),
            (id("/datum/reagent/beta"), 5.0),
        ];
        *catalysts = vec![(id("/datum/reagent/water"), 5.0)];
    }
    // Water and alpha, then beta, then the reaction and the first half sent on.
    tick(&mut world, 1.0);
    assert_eq!(amount(&world, chamber, &chem, "/datum/reagent/water"), 5.0);
    assert_eq!(amount(&world, chamber, &chem, "/datum/reagent/alpha"), 5.0);
    tick(&mut world, 1.0);
    assert_eq!(amount(&world, chamber, &chem, "/datum/reagent/beta"), 5.0);
    for _ in 0..3 {
        tick(&mut world, 1.0);
    }
    assert_eq!(amount(&world, output, &chem, "/datum/reagent/gamma"), 10.0);
    assert_eq!(amount(&world, output, &chem, "/datum/reagent/water"), 0.0);
    // The catalyst stayed; the input gave its water only once.
    assert_eq!(amount(&world, chamber, &chem, "/datum/reagent/water"), 5.0);
    assert_eq!(amount(&world, input, &chem, "/datum/reagent/water"), 5.0);
}

#[test]
fn synthesizer_and_disposer() {
    let (mut world, chem) = chem_world();
    let at = |x, y| TilePos { x, y, z: 0 };
    let synth = with_reagents(
        &mut world,
        "/obj/machinery/plumbing/synthesizer",
        at(0, 1),
        Dir::South,
    );
    let disposer = with_reagents(
        &mut world,
        "/obj/machinery/plumbing/disposer",
        at(0, 0),
        Dir::North,
    );
    set_kind(
        &mut world,
        synth,
        Kind::Synthesizer {
            list: &["/datum/reagent/water"],
            reagent: chem.id("/datum/reagent/water"),
            amount: 5.0,
        },
    );
    tick(&mut world, 1.0);
    assert_eq!(amount(&world, synth, &chem, "/datum/reagent/water"), 5.0);
    // Off: the disposer takes nothing.
    tick(&mut world, 1.0);
    assert_eq!(world.get::<Reagents>(disposer).unwrap().0.total_volume, 0.0);
    set_kind(
        &mut world,
        disposer,
        Kind::Disposer {
            on: true,
            rate: 5.0,
        },
    );
    tick(&mut world, 1.0);
    assert_eq!(amount(&world, disposer, &chem, "/datum/reagent/water"), 5.0);
    // Destroyed at its rate the next tick.
    tick(&mut world, 1.0);
    assert!(amount(&world, disposer, &chem, "/datum/reagent/water") <= 5.0);
}

#[test]
fn receiver_pulls_from_linked_sender() {
    let (mut world, chem) = chem_world();
    let at = |x, y| TilePos { x, y, z: 0 };
    let sender = with_reagents(
        &mut world,
        "/obj/machinery/plumbing/sender",
        at(0, 0),
        Dir::South,
    );
    let receiver = with_reagents(
        &mut world,
        "/obj/machinery/plumbing/receiver",
        at(9, 9),
        Dir::South,
    );
    let water = chem.id("/datum/reagent/water").unwrap();
    world
        .get_mut::<Reagents>(sender)
        .unwrap()
        .0
        .add_reagent(&chem, water, 30.0, 300.0, None, None, true, false);
    link_sender(&mut world, sender, receiver);
    tick(&mut world, 1.0);
    assert_eq!(
        amount(&world, receiver, &chem, "/datum/reagent/water"),
        RECEIVER_PULL
    );
    assert_eq!(amount(&world, sender, &chem, "/datum/reagent/water"), 10.0);
}

#[test]
fn filter_directions() {
    assert!(filter_allows(&[1], &[2], S, 3));
    assert!(!filter_allows(&[1], &[2], S, 1));
    assert!(filter_allows(&[1], &[2], E, 1));
    assert!(filter_allows(&[1], &[2], W, 2));
    assert!(!filter_allows(&[1], &[2], W, 1));
}

#[test]
fn machine_types() {
    let (p, buffer, f) = plumber_for("/obj/machinery/plumbing/tank").unwrap();
    assert_eq!(p.kind, Kind::Tank);
    assert_eq!(buffer, 400.0);
    assert_eq!(f & flags::NO_REACT, flags::NO_REACT);
    let (p, ..) = plumber_for("/obj/machinery/plumbing/output/tap").unwrap();
    assert_eq!(p.cap, Some(5));
    let (p, ..) = plumber_for("/obj/machinery/plumbing/layer_manifold").unwrap();
    assert_eq!(p.ports.len(), 5);
    assert!(plumber_for("/obj/machinery/plumbing/synthesizer").is_some());
}
