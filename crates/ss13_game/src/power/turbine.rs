//! The gas turbine (server): tg `code/modules/power/turbine/turbine.dm`, `turbine_parts.dm`,
//! `turbine_computer.dm`.
//!
//! - Three machines in a row: the inlet compressor behind the core rotor, the turbine outlet in
//!   front of it (along the rotor's `dir`); compressor and outlet face the same way. Each holds a
//!   part ([`TurbineSection::tier`], tg `installed_part`) and a gas mixture of 1000, 3000 and
//!   6000 L.
//! - The rotor links the three ([`activate_parts`]: on map load, a closed panel, a multitool on
//!   the rotor), averages their parts' rated rpm and temperature and joins the powernet of its
//!   tile. An open panel on any of them unlinks the whole turbine.
//! - The control console ([`TurbineComputer`], window `TurbineComputer`) links to a rotor by
//!   `mapping_id` at roundstart or by a multitool's buffer; it switches the turbine on and off
//!   (off only under 1000 rpm) and sets the compressor's intake.
//! - Every machine tick a running turbine ([`process_rotor`]) pumps the air behind the compressor
//!   in (intake × its pressure), on through the rotor and the outlet and out in front, turns the
//!   work into rpm (capped by the parts) and puts `rpm × 0.3 × 15 × 2 s` joules into the
//!   powernet. Air hotter than the parts' rating damages it; past 500 damage it explodes by its
//!   rpm; damage is announced on the engineering channel.
//! - Parts: a crowbar (right click) takes one out of an open machine, a part goes in over 2 s;
//!   sheets upgrade a part one tier (`get_tier_upgrades`).
//!
//! Cost: O(turbines) per machine tick and per frame (looks); stations have one.
// shortcut: no deconstruction (crowbar) or construction from boards, no cable layer change, no
// active power use of a running rotor, no guide paper beside the rotor, no screentips; the
// console links at once (tg 0.2 s) and is not reachable by silicons.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_atmos::GasMixture;
use ss13_atmos::gas::{CELL_VOLUME, R_IDEAL_GAS_EQUATION, TCMB, byond_round, quantize};
use ss13_atmos::pipenet::air::{PipeAir, TurfMix, pump_moles};
use ss13_chat::Span;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmName};
use ss13_shared::appearance::Overlay;
use ss13_shared::{Appearance, Dir, EMISSIVE_PLANE, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::turbine_computer::{TurbineComputerData, TurbineComputerUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::{DmPower, Grid, MACHINE_TICK_SECS, MachineTicks, NodeKind, PowerJob, Unpowered};
use crate::construction::Job;
use crate::obj_damage::{Integrity, anchored};
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of};

pub const TURBINE_ROOT: &str = "/obj/machinery/power/turbine";
pub const COMPRESSOR: &str = "/obj/machinery/power/turbine/inlet_compressor";
pub const ROTOR: &str = "/obj/machinery/power/turbine/core_rotor";
pub const OUTLET: &str = "/obj/machinery/power/turbine/turbine_outlet";
pub const COMPUTER: &str = "/obj/machinery/computer/turbine_computer";
pub const PART_ROOT: &str = "/obj/item/turbine_parts";
pub const COMPRESSOR_PART: &str = "/obj/item/turbine_parts/compressor";
pub const ROTOR_PART: &str = "/obj/item/turbine_parts/rotor";
pub const STATOR_PART: &str = "/obj/item/turbine_parts/stator";

/// `TURBINE_MAX_BASE_RPM`.
pub const MAX_BASE_RPM: f64 = 50_000.0;
/// `TURBINE_RPM_CONVERSION`.
const RPM_CONVERSION: f64 = 15.0;
/// `TURBINE_ENERGY_RECTIFICATION_MULTIPLIER`.
const ENERGY_RECTIFICATION: f64 = 0.3;
/// `TURBINE_MAX_TAKEN_DAMAGE`, `TURBINE_DAMAGE_HEALING`, `TURBINE_DAMAGE_ALARM_START`.
const MAX_TAKEN_DAMAGE: f64 = 10.0;
const DAMAGE_HEALING: f64 = 2.0;
const DAMAGE_ALARM_START: f64 = 15.0;
/// `TURBINE_WORK_CONVERSION_MULTIPLIER`, `TURBINE_HEAT_CONVERSION_MULTIPLIER`.
const WORK_CONVERSION: f64 = 0.01;
const HEAT_CONVERSION: f64 = 0.005;
/// `TURBINE_COMPRESSOR_STATOR_INTERACTION_MULTIPLIER`.
const COMPRESSOR_STATOR_INTERACTION: f64 = 0.15;
/// `MINIMUM_TURBINE_PRESSURE`.
const MINIMUM_PRESSURE: f64 = 0.01;
/// The highest part tier (`TURBINE_PART_TIER_FOUR`).
pub const MAX_TIER: u8 = 4;
/// Turning the turbine off needs it under this rpm.
const SHUTDOWN_RPM: f64 = 1000.0;
const ALERT_SOUND: &str = "sound/machines/engine_alert/engine_alert1.ogg";

/// `PRESSURE_MAX`.
fn pressure_max(value: f64) -> f64 {
    value.max(MINIMUM_PRESSURE)
}

/// Which of the three machines (and which part fits it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Compressor,
    Rotor,
    Outlet,
}

impl Section {
    pub fn of_machine(path: &str) -> Option<Self> {
        [
            (COMPRESSOR, Self::Compressor),
            (ROTOR, Self::Rotor),
            (OUTLET, Self::Outlet),
        ]
        .into_iter()
        .find(|(root, _)| is_under(path, root))
        .map(|(_, s)| s)
    }

    pub fn of_part(path: &str) -> Option<Self> {
        [
            (COMPRESSOR_PART, Self::Compressor),
            (ROTOR_PART, Self::Rotor),
            (STATOR_PART, Self::Outlet),
        ]
        .into_iter()
        .find(|(root, _)| is_under(path, root))
        .map(|(_, s)| s)
    }

    /// `gas_theoretical_volume`.
    pub fn volume(self) -> f64 {
        match self {
            Self::Compressor => 1000.0,
            Self::Rotor => 3000.0,
            Self::Outlet => 6000.0,
        }
    }

    pub fn part_path(self) -> &'static str {
        match self {
            Self::Compressor => COMPRESSOR_PART,
            Self::Rotor => ROTOR_PART,
            Self::Outlet => STATOR_PART,
        }
    }

    fn part_name(self) -> &'static str {
        match self {
            Self::Compressor => "деталь компрессора",
            Self::Rotor => "деталь ротора",
            Self::Outlet => "деталь статора",
        }
    }

    /// `base_icon_state`.
    fn base_icon_state(self) -> &'static str {
        match self {
            Self::Compressor => "inlet",
            Self::Rotor => "core",
            Self::Outlet => "outlet",
        }
    }
}

/// `get_tier_value(TURBINE_MAX_RPM)`.
pub fn tier_rpm(tier: u8) -> f64 {
    MAX_BASE_RPM * 2.5_f64.powi(i32::from(tier.max(1)) - 1)
}

/// `get_tier_value(TURBINE_MAX_TEMP)`: 50000 K, raised to the 1.2th power per tier.
pub fn tier_temperature(tier: u8) -> f64 {
    (1..tier.max(1)).fold(50_000.0, |t, _| f64::powf(t, 1.2))
}

/// `get_tier_value(TURBINE_MAX_EFFICIENCY)`; the stator's own.
pub fn tier_efficiency(section: Section, tier: u8) -> f64 {
    let steps = f64::from(tier.max(1) - 1);
    match section {
        Section::Outlet => 0.85 + 0.015 * steps,
        _ => 0.25 + 0.2 * steps,
    }
}

/// `get_tier_upgrades`: the sheets and their amount for the next tier.
pub fn tier_upgrade(section: Section, tier: u8) -> Option<(&'static str, u32, &'static str)> {
    const PLASTEEL: (&str, &str) = ("/obj/item/stack/sheet/plasteel", "пластали");
    const TITANIUM: (&str, &str) = ("/obj/item/stack/sheet/mineral/titanium", "титана");
    const HYDROGEN: (&str, &str) = (
        "/obj/item/stack/sheet/mineral/metal_hydrogen",
        "металлического водорода",
    );
    const ZAUKERITE: (&str, &str) = ("/obj/item/stack/sheet/mineral/zaukerite", "заукерита");
    let ((path, name), amount) = match (section, tier) {
        (Section::Outlet, 1) => (TITANIUM, 15),
        (Section::Outlet, 2) => (HYDROGEN, 15),
        (Section::Outlet, 3) => (ZAUKERITE, 10),
        (_, 1) => (PLASTEEL, 10),
        (_, 2) => (TITANIUM, 10),
        (_, 3) => (HYDROGEN, 5),
        _ => return None,
    };
    Some((path, amount, name))
}

/// One of the three machines.
#[derive(Component, Clone, Debug)]
pub struct TurbineSection {
    pub section: Section,
    /// The installed part's `current_tier`; `None`: no part.
    pub tier: Option<u8>,
    pub panel_open: bool,
    /// `machine_stat & MAINT` (set by `deactivate_parts`).
    pub maint: bool,
    /// `machine_gasmix`.
    pub gas: PipeAir,
    /// Compressor and outlet: the rotor that linked them.
    pub rotor: Option<Entity>,
    /// Compressor: `intake_regulator`, `compressor_work`, `compressor_pressure`.
    pub intake_regulator: f64,
    pub compressor_work: f64,
    pub compressor_pressure: f64,
}

impl TurbineSection {
    pub fn new(section: Section, tier: Option<u8>) -> Self {
        Self {
            section,
            tier,
            panel_open: false,
            maint: false,
            gas: PipeAir::new(section.volume()),
            rotor: None,
            intake_regulator: 0.5,
            compressor_work: 0.0,
            compressor_pressure: MINIMUM_PRESSURE,
        }
    }

    /// `efficiency`, cached from the part.
    pub fn efficiency(&self) -> f64 {
        self.tier.map_or(0.0, |t| tier_efficiency(self.section, t))
    }
}

/// `/obj/machinery/power/turbine/core_rotor`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct TurbineRotor {
    pub mapping_id: String,
    pub active: bool,
    pub compressor: Option<Entity>,
    pub turbine: Option<Entity>,
    pub rpm: f64,
    /// Joules of the last machine tick.
    pub produced_energy: f64,
    pub all_parts_connected: bool,
    pub max_allowed_rpm: f64,
    pub max_allowed_temperature: f64,
    pub damage: f64,
    pub damage_archived: f64,
    /// Game time the next damage alert may play (`turbine_damage_alert`).
    pub alert_at: f64,
    pub layer: u8,
}

impl TurbineRotor {
    /// `get_turbine_integrity`, percent.
    pub fn integrity(&self) -> f64 {
        byond_round(100.0 - self.damage / 500.0 * 100.0, 0.01).max(0.0)
    }
}

/// `/obj/machinery/computer/turbine_computer`: `turbine_core`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct TurbineComputer {
    pub core: Option<Entity>,
}

/// `/obj/item/turbine_parts`: `current_tier`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurbinePartItem {
    pub tier: u8,
}

/// Timed steps on turbine parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurbineJob {
    /// The held part into the machine (`item_interaction`).
    Install { tier_before: Option<u8> },
    /// The held sheets onto the part (`turbine_parts/item_interaction`).
    Upgrade { tier: u8, amount: u32 },
}

pub(super) struct TurbinePlugin;

impl Plugin for TurbinePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.register_ui::<TurbineWindow>()
            .add_observer(add_part_tier)
            .add_systems(ss13_world::MapInitLate, init_turbines)
            .add_systems(
                Update,
                (
                    process_turbines.after(super::PowerSystems),
                    turbine_looks.after(super::PowerSystems),
                    window_acts.in_set(UiSystems::Handle),
                ),
            );
        // The console's window.
        app.add_click_handler(ClickPhase::EmptyHand, 2021, |w, c| {
            interact(w, c.client, c.mob, c.target)
        })
        // Tools and parts on the machines, sheets on a part, a multitool on the console.
        .add_click_handler(ClickPhase::HeldOnTarget, 271, |w, c| {
            let Some(held) = c.held else { return false };
            item_interaction(w, c.client, c.mob, held, c.target, c.modifiers.right)
        })
        // Sheets on a part in the other hand.
        .add_click_handler(ClickPhase::OtherHand, 1511, |w, c| {
            let Some(held) = c.held else { return false };
            part_upgrade(w, c.client, c.mob, held, c.target)
        });
    }
}

/// New parts are tier 1 (`current_tier`); a part taken out of a machine is given its own.
fn add_part_tier(
    add: On<Add<ss13_mapload::DmItem>>,
    items: Query<&ss13_mapload::DmItem>,
    mut commands: Commands,
) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, PART_ROOT))
    {
        commands
            .entity(add.entity)
            .insert_if_new(TurbinePartItem { tier: 1 });
    }
}

fn dir_of(world: &World, e: Entity) -> u8 {
    world
        .get::<ss13_shared::Facing>(e)
        .map(|f| f.0 as u8)
        .or_else(|| world.get::<DmAtom>(e).map(|a| a.dir))
        .unwrap_or(2)
}

fn step(pos: TilePos, dir: u8) -> Option<TilePos> {
    Dir::from_bits(dir).map(|d| pos.step(d))
}

fn broken(world: &World, e: Entity) -> bool {
    world.get::<Integrity>(e).is_some_and(|i| i.broken)
}

/// Map turbines: their state with a tier 1 part (`Initialize(mapload)`), then the rotors link
/// (`post_machine_initialize`), then the consoles find their rotor by `mapping_id`.
fn init_turbines(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmPower), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, String, String, u8)> = q
        .iter(world)
        .filter(|(_, dm)| is_under(&dm.path, TURBINE_ROOT) || is_under(&dm.path, COMPUTER))
        .map(|(e, dm)| (e, dm.path.clone(), dm.mapping_id.clone(), super::layer(dm)))
        .collect();
    let mut rotors = Vec::new();
    let mut computers = Vec::new();
    for (e, path, mapping_id, layer) in found {
        if is_under(&path, COMPUTER) {
            world.entity_mut(e).insert(TurbineComputer::default());
            computers.push((e, mapping_id));
            continue;
        }
        let Some(section) = Section::of_machine(&path) else {
            continue;
        };
        world
            .entity_mut(e)
            .insert(TurbineSection::new(section, Some(1)));
        if section == Section::Rotor {
            world.entity_mut(e).insert(TurbineRotor {
                mapping_id,
                layer,
                alert_at: f64::NEG_INFINITY,
                ..default()
            });
            rotors.push(e);
        }
    }
    for &rotor in &rotors {
        activate_parts(world, rotor, None, false);
    }
    for (computer, id) in computers {
        if id.is_empty() {
            continue;
        }
        let core = rotors.iter().copied().find(|&r| {
            world
                .get::<TurbineRotor>(r)
                .is_some_and(|r| r.mapping_id == id)
        });
        if let Some(mut c) = world.get_mut::<TurbineComputer>(computer) {
            c.core = core;
        }
    }
    if !rotors.is_empty() {
        info!("gas turbines: {}", rotors.len());
    }
}

/// `feedback`: a balloon to the user, if any.
fn feedback(world: &mut World, user: Option<Entity>, rotor: Entity, text: &str) {
    if let Some(user) = user {
        crate::balloon::balloon_alert(world, user, rotor, text);
    }
}

/// The first machine of `section` on `pos` (`locate() in get_step(...)`).
fn section_at(world: &World, pos: Option<TilePos>, section: Section) -> Option<Entity> {
    let pos = pos?;
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| {
            world
                .get::<TurbineSection>(e)
                .is_some_and(|s| s.section == section)
        })
}

/// Why `part` (the compressor or the outlet) cannot run with a rotor facing `dir`.
fn section_problem(
    world: &World,
    part: Option<Entity>,
    section: Section,
    dir: u8,
) -> Option<&'static str> {
    let compressor = section == Section::Compressor;
    let Some((e, state)) = part.and_then(|e| Some((e, world.get::<TurbineSection>(e)?))) else {
        return Some(if compressor {
            "нет компрессора!"
        } else {
            "нет турбины!"
        });
    };
    let part_dir = dir_of(world, e);
    // Not perpendicular to the rotor.
    if part_dir != dir && part_dir != crate::atmos_machinery::reverse_dir(dir) {
        return Some(if compressor {
            "компрессор не выровнен с ротором!"
        } else {
            "турбина не выровнена с ротором!"
        });
    }
    if state.maint {
        return Some(if compressor {
            "закройте панель компрессора!"
        } else {
            "закройте панель турбины!"
        });
    }
    if broken(world, e) {
        return Some(if compressor {
            "компрессор сломан!"
        } else {
            "турбина сломана!"
        });
    }
    if state.tier.is_none() {
        return Some(if compressor {
            "в компрессоре нет детали!"
        } else {
            "в турбине нет статора!"
        });
    }
    None
}

fn tier_of(world: &World, e: Entity) -> Option<u8> {
    world.get::<TurbineSection>(e).and_then(|s| s.tier)
}

/// `core_rotor/activate_parts(user, check_only)`: true when the three are linked. A full check
/// looks for the compressor behind and the outlet in front; `check_only` re-checks the linked
/// ones and changes nothing on success.
pub fn activate_parts(
    world: &mut World,
    rotor: Entity,
    user: Option<Entity>,
    check_only: bool,
) -> bool {
    let Some(state) = world.get::<TurbineRotor>(rotor).cloned() else {
        return false;
    };
    if !check_only && state.all_parts_connected {
        return true;
    }
    let fail = |world: &mut World, text: &str| {
        feedback(world, user, rotor, text);
        if let Some(mut r) = world.get_mut::<TurbineRotor>(rotor) {
            r.all_parts_connected = false;
        }
        false
    };
    if broken(world, rotor) {
        return fail(world, "ротор сломан!");
    }
    let Some(&pos) = world.get::<TilePos>(rotor) else {
        return false;
    };
    let dir = dir_of(world, rotor);
    let back = step(pos, crate::atmos_machinery::reverse_dir(dir));
    let front = step(pos, dir);
    let (mut compressor, mut turbine) = (state.compressor, state.turbine);
    if !check_only {
        compressor = section_at(world, back, Section::Compressor);
        turbine = section_at(world, front, Section::Outlet);
        if compressor.is_none() && turbine.is_none() {
            compressor = section_at(world, front, Section::Compressor);
            turbine = section_at(world, back, Section::Outlet);
            if compressor.is_some() || turbine.is_some() {
                return fail(world, "ротор повёрнут не в ту сторону!");
            }
        }
    }
    for (part, section) in [
        (compressor, Section::Compressor),
        (turbine, Section::Outlet),
    ] {
        if let Some(text) = section_problem(world, part, section, dir) {
            return fail(world, text);
        }
    }
    let (Some(compressor), Some(turbine)) = (compressor, turbine) else {
        return false;
    };
    // From the front they look away from each other (the sprites face opposite ways).
    if dir_of(world, compressor) != dir_of(world, turbine) {
        return fail(world, "турбина и компрессор не смотрят в разные стороны!");
    }
    // tg reads the rotor's own part unchecked; a rotor without one cannot link here.
    let (Some(ct), Some(tt), Some(rt)) = (
        tier_of(world, compressor),
        tier_of(world, turbine),
        tier_of(world, rotor),
    ) else {
        return fail(world, "в роторе нет детали!");
    };
    if let Some(mut r) = world.get_mut::<TurbineRotor>(rotor) {
        r.all_parts_connected = true;
    }
    if check_only {
        return true;
    }
    for part in [compressor, turbine] {
        if let Some(mut s) = world.get_mut::<TurbineSection>(part) {
            s.rotor = Some(rotor);
        }
    }
    let layer = {
        let mut r = world.get_mut::<TurbineRotor>(rotor).expect("checked");
        r.compressor = Some(compressor);
        r.turbine = Some(turbine);
        r.max_allowed_rpm = (tier_rpm(ct) + tier_rpm(tt) + tier_rpm(rt)) / 3.0;
        r.max_allowed_temperature =
            (tier_temperature(ct) + tier_temperature(tt) + tier_temperature(rt)) / 3.0;
        r.layer
    };
    // `connect_to_network`.
    let mut grid = world.resource_mut::<Grid>();
    if grid.node_net(rotor).is_none() {
        grid.remove_node(rotor);
        grid.add_node(rotor, pos, layer, NodeKind::Machine);
    }
    if let Some(mut s) = world.get_mut::<TurbineSection>(rotor) {
        s.maint = false;
    }
    true
}

/// `deactivate_parts`: a rotor unlinks and stops; a compressor or outlet takes its rotor down.
pub fn deactivate_parts(world: &mut World, e: Entity) {
    if let Some(mut s) = world.get_mut::<TurbineSection>(e) {
        s.maint = true;
    }
    if world.get::<TurbineRotor>(e).is_none() {
        let rotor = world
            .get_mut::<TurbineSection>(e)
            .and_then(|mut s| s.rotor.take());
        if let Some(rotor) = rotor.filter(|&r| world.get::<TurbineRotor>(r).is_some()) {
            deactivate_parts(world, rotor);
        }
        return;
    }
    let linked = {
        let mut r = world.get_mut::<TurbineRotor>(e).expect("checked");
        r.active = false;
        r.all_parts_connected = false;
        r.rpm = 0.0;
        r.produced_energy = 0.0;
        [r.compressor.take(), r.turbine.take()]
    };
    for part in linked.into_iter().flatten() {
        if let Some(mut s) = world.get_mut::<TurbineSection>(part)
            && s.rotor == Some(e)
        {
            s.rotor = None;
        }
    }
    // `disconnect_from_network`.
    world.resource_mut::<Grid>().remove_node(e);
}

/// `is_active`: the rotor runs (a compressor or an outlet: its rotor).
pub fn is_active(world: &World, e: Entity) -> bool {
    let rotor = if world.get::<TurbineRotor>(e).is_some() {
        Some(e)
    } else {
        world.get::<TurbineSection>(e).and_then(|s| s.rotor)
    };
    rotor
        .and_then(|r| world.get::<TurbineRotor>(r))
        .is_some_and(|r| r.active)
}

/// `transfer_gases` after the pump moved `transferred` into `output` (whose pressure was
/// `output_pressure` before): the work done, and `output` heated by it.
pub fn transfer_work(
    transferred: &GasMixture,
    output: &mut PipeAir,
    output_pressure: f64,
    work_amount_to_remove: f64,
) -> f64 {
    // A removed mixture has the default volume (`CELL_VOLUME`).
    let ratio =
        (CELL_VOLUME * pressure_max(transferred.pressure())) / (output.volume * output_pressure);
    let mut work = quantize(transferred.total_moles())
        * R_IDEAL_GAS_EQUATION
        * transferred.temperature
        * ratio.ln()
        * WORK_CONVERSION;
    work -= work_amount_to_remove;
    let capacity = output.heat_capacity();
    if capacity == 0.0 {
        return 0.0;
    }
    let temperature = output.temperature();
    work = work.min((capacity * temperature - capacity * TCMB) / HEAT_CONVERSION);
    output.gas.temperature =
        ((temperature * capacity + work * HEAT_CONVERSION) / capacity).max(TCMB);
    work
}

/// `transfer_gases` between two machine mixtures (`intake_size` 1).
pub fn transfer_between(
    input: &mut PipeAir,
    output: &mut PipeAir,
    work_amount_to_remove: f64,
) -> f64 {
    let output_pressure = pressure_max(output.pressure());
    let moles = pump_moles(&*input, &*output, input.pressure(), None);
    let Some(moved) = input.remove(moles) else {
        return 0.0;
    };
    output.merge(&moved);
    transfer_work(&moved, output, output_pressure, work_amount_to_remove)
}

/// `compress_gases`: the air behind the compressor into it; the input air's temperature.
fn compress_gases(
    atmos: &mut ss13_atmos::Atmos,
    input: Option<TilePos>,
    compressor: &mut TurbineSection,
) -> f64 {
    compressor.compressor_work = 0.0;
    compressor.compressor_pressure = MINIMUM_PRESSURE;
    let Some((pos, air)) = input.and_then(|p| Some((p, atmos.air(p)?.clone()))) else {
        return 0.0;
    };
    let output_pressure = pressure_max(compressor.gas.pressure());
    let target = air.pressure() * compressor.intake_regulator;
    let moles = pump_moles(&TurfMix(&air), &compressor.gas, target, None);
    if let Some(moved) = atmos.remove_air(pos, moles) {
        compressor.gas.merge(&moved);
        compressor.compressor_work =
            transfer_work(&moved, &mut compressor.gas, output_pressure, 0.0);
    }
    compressor.compressor_pressure = pressure_max(compressor.gas.pressure());
    air.temperature
}

/// `expel_gases`: the outlet's air out in front, unless that tile shares no air.
fn expel_gases(
    atmos: &mut ss13_atmos::Atmos,
    output: Option<TilePos>,
    outlet: &mut TurbineSection,
) -> Option<GasMixture> {
    let pos = output?;
    // `TURF_SHARES`.
    if atmos.adjacent_turfs(pos, false).is_empty() {
        return None;
    }
    let air = atmos.air(pos)?.clone();
    let moles = pump_moles(&outlet.gas, &TurfMix(&air), outlet.gas.pressure(), None);
    let moved = outlet.gas.remove(moles)?;
    atmos.merge_air(pos, &moved);
    Some(moved)
}

/// `round(log(90, max(temperature_difference, 1)), 0.5)`.
pub fn heat_damage(temperature_difference: f64) -> f64 {
    byond_round(temperature_difference.max(1.0).ln() / 90.0_f64.ln(), 0.5)
}

/// The explosion of a turbine at 0 integrity, by its rpm; `None` past the last band.
pub fn failure_explosion(rpm: f64) -> Option<(i32, i32, i32)> {
    let bands = [
        (MAX_BASE_RPM, (0, 1, 4)),
        (MAX_BASE_RPM * 2.5, (0, 2, 6)),
        (MAX_BASE_RPM * 2.5 * 2.5, (1, 3, 7)),
        (MAX_BASE_RPM * 2.5 * 2.5 * 2.5, (2, 5, 7)),
    ];
    bands
        .into_iter()
        .find(|&(limit, _)| rpm < limit)
        .map(|(_, e)| e)
}

/// The rpm from the work done (`rpm = ((work * ce) ** te) * re / 15`, capped).
pub fn rpm_of(work_done: f64, efficiencies: [f64; 3], max_rpm: f64) -> f64 {
    let [compressor, outlet, rotor] = efficiencies;
    let rpm = (work_done * compressor).powf(outlet) * rotor / RPM_CONVERSION;
    rpm.ceil().min(max_rpm)
}

fn process_turbines(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let mut q = world.query::<(Entity, &TurbineRotor)>();
    let active: Vec<Entity> = q
        .iter(world)
        .filter(|(_, r)| r.active)
        .map(|(e, _)| e)
        .collect();
    for _ in from..ticks {
        for &rotor in &active {
            process_rotor(world, rotor, MACHINE_TICK_SECS);
        }
    }
}

/// `core_rotor/process(seconds_per_tick)`.
pub fn process_rotor(world: &mut World, rotor: Entity, seconds: f64) {
    let running = world.get::<TurbineRotor>(rotor).is_some_and(|r| r.active);
    if !running
        || !activate_parts(world, rotor, None, true)
        || world.get::<Unpowered>(rotor).is_some()
    {
        if world.get::<TurbineRotor>(rotor).is_some() {
            deactivate_parts(world, rotor);
        }
        return;
    }
    let state = world.get::<TurbineRotor>(rotor).cloned().expect("checked");
    let (Some(ce), Some(te)) = (state.compressor, state.turbine) else {
        return;
    };
    let (Some(mut compressor), Some(mut core), Some(mut outlet)) = (
        world.get::<TurbineSection>(ce).cloned(),
        world.get::<TurbineSection>(rotor).cloned(),
        world.get::<TurbineSection>(te).cloned(),
    ) else {
        return;
    };
    let input = world
        .get::<TilePos>(ce)
        .and_then(|&p| step(p, crate::atmos_machinery::reverse_dir(dir_of(world, ce))));
    let output = world
        .get::<TilePos>(te)
        .and_then(|&p| step(p, dir_of(world, te)));

    // The compressor.
    let temperature = world
        .try_resource_scope(|_, mut atmos: Mut<ss13_atmos::Atmos>| {
            compress_gases(&mut atmos, input, &mut compressor)
        })
        .unwrap_or(0.0);
    let mut r = state;
    r.damage_archived = r.damage;
    let difference = temperature - r.max_allowed_temperature;
    let damage_done = heat_damage(difference);
    r.damage = (r.damage + damage_done * 0.5).max(0.0);
    r.damage = r.damage.min(r.damage_archived + MAX_TAKEN_DAMAGE);
    if difference < 0.0 {
        r.damage = (r.damage - DAMAGE_HEALING).max(0.0);
    }
    let now = crate::tools::now(world);
    let mut alert = None;
    if (r.damage - r.damage_archived >= 2.0 || r.damage > DAMAGE_ALARM_START) && now >= r.alert_at {
        r.alert_at = now + (DAMAGE_ALARM_START - damage_done).floor().max(5.0);
        alert = Some(r.integrity());
    }
    if let Some(mut s) = world.get_mut::<TurbineSection>(ce) {
        *s = compressor.clone();
    }
    if let Some(mut live) = world.get_mut::<TurbineRotor>(rotor) {
        *live = r.clone();
    }
    if let Some(integrity) = alert {
        if integrity <= 0.0 {
            let Some(&pos) = world.get::<TilePos>(rotor) else {
                return;
            };
            deactivate_parts(world, rotor);
            if let Some((dev, heavy, light)) = failure_explosion(r.rpm) {
                let mut ex = crate::explosions::Explosion::new(dev, heavy, light);
                ex.cause = "gas turbine".into();
                ex.source = Some(rotor);
                crate::explosions::explosion(world, pos, ex);
            }
            return;
        }
        damage_alert(world, rotor, integrity);
    }

    // The rotor expands the compressed gas to 3000 L, the outlet to 6000 L.
    let rotor_work = transfer_between(
        &mut compressor.gas,
        &mut core.gas,
        compressor.compressor_work,
    );
    let turbine_work = transfer_between(&mut core.gas, &mut outlet.gas, rotor_work.abs());
    let ejected = world
        .try_resource_scope(|_, mut atmos: Mut<ss13_atmos::Atmos>| {
            expel_gases(&mut atmos, output, &mut outlet)
        })
        .flatten();
    for (e, s) in [(ce, &compressor), (rotor, &core), (te, &outlet)] {
        if let Some(mut live) = world.get_mut::<TurbineSection>(e) {
            live.gas = s.gas.clone();
        }
    }
    let Some(ejected) = ejected else {
        // The output tile is blocked: no power.
        if let Some(mut live) = world.get_mut::<TurbineRotor>(rotor) {
            live.rpm = 0.0;
            live.produced_energy = 0.0;
        }
        return;
    };
    let work = quantize(ejected.total_moles())
        * R_IDEAL_GAS_EQUATION
        * ejected.temperature
        * (compressor.compressor_pressure / pressure_max(ejected.pressure())).ln();
    // The compressor's load comes off; the outlet's own work is what makes most of the power.
    let work =
        (work - compressor.compressor_work * COMPRESSOR_STATOR_INTERACTION - turbine_work).max(0.0);
    let rpm = rpm_of(
        work,
        [
            compressor.efficiency(),
            outlet.efficiency(),
            core.efficiency(),
        ],
        r.max_allowed_rpm,
    );
    let produced = rpm * ENERGY_RECTIFICATION * RPM_CONVERSION * seconds;
    if let Some(mut live) = world.get_mut::<TurbineRotor>(rotor) {
        live.rpm = rpm;
        live.produced_energy = produced;
    }
    let mut grid = world.resource_mut::<Grid>();
    if let Some(net) = grid.node_net(rotor).and_then(|n| grid.net_mut(n)) {
        net.newavail += produced;
    }
}

/// `aas_config_announce(/datum/aas_config_entry/engineering_turbine_failure)` and the alarm.
fn damage_alert(world: &mut World, rotor: Entity, integrity: f64) {
    let Some(&pos) = world.get::<TilePos>(rotor) else {
        return;
    };
    let area = world
        .resource::<ss13_mapload::Areas>()
        .area_at(pos)
        .map(|a| crate::shuttle::area_name(world, a))
        .unwrap_or_default();
    let integrity = super::supermatter::dm_number(integrity);
    let text = format!(
        "Внимание, турбина в {area} получает повреждения, текущая целостность {integrity}%!"
    );
    let name = crate::examine::name_of(world, rotor);
    world.write_message(crate::radio::MachineRadio {
        freq: ss13_chat::radio::FREQ_ENGINEERING,
        z: pos.z,
        name,
        text,
    });
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, ALERT_SOUND, 100.0, false)
            .extrarange(30.0)
            .falloff_exponent(30.0)
            .falloff_distance(10.0),
    );
}

/// `core_rotor/toggle_power()`.
fn toggle_power(world: &mut World, rotor: Entity) {
    if let Some(mut r) = world.get_mut::<TurbineRotor>(rotor) {
        r.active = !r.active;
    }
}

/// A part item into the user's hands (else onto the machine's tile).
fn give_part(world: &mut World, mob: Entity, machine: Entity, section: Section, tier: u8) {
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return;
    };
    let Some(item) = crate::construction::spawn_obj(world, section.part_path(), pos) else {
        return;
    };
    world.entity_mut(item).insert(TurbinePartItem { tier });
    crate::cooking::give_to(world, mob, machine, item);
}

fn alert(world: &mut World, mob: Entity, target: Entity, text: &str) {
    crate::balloon::balloon_alert(world, mob, target, text);
}

/// Items on the turbine's machines, its parts and its console. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<TurbinePartItem>(target).is_some() {
        return part_upgrade(world, client, mob, held, target);
    }
    if world.get::<TurbineComputer>(target).is_some() {
        return computer_multitool(world, client, held, target);
    }
    let Some(state) = world.get::<TurbineSection>(target).cloned() else {
        return false;
    };
    let tool = tool_of(world, held).map(|t| t.0);
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    match tool {
        Some(ToolKind::Screwdriver) => {
            if is_active(world, target) {
                alert(world, mob, target, "сначала выключите!");
            } else if !anchored(world, target) {
                alert(world, mob, target, "нужно прикрутить!");
            } else {
                let open = !state.panel_open;
                if let Some(mut s) = world.get_mut::<TurbineSection>(target) {
                    s.panel_open = open;
                }
                if open {
                    deactivate_parts(world, target);
                } else if state.section == Section::Rotor {
                    activate_parts(world, target, Some(mob), false);
                } else if let Some(mut s) = world.get_mut::<TurbineSection>(target) {
                    s.maint = false;
                }
            }
            true
        }
        // `default_change_direction_wrench`: clockwise with the panel open.
        Some(ToolKind::Wrench) => {
            if state.panel_open {
                let dir = crate::atmos_machinery::turn_cw(dir_of(world, target));
                if let Some(facing) = Dir::from_bits(dir) {
                    world.entity_mut(target).insert(ss13_shared::Facing(facing));
                }
                let name = crate::examine::name_of(world, target);
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("Вы поворачиваете {name}."),
                );
            }
            true
        }
        Some(ToolKind::Crowbar) if right => {
            if !state.panel_open {
                alert(world, mob, target, "панель закрыта!");
            } else if state.tier.is_none() {
                alert(world, mob, target, "деталь не установлена!");
            } else if is_active(world, target) {
                let name = crate::examine::name_of(world, target);
                alert(world, mob, target, &format!("{name} работает!"));
            } else if let Some(tier) = world
                .get_mut::<TurbineSection>(target)
                .and_then(|mut s| s.tier.take())
            {
                give_part(world, mob, target, state.section, tier);
            }
            true
        }
        Some(ToolKind::Multitool) if state.section == Section::Rotor && !state.panel_open => {
            if activate_parts(world, target, Some(mob), false) {
                alert(world, mob, target, "все части связаны");
                crate::tools::set_multitool_buffer(world, held, target);
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы сохраняете данные о связи в буфер мультитула.",
                );
            }
            true
        }
        _ => {
            let fits = world.get::<TurbinePartItem>(held).is_some()
                && crate::construction::path_of(world, held).and_then(|p| Section::of_part(&p))
                    == Some(state.section);
            if !fits {
                return false;
            }
            if is_active(world, target) {
                alert(world, mob, target, "сначала выключите машину!");
            } else if !state.panel_open {
                alert(
                    world,
                    mob,
                    target,
                    "сначала откройте панель техобслуживания!",
                );
            } else {
                crate::tools::do_after(
                    world,
                    actor,
                    target,
                    20.0,
                    0,
                    Job::Power(PowerJob::Turbine(TurbineJob::Install {
                        tier_before: state.tier,
                    })),
                );
            }
            true
        }
    }
}

/// `turbine_parts/item_interaction`: sheets raise the part a tier after `tier` seconds.
fn part_upgrade(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    part: Entity,
) -> bool {
    let Some(tier) = world.get::<TurbinePartItem>(part).map(|p| p.tier) else {
        return false;
    };
    let Some(section) =
        crate::construction::path_of(world, part).and_then(|p| Section::of_part(&p))
    else {
        return false;
    };
    let Some((sheet, amount, _)) = tier_upgrade(section, tier) else {
        alert(world, mob, part, "уже максимальный уровень!");
        return true;
    };
    if !crate::construction::path_of(world, held).is_some_and(|p| is_under(&p, sheet)) {
        alert(world, mob, part, "неподходящий материал!");
        return true;
    }
    if crate::tools::stack_amount(world, held) < amount {
        alert(world, mob, part, &format!("нужно {amount} листов!"));
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    crate::tools::do_after(
        world,
        actor,
        part,
        f32::from(tier) * 10.0,
        amount,
        Job::Power(PowerJob::Turbine(TurbineJob::Upgrade { tier, amount })),
    );
    true
}

/// `turbine_computer/multitool_act`: the rotor in the buffer becomes the console's turbine.
fn computer_multitool(world: &mut World, client: Entity, held: Entity, console: Entity) -> bool {
    if tool_of(world, held).map(|t| t.0) != Some(ToolKind::Multitool) {
        return false;
    }
    let Some(rotor) = crate::tools::multitool_buffer(world, held)
        .filter(|&b| world.get::<TurbineRotor>(b).is_some())
    else {
        tell(
            world,
            client,
            Span::Notice,
            "В буфере мультитула машина не того типа...",
        );
        return true;
    };
    if world
        .get::<TurbineComputer>(console)
        .is_some_and(|c| c.core.is_some())
    {
        tell(world, client, Span::Notice, "Смена сети консоли...");
    }
    if let Some(mut c) = world.get_mut::<TurbineComputer>(console) {
        c.core = Some(rotor);
    }
    tell(
        world,
        client,
        Span::Notice,
        "Вы связываете консоль с машиной из буфера мультитула.",
    );
    true
}

/// The end of a part step, checked again.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: TurbineJob) {
    let Some(item) = action.item else { return };
    match job {
        TurbineJob::Install { tier_before } => {
            let machine = action.target;
            let Some(state) = world.get::<TurbineSection>(machine).cloned() else {
                return;
            };
            let Some(tier) = world.get::<TurbinePartItem>(item).map(|p| p.tier) else {
                return;
            };
            if state.tier != tier_before || !state.panel_open || is_active(world, machine) {
                return;
            }
            world.despawn(item);
            if let Some(old) = state.tier {
                give_part(world, mob, machine, state.section, old);
                alert(world, mob, machine, "деталь заменена на ту, что в руке");
            } else {
                alert(world, mob, machine, "установлена новая деталь");
            }
            if let Some(mut s) = world.get_mut::<TurbineSection>(machine) {
                s.tier = Some(tier);
            }
        }
        TurbineJob::Upgrade { tier, amount } => {
            let part = action.target;
            if world.get::<TurbinePartItem>(part).map(|p| p.tier) != Some(tier) {
                return;
            }
            if crate::tools::use_stack(world, item, amount)
                && let Some(mut p) = world.get_mut::<TurbinePartItem>(part)
            {
                p.tier = (tier + 1).min(MAX_TIER);
            }
        }
    }
}

/// `examine` of the machines and the parts.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(part) = world.get::<TurbinePartItem>(target) {
        let Some(section) =
            crate::construction::path_of(world, target).and_then(|p| Section::of_part(&p))
        else {
            return;
        };
        lines.push((
            Span::Notice,
            format!(
                "Это деталь турбины уровня {}, рассчитанная на {} об/мин и {} K.",
                part.tier,
                super::supermatter::dm_number(tier_rpm(part.tier)),
                super::supermatter::dm_number(tier_temperature(part.tier)),
            ),
        ));
        lines.push(match tier_upgrade(section, part.tier) {
            Some((_, amount, name)) => (
                Span::Notice,
                format!("Можно улучшить {amount} листами {name}."),
            ),
            None => (Span::Notice, "Уже максимального уровня.".to_owned()),
        });
        return;
    }
    let Some(state) = world.get::<TurbineSection>(target) else {
        return;
    };
    match state.tier {
        Some(tier) => {
            lines.push((Span::Notice, format!("Текущий уровень: {tier}.")));
            if tier + 1 < MAX_TIER {
                lines.push((
                    Span::Notice,
                    format!("Можно улучшить деталью уровня {}.", tier + 1),
                ));
            }
            lines.push((
                Span::Notice,
                format!(
                    "{} можно вытащить ломом.",
                    ss13_shared::capitalize(state.section.part_name())
                ),
            ));
        }
        None => lines.push((
            Span::Warning,
            format!("Отсутствует {}.", state.section.part_name()),
        )),
    }
    let action = if state.panel_open {
        "закрыть"
    } else {
        "открыть"
    };
    lines.push((
        Span::Notice,
        format!("Панель техобслуживания можно {action} отвёрткой."),
    ));
    if state.panel_open {
        lines.push((Span::Notice, "Можно повернуть гаечным ключом.".to_owned()));
    }
    if world
        .get::<TurbineRotor>(target)
        .is_some_and(|r| !r.all_parts_connected)
    {
        lines.push((Span::Warning, "Части нужно связать мультитулом.".to_owned()));
    }
}

/// `set_overlays` and `update_overlays` of the three machines.
fn turbine_looks(
    icons: Option<Res<IconIndex>>,
    rotors: Query<&TurbineRotor>,
    mut machines: Query<(
        &TurbineSection,
        &DmAtom,
        &mut Appearance,
        Option<&TurbineRotor>,
    )>,
) {
    let Some(icons) = icons else { return };
    for (state, atom, mut appearance, own) in &mut machines {
        let active = own
            .or_else(|| state.rotor.and_then(|r| rotors.get(r).ok()))
            .is_some_and(|r| r.active);
        let base = state.section.base_icon_state();
        let mut states: Vec<(String, bool)> = Vec::new();
        if state.panel_open {
            states.push((format!("{base}_open"), false));
        }
        if state.section == Section::Rotor {
            if active {
                states.push((format!("{base}_on"), false));
                states.push((format!("{base}_on"), true));
            }
        } else {
            let on = if active { "on" } else { "off" };
            states.push((format!("{base}_{on}"), false));
        }
        let overlays: Vec<Overlay> = states
            .iter()
            .filter_map(|(s, emissive)| {
                icons.get(&atom.icon, s).map(|s| Overlay {
                    plane: emissive.then_some(EMISSIVE_PLANE),
                    ..Overlay::new(s)
                })
            })
            .collect();
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
}

// --- The console's window --------------------------------------------------------------------

pub struct TurbineWindow;

impl UiInterface for TurbineWindow {
    const NAME: &'static str = TurbineComputerUi::NAME;
    type Data = TurbineComputerData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TurbineAct {
    TogglePower,
    /// Percent, 1-100.
    Regulate(f64),
}

/// A click on a console opens its window while powered.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<TurbineComputer>(target).is_none() {
        return false;
    }
    if world.get::<Unpowered>(target).is_none() {
        let title = world.get::<DmName>(target).map_or_else(String::new, |n| {
            crate::vending::plain_name(&n.name).to_owned()
        });
        world.write_message(OpenUi::new::<TurbineWindow>(client, mob, target, title));
    }
    true
}

type WindowParam = (
    Query<'static, 'static, &'static TurbineRotor>,
    Query<
        'static,
        'static,
        (
            &'static TurbineSection,
            &'static TilePos,
            Option<&'static ss13_shared::Facing>,
            Option<&'static DmAtom>,
        ),
    >,
    Option<Res<'static, ss13_atmos::Atmos>>,
);

/// `ui_data`.
pub fn window_data(
    computer: &TurbineComputer,
    (rotors, sections, atmos): &SystemParamItem<'_, '_, WindowParam>,
) -> TurbineComputerData {
    let Some(rotor) = computer
        .core
        .and_then(|c| rotors.get(c).ok())
        .filter(|r| r.all_parts_connected)
    else {
        return TurbineComputerData::default();
    };
    let compressor = rotor.compressor.and_then(|c| sections.get(c).ok());
    let temp = compressor
        .and_then(|(_, &pos, facing, atom)| {
            let dir = facing
                .map(|f| f.0 as u8)
                .or(atom.map(|a| a.dir))
                .unwrap_or(2);
            step(pos, crate::atmos_machinery::reverse_dir(dir))
        })
        .and_then(|p| atmos.as_ref()?.temperature(p))
        .unwrap_or(0.0);
    TurbineComputerData {
        connected: true,
        active: rotor.active,
        rpm: rotor.rpm as f32,
        power: (rotor.produced_energy / MACHINE_TICK_SECS) as f32,
        temp: temp as f32,
        integrity: rotor.integrity() as f32,
        max_rpm: rotor.max_allowed_rpm as f32,
        max_temperature: rotor.max_allowed_temperature as f32,
        regulator: compressor.map_or(0.0, |(s, ..)| s.intake_regulator) as f32,
    }
}

impl ServerUi for TurbineWindow {
    type Source = &'static TurbineComputer;
    type UserParam = WindowParam;
    type Act = TurbineAct;

    fn static_data(_: &TurbineComputer) {}

    fn data(
        computer: &TurbineComputer,
        _: Entity,
        param: &SystemParamItem<'_, '_, WindowParam>,
    ) -> TurbineComputerData {
        window_data(computer, param)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<TurbineAct, UiReject> {
        match action {
            "toggle_power" => Ok(TurbineAct::TogglePower),
            "regulate" => Ok(TurbineAct::Regulate(params.number("regulate")?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act`, every check at the moment of effect.
fn window_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<TurbineWindow>>>) {
    let pending: Vec<(Entity, Entity, TurbineAct)> = acts
        .read(world.resource::<Messages<UiActed<TurbineWindow>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, console, act) in pending {
        if world.get::<Unpowered>(console).is_some() {
            continue;
        }
        let Some(rotor) = world
            .get::<TurbineComputer>(console)
            .and_then(|c| c.core)
            .filter(|&r| world.get::<TurbineRotor>(r).is_some())
        else {
            continue;
        };
        match act {
            TurbineAct::TogglePower => {
                let r = world.get::<TurbineRotor>(rotor).expect("checked");
                let (active, rpm) = (r.active, r.rpm);
                // On: every part linked; off: under 1000 rpm.
                if !active {
                    if !activate_parts(world, rotor, Some(actor), true) {
                        continue;
                    }
                } else if rpm > SHUTDOWN_RPM {
                    continue;
                }
                toggle_power(world, rotor);
                if let Some(mut r) = world.get_mut::<TurbineRotor>(rotor) {
                    r.rpm = 0.0;
                    r.produced_energy = 0.0;
                }
            }
            TurbineAct::Regulate(percent) => {
                if !percent.is_finite() {
                    continue;
                }
                let Some(compressor) = world.get::<TurbineRotor>(rotor).and_then(|r| r.compressor)
                else {
                    continue;
                };
                if let Some(mut s) = world.get_mut::<TurbineSection>(compressor) {
                    s.intake_regulator = (percent / 100.0).clamp(0.01, 1.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_atmos::GasId;

    #[test]
    fn tier_values_follow_tg() {
        assert_eq!(tier_rpm(1), 50_000.0);
        assert_eq!(tier_rpm(3), 312_500.0);
        assert_eq!(tier_temperature(1), 50_000.0);
        assert!((tier_temperature(2) - 50_000.0_f64.powf(1.2)).abs() < 1e-6);
        assert_eq!(tier_efficiency(Section::Compressor, 1), 0.25);
        assert!((tier_efficiency(Section::Rotor, 4) - 0.85).abs() < 1e-12);
        assert!((tier_efficiency(Section::Outlet, 2) - 0.865).abs() < 1e-12);
        assert_eq!(
            tier_upgrade(Section::Outlet, 3).map(|u| (u.0, u.1)),
            Some(("/obj/item/stack/sheet/mineral/zaukerite", 10))
        );
        assert_eq!(tier_upgrade(Section::Compressor, 4), None);
    }

    #[test]
    fn damage_and_failure_bands() {
        // Under the rating: log(90, 1) = 0.
        assert_eq!(heat_damage(-500.0), 0.0);
        // 90^2 K over: 2.
        assert_eq!(heat_damage(8100.0), 2.0);
        assert_eq!(failure_explosion(10_000.0), Some((0, 1, 4)));
        assert_eq!(failure_explosion(100_000.0), Some((0, 2, 6)));
        assert_eq!(failure_explosion(1_000_000.0), None);
        let mut r = TurbineRotor::default();
        r.damage = 250.0;
        assert_eq!(r.integrity(), 50.0);
        r.damage = 600.0;
        assert_eq!(r.integrity(), 0.0);
    }

    #[test]
    fn rpm_is_capped_by_the_parts() {
        let rpm = rpm_of(1.0e6, [0.25, 0.85, 0.25], 50_000.0);
        let want = ((1.0e6_f64 * 0.25).powf(0.85) * 0.25 / 15.0).ceil();
        assert_eq!(rpm, want.min(50_000.0));
        assert_eq!(rpm_of(1.0e30, [0.25, 0.85, 0.25], 50_000.0), 50_000.0);
    }

    #[test]
    fn expansion_between_mixtures_moves_gas_and_heats_the_output() {
        let mut input = PipeAir::new(1000.0);
        input.gas.set(GasId::PLASMA, 100.0);
        input.gas.temperature = 1000.0;
        let mut output = PipeAir::new(3000.0);
        let before = input.total_moles();
        let work = transfer_between(&mut input, &mut output, 0.0);
        assert!(output.total_moles() > 0.0);
        assert!((input.total_moles() + output.total_moles() - before).abs() < 1e-6);
        assert!(work.is_finite());
        // Nothing to move: no work.
        let mut empty = PipeAir::new(1000.0);
        assert_eq!(transfer_between(&mut empty, &mut output, 0.0), 0.0);
    }
}
