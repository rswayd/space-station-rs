//! Plumbing ducts and machines (server), after tg `code/modules/plumbing/ducts.dm`,
//! `plumbers/*.dm` and `code/datums/components/plumbing/*.dm`.
//!
//! A duct ([`Duct`], `/obj/machinery/duct`) sits on one of five ducting layers and joins the
//! ducts of its layer on the four neighbouring tiles into a network (`/datum/ductnet`). A
//! plumbing machine ([`Plumber`], `/datum/component/plumbing`) has demand and supply connects
//! (drawn for a machine facing south, turned with its `dir`); while anchored, a connect touching
//! a duct of its layer, or the opposite connect of an adjacent machine, joins that network.
//! Every second (`SSplumbing`) each demander that is not full asks its networks for
//! `MACHINE_REAGENT_TRANSFER` units (`process_request`), taken evenly from every supplier that
//! can give (`can_give`, `transfer_to`).
//! - Ducts: a duct stack clicked on an open floor lays a duct of its layer (`check_attach_turf`,
//!   one per layer and tile), clicked on a duct takes it back; a wrench takes a duct apart into
//!   a stack of one; the stack used in hand cycles its layer.
//! - Machines: a wrench (2 s, out of combat mode) anchors or unanchors them; alt-click turns an
//!   unanchored one (`simple_rotation`).
//! - Input gate (refillable: containers pour into it), output gate and drinking tap (drainable,
//!   at most 5 distinct reagents), chemical tank (west in, east out, 400 u), layer manifold
//!   (north in, south out on every layer), liquid pump (supplies what it pumped from a geyser).
//! - Acclimator: fills to its volume, heats or cools to the target (`HEATER_COFFICIENT`), then
//!   empties. Filter: left and right reagent lists, the rest goes straight. Splitter: up to
//!   1..9 u straight, left and right per request. Buffer: fills to its activation volume, waits
//!   for the buffers linked to it (screwdriver), and all release together. Reaction chamber:
//!   takes its required reagents, then lets them react at its target temperature and empties.
//! - Disposer: when on, destroys `disposal_rate` u a second. Automated IV drip: its own 100 u
//!   storage is the drip's container, filled from the network while injecting, emptied into it
//!   while drawing.
//! - Production: synthesizers make their chosen reagent; the pill press packs pills, patches
//!   or bottles; the bottler fills cups from behind it; the fermenter distills produce on its
//!   tile; the chemical grinder grinds what is put in it. The chemical beacon sends to the
//!   recipient it is linked to by multitool (20 u a tick, beacons in turn).
//!
//! The machines' settings are prompts (an empty hand on the machine).
// shortcut: prompts (`tgui_input_list`/`number`/`text`) instead of the ChemAcclimator,
// ChemFilter, ChemSplitter, ChemReactionChamber, ChemSynthesizer, ChemPress and ChemDisposer
// windows; no duct colours (every duct is omni), no duct or connect overlays (only the duct's
// own `nduct_<dirs>` icon), no undertile hiding, no plunger flush, no welder deconstruction,
// no power use; the networks are rebuilt every plumbing tick
// (upgrade to tg's incremental merge/split when a map has thousands of ducts). Geysers are not
// ported (→ ../mining): a liquid pump has nothing to pump.
// Cost: every second one pass over the ducts and plumbing machines (none on most maps, a few
// dozen in chemistry) plus one request per demand connect.

use std::collections::HashMap;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmObj, Turf};
use ss13_reagents::{CHEMICAL_VOLUME_ROUNDING, Holder, Owner, ReagentId, flags, round_to};
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::SpatialIndex;

use crate::chem::{Chem, Reagents};
use crate::construction::{Job, path_of, spawn_stack};
use crate::devices::rhd::PlumbingLayer;
use crate::obj_damage::Anchored;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, Stack, ToolKind, do_after, tell, tool_of, use_stack};

pub const DUCT: &str = "/obj/machinery/duct";
pub const DUCT_STACK: &str = "/obj/item/stack/ducts";
const PLUMBING: &str = "/obj/machinery/plumbing";
const PILL_STYLE: &str = "/obj/item/reagent_containers/applicator/pill/style";
const PATCH_STYLE: &str = "/obj/item/reagent_containers/applicator/patch/style";
const BOTTLE: &str = "/obj/item/reagent_containers/cup/bottle";
const CUP: &str = "/obj/item/reagent_containers/cup";
const DART: &str = "/obj/item/ammo_casing/shotgun/dart";
const REAGENT_CONTAINER: &str = "/obj/item/reagent_containers";
const GROWN: &str = "/obj/item/food/grown";
const ACIDIC_BUFFER: &str = "/datum/reagent/reaction_agent/acidic_buffer";
const BASIC_BUFFER: &str = "/datum/reagent/reaction_agent/basic_buffer";
/// `BUFFER_IONIZING_STRENGTH`.
const BUFFER_IONIZING_STRENGTH: f32 = 30.0;
/// The pill press's `MIN_VOLUME`, `MAX_FLOOR_PRODUCTS`.
const PRESS_MIN_VOLUME: f32 = 5.0;
const MAX_FLOOR_PRODUCTS: usize = 10;
/// The receiver's `pull_amount`.
const RECEIVER_PULL: f32 = 20.0;
/// `synthesizer/possible_amounts`.
const SYNTH_MAX: f32 = 5.0;
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: u32 = 42;
/// `synthesizer/default_reagents`.
const DEFAULT_REAGENTS: &[&str] = &[
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
const SODA_REAGENTS: &[&str] = &[
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
const BEER_REAGENTS: &[&str] = &[
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
const MINING_REAGENTS: &[&str] = &[
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
const IV_PLUMBING: &str = "/obj/machinery/iv_drip/plumbing";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";
const CLICK: &str = "sound/machines/click.ogg";
/// `MACHINE_REAGENT_TRANSFER`.
const MACHINE_REAGENT_TRANSFER: f32 = 10.0;
/// `SSplumbing` wait, seconds (`SSFLUIDS_DT`).
const PLUMBING_WAIT: f32 = 1.0;
/// `DUCT_LAYER_DEFAULT` (`THIRD_DUCT_LAYER`).
const DEFAULT_LAYER: u8 = 1 << 2;
/// `default_unfasten_wrench` time, deciseconds.
const UNFASTEN_DS: f32 = 20.0;
/// `/obj/item/stack/ducts` `max_amount`.
const DUCT_STACK_MAX: u32 = 50;
/// The acclimator's `HEATER_COFFICIENT`, `TEMP_DIFF`; the reaction chamber's
/// `HEATER_COEFFICIENT`.
const ACCLIMATOR_COEFF: f32 = 0.3;
const ACCLIMATOR_TEMP_DIFF: f32 = 0.5;
const CHAMBER_COEFF: f32 = 0.05;
/// The splitter's `MAX_TRANSFER`, the disposer's `MAX_DISPOSAL_RATE`.
const SPLITTER_MAX: f32 = 9.0;
const DISPOSAL_MAX: f32 = 25.0;

const N: u8 = 1;
const S: u8 = 2;
const E: u8 = 4;
const W: u8 = 8;
const CARDINALS: [u8; 4] = [N, S, E, W];

/// `/obj/machinery/duct`: its `duct_layer` bit.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Duct {
    pub layer: u8,
}

/// `/obj/item/stack/ducts` `duct_layer` bit.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuctStack {
    pub layer: u8,
}

/// One `/datum/component/plumbing` of a machine: its layer bit and connects as drawn facing
/// south (`initial(demand_connects)`, `initial(supply_connects)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Port {
    pub layer: u8,
    pub demand: u8,
    pub supply: u8,
    /// `/datum/component/plumbing/buffered`: the chemistry reaction chamber's acidic (true) or
    /// alkaline (false) buffer input, received into that buffer instead of the machine.
    pub buffer: Option<bool>,
}

/// The chemistry reaction chamber's pH control (`reaction_chamber/chem`): the limits and the
/// two 50 u buffer beakers.
#[derive(Clone, Debug, PartialEq)]
pub struct PhControl {
    pub acidic_limit: f32,
    pub alkaline_limit: f32,
    pub acid: Holder,
    pub base: Holder,
}

/// The pill press's packaging (`packaging_category`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Package {
    Pill,
    Patch,
    Bottle,
}

impl Package {
    /// `GLOB.reagent_containers[category][1]` and its `volume`.
    fn container(self) -> (&'static str, f32) {
        match self {
            Self::Pill => (PILL_STYLE, 50.0),
            Self::Patch => (PATCH_STYLE, 40.0),
            Self::Bottle => (BOTTLE, 50.0),
        }
    }
    fn suffix(self) -> &'static str {
        match self {
            Self::Pill => "таблетка",
            Self::Patch => "пластырь",
            Self::Bottle => "бутылочка",
        }
    }
}

/// `acclimate_state`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acclimate {
    Filling,
    Heating,
    Cooling,
    Emptying,
}

/// The automatic buffer's `mode` (`AB_UNREADY`, `AB_IDLE`, `AB_READY`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferMode {
    Unready,
    Idle,
    Ready,
}

/// What a plumbing machine does with its reagents.
// One `Kind` per machine entity, a few hundred at most: boxing the big variants buys nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Input,
    Output,
    Tank,
    Manifold,
    Pump,
    Acclimator {
        target: f32,
        max_volume: f32,
        state: Acclimate,
    },
    Filter {
        left: Vec<ReagentId>,
        right: Vec<ReagentId>,
    },
    Splitter {
        straight: f32,
        left: f32,
        right: f32,
    },
    Buffer {
        activation: f32,
        mode: BufferMode,
        links: Vec<Entity>,
    },
    ReactionChamber {
        required: Vec<(ReagentId, f32)>,
        /// `catalysts`: kept in the chamber, never sent on.
        catalysts: Vec<(ReagentId, f32)>,
        emptying: bool,
        target: f32,
        ph: Option<PhControl>,
    },
    Synthesizer {
        /// `dispensable_reagents`.
        list: &'static [&'static str],
        reagent: Option<ReagentId>,
        amount: f32,
    },
    PillPress {
        volume: f32,
        product: String,
        package: Package,
        duration: f32,
    },
    Bottler {
        wanted: f32,
    },
    Fermenter,
    Grinder,
    Sender {
        target: Option<Entity>,
    },
    Receiver {
        senders: Vec<Entity>,
        next: usize,
    },
    Disposer {
        on: bool,
        rate: f32,
    },
    IvDrip,
}

/// A plumbing machine (`/datum/component/plumbing` on its parent).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Plumber {
    pub kind: Kind,
    pub ports: Vec<Port>,
    /// `distinct_reagent_cap` (`None`: `INFINITY`).
    pub cap: Option<usize>,
}

/// One end of a network: a machine's port and the connect's direction as drawn facing south.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct End {
    machine: Entity,
    port: usize,
    dir: u8,
}

#[derive(Default, Debug)]
struct Net {
    suppliers: Vec<End>,
    demanders: Vec<End>,
}

/// The networks of the last plumbing tick.
#[derive(Resource, Default)]
pub struct DuctNets {
    nets: Vec<Net>,
}

/// A machine settings prompt.
#[derive(Component, Clone, Debug)]
struct PlumbPrompt {
    machine: Entity,
    step: Step,
}

#[derive(Clone, Debug)]
enum Step {
    /// The machine's menu; the options in order.
    Menu(Vec<Choice>),
    /// A reagent for the filter's side (true: left) or the chamber's list (with the amount).
    Reagent { left: Option<bool>, amount: f32 },
    /// A number for a setting.
    Number(Setting),
    /// The synthesizer's reagent, out of its list.
    Synth(Vec<ReagentId>),
    /// The pill press's `product_name`.
    Name,
}

#[derive(Clone, Copy, Debug)]
enum Choice {
    Number(Setting),
    FilterAdd(bool),
    FilterRemove(bool, ReagentId),
    ChamberAdd,
    ChamberRemove(ReagentId),
    /// `catalyst`: a required reagent becomes a catalyst, or back.
    ChamberCatalyst(ReagentId),
    DisposerToggle,
    SynthReagent,
    PressPackage(Package),
    PressName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Setting {
    AcclimatorTarget,
    AcclimatorVolume,
    Splitter(u8),
    BufferActivation,
    ChamberTarget,
    ChamberAmount,
    DisposerRate,
    AcidicLimit,
    AlkalineLimit,
    SynthAmount,
    PressVolume,
    PillDuration,
    BottlerAmount,
}

pub struct PlumbingNetPlugin;

impl Plugin for PlumbingNetPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand`/`ui_interact` of the configurable plumbing machines.
        app.add_click_handler(ClickPhase::EmptyHand, 3097, |w, c| {
            crate::plumbing_net::interact(w, c.client, c.mob, c.target)
        });
        // Ducts and duct stacks, wrenches and screwdrivers on plumbing machines; before
        // `chem::item_interaction` so a wrench never pours.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3397, |w, c| {
            c.held.is_some_and(|h| {
                crate::plumbing_net::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        // `stack/ducts/attack_self`: the layer.
        app.add_click_handler(ClickPhase::AttackSelf, 3097, |w, c| {
            crate::plumbing_net::attack_self(w, c.client, c.target)
        });
        app.init_resource::<DuctNets>()
            .add_observer(add_plumbing)
            .add_systems(
                Update,
                (run_plumbing, answers).after(crate::click::ResolveClicks),
            );
    }
}

/// The layer bit of an RPLD layer index 1..=5 (`GLOB.plumbing_layers`).
fn layer_bit(world: &World, e: Entity) -> u8 {
    world
        .get::<PlumbingLayer>(e)
        .filter(|l| (1..=5).contains(&l.0))
        .map_or(DEFAULT_LAYER, |l| 1 << (l.0 - 1))
}

fn layer_index(bit: u8) -> u8 {
    bit.trailing_zeros() as u8 + 1
}

fn port(demand: u8, supply: u8) -> Vec<Port> {
    vec![Port {
        layer: DEFAULT_LAYER,
        demand,
        supply,
        buffer: None,
    }]
}

/// The component, buffer and reagent flags of a plumbing type (`Initialize`).
fn plumber_for(path: &str) -> Option<(Plumber, f32, u32)> {
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
fn add_plumbing(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
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

/// `stack/ducts` on a newly made item stack.
fn duct_stack_layer(world: &mut World, item: Entity) -> Option<u8> {
    if let Some(s) = world.get::<DuctStack>(item) {
        return Some(s.layer);
    }
    let path = path_of(world, item)?;
    if !is_under(&path, DUCT_STACK) {
        return None;
    }
    world.entity_mut(item).insert(DuctStack {
        layer: DEFAULT_LAYER,
    });
    Some(DEFAULT_LAYER)
}

fn dir_of(world: &World, e: Entity) -> u8 {
    world
        .get::<Facing>(e)
        .map(|f| f.0 as u8)
        .or_else(|| world.get::<DmAtom>(e).map(|a| a.dir))
        .filter(|d| CARDINALS.contains(d))
        .unwrap_or(S)
}

fn cw_index(d: u8) -> usize {
    match d {
        N => 0,
        E => 1,
        S => 2,
        _ => 3,
    }
}
const CW: [u8; 4] = [N, E, S, W];

/// `on_parent_dir_change`: a connect drawn facing south, for a machine facing `facing`.
pub fn rotate(dir: u8, facing: u8) -> u8 {
    CW[(cw_index(dir) + cw_index(facing) + 2) % 4]
}

/// `multidirectional/get_connection`: back from the world direction to the drawn one.
fn unrotate(dir: u8, facing: u8) -> u8 {
    CW[(cw_index(dir) + 6 - cw_index(facing)) % 4]
}

fn reverse(dir: u8) -> u8 {
    match dir {
        N => S,
        S => N,
        E => W,
        _ => E,
    }
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<Anchored>(e).is_some_and(|a| a.0)
}

/// The networks: ducts joined by layer, the anchored machines' connects into them, and
/// machine-to-machine connects (`duct/post_machine_initialize`, `plumbing/enable`). Updates the
/// ducts' icons.
fn rebuild(world: &mut World) -> Vec<Net> {
    let ducts: Vec<(Entity, TilePos, u8)> = world
        .query::<(Entity, &TilePos, &Duct)>()
        .iter(world)
        .map(|(e, p, d)| (e, *p, d.layer))
        .collect();
    let mut at: HashMap<(TilePos, u8), usize> = HashMap::new();
    for (i, &(_, pos, layer)) in ducts.iter().enumerate() {
        at.entry((pos, layer)).or_insert(i);
    }
    let mut net_of = vec![usize::MAX; ducts.len()];
    let mut nets: Vec<Net> = Vec::new();
    for start in 0..ducts.len() {
        if net_of[start] != usize::MAX {
            continue;
        }
        let id = nets.len();
        nets.push(Net::default());
        let mut queue = vec![start];
        net_of[start] = id;
        while let Some(i) = queue.pop() {
            let (_, pos, layer) = ducts[i];
            for d in CARDINALS {
                let Some(dir) = Dir::from_bits(d) else {
                    continue;
                };
                if let Some(&j) = at.get(&(pos.step(dir), layer))
                    && net_of[j] == usize::MAX
                {
                    net_of[j] = id;
                    queue.push(j);
                }
            }
        }
    }
    let machines: Vec<(Entity, TilePos, u8, Vec<Port>)> = world
        .query::<(Entity, &TilePos, &Plumber)>()
        .iter(world)
        .map(|(e, p, pl)| (e, *p, 0, pl.ports.clone()))
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|(e, ..)| anchored(world, *e))
        .map(|(e, p, _, ports)| (e, p, dir_of(world, e), ports))
        .collect();
    let mut machine_at: HashMap<TilePos, Vec<usize>> = HashMap::new();
    for (i, m) in machines.iter().enumerate() {
        machine_at.entry(m.1).or_default().push(i);
    }
    let mut duct_links: Vec<u8> = vec![0; ducts.len()];
    for &(machine, pos, facing, ref ports) in &machines {
        for (pi, p) in ports.iter().enumerate() {
            for drawn in CARDINALS {
                let demand = p.demand & drawn != 0;
                let supply = p.supply & drawn != 0;
                if !demand && !supply {
                    continue;
                }
                let world_dir = rotate(drawn, facing);
                let Some(step) = Dir::from_bits(world_dir) else {
                    continue;
                };
                let next = pos.step(step);
                let end = End {
                    machine,
                    port: pi,
                    dir: drawn,
                };
                if let Some(&j) = at.get(&(next, p.layer)) {
                    duct_links[j] |= reverse(world_dir);
                    let net = &mut nets[net_of[j]];
                    if supply {
                        net.suppliers.push(end);
                    }
                    if demand {
                        net.demanders.push(end);
                    }
                    continue;
                }
                // Machine to machine: our supply into their demand makes a two-end network.
                if !supply {
                    continue;
                }
                for &k in machine_at.get(&next).into_iter().flatten() {
                    let (other, _, other_facing, ref other_ports) = machines[k];
                    for (oi, op) in other_ports.iter().enumerate() {
                        if op.layer & p.layer == 0 {
                            continue;
                        }
                        let other_drawn = unrotate(reverse(world_dir), other_facing);
                        if op.demand & other_drawn != 0 {
                            nets.push(Net {
                                suppliers: vec![end],
                                demanders: vec![End {
                                    machine: other,
                                    port: oi,
                                    dir: other_drawn,
                                }],
                            });
                        }
                    }
                }
            }
        }
    }
    // `duct/update_icon_state`: connects to ducts and machines.
    for (i, &(e, pos, layer)) in ducts.iter().enumerate() {
        let mut connects = duct_links[i];
        for d in CARDINALS {
            if let Some(dir) = Dir::from_bits(d)
                && at.contains_key(&(pos.step(dir), layer))
            {
                connects |= d;
            }
        }
        let mut state = "nduct".to_owned();
        for (bit, s) in [(N, "_n"), (S, "_s"), (E, "_e"), (W, "_w")] {
            if connects & bit != 0 {
                state.push_str(s);
            }
        }
        let current = world
            .get::<ss13_mapload::DmAtom>(e)
            .map(|a| a.icon_state.clone());
        if current.as_deref() != Some(state.as_str()) {
            crate::obj_damage::set_icon_state(world, e, &state);
            if let Some(mut atom) = world.get_mut::<ss13_mapload::DmAtom>(e) {
                atom.icon_state = state;
            }
        }
    }
    nets
}

fn take_holder(world: &mut World, e: Entity) -> Option<Holder> {
    let mut r = world.get_mut::<Reagents>(e)?;
    Some(std::mem::replace(
        &mut r.0,
        Holder::new(0.0, 0, Owner::default()),
    ))
}

/// Both holders of a transfer, or neither (a missing one leaves the other in place).
fn take_pair(world: &mut World, a: Entity, b: Entity) -> Option<(Holder, Holder)> {
    if a == b || world.get::<Reagents>(a).is_none() || world.get::<Reagents>(b).is_none() {
        return None;
    }
    Some((take_holder(world, a)?, take_holder(world, b)?))
}

fn put_holder(world: &mut World, e: Entity, holder: Holder) {
    let reacting = holder.is_reacting;
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        r.0 = holder;
    }
    if reacting {
        world.entity_mut(e).insert(crate::chem::Reacting);
    }
}

fn holder(world: &World, e: Entity) -> Option<&Holder> {
    world.get::<Reagents>(e).map(|r| &r.0)
}

fn kind(world: &World, e: Entity) -> Option<&Kind> {
    world.get::<Plumber>(e).map(|p| &p.kind)
}

/// `filter/can_give_in_direction`.
fn filter_allows(left: &[ReagentId], right: &[ReagentId], dir: u8, id: ReagentId) -> bool {
    match dir {
        S => !left.contains(&id) && !right.contains(&id),
        E => left.contains(&id),
        W => right.contains(&id),
        _ => false,
    }
}

/// `can_give` of a supplier through its connect `dir`.
fn can_give(world: &World, end: End, amount: f32, reagent: Option<ReagentId>) -> bool {
    let Some(h) = holder(world, end.machine) else {
        return false;
    };
    if amount <= 0.0 {
        return false;
    }
    let base = match reagent {
        Some(id) => h.has(id).is_some(),
        None => h.total_volume > 0.0,
    };
    match kind(world, end.machine) {
        Some(Kind::Buffer { mode, .. }) => *mode == BufferMode::Ready && base,
        Some(Kind::Acclimator { state, .. }) => {
            *state == Acclimate::Emptying && !h.is_reacting && base
        }
        // `reaction_chamber/can_give`: only what exceeds the catalysts.
        Some(Kind::ReactionChamber {
            emptying,
            catalysts,
            ..
        }) => {
            *emptying
                && !h.is_reacting
                && h.reagents.iter().any(|r| {
                    reagent.is_none_or(|id| id == r.id)
                        && r.volume > catalyst_amount(catalysts, r.id)
                })
        }
        Some(Kind::Filter { left, right }) => {
            reagent.is_none_or(|id| filter_allows(left, right, end.dir, id)) && base
        }
        Some(Kind::IvDrip) => {
            base && world
                .get::<crate::medical::blood::IvDrip>(end.machine)
                .is_some_and(|d| !d.injecting)
        }
        _ => base,
    }
}

fn catalyst_amount(catalysts: &[(ReagentId, f32)], id: ReagentId) -> f32 {
    catalysts
        .iter()
        .find(|&&(c, _)| c == id)
        .map_or(0.0, |&(_, v)| v)
}

/// `get_catalyst_excluded_volume`.
fn catalyst_excluded_volume(h: &Holder, catalysts: &[(ReagentId, f32)]) -> f32 {
    let total: f32 = h
        .reagents
        .iter()
        .map(|r| (r.volume - catalyst_amount(catalysts, r.id)).max(0.0))
        .sum();
    round_to(total, CHEMICAL_VOLUME_ROUNDING).min(h.maximum_volume)
}

/// `transfer_to` from `from` into `to`.
fn transfer_to(world: &mut World, from: End, to: Entity, amount: f32, reagent: Option<ReagentId>) {
    let chem = world.resource::<Chem>().0.clone();
    let mut amount = amount;
    // The reagents that may leave and how much of each (`remove_blacklisted`, catalysts).
    let mut allowed: Option<Vec<(ReagentId, f32)>> = None;
    match kind(world, from.machine) {
        Some(Kind::Splitter {
            straight,
            left,
            right,
        }) => {
            let limit = match from.dir {
                S => *straight,
                E => *left,
                W => *right,
                _ => f32::INFINITY,
            };
            amount = amount.min(limit);
        }
        Some(Kind::Filter { left, right }) if reagent.is_none() => {
            let h = holder(world, from.machine);
            allowed = Some(
                h.map(|h| {
                    h.reagents
                        .iter()
                        .filter(|r| filter_allows(left, right, from.dir, r.id))
                        .map(|r| (r.id, r.volume))
                        .collect()
                })
                .unwrap_or_default(),
            );
        }
        // `/datum/reagents/plumbing/reaction_chamber/trans_to`: the catalysts stay.
        Some(Kind::ReactionChamber { catalysts, .. }) if !catalysts.is_empty() => {
            let h = holder(world, from.machine);
            allowed = Some(
                h.map(|h| {
                    h.reagents
                        .iter()
                        .filter(|r| reagent.is_none_or(|id| id == r.id))
                        .map(|r| (r.id, r.volume - catalyst_amount(catalysts, r.id)))
                        .filter(|&(_, v)| v > 0.0)
                        .collect()
                })
                .unwrap_or_default(),
            );
        }
        _ => {}
    }
    if from.machine == to {
        return;
    }
    let Some((mut a, mut b)) = take_pair(world, from.machine, to) else {
        return;
    };
    match allowed {
        // `remove_blacklisted`: only the allowed reagents, in proportion.
        Some(ids) => {
            let total: f32 = ids.iter().map(|&(_, v)| v).sum();
            if total > 0.0 {
                let amount = amount.min(total);
                for (id, available) in ids {
                    let part = available / total * amount;
                    a.trans_to(&chem, &mut b, part, 1.0, Some(id), false, false);
                }
            }
        }
        None => {
            a.trans_to(&chem, &mut b, amount, 1.0, reagent, false, false);
        }
    }
    put_holder(world, from.machine, a);
    put_holder(world, to, b);
}

/// `process_request`: an even share from every valid supplier of `net`.
fn process_request(
    world: &mut World,
    net: &Net,
    me: Entity,
    amount: f32,
    reagent: Option<ReagentId>,
) {
    let valid: Vec<End> = net
        .suppliers
        .iter()
        .copied()
        .filter(|s| s.machine != me && can_give(world, *s, amount, reagent))
        .collect();
    let Some(current) = holder(world, me).map(|h| h.total_volume) else {
        return;
    };
    let target = current + amount;
    let mut left = valid.len();
    for give in valid {
        let now = holder(world, me).map_or(target, |h| h.total_volume);
        let request = (target - now) / left as f32;
        transfer_to(world, give, me, request, reagent);
        left -= 1;
    }
}

/// `get_all_network_reagents`: ours first, then every supplier's.
fn network_reagents(world: &World, net: &Net, me: Entity) -> Vec<ReagentId> {
    let mut out: Vec<ReagentId> = Vec::new();
    let mut add = |h: Option<&Holder>| {
        for r in h.map(|h| h.reagents.as_slice()).unwrap_or_default() {
            if !out.contains(&r.id) {
                out.push(r.id);
            }
        }
    };
    add(holder(world, me));
    for s in &net.suppliers {
        add(holder(world, s.machine));
    }
    out
}

/// `send_request` of the base component (`distinct_reagent_cap`).
fn default_request(world: &mut World, net: &Net, me: Entity, cap: Option<usize>) {
    let Some(cap) = cap else {
        process_request(world, net, me, MACHINE_REAGENT_TRANSFER, None);
        return;
    };
    let mut all = network_reagents(world, net, me);
    all.truncate(cap);
    if all.is_empty() {
        return;
    }
    let per = round_to(
        MACHINE_REAGENT_TRANSFER / all.len() as f32,
        CHEMICAL_VOLUME_ROUNDING,
    );
    for id in all {
        process_request(world, net, me, per, Some(id));
    }
}

fn set_kind(world: &mut World, e: Entity, kind: Kind) {
    if let Some(mut p) = world.get_mut::<Plumber>(e) {
        p.kind = kind;
    }
}

fn set_flag(world: &mut World, e: Entity, flag: u32, on: bool) {
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        if on {
            r.0.flags |= flag;
        } else {
            r.0.flags &= !flag;
        }
    }
}

/// `send_request(dir)` of a demander's connect.
fn send_request(world: &mut World, net: &Net, end: End) {
    let me = end.machine;
    let Some(plumber) = world.get::<Plumber>(me).cloned() else {
        return;
    };
    if let Some(acidic) = plumber.ports.get(end.port).and_then(|p| p.buffer) {
        buffered_request(world, net, me, acidic);
        return;
    }
    match plumber.kind {
        Kind::Disposer { on, rate } => {
            if on {
                process_request(world, net, me, rate * PLUMBING_WAIT, None);
            }
        }
        Kind::Buffer { mode, .. } => {
            if mode == BufferMode::Unready {
                default_request(world, net, me, plumber.cap);
            }
        }
        Kind::Acclimator {
            target,
            max_volume,
            state: Acclimate::Filling,
        } => {
            let Some(h) = holder(world, me) else {
                return;
            };
            if h.total_volume < max_volume {
                default_request(world, net, me, plumber.cap);
                if !holder(world, me).is_some_and(Holder::holder_full) {
                    return;
                }
            }
            set_flag(world, me, flags::NO_REACT, false);
            let temp = holder(world, me).map_or(target, |h| h.chem_temp);
            let state = if temp > target {
                Acclimate::Cooling
            } else {
                Acclimate::Heating
            };
            set_kind(
                world,
                me,
                Kind::Acclimator {
                    target,
                    max_volume,
                    state,
                },
            );
        }
        Kind::Acclimator { .. } => {}
        Kind::ReactionChamber {
            required,
            catalysts,
            emptying,
            ..
        } => {
            if emptying || (required.is_empty() && catalysts.is_empty()) {
                return;
            }
            // `catalysts | required_reagents`: a missing catalyst is asked for without waiting.
            let wanted = catalysts
                .iter()
                .map(|&(id, v)| (id, v, true))
                .chain(required.iter().map(|&(id, v)| (id, v, false)));
            for (id, wanted, catalyst) in wanted.collect::<Vec<_>>() {
                let present = holder(world, me).map_or(0.0, |h| h.amount(id));
                let diff = (wanted - present).min(MACHINE_REAGENT_TRANSFER);
                if diff >= ss13_reagents::CHEMICAL_QUANTISATION_LEVEL {
                    process_request(world, net, me, diff, Some(id));
                    if !catalyst {
                        return;
                    }
                }
            }
            set_flag(world, me, flags::NO_REACT, false);
            let chem = world.resource::<Chem>().0.clone();
            let mut emptying = true;
            if let Some(mut h) = take_holder(world, me) {
                h.handle_reactions(&chem);
                // `on_reagent_change`: some reactions leave nothing to send on.
                if catalyst_excluded_volume(&h, &catalysts) <= 0.0 {
                    emptying = false;
                    h.flags |= flags::NO_REACT;
                }
                put_holder(world, me, h);
            }
            if let Some(mut p) = world.get_mut::<Plumber>(me)
                && let Kind::ReactionChamber { emptying: e, .. } = &mut p.kind
            {
                *e = emptying;
            }
        }
        // `pill_press/send_request`: not past the product's volume, nor with 10 containers on
        // the floor.
        Kind::PillPress { volume, .. } => {
            if holder(world, me).is_some_and(|h| h.total_volume >= volume) {
                return;
            }
            let Some(&pos) = world.get::<TilePos>(me) else {
                return;
            };
            let products = world
                .resource::<SpatialIndex>()
                .on_tile(pos)
                .iter()
                .filter(|&&e| {
                    world
                        .get::<ss13_mapload::DmItem>(e)
                        .is_some_and(|i| is_under(&i.path, REAGENT_CONTAINER))
                })
                .count();
            if products >= MAX_FLOOR_PRODUCTS {
                return;
            }
            default_request(world, net, me, plumber.cap);
        }
        Kind::IvDrip => {
            if world
                .get::<crate::medical::blood::IvDrip>(me)
                .is_some_and(|d| d.injecting)
            {
                default_request(world, net, me, plumber.cap);
            }
        }
        _ => default_request(world, net, me, plumber.cap),
    }
}

/// `buffered/send_request`: the chamber's acidic or alkaline buffer beaker asks for its
/// buffer. The beaker stands in for the chamber's holder while the request runs.
fn buffered_request(world: &mut World, net: &Net, me: Entity, acidic: bool) {
    let chem = world.resource::<Chem>().0.clone();
    let Some(id) = chem.id(if acidic { ACIDIC_BUFFER } else { BASIC_BUFFER }) else {
        return;
    };
    let Some(Kind::ReactionChamber { ph: Some(ph), .. }) = kind(world, me).cloned() else {
        return;
    };
    let beaker = if acidic { ph.acid } else { ph.base };
    if beaker.holder_full() {
        return;
    }
    let Some(main) = take_holder(world, me) else {
        return;
    };
    if let Some(mut r) = world.get_mut::<Reagents>(me) {
        r.0 = beaker;
    }
    process_request(world, net, me, MACHINE_REAGENT_TRANSFER, Some(id));
    let beaker = take_holder(world, me);
    put_holder(world, me, main);
    if let Some(beaker) = beaker
        && let Some(mut p) = world.get_mut::<Plumber>(me)
        && let Kind::ReactionChamber { ph: Some(ph), .. } = &mut p.kind
    {
        if acidic {
            ph.acid = beaker;
        } else {
            ph.base = beaker;
        }
    }
}

/// The machines' own `process` and the reagent-change hooks (`acclimator/clear`,
/// `reaction_chamber/on_reagent_change`, `buffer/on_reagent_change`, `disposer/process`).
fn process_machines(world: &mut World, secs: f32) {
    let chem = world.resource::<Chem>().0.clone();
    let machines: Vec<Entity> = world
        .query_filtered::<Entity, With<Plumber>>()
        .iter(world)
        .collect();
    for e in machines {
        if !anchored(world, e) {
            continue;
        }
        let Some(current) = kind(world, e).cloned() else {
            continue;
        };
        match current {
            Kind::Acclimator {
                target,
                max_volume,
                state,
            } => {
                let Some(mut h) = take_holder(world, e) else {
                    continue;
                };
                let mut state = state;
                if state == Acclimate::Emptying && h.total_volume <= 0.0 {
                    state = Acclimate::Filling;
                    h.flags |= flags::NO_REACT;
                } else if h.total_volume > 0.0
                    && matches!(state, Acclimate::Heating | Acclimate::Cooling)
                {
                    state = if h.chem_temp > target {
                        Acclimate::Cooling
                    } else {
                        Acclimate::Heating
                    };
                    let energy =
                        (target - h.chem_temp) * ACCLIMATOR_COEFF * secs * h.heat_capacity(&chem);
                    h.adjust_thermal_energy(
                        &chem,
                        energy,
                        0.0,
                        ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE,
                    );
                    h.handle_reactions(&chem);
                    let temp = round_to(h.chem_temp, 0.01);
                    if !h.is_reacting
                        && temp >= target - ACCLIMATOR_TEMP_DIFF
                        && temp <= target + ACCLIMATOR_TEMP_DIFF
                    {
                        h.set_temperature(target);
                        state = Acclimate::Emptying;
                    }
                }
                put_holder(world, e, h);
                set_kind(
                    world,
                    e,
                    Kind::Acclimator {
                        target,
                        max_volume,
                        state,
                    },
                );
            }
            Kind::ReactionChamber { .. } => chamber_process(world, e, secs),
            Kind::Synthesizer {
                reagent: Some(id),
                amount,
                ..
            } => {
                // `synthesizer/process`: `amount` more whenever it holds less.
                if amount > 0.0
                    && let Some(mut r) = world.get_mut::<Reagents>(e)
                    && r.0.total_volume < amount
                {
                    r.0.add_reagent(
                        &chem,
                        id,
                        amount,
                        ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                        None,
                        None,
                        false,
                        false,
                    );
                }
            }
            Kind::PillPress {
                volume,
                product,
                package,
                duration,
            } => press(world, e, volume, &product, package, duration),
            Kind::Bottler { wanted } => bottle(world, e, wanted),
            Kind::Fermenter => ferment_tile(world, e),
            Kind::Grinder => {
                for item in loose_items(world, e) {
                    blend(world, e, item);
                }
            }
            Kind::Receiver { senders, next } => {
                // `receiver/process`: one sender a tick, in turn.
                let senders: Vec<Entity> = senders
                    .into_iter()
                    .filter(|&s| matches!(kind(world, s), Some(Kind::Sender { target: Some(t) }) if *t == e))
                    .collect();
                if senders.is_empty() {
                    set_kind(world, e, Kind::Receiver { senders, next: 0 });
                    continue;
                }
                let index = if next >= senders.len() { 0 } else { next };
                let sender = senders[index];
                if let Some((mut a, mut b)) = take_pair(world, sender, e) {
                    a.trans_to(&chem, &mut b, RECEIVER_PULL, 1.0, None, false, false);
                    put_holder(world, sender, a);
                    put_holder(world, e, b);
                }
                set_kind(
                    world,
                    e,
                    Kind::Receiver {
                        senders,
                        next: index + 1,
                    },
                );
            }
            Kind::Disposer { on: true, rate } => {
                if let Some(mut r) = world.get_mut::<Reagents>(e) {
                    r.0.remove_all(rate * secs);
                }
            }
            Kind::Buffer { .. } => buffer_changed(world, e),
            _ => {}
        }
    }
}

/// `reaction_chamber/process`: holds the target temperature while filling or reacting, and
/// the chemistry chamber's `handle_reagents` keeps the pH between its limits from its
/// buffers; `on_reagent_change` ends the emptying.
fn chamber_process(world: &mut World, e: Entity, secs: f32) {
    let chem = world.resource::<Chem>().0.clone();
    let Some(Kind::ReactionChamber {
        catalysts,
        emptying,
        target,
        ph,
        ..
    }) = kind(world, e).cloned()
    else {
        return;
    };
    let Some(mut h) = take_holder(world, e) else {
        return;
    };
    let mut emptying = emptying;
    let mut ph = ph;
    if emptying && catalyst_excluded_volume(&h, &catalysts) <= 0.0 {
        emptying = false;
        h.flags |= flags::NO_REACT;
    }
    if h.total_volume > 0.0 && (!emptying || h.is_reacting) {
        let energy = (target - h.chem_temp) * CHAMBER_COEFF * secs * h.heat_capacity(&chem);
        h.adjust_thermal_energy(
            &chem,
            energy,
            0.0,
            ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE,
        );
        if let Some(ph) = ph.as_mut() {
            let n = h.reagents.len();
            let too_basic = h.ph > ph.alkaline_limit;
            if n > 0 && (h.ph < ph.acidic_limit || too_basic) {
                let change = if too_basic {
                    h.ph - ph.alkaline_limit
                } else {
                    ph.acidic_limit - h.ph
                }
                .max(0.25);
                let amount = change * h.total_volume / (BUFFER_IONIZING_STRENGTH * n as f32) * secs;
                let buffer = if too_basic {
                    &mut ph.acid
                } else {
                    &mut ph.base
                };
                buffer.trans_to(&chem, &mut h, amount, 1.0, None, false, false);
            }
        }
    }
    put_holder(world, e, h);
    if let Some(mut p) = world.get_mut::<Plumber>(e)
        && let Kind::ReactionChamber {
            emptying: em,
            ph: p_ph,
            ..
        } = &mut p.kind
    {
        *em = emptying;
        *p_ph = ph;
    }
}

/// Items lying loose on the machine's tile.
fn loose_items(world: &World, e: Entity) -> Vec<Entity> {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return Vec::new();
    };
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&i| i != e && world.get::<ss13_mapload::DmItem>(i).is_some())
        .filter(|&i| !anchored(world, i))
        .collect()
}

/// `pill_press/process`: a product of `volume` units on the floor.
fn press(world: &mut World, e: Entity, volume: f32, product: &str, package: Package, layers: f32) {
    if holder(world, e).is_none_or(|h| h.total_volume < volume) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let (path, _) = package.container();
    let Some(item) = crate::construction::spawn_obj(world, path, pos) else {
        warn!("pill press: no prototype for {path}");
        return;
    };
    if let Some(mut n) = world.get_mut::<ss13_mapload::DmName>(item) {
        n.name = format!("{product} {}", package.suffix());
    }
    let chem = world.resource::<Chem>().0.clone();
    if let Some((mut a, mut b)) = take_pair(world, e, item) {
        b.clear();
        a.trans_to(&chem, &mut b, volume, 1.0, None, false, false);
        put_holder(world, e, a);
        put_holder(world, item, b);
    }
    if package == Package::Pill {
        world
            .entity_mut(item)
            .insert(crate::chem::PillShell(layers));
    }
}

/// `bottler/process`: a cup or dart from behind is filled and put in front, or to the side
/// when it has no room.
// shortcut: the first item behind instead of `pick` among the tile's contents; slime extracts
// are not filled (→ ../xenobiology).
fn bottle(world: &mut World, e: Entity, wanted: f32) {
    if holder(world, e).is_none_or(|h| h.total_volume < wanted) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let facing = dir_of(world, e);
    let step = |d: u8| Dir::from_bits(d).map(|d| pos.step(d));
    // `setDir`: front, behind, and the side clockwise of the front.
    let (Some(good), Some(input), Some(bad)) = (
        step(facing),
        step(reverse(facing)),
        step(CW[(cw_index(facing) + 1) % 4]),
    ) else {
        return;
    };
    let item = world
        .resource::<SpatialIndex>()
        .on_tile(input)
        .iter()
        .copied()
        .find(|&i| world.get::<ss13_mapload::DmItem>(i).is_some() && !anchored(world, i));
    let Some(item) = item else {
        return;
    };
    let path = path_of(world, item).unwrap_or_default();
    if !is_under(&path, CUP) && !is_under(&path, DART) {
        return;
    }
    let fits = holder(world, item).is_some_and(|h| h.total_volume + wanted <= h.maximum_volume);
    if !fits {
        world.entity_mut(item).insert(bad);
        return;
    }
    let chem = world.resource::<Chem>().0.clone();
    if let Some((mut a, mut b)) = take_pair(world, e, item) {
        a.trans_to(&chem, &mut b, wanted, 1.0, None, false, false);
        put_holder(world, e, a);
        put_holder(world, item, b);
    }
    world.entity_mut(item).insert(good);
}

/// `fermenter/ferment` of the produce on its tile: `potency × 0.25` of its `distill_reagent`.
// shortcut: the tile is checked every plumbing tick (no `COMSIG_ATOM_ENTERED`), and the
// fermenter does not let items through its eating side.
fn ferment_tile(world: &mut World, e: Entity) {
    let chem = world.resource::<Chem>().0.clone();
    for item in loose_items(world, e) {
        if holder(world, e).is_none_or(Holder::holder_full) {
            return;
        }
        let path = path_of(world, item).unwrap_or_default();
        if !is_under(&path, GROWN) {
            continue;
        }
        let distill = world
            .get_resource::<crate::botany::Botany>()
            .and_then(|b| b.0.grown_def(&path).map(|d| d.distill_reagent.clone()))
            .unwrap_or_default();
        let Some(id) = chem.id(&distill) else {
            continue;
        };
        let potency = world
            .get::<crate::botany::seeds::Seed>(item)
            .map_or(0.0, |s| s.potency);
        if let Some(mut r) = world.get_mut::<Reagents>(e) {
            r.0.add_reagent(
                &chem,
                id,
                potency * 0.25,
                ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                None,
                None,
                false,
                false,
            );
        }
        world.despawn(item);
    }
}

/// `grinder_chemical/blend`: the item's reagents into the grinder (`/obj/item/proc/grind`),
/// and it is gone.
// shortcut: no juicing mode and no `grind_results` (as `crate::kitchen`'s grinder).
fn blend(world: &mut World, grinder: Entity, item: Entity) {
    if holder(world, grinder).is_none_or(Holder::holder_full) {
        return;
    }
    let chem = world.resource::<Chem>().0.clone();
    if let Some((mut a, mut b)) = take_pair(world, item, grinder) {
        let total = a.total_volume;
        a.trans_to(&chem, &mut b, total, 1.0, None, false, false);
        put_holder(world, item, a);
        put_holder(world, grinder, b);
    }
    world.despawn(item);
}

/// `buffer/on_reagent_change`: fill to the threshold, wait for the linked ones, release
/// together, and start over once all are empty.
fn buffer_changed(world: &mut World, e: Entity) {
    let Some(Kind::Buffer {
        activation,
        mode,
        links,
    }) = kind(world, e).cloned()
    else {
        return;
    };
    let volume = |w: &World, b: Entity| holder(w, b).map_or(0.0, |h| h.total_volume);
    let mode_of = |w: &World, b: Entity| match kind(w, b) {
        Some(Kind::Buffer { mode, .. }) => Some(*mode),
        _ => None,
    };
    let mut group: Vec<Entity> = links
        .iter()
        .copied()
        .filter(|&b| mode_of(world, b).is_some())
        .collect();
    if !group.contains(&e) {
        group.push(e);
    }
    let mut mode = mode;
    if mode == BufferMode::Unready && volume(world, e) >= activation {
        mode = BufferMode::Idle;
        set_buffer_mode(world, e, mode);
    }
    if mode == BufferMode::Idle {
        if group
            .iter()
            .any(|&b| mode_of(world, b) == Some(BufferMode::Unready))
        {
            return;
        }
        for &b in &group {
            set_buffer_mode(world, b, BufferMode::Ready);
        }
        mode = BufferMode::Ready;
    }
    if mode == BufferMode::Ready {
        if group.iter().any(|&b| volume(world, b) > 0.0) {
            return;
        }
        for &b in &group {
            set_buffer_mode(world, b, BufferMode::Unready);
        }
    }
}

fn set_buffer_mode(world: &mut World, e: Entity, new: BufferMode) {
    if let Some(mut p) = world.get_mut::<Plumber>(e)
        && let Kind::Buffer { mode, .. } = &mut p.kind
    {
        *mode = new;
    }
}

/// `SSplumbing`: every second the networks, the machines and the requests.
fn run_plumbing(world: &mut World, mut since: Local<f32>) {
    if !world.contains_resource::<Chem>() {
        return;
    }
    *since += world.resource::<Time>().delta_secs();
    if *since < PLUMBING_WAIT {
        return;
    }
    let secs = *since;
    *since = 0.0;
    tick(world, secs);
}

/// One `SSplumbing` fire: the networks, the machines' `process`, every demand connect's
/// request.
pub fn tick(world: &mut World, secs: f32) {
    let nets = rebuild(world);
    process_machines(world, secs);
    for net in &nets {
        for &d in &net.demanders {
            // A buffered input checks its own beaker.
            let buffered = world
                .get::<Plumber>(d.machine)
                .and_then(|p| p.ports.get(d.port))
                .is_some_and(|p| p.buffer.is_some());
            if !buffered && holder(world, d.machine).is_none_or(Holder::holder_full) {
                continue;
            }
            send_request(world, net, d);
        }
    }
    if let Some(mut r) = world.get_resource_mut::<DuctNets>() {
        r.nets = nets;
    }
}

fn sound(world: &mut World, e: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(e) {
        playsound(world, PlaySound::new(pos, file, vol, true));
    }
}

/// A duct of `layer` or a plumbing machine of that layer on `pos` (`ducting_layer_check`).
fn layer_taken(world: &World, pos: TilePos, layer: u8, except: Option<Entity>) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&e| Some(e) != except)
        .any(|&e| {
            world.get::<Duct>(e).is_some_and(|d| d.layer & layer != 0)
                || (anchored(world, e)
                    && world
                        .get::<Plumber>(e)
                        .is_some_and(|p| p.ports.iter().any(|p| p.layer & layer != 0)))
        })
}

/// `stack/ducts/attack_self`: the next layer (shortcut: a cycle instead of the two lists).
pub fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(layer) = duct_stack_layer(world, item) else {
        return false;
    };
    let next = if layer >= 1 << 4 { 1 } else { layer << 1 };
    world.entity_mut(item).insert(DuctStack { layer: next });
    tell(
        world,
        client,
        Span::Notice,
        format!("Слой дукта: {}.", layer_index(next)),
    );
    true
}

/// Duct stacks, wrenches on ducts and plumbing machines, screwdrivers on buffers. True when
/// handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let _ = modifiers;
    if let Some(layer) = duct_stack_layer(world, held) {
        // `stack/ducts/interact_with_atom`: a duct back into the stack.
        if world.get::<Duct>(target).is_some() {
            let amount = world.get::<Stack>(held).map_or(0, |s| s.amount);
            if amount >= DUCT_STACK_MAX {
                tell(world, client, Span::Warning, "Стопка заполнена!");
                return true;
            }
            world.despawn(target);
            if let Some(mut s) = world.get_mut::<Stack>(held) {
                s.amount += 1;
            }
            return true;
        }
        // `check_attach_turf`.
        let open = world.get::<Turf>(target).is_some()
            && path_of(world, target).is_some_and(|p| is_under(&p, "/turf/open"));
        if !open {
            return false;
        }
        let Some(&pos) = world.get::<TilePos>(target) else {
            return true;
        };
        if layer_taken(world, pos, layer, None) {
            tell(
                world,
                client,
                Span::Warning,
                "Здесь уже есть дукт или машина!",
            );
            return true;
        }
        if !use_stack(world, held, 1) {
            return true;
        }
        if let Some(duct) = crate::construction::spawn_obj(world, DUCT, pos) {
            world
                .entity_mut(duct)
                .insert((Duct { layer }, PlumbingLayer(layer_index(layer))));
        }
        sound(world, target, CLICK, 50.0);
        return true;
    }
    let Some((tool, _)) = tool_of(world, held) else {
        // `grinder_chemical/item_interaction`: a non-tool item in hand is ground at once.
        if matches!(kind(world, target), Some(Kind::Grinder)) {
            let combat = world
                .get::<crate::combat::CombatMode>(mob)
                .is_some_and(|c| c.0);
            if combat {
                return false;
            }
            if !anchored(world, target) {
                tell(world, client, Span::Warning, "Сначала прикрутите его.");
                return true;
            }
            let name = crate::chem::name_of(world, held);
            crate::storage::put_inside(world, held, target);
            blend(world, target, held);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы перемалываете {name}."),
            );
            return true;
        }
        return false;
    };
    // `duct/wrench_act`: `deconstruct` into a stack of one.
    if tool == ToolKind::Wrench
        && let Some(duct) = world.get::<Duct>(target).copied()
    {
        let Some(&pos) = world.get::<TilePos>(target) else {
            return true;
        };
        sound(world, target, RATCHET, 50.0);
        world.despawn(target);
        if let Some(stack) = spawn_stack(world, DUCT_STACK, pos, 1) {
            world
                .entity_mut(stack)
                .insert(DuctStack { layer: duct.layer });
        }
        tell(world, client, Span::Notice, "Вы откручиваете дукт.");
        return true;
    }
    if world.get::<Plumber>(target).is_none() {
        return false;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    match tool {
        // `plumbing/wrench_act` -> `default_unfasten_wrench`.
        ToolKind::Wrench if !combat => {
            let was = anchored(world, target);
            if !was
                && let Some(&pos) = world.get::<TilePos>(target)
                && world.get::<Plumber>(target).is_some_and(|p| {
                    p.ports
                        .iter()
                        .any(|p| layer_taken(world, pos, p.layer, Some(target)))
                })
            {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Здесь уже есть дукт или машина!",
                );
                return true;
            }
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            sound(world, target, RATCHET, 50.0);
            do_after(
                world,
                actor,
                target,
                UNFASTEN_DS,
                0,
                Job::Unfasten { anchored: was },
            );
            reset_on_unfasten(world, target);
            true
        }
        // `buffer/screwdriver_act`: links every buffer reached through adjacent buffers.
        ToolKind::Screwdriver if matches!(kind(world, target), Some(Kind::Buffer { .. })) => {
            let group = buffer_group(world, target);
            for &b in &group {
                if let Some(mut p) = world.get_mut::<Plumber>(b)
                    && let Kind::Buffer { links, .. } = &mut p.kind
                {
                    *links = group.clone();
                }
            }
            tell(
                world,
                client,
                Span::Notice,
                format!("Связано буферов: {}.", group.len()),
            );
            true
        }
        // `receiver/multitool_act`: into the multitool's buffer.
        ToolKind::Multitool if matches!(kind(world, target), Some(Kind::Receiver { .. })) => {
            crate::tools::set_multitool_buffer(world, held, target);
            tell(world, client, Span::Notice, "Сохранено в буфер мультитула.");
            true
        }
        // `sender/multitool_act`: linked to the buffered receiver.
        ToolKind::Multitool if matches!(kind(world, target), Some(Kind::Sender { .. })) => {
            let receiver = crate::tools::multitool_buffer(world, held)
                .filter(|&r| matches!(kind(world, r), Some(Kind::Receiver { .. })));
            let Some(receiver) = receiver else {
                tell(world, client, Span::Warning, "Неверный буфер.");
                return true;
            };
            link_sender(world, target, receiver);
            tell(world, client, Span::Notice, "Маяк связан с приёмником.");
            true
        }
        _ => false,
    }
}

/// `sender/set_teleport_target` (after `lose_teleport_target`).
fn link_sender(world: &mut World, sender: Entity, receiver: Entity) {
    if let Some(Kind::Sender {
        target: Some(old), ..
    }) = kind(world, sender).cloned()
        && let Some(mut p) = world.get_mut::<Plumber>(old)
        && let Kind::Receiver { senders, .. } = &mut p.kind
    {
        senders.retain(|&s| s != sender);
    }
    set_kind(
        world,
        sender,
        Kind::Sender {
            target: Some(receiver),
        },
    );
    if let Some(mut p) = world.get_mut::<Plumber>(receiver)
        && let Kind::Receiver { senders, .. } = &mut p.kind
        && !senders.contains(&sender)
    {
        senders.push(sender);
    }
}

/// `acclimator/wrench_act`: back to filling.
fn reset_on_unfasten(world: &mut World, target: Entity) {
    if let Some(mut p) = world.get_mut::<Plumber>(target)
        && let Kind::Acclimator { state, .. } = &mut p.kind
    {
        *state = Acclimate::Filling;
    }
}

fn buffer_group(world: &World, start: Entity) -> Vec<Entity> {
    let mut group = vec![start];
    let mut i = 0;
    while i < group.len() {
        let Some(&pos) = world.get::<TilePos>(group[i]) else {
            i += 1;
            continue;
        };
        for d in Dir::CARDINALS {
            for &e in world.resource::<SpatialIndex>().on_tile(pos.step(d)) {
                if matches!(kind(world, e), Some(Kind::Buffer { .. })) && !group.contains(&e) {
                    group.push(e);
                }
            }
        }
        i += 1;
    }
    group
}

/// `simple_rotation` of an unanchored plumbing machine (alt-click, counterclockwise). True when
/// `target` is one.
pub fn rotate_machine(world: &mut World, target: Entity) -> bool {
    if world.get::<Plumber>(target).is_none() || anchored(world, target) {
        return false;
    }
    let dir = dir_of(world, target);
    let ccw = CW[(cw_index(dir) + 3) % 4];
    if let Some(facing) = Dir::from_bits(ccw) {
        world.entity_mut(target).insert(Facing(facing));
    }
    true
}

fn reagent_name(world: &World, id: ReagentId) -> String {
    world.resource::<Chem>().0.def(id).name.clone()
}

fn ask(world: &mut World, client: Entity, mob: Entity, machine: Entity, title: &str, step: Step) {
    let kind = match &step {
        Step::Menu(choices) => PromptKind::List {
            items: choices
                .iter()
                .map(|c| choice_label(world, machine, *c))
                .collect(),
            default: None,
        },
        Step::Reagent { .. } => PromptKind::List {
            items: world
                .resource::<Chem>()
                .0
                .reagents
                .iter()
                .map(|r| r.name.clone())
                .collect(),
            default: None,
        },
        Step::Number(setting) => {
            let (default, min, max) = setting_range(world, machine, *setting);
            PromptKind::Number {
                default,
                min,
                max,
                round: matches!(
                    setting,
                    Setting::SynthAmount | Setting::PillDuration | Setting::BottlerAmount
                ),
            }
        }
        Step::Synth(ids) => PromptKind::List {
            items: ids.iter().map(|&id| reagent_name(world, id)).collect(),
            default: None,
        },
        Step::Name => PromptKind::Text {
            default: match kind(world, machine) {
                Some(Kind::PillPress { product, .. }) => product.clone(),
                _ => String::new(),
            },
            max_length: MAX_NAME_LEN,
            multiline: false,
        },
    };
    let prompt = ask_world(world, Ask::new(client, mob, title, "", kind));
    world
        .entity_mut(prompt)
        .insert(PlumbPrompt { machine, step });
}

fn choice_label(world: &World, machine: Entity, choice: Choice) -> String {
    let k = kind(world, machine);
    match choice {
        Choice::Number(s) => match (s, k) {
            (Setting::AcclimatorTarget, Some(Kind::Acclimator { target, .. })) => {
                format!("Целевая температура: {target} K")
            }
            (Setting::AcclimatorVolume, Some(Kind::Acclimator { max_volume, .. })) => {
                format!("Объём: {max_volume} u")
            }
            (
                Setting::Splitter(d),
                Some(Kind::Splitter {
                    straight,
                    left,
                    right,
                }),
            ) => {
                let (name, v) = match d {
                    S => ("Прямо", straight),
                    E => ("Влево", left),
                    _ => ("Вправо", right),
                };
                format!("{name}: {v} u")
            }
            (Setting::ChamberTarget, Some(Kind::ReactionChamber { target, .. })) => {
                format!("Целевая температура: {target} K")
            }
            (Setting::DisposerRate, Some(Kind::Disposer { rate, .. })) => {
                format!("Скорость: {rate} u/с")
            }
            (Setting::AcclimatorVolume, _) => "Объём".to_owned(),
            (Setting::AcidicLimit, Some(Kind::ReactionChamber { ph: Some(ph), .. })) => {
                format!("Кислотный предел pH: {}", ph.acidic_limit)
            }
            (Setting::AlkalineLimit, Some(Kind::ReactionChamber { ph: Some(ph), .. })) => {
                format!("Щелочной предел pH: {}", ph.alkaline_limit)
            }
            (Setting::SynthAmount, Some(Kind::Synthesizer { amount, .. })) => {
                format!("Количество: {amount} u")
            }
            (Setting::PressVolume, Some(Kind::PillPress { volume, .. })) => {
                format!("Объём продукта: {volume} u")
            }
            (Setting::PillDuration, Some(Kind::PillPress { duration, .. })) => {
                format!("Слои сахара таблетки: {duration}")
            }
            _ => "?".to_owned(),
        },
        Choice::FilterAdd(true) => "Добавить влево".to_owned(),
        Choice::FilterAdd(false) => "Добавить вправо".to_owned(),
        Choice::FilterRemove(left, id) => format!(
            "Убрать {} ({})",
            reagent_name(world, id),
            if left { "влево" } else { "вправо" }
        ),
        Choice::ChamberAdd => "Добавить реагент".to_owned(),
        Choice::ChamberRemove(id) => format!("Убрать {}", reagent_name(world, id)),
        Choice::DisposerToggle => match k {
            Some(Kind::Disposer { on: true, .. }) => "Выключить".to_owned(),
            _ => "Включить".to_owned(),
        },
        Choice::ChamberCatalyst(id) => {
            let catalyst = matches!(k, Some(Kind::ReactionChamber { catalysts, .. })
                if catalysts.iter().any(|&(c, _)| c == id));
            if catalyst {
                format!("{}: катализатор → реагент", reagent_name(world, id))
            } else {
                format!("{}: реагент → катализатор", reagent_name(world, id))
            }
        }
        Choice::SynthReagent => match k {
            Some(Kind::Synthesizer {
                reagent: Some(id), ..
            }) => format!("Реагент: {}", reagent_name(world, *id)),
            _ => "Реагент: нет".to_owned(),
        },
        Choice::PressPackage(p) => {
            let current = matches!(k, Some(Kind::PillPress { package, .. }) if *package == p);
            format!("{}{}", p.suffix(), if current { " (выбрано)" } else { "" })
        }
        Choice::PressName => match k {
            Some(Kind::PillPress { product, .. }) => format!("Название: {product}"),
            _ => "Название".to_owned(),
        },
    }
}

fn setting_range(world: &World, machine: Entity, setting: Setting) -> (f64, f64, f64) {
    let max_temp = f64::from(ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE);
    match (setting, kind(world, machine)) {
        (Setting::AcclimatorTarget, Some(Kind::Acclimator { target, .. })) => {
            (f64::from(*target), 0.0, max_temp)
        }
        (Setting::AcclimatorVolume, Some(Kind::Acclimator { max_volume, .. })) => {
            let buffer = holder(world, machine).map_or(200.0, |h| h.maximum_volume);
            (f64::from(*max_volume), 1.0, f64::from(buffer))
        }
        (Setting::Splitter(_), _) => (5.0, 1.0, f64::from(SPLITTER_MAX)),
        (Setting::BufferActivation, Some(Kind::Buffer { activation, .. })) => {
            let buffer = holder(world, machine).map_or(200.0, |h| h.maximum_volume);
            (f64::from(*activation), 1.0, f64::from(buffer))
        }
        (Setting::ChamberTarget, Some(Kind::ReactionChamber { target, .. })) => {
            (f64::from(*target), 0.0, 1000.0)
        }
        (Setting::ChamberAmount, _) => (10.0, 0.01, 200.0),
        (Setting::DisposerRate, Some(Kind::Disposer { rate, .. })) => {
            (f64::from(*rate), 0.1, f64::from(DISPOSAL_MAX))
        }
        // `acidic`: `CHEMICAL_MIN_PH..alkaline_limit - 1`; `alkaline`: `acidic_limit + 1..14`.
        (Setting::AcidicLimit, Some(Kind::ReactionChamber { ph: Some(ph), .. })) => (
            f64::from(ph.acidic_limit),
            0.0,
            f64::from(ph.alkaline_limit - 1.0),
        ),
        (Setting::AlkalineLimit, Some(Kind::ReactionChamber { ph: Some(ph), .. })) => (
            f64::from(ph.alkaline_limit),
            f64::from(ph.acidic_limit + 1.0),
            14.0,
        ),
        (Setting::SynthAmount, Some(Kind::Synthesizer { amount, .. })) => {
            (f64::from(*amount), 0.0, f64::from(SYNTH_MAX))
        }
        (
            Setting::PressVolume,
            Some(Kind::PillPress {
                volume, package, ..
            }),
        ) => (
            f64::from(*volume),
            f64::from(PRESS_MIN_VOLUME),
            f64::from(package.container().1),
        ),
        (Setting::PillDuration, Some(Kind::PillPress { duration, .. })) => (
            f64::from(*duration),
            0.0,
            f64::from(crate::chem_master::MAX_PILL_LAYERS),
        ),
        // `bottler/interact`: `tgui_input_number(..., max_value = 100, min_value = 1)`.
        (Setting::BottlerAmount, Some(Kind::Bottler { wanted })) => {
            (f64::from(*wanted), 1.0, 100.0)
        }
        _ => (0.0, 0.0, 0.0),
    }
}

/// An empty hand on a configurable machine: its settings menu (tg `ui_interact`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(k) = kind(world, target).cloned() else {
        return false;
    };
    let choices = match k {
        Kind::Acclimator { .. } => vec![
            Choice::Number(Setting::AcclimatorTarget),
            Choice::Number(Setting::AcclimatorVolume),
        ],
        Kind::Filter { left, right } => {
            let mut c = vec![Choice::FilterAdd(true), Choice::FilterAdd(false)];
            c.extend(left.iter().map(|&id| Choice::FilterRemove(true, id)));
            c.extend(right.iter().map(|&id| Choice::FilterRemove(false, id)));
            c
        }
        Kind::Splitter { .. } => vec![
            Choice::Number(Setting::Splitter(S)),
            Choice::Number(Setting::Splitter(E)),
            Choice::Number(Setting::Splitter(W)),
        ],
        Kind::Buffer { .. } => {
            // `buffer/attack_hand`: `tgui_input_number` of the activation volume.
            ask(
                world,
                client,
                mob,
                target,
                "Порог активации",
                Step::Number(Setting::BufferActivation),
            );
            return true;
        }
        Kind::ReactionChamber {
            required,
            catalysts,
            ph,
            ..
        } => {
            let mut c = vec![Choice::ChamberAdd, Choice::Number(Setting::ChamberTarget)];
            if ph.is_some() {
                c.push(Choice::Number(Setting::AcidicLimit));
                c.push(Choice::Number(Setting::AlkalineLimit));
            }
            for &(id, _) in required.iter().chain(&catalysts) {
                c.push(Choice::ChamberCatalyst(id));
                c.push(Choice::ChamberRemove(id));
            }
            c
        }
        Kind::Synthesizer { .. } => {
            vec![Choice::SynthReagent, Choice::Number(Setting::SynthAmount)]
        }
        Kind::PillPress { .. } => vec![
            Choice::PressPackage(Package::Pill),
            Choice::PressPackage(Package::Patch),
            Choice::PressPackage(Package::Bottle),
            Choice::Number(Setting::PressVolume),
            Choice::Number(Setting::PillDuration),
            Choice::PressName,
        ],
        Kind::Bottler { .. } => {
            ask(
                world,
                client,
                mob,
                target,
                "Объём заполнения",
                Step::Number(Setting::BottlerAmount),
            );
            return true;
        }
        Kind::Disposer { .. } => vec![
            Choice::DisposerToggle,
            Choice::Number(Setting::DisposerRate),
        ],
        _ => return false,
    };
    let title = crate::chem::name_of(world, target);
    ask(world, client, mob, target, &title, Step::Menu(choices));
    true
}

/// `ui_status`: the user still next to the machine, conscious.
fn can_use(world: &World, mob: Entity, machine: Entity) -> bool {
    let near = match (world.get::<TilePos>(mob), world.get::<TilePos>(machine)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    near && world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Conscious)
}

fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(PlumbPrompt { machine, step }) = world.get::<PlumbPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(answer) = answer else {
            continue;
        };
        if world.get::<Plumber>(machine).is_none() || !can_use(world, mob, machine) {
            continue;
        }
        apply_answer(world, client, mob, machine, step, answer);
    }
}

fn apply_answer(
    world: &mut World,
    client: Entity,
    mob: Entity,
    machine: Entity,
    step: Step,
    answer: PromptAnswer,
) {
    let Some(mut plumber) = world.get::<Plumber>(machine).cloned() else {
        return;
    };
    match (step, answer) {
        (Step::Menu(choices), PromptAnswer::Item(i)) => {
            let Some(&choice) = choices.get(i) else {
                return;
            };
            match choice {
                Choice::Number(s) => ask(world, client, mob, machine, "Значение", Step::Number(s)),
                Choice::FilterAdd(left) => ask(
                    world,
                    client,
                    mob,
                    machine,
                    "Реагент",
                    Step::Reagent {
                        left: Some(left),
                        amount: 0.0,
                    },
                ),
                Choice::ChamberAdd => ask(
                    world,
                    client,
                    mob,
                    machine,
                    "Количество",
                    Step::Number(Setting::ChamberAmount),
                ),
                Choice::FilterRemove(side, id) => {
                    if let Kind::Filter { left, right } = &mut plumber.kind {
                        let list = if side { left } else { right };
                        list.retain(|&r| r != id);
                    }
                }
                Choice::ChamberRemove(id) => {
                    if let Kind::ReactionChamber { required, .. } = &mut plumber.kind {
                        required.retain(|&(r, _)| r != id);
                    }
                }
                Choice::DisposerToggle => {
                    if let Kind::Disposer { on, .. } = &mut plumber.kind {
                        *on = !*on;
                    }
                }
                Choice::ChamberCatalyst(id) => {
                    if let Kind::ReactionChamber {
                        required,
                        catalysts,
                        ..
                    } = &mut plumber.kind
                    {
                        if let Some(i) = required.iter().position(|&(r, _)| r == id) {
                            catalysts.push(required.remove(i));
                        } else if let Some(i) = catalysts.iter().position(|&(r, _)| r == id) {
                            required.push(catalysts.remove(i));
                        }
                    }
                }
                Choice::SynthReagent => {
                    if let Kind::Synthesizer { list, .. } = &plumber.kind {
                        let chem = world.resource::<Chem>().0.clone();
                        let ids = list.iter().filter_map(|p| chem.id(p)).collect();
                        ask(world, client, mob, machine, "Реагент", Step::Synth(ids));
                    }
                    return;
                }
                Choice::PressPackage(p) => {
                    // `change_product`: the volume clamps to the new container's.
                    if let Kind::PillPress {
                        package, volume, ..
                    } = &mut plumber.kind
                    {
                        *package = p;
                        *volume = volume.clamp(PRESS_MIN_VOLUME, p.container().1);
                    }
                }
                Choice::PressName => {
                    ask(world, client, mob, machine, "Название", Step::Name);
                    return;
                }
            }
        }
        (Step::Number(Setting::ChamberAmount), PromptAnswer::Number(n)) => {
            let amount = round_to(n as f32, CHEMICAL_VOLUME_ROUNDING);
            if amount.is_finite() && amount > 0.0 {
                ask(
                    world,
                    client,
                    mob,
                    machine,
                    "Реагент",
                    Step::Reagent { left: None, amount },
                );
            }
            return;
        }
        (Step::Number(setting), PromptAnswer::Number(n)) => {
            let (_, min, max) = setting_range(world, machine, setting);
            if !n.is_finite() {
                return;
            }
            let v = n.clamp(min, max) as f32;
            match (&mut plumber.kind, setting) {
                (Kind::Acclimator { target, .. }, Setting::AcclimatorTarget) => *target = v.round(),
                (Kind::Acclimator { max_volume, .. }, Setting::AcclimatorVolume) => {
                    *max_volume = v.round();
                }
                (
                    Kind::Splitter {
                        straight,
                        left,
                        right,
                    },
                    Setting::Splitter(d),
                ) => {
                    let v = v.round().clamp(1.0, SPLITTER_MAX);
                    match d {
                        S => *straight = v,
                        E => *left = v,
                        _ => *right = v,
                    }
                }
                (Kind::Buffer { activation, .. }, Setting::BufferActivation) => {
                    *activation = v.round();
                }
                (Kind::ReactionChamber { target, .. }, Setting::ChamberTarget) => {
                    *target = v.round();
                }
                (Kind::Disposer { rate, .. }, Setting::DisposerRate) => {
                    *rate = round_to(v, CHEMICAL_VOLUME_ROUNDING);
                }
                (Kind::ReactionChamber { ph: Some(ph), .. }, Setting::AcidicLimit) => {
                    ph.acidic_limit = v.round();
                }
                (Kind::ReactionChamber { ph: Some(ph), .. }, Setting::AlkalineLimit) => {
                    ph.alkaline_limit = v.round();
                }
                // `possible_amounts`: 0..5.
                (Kind::Synthesizer { amount, .. }, Setting::SynthAmount) => *amount = v.round(),
                (Kind::PillPress { volume, .. }, Setting::PressVolume) => *volume = v,
                (Kind::PillPress { duration, .. }, Setting::PillDuration) => {
                    *duration = v.round();
                }
                (Kind::Bottler { wanted }, Setting::BottlerAmount) => *wanted = v.round(),
                _ => return,
            }
        }
        (Step::Reagent { left, amount }, PromptAnswer::Item(i)) => {
            let chem = world.resource::<Chem>().0.clone();
            let Some(def) = chem.reagents.get(i) else {
                return;
            };
            let Some(id) = chem.id(&def.path) else {
                return;
            };
            match (&mut plumber.kind, left) {
                (Kind::Filter { left, right }, Some(side)) => {
                    let list = if side { left } else { right };
                    if !list.contains(&id) {
                        list.push(id);
                    }
                }
                (Kind::ReactionChamber { required, .. }, None) => {
                    if !required.iter().any(|&(r, _)| r == id) {
                        required.push((id, amount));
                    }
                }
                _ => return,
            }
        }
        // `ui_act("select")`: only a reagent of the machine's list.
        (Step::Synth(ids), PromptAnswer::Item(i)) => {
            let Some(&id) = ids.get(i) else {
                return;
            };
            if let Kind::Synthesizer { list, reagent, .. } = &mut plumber.kind {
                let chem = world.resource::<Chem>().0.clone();
                if !list.iter().any(|p| chem.id(p) == Some(id)) {
                    return;
                }
                *reagent = Some(id);
            }
            // The old reagent is flushed (`reagents.clear_reagents()`).
            if let Some(mut r) = world.get_mut::<Reagents>(machine) {
                r.0.clear();
            }
        }
        (Step::Name, PromptAnswer::Text(name)) => {
            let name: String = name.trim().chars().take(MAX_NAME_LEN as usize).collect();
            if name.is_empty() {
                return;
            }
            if let Kind::PillPress { product, .. } = &mut plumber.kind {
                *product = name;
            }
        }
        _ => return,
    }
    world.entity_mut(machine).insert(plumber);
}

/// Examine lines of a duct, duct stack or plumbing machine (`duct/examine`,
/// `stack/ducts/examine`, `plumbing/examine`, the machines' own `examine`).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let mut note = |text: String| lines.push((Span::Notice, text));
    if let Some(d) = world.get::<Duct>(target) {
        note(format!("Слой дукта: {}.", layer_index(d.layer)));
        note("Его можно открутить гаечным ключом.".to_owned());
        return;
    }
    if let Some(d) = world.get::<DuctStack>(target) {
        note(format!(
            "Слой: {}. Используйте в руке, чтобы сменить.",
            layer_index(d.layer)
        ));
        note("Кликните по полу, чтобы проложить дукт.".to_owned());
        return;
    }
    let Some(p) = world.get::<Plumber>(target) else {
        return;
    };
    if let Some(h) = holder(world, target) {
        note(format!(
            "Дисплей максимального объёма: {} u. Содержит:",
            crate::chem::dm_num(h.maximum_volume)
        ));
        if h.reagents.is_empty() {
            note("Ничего.".to_owned());
        }
        for r in &h.reagents {
            note(format!(
                "{} u {}",
                crate::chem::dm_num(round_to(r.volume, CHEMICAL_VOLUME_ROUNDING)),
                reagent_name(world, r.id)
            ));
        }
    }
    if anchored(world, target) {
        note("Прикручено на месте.".to_owned());
    } else {
        lines.push((
            Span::Warning,
            "Нужно прикрутить гаечным ключом, чтобы заработало.".to_owned(),
        ));
    }
    let mut note = |text: String| lines.push((Span::Notice, text));
    if let Some(cap) = p.cap {
        note(format!("Принимает не больше {cap} разных реагентов."));
    }
    match &p.kind {
        Kind::Buffer {
            activation, mode, ..
        } => {
            note(format!("Срабатывает при {activation} u."));
            note(
                match mode {
                    BufferMode::Unready => "Набирает реагенты.",
                    BufferMode::Idle => "Ждёт другие буферы.",
                    BufferMode::Ready => "Выпускает реагенты.",
                }
                .to_owned(),
            );
        }
        Kind::Disposer { rate, .. } => note(format!("Уничтожает {rate} u реагентов в секунду.")),
        Kind::Bottler { wanted } => note(format!("Наполняет по {wanted} u.")),
        Kind::ReactionChamber {
            ph: Some(ph),
            target: temp,
            ..
        } => note(format!(
            "Держит {temp} K и pH {}–{}.",
            ph.acidic_limit, ph.alkaline_limit
        )),
        Kind::Sender { target: None } => note("Не связан с приёмником.".to_owned()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

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
}
