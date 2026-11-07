use super::*;

/// `synthesizer/default_reagents`.
pub(super) const DEFAULT_REAGENTS: &[&str] = &[
    "/datum/reagent/aluminium",
    "/datum/reagent/bromine",
    "/datum/reagent/carbon",
    "/datum/reagent/chlorine",
    "/datum/reagent/copper",
    "/datum/reagent/consumable/ethanol",
    "/datum/reagent/fluorine",
    "/datum/reagent/hydrogen",
    "/datum/reagent/iodine",
    "/datum/reagent/iron",
    "/datum/reagent/lithium",
    "/datum/reagent/mercury",
    "/datum/reagent/nitrogen",
    "/datum/reagent/oxygen",
    "/datum/reagent/phosphorus",
    "/datum/reagent/potassium",
    "/datum/reagent/uranium/radium",
    "/datum/reagent/sodium",
    "/datum/reagent/stable_plasma",
    "/datum/reagent/consumable/sugar",
    "/datum/reagent/sulfur",
    "/datum/reagent/toxin/acid",
    "/datum/reagent/water",
    "/datum/reagent/fuel",
];

/// `synthesizer/soda/soda_reagents`.
pub(super) const SODA_REAGENTS: &[&str] = &[
    "/datum/reagent/consumable/coffee",
    "/datum/reagent/consumable/space_cola",
    "/datum/reagent/consumable/cream",
    "/datum/reagent/consumable/dr_gibb",
    "/datum/reagent/consumable/grenadine",
    "/datum/reagent/consumable/ice",
    "/datum/reagent/consumable/icetea",
    "/datum/reagent/consumable/lemonjuice",
    "/datum/reagent/consumable/lemon_lime",
    "/datum/reagent/consumable/limejuice",
    "/datum/reagent/consumable/menthol",
    "/datum/reagent/consumable/orangejuice",
    "/datum/reagent/consumable/pineapplejuice",
    "/datum/reagent/consumable/pwr_game",
    "/datum/reagent/consumable/shamblers",
    "/datum/reagent/consumable/spacemountainwind",
    "/datum/reagent/consumable/sodawater",
    "/datum/reagent/consumable/space_up",
    "/datum/reagent/consumable/sugar",
    "/datum/reagent/consumable/tea",
    "/datum/reagent/consumable/tomatojuice",
    "/datum/reagent/consumable/tonic",
    "/datum/reagent/water",
];

/// `synthesizer/beer/beer_reagents`.
pub(super) const BEER_REAGENTS: &[&str] = &[
    "/datum/reagent/consumable/ethanol/absinthe",
    "/datum/reagent/consumable/ethanol/ale",
    "/datum/reagent/consumable/ethanol/applejack",
    "/datum/reagent/consumable/ethanol/beer",
    "/datum/reagent/consumable/ethanol/cognac",
    "/datum/reagent/consumable/ethanol/creme_de_cacao",
    "/datum/reagent/consumable/ethanol/creme_de_coconut",
    "/datum/reagent/consumable/ethanol/creme_de_menthe",
    "/datum/reagent/consumable/ethanol/curacao",
    "/datum/reagent/consumable/ethanol/gin",
    "/datum/reagent/consumable/ethanol/hcider",
    "/datum/reagent/consumable/ethanol/kahlua",
    "/datum/reagent/consumable/ethanol/beer/maltliquor",
    "/datum/reagent/consumable/ethanol/navy_rum",
    "/datum/reagent/consumable/ethanol/rum",
    "/datum/reagent/consumable/ethanol/sake",
    "/datum/reagent/consumable/ethanol/tequila",
    "/datum/reagent/consumable/ethanol/triple_sec",
    "/datum/reagent/consumable/ethanol/vermouth",
    "/datum/reagent/consumable/ethanol/vodka",
    "/datum/reagent/consumable/ethanol/whiskey",
    "/datum/reagent/consumable/ethanol/wine",
];

/// `synthesizer/mining/mining_chems`.
pub(super) const MINING_REAGENTS: &[&str] = &[
    "/datum/reagent/toxin/acid",
    "/datum/reagent/toxin/acid/fluacid",
    "/datum/reagent/toxin/acid/nitracid",
    "/datum/reagent/teslium",
    "/datum/reagent/fuel",
    "/datum/reagent/thermite",
    "/datum/reagent/gunpowder",
    "/datum/reagent/liquid_dark_matter",
    "/datum/reagent/toxin/acid/industrial_waste",
];

pub(super) const IV_PLUMBING: &str = "/obj/machinery/iv_drip/plumbing";

/// The component, buffer and reagent flags of a plumbing type (`Initialize`).
pub(super) fn plumber_for(path: &str) -> Option<(Plumber, f32, u32)> {
    let base = flags::TRANSPARENT | flags::NO_REACT;
    let sub = |p: &str| is_under(path, &format!("{PLUMBING}/{p}"));
    let (kind, ports, cap, buffer, flags) = if sub("input") {
        (
            Kind::Input,
            port(0, S),
            None,
            50.0,
            base | flags::REFILLABLE,
        )
    } else if sub("output") {
        (
            Kind::Output,
            port(S, 0),
            Some(5),
            50.0,
            base | flags::DRAINABLE,
        )
    } else if sub("tank") {
        (Kind::Tank, port(W, E), None, 400.0, base)
    } else if sub("layer_manifold") {
        let ports = (0..5)
            .map(|i| Port {
                layer: 1 << i,
                demand: N,
                supply: S,
                buffer: None,
            })
            .collect();
        (Kind::Manifold, ports, None, 50.0, base)
    } else if sub("liquid_pump") {
        (Kind::Pump, port(0, S), None, 200.0, base)
    } else if sub("acclimator") {
        let kind = Kind::Acclimator {
            target: 300.0,
            max_volume: 200.0,
            state: Acclimate::Filling,
        };
        (kind, port(W, E), None, 200.0, base)
    } else if sub("filter") {
        let kind = Kind::Filter {
            left: Vec::new(),
            right: Vec::new(),
        };
        (kind, port(N, S | E | W), None, 50.0, base)
    } else if sub("splitter") {
        let kind = Kind::Splitter {
            straight: 5.0,
            left: 5.0,
            right: 5.0,
        };
        (kind, port(N, S | E | W), None, 100.0, base)
    } else if sub("buffer") {
        let kind = Kind::Buffer {
            activation: 100.0,
            mode: BufferMode::Unready,
            links: Vec::new(),
        };
        (kind, port(W, E), None, 200.0, base)
    } else if sub("reaction_chamber") {
        let chem = sub("reaction_chamber/chem");
        let mut ports = port(N, S);
        // `buffered/acidic_input` (west, second layer), `alkaline_input` (east, fourth layer).
        let beaker = || Holder::new(50.0, flags::NO_REACT, Owner::default());
        let ph = chem.then(|| {
            ports.push(Port {
                layer: 1 << 1,
                demand: W,
                supply: 0,
                buffer: Some(true),
            });
            ports.push(Port {
                layer: 1 << 3,
                demand: E,
                supply: 0,
                buffer: Some(false),
            });
            PhControl {
                acidic_limit: 5.0,
                alkaline_limit: 9.0,
                acid: beaker(),
                base: beaker(),
            }
        });
        let kind = Kind::ReactionChamber {
            required: Vec::new(),
            catalysts: Vec::new(),
            emptying: false,
            target: 300.0,
            ph,
        };
        (kind, ports, None, 200.0, base)
    } else if sub("synthesizer") {
        let list = if sub("synthesizer/soda") {
            SODA_REAGENTS
        } else if sub("synthesizer/beer") {
            BEER_REAGENTS
        } else if sub("synthesizer/mining") {
            MINING_REAGENTS
        } else {
            DEFAULT_REAGENTS
        };
        let kind = Kind::Synthesizer {
            list,
            reagent: None,
            amount: 5.0,
        };
        (kind, port(0, S), None, 50.0, base)
    } else if sub("pill_press") {
        let kind = Kind::PillPress {
            volume: 10.0,
            product: "factory".to_owned(),
            package: Package::Pill,
            duration: crate::chem_master::PILL_LAYERS,
        };
        (kind, port(S, 0), Some(3), 50.0, base)
    } else if sub("bottler") {
        let kind = Kind::Bottler { wanted: 10.0 };
        (kind, port(S, 0), Some(3), 100.0, base | flags::DRAINABLE)
    } else if sub("fermenter") {
        (
            Kind::Fermenter,
            port(0, S),
            None,
            400.0,
            base | flags::DRAINABLE,
        )
    } else if sub("grinder_chemical") {
        (
            Kind::Grinder,
            port(0, S),
            None,
            400.0,
            base | flags::DRAINABLE,
        )
    } else if sub("sender") {
        (Kind::Sender { target: None }, port(S, 0), None, 50.0, base)
    } else if sub("receiver") {
        let kind = Kind::Receiver {
            senders: Vec::new(),
            next: 0,
        };
        (kind, port(0, S), None, 150.0, base)
    } else if sub("disposer") {
        let kind = Kind::Disposer {
            on: false,
            rate: 5.0,
        };
        (kind, port(S, 0), None, 50.0, base)
    } else if is_under(path, IV_PLUMBING) {
        // `internal_volume_maximum`, `create_reagents(..., TRANSPARENT)`.
        (Kind::IvDrip, port(S, N), Some(3), 100.0, flags::TRANSPARENT)
    } else {
        return None;
    };
    Some((Plumber { kind, ports, cap }, buffer, flags))
}

/// `duct/Initialize`, `plumbing/Initialize` (`create_reagents(buffer, reagent_flags)`, anchored
/// on the map), `stack/ducts`.
pub(super) fn add_plumbing(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    let entity = add.entity;
    if is_under(&dm.path, DUCT) {
        commands.queue(move |world: &mut World| {
            if world.get_entity(entity).is_ok() {
                let layer = layer_bit(world, entity);
                world.entity_mut(entity).insert(Duct { layer });
            }
        });
        return;
    }
    let Some((plumber, buffer, reagent_flags)) = plumber_for(&dm.path) else {
        return;
    };
    let owner = Owner {
        path: dm.path.clone(),
        living: false,
    };
    let iv = matches!(plumber.kind, Kind::IvDrip);
    commands
        .entity(entity)
        .insert((plumber, Reagents(Holder::new(buffer, reagent_flags, owner))));
    // The RPLD's layer lands after the spawn; the drip's own storage is its container.
    commands.queue(move |world: &mut World| {
        if world.get_entity(entity).is_err() {
            return;
        }
        let layer = layer_bit(world, entity);
        // The manifold and the chamber's buffer inputs keep their own layers.
        if let Some(mut p) = world.get_mut::<Plumber>(entity)
            && !matches!(p.kind, Kind::Manifold)
        {
            p.ports[0].layer = layer;
        }
        if iv && let Some(mut drip) = world.get_mut::<crate::medical::blood::IvDrip>(entity) {
            drip.container = Some(entity);
        }
    });
}
