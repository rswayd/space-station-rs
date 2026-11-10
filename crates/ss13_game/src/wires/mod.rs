//! Wires and hacking (server), after tg `code/datums/wires/` (`_wires.dm`, `airlock.dm`,
//! `apc.dm`, `vending.dm`) and the machines' `shock()` (`objs.dm`, `airlock.dm`, `vending.dm`,
//! `electrocute_mob` in `power.dm`, `electrocute_act` in `living_defense.dm`,
//! `carbon_defense.dm`, `human_defense.dm`).
//!
//! - [`Wires`] is tg `/datum/wires` on an airlock, an APC or a vending machine: the wire list of
//!   its set, the colors (shuffled once per round per `dictionary_key`/`holder_type`, tg
//!   `GLOB.wire_color_directory`; here seeded from the map seed and the key, [`colors_for`]) and
//!   the cut wires. [`cut`] toggles a wire (wirecutters), [`pulse`] pulses one (multitool); each
//!   runs the holder's `on_cut`/`on_pulse`.
//! - The tgui `Wires` window (`ss13_tgui::interfaces::wires`) opens from a wire tool on an open
//!   panel or an empty hand on it (`INTERACT_MACHINE_WIRES_IF_OPEN`); its status closes it when
//!   the holder stops being `interactable` (panel shut, electrified and powered). Acts are
//!   checked at the moment of effect: interactable, adjacent, the wire color exists, the right
//!   tool held.
//! - Electrified holders ([`Electrified`]) shock a bare hand or a bump: burn by the area's power
//!   (APC cell and powernet), stun, then paralysis 2 s later; insulated gloves
//!   (`siemens_coefficient` 0) stop it, and then the touch goes through as usual.
//!
//! Per tick only holders with something pending are touched: power outages ([`PowerOutage`]),
//! electrification countdowns, timed resets ([`WireResets`]), shooting vendors
//! ([`ShootsInventory`]) and mobs waiting for their second shock ([`SecondaryShock`]).
// shortcut: no wire revealing (blueprints, abductor multitool,
// `TRAIT_KNOW_ENGI_WIRES`), no colorblindness, no silicon access (the AI wire
// only changes the stub state and its status light), no airlock security plating.

use std::collections::HashMap;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use ss13_mapload::fidelity::{MapSeed, is_under};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{Areas, DmClothing, DmName, DmPower, DoorKind};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::interfaces::wires::{WireEntry, WiresData, WiresUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow, distance_status,
};
use ss13_tgui::{UiInterface, UiStatus};
use ss13_world::{Bumped, SpatialIndex, WorldSystems};

use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::CombatRng;
use crate::construction::PanelOpen;
use crate::doors::{Door, DoorState, with_door};
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat, update_stat};
use crate::inventory::Inventory;
use crate::power::model::{CHANNEL_AUTO_ON, CHANNEL_OFF};
use crate::power::{Apc, AreaPower, AreaPowerChanged, Channel, Grid, MACHINE_TICK_SECS, Unpowered};
use crate::status_effects::Incap;
use crate::tools::tell_mob;
use crate::tools::{Actor, ToolKind, tool_of};
use crate::vending::{RecordKind, Vendor};

mod holders;
mod interact;
mod pulse;
mod sets;
mod shock;
#[cfg(test)]
mod tests;

use holders::*;
pub(crate) use holders::{apc_channel, apc_update};
use interact::*;
pub use interact::{
    airlock_takes_click, attach_assembly, detach_assembly, interact, pulse_assembly,
    screwdriver_act, wire_tool_interaction,
};
use pulse::*;
pub use pulse::{cut, cut_color, emp_pulse, pulse, pulse_color};
pub(crate) use pulse::{lose_power, schedule_reset, set_bolts, set_electrified};
pub(crate) use sets::with_duds;
pub use sets::{
    add_autolathe_wires, add_vending_wires, airlock_set, apc_set, autolathe_set, chem_grenade_set,
    colors_for, vending_set,
};
use shock::*;
pub(crate) use shock::{electrocute_act, electrocute_from_area, electrocute_from_node};
pub use shock::{interactable, shock};

pub const WIRE_AI: &str = "AI Connection";
pub const WIRE_BACKUP1: &str = "Auxiliary Power 1";
pub const WIRE_BACKUP2: &str = "Auxiliary Power 2";
pub const WIRE_BOLTS: &str = "Bolts";
pub const WIRE_CONTRABAND: &str = "Contraband";
pub const WIRE_DISABLE: &str = "Disable";
pub const WIRE_HACK: &str = "Hack";
pub const WIRE_ZAP: &str = "High Voltage Circuit";
pub const WIRE_EQUIPMENT: &str = "Equipment";
pub const WIRE_ENVIRONMENT: &str = "Environment";
pub const WIRE_FEEDBACK: &str = "Feedback";
pub const WIRE_IDSCAN: &str = "ID Scan";
pub const WIRE_INTERFACE: &str = "Interface";
pub const WIRE_LIGHT: &str = "Lights";
pub const WIRE_OPEN: &str = "Open";
pub const WIRE_POWER1: &str = "Main Power 1";
pub const WIRE_POWER2: &str = "Main Power 2";
pub const WIRE_SAFETY: &str = "Safety";
pub const WIRE_SHOCK: &str = "High Voltage Ground";
pub const WIRE_SPEAKER: &str = "Speaker";
pub const WIRE_THROW: &str = "Throw";
pub const WIRE_TIMING: &str = "Timing";
pub const WIRE_UNRESTRICTED_EXIT: &str = "Unrestricted Exit";
pub const WIRE_ZAP1: &str = "High Voltage Circuit 1";
pub const WIRE_ZAP2: &str = "High Voltage Circuit 2";
/// `WIRE_DUD_PREFIX`.
pub const WIRE_DUD_PREFIX: &str = "__dud";

/// `default_possible_colors`: color -> its greyscale variant, in the DM list's order.
pub const POSSIBLE_COLORS: [(&str, &str); 17] = [
    ("blue", "dim grey"),
    ("brown", "dim grey"),
    ("crimson", "grey"),
    ("cyan", "light grey"),
    ("gold", "grey"),
    ("green", "grey"),
    ("grey", "grey"),
    ("lime", "light grey"),
    ("magenta", "grey"),
    ("orange", "grey"),
    ("pink", "grey"),
    ("purple", "grey"),
    ("red", "grey"),
    ("silver", "light grey"),
    ("violet", "grey"),
    ("white", "white"),
    ("yellow", "light grey"),
];

/// `MAXIMUM_EMP_WIRES`.
const MAXIMUM_EMP_WIRES: usize = 3;
/// `MACHINE_DEFAULT_ELECTRIFY_TIME`: seconds for airlocks, machine ticks for vendors.
pub const MACHINE_DEFAULT_ELECTRIFY_TIME: f64 = 30.0;
/// tg airlock `loseMainPower`/`loseBackupPower` outages, seconds.
const MAIN_OUTAGE_SECS: f64 = 60.0;
const BACKUP_OUTAGE_SECS: f64 = 60.0;
const BACKUP_AFTER_MAIN_SECS: f64 = 10.0;
/// `COOLDOWN_START(src, shockCooldown, 1 SECONDS)`.
const SHOCK_COOLDOWN_SECS: f64 = 1.0;
/// `electrocute_act` defaults: `stun_duration`, the second shock's delay and factor.
const SHOCK_STUN_SECS: f32 = 4.0;
const SECONDARY_SHOCK_DELAY: f64 = 2.0;
const SECONDARY_SHOCK_FACTOR: f32 = 1.5;
/// `JOULES_PER_DAMAGE`, `STANDARD_CELL_CHARGE`.
const JOULES_PER_DAMAGE: f64 = 25_000.0;
const STANDARD_CELL_CHARGE: f64 = 10_000.0;
/// `/obj/machinery/vending/shock`: the default `siemens_coeff`.
const VENDING_SIEMENS: f32 = 0.7;
/// Vending `shoot_inventory_chance` (percent per machine tick) and `throw_at(target, 16, 3)`.
const SHOOT_CHANCE: f64 = 1.0;
const SHOOT_RANGE: u32 = 16;
const SHOOT_SPEED: f32 = 3.0;
/// `view(7, src)` of `throw_item`.
const SHOOT_VIEW: u32 = 7;
/// APC `reset` delays after a pulse.
const APC_SHORT_SECS: f64 = 120.0;
const AI_RESET_SECS: f64 = 1.0;

/// tg `AI_WIRE_*` (`aiControlDisabled`).
const AI_WIRE_NORMAL: i8 = 0;
const AI_WIRE_DISABLED: i8 = 1;
const AI_WIRE_HACKED: i8 = 2;
const AI_WIRE_DISABLED_HACKED: i8 = -1;

/// The holder kind, which picks `on_cut`, `on_pulse`, `interactable` and `get_status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireHolder {
    Airlock,
    Apc,
    Vending,
    Autolathe,
    /// `/datum/wires/mulebot` (`crate::bots::mule`).
    Mulebot,
    /// `/datum/wires/explosive/chem_grenade` (`crate::assemblies::chem_grenade`).
    ChemGrenade,
    /// The machines of [`crate::machine_wires`].
    Machine(crate::machine_wires::MachineWires),
}

/// tg `assemblies` of a [`Wires`] holder: color -> the attached assembly (inside the holder).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct WireAssemblies(pub Vec<(&'static str, Entity)>);

impl WireAssemblies {
    pub fn at(&self, color: &str) -> Option<Entity> {
        self.0.iter().find(|(c, _)| *c == color).map(|(_, e)| *e)
    }
}

/// tg `/datum/wires` of one holder.
#[derive(Component, Clone, Debug)]
pub struct Wires {
    pub holder: WireHolder,
    pub proper_name: &'static str,
    /// Color -> wire, in the order the colors were assigned (the DM assoc list's order).
    pub colors: Vec<(&'static str, String)>,
    /// `cut_wires`, in cutting order.
    pub cut: Vec<String>,
}

impl Wires {
    pub fn get_wire(&self, color: &str) -> Option<&str> {
        self.colors
            .iter()
            .find(|(c, _)| *c == color)
            .map(|(_, w)| w.as_str())
    }

    pub fn color_of(&self, wire: &str) -> Option<&'static str> {
        self.colors.iter().find(|(_, w)| w == wire).map(|(c, _)| *c)
    }

    pub fn is_cut(&self, wire: &str) -> bool {
        self.cut.iter().any(|w| w == wire)
    }

    pub fn is_dud(wire: &str) -> bool {
        wire.starts_with(WIRE_DUD_PREFIX)
    }
}

/// A wire set (`/datum/wires` subtype): its `dictionary_key` (or `holder_type`), name and
/// wires, duds included (`add_duds`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireSet {
    pub key: &'static str,
    pub proper_name: &'static str,
    pub wires: Vec<String>,
}

/// tg `GLOB.wire_color_directory`: the colors of each key this round.
#[derive(Resource, Default)]
pub struct WireColors {
    pub seed: u64,
    directory: HashMap<&'static str, Vec<(&'static str, String)>>,
}

impl WireColors {
    /// The holder's [`Wires`], with its key's colors (assigned on first use).
    pub fn wires_for(&mut self, holder: WireHolder, set: &WireSet) -> Wires {
        let seed = self.seed;
        let colors = self
            .directory
            .entry(set.key)
            .or_insert_with(|| colors_for(seed, set))
            .clone();
        Wires {
            holder,
            proper_name: set.proper_name,
            colors,
            cut: Vec::new(),
        }
    }
}

/// tg `secondsElectrified` / `seconds_electrified` while not 0.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum Electrified {
    /// `MACHINE_ELECTRIFIED_PERMANENT`.
    Permanent,
    /// Seconds left (airlocks) or machine ticks left (vendors).
    Timed(f64),
}

/// On an airlock with main or backup power coming back.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PowerOutage;

/// tg `addtimer(CALLBACK(..., reset), ...)` after a pulse: (when, wire).
#[derive(Component, Clone, Debug, Default)]
pub struct WireResets(pub Vec<(f64, &'static str)>);

/// Vending machine hack state (tg `shoot_inventory`, `scan_id`, `shut_up`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VendorHack {
    pub shoot_inventory: bool,
    pub scan_id: bool,
    pub shut_up: bool,
}

impl Default for VendorHack {
    fn default() -> Self {
        Self {
            shoot_inventory: false,
            scan_id: true,
            shut_up: false,
        }
    }
}

/// On a vendor with `shoot_inventory` on.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ShootsInventory;

/// tg `secondary_shock`: the paralysis due at `at`.
#[derive(Component, Clone, Copy, Debug)]
pub struct SecondaryShock {
    pub at: f64,
    pub secs: f32,
}

/// The server side of the `Wires` interface.
pub struct WiresWindow;

impl ss13_tgui::UiInterface for WiresWindow {
    const NAME: &'static str = WiresUi::NAME;
    type Data = WiresData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireAct {
    Cut(String),
    Pulse(String),
    Attach(String),
}

/// What `ui_data` reads from the holder.
#[derive(QueryData)]
pub struct WiresSource {
    wires: &'static Wires,
    door: Option<&'static Door>,
    unpowered: Has<Unpowered>,
    apc: Option<&'static Apc>,
    vendor: Option<&'static Vendor>,
    hack: Option<&'static VendorHack>,
    electrified: Has<Electrified>,
    autolathe: Option<&'static crate::science::autolathe::Autolathe>,
    assemblies: Option<&'static WireAssemblies>,
    machine_status: Option<&'static crate::machine_wires::MachineWireStatus>,
}

impl ServerUi for WiresWindow {
    type Source = WiresSource;
    type UserParam = ();
    type Act = WireAct;

    fn static_data(_: WiresSourceItem<'_, '_>) {}

    fn data(source: WiresSourceItem<'_, '_>, _: Entity, _: &()) -> WiresData {
        let wires = source.wires;
        WiresData {
            proper_name: (wires.proper_name != "Unknown").then(|| wires.proper_name.to_owned()),
            wires: wires
                .colors
                .iter()
                .map(|(color, wire)| WireEntry {
                    color: (*color).to_owned(),
                    shown_color: (*color).to_owned(),
                    // `can_reveal_wires` is always false here (see the module shortcut);
                    // `always_reveal_wire` of the fire alarm (detection), the cyborg (model
                    // reset) and the wire bundle (every port).
                    wire: always_reveal(wires.holder, wire).then(|| wire.clone()),
                    cut: wires.is_cut(wire),
                    attached: source.assemblies.is_some_and(|a| a.at(color).is_some()),
                })
                .collect(),
            status: status(&source),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<WireAct, UiReject> {
        let wire = params
            .get("wire")
            .filter(|w| !w.is_empty())
            .ok_or_else(|| UiReject::BadParams("wire: expected a color".into()))?
            .to_owned();
        match action {
            "cut" => Ok(WireAct::Cut(wire)),
            "pulse" => Ok(WireAct::Pulse(wire)),
            "attach" => Ok(WireAct::Attach(wire)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub struct WiresServerPlugin;

impl Plugin for WiresServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<WiresWindow>()
            .init_resource::<WireColors>()
            // Doors, APCs and vendors are built in `MapInit`.
            .add_systems(ss13_world::MapInitLate, init_wires)
            .add_systems(
                Update,
                (
                    touches
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    wire_window_status.in_set(UiSystems::StatusHook),
                    wire_acts.in_set(UiSystems::Handle),
                    tick_wires.after(WorldSystems::Movement),
                ),
            );
        crate::machine_wires::build(app);
    }
}

/// [`Wires`] on every airlock, APC and vending machine of the map pass, colors from the map
/// seed (a later load keeps the round's colors).
fn init_wires(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0) ^ 0x3172_e5ee_d5a1_7c3d;
    let mut colors = world.remove_resource::<WireColors>().unwrap_or_default();
    colors.seed = seed;
    let mut doors = world.query_filtered::<(Entity, &Door, &TilePos), With<ss13_world::Mapped>>();
    let airlocks: Vec<(Entity, TilePos)> = doors
        .iter(world)
        .filter(|(_, d, _)| d.kind == DoorKind::Airlock)
        .map(|(e, _, p)| (e, *p))
        .collect();
    let mut new = Vec::new();
    for (e, pos) in airlocks {
        let area_path = world
            .get_resource::<Areas>()
            .and_then(|a| a.area_at(pos))
            .and_then(|a| world.get::<DmPower>(a))
            .map(|p| p.path.clone())
            .unwrap_or_default();
        new.push((
            e,
            colors.wires_for(WireHolder::Airlock, &airlock_set(&area_path)),
        ));
    }
    let mut apcs = world.query_filtered::<Entity, (With<Apc>, With<ss13_world::Mapped>)>();
    for e in apcs.iter(world).collect::<Vec<_>>() {
        new.push((e, colors.wires_for(WireHolder::Apc, &apc_set())));
    }
    let mut vendors = world.query_filtered::<Entity, (With<Vendor>, With<ss13_world::Mapped>)>();
    let vendors: Vec<Entity> = vendors.iter(world).collect();
    for &e in &vendors {
        new.push((e, colors.wires_for(WireHolder::Vending, &vending_set())));
    }
    let mut lathes = world.query_filtered::<Entity, (
        With<crate::science::autolathe::Autolathe>,
        With<ss13_world::Mapped>,
    )>();
    for e in lathes.iter(world).collect::<Vec<_>>() {
        new.push((e, colors.wires_for(WireHolder::Autolathe, &autolathe_set())));
    }
    info!("wires on {} holders", new.len());
    world.insert_batch(new);
    world.insert_batch(vendors.into_iter().map(|e| (e, VendorHack::default())));
    world.insert_resource(colors);
}
