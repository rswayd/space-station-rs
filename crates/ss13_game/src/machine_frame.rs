//! Machine and computer frames, circuit boards, and taking machines apart (server), after tg
//! `code/game/machinery/constructable_frame.dm`, `machine_frame.dm`,
//! `computer/buildandrepair.dm` and `_machinery.dm` (`default_deconstruction_screwdriver`,
//! `default_deconstruction_crowbar`, `handle_deconstruct`, `spawn_frame`).
//!
//! - Frames: 5 iron sheets right-clicked on a floor with a circuit board in the other hand
//!   (`sheet/iron/interact_with_atom_secondary`, 2.5 s). A machine frame takes 5 cable, a
//!   machine board, then its `req_components` (parts and stacks); a screwdriver completes it. A
//!   computer frame takes a computer board (screwdriver secures it), 5 cable and 2 glass; a
//!   screwdriver completes it. Every step comes undone with the reverse tool.
//! - Machines of the [covered](family) types: a screwdriver opens the panel and a crowbar with
//!   the panel open takes the machine apart into a wired frame, its board, its parts and the
//!   stacks its board needs; a screwdriver on a computer (2 s) leaves a glassed computer frame
//!   with the board inside. Destroyed, a machine leaves a half-broken frame, a computer a wired
//!   frame and two shards (`deconstruct(FALSE)`).
//! - A finished frame spawns the board's `build_path` as the map would and gives it the setup
//!   map load gives its family ([`setup_built`]): power, the family's live state, the pipe graph
//!   for thermomachines.
//! - Boards and parts are the items put in ([`crate::stock_parts::PartOf`]): taking a machine or
//!   frame apart gives the same items back. A built machine runs `RefreshParts` on its parts
//!   ([`crate::stock_parts::refresh`]); an RPED exchanges them or fills a frame.
// shortcut: a board keeps no instance state but a vendor board's brand
// (`crate::vending::refill::VendorBoard`; not a thermomachine board's layer), no panel sprites,
// no stack recipe menu (the board-in-hand right-click is tg's own shortcut for frames).
// shortcut: only the families in `FAMILIES` can be taken apart or built; other boards are
// refused by frames.

use bevy::prelude::*;
use ss13_mapload::{CircuitPart, DmCircuit, DmItem, DmMachine, DmName, DmObj, DmStack};
use ss13_proto::Prototypes;
use ss13_shared::chat::Span;
use ss13_shared::{Facing, TilePos};
use ss13_world::{Mover, SpatialIndex};

use crate::construction::{Job, PanelOpen, spawn_obj, spawn_stack, tile_changed, turf_at};
use crate::obj_damage::{Integrity, ObjState, is_under, set_anchored, set_icon_state};
use crate::stock_parts::{ComponentParts, attach, detach};
use crate::storage::{Stored, take_out};
use crate::tools::{Actor, DoAfter, Stack, ToolKind, tell, tool_of, use_tool};

pub use ss13_mapload::circuit::{CABLE_COIL, COMPUTER_FRAME, MACHINE_FRAME};

const FRAME_ROOT: &str = "/obj/structure/frame";
const BOARD_ROOT: &str = ss13_mapload::circuit::BOARD_ROOT;
const MACHINE_BOARD_ROOT: &str = ss13_mapload::circuit::MACHINE_BOARD_ROOT;
const COMPUTER_ROOT: &str = "/obj/machinery/computer";
const IRON: &str = "/obj/item/stack/sheet/iron";
const GLASS: &str = "/obj/item/stack/sheet/glass";
const SHARD: &str = "/obj/item/shard";
const FLOOR: &str = "/turf/open/floor";
const THERMOMACHINE: &str = "/obj/machinery/atmospherics/components/unary/thermomachine";
const RECHARGER: &str = crate::power::chargers::RECHARGER;
const CELL_CHARGER: &str = crate::power::chargers::CELL_CHARGER;

/// `FRAME_STATE_*` (machine frames).
pub const FRAME_EMPTY: i32 = 0;
pub const FRAME_WIRED: i32 = 1;
pub const FRAME_BOARD: i32 = 2;
/// `FRAME_COMPUTER_STATE_*`.
pub const COMPUTER_BOARD: i32 = 1;
pub const COMPUTER_SECURED: i32 = 2;
pub const COMPUTER_WIRED: i32 = 3;
pub const COMPUTER_GLASSED: i32 = 4;

/// Iron sheets of a frame (`custom_materials`, the `metal_recipes` cost), cable it takes.
pub const FRAME_SHEETS: u32 = 5;
const FRAME_CABLE: u32 = 5;
const COMPUTER_GLASS: u32 = 2;
/// Deciseconds: the stack recipe, `default_unfasten_wrench`, `try_dissassemble` (screwdriver,
/// welder), cabling and glass, a computer's `time_to_unscrew`.
pub const BUILD_DS: f32 = 25.0;
const WRENCH_DS: f32 = 40.0;
const SCREW_APART_DS: f32 = 80.0;
const WELD_APART_DS: f32 = 20.0;
const CABLE_DS: f32 = 20.0;
const COMPUTER_UNSCREW_DS: f32 = 20.0;

/// The machine families frames build and tools take apart: their tg type roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Charger,
    Kitchen,
    Tray,
    Station,
    Analyzer,
    Thermo,
    RdConsole,
    CargoConsole,
    AtmosAlert,
    Autolathe,
    /// Vendors with a refill canister type (`crate::vending::refill`).
    Vending,
    /// Chem, soda and booze dispensers (`crate::chem_dispenser`).
    ChemDispenser,
    /// Smart fridges (`crate::smartfridge`).
    SmartFridge,
    /// ChemMasters and CondiMasters (`crate::chem_master`).
    ChemMaster,
    /// Reaction chambers (`crate::chem_heater`).
    ChemHeater,
    /// Destructive scanners (`crate::science::experi_scanner`).
    DestructiveScanner,
    /// R&D server control consoles (`crate::science::server_control`).
    ServerControl,
    /// P.A.C.M.A.N. generators (`crate::power::pacman`).
    Pacman,
    /// SMES (`crate::power::smes`).
    Smes,
    /// Shield wall generators (`crate::power::shields`).
    ShieldWallGen,
    /// Ore redemption machines (`crate::mining::orm`).
    Orm,
    /// Mail sorters (`crate::cargo::mail_sorter`).
    MailSorter,
    /// Materials markets (`crate::cargo::materials_market`).
    MaterialsMarket,
    /// BandaStation's teapot (`crate::teapot`).
    Teapot,
    /// Quantum pads (`crate::science::quantum_pad`: no state until used).
    QuantumPad,
    /// Teleporter hub, station and console; launchpads and their console; satellite control.
    Teleportation,
    /// Sheetifiers (`crate::eng_machines::sheetifier`).
    Sheetifier,
    /// Atmospheric shield generators (`crate::eng_machines::atmos_shield`).
    AtmosShieldGen,
    /// Flatpackers (`crate::eng_machines::flatpacker`).
    Flatpacker,
    /// Modular shield generators and gates (`crate::eng_machines::modular_shield`).
    ModularShieldGen,
    /// Modular shield nodes, cables, chargers, relays and wells.
    ShieldModule,
    /// Manufactorio machines (`crate::eng_machines::manufactorio`).
    Manufacturing,
    /// Smoke machines, HPLC machines and PANDEMICs (`crate::chem_machines`).
    ChemMachine,
    /// LTSRBT pads (`crate::cargo::black_market`).
    Ltsrbt,
    /// Power monitoring consoles (`crate::power::monitor`).
    PowerMonitor,
}

const FAMILIES: [(&str, Family); 50] = [
    (CELL_CHARGER, Family::Charger),
    (RECHARGER, Family::Charger),
    (crate::kitchen::MICROWAVE_ROOT, Family::Kitchen),
    (crate::kitchen::PROCESSOR_ROOT, Family::Kitchen),
    (crate::kitchen::GRINDER_ROOT, Family::Kitchen),
    (crate::cooking::OVEN_ROOT, Family::Kitchen),
    (crate::cooking::GRIDDLE_ROOT, Family::Kitchen),
    (crate::cooking::FRYER_ROOT, Family::Kitchen),
    (crate::gibber::GIBBER_ROOT, Family::Kitchen),
    (crate::coffee::COFFEEMAKER_ROOT, Family::Kitchen),
    (crate::stove::STOVE_ROOT, Family::Kitchen),
    (crate::chem_dispenser::DISPENSER_ROOT, Family::ChemDispenser),
    (crate::smartfridge::FRIDGE_ROOT, Family::SmartFridge),
    (crate::chem_master::MASTER_ROOT, Family::ChemMaster),
    (crate::chem_heater::HEATER_ROOT, Family::ChemHeater),
    ("/obj/machinery/hydroponics/constructable", Family::Tray),
    ("/obj/machinery/recharge_station", Family::Station),
    (crate::science::analyzer::ANALYZER_ROOT, Family::Analyzer),
    (THERMOMACHINE, Family::Thermo),
    (crate::science::console::CONSOLE_ROOT, Family::RdConsole),
    ("/obj/machinery/computer/cargo", Family::CargoConsole),
    (
        crate::atmos_machinery::consoles::ALERT_CONSOLE,
        Family::AtmosAlert,
    ),
    (crate::science::autolathe::AUTOLATHE_ROOT, Family::Autolathe),
    (crate::vending::VENDING_ROOT, Family::Vending),
    (
        crate::science::experi_scanner::DESTRUCTIVE_SCANNER,
        Family::DestructiveScanner,
    ),
    (
        crate::science::server_control::SERVER_CONTROL_ROOT,
        Family::ServerControl,
    ),
    (crate::power::pacman::PACMAN_ROOT, Family::Pacman),
    (ss13_mapload::power::SMES_ROOT, Family::Smes),
    (
        crate::power::shields::SHIELD_WALL_GEN,
        Family::ShieldWallGen,
    ),
    (ss13_mapload::mining::ORM_ROOT, Family::Orm),
    (ss13_mapload::cargo::MAIL_SORTER, Family::MailSorter),
    (
        ss13_mapload::cargo::MATERIALS_MARKET,
        Family::MaterialsMarket,
    ),
    (crate::teapot::TEAPOT, Family::Teapot),
    (crate::science::quantum_pad::PAD, Family::QuantumPad),
    ("/obj/machinery/teleport", Family::Teleportation),
    ("/obj/machinery/computer/teleporter", Family::Teleportation),
    ("/obj/machinery/launchpad", Family::Teleportation),
    ("/obj/machinery/computer/launchpad", Family::Teleportation),
    ("/obj/machinery/computer/sat_control", Family::Teleportation),
    (
        crate::eng_machines::sheetifier::SHEETIFIER,
        Family::Sheetifier,
    ),
    (
        crate::eng_machines::atmos_shield::GENERATOR,
        Family::AtmosShieldGen,
    ),
    (
        crate::eng_machines::flatpacker::FLATPACKER,
        Family::Flatpacker,
    ),
    (
        crate::eng_machines::modular_shield::GENERATOR,
        Family::ModularShieldGen,
    ),
    (
        crate::eng_machines::modular_shield::MODULE,
        Family::ShieldModule,
    ),
    (
        crate::eng_machines::manufactorio::ROOT,
        Family::Manufacturing,
    ),
    (
        crate::chem_machines::smoke_machine::SMOKE_MACHINE,
        Family::ChemMachine,
    ),
    (
        crate::chem_machines::mass_spec::MASS_SPEC,
        Family::ChemMachine,
    ),
    (
        crate::chem_machines::pandemic::PANDEMIC,
        Family::ChemMachine,
    ),
    (crate::cargo::black_market::LTSRBT, Family::Ltsrbt),
    (crate::power::monitor::MONITOR, Family::PowerMonitor),
];

/// The covered family of a machine type (`None`: not built or taken apart here).
pub fn family(path: &str) -> Option<Family> {
    if is_under(path, "/obj/machinery/computer/cargo/express") {
        // Its own console state is not ported.
        return None;
    }
    if is_under(
        path,
        crate::science::teleportation::launchpad::BRIEFCASE_PAD,
    ) {
        // A syndicate kit without a board.
        return None;
    }
    FAMILIES
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map(|&(_, f)| f)
}

/// A frame's board and what it still needs; its `state` is the frame's [`ObjState`], the board
/// and parts put in are its [`ComponentParts`].
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// The installed board's type.
    pub board: Option<String>,
    /// Machine frames: what is still needed (`req_components` counting down).
    pub needed: Vec<CircuitPart>,
}

/// A timed frame or machine step, with the state it started from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameJob {
    /// Iron sheets on a floor with a board in the other hand.
    Build { computer: bool },
    /// Wrench: `default_unfasten_wrench`.
    Anchor { anchored: bool },
    /// Screwdriver or welder on an empty, loose frame.
    Disassemble,
    /// Cable coil on a machine frame (empty) or a computer frame (board secured).
    Wire { state: i32 },
    /// Wirecutters on a wired machine frame.
    Unwire,
    /// Glass on a wired computer frame.
    Glass,
    /// Screwdriver on a computer: `deconstruct(TRUE)`.
    Unscrew,
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    crate::construction::path_of(world, e)
}

fn state(world: &World, e: Entity) -> i32 {
    world.get::<ObjState>(e).map_or(0, |s| s.0)
}

use crate::obj_damage::anchored;

fn board_def(world: &World, board: &str) -> Option<DmCircuit> {
    world
        .get_resource::<Prototypes>()?
        .get(board)?
        .get::<DmCircuit>()
        .cloned()
}

/// A prototype's name for messages (`singular_name` of stacks).
pub(crate) fn type_name(world: &World, path: &str) -> String {
    let proto = world.get_resource::<Prototypes>().and_then(|p| p.get(path));
    let singular = proto
        .and_then(|p| p.get::<DmStack>())
        .map(|s| s.singular_name.clone())
        .filter(|s| !s.is_empty());
    singular
        .or_else(|| {
            proto
                .and_then(|p| p.get::<DmName>())
                .map(|n| crate::obj_damage::strip(&n.name).to_owned())
        })
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).replace('_', " "))
}

fn item_path(world: &World, item: Entity) -> Option<String> {
    world.get::<DmItem>(item).map(|i| i.path.clone())
}

fn is_frame(world: &World, e: Entity) -> Option<bool> {
    let path = &world.get::<DmObj>(e)?.path;
    if is_under(path, COMPUTER_FRAME) {
        Some(true)
    } else if is_under(path, FRAME_ROOT) {
        Some(false)
    } else {
        None
    }
}

/// A machine of a covered family with a board: what crowbars, screwdrivers and destruction take
/// apart here.
pub fn buildable_machine(world: &World, e: Entity) -> Option<Family> {
    let path = &world.get::<DmObj>(e)?.path;
    let fam = family(path)?;
    // `if(!refill_canister) circuit = null`.
    if fam == Family::Vending && !crate::vending::refill::has_circuit(world, e) {
        return None;
    }
    let has_board = world
        .get::<ComponentParts>(e)
        .is_some_and(|p| !p.parts().is_empty())
        || world
            .get::<DmMachine>(e)
            .is_some_and(|m| board_def(world, &m.circuit).is_some());
    has_board.then_some(fam)
}

/// The board in `mob`'s hands other than `held`: which frame iron sheets build (computer:
/// true).
pub fn held_board_frame(world: &World, mob: Entity, held: Entity) -> Option<bool> {
    let inventory = world.get::<crate::inventory::Inventory>(mob)?;
    inventory
        .hands
        .iter()
        .flatten()
        .filter(|&&e| e != held)
        .find_map(|&e| {
            item_path(world, e)
                .filter(|p| is_under(p, BOARD_ROOT))
                .map(|p| !is_under(&p, MACHINE_BOARD_ROOT))
        })
}

pub(crate) fn panel_open(world: &World, e: Entity) -> bool {
    world.get::<PanelOpen>(e).is_some_and(|p| p.0)
        || world
            .get::<crate::atmos_machinery::devices::ThermoMachine>(e)
            .is_some_and(|m| m.panel_open)
        || world
            .get::<crate::power::pacman::Pacman>(e)
            .is_some_and(|p| p.panel_open)
}

/// The held item on a frame or a covered machine (tg `base_item_interaction` of frames, the
/// machines' `screwdriver_act`/`crowbar_act`). True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    let tool = if combat {
        None
    } else {
        tool_of(world, held).map(|t| t.0)
    };
    if !combat
        && crate::stock_parts::is_rped(world, held)
        && rped_interaction(world, client, held, target)
    {
        return true;
    }
    if let Some(computer) = is_frame(world, target) {
        if let Some(kind) = tool
            && frame_tool(world, actor, kind, target, computer)
        {
            return true;
        }
        if frame_item(world, actor, held, target, computer) {
            return true;
        }
        if computer || combat {
            return false;
        }
        add_part(world, actor, held, target);
        return true;
    }
    if let (Some(fam), Some(kind)) = (buildable_machine(world, target), tool) {
        return machine_tool(world, actor, kind, target, fam);
    }
    // An R&D lathe's `default_deconstruction_screwdriver` (for its RPED; no deconstruction).
    if tool == Some(ToolKind::Screwdriver)
        && world
            .get::<crate::science::lathe::Lathe>(target)
            .is_some_and(|l| !l.busy)
    {
        let open = panel_open(world, target);
        world.entity_mut(target).insert(PanelOpen(!open));
        let text = if open {
            "панель обслуживания закрыта"
        } else {
            "панель обслуживания открыта"
        };
        tell(world, actor.client, Span::Notice, text);
        return true;
    }
    false
}

/// An RPED on a machine (`exchange_parts`) or a machine frame (`install_circuit_from_part_replacer`,
/// `install_parts_from_part_replacer`). True when the click was its.
pub fn rped_interaction(world: &mut World, client: Entity, rped: Entity, target: Entity) -> bool {
    if let Some(computer) = is_frame(world, target) {
        if computer {
            return false;
        }
        match state(world, target) {
            FRAME_WIRED => {
                let Some(board) = crate::stock_parts::board_in(world, rped, false) else {
                    return false;
                };
                let path = item_path(world, board).unwrap_or_default();
                if install_board(world, client, board, &path, target, false) {
                    rped_fill(world, client, rped, target);
                }
                true
            }
            FRAME_BOARD => {
                rped_fill(world, client, rped, target);
                true
            }
            _ => false,
        }
    } else if world.get::<crate::science::lathe::Lathe>(target).is_some() {
        // Not built from frames here, but its parts upgrade like tg's.
        crate::stock_parts::exchange_parts(world, client, rped, target);
        true
    } else if let Some(fam) = buildable_machine(world, target) {
        if matches!(
            fam,
            Family::RdConsole
                | Family::CargoConsole
                | Family::AtmosAlert
                | Family::ServerControl
                | Family::PowerMonitor
        ) {
            // `istype(attacked_object, /obj/machinery/computer)`.
            return false;
        }
        if fam == Family::Vending {
            return crate::vending::refill::rped_restock(world, client, rped, target);
        }
        crate::stock_parts::exchange_parts(world, client, rped, target);
        true
    } else {
        false
    }
}

fn rped_fill(world: &mut World, client: Entity, rped: Entity, frame: Entity) {
    let Some(mut f) = world.get::<Frame>(frame).cloned() else {
        return;
    };
    if crate::stock_parts::install_into_frame(world, client, rped, frame, &mut f.needed) {
        world.entity_mut(frame).insert(f);
    }
}

/// `screwdriver_act`/`crowbar_act` of the covered machines.
fn machine_tool(
    world: &mut World,
    actor: Actor,
    kind: ToolKind,
    machine: Entity,
    fam: Family,
) -> bool {
    let client = actor.client;
    let path = path_of(world, machine).unwrap_or_default();
    let computer = is_under(&path, COMPUTER_ROOT);
    // `if(operating) balloon_alert("still operating!")` and the chargers' `charging` checks.
    let busy = world
        .get::<crate::kitchen::Grinder>(machine)
        .is_some_and(|g| g.ends_at.is_some())
        || world
            .get::<crate::science::autolathe::Autolathe>(machine)
            .is_some_and(|a| a.busy);
    match kind {
        ToolKind::Screwdriver if computer => {
            if use_tool(
                world,
                actor,
                machine,
                COMPUTER_UNSCREW_DS,
                0,
                Job::Frame(FrameJob::Unscrew),
            ) {
                tell(world, client, Span::Notice, "Вы отсоединяете монитор...");
            }
            true
        }
        // The thermomachine's own `screwdriver_act` (`atmos_machinery::thermo`), the vendors'
        // (`crate::construction`: only while anchored).
        // The generator's own (`power::pacman`: not while running).
        ToolKind::Screwdriver
            if matches!(
                fam,
                Family::Thermo | Family::Vending | Family::Pacman | Family::ShieldWallGen
            ) =>
        {
            false
        }
        // `shieldwallgen/crowbar_act`: `if(active) return ITEM_INTERACT_BLOCKING`.
        ToolKind::Crowbar if crate::power::shields::crowbar_blocked(world, machine) => true,
        // `smes/crowbar_act`: not with a terminal.
        ToolKind::Crowbar
            if fam == Family::Smes
                && crate::power::smes::crowbar_refused(world, client, machine) =>
        {
            true
        }
        // `port_gen/crowbar_act`: `if(active) return NONE`.
        ToolKind::Crowbar
            if world
                .get::<crate::power::pacman::Pacman>(machine)
                .is_some_and(|p| p.active) =>
        {
            false
        }
        ToolKind::Screwdriver => {
            if busy {
                tell(world, client, Span::Warning, "Машина ещё работает!");
                return true;
            }
            // `recharger`: `(!anchored || charging) ? ITEM_INTERACT_BLOCKING`; `cell_charger`:
            // `charging ? NONE`; `recharge_station`: `state_open ? ... : NONE`.
            if let Some(r) = world.get::<crate::power::chargers::Recharger>(machine)
                && (r.charging.is_some() || !anchored(world, machine))
            {
                return true;
            }
            let cell_busy = world
                .get::<crate::power::chargers::CellCharger>(machine)
                .is_some_and(|c| c.charging.is_some());
            let closed_station = world
                .get::<crate::silicon::cyborg::RechargeStation>(machine)
                .is_some_and(|s| !s.open);
            if cell_busy || closed_station {
                return false;
            }
            let open = panel_open(world, machine);
            world.entity_mut(machine).insert(PanelOpen(!open));
            let text = if open {
                "панель обслуживания закрыта"
            } else {
                "панель обслуживания открыта"
            };
            tell(world, client, Span::Notice, text);
            true
        }
        ToolKind::Crowbar if computer => false,
        ToolKind::Crowbar
            if fam == Family::Vending
                && crate::vending::custom::asks_before_crowbar(world, actor, machine) =>
        {
            true
        }
        ToolKind::Crowbar => {
            if busy {
                tell(world, client, Span::Warning, "Машина ещё работает!");
            } else if panel_open(world, machine) {
                crate::obj_damage::deconstruct(world, machine, true);
            }
            true
        }
        _ => false,
    }
}

fn wrong(world: &mut World, client: Entity, text: &str) -> bool {
    tell(world, client, Span::Warning, text);
    true
}

/// Tool acts of frames (`wrench_act`, `screwdriver_act`, `welder_act`, `wirecutter_act`,
/// `crowbar_act`).
fn frame_tool(
    world: &mut World,
    actor: Actor,
    kind: ToolKind,
    frame: Entity,
    computer: bool,
) -> bool {
    let client = actor.client;
    let st = state(world, frame);
    let anchored_now = anchored(world, frame);
    let job = |job| Job::Frame(job);
    match kind {
        ToolKind::Wrench => {
            let needs_anchor = !computer
                && world
                    .get::<Frame>(frame)
                    .and_then(|f| f.board.as_deref())
                    .and_then(|b| board_def(world, b))
                    .is_some_and(|b| b.needs_anchored);
            if needs_anchor {
                return wrong(world, client, "Рама должна быть закреплена!");
            }
            if !anchored_now && !on_floor(world, frame) {
                return wrong(world, client, "Здесь её не закрепить!");
            }
            let anchored = anchored_now;
            if use_tool(
                world,
                actor,
                frame,
                WRENCH_DS,
                0,
                job(FrameJob::Anchor { anchored }),
            ) {
                let text = if anchored {
                    "Вы начинаете откручивать раму..."
                } else {
                    "Вы начинаете прикручивать раму..."
                };
                tell(world, client, Span::Notice, text);
            }
            true
        }
        ToolKind::Screwdriver | ToolKind::Welder if st == FRAME_EMPTY => {
            // `try_dissassemble`.
            if anchored_now {
                return wrong(world, client, "Сначала открепите раму!");
            }
            let (delay, fuel) = if kind == ToolKind::Welder {
                (WELD_APART_DS, 1)
            } else {
                (SCREW_APART_DS, 0)
            };
            if use_tool(world, actor, frame, delay, fuel, job(FrameJob::Disassemble)) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете разбирать раму...",
                );
            }
            true
        }
        ToolKind::Screwdriver if !computer => {
            if st == FRAME_BOARD {
                finalize_machine(world, actor, frame);
                return true;
            }
            false
        }
        ToolKind::Screwdriver => {
            match st {
                COMPUTER_BOARD | COMPUTER_SECURED => {
                    let (to, text) = if st == COMPUTER_BOARD {
                        (COMPUTER_SECURED, "плата закреплена")
                    } else {
                        (COMPUTER_BOARD, "плата откреплена")
                    };
                    set_state(world, frame, to);
                    tell(world, client, Span::Notice, text);
                }
                COMPUTER_WIRED => {
                    tell(world, client, Span::Warning, "Нет стекла!");
                }
                COMPUTER_GLASSED => finalize_computer(world, actor, frame),
                _ => return false,
            }
            true
        }
        ToolKind::Wirecutters if !computer => {
            if st == FRAME_WIRED
                && use_tool(world, actor, frame, CABLE_DS, 0, job(FrameJob::Unwire))
            {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете снимать кабели...",
                );
            }
            true
        }
        ToolKind::Wirecutters => {
            if st == COMPUTER_WIRED {
                set_state(world, frame, COMPUTER_SECURED);
                drop_stack(world, frame, CABLE_COIL, FRAME_CABLE);
                tell(world, client, Span::Notice, "кабели сняты");
            }
            true
        }
        ToolKind::Crowbar if !computer => {
            if st == FRAME_BOARD {
                let had_parts = world
                    .get::<ComponentParts>(frame)
                    .is_some_and(|p| p.parts().len() > 1);
                dump_frame(world, frame);
                let text = if had_parts {
                    "плата и детали извлечены"
                } else {
                    "плата извлечена"
                };
                tell(world, client, Span::Notice, text);
            }
            true
        }
        ToolKind::Crowbar => {
            match st {
                COMPUTER_BOARD => {
                    dump_frame(world, frame);
                    tell(world, client, Span::Notice, "плата извлечена");
                }
                COMPUTER_SECURED => {
                    tell(world, client, Span::Warning, "Сначала открутите плату!");
                }
                COMPUTER_WIRED => {
                    tell(world, client, Span::Warning, "Сначала уберите проводку!");
                }
                COMPUTER_GLASSED => {
                    set_state(world, frame, COMPUTER_WIRED);
                    drop_stack(world, frame, GLASS, COMPUTER_GLASS);
                    tell(world, client, Span::Notice, "стекло извлечено");
                }
                _ => return false,
            }
            true
        }
        _ => false,
    }
}

/// Boards, cable and glass on frames (`item_interaction`, `install_board`).
fn frame_item(
    world: &mut World,
    actor: Actor,
    held: Entity,
    frame: Entity,
    computer: bool,
) -> bool {
    let client = actor.client;
    let Some(path) = item_path(world, held) else {
        return false;
    };
    let st = state(world, frame);
    if is_under(&path, BOARD_ROOT) {
        install_board(world, client, held, &path, frame, computer);
        return true;
    }
    let stack = world.get::<Stack>(held).is_some();
    if stack && is_under(&path, CABLE_COIL) {
        let wire_state = if computer {
            COMPUTER_SECURED
        } else {
            FRAME_EMPTY
        };
        if st != wire_state {
            return false;
        }
        if use_tool(
            world,
            actor,
            frame,
            CABLE_DS,
            FRAME_CABLE,
            Job::Frame(FrameJob::Wire { state: st }),
        ) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете прокладывать кабели...",
            );
        }
        return true;
    }
    if computer && stack && is_under(&path, GLASS) && st == COMPUTER_WIRED {
        if use_tool(
            world,
            actor,
            frame,
            CABLE_DS,
            COMPUTER_GLASS,
            Job::Frame(FrameJob::Glass),
        ) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(
                    world.get::<TilePos>(frame).copied().unwrap_or_default(),
                    "sound/items/deconstruct.ogg",
                    50.0,
                    true,
                ),
            );
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете вставлять стекло...",
            );
        }
        return true;
    }
    false
}

/// True when the board went in.
fn install_board(
    world: &mut World,
    client: Entity,
    board: Entity,
    path: &str,
    frame: Entity,
    computer: bool,
) -> bool {
    let st = state(world, frame);
    let def = board_def(world, path);
    if computer {
        if st != FRAME_EMPTY {
            tell(world, client, Span::Warning, "Плата уже установлена!");
            return false;
        }
    } else {
        if st == FRAME_EMPTY {
            tell(world, client, Span::Warning, "Сначала проложите кабели!");
            return false;
        }
        if st == FRAME_BOARD {
            tell(world, client, Span::Warning, "Плата уже установлена!");
            return false;
        }
        if def.as_ref().is_some_and(|d| d.needs_anchored) && !anchored(world, frame) {
            tell(world, client, Span::Warning, "Рама должна быть закреплена!");
            return false;
        }
    }
    // `istype(board, board_type) && board.build_path`; a type this port does not build yet is
    // refused the same way.
    let fits = def.as_ref().is_some_and(|d| {
        d.computer == computer && !d.build_path.is_empty() && family(&d.build_path).is_some()
    });
    let Some(def) = def.filter(|_| fits) else {
        tell(world, client, Span::Warning, "Неподходящая плата!");
        return false;
    };
    attach(world, board, frame);
    let needed = if computer {
        Vec::new()
    } else if let Some((_, parts)) = crate::vending::refill::board_override(world, board) {
        // A vendor board takes its brand's canister (`set_type`).
        parts
    } else {
        def.req_components.clone()
    };
    world.entity_mut(frame).insert(Frame {
        board: Some(path.to_owned()),
        needed,
    });
    set_state(
        world,
        frame,
        if computer {
            COMPUTER_BOARD
        } else {
            FRAME_BOARD
        },
    );
    if let Some(&pos) = world.get::<TilePos>(frame) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/items/deconstruct.ogg", 50.0, true),
        );
    }
    tell(world, client, Span::Notice, "плата установлена");
    true
}

/// `machine/add_part`: the held part or stack into what the frame still needs.
fn add_part(world: &mut World, actor: Actor, held: Entity, frame: Entity) {
    let client = actor.client;
    let Some(path) = item_path(world, held) else {
        return;
    };
    let Some(mut f) = world.get::<Frame>(frame).cloned() else {
        tell(world, client, Span::Warning, "Это сюда не подходит!");
        return;
    };
    let Some(slot) = f
        .needed
        .iter_mut()
        .find(|n| n.amount > 0 && is_under(&path, &n.path))
    else {
        tell(world, client, Span::Warning, "Это сюда не подходит!");
        return;
    };
    let name = type_name(world, &path);
    if world.get::<Stack>(held).is_some() {
        let used = crate::tools::stack_amount(world, held).min(slot.amount);
        if used == 0 || !crate::tools::use_stack(world, held, used) {
            return;
        }
        slot.amount -= used;
    } else {
        slot.amount -= 1;
        attach(world, held, frame);
    }
    world.entity_mut(frame).insert(f);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы добавляете {name} в раму."),
    );
}

fn set_state(world: &mut World, frame: Entity, to: i32) {
    world.entity_mut(frame).insert(ObjState(to));
    refresh_icon(world, frame);
}

/// `update_icon_state`: `"[base_icon_state][state]"` (`box_N`, computer frames `N`).
pub fn refresh_icon(world: &mut World, frame: Entity) {
    let Some(computer) = is_frame(world, frame) else {
        return;
    };
    let st = state(world, frame);
    let icon = if computer {
        st.to_string()
    } else {
        format!("box_{st}")
    };
    set_icon_state(world, frame, &icon);
}

fn drop_stack(world: &mut World, at: Entity, path: &str, amount: u32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        spawn_stack(world, path, pos, amount);
    }
}

/// `machine/dump_contents` (and the computer frame's board out): the board, the parts put in
/// and the stacks used so far onto the tile; the frame goes back to wired (computer: empty).
fn dump_frame(world: &mut World, frame: Entity) {
    let Some(&pos) = world.get::<TilePos>(frame) else {
        return;
    };
    let Some(f) = world
        .get_mut::<Frame>(frame)
        .map(|mut f| std::mem::take(&mut *f))
    else {
        return;
    };
    drop_frame_contents(world, frame, pos, &f);
    let computer = is_frame(world, frame) == Some(true);
    set_state(
        world,
        frame,
        if computer { FRAME_EMPTY } else { FRAME_WIRED },
    );
}

fn drop_frame_contents(world: &mut World, frame: Entity, pos: TilePos, f: &Frame) {
    let Some(board) = &f.board else {
        return;
    };
    if let Some(def) = board_def(world, board) {
        for req in def
            .req_components
            .iter()
            .filter(|r| is_under(&r.path, "/obj/item/stack"))
        {
            let left = f
                .needed
                .iter()
                .find(|n| n.path == req.path)
                .map_or(0, |n| n.amount);
            spawn_stack(world, &req.path, pos, req.amount.saturating_sub(left));
        }
    }
    let parts: Vec<Entity> = world
        .get::<ComponentParts>(frame)
        .map(|p| p.parts().to_vec())
        .unwrap_or_default();
    for part in parts {
        detach(world, part, pos);
    }
}

/// tg `/obj/structure/frame/atom_deconstruct` (from [`crate::obj_damage::deconstruct`]):
/// 5 iron, the cable of a wired frame, a computer frame's glass (shards when smashed), the
/// board and what was put in.
pub fn frame_deconstruct(world: &mut World, frame: Entity, pos: TilePos, disassembled: bool) {
    let Some(computer) = is_frame(world, frame) else {
        return;
    };
    let st = state(world, frame);
    if computer && st == COMPUTER_GLASSED {
        if disassembled {
            spawn_stack(world, GLASS, pos, COMPUTER_GLASS);
        } else {
            spawn_obj(world, SHARD, pos);
            spawn_obj(world, SHARD, pos);
        }
    }
    let wired = if computer {
        st >= COMPUTER_WIRED
    } else {
        st >= FRAME_WIRED
    };
    if wired {
        spawn_stack(world, CABLE_COIL, pos, FRAME_CABLE);
    }
    if let Some(f) = world.get::<Frame>(frame).cloned() {
        drop_frame_contents(world, frame, pos, &f);
    }
    spawn_stack(world, IRON, pos, FRAME_SHEETS);
}

fn on_floor(world: &World, e: Entity) -> bool {
    world
        .get::<TilePos>(e)
        .and_then(|&pos| turf_at(world, pos))
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, FLOOR))
}

/// `finalize_construction` of a machine frame.
fn finalize_machine(world: &mut World, actor: Actor, frame: Entity) {
    let client = actor.client;
    let (Some(f), Some(&pos)) = (
        world.get::<Frame>(frame).cloned(),
        world.get::<TilePos>(frame),
    ) else {
        return;
    };
    let Some(board) = f.board.clone() else {
        return;
    };
    let Some(def) = board_def(world, &board) else {
        return;
    };
    // `locate(circuit.build_path) in loc`.
    let present = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| path_of(world, e).is_some_and(|p| is_under(&p, &def.build_path)));
    if present {
        tell(world, client, Span::Warning, "Здесь уже есть такая машина!");
        return;
    }
    if f.needed.iter().any(|n| n.amount > 0) {
        tell(world, client, Span::Warning, "Не хватает деталей!");
        return;
    }
    let anchored_now = anchored(world, frame);
    let parts = frame_parts(world, frame);
    if build_machine(world, pos, board, parts, anchored_now, None).is_some() {
        world.despawn(frame);
        tell(world, client, Span::Notice, "машина собрана");
    }
}

/// `finalize_construction` of a computer frame.
fn finalize_computer(world: &mut World, actor: Actor, frame: Entity) {
    let client = actor.client;
    if !anchored(world, frame) {
        tell(world, client, Span::Warning, "Рама должна быть закреплена!");
        return;
    }
    let (Some(board), Some(&pos)) = (
        world.get::<Frame>(frame).and_then(|f| f.board.clone()),
        world.get::<TilePos>(frame),
    ) else {
        return;
    };
    let facing = world.get::<Facing>(frame).copied();
    let parts = frame_parts(world, frame);
    if build_machine(world, pos, board, parts, true, facing).is_some() {
        world.despawn(frame);
        tell(world, client, Span::Notice, "монитор подключён");
    }
}

fn frame_parts(world: &World, frame: Entity) -> Vec<Entity> {
    world
        .get::<ComponentParts>(frame)
        .map(|p| p.parts().to_vec())
        .unwrap_or_default()
}

/// The board's machine on `pos` (`new circuit.build_path(loc)`, `set_anchored`,
/// `on_construction`): spawned as the map would, with `parts` (its board item included) and its
/// family's setup, then `RefreshParts`.
pub fn build_machine(
    world: &mut World,
    pos: TilePos,
    board: String,
    parts: Vec<Entity>,
    anchored: bool,
    facing: Option<Facing>,
) -> Option<Entity> {
    let def = board_def(world, &board)?;
    // A vendor board builds its brand.
    let build_path = parts
        .iter()
        .find_map(|&p| crate::vending::refill::board_override(world, p))
        .map_or(def.build_path, |(path, _)| path);
    let Some(machine) = spawn_obj(world, &build_path, pos) else {
        warn!("frame at {pos:?}: no prototype for {build_path}");
        return None;
    };
    if let Some(facing) = facing {
        world.entity_mut(machine).insert(facing);
    }
    set_anchored(world, machine, anchored);
    for part in parts {
        attach(world, part, machine);
    }
    setup_built(world, machine);
    crate::stock_parts::refresh(world, machine);
    tile_changed(world, pos);
    Some(machine)
}

/// What map load gives a machine of its family, for one built from a frame.
pub fn setup_built(world: &mut World, machine: Entity) {
    let Some(fam) = path_of(world, machine).and_then(|p| family(&p)) else {
        return;
    };
    crate::power::register_machine(world, machine);
    match fam {
        Family::Charger => crate::power::chargers::setup(world, machine),
        Family::Kitchen => crate::kitchen::setup_machine(world, machine, false),
        Family::Tray => crate::botany::setup_tray(world, machine, false),
        Family::Station => crate::silicon::cyborg::setup_station(world, machine),
        Family::Analyzer => crate::science::analyzer::setup(world, machine),
        Family::Thermo => {
            crate::atmos_machinery::join_built(world, machine);
        }
        Family::RdConsole => crate::science::console::setup_console(world, machine, None),
        Family::CargoConsole => crate::cargo::console::setup_console(world, machine, None),
        Family::AtmosAlert => crate::atmos_machinery::consoles::setup_console(world, machine),
        Family::Autolathe => crate::science::autolathe::setup(world, machine),
        Family::Vending => crate::vending::refill::setup_built(world, machine),
        Family::ChemDispenser => crate::chem_dispenser::setup(world, machine),
        Family::SmartFridge => crate::smartfridge::setup(world, machine),
        Family::ChemMaster => crate::chem_master::setup(world, machine),
        Family::ChemHeater => crate::chem_heater::setup(world, machine),
        Family::DestructiveScanner => {
            if let Some(path) = path_of(world, machine) {
                crate::science::experi_scanner::setup(world, machine, path);
            }
        }
        Family::ServerControl => crate::science::server_control::setup(world, machine),
        Family::Pacman => crate::power::pacman::setup(world, machine),
        Family::Smes => crate::power::smes::setup(world, machine),
        Family::ShieldWallGen => crate::power::shields::setup_wall_gen(world, machine),
        Family::Orm => crate::mining::orm::setup(world, machine),
        Family::MailSorter => crate::cargo::mail_sorter::setup(world, machine),
        Family::MaterialsMarket => crate::cargo::materials_market::setup(world, machine),
        Family::Teapot => crate::teapot::setup(world, machine, false),
        Family::QuantumPad | Family::Teleportation => {}
        Family::Sheetifier => crate::eng_machines::sheetifier::setup(world, machine),
        Family::AtmosShieldGen => crate::eng_machines::atmos_shield::setup(world, machine),
        Family::Flatpacker => crate::eng_machines::flatpacker::setup(world, machine),
        Family::ModularShieldGen | Family::ShieldModule => {
            crate::eng_machines::modular_shield::setup(world, machine)
        }
        Family::Manufacturing => crate::eng_machines::manufactorio::setup(world, machine),
        Family::ChemMachine => {
            crate::chem_machines::ensure(world, machine);
        }
        Family::Ltsrbt => crate::cargo::black_market::setup(world, machine),
        Family::PowerMonitor => crate::power::monitor::setup(world, machine),
    }
}

/// Dense non-mob objects on `pos` other than `except` (tg `is_blocked_turf(TRUE, ignore_atoms)`).
fn blocked_except(world: &World, pos: TilePos, except: &[Entity]) -> bool {
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| {
            !except.contains(&e)
                && world.get::<Mover>(e).is_none()
                && world.get::<ss13_mapload::Turf>(e).is_none()
        })
}

/// tg `handle_deconstruct` of a covered machine (from [`crate::obj_damage::deconstruct`], before
/// it is despawned): out of its area's power, its contents onto the tile (`dump_contents`), a
/// frame (`spawn_frame`), its board, parts and the stacks its board needs.
pub fn on_deconstruction(world: &mut World, machine: Entity, pos: TilePos, disassembled: bool) {
    if !crate::stock_parts::ensure_parts(world, machine) {
        return;
    }
    let parts = frame_parts(world, machine);
    let Some((board_item, board)) = parts.iter().find_map(|&e| {
        item_path(world, e)
            .filter(|p| is_under(p, BOARD_ROOT))
            .map(|p| (e, p))
    }) else {
        return;
    };
    crate::power::unregister_machine(world, machine);
    let contents: Vec<Entity> = world
        .get::<Stored>(machine)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for item in contents {
        take_out(world, item, pos);
    }
    crate::science::autolathe::drop_sheets(world, machine, pos);
    crate::power::pacman::on_deconstruction(world, machine, pos);
    crate::power::smes::on_deconstruction(world, machine);
    crate::eng_machines::manufactorio::on_destroy(world, machine);
    let Some(def) = board_def(world, &board) else {
        return;
    };
    let path = path_of(world, machine).unwrap_or_default();
    if is_under(&path, COMPUTER_ROOT) {
        // `computer/spawn_frame`: the board stays in the frame.
        let broken = world.get::<Integrity>(machine).is_some_and(|i| i.broken);
        let facing = world.get::<Facing>(machine).copied();
        let Some(frame) = spawn_obj(world, COMPUTER_FRAME, pos) else {
            return;
        };
        if let Some(facing) = facing {
            world.entity_mut(frame).insert(facing);
        }
        set_anchored(world, frame, true);
        let st = if broken || !disassembled {
            spawn_obj(world, SHARD, pos);
            spawn_obj(world, SHARD, pos);
            COMPUTER_WIRED
        } else {
            COMPUTER_GLASSED
        };
        world.entity_mut(frame).insert(Frame {
            board: Some(board),
            ..default()
        });
        attach(world, board_item, frame);
        set_state(world, frame, st);
        return;
    }
    let anchored_now = anchored(world, machine);
    if let Some(frame) = spawn_obj(world, MACHINE_FRAME, pos) {
        set_state(world, frame, FRAME_WIRED);
        if blocked_except(world, pos, &[machine, frame]) {
            frame_deconstruct(world, frame, pos, disassembled);
            world.despawn(frame);
        } else {
            set_anchored(world, frame, anchored_now);
            if !disassembled {
                let max = world.get::<DmObj>(frame).map_or(0.0, |o| o.max_integrity);
                world.entity_mut(frame).insert(Integrity {
                    current: max * 0.5,
                    broken: false,
                });
            }
        }
    }
    for part in parts {
        detach(world, part, pos);
    }
    for req in def
        .req_components
        .iter()
        .filter(|r| is_under(&r.path, "/obj/item/stack"))
    {
        spawn_stack(world, &req.path, pos, req.amount);
    }
}

/// The end of a [`FrameJob`]: its effect if the state it started from still holds.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: FrameJob) {
    let target = action.target;
    let client = action.client;
    let pos = action.target_pos;
    let consume = |world: &mut World| {
        action
            .item
            .is_none_or(|i| crate::tools::consume(world, i, action.amount))
    };
    if let FrameJob::Build { computer } = job {
        if crate::construction::tile_blocked(world, pos) {
            tell(world, client, Span::Warning, "Что-то загораживает клетку!");
            return;
        }
        if !consume(world) {
            return;
        }
        let path = if computer {
            COMPUTER_FRAME
        } else {
            MACHINE_FRAME
        };
        if let Some(frame) = spawn_obj(world, path, pos) {
            // `setDir(REVERSE_DIR(user.dir))`: a computer frame faces its builder.
            if let Some(facing) = world.get::<Facing>(mob).map(|f| f.0.reverse()) {
                world.entity_mut(frame).insert(Facing(facing));
            }
            refresh_icon(world, frame);
        }
        tell(world, client, Span::Notice, "рама создана");
        return;
    }
    if job == FrameJob::Unscrew {
        if buildable_machine(world, target).is_some() {
            crate::obj_damage::deconstruct(world, target, true);
        }
        return;
    }
    let Some(computer) = is_frame(world, target) else {
        return;
    };
    let st = state(world, target);
    match job {
        FrameJob::Anchor { anchored: was } => {
            // `unfasten_wrench_check`: `can_be_unfasten_wrench` again at the end.
            let needs_anchor = was
                && world
                    .get::<Frame>(target)
                    .and_then(|f| f.board.as_deref())
                    .and_then(|b| board_def(world, b))
                    .is_some_and(|b| b.needs_anchored && !b.computer);
            if anchored(world, target) != was || needs_anchor {
                return;
            }
            set_anchored(world, target, !was);
            let text = if was {
                "Вы откручиваете раму."
            } else {
                "Вы прикручиваете раму."
            };
            tell(world, client, Span::Notice, text);
        }
        FrameJob::Disassemble => {
            if st != FRAME_EMPTY || anchored(world, target) || !consume(world) {
                return;
            }
            crate::obj_damage::deconstruct(world, target, true);
            tell(world, client, Span::Notice, "Вы разбираете раму.");
        }
        FrameJob::Wire { state: from } => {
            if st != from || !consume(world) {
                return;
            }
            set_state(
                world,
                target,
                if computer {
                    COMPUTER_WIRED
                } else {
                    FRAME_WIRED
                },
            );
            tell(world, client, Span::Notice, "Вы прокладываете кабели.");
        }
        FrameJob::Unwire => {
            if computer || st != FRAME_WIRED {
                return;
            }
            set_state(world, target, FRAME_EMPTY);
            spawn_stack(world, CABLE_COIL, pos, FRAME_CABLE);
            tell(world, client, Span::Notice, "Вы снимаете кабели.");
        }
        FrameJob::Glass => {
            if !computer || st != COMPUTER_WIRED || !consume(world) {
                return;
            }
            set_state(world, target, COMPUTER_GLASSED);
            tell(world, client, Span::Notice, "Вы вставляете стекло.");
        }
        FrameJob::Build { .. } | FrameJob::Unscrew => {}
    }
}

/// `examine` of frames: the board and what a machine frame still needs.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(computer) = is_frame(world, target) else {
        return;
    };
    let f = world.get::<Frame>(target);
    if let Some(board) = f.and_then(|f| f.board.as_deref()) {
        lines.push((
            Span::Notice,
            format!("Установлена плата: {}.", type_name(world, board)),
        ));
    } else if !computer && state(world, target) == FRAME_EMPTY {
        lines.push((Span::Info, "Нужно проложить 5 кабелей.".to_owned()));
    }
    if computer {
        return;
    }
    let Some(f) = f.filter(|f| f.board.is_some()) else {
        return;
    };
    let needed: Vec<String> = f
        .needed
        .iter()
        .filter(|n| n.amount > 0)
        .map(|n| format!("{} × {}", n.amount, type_name(world, &n.path)))
        .collect();
    if needed.is_empty() {
        lines.push((
            Span::Info,
            "Детали на месте: отвёртка завершит сборку.".to_owned(),
        ));
    } else {
        lines.push((Span::Info, format!("Нужно: {}.", needed.join(", "))));
    }
}
