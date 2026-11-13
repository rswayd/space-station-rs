//! The Hypertorus Fusion Reactor's machines (server): tg `code/modules/atmospherics/machinery/
//! components/fusion/` (`hfr_core.dm`, `hfr_parts.dm`, `hfr_procs.dm`); the reaction itself is
//! [`ss13_atmos::fusion`].
//!
//! - Assembly: a multitool on the core box with the eight other boxes around it (corners on the
//!   diagonals, the fuel input, moderator input, waste output and interface on the sides) turns
//!   them into the machines, facing outward ([`build_reactor`]). The core and the three ports are
//!   pipe devices (the core's port is the coolant loop).
//! - A multitool on the interface checks the 3x3 ([`connectivity`]: anchored, panels closed,
//!   every part facing away from the core, corners turned their way) and links it; a part that
//!   goes, turns or opens unlinks it on the next fire.
//! - The interface's window `Hypertorus` runs the core: power, cooling, fuel and moderator
//!   injection, the recipe, the knobs, waste filtering.
//! - Every SSair fire a linked core runs [`ss13_atmos::fusion::Reactor::process`] on its four
//!   ports and applies the effects: hallucinations, lightning, gravity pulls, radiation, the
//!   moderator cracking a port and leaking (a welder mends it, 10 s).
//! - Damage: radio warnings (engineering, the emergency on common), alarms by integrity; past
//!   the melting point a 30 s countdown ([`countdown_tick`]) ends in [`meltdown`]: gas spread
//!   around, an explosion, radiation and an EMP by the fuel's flags; the reactor is gone.
//!
//! Cost: O(cores) per SSair fire (a 3x3 scan each); stations have none or one.
// shortcut: no construction from boards or deconstruction (crowbar), no sound loop or accent
// sounds, no nuclear particles (the port has none), no EMP scrambling of the radio, no ghost
// notification, no guide paper, gravity pulls ignore magboots; the countdown runs on game time.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_atmos::fusion::{
    self, EMERGENCY_POINT, FUELS, FUSION_MAXIMUM_TEMPERATURE, MELTING_POINT, Overflow, Ports,
    Reactor, WARNING_POINT, warning,
};
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::pipenet::air::PipeAir;
use ss13_atmos::{AtmosSystems, FIRE_INTERVAL, GAS_COUNT, GasId, air_tick_due};
use ss13_chat::Span;
use ss13_chat::radio::{FREQ_COMMON, FREQ_ENGINEERING};
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmName};
use ss13_shared::appearance::Overlay;
use ss13_shared::{Appearance, Dir, EMISSIVE_PLANE, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::hypertorus::{
    HfrFilter, HfrFuelEntry, HfrGas, HypertorusData, HypertorusStatic, HypertorusUi,
};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use super::AtmosDevice;
use crate::power::Unpowered;
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of};

pub const CORE: &str = "/obj/machinery/atmospherics/components/unary/hypertorus/core";
pub const FUEL_INPUT: &str = "/obj/machinery/atmospherics/components/unary/hypertorus/fuel_input";
pub const WASTE_OUTPUT: &str =
    "/obj/machinery/atmospherics/components/unary/hypertorus/waste_output";
pub const MODERATOR_INPUT: &str =
    "/obj/machinery/atmospherics/components/unary/hypertorus/moderator_input";
pub const INTERFACE: &str = "/obj/machinery/hypertorus/interface";
pub const CORNER: &str = "/obj/machinery/hypertorus/corner";
const BOX_ROOT: &str = "/obj/item/hfr_box";
const BOX_CORE: &str = "/obj/item/hfr_box/core";
const BOX_CORNER: &str = "/obj/item/hfr_box/corner";
/// The body boxes and what they deploy into.
const BODY_BOXES: [(&str, &str); 4] = [
    ("/obj/item/hfr_box/body/fuel_input", FUEL_INPUT),
    ("/obj/item/hfr_box/body/moderator_input", MODERATOR_INPUT),
    ("/obj/item/hfr_box/body/waste_output", WASTE_OUTPUT),
    ("/obj/item/hfr_box/body/interface", INTERFACE),
];
const SAFE_ALERT: &str = "Главное защитное поле возвращается к безопасным рабочим параметрам.";
const WARNING_ALERT: &str = "Опасность! Магнитное поле сдерживания ослабевает!";
const EMERGENCY_ALERT: &str = "РАЗРУШЕНИЕ ГИПЕРТОРУСА НЕИЗБЕЖНО.";
/// `WARNING_TIME_DELAY`, seconds.
const WARNING_TIME_DELAY: f64 = 60.0;
/// `HYPERTORUS_COUNTDOWN_TIME`, deciseconds.
const COUNTDOWN_DS: i32 = 300;
/// `DEFAULT_RADIATION_CHANCE`.
const RADIATION_CHANCE: f32 = 10.0;
/// Welding a crack shut, deciseconds.
const REPAIR_DS: f32 = 100.0;
const ZAP_SOUND: &str = "sound/items/weapons/emitter2.ogg";
const CRITICAL_SOUND: &str = "sound/machines/hypertorus/HFR_critical_explosion.ogg";

/// The eight machines around and including the core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HfrKind {
    Core,
    FuelInput,
    WasteOutput,
    ModeratorInput,
    Interface,
    Corner,
}

impl HfrKind {
    pub fn of_path(path: &str) -> Option<Self> {
        [
            (CORE, Self::Core),
            (FUEL_INPUT, Self::FuelInput),
            (WASTE_OUTPUT, Self::WasteOutput),
            (MODERATOR_INPUT, Self::ModeratorInput),
            (INTERFACE, Self::Interface),
            (CORNER, Self::Corner),
        ]
        .into_iter()
        .find(|(root, _)| is_under(path, root))
        .map(|(_, k)| k)
    }

    /// `base_icon_state`.
    fn base(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::FuelInput => "fuel_input",
            Self::WasteOutput => "waste_output",
            Self::ModeratorInput => "moderator_input",
            Self::Interface => "interface",
            Self::Corner => "corner",
        }
    }

    /// The pipe machines (`/obj/machinery/atmospherics/components/unary/hypertorus`).
    fn is_pipe(self) -> bool {
        !matches!(self, Self::Interface | Self::Corner)
    }
}

/// Every HFR machine.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HfrPart {
    pub kind: HfrKind,
    pub panel_open: bool,
    pub active: bool,
    pub fusion_started: bool,
    /// The ports: cracked by the moderator (`cracked`).
    pub cracked: bool,
}

impl HfrPart {
    pub fn new(kind: HfrKind) -> Self {
        Self {
            kind,
            panel_open: false,
            active: false,
            fusion_started: false,
            cracked: false,
        }
    }
}

/// The linked parts of a core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Links {
    pub interface: Entity,
    pub input: Entity,
    pub output: Entity,
    pub moderator: Entity,
}

/// The core's state beyond the reaction.
#[derive(Component, Clone, Debug, Default)]
pub struct HfrCore {
    pub reactor: Reactor,
    /// Set while active (`active` and the `linked_*`).
    pub links: Option<Links>,
    pub corners: Vec<Entity>,
    /// Game time of the last warning (`lastwarning`).
    pub lastwarning: f64,
    pub has_reached_emergency: bool,
    /// The final countdown: deciseconds left and the game time of the next step.
    pub countdown: Option<(i32, f64)>,
    /// The area APC's cell, percent (`get_area_cell_percent`).
    pub apc_percent: f64,
}

/// `/obj/machinery/hypertorus/interface`: `connected_core`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HfrInterface {
    pub core: Option<Entity>,
}

/// Timed HFR steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HfrJob {
    /// `welder_act`: a cracked port mended.
    Repair,
}

pub fn plugin(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.register_ui::<HfrWindow>()
        .add_systems(ss13_world::MapInitLate, init_mapped)
        .add_systems(
            Update,
            (
                process_cores
                    .run_if(air_tick_due)
                    .after(AtmosSystems::Machinery),
                countdown_tick.after(AtmosSystems::Machinery),
                hfr_looks.after(AtmosSystems::Machinery),
                window_acts.in_set(UiSystems::Handle),
            ),
        );
    // The interface's window.
    app.add_click_handler(ClickPhase::EmptyHand, 1781, |w, c| {
        interact(w, c.client, c.mob, c.target)
    })
    // Boxes, multitools, screwdrivers, wrenches and welders on the machines.
    .add_click_handler(ClickPhase::HeldOnTarget, 3991, |w, c| {
        let Some(held) = c.held else { return false };
        item_interaction(w, c.client, c.mob, held, c.target)
    });
}

/// Mapped machines get their state (maps carry the boxes; this is for edited maps).
fn init_mapped(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &ss13_mapload::DmObj), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, HfrKind)> = q
        .iter(world)
        .filter_map(|(e, o)| Some((e, HfrKind::of_path(&o.path)?)))
        .collect();
    for (e, kind) in found {
        setup(world, e, kind);
    }
}

fn setup(world: &mut World, e: Entity, kind: HfrKind) {
    world.entity_mut(e).insert(HfrPart::new(kind));
    match kind {
        HfrKind::Core => {
            world.entity_mut(e).insert(HfrCore {
                lastwarning: f64::NEG_INFINITY,
                ..default()
            });
        }
        HfrKind::Interface => {
            world.entity_mut(e).insert(HfrInterface::default());
        }
        _ => {}
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

/// The eight tiles around `pos` with their direction from it (`orange(1)`).
fn around(pos: TilePos) -> impl Iterator<Item = (u8, TilePos)> {
    [1u8, 2, 4, 8, 5, 6, 9, 10]
        .into_iter()
        .filter_map(move |d| Some((d, step(pos, d)?)))
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    crate::construction::path_of(world, e)
}

// --- Assembly ------------------------------------------------------------------------------

/// What a box deploys into: `Some(None)` for the core box, `Some(Some(path))` for the others.
fn box_part(path: &str) -> Option<Option<&'static str>> {
    if !is_under(path, BOX_ROOT) {
        return None;
    }
    if is_under(path, BOX_CORE) {
        return Some(None);
    }
    if is_under(path, BOX_CORNER) {
        return Some(Some(CORNER));
    }
    BODY_BOXES
        .iter()
        .find(|(b, _)| is_under(path, b))
        .map(|&(_, part)| Some(part))
}

/// `hfr_box/core/multitool_act`: the boxes around it, corners on the diagonals turned
/// (NE east, SE south, SW west, NW north) and bodies on the sides facing out; with all eight
/// the reactor is built.
fn assemble(world: &mut World, core_box: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(core_box) else {
        return false;
    };
    let mut parts: Vec<(Entity, &'static str, u8, TilePos)> = Vec::new();
    for (direction, tile) in around(pos) {
        let boxes: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(tile).to_vec();
        for b in boxes {
            let Some(Some(part)) = path_of(world, b).and_then(|p| box_part(&p)) else {
                continue;
            };
            let diagonal = !matches!(direction, 1 | 2 | 4 | 8);
            let facing = match (part == CORNER, direction) {
                (true, 5) => 4,
                (true, 6) => 2,
                (true, 10) => 8,
                (true, 9) => 1,
                (false, d) if !diagonal => d,
                _ => continue,
            };
            parts.push((b, part, facing, tile));
        }
    }
    if parts.len() != 8 {
        return false;
    }
    build_reactor(world, core_box, pos, &parts);
    true
}

/// `build_reactor`: the boxes become the machines.
fn build_reactor(
    world: &mut World,
    core_box: Entity,
    pos: TilePos,
    parts: &[(Entity, &'static str, u8, TilePos)],
) {
    for &(b, part, facing, tile) in parts {
        world.despawn(b);
        spawn_part(world, part, tile, facing);
    }
    world.despawn(core_box);
    if let Some(core) = spawn_part(world, CORE, pos, 2) {
        crate::power::register_machine(world, core);
    }
}

fn spawn_part(world: &mut World, path: &str, pos: TilePos, facing: u8) -> Option<Entity> {
    let kind = HfrKind::of_path(path)?;
    let Some(e) = crate::construction::spawn_obj(world, path, pos) else {
        warn!("hfr: no prototype for {path}");
        return None;
    };
    if let Some(dir) = Dir::from_bits(facing) {
        world.entity_mut(e).insert(ss13_shared::Facing(dir));
    }
    setup(world, e, kind);
    if kind.is_pipe() {
        super::join_built(world, e);
    }
    Some(e)
}

// --- Linking -------------------------------------------------------------------------------

/// `check_part_connectivity`: the core anchored with its panel shut, every part around it
/// closed, the side parts facing away from the core, the corners on the diagonals turned their
/// way, one of each side part and four corners.
pub fn connectivity(world: &World, core: Entity) -> Option<(Links, Vec<Entity>)> {
    let pos = *world.get::<TilePos>(core)?;
    let core_part = world.get::<HfrPart>(core)?;
    if !crate::obj_damage::anchored(world, core) || core_part.panel_open {
        return None;
    }
    let (mut interface, mut input, mut output, mut moderator) = (None, None, None, None);
    let mut corners = Vec::new();
    for (direction, tile) in around(pos) {
        for &e in world.resource::<SpatialIndex>().on_tile(tile) {
            let Some(part) = world.get::<HfrPart>(e) else {
                continue;
            };
            if part.panel_open || part.kind == HfrKind::Core {
                return None;
            }
            let dir = dir_of(world, e);
            if part.kind == HfrKind::Corner {
                let wanted = match direction {
                    6 => 2,
                    10 => 8,
                    5 => 4,
                    9 => 1,
                    _ => return None,
                };
                if dir != wanted {
                    return None;
                }
                corners.push(e);
                continue;
            }
            if step(tile, super::reverse_dir(dir)) != Some(pos) {
                return None;
            }
            let slot = match part.kind {
                HfrKind::Interface => &mut interface,
                HfrKind::FuelInput => &mut input,
                HfrKind::WasteOutput => &mut output,
                HfrKind::ModeratorInput => &mut moderator,
                _ => continue,
            };
            if slot.is_some_and(|s| s != e) {
                return None;
            }
            *slot = Some(e);
        }
    }
    if corners.len() != 4 {
        return None;
    }
    Some((
        Links {
            interface: interface?,
            input: input?,
            output: output?,
            moderator: moderator?,
        },
        corners,
    ))
}

fn set_active(world: &mut World, parts: impl IntoIterator<Item = Entity>, active: bool) {
    for e in parts {
        if let Some(mut p) = world.get_mut::<HfrPart>(e) {
            p.active = active;
            if !active {
                p.fusion_started = false;
            }
        }
    }
}

fn all_parts(core: Entity, links: Links, corners: &[Entity]) -> Vec<Entity> {
    let mut out = vec![
        core,
        links.interface,
        links.input,
        links.output,
        links.moderator,
    ];
    out.extend_from_slice(corners);
    out
}

/// `activate`.
fn activate(world: &mut World, client: Entity, core: Entity, links: Links, corners: Vec<Entity>) {
    if world
        .get::<HfrCore>(core)
        .is_some_and(|c| c.links.is_some())
    {
        tell(world, client, Span::Notice, "Машина уже активирована.");
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        "Вы связываете все части вместе.",
    );
    set_active(world, all_parts(core, links, &corners), true);
    if let Some(mut c) = world.get_mut::<HfrCore>(core) {
        c.links = Some(links);
        c.corners = corners;
    }
}

/// `deactivate`.
pub fn deactivate(world: &mut World, core: Entity) {
    let Some((links, corners)) = world
        .get_mut::<HfrCore>(core)
        .and_then(|mut c| Some((c.links.take()?, std::mem::take(&mut c.corners))))
    else {
        return;
    };
    let parts: Vec<Entity> = all_parts(core, links, &corners)
        .into_iter()
        .filter(|&e| world.get_entity(e).is_ok())
        .collect();
    set_active(world, parts, false);
}

// --- Interaction ---------------------------------------------------------------------------

/// The interface's window while active (`ui_interact`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<HfrInterface>(target).is_none() {
        return false;
    }
    if world.get::<HfrPart>(target).is_some_and(|p| p.active) {
        let title = world.get::<DmName>(target).map_or_else(String::new, |n| {
            crate::vending::plain_name(&n.name).to_owned()
        });
        world.write_message(OpenUi::new::<HfrWindow>(client, mob, target, title));
    } else {
        tell(
            world,
            client,
            Span::Notice,
            "Сначала активируйте машину, применив мультитул к интерфейсу.",
        );
    }
    true
}

/// Items on the boxes and machines. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let tool = tool_of(world, held).map(|t| t.0);
    // The core box deploys where it lies.
    if tool == Some(ToolKind::Multitool)
        && world.get::<crate::inventory::CarriedBy>(target).is_none()
        && path_of(world, target).is_some_and(|p| is_under(&p, BOX_CORE))
    {
        assemble(world, target);
        return true;
    }
    let Some(part) = world.get::<HfrPart>(target).copied() else {
        return false;
    };
    match tool {
        // `default_deconstruction_screwdriver`, not once fusion started.
        Some(ToolKind::Screwdriver) => {
            if part.fusion_started {
                return false;
            }
            let open = !part.panel_open;
            if let Some(mut p) = world.get_mut::<HfrPart>(target) {
                p.panel_open = open;
            }
            let text = if open {
                "Вы открываете панель техобслуживания."
            } else {
                "Вы закрываете панель техобслуживания."
            };
            tell(world, client, Span::Notice, text);
            true
        }
        // `default_change_direction_wrench`.
        Some(ToolKind::Wrench) => {
            if !part.panel_open {
                return false;
            }
            rotate(world, target);
            let name = crate::examine::name_of(world, target);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы поворачиваете {name}."),
            );
            true
        }
        Some(ToolKind::Welder) if part.cracked => {
            if crate::medical::surgery::combat_mode(world, mob) {
                return false;
            }
            crate::balloon::balloon_alert(world, mob, target, "ремонт...");
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            crate::tools::use_tool(
                world,
                actor,
                target,
                REPAIR_DS,
                0,
                crate::construction::Job::Atmos(super::windows::AtmosJob::Hfr(HfrJob::Repair)),
            );
            true
        }
        // `interface/multitool_act`: the core behind it links.
        Some(ToolKind::Multitool) if part.kind == HfrKind::Interface => {
            let core = world.get::<TilePos>(target).and_then(|&pos| {
                let tile = step(pos, super::reverse_dir(dir_of(world, target)))?;
                world
                    .resource::<SpatialIndex>()
                    .on_tile(tile)
                    .iter()
                    .copied()
                    .find(|&e| world.get::<HfrCore>(e).is_some())
            });
            match core.and_then(|c| Some((c, connectivity(world, c)?))) {
                Some((core, (links, corners))) if links.interface == target => {
                    if let Some(mut i) = world.get_mut::<HfrInterface>(target) {
                        i.core = Some(core);
                    }
                    activate(world, client, core, links, corners);
                }
                _ => tell(
                    world,
                    client,
                    Span::Notice,
                    "Проверьте все части и попробуйте снова.",
                ),
            }
            true
        }
        _ => false,
    }
}

/// A clockwise quarter turn; a pipe machine joins the pipes again facing its new way.
fn rotate(world: &mut World, e: Entity) {
    let dir = super::turn_cw(dir_of(world, e));
    let Some(facing) = Dir::from_bits(dir) else {
        return;
    };
    world.entity_mut(e).insert(ss13_shared::Facing(facing));
    if let Some(device) = world.get::<AtmosDevice>(e).copied() {
        world.resource_mut::<Pipenet>().remove(device.id);
        world.entity_mut(e).remove::<AtmosDevice>();
        super::join_built(world, e);
    }
}

/// The end of a weld.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: HfrJob) {
    match job {
        HfrJob::Repair => {
            let target = action.target;
            if let Some(mut p) = world.get_mut::<HfrPart>(target)
                && p.cracked
            {
                p.cracked = false;
                crate::balloon::balloon_alert(world, mob, target, "отремонтировано");
            }
        }
    }
}

/// `examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(part) = world.get::<HfrPart>(target) else {
        return;
    };
    let name = crate::examine::name_of(world, target);
    lines.push((
        Span::Notice,
        format!("{name} можно повернуть, открыв панель отвёрткой и затем используя гаечный ключ."),
    ));
    if part.cracked {
        lines.push((
            Span::Warning,
            "Корпус треснул: его можно заварить.".to_owned(),
        ));
    }
}

// --- The reaction --------------------------------------------------------------------------

/// The port mixture of a pipe machine.
fn port(
    net: &Pipenet,
    world: &World,
    e: Entity,
) -> Option<(ss13_atmos::pipenet::DeviceId, PipeAir)> {
    let id = world.get::<AtmosDevice>(e)?.id;
    Some((id, net.air(id, 0)?.clone()))
}

fn process_cores(world: &mut World) {
    let mut q = world.query::<(Entity, &HfrCore)>();
    let cores: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.links.is_some())
        .map(|(e, _)| e)
        .collect();
    for core in cores {
        process_core(world, core);
    }
}

/// `process_atmos` of a linked core.
fn process_core(world: &mut World, core: Entity) {
    let stored = world.get::<HfrCore>(core).and_then(|c| c.links);
    let Some((links, _)) = connectivity(world, core).filter(|(l, _)| Some(*l) == stored) else {
        deactivate(world, core);
        return;
    };
    let Some(&pos) = world.get::<TilePos>(core) else {
        return;
    };
    let ports = {
        let net = world.resource::<Pipenet>();
        [core, links.input, links.moderator, links.output].map(|e| port(net, world, e))
    };
    let [Some(cooling), Some(fuel), Some(moderator), Some(output)] = ports else {
        return;
    };
    let Some(mut state) = world.get::<HfrCore>(core).cloned() else {
        return;
    };
    let powered = world.get::<Unpowered>(core).is_none()
        && !world
            .get::<crate::obj_damage::Integrity>(core)
            .is_some_and(|i| i.broken);
    let cracked = [links.input, links.output, links.moderator]
        .into_iter()
        .any(|e| world.get::<HfrPart>(e).is_some_and(|p| p.cracked));
    let ran = state.reactor.start_power || state.reactor.power_level > 0;
    if state.reactor.start_power {
        // `update_mode_power_usage(ACTIVE_POWER_USE, (power_level + 1) * MIN_POWER_USAGE)`.
        let watts = (f64::from(state.reactor.power_level) + 1.0) * fusion::MIN_POWER_USAGE;
        crate::power::machine_use_energy(world, core, watts * FIRE_INTERVAL, false);
    }
    let (mut c, mut f, mut m, mut o) = (cooling.1, fuel.1, moderator.1, output.1);
    let mut rng = std::mem::take(&mut world.resource_mut::<crate::power::EngineRng>().0);
    let effects = state.reactor.process(
        FIRE_INTERVAL,
        powered,
        cracked,
        &mut Ports {
            cooling: &mut c,
            fuel: &mut f,
            moderator: &mut m,
            output: &mut o,
        },
        &mut |p| rng.chance(p as f32),
    );
    {
        let mut net = world.resource_mut::<Pipenet>();
        for (id, air) in [(cooling.0, c), (fuel.0, f), (moderator.0, m), (output.0, o)] {
            if let Some(slot) = net.air_mut(id, 0) {
                *slot = air;
            }
            net.update_parents(id);
        }
    }
    state.apc_percent = apc_percent(world, pos);
    let level = state.reactor.power_level;
    let overflow = effects.overflow;
    if let Some(mut live) = world.get_mut::<HfrCore>(core) {
        *live = state;
    }
    apply_effects(world, core, pos, &effects, &mut rng);
    if let Some(overflow) = overflow {
        moderator_overflow(world, core, links, overflow, &mut rng);
    }
    world.resource_mut::<crate::power::EngineRng>().0 = rng;
    if ran {
        check_alert(world, core, pos);
    }
    // `check_deconstructable`.
    let corners = world
        .get::<HfrCore>(core)
        .map(|c| c.corners.clone())
        .unwrap_or_default();
    for e in all_parts(core, links, &corners) {
        if let Some(mut p) = world.get_mut::<HfrPart>(e)
            && p.fusion_started != (level > 0)
        {
            p.fusion_started = level > 0;
        }
    }
}

/// `get_area_cell_percent`.
fn apc_percent(world: &World, pos: TilePos) -> f64 {
    world
        .resource::<ss13_mapload::Areas>()
        .area_at(pos)
        .and_then(|a| world.get::<crate::power::AreaPower>(a)?.apc)
        .and_then(|apc| world.get::<crate::power::Apc>(apc)?.cell)
        .map_or(0.0, |c| c.percent())
}

fn apply_effects(
    world: &mut World,
    core: Entity,
    pos: TilePos,
    effects: &fusion::Effects,
    rng: &mut crate::combat::CombatRng,
) {
    if let Some((radius, seconds)) = effects.hallucination {
        let seconds = seconds as f32;
        crate::hallucination::hallucination_pulse(world, pos, radius, seconds, seconds, &[]);
    }
    if let Some(zaps) = effects.zaps {
        use crate::power::supermatter::{
            ZAP_MOB_DAMAGE, ZAP_OBJ_DAMAGE, ZAP_SUPERMATTER_FLAGS, ZapArc, zap,
        };
        let flags = ZAP_SUPERMATTER_FLAGS
            | match zaps.power_level {
                5 => ZAP_MOB_DAMAGE,
                6 => ZAP_MOB_DAMAGE | ZAP_OBJ_DAMAGE,
                _ => 0,
            };
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, ZAP_SOUND, 100.0, true).extrarange(10.0),
        );
        for _ in 0..zaps.count {
            zap(
                world,
                rng,
                ZapArc {
                    from: core,
                    from_pos: pos,
                    range: 5,
                    energy: zaps.energy,
                    flags,
                    cutoff: zaps.cutoff,
                    power_level: f64::from(zaps.power_level) * 1000.0,
                },
                &mut Vec::new(),
                0,
            );
        }
    }
    if let Some(range) = effects.gravity_range {
        let mut q = world.query::<(Entity, &TilePos, &crate::health::Stat)>();
        let pulled: Vec<Entity> = q
            .iter(world)
            .filter(|(_, p, stat)| {
                **stat != crate::health::Stat::Dead && p.chebyshev(pos).is_some_and(|d| d <= range)
            })
            .map(|(e, ..)| e)
            .collect();
        for mob in pulled {
            crate::events::anomalies::step_towards(world, mob, pos);
        }
    }
    if effects.radiation {
        crate::radiation::radiation_pulse(world, core, 6.0, 0.3, RADIATION_CHANCE, 0.0);
    }
}

/// `spill_gases`: `ratio` of the moderator onto `part`'s tile.
fn spill(world: &mut World, core: Entity, part: Entity, ratio: f64) {
    let Some(&pos) = world.get::<TilePos>(part) else {
        return;
    };
    let Some(gas) = world
        .get_mut::<HfrCore>(core)
        .map(|mut c| c.reactor.moderator.remove_ratio(ratio))
    else {
        return;
    };
    world
        .resource_mut::<ss13_atmos::Atmos>()
        .merge_air(pos, &gas);
}

/// `process_moderator_overflow`'s effects.
fn moderator_overflow(
    world: &mut World,
    core: Entity,
    links: Links,
    overflow: Overflow,
    rng: &mut crate::combat::CombatRng,
) {
    let parts = [links.input, links.output, links.moderator];
    match overflow {
        Overflow::Leak(ratio) => {
            if let Some(cracked) = parts
                .into_iter()
                .find(|&e| world.get::<HfrPart>(e).is_some_and(|p| p.cracked))
            {
                spill(world, core, cracked, ratio);
            }
        }
        Overflow::Crack { explosion } => {
            let part = parts[rng.range(0, 2) as usize];
            if let Some(mut p) = world.get_mut::<HfrPart>(part) {
                p.cracked = true;
            }
            let Some(strong) = explosion else {
                return;
            };
            let Some(&pos) = world.get::<TilePos>(part) else {
                return;
            };
            let (ex, initial) = if strong {
                (
                    crate::explosions::Explosion::new(0, 1, 3).flame(5).flash(5),
                    fusion::STRONG_SPILL_INITIAL,
                )
            } else {
                (
                    crate::explosions::Explosion::new(0, 0, 1).flame(3).flash(3),
                    fusion::MEDIUM_SPILL_INITIAL,
                )
            };
            crate::explosions::explosion(world, pos, ex);
            spill(world, core, part, initial);
        }
    }
}

fn talk(world: &mut World, core: Entity, pos: TilePos, freq: u16, text: String) {
    let name = crate::examine::name_of(world, core);
    info!(target: "engine", "{name} ({freq}): {text}");
    world.write_message(crate::radio::MachineRadio {
        freq,
        z: pos.z,
        name,
        text,
    });
}

/// `alarm()`: the sound of the integrity band.
fn alarm(world: &mut World, pos: TilePos, integrity: f64) {
    use crate::sound::PlaySound;
    let sound = if integrity < 5.0 {
        PlaySound::new(pos, "sound/announcer/alarm/bloblarm.ogg", 100.0, false).extrarange(40.0)
    } else if integrity < 25.0 {
        PlaySound::new(
            pos,
            "sound/machines/engine_alert/engine_alert1.ogg",
            100.0,
            false,
        )
        .extrarange(30.0)
    } else if integrity < 50.0 {
        PlaySound::new(
            pos,
            "sound/machines/engine_alert/engine_alert2.ogg",
            100.0,
            false,
        )
        .extrarange(30.0)
    } else if integrity < 100.0 {
        PlaySound::new(
            pos,
            "sound/machines/terminal/terminal_alert.ogg",
            75.0,
            false,
        )
    } else {
        return;
    };
    let sound = if integrity < 50.0 {
        sound.falloff_exponent(30.0).falloff_distance(10.0)
    } else {
        sound
    };
    crate::sound::playsound(world, sound);
}

/// `check_alert`: warnings every minute while damaged (half a minute while it grows), the
/// emergency on common past 700, the countdown past the melting point.
fn check_alert(world: &mut World, core: Entity, pos: TilePos) {
    let Some(state) = world.get::<HfrCore>(core).cloned() else {
        return;
    };
    let r = &state.reactor;
    let ctp = r.critical_threshold_proximity;
    if ctp < WARNING_POINT {
        return;
    }
    let now = crate::tools::now(world);
    let integrity = crate::power::supermatter::dm_number(r.integrity_percent());
    if now - state.lastwarning >= WARNING_TIME_DELAY {
        alarm(world, pos, r.integrity_percent());
        let (freq, text, lastwarning) = if ctp > EMERGENCY_POINT {
            if !state.has_reached_emergency {
                warn!("hfr: {core:?} reached the emergency point for the first time");
            }
            (
                FREQ_COMMON,
                format!("{EMERGENCY_ALERT} Целостность: {integrity}%"),
                now,
            )
        } else if ctp >= r.critical_threshold_proximity_archived {
            (
                FREQ_ENGINEERING,
                format!("{WARNING_ALERT} Целостность: {integrity}%"),
                now - WARNING_TIME_DELAY / 2.0,
            )
        } else {
            (
                FREQ_ENGINEERING,
                format!("{SAFE_ALERT} Целостность: {integrity}%"),
                now,
            )
        };
        let explain = text.starts_with(EMERGENCY_ALERT) || text.starts_with(WARNING_ALERT);
        talk(world, core, pos, freq, text);
        if let Some(mut c) = world.get_mut::<HfrCore>(core) {
            c.lastwarning = lastwarning;
            c.has_reached_emergency |= ctp > EMERGENCY_POINT;
        }
        if explain {
            send_radio_explanation(world, core, pos, r.warning_damage_flags);
        }
    }
    if ctp > MELTING_POINT && state.countdown.is_none() {
        start_countdown(world, core, pos);
    }
}

/// `send_radio_explanation`.
fn send_radio_explanation(world: &mut World, core: Entity, pos: TilePos, flags: u8) {
    let lines = [
        (
            warning::HIGH_POWER_DAMAGE,
            "Внимание! Щит дестабилизируется из-за избыточной мощности!",
        ),
        (
            warning::IRON_CONTENT_DAMAGE,
            "Внимание! Осколки железа повреждают внутреннюю защиту ядра!",
        ),
        (
            warning::HIGH_FUEL_MIX_MOLE,
            "Внимание! Количество молей топливной смеси достигает критического уровня!",
        ),
        (
            warning::IRON_CONTENT_INCREASE,
            "Внимание! Количество железа внутри ядра растёт!",
        ),
    ];
    for (flag, text) in lines {
        if flags & flag != 0 {
            talk(world, core, pos, FREQ_ENGINEERING, text.to_owned());
        }
    }
}

/// `countdown()` begins.
fn start_countdown(world: &mut World, core: Entity, pos: TilePos) {
    let critical = world
        .get::<HfrCore>(core)
        .and_then(|c| c.reactor.fuel())
        .is_some_and(|f| f.meltdown_flags & fusion::meltdown::CRITICAL_MELTDOWN != 0);
    if critical {
        crate::events::priority_announce_sound(
            world,
            "Внимание",
            "ПРЕДУПРЕЖДЕНИЕ - Взрыв, скорее всего, накроет большую часть станции, а грядущее ЭМИ уничтожит большую часть электроники. Отойдите как можно дальше от реактора или найдите способ его отключить.",
            "sound/announcer/notice/notice3.ogg",
        );
    }
    talk(
        world,
        core,
        pos,
        FREQ_COMMON,
        format!(
            "{EMERGENCY_ALERT} Термоядерный реактор \"Гиперторус\" достиг критического нарушения целостности. Аварийные магнитные демпферы включены."
        ),
    );
    let now = crate::tools::now(world);
    if let Some(mut c) = world.get_mut::<HfrCore>(core) {
        c.countdown = Some((COUNTDOWN_DS, now));
    }
}

/// One second of the countdown: called off while the damage fell under the melting point,
/// a message every 5 s and each of the last 5, then the meltdown.
fn countdown_tick(world: &mut World) {
    let mut q = world.query::<(Entity, &HfrCore, &TilePos)>();
    let now = crate::tools::now(world);
    let due: Vec<(Entity, TilePos, i32)> = q
        .iter(world)
        .filter_map(|(e, c, &p)| {
            let (left, at) = c.countdown?;
            (now >= at).then_some((e, p, left))
        })
        .collect();
    for (core, pos, left) in due {
        let Some(state) = world.get::<HfrCore>(core).cloned() else {
            continue;
        };
        if left < 0 {
            meltdown(world, core, pos);
            continue;
        }
        if state.reactor.critical_threshold_proximity < MELTING_POINT {
            talk(
                world,
                core,
                pos,
                FREQ_COMMON,
                format!("{SAFE_ALERT} Отказоустойчивость отключена."),
            );
            if let Some(mut c) = world.get_mut::<HfrCore>(core) {
                c.countdown = None;
            }
            continue;
        }
        let critical = state
            .reactor
            .fuel()
            .is_some_and(|f| f.meltdown_flags & fusion::meltdown::CRITICAL_MELTDOWN != 0);
        if left % 50 == 0 || left <= 50 {
            if left == 100 && critical {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::to_playing_players(CRITICAL_SOUND, 100.0, false),
                );
            }
            let text = if left > 50 {
                format!(
                    "До полного нарушения целостности осталось {} секунд.",
                    left / 10
                )
            } else {
                format!("{}...", left / 10)
            };
            talk(world, core, pos, FREQ_COMMON, text);
        }
        if let Some(mut c) = world.get_mut::<HfrCore>(core) {
            c.countdown = Some((left - 10, now + 1.0));
        }
    }
}

/// `meltdown()`: gas pockets around, the rest on the core's tile, the explosion, radiation and
/// EMP of the fuel's flags; the core and its parts are gone (`Destroy`).
pub fn meltdown(world: &mut World, core: Entity, pos: TilePos) {
    let Some(state) = world.get::<HfrCore>(core).cloned() else {
        return;
    };
    let mut reactor = state.reactor;
    let flags = reactor.fuel().map_or(0, |f| f.meltdown_flags);
    let m = fusion::meltdown(flags, reactor.power_level);
    let around: Vec<TilePos> = {
        let atmos = world.resource::<ss13_atmos::Atmos>();
        let r = m.gas_spread;
        let mut out = Vec::new();
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                let space = crate::construction::turf_at(world, tile)
                    .and_then(|t| path_of(world, t))
                    .is_some_and(|p| is_under(&p, "/turf/open/space"));
                if atmos.air(tile).is_some() && !space {
                    out.push(tile);
                }
            }
        }
        out
    };
    let mut rng = std::mem::take(&mut world.resource_mut::<crate::power::EngineRng>().0);
    for mix in [&mut reactor.fusion, &mut reactor.moderator] {
        if mix.total_moles() <= 0.0 {
            continue;
        }
        let mut pockets = mix.remove_ratio(0.2);
        let mut atmos = world.resource_mut::<ss13_atmos::Atmos>();
        for _ in 0..m.gas_pockets {
            let pocket = pockets.remove_ratio(1.0 / f64::from(m.gas_pockets));
            if !around.is_empty() {
                let tile = around[rng.range(0, around.len() as u32 - 1) as usize];
                atmos.merge_air(tile, &pocket);
            }
        }
        atmos.merge_air(pos, &mix.gas);
    }
    world.resource_mut::<crate::power::EngineRng>().0 = rng;
    let mut ex = crate::explosions::Explosion::new(m.devastation, m.heavy, m.light)
        .flash(m.flash)
        .ignorecap();
    ex.cause = "hypertorus meltdown".into();
    ex.source = Some(core);
    warn!("hfr: {core:?} melted down at {pos:?}");
    if let Some(range) = m.radiation_range {
        crate::radiation::radiation_pulse(world, core, range as f32, 0.05, RADIATION_CHANCE, 0.0);
    }
    let parts = state
        .links
        .map(|l| all_parts(core, l, &state.corners))
        .unwrap_or_else(|| vec![core]);
    crate::explosions::explosion(world, pos, ex);
    if let Some((heavy, light)) = m.emp {
        crate::emp::empulse(
            world,
            crate::emp::Empulse {
                epicenter: pos,
                heavy,
                light,
            },
        );
    }
    for e in parts {
        remove_machine(world, e);
    }
}

/// A machine gone: out of the pipes and the area's power, then despawned.
fn remove_machine(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() {
        return;
    }
    if let Some(device) = world.get::<AtmosDevice>(e).copied() {
        world.resource_mut::<Pipenet>().remove(device.id);
    }
    crate::power::unregister_machine(world, e);
    world.despawn(e);
}

// --- Looks ---------------------------------------------------------------------------------

/// `update_icon_state` and `update_overlays`: the open panel, the crack, the active glow.
fn hfr_looks(
    icons: Option<Res<IconIndex>>,
    mut parts: Query<(&HfrPart, &DmAtom, &mut Appearance), Changed<HfrPart>>,
) {
    let Some(icons) = icons else { return };
    for (part, atom, mut appearance) in &mut parts {
        let base = part.kind.base();
        let state = if part.panel_open {
            format!("{base}_open")
        } else {
            base.to_owned()
        };
        if let Some(s) = icons.get(&atom.icon, &state)
            && appearance.icon_state != s
        {
            appearance.icon_state = s;
        }
        let mut overlays = Vec::new();
        if part.cracked {
            overlays.extend(icons.get(&atom.icon, "crack").map(Overlay::new));
        }
        if part.active {
            let active = format!("{base}_active");
            if let Some(s) = icons.get(&atom.icon, &active) {
                overlays.push(Overlay::new(s));
                overlays.push(Overlay {
                    plane: Some(EMISSIVE_PLANE),
                    ..Overlay::new(s)
                });
            }
        }
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
}

// --- The interface's window ----------------------------------------------------------------

pub struct HfrWindow;

impl UiInterface for HfrWindow {
    const NAME: &'static str = HypertorusUi::NAME;
    type Data = HypertorusData;
    type StaticData = HypertorusStatic;
}

#[derive(Clone, Debug, PartialEq)]
pub enum HfrAct {
    StartPower,
    StartCooling,
    StartFuel,
    StartModerator,
    WasteRemove,
    /// A knob and its value.
    Set(Knob, f64),
    /// A gas id toggled in the moderator filter.
    Filter(String),
    /// A fuel id, `""` for none.
    Fuel(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knob {
    HeatingConductor,
    MagneticConstrictor,
    FuelInjectionRate,
    ModeratorInjectionRate,
    CurrentDamper,
    ModFilteringRate,
    CoolingVolume,
}

impl Knob {
    const ALL: [(&'static str, Self); 7] = [
        ("heating_conductor", Self::HeatingConductor),
        ("magnetic_constrictor", Self::MagneticConstrictor),
        ("fuel_injection_rate", Self::FuelInjectionRate),
        ("moderator_injection_rate", Self::ModeratorInjectionRate),
        ("current_damper", Self::CurrentDamper),
        ("mod_filtering_rate", Self::ModFilteringRate),
        ("cooling_volume", Self::CoolingVolume),
    ];

    /// The `clamp` of `ui_act`.
    fn range(self) -> (f64, f64) {
        match self {
            Self::HeatingConductor => (50.0, 500.0),
            Self::MagneticConstrictor => (50.0, 1000.0),
            Self::FuelInjectionRate | Self::ModeratorInjectionRate => (0.5, 150.0),
            Self::CurrentDamper => (0.0, 1000.0),
            Self::ModFilteringRate => (5.0, 200.0),
            Self::CoolingVolume => (50.0, 2000.0),
        }
    }
}

type WindowParam = (
    Query<'static, 'static, &'static HfrCore>,
    Query<'static, 'static, &'static AtmosDevice>,
    Res<'static, Pipenet>,
);

fn gas_entry(g: GasId, amount: f64) -> HfrGas {
    let info = g.info();
    HfrGas {
        id: info.id.to_owned(),
        name: info.name.to_owned(),
        amount: ss13_atmos::gas::byond_round(amount, 0.01) as f32,
    }
}

/// `ui_data`.
pub fn window_data(core: &HfrCore, cooling_volume: f64) -> HypertorusData {
    let r = &core.reactor;
    let mut fusion_gases: Vec<GasId> = vec![GasId::ANTINOBLIUM];
    if let Some(f) = r.fuel() {
        fusion_gases.extend(f.requirements);
        fusion_gases.extend(f.primary_products.iter().copied());
    }
    for g in 0..GAS_COUNT as u8 {
        if r.fusion.gas.get(GasId(g)) > 0.0 && !fusion_gases.contains(&GasId(g)) {
            fusion_gases.push(GasId(g));
        }
    }
    HypertorusData {
        selected: r.fuel().map_or_else(String::new, |f| f.id.to_owned()),
        fusion_gases: fusion_gases
            .into_iter()
            .map(|g| gas_entry(g, r.fusion.gas.get(g)))
            .collect(),
        moderator_gases: (0..GAS_COUNT as u8)
            .map(|g| gas_entry(GasId(g), r.moderator.gas.get(GasId(g))))
            .collect(),
        energy_level: r.energy,
        heat_limiter_modifier: r.heat_limiter_modifier,
        heat_output_min: r.heat_output_min,
        heat_output_max: r.heat_output_max,
        heat_output: r.heat_output,
        instability: r.instability as f32,
        heating_conductor: r.heating_conductor as f32,
        magnetic_constrictor: r.magnetic_constrictor as f32,
        fuel_injection_rate: r.fuel_injection_rate as f32,
        moderator_injection_rate: r.moderator_injection_rate as f32,
        current_damper: r.current_damper as f32,
        power_level: r.power_level,
        apc_energy: core.apc_percent as f32,
        iron_content: r.iron_content as f32,
        integrity: r.integrity_percent() as f32,
        start_power: r.start_power,
        start_cooling: r.start_cooling,
        start_fuel: r.start_fuel,
        start_moderator: r.start_moderator,
        internal_fusion_temperature: r.fusion_temperature,
        moderator_internal_temperature: r.moderator_temperature,
        internal_output_temperature: r.output_temperature,
        internal_coolant_temperature: r.coolant_temperature,
        waste_remove: r.waste_remove,
        filter_types: (0..GAS_COUNT as u8)
            .map(|g| {
                let info = GasId(g).info();
                HfrFilter {
                    gas_id: info.id.to_owned(),
                    gas_name: info.name.to_owned(),
                    enabled: r.moderator_scrubbing & (1 << g) != 0,
                }
            })
            .collect(),
        cooling_volume: cooling_volume as f32,
        mod_filtering_rate: r.moderator_filtering_rate as f32,
    }
}

impl ServerUi for HfrWindow {
    type Source = &'static HfrInterface;
    type UserParam = WindowParam;
    type Act = HfrAct;

    fn static_data(_: &HfrInterface) -> HypertorusStatic {
        let ids = |gases: &[GasId]| -> Vec<String> {
            gases.iter().map(|g| g.info().id.to_owned()).collect()
        };
        let mut fuels = vec![HfrFuelEntry {
            name: "Nothing".into(),
            ..default()
        }];
        fuels.extend(FUELS.iter().map(|f| HfrFuelEntry {
            name: f.name.into(),
            id: f.id.into(),
            requirements: ids(&f.requirements),
            fusion_byproducts: ids(f.primary_products),
            product_gases: ids(&f.secondary_products),
            recipe_cooling_multiplier: f.negative_temperature_multiplier as f32,
            recipe_heating_multiplier: f.positive_temperature_multiplier as f32,
            energy_loss_multiplier: f.energy_concentration_multiplier as f32,
            fuel_consumption_multiplier: f.fuel_consumption_multiplier as f32,
            gas_production_multiplier: f.gas_production_multiplier as f32,
            temperature_multiplier: f.temperature_change_multiplier as f32,
        }));
        HypertorusStatic {
            base_max_temperature: FUSION_MAXIMUM_TEMPERATURE as f32,
            selectable_fuel: fuels,
        }
    }

    fn data(
        interface: &HfrInterface,
        _: Entity,
        (cores, devices, net): &SystemParamItem<'_, '_, WindowParam>,
    ) -> HypertorusData {
        let Some((core, e)) = interface.core.and_then(|c| Some((cores.get(c).ok()?, c))) else {
            return HypertorusData::default();
        };
        let volume = devices
            .get(e)
            .ok()
            .and_then(|d| net.air(d.id, 0))
            .map_or(0.0, |a| a.volume);
        window_data(core, volume)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<HfrAct, UiReject> {
        if let Some(&(_, knob)) = Knob::ALL.iter().find(|(name, _)| *name == action) {
            return Ok(HfrAct::Set(knob, params.number(action)?));
        }
        Ok(match action {
            "start_power" => HfrAct::StartPower,
            "start_cooling" => HfrAct::StartCooling,
            "start_fuel" => HfrAct::StartFuel,
            "start_moderator" => HfrAct::StartModerator,
            "waste_remove" => HfrAct::WasteRemove,
            "filter" => HfrAct::Filter(params.text("mode")?),
            "fuel" => HfrAct::Fuel(params.text("mode")?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `ui_act`, at the moment of effect. The window's own disabled buttons are checked here too:
/// power only switches at fusion level 0, cooling with power and without injection, injection
/// with power and cooling, the recipe at level 0.
fn window_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<HfrWindow>>>) {
    let pending: Vec<(Entity, HfrAct)> = acts
        .read(world.resource::<Messages<UiActed<HfrWindow>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (interface, act) in pending {
        if !world.get::<HfrPart>(interface).is_some_and(|p| p.active) {
            continue;
        }
        let Some(core) = world.get::<HfrInterface>(interface).and_then(|i| i.core) else {
            continue;
        };
        let Some(links) = world.get::<HfrCore>(core).and_then(|c| c.links) else {
            continue;
        };
        apply_act(world, core, links, act);
    }
}

fn apply_act(world: &mut World, core: Entity, links: Links, act: HfrAct) {
    let cooling_id = world.get::<AtmosDevice>(core).map(|d| d.id);
    let output_id = world.get::<AtmosDevice>(links.output).map(|d| d.id);
    let Some(mut state) = world.get::<HfrCore>(core).cloned() else {
        return;
    };
    let r = &mut state.reactor;
    let level = r.power_level;
    match act {
        HfrAct::StartPower if level == 0 => {
            r.start_power = !r.start_power;
        }
        HfrAct::StartCooling
            if r.start_power
                && !r.start_fuel
                && !r.start_moderator
                && !(r.start_cooling && level > 0) =>
        {
            r.start_cooling = !r.start_cooling;
        }
        HfrAct::StartFuel if r.start_power && r.start_cooling => r.start_fuel = !r.start_fuel,
        HfrAct::StartModerator if r.start_power && r.start_cooling => {
            r.start_moderator = !r.start_moderator;
        }
        HfrAct::WasteRemove => r.waste_remove = !r.waste_remove,
        HfrAct::Set(knob, value) if value.is_finite() => {
            let (min, max) = knob.range();
            let value = value.clamp(min, max);
            match knob {
                Knob::HeatingConductor => r.heating_conductor = value,
                Knob::MagneticConstrictor => r.magnetic_constrictor = value,
                Knob::FuelInjectionRate => r.fuel_injection_rate = value,
                Knob::ModeratorInjectionRate => r.moderator_injection_rate = value,
                Knob::CurrentDamper => r.current_damper = value,
                Knob::ModFilteringRate => r.moderator_filtering_rate = value,
                Knob::CoolingVolume => {
                    if let Some(id) = cooling_id {
                        let mut net = world.resource_mut::<Pipenet>();
                        if let Some(air) = net.air_mut(id, 0) {
                            air.volume = value;
                        }
                        net.update_parents(id);
                    }
                }
            }
        }
        HfrAct::Filter(id) => {
            if let Some(g) = GasId::parse(&id) {
                r.moderator_scrubbing ^= 1 << g.0;
            }
        }
        HfrAct::Fuel(id) if level == 0 => {
            let Some(output_id) = output_id else { return };
            let mut net = world.resource_mut::<Pipenet>();
            let Some(output) = net.air_mut(output_id, 0) else {
                return;
            };
            r.select_fuel(fusion::fuel(&id), output);
            net.update_parents(output_id);
        }
        _ => return,
    }
    if let Some(mut live) = world.get_mut::<HfrCore>(core) {
        *live = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_boxes_follow_the_paths() {
        assert_eq!(HfrKind::of_path(CORE), Some(HfrKind::Core));
        assert_eq!(HfrKind::of_path(CORNER), Some(HfrKind::Corner));
        assert_eq!(box_part(BOX_CORE), Some(None));
        assert_eq!(
            box_part("/obj/item/hfr_box/body/waste_output"),
            Some(Some(WASTE_OUTPUT))
        );
        assert_eq!(box_part("/obj/item/hfr_box/body"), None);
        assert_eq!(box_part("/obj/item/pen"), None);
    }

    #[test]
    fn knobs_clamp_like_tg() {
        assert_eq!(Knob::CurrentDamper.range(), (0.0, 1000.0));
        assert_eq!(Knob::CoolingVolume.range(), (50.0, 2000.0));
    }
}
