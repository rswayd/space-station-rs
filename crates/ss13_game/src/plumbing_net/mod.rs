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
use crate::obj_damage::anchored;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, Stack, ToolKind, do_after, tell, tool_of, use_stack};

mod interact;
mod kinds;
mod machines;
mod net;
mod prompt;
#[cfg(test)]
mod tests;

use interact::*;
pub use interact::{attack_self, examine_lines, interact, item_interaction, rotate_machine};
use kinds::*;
pub use machines::tick;
use machines::*;
use net::*;
use prompt::*;

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
