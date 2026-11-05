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
// shortcut: no assemblies (signalers) on wires: `attach` does nothing and nothing is ever
// attached; add them with assemblies. No wire revealing (blueprints, abductor multitool,
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
use crate::server::PlayerMob;
use crate::status_effects::Incap;
use crate::tools::{Actor, ToolKind, tool_of};
use crate::vending::{RecordKind, Vendor};

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

/// `add_duds(n)`: `__dud<n-1>` .. `__dud0`.
fn with_duds(wires: &[&str], duds: usize) -> Vec<String> {
    let mut out: Vec<String> = wires.iter().map(|w| (*w).to_owned()).collect();
    out.extend((0..duds).rev().map(|i| format!("{WIRE_DUD_PREFIX}{i}")));
    out
}

/// tg `area.airlock_wires`, most specific area type first wins.
const AIRLOCK_AREA_WIRES: [(&str, &str, &str); 17] = [
    ("/area/station/ai", "/datum/wires/airlock/ai", "AI Airlock"),
    (
        "/area/station/cargo",
        "/datum/wires/airlock/cargo",
        "Cargo Airlock",
    ),
    (
        "/area/station/command",
        "/datum/wires/airlock/command",
        "Command Airlock",
    ),
    (
        "/area/station/engineering",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/station/maintenance",
        "/datum/wires/airlock/maint",
        "Maintenance Airlock",
    ),
    (
        "/area/station/medical",
        "/datum/wires/airlock/medbay",
        "Medbay Airlock",
    ),
    (
        "/area/station/science",
        "/datum/wires/airlock/science",
        "Science Airlock",
    ),
    (
        "/area/station/security",
        "/datum/wires/airlock/security",
        "Security Airlock",
    ),
    (
        "/area/station/service",
        "/datum/wires/airlock/service",
        "Service Airlock",
    ),
    (
        "/area/station/solars",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/station/tcommsat",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/ruin/space/solars",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/ruin/space/tcommsat_oldaisat",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/ruin/space/has_grav/hauntedtradingpost/employees",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/ruin/space/has_grav/hauntedtradingpost/maint",
        "/datum/wires/airlock/maint",
        "Maintenance Airlock",
    ),
    (
        "/area/ruin/space/has_grav/hauntedtradingpost/office",
        "/datum/wires/airlock/cargo",
        "Cargo Airlock",
    ),
    (
        "/area/ruin/space/has_grav/hauntedtradingpost/aicore",
        "/datum/wires/airlock/security",
        "Security Airlock",
    ),
];

/// `/datum/wires/airlock` (or the subtype the airlock's area names, tg `get_wires()`).
pub fn airlock_set(area_path: &str) -> WireSet {
    let (key, proper_name) = AIRLOCK_AREA_WIRES
        .iter()
        .filter(|(area, ..)| is_under(area_path, area))
        .max_by_key(|(area, ..)| area.len())
        .map_or(
            ("/obj/machinery/door/airlock", "Generic Airlock"),
            |(_, k, n)| (*k, *n),
        );
    WireSet {
        key,
        proper_name,
        wires: with_duds(
            &[
                WIRE_AI,
                WIRE_BACKUP1,
                WIRE_BACKUP2,
                WIRE_BOLTS,
                WIRE_IDSCAN,
                WIRE_FEEDBACK,
                WIRE_OPEN,
                WIRE_POWER1,
                WIRE_POWER2,
                WIRE_SAFETY,
                WIRE_SHOCK,
                WIRE_TIMING,
                WIRE_UNRESTRICTED_EXIT,
                WIRE_ZAP1,
                WIRE_ZAP2,
            ],
            2,
        ),
    }
}

/// `/datum/wires/apc`.
pub fn apc_set() -> WireSet {
    WireSet {
        key: "/obj/machinery/power/apc",
        proper_name: "APC",
        wires: with_duds(
            &[
                WIRE_EQUIPMENT,
                WIRE_LIGHT,
                WIRE_ENVIRONMENT,
                WIRE_POWER1,
                WIRE_POWER2,
                WIRE_INTERFACE,
                WIRE_AI,
            ],
            6,
        ),
    }
}

/// `/datum/wires/vending`.
pub fn vending_set() -> WireSet {
    WireSet {
        key: "/obj/machinery/vending",
        proper_name: "Vending Unit",
        wires: with_duds(
            &[
                WIRE_THROW,
                WIRE_SHOCK,
                WIRE_SPEAKER,
                WIRE_CONTRABAND,
                WIRE_IDSCAN,
            ],
            1,
        ),
    }
}

/// `/datum/wires/autolathe`.
pub fn autolathe_set() -> WireSet {
    WireSet {
        key: crate::science::autolathe::AUTOLATHE_ROOT,
        proper_name: "Autolathe",
        wires: with_duds(&[WIRE_HACK, WIRE_DISABLE, WIRE_SHOCK, WIRE_ZAP], 6),
    }
}

/// [`Wires`] on an autolathe (map load replaces them with the round's colors in
/// [`init_wires`]).
pub fn add_autolathe_wires(world: &mut World, e: Entity) {
    let wires = world
        .get_resource_or_init::<WireColors>()
        .wires_for(WireHolder::Autolathe, &autolathe_set());
    world.entity_mut(e).insert(wires);
}

/// [`Wires`] and [`VendorHack`] on a vendor built from a frame.
pub fn add_vending_wires(world: &mut World, e: Entity) {
    let wires = world
        .get_resource_or_init::<WireColors>()
        .wires_for(WireHolder::Vending, &vending_set());
    world.entity_mut(e).insert((wires, VendorHack::default()));
}

/// FNV-1a, to give every wire set key its own stream from the round seed.
fn key_hash(key: &str) -> u64 {
    key.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// tg `randomize()` for a set: `shuffle(wires)`, then each wire takes `pick_n_take` of the
/// remaining colors (refilled when they run out). Deterministic in `(seed, key)`.
pub fn colors_for(seed: u64, set: &WireSet) -> Vec<(&'static str, String)> {
    let mut rng = Rng::new(seed ^ key_hash(set.key));
    let mut wires = set.wires.clone();
    // tg `/proc/shuffle`: `Swap(i, rand(i, len))`.
    for i in 0..wires.len().saturating_sub(1) {
        let j = rng.range(i as i64, wires.len() as i64 - 1) as usize;
        wires.swap(i, j);
    }
    let mut possible: Vec<&'static str> = Vec::new();
    let mut colors: Vec<(&'static str, String)> = Vec::new();
    for wire in wires {
        if possible.is_empty() {
            possible = POSSIBLE_COLORS.iter().map(|(c, _)| *c).collect();
        }
        let color = possible.remove(rng.range(0, possible.len() as i64 - 1) as usize);
        // `LAZYSET`: an assigned color keeps its place and takes the new wire.
        match colors.iter_mut().find(|(c, _)| *c == color) {
            Some(entry) => entry.1 = wire,
            None => colors.push((color, wire)),
        }
    }
    colors
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
                    // `can_reveal_wires` is always false here (see the module shortcut).
                    wire: None,
                    cut: wires.is_cut(wire),
                    attached: false,
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

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// DM `dir2text`.
fn dir2text(dir: u8) -> &'static str {
    match dir {
        1 => "north",
        2 => "south",
        4 => "east",
        8 => "west",
        5 => "northeast",
        6 => "southeast",
        9 => "northwest",
        10 => "southwest",
        _ => "",
    }
}

/// `REVERSE_DIR` on dir bits.
fn reverse_dir(dir: u8) -> u8 {
    ((dir & 0b0101) << 1) | ((dir & 0b1010) >> 1)
}

/// `get_status()` of the holder.
fn status(s: &WiresSourceItem<'_, '_>) -> Vec<String> {
    match s.wires.holder {
        WireHolder::Airlock => {
            let Some(a) = s.door else {
                return Vec::new();
            };
            let mut out = vec![
                format!(
                    "The door bolts {}",
                    if a.locked {
                        "have engaged!"
                    } else {
                        "have disengaged."
                    }
                ),
                format!("The test light is {}.", on_off(a.has_power(!s.unpowered))),
                format!(
                    "The AI connection light is {}.",
                    on_off(a.ai_control_disabled == AI_WIRE_NORMAL)
                ),
                format!("The check wiring light is {}.", on_off(!a.safe)),
                format!("The timer is powered {}.", on_off(a.autoclose)),
                format!("The speed light is {}.", on_off(a.normalspeed)),
                format!("The emergency light is {}.", on_off(a.emergency)),
            ];
            out.push(if a.unres_latch {
                if a.unres_sides != 0 {
                    format!(
                        "The unrestricted exit display is indicating that it is letting people pass from the {}.",
                        dir2text(reverse_dir(a.unres_sides))
                    )
                } else {
                    "The unrestricted exit display is faintly flickering.".to_owned()
                }
            } else {
                "The unrestricted exit display is completely inactive.".to_owned()
            });
            out
        }
        WireHolder::Apc => {
            let Some(a) = s.apc else {
                return Vec::new();
            };
            let channel = |c: Channel| on_off(a.channels[c as usize] > CHANNEL_OFF);
            vec![
                format!(
                    "The interface light is {}.",
                    if a.locked { "red" } else { "green" }
                ),
                format!(
                    "The short indicator is {}.",
                    if a.shorted { "lit" } else { "off" }
                ),
                format!("The channel one light is {}.", channel(Channel::Equip)),
                format!("The channel two light is {}.", channel(Channel::Light)),
                format!("The channel three light is {}.", channel(Channel::Environ)),
                format!("The AI connection light is {}.", on_off(!a.aidisabled)),
            ]
        }
        WireHolder::Vending => {
            let hack = s.hack.copied().unwrap_or_default();
            let extended = s.vendor.is_some_and(|v| v.extended_inventory);
            vec![
                format!("The orange light is {}.", on_off(s.electrified)),
                format!(
                    "The red light is {}.",
                    if hack.shoot_inventory {
                        "off"
                    } else {
                        "blinking"
                    }
                ),
                format!("The green light is {}.", on_off(extended)),
                format!(
                    "A {} light is on.",
                    if hack.scan_id { "purple" } else { "yellow" }
                ),
                // `age_restrictions` defaults to TRUE and no vending wire changes it.
                "A white light is on.".to_owned(),
                // shortcut: vendors speak Galactic Common only; the speaker wire cycles nothing.
                format!(
                    "The speaker light is {}. The language is set to Galactic Common.",
                    on_off(!hack.shut_up)
                ),
            ]
        }
        // `/datum/wires/mulebot` has no `get_status`.
        WireHolder::Mulebot => Vec::new(),
        WireHolder::Autolathe => {
            let Some(a) = s.autolathe else {
                return Vec::new();
            };
            vec![
                format!("The red light is {}.", on_off(a.disabled)),
                format!("The blue light is {}.", on_off(a.hacked)),
            ]
        }
    }
}

pub struct WiresServerPlugin;

impl Plugin for WiresServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<WiresWindow>()
            .init_resource::<WireColors>()
            // Doors, APCs and vendors are built in `Startup`.
            .add_systems(PostStartup, init_wires)
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
    }
}

/// [`Wires`] on every airlock, APC and vending machine, colors from the map seed.
fn init_wires(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0) ^ 0x3172_e5ee_d5a1_7c3d;
    let mut colors = WireColors { seed, ..default() };
    let mut doors = world.query::<(Entity, &Door, &TilePos)>();
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
    let mut apcs = world.query_filtered::<Entity, With<Apc>>();
    for e in apcs.iter(world).collect::<Vec<_>>() {
        new.push((e, colors.wires_for(WireHolder::Apc, &apc_set())));
    }
    let mut vendors = world.query_filtered::<Entity, With<Vendor>>();
    let vendors: Vec<Entity> = vendors.iter(world).collect();
    for &e in &vendors {
        new.push((e, colors.wires_for(WireHolder::Vending, &vending_set())));
    }
    let mut lathes = world.query_filtered::<Entity, With<crate::science::autolathe::Autolathe>>();
    for e in lathes.iter(world).collect::<Vec<_>>() {
        new.push((e, colors.wires_for(WireHolder::Autolathe, &autolathe_set())));
    }
    info!("wires on {} holders", new.len());
    world.insert_batch(new);
    world.insert_batch(vendors.into_iter().map(|e| (e, VendorHack::default())));
    world.insert_resource(colors);
}

/// `airlock_takes_click`: whether an airlock's own click handling (opening, closing) yields
/// to its panel and wires or a tool: a screwdriver (`screwdriver_act`), a crowbar, a wire tool on an open panel
/// (`item_interaction`), a bare hand on an open panel (`INTERACT_MACHINE_WIRES_IF_OPEN`) or on
/// an electrified airlock (the shock comes first, [`touches`]).
pub fn airlock_takes_click(
    door: &Door,
    holding: bool,
    tool: Option<ToolKind>,
    electrified: bool,
) -> bool {
    if door.kind != DoorKind::Airlock {
        return false;
    }
    match tool {
        // `screwdriver_act`, `crowbar_act` (`crate::doors::crowbar_act`).
        Some(ToolKind::Screwdriver | ToolKind::Crowbar) => true,
        Some(ToolKind::Wirecutters | ToolKind::Multitool) => door.panel_open,
        _ => !holding && (door.panel_open || electrified),
    }
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

/// Everyone within hearing of `pos` (tg `audible_message`).
fn hear(world: &mut World, pos: TilePos, text: &str) {
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![ChatSegment::new(Span::Italics, text)],
        },
    });
}

fn name_of(world: &World, e: Entity) -> String {
    if let Some(name) = world.get::<Name>(e) {
        return name.as_str().to_owned();
    }
    world.get::<DmName>(e).map_or_else(
        || "что-то".to_owned(),
        |n| crate::vending::plain_name(&n.name).to_owned(),
    )
}

fn panel_open(world: &World, holder: Entity) -> bool {
    match world.get::<Door>(holder) {
        Some(door) => door.panel_open,
        None => world.get::<PanelOpen>(holder).is_some_and(|p| p.0),
    }
}

/// Airlock and APC `screwdriver_act`: toggles the wire panel. True when handled.
pub fn screwdriver_act(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    if kind != ToolKind::Screwdriver {
        return false;
    }
    let Some(holder) = world.get::<Wires>(target).map(|w| w.holder) else {
        return false;
    };
    let text = match holder {
        WireHolder::Airlock => {
            let opened = with_door(world, target, |door, appearance, _| {
                door.panel_open = !door.panel_open;
                door.apply_look(appearance);
                (door.panel_open, false)
            });
            match opened {
                Some(true) => "Вы открываете панель техобслуживания шлюза.",
                Some(false) => "Вы закрываете панель техобслуживания шлюза.",
                None => return false,
            }
        }
        // shortcut: APC covers (crowbar, `opened`) are not ported, so the cover is always shut
        // and the screwdriver always reaches the wire panel.
        WireHolder::Apc => {
            let open = !panel_open(world, target);
            world.entity_mut(target).insert(PanelOpen(open));
            if open {
                "панель проводов открыта"
            } else {
                "панель проводов закрыта"
            }
        }
        // `crate::construction` handles the vending panel, `crate::machine_frame` the
        // autolathe's.
        // `crate::bots` handles the bot's panel.
        WireHolder::Vending | WireHolder::Autolathe | WireHolder::Mulebot => return false,
    };
    crate::tools::tell(world, actor.client, Span::Notice, text);
    true
}

/// `is_wire_tool(tool) && panel_open` in the holders' `item_interaction`: the wires window.
pub fn wire_tool_interaction(
    world: &mut World,
    actor: Actor,
    held: Entity,
    target: Entity,
) -> bool {
    let wire_tool = matches!(
        tool_of(world, held).map(|t| t.0),
        Some(ToolKind::Wirecutters | ToolKind::Multitool)
    );
    if !wire_tool || world.get::<Wires>(target).is_none() || !panel_open(world, target) {
        return false;
    }
    interact(world, actor.client, actor.mob, target);
    true
}

/// tg `wires.interact(user)`: the holder's shock first, then the window if `interactable`.
pub fn interact(world: &mut World, client: Entity, user: Entity, holder: Entity) {
    let Some(kind) = world.get::<Wires>(holder).map(|w| w.holder) else {
        return;
    };
    let electrified = world.get::<Electrified>(holder).is_some();
    if electrified
        && matches!(kind, WireHolder::Airlock | WireHolder::Vending)
        && shock(world, holder, user, 100.0)
    {
        return;
    }
    if !interactable(world, holder, user) {
        return;
    }
    let title = format!("{} Wires", name_of(world, holder));
    world.write_message(OpenUi::new::<WiresWindow>(client, user, holder, title));
}

/// The area's power for a shock: tg `get_powernet_info_from_source(get_area(src))`, the APC's
/// cell charge and powernet. `None`: no APC, or one without a cell and a powernet.
fn area_power_source(world: &World, holder: Entity) -> Option<(Entity, Option<f64>, bool)> {
    let pos = *world.get::<TilePos>(holder)?;
    let area = world.get_resource::<Areas>()?.area_at(pos)?;
    let apc = world.get::<AreaPower>(area)?.apc?;
    apc_source(world, apc)
}

fn apc_source(world: &World, apc: Entity) -> Option<(Entity, Option<f64>, bool)> {
    let cell = world.get::<Apc>(apc)?.cell.map(|c| c.charge);
    let net = world
        .get_resource::<Grid>()
        .and_then(|g| g.node_net(apc))
        .is_some();
    (cell.is_some() || net).then_some((apc, cell, net))
}

/// `should_electrocute(get_area(holder))` of a carbon.
fn should_electrocute(world: &World, holder: Entity, user: Entity) -> bool {
    world.get::<Inventory>(user).is_some() && area_power_source(world, holder).is_some()
}

/// The holder's `interactable(user)`.
pub fn interactable(world: &World, holder: Entity, user: Entity) -> bool {
    let Some(wires) = world.get::<Wires>(holder) else {
        return false;
    };
    let electrified = world.get::<Electrified>(holder).is_some();
    match wires.holder {
        WireHolder::Airlock => {
            let Some(door) = world.get::<Door>(holder) else {
                return false;
            };
            let powered = door.has_power(world.get::<Unpowered>(holder).is_none());
            if electrified && powered && should_electrocute(world, holder, user) {
                return false;
            }
            door.panel_open
        }
        WireHolder::Apc | WireHolder::Autolathe => panel_open(world, holder),
        WireHolder::Mulebot => crate::bots::mule::interactable(world, holder),
        WireHolder::Vending => {
            if electrified && should_electrocute(world, holder, user) {
                return false;
            }
            panel_open(world, holder)
        }
    }
}

/// tg `ELECTROCUTE_DAMAGE(energy)`.
fn electrocute_damage(energy: f64, rng: &mut CombatRng) -> f32 {
    if energy < 1000.0 {
        return 0.0;
    }
    let base = (20.0 + (energy / JOULES_PER_DAMAGE).round()).clamp(20.0, 195.0) as f32;
    base + rng.range(0, 10) as f32 - 5.0
}

/// The holder's `shock(user, chance)`: per holder checks, then `/obj/shock` ->
/// `electrocute_mob`. True when a shock went through.
pub fn shock(world: &mut World, holder: Entity, mob: Entity, chance: f32) -> bool {
    let Some(kind) = world.get::<Wires>(holder).map(|w| w.holder) else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let unpowered = world.get::<Unpowered>(holder).is_some();
    let (source, siemens) = match kind {
        WireHolder::Airlock => {
            let Some(door) = world.get::<Door>(holder) else {
                return false;
            };
            // tg also gives the mob `TRAIT_AIRLOCK_SHOCKIMMUNE` for 1 s, which the 1 s
            // cooldown of the airlock already covers.
            if !door.has_power(!unpowered) || now < door.shock_ready_at {
                return false;
            }
            (area_power_source(world, holder), 1.0)
        }
        // `autolathe/shock`: `siemens_coeff` 0.7 too.
        WireHolder::Vending | WireHolder::Autolathe => {
            if unpowered {
                return false;
            }
            (area_power_source(world, holder), VENDING_SIEMENS)
        }
        // MULEbots never shock (no `shock` override, no electrified state).
        WireHolder::Mulebot => return false,
        // `get_cell()`: the APC's own cell.
        WireHolder::Apc => (
            world
                .get::<Apc>(holder)
                .and_then(|a| a.cell)
                .map(|c| (holder, Some(c.charge), false)),
            1.0,
        ),
    };
    if world.get::<Stat>(mob).is_none() || !world.resource_mut::<CombatRng>().chance(chance) {
        return false;
    }
    // shortcut: no sparks (`do_sparks`).
    let delivered = electrocute_mob(world, mob, source, holder, siemens);
    if delivered
        && kind == WireHolder::Airlock
        && let Some(mut door) = world.get_mut::<Door>(holder)
    {
        door.shock_ready_at = now + SHOCK_COOLDOWN_SECS;
    }
    delivered
}

/// tg `electrocute_mob(victim, power_source, source, siemens_coeff, dist_check = TRUE)`.
fn electrocute_mob(
    world: &mut World,
    victim: Entity,
    power: Option<(Entity, Option<f64>, bool)>,
    source: Entity,
    siemens: f32,
) -> bool {
    let (Some(&vpos), Some(&spos)) = (world.get::<TilePos>(victim), world.get::<TilePos>(source))
    else {
        return false;
    };
    // `iscarbon` and `in_range`.
    if world.get::<Inventory>(victim).is_none()
        || vpos.z != spos.z
        || (vpos.x - spos.x).abs() > 1
        || (vpos.y - spos.y).abs() > 1
    {
        return false;
    }
    let gloves = world
        .get::<Inventory>(victim)
        .and_then(|i| i.gloves)
        .map(|g| {
            world
                .get::<DmClothing>(g)
                .map_or(1.0, |c| c.siemens_coefficient)
        });
    if gloves == Some(0.0) {
        return false;
    }
    let Some((apc, cell, net)) = power else {
        return false;
    };
    let net_id = net
        .then(|| world.resource::<Grid>().node_net(apc))
        .flatten();
    let avail = net_id
        .and_then(|n| world.resource::<Grid>().net(n))
        .map_or(0.0, |n| n.avail);
    let (net_damage, cell_damage) = {
        let mut rng = world.resource_mut::<CombatRng>();
        (
            net_id.map_or(0.0, |_| {
                electrocute_damage(avail / MACHINE_TICK_SECS, &mut rng)
            }),
            cell.map_or(0.0, |c| {
                electrocute_damage(c / (0.001 * STANDARD_CELL_CHARGE).max(1.0), &mut rng)
            }),
        )
    };
    let from_net = net_damage >= cell_damage;
    let damage = net_damage.max(cell_damage);
    let drained = electrocute_act(
        world,
        victim,
        damage,
        source,
        siemens * gloves.unwrap_or(1.0),
    );
    let energy = f64::from(drained) * 20.0;
    if energy > 0.0 {
        if from_net {
            if let Some(net) =
                net_id.and_then(|n| world.resource_mut::<Grid>().into_inner().net_mut(n))
            {
                net.delayedload += energy.min((net.newavail - net.delayedload).max(0.0));
            }
        } else if let Some(cell) = world
            .get_mut::<Apc>(apc)
            .and_then(|a| a.into_inner().cell.as_mut())
        {
            cell.take(energy, false);
        }
    }
    drained > 0.0
}

/// tg `/mob/living/carbon/human/electrocute_act` (the gloves are already in `siemens`): burn
/// over the body, stun (paralysis when already incapacitated), paralysis 2 s later. Returns the
/// damage dealt (0: blocked or too weak).
// shortcut: no jitter, stutter, `force_say`, heart restarts or shocks passed on through pulls.
pub(crate) fn electrocute_act(
    world: &mut World,
    mob: Entity,
    damage: f32,
    source: Entity,
    siemens: f32,
) -> f32 {
    let damage = damage * siemens;
    if damage < 1.0 {
        return 0.0;
    }
    let Some(mut stat) = world.get::<Stat>(mob).copied() else {
        return 0.0;
    };
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.adjust(DamageType::Burn, damage);
        let health = health.clone();
        if update_stat(&health, &mut stat).is_some() {
            *world.get_mut::<Stat>(mob).expect("checked above") = stat;
        }
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let incapacitated = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(|e| e.incapacitated());
    let kind = if incapacitated {
        Incap::Paralyze
    } else {
        Incap::Stun
    };
    crate::status_effects::apply(world, mob, kind, SHOCK_STUN_SECS);
    world.entity_mut(mob).insert(SecondaryShock {
        at: now + SECONDARY_SHOCK_DELAY,
        secs: SHOCK_STUN_SECS * SECONDARY_SHOCK_FACTOR,
    });
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let (source_name, mob_name) = (name_of(world, source), name_of(world, mob));
        crate::security::show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{source_name} ударяет током {mob_name}!"),
            ),
            &[(
                mob,
                Span::Danger,
                "Вы чувствуете мощный разряд, проходящий через всё ваше тело!".to_owned(),
            )],
        );
    }
    damage
}

/// Bare-hand clicks and bumps on wire holders: an open panel opens the wires
/// (`attempt_wire_interaction`), an electrified holder shocks (`attack_hand`, `bumpopen`,
/// vending `interact`); a shock that does not go through lets the touch do what it does
/// otherwise.
fn touches(
    world: &mut World,
    mut bumps: Local<MessageCursor<Bumped>>,
    mut clicks: Local<MessageCursor<ClickResolved>>,
) {
    let bumped: Vec<(Entity, Entity)> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .map(|b| (b.entity, b.obstacle))
        .collect();
    for (mob, obstacle) in bumped {
        let door = world
            .get::<crate::doors::DoorFiller>(obstacle)
            .map_or(obstacle, |f| f.0);
        let powered = world.get::<Unpowered>(door).is_none();
        let Some(has_power) = world
            .get::<Door>(door)
            .filter(|_| world.get::<Electrified>(door).is_some())
            .map(|d| d.has_power(powered))
        else {
            continue;
        };
        // `bumpopen`: unpowered airlocks ignore bumps; non-carbons skip the shock.
        if !has_power || world.get::<Stat>(mob).is_none() {
            continue;
        }
        if world.get::<Inventory>(mob).is_some() && shock(world, door, mob, 100.0) {
            continue;
        }
        crate::doors::try_activate(world, door, mob, true);
    }
    let clicked: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicked {
        let Some(kind) = world.get::<Wires>(click.target).map(|w| w.holder) else {
            continue;
        };
        let empty = world
            .get::<Inventory>(click.mob)
            .is_some_and(|i| i.active_item().is_none());
        if !empty {
            continue;
        }
        if panel_open(world, click.target) {
            interact(world, click.client, click.mob, click.target);
            continue;
        }
        if world.get::<Electrified>(click.target).is_none() {
            continue;
        }
        match kind {
            WireHolder::Airlock => {
                if !shock(world, click.target, click.mob, 100.0) {
                    crate::doors::try_activate(world, click.target, click.mob, false);
                }
            }
            WireHolder::Vending => {
                if world.get::<Unpowered>(click.target).is_some()
                    || shock(world, click.target, click.mob, 100.0)
                {
                    continue;
                }
                let title = world
                    .get::<DmName>(click.target)
                    .map(|n| crate::vending::plain_name(&n.name).to_owned())
                    .unwrap_or_default();
                world.write_message(OpenUi::new::<crate::vending::VendingWindow>(
                    click.client,
                    click.mob,
                    click.target,
                    title,
                ));
            }
            WireHolder::Apc | WireHolder::Autolathe | WireHolder::Mulebot => {}
        }
    }
}

/// `/datum/wires/ui_status`: `UI_CLOSE` unless `interactable`.
fn wire_window_status(world: &mut World) {
    let mut windows = world.query::<(Entity, &UiWindow)>();
    let open: Vec<(Entity, Entity, Entity)> = windows
        .iter(world)
        .filter(|(_, w)| w.interface == WiresUi::NAME)
        .map(|(e, w)| (e, w.source, w.actor))
        .collect();
    for (window, source, actor) in open {
        if !interactable(world, source, actor) {
            world
                .get_mut::<UiWindow>(window)
                .expect("listed above")
                .cap_status(UiStatus::Close);
        }
    }
}

/// `is_holding_tool_quality(kind)`: either hand.
fn holding_tool(world: &World, mob: Entity, kind: ToolKind) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| {
        i.hands
            .iter()
            .flatten()
            .any(|&h| tool_of(world, h).is_some_and(|t| t.0 == kind))
    })
}

/// `ui_act`, every check again at the moment of effect.
fn wire_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<WiresWindow>>>) {
    let pending: Vec<(Entity, Entity, WireAct)> = acts
        .read(world.resource::<Messages<UiActed<WiresWindow>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, holder, act) in pending {
        if let Err(reason) = apply_act(world, actor, holder, &act) {
            debug!("wires act {act:?} on {holder} by {actor} rejected: {reason}");
        }
    }
}

fn apply_act(
    world: &mut World,
    actor: Entity,
    holder: Entity,
    act: &WireAct,
) -> Result<(), &'static str> {
    let (Some(&apos), Some(&hpos)) = (world.get::<TilePos>(actor), world.get::<TilePos>(holder))
    else {
        return Err("gone");
    };
    if world.get::<Stat>(actor) != Some(&Stat::Conscious)
        || distance_status(apos, hpos) != UiStatus::Interactive
    {
        return Err("too far or incapacitated");
    }
    if !interactable(world, holder, actor) {
        return Err("not interactable");
    }
    let color = match act {
        WireAct::Cut(c) | WireAct::Pulse(c) | WireAct::Attach(c) => c,
    };
    let wire = world
        .get::<Wires>(holder)
        .and_then(|w| w.get_wire(color))
        .map(str::to_owned)
        .ok_or("no such color")?;
    match act {
        WireAct::Cut(_) => {
            if !holding_tool(world, actor, ToolKind::Wirecutters) {
                tell_mob(world, actor, Span::Warning, "Вам нужны кусачки!");
                return Err("no wirecutters");
            }
            cut(world, holder, &wire, Some(actor));
        }
        WireAct::Pulse(_) => {
            if !holding_tool(world, actor, ToolKind::Multitool) {
                tell_mob(world, actor, Span::Warning, "Вам нужен мультитул!");
                return Err("no multitool");
            }
            pulse(world, holder, &wire, Some(actor));
        }
        // Nothing is attached and no assemblies exist (module shortcut).
        WireAct::Attach(_) => {}
    }
    Ok(())
}

/// tg `cut(wire, source)`: cuts it, or mends a cut one, then `on_cut`.
pub fn cut(world: &mut World, holder: Entity, wire: &str, source: Option<Entity>) {
    let Some(mut wires) = world.get_mut::<Wires>(holder) else {
        return;
    };
    let mend = wires.is_cut(wire);
    if mend {
        wires.cut.retain(|w| w != wire);
    } else {
        wires.cut.push(wire.to_owned());
    }
    let kind = wires.holder;
    match kind {
        WireHolder::Airlock => airlock_on_cut(world, holder, wire, mend, source),
        WireHolder::Apc => apc_on_cut(world, holder, wire, mend, source),
        WireHolder::Vending => vending_on_cut(world, holder, wire, mend, source),
        WireHolder::Autolathe => autolathe_on_cut(world, holder, wire, mend, source),
        WireHolder::Mulebot => crate::bots::mule::on_cut(world, holder, wire, source),
    }
}

/// tg `pulse(wire, user)`: nothing on a cut wire.
pub fn pulse(world: &mut World, holder: Entity, wire: &str, user: Option<Entity>) {
    let Some(wires) = world.get::<Wires>(holder) else {
        return;
    };
    if wires.is_cut(wire) {
        return;
    }
    match wires.holder {
        WireHolder::Airlock => airlock_on_pulse(world, holder, wire, user),
        WireHolder::Apc => apc_on_pulse(world, holder, wire, user),
        WireHolder::Vending => vending_on_pulse(world, holder, wire, user),
        WireHolder::Autolathe => autolathe_on_pulse(world, holder, wire),
        WireHolder::Mulebot => crate::bots::mule::on_pulse(world, holder, wire),
    }
}

/// tg `emp_pulse()`: in a shuffled order each wire is pulsed at 33%, at most
/// `MAXIMUM_EMP_WIRES` (3) of them.
pub fn emp_pulse(world: &mut World, holder: Entity) {
    let Some(mut wires) = world
        .get::<Wires>(holder)
        .map(|w| w.colors.iter().map(|(_, w)| w.clone()).collect::<Vec<_>>())
    else {
        return;
    };
    let mut pulsed = Vec::new();
    {
        let mut rng = world.resource_mut::<crate::emp::EmpRng>();
        for i in (1..wires.len()).rev() {
            let j = rng.range(0, i as i64) as usize;
            wires.swap(i, j);
        }
        for wire in wires {
            if pulsed.len() == MAXIMUM_EMP_WIRES {
                break;
            }
            if rng.prob(33.0) {
                pulsed.push(wire);
            }
        }
    }
    for wire in pulsed {
        pulse(world, holder, &wire, None);
    }
}

/// `cut_color`/`pulse_color` by color.
pub fn cut_color(world: &mut World, holder: Entity, color: &str, source: Option<Entity>) {
    if let Some(wire) = world
        .get::<Wires>(holder)
        .and_then(|w| w.get_wire(color))
        .map(str::to_owned)
    {
        cut(world, holder, &wire, source);
    }
}

pub fn pulse_color(world: &mut World, holder: Entity, color: &str, user: Option<Entity>) {
    if let Some(wire) = world
        .get::<Wires>(holder)
        .and_then(|w| w.get_wire(color))
        .map(str::to_owned)
    {
        pulse(world, holder, &wire, user);
    }
}

fn shock_user(world: &mut World, holder: Entity, user: Option<Entity>, chance: f32) {
    if let Some(user) = user {
        shock(world, holder, user, chance);
    }
}

fn schedule_reset(world: &mut World, holder: Entity, secs: f64, wire: &'static str) {
    let at = world.resource::<Time>().elapsed_secs_f64() + secs;
    match world.get_mut::<WireResets>(holder) {
        Some(mut resets) => resets.0.push((at, wire)),
        None => {
            world
                .entity_mut(holder)
                .insert(WireResets(vec![(at, wire)]));
        }
    }
}

pub(crate) fn set_electrified(world: &mut World, holder: Entity, value: Option<Electrified>) {
    let mut e = world.entity_mut(holder);
    match value {
        Some(v) => {
            e.insert(v);
        }
        None => {
            e.remove::<Electrified>();
        }
    }
}

/// tg airlock `bolt()` / `unbolt()`.
pub(crate) fn set_bolts(world: &mut World, door: Entity, bolt: bool) {
    let changed = with_door(world, door, |d, appearance, _| {
        let changed = d.locked != bolt;
        d.locked = bolt;
        d.apply_look(appearance);
        (changed, false)
    });
    if changed == Some(true)
        && let Some(&pos) = world.get::<TilePos>(door)
    {
        let sounds = world.get::<Door>(door).map(|d| d.sounds.clone());
        if let Some(sounds) = sounds {
            let file = if bolt {
                &sounds.bolt_down
            } else {
                &sounds.bolt_up
            };
            let sound = crate::sound::PlaySound::new(pos, file.as_str(), 30.0, false);
            crate::sound::playsound(world, sound.extrarange(3.0));
        }
        hear(world, pos, "Вы слышите щелчок у основания двери.");
    }
}

/// tg `loseMainPower` / `loseBackupPower` / `regain*`.
fn set_outage(world: &mut World, door: Entity, f: impl FnOnce(&mut Door)) {
    let pending = with_door(world, door, |d, appearance, _| {
        f(d);
        d.apply_look(appearance);
        (d.main_outage > 0.0 || d.backup_outage > 0.0, false)
    });
    if pending == Some(true) {
        world.entity_mut(door).insert(PowerOutage);
    }
}

/// tg airlock `loseMainPower()` / `loseBackupPower()` (the AI's "Disrupt" buttons).
pub(crate) fn lose_power(world: &mut World, door: Entity, main: bool) {
    set_outage(world, door, |d| {
        if main {
            lose_main_power(d);
        } else {
            lose_backup_power(d);
        }
    });
}

fn lose_main_power(d: &mut Door) {
    if d.main_outage <= 0.0 {
        d.main_outage = MAIN_OUTAGE_SECS;
        if d.backup_outage < BACKUP_AFTER_MAIN_SECS {
            d.backup_outage = BACKUP_AFTER_MAIN_SECS;
        }
    }
}

fn lose_backup_power(d: &mut Door) {
    if d.backup_outage < BACKUP_OUTAGE_SECS {
        d.backup_outage = BACKUP_OUTAGE_SECS;
    }
}

fn ai_cut(state: i8, mend: bool) -> i8 {
    match (mend, state) {
        (true, AI_WIRE_DISABLED) => AI_WIRE_NORMAL,
        (true, AI_WIRE_HACKED) => AI_WIRE_DISABLED_HACKED,
        (false, AI_WIRE_NORMAL) => AI_WIRE_DISABLED,
        (false, AI_WIRE_DISABLED_HACKED) => AI_WIRE_HACKED,
        (_, s) => s,
    }
}

/// `/datum/wires/airlock/on_cut`.
fn airlock_on_cut(world: &mut World, door: Entity, wire: &str, mend: bool, source: Option<Entity>) {
    let wires = world.get::<Wires>(door).cloned().expect("caller checked");
    match wire {
        WIRE_POWER1 | WIRE_POWER2 => {
            let both = !wires.is_cut(WIRE_POWER1) && !wires.is_cut(WIRE_POWER2);
            set_outage(world, door, |d| {
                if mend && both {
                    d.main_outage = 0.0;
                } else {
                    lose_main_power(d);
                }
            });
            shock_user(world, door, source, 50.0);
        }
        WIRE_BACKUP1 | WIRE_BACKUP2 => {
            let both = !wires.is_cut(WIRE_BACKUP1) && !wires.is_cut(WIRE_BACKUP2);
            set_outage(world, door, |d| {
                if mend && both {
                    d.backup_outage = 0.0;
                } else {
                    lose_backup_power(d);
                }
            });
            shock_user(world, door, source, 50.0);
        }
        WIRE_BOLTS if !mend => set_bolts(world, door, true),
        WIRE_AI => {
            if let Some(mut d) = world.get_mut::<Door>(door) {
                d.ai_control_disabled = ai_cut(d.ai_control_disabled, mend);
            }
        }
        WIRE_SHOCK => {
            if let Some(source) = source {
                info!(
                    "{} {} shocking for airlock {door}",
                    name_of(world, source),
                    if mend { "disabled" } else { "enabled" }
                );
            }
            if mend {
                set_electrified(world, door, None);
            } else {
                set_electrified(world, door, Some(Electrified::Permanent));
                shock_user(world, door, source, 100.0);
            }
        }
        WIRE_SAFETY => {
            if let Some(mut d) = world.get_mut::<Door>(door) {
                d.safe = mend;
            }
        }
        WIRE_TIMING => {
            with_door(world, door, |d, appearance, ctx| {
                d.autoclose = mend;
                if !mend {
                    // tg `autoclose()` checks `autoclose` when its timer fires.
                    d.close_at_clear();
                    return ((), false);
                }
                let closing = d.state == DoorState::Open && ctx.close(d, appearance, false);
                ((), closing)
            });
        }
        WIRE_FEEDBACK => {
            with_door(world, door, |d, appearance, _| {
                d.feedback = mend;
                d.apply_look(appearance);
                ((), false)
            });
        }
        WIRE_ZAP1 | WIRE_ZAP2 => shock_user(world, door, source, 50.0),
        WIRE_UNRESTRICTED_EXIT => {
            let side = if mend {
                [1u8, 2, 4, 8][world.resource_mut::<CombatRng>().range(0, 3) as usize]
            } else {
                0
            };
            if let Some(mut d) = world.get_mut::<Door>(door)
                && d.unres_latch
            {
                d.unres_sides = side;
            }
        }
        _ => {}
    }
}

/// `/datum/wires/airlock/on_pulse`.
fn airlock_on_pulse(world: &mut World, door: Entity, wire: &str, user: Option<Entity>) {
    match wire {
        WIRE_POWER1 | WIRE_POWER2 => set_outage(world, door, lose_main_power),
        WIRE_BACKUP1 | WIRE_BACKUP2 => set_outage(world, door, lose_backup_power),
        WIRE_OPEN => {
            // Only without an ID check (cut ID scan) or on a door that needs no access.
            with_door(world, door, |d, appearance, ctx| {
                let free = !d.requires_id(ctx.env.wires) || d.allows(None);
                let moved = free
                    && match d.state {
                        DoorState::Closed => ctx.open(d, appearance, true),
                        DoorState::Open => ctx.close(d, appearance, true),
                        _ => false,
                    };
                ((), moved)
            });
        }
        WIRE_BOLTS => {
            let Some((locked, has_power)) = world.get::<Door>(door).map(|d| {
                (
                    d.locked,
                    d.has_power(world.get::<Unpowered>(door).is_none()),
                )
            }) else {
                return;
            };
            // Drops them always, raises them only with power.
            if !locked {
                set_bolts(world, door, true);
            } else if has_power {
                set_bolts(world, door, false);
            }
        }
        WIRE_IDSCAN => {
            with_door(world, door, |d, appearance, ctx| {
                let flash = d.has_power(ctx.env.powered) && d.state == DoorState::Closed;
                let active = flash && ctx.deny(d, appearance);
                ((), active)
            });
        }
        WIRE_AI => {
            if let Some(mut d) = world.get_mut::<Door>(door) {
                d.ai_control_disabled = ai_cut(d.ai_control_disabled, false);
            }
            schedule_reset(world, door, AI_RESET_SECS, WIRE_AI);
        }
        WIRE_SHOCK => {
            if world.get::<Electrified>(door).is_none() {
                set_electrified(
                    world,
                    door,
                    Some(Electrified::Timed(MACHINE_DEFAULT_ELECTRIFY_TIME)),
                );
            }
            shock_user(world, door, user, 100.0);
        }
        WIRE_SAFETY => {
            with_door(world, door, |d, appearance, ctx| {
                d.safe = !d.safe;
                let closing = d.state == DoorState::Open && ctx.close(d, appearance, false);
                ((), closing)
            });
        }
        WIRE_TIMING => {
            if let Some(mut d) = world.get_mut::<Door>(door) {
                d.normalspeed = !d.normalspeed;
            }
        }
        WIRE_FEEDBACK => {
            with_door(world, door, |d, appearance, _| {
                d.feedback = !d.feedback;
                d.apply_look(appearance);
                ((), false)
            });
        }
        WIRE_UNRESTRICTED_EXIT => {
            if let Some(mut d) = world.get_mut::<Door>(door)
                && d.unres_latch
            {
                d.unres_sides = reverse_dir(d.unres_sides);
            }
        }
        _ => {}
    }
}

/// tg APC `update()` after a wire changed its state: the area follows at once.
pub(crate) fn apc_update(world: &mut World, apc: Entity) {
    let Some(state) = world.get::<Apc>(apc).cloned() else {
        return;
    };
    let changed = world
        .get_mut::<AreaPower>(state.area)
        .is_some_and(|mut area| state.update(&mut area));
    if changed {
        world.write_message(AreaPowerChanged(state.area));
    }
}

pub(crate) fn apc_channel(
    world: &mut World,
    apc: Entity,
    channel: Channel,
    value: impl FnOnce(u8) -> u8,
) {
    if let Some(mut a) = world.get_mut::<Apc>(apc) {
        let i = channel as usize;
        a.channels[i] = value(a.channels[i]);
    }
    apc_update(world, apc);
}

fn apc_wire_channel(wire: &str) -> Option<Channel> {
    match wire {
        WIRE_EQUIPMENT => Some(Channel::Equip),
        WIRE_LIGHT => Some(Channel::Light),
        WIRE_ENVIRONMENT => Some(Channel::Environ),
        _ => None,
    }
}

/// `/datum/wires/apc/on_cut`.
fn apc_on_cut(world: &mut World, apc: Entity, wire: &str, mend: bool, source: Option<Entity>) {
    if let Some(channel) = apc_wire_channel(wire) {
        let value = if mend { CHANNEL_AUTO_ON } else { CHANNEL_OFF };
        apc_channel(world, apc, channel, |_| value);
        return;
    }
    let wires = world.get::<Wires>(apc).cloned().expect("caller checked");
    match wire {
        WIRE_POWER1 | WIRE_POWER2 => {
            let shorted = !(mend && !wires.is_cut(WIRE_POWER1) && !wires.is_cut(WIRE_POWER2));
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.shorted = shorted;
            }
            apc_update(world, apc);
            shock_user(world, apc, source, 50.0);
        }
        WIRE_INTERFACE => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.locked = !mend;
            }
        }
        WIRE_AI => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.aidisabled = !mend;
            }
        }
        _ => {}
    }
}

/// `/datum/wires/apc/on_pulse`.
fn apc_on_pulse(world: &mut World, apc: Entity, wire: &str, _user: Option<Entity>) {
    if let Some(channel) = apc_wire_channel(wire) {
        apc_channel(world, apc, channel, |v| {
            if v > CHANNEL_OFF {
                CHANNEL_OFF
            } else {
                CHANNEL_AUTO_ON
            }
        });
        return;
    }
    match wire {
        WIRE_POWER1 | WIRE_POWER2 => {
            let was = world.get::<Apc>(apc).is_some_and(|a| a.shorted);
            if !was {
                if let Some(mut a) = world.get_mut::<Apc>(apc) {
                    a.shorted = true;
                }
                apc_update(world, apc);
                schedule_reset(world, apc, APC_SHORT_SECS, WIRE_POWER1);
            }
        }
        // tg schedules `reset(WIRE_INTERFACE)`, which no case of `reset()` matches: the APC
        // stays unlocked until something locks it.
        WIRE_INTERFACE => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.locked = false;
            }
        }
        WIRE_AI => {
            let was = world.get::<Apc>(apc).is_some_and(|a| a.aidisabled);
            if !was {
                if let Some(mut a) = world.get_mut::<Apc>(apc) {
                    a.aidisabled = true;
                }
                schedule_reset(world, apc, AI_RESET_SECS, WIRE_AI);
            }
        }
        _ => {}
    }
}

fn hack_mut(world: &mut World, vendor: Entity, f: impl FnOnce(&mut VendorHack)) {
    let Some(mut hack) = world.get_mut::<VendorHack>(vendor) else {
        return;
    };
    f(&mut hack);
    let shoots = hack.shoot_inventory;
    let mut e = world.entity_mut(vendor);
    if shoots {
        e.insert(ShootsInventory);
    } else {
        e.remove::<ShootsInventory>();
    }
}

/// `/datum/wires/vending/on_cut`.
fn vending_on_cut(
    world: &mut World,
    vendor: Entity,
    wire: &str,
    mend: bool,
    source: Option<Entity>,
) {
    match wire {
        WIRE_THROW => hack_mut(world, vendor, |h| h.shoot_inventory = !mend),
        WIRE_CONTRABAND => {
            if let Some(mut v) = world.get_mut::<Vendor>(vendor) {
                v.extended_inventory = false;
            }
        }
        WIRE_SHOCK => {
            if mend {
                set_electrified(world, vendor, None);
            } else {
                set_electrified(world, vendor, Some(Electrified::Permanent));
                shock_user(world, vendor, source, 100.0);
            }
        }
        WIRE_IDSCAN => hack_mut(world, vendor, |h| h.scan_id = mend),
        WIRE_SPEAKER => hack_mut(world, vendor, |h| h.shut_up = mend),
        _ => {}
    }
}

/// `/datum/wires/vending/on_pulse`.
fn vending_on_pulse(world: &mut World, vendor: Entity, wire: &str, user: Option<Entity>) {
    match wire {
        WIRE_THROW => hack_mut(world, vendor, |h| h.shoot_inventory = !h.shoot_inventory),
        WIRE_CONTRABAND => {
            if let Some(mut v) = world.get_mut::<Vendor>(vendor) {
                v.extended_inventory = !v.extended_inventory;
            }
        }
        WIRE_SHOCK => {
            set_electrified(
                world,
                vendor,
                Some(Electrified::Timed(MACHINE_DEFAULT_ELECTRIFY_TIME)),
            );
            shock_user(world, vendor, user, 100.0);
        }
        WIRE_IDSCAN => hack_mut(world, vendor, |h| h.scan_id = !h.scan_id),
        // Speaker: the language cycle (module shortcut in `status`).
        _ => {}
    }
}

/// tg vending `throw_item()`: a random stocked product flies at the first living mob in view.
fn throw_item(world: &mut World, vendor: Entity) {
    let Some(&pos) = world.get::<TilePos>(vendor) else {
        return;
    };
    let target = world
        .resource::<SpatialIndex>()
        .in_radius(pos, SHOOT_VIEW)
        .find(|&e| world.get::<Health>(e).is_some());
    let Some(target) = target else {
        return;
    };
    let Some(&target_pos) = world.get::<TilePos>(target) else {
        return;
    };
    let Some(machine) = world.get::<Vendor>(vendor) else {
        return;
    };
    let mut stocked: Vec<usize> = machine
        .records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RecordKind::Product && r.amount > 0)
        .map(|(i, _)| i)
        .collect();
    if stocked.is_empty() {
        return;
    }
    let pick = world
        .resource_mut::<CombatRng>()
        .range(0, stocked.len() as u32 - 1) as usize;
    let record = stocked.swap_remove(pick);
    // "Always throw new stuff that costs before free returned stuff".
    let only_returned = world
        .get::<Vendor>(vendor)
        .is_some_and(|v| v.records[record].amount as usize <= v.records[record].returned.len());
    let Some(item) = crate::vending::dispense(world, vendor, record, pos, true, only_returned)
    else {
        return;
    };
    let now_ds = world.resource::<Time>().elapsed_secs_f64() * 10.0;
    let Some(path) =
        crate::throw::Trajectory::new(pos, target_pos, SHOOT_RANGE, SHOOT_SPEED, now_ds)
    else {
        return;
    };
    world.entity_mut(item).insert((
        ss13_net::server::Networked,
        pos,
        crate::throw::Thrown {
            path,
            thrower: None,
            target: Some(target),
            last_dir: None,
        },
    ));
    let text = format!(
        "{} швыряет {} в {}!",
        name_of(world, vendor),
        name_of(world, item),
        name_of(world, target)
    );
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, text),
        &[],
    );
}

/// Countdowns: airlock power outages (paused while their wires are cut, tg
/// `handle_main_power`), electrification (airlocks per second, vendors per machine tick while
/// powered), timed resets, shooting vendors, second shocks.
fn tick_wires(world: &mut World) {
    let dt = f64::from(world.resource::<Time>().delta_secs());
    let now = world.resource::<Time>().elapsed_secs_f64();

    let mut outages =
        world.query_filtered::<(Entity, &mut Door, &Wires, &mut Appearance), With<PowerOutage>>();
    let mut done = Vec::new();
    for (e, mut door, wires, mut appearance) in outages.iter_mut(world) {
        let was = door.wire_power();
        if door.main_outage > 0.0 && !wires.is_cut(WIRE_POWER1) && !wires.is_cut(WIRE_POWER2) {
            door.main_outage = (door.main_outage - dt).max(0.0);
        }
        if door.backup_outage > 0.0 && !wires.is_cut(WIRE_BACKUP1) && !wires.is_cut(WIRE_BACKUP2) {
            door.backup_outage = (door.backup_outage - dt).max(0.0);
        }
        if was != door.wire_power() {
            door.apply_look(&mut appearance);
        }
        if door.main_outage <= 0.0 && door.backup_outage <= 0.0 {
            done.push(e);
        }
    }
    for e in done {
        world.entity_mut(e).remove::<PowerOutage>();
    }

    let mut electrified = world.query::<(Entity, &Electrified, Option<&Door>, Has<Unpowered>)>();
    let timed: Vec<(Entity, f64, bool, bool)> = electrified
        .iter(world)
        .filter_map(|(e, el, door, unpowered)| match el {
            Electrified::Timed(left) => Some((e, *left, door.is_some(), unpowered)),
            Electrified::Permanent => None,
        })
        .collect();
    for (e, left, is_door, unpowered) in timed {
        // Vendors count `process()` calls, which stop without power (`is_operational`).
        let step = if is_door {
            dt
        } else if unpowered {
            0.0
        } else {
            dt / MACHINE_TICK_SECS
        };
        let left = left - step;
        set_electrified(world, e, (left > 0.0).then_some(Electrified::Timed(left)));
    }

    let mut resets = world.query::<(Entity, &WireResets)>();
    let due: Vec<(Entity, Vec<&'static str>, bool)> = resets
        .iter(world)
        .filter(|(_, r)| r.0.iter().any(|(at, _)| *at <= now))
        .map(|(e, r)| {
            let wires =
                r.0.iter()
                    .filter(|(at, _)| *at <= now)
                    .map(|(_, w)| *w)
                    .collect();
            (e, wires, r.0.iter().all(|(at, _)| *at <= now))
        })
        .collect();
    for (e, wires, all) in due {
        if all {
            world.entity_mut(e).remove::<WireResets>();
        } else if let Some(mut r) = world.get_mut::<WireResets>(e) {
            r.0.retain(|(at, _)| *at > now);
        }
        for wire in wires {
            reset(world, e, wire);
        }
    }

    let mut shooters =
        world.query_filtered::<Entity, (With<ShootsInventory>, Without<Unpowered>)>();
    let shooters: Vec<Entity> = shooters.iter(world).collect();
    // `SPT_PROB(shoot_inventory_chance, seconds_per_tick)` per machine tick, as a rate.
    let chance = (1.0 - (1.0 - SHOOT_CHANCE / 100.0).powf(dt / MACHINE_TICK_SECS)) * 100.0;
    for vendor in shooters {
        if world.resource_mut::<CombatRng>().chance(chance as f32) {
            throw_item(world, vendor);
        }
    }

    let mut seconds = world.query::<(Entity, &SecondaryShock)>();
    let due: Vec<(Entity, f32)> = seconds
        .iter(world)
        .filter(|(_, s)| s.at <= now)
        .map(|(e, s)| (e, s.secs))
        .collect();
    for (mob, secs) in due {
        world.entity_mut(mob).remove::<SecondaryShock>();
        crate::status_effects::apply(world, mob, Incap::Paralyze, secs);
    }
}

/// `/datum/wires/autolathe/on_cut`: the wire's state follows the cut; zap shocks.
fn autolathe_on_cut(
    world: &mut World,
    lathe: Entity,
    wire: &str,
    mend: bool,
    source: Option<Entity>,
) {
    match wire {
        WIRE_HACK => crate::science::autolathe::set_hacked(world, lathe, !mend),
        WIRE_SHOCK | WIRE_DISABLE => {
            if let Some(mut a) = world.get_mut::<crate::science::autolathe::Autolathe>(lathe) {
                if wire == WIRE_SHOCK {
                    a.shocked = !mend;
                } else {
                    a.disabled = !mend;
                }
            }
        }
        WIRE_ZAP => shock_user(world, lathe, source, 50.0),
        _ => {}
    }
}

/// `/datum/wires/autolathe/on_pulse`: hack, shock and disable flip for 6 s.
fn autolathe_on_pulse(world: &mut World, lathe: Entity, wire: &str) {
    let Some(mut a) = world.get_mut::<crate::science::autolathe::Autolathe>(lathe) else {
        return;
    };
    let wire: &'static str = match wire {
        WIRE_HACK => {
            a.hacked = !a.hacked;
            WIRE_HACK
        }
        WIRE_SHOCK => {
            a.shocked = !a.shocked;
            WIRE_SHOCK
        }
        WIRE_DISABLE => {
            a.disabled = !a.disabled;
            WIRE_DISABLE
        }
        _ => return,
    };
    schedule_reset(world, lathe, AUTOLATHE_RESET_SECS, wire);
}

/// `addtimer(CALLBACK(A, reset, wire), 6 SECONDS)`.
const AUTOLATHE_RESET_SECS: f64 = 6.0;

/// tg airlock `reset_ai_wire`, APC and autolathe `reset(wire)`.
fn reset(world: &mut World, holder: Entity, wire: &'static str) {
    let Some(wires) = world.get::<Wires>(holder).cloned() else {
        return;
    };
    if wires.holder == WireHolder::Autolathe {
        // `reset(wire)`: back to off unless the wire is cut.
        if !wires.is_cut(wire)
            && let Some(mut a) = world.get_mut::<crate::science::autolathe::Autolathe>(holder)
        {
            match wire {
                WIRE_HACK => a.hacked = false,
                WIRE_SHOCK => a.shocked = false,
                WIRE_DISABLE => a.disabled = false,
                _ => {}
            }
        }
        return;
    }
    match (wires.holder, wire) {
        (WireHolder::Airlock, WIRE_AI) => {
            if let Some(mut d) = world.get_mut::<Door>(holder) {
                d.ai_control_disabled = match d.ai_control_disabled {
                    AI_WIRE_DISABLED => AI_WIRE_NORMAL,
                    AI_WIRE_HACKED => AI_WIRE_DISABLED_HACKED,
                    s => s,
                };
            }
        }
        (WireHolder::Apc, WIRE_POWER1) => {
            if !wires.is_cut(WIRE_POWER1) && !wires.is_cut(WIRE_POWER2) {
                if let Some(mut a) = world.get_mut::<Apc>(holder) {
                    a.shorted = false;
                }
                apc_update(world, holder);
            }
        }
        (WireHolder::Apc, WIRE_AI) => {
            if !wires.is_cut(WIRE_AI)
                && let Some(mut a) = world.get_mut::<Apc>(holder)
            {
                a.aidisabled = false;
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_deterministic_per_seed_and_key() {
        let set = airlock_set("/area/station/maintenance/fore");
        assert_eq!(set.key, "/datum/wires/airlock/maint");
        assert_eq!(set.wires.len(), 17);
        assert_eq!(&set.wires[15..], ["__dud1", "__dud0"]);
        let a = colors_for(42, &set);
        assert_eq!(a, colors_for(42, &set), "same seed, same colors");
        assert_ne!(a, colors_for(43, &set), "another round, other colors");
        // Every wire has one color, every color one wire (17 wires, 17 colors).
        let mut wires: Vec<&str> = a.iter().map(|(_, w)| w.as_str()).collect();
        wires.sort_unstable();
        let mut expected: Vec<&str> = set.wires.iter().map(String::as_str).collect();
        expected.sort_unstable();
        assert_eq!(wires, expected);
        let mut colors: Vec<&str> = a.iter().map(|(c, _)| *c).collect();
        colors.sort_unstable();
        colors.dedup();
        assert_eq!(colors.len(), 17);
        // Keys are independent: the generic airlock set differs from the maintenance one.
        let generic = airlock_set("/area/station/hallway");
        assert_eq!(generic.proper_name, "Generic Airlock");
        assert_ne!(colors_for(42, &generic), a);
        // The directory hands every holder of a key the same colors.
        let mut dir = WireColors {
            seed: 42,
            ..default()
        };
        let one = dir.wires_for(WireHolder::Airlock, &set);
        let two = dir.wires_for(WireHolder::Airlock, &set);
        assert_eq!(one.colors, two.colors);
        assert_eq!(one.colors, a);
        assert_eq!(apc_set().wires.len(), 13);
        assert_eq!(vending_set().wires.len(), 6);
    }

    fn world_with_airlock(req_access: &[&str]) -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(SpatialIndex::default());
        world.insert_resource(CombatRng(1));
        let mut colors = WireColors {
            seed: 7,
            ..default()
        };
        let wires = colors.wires_for(WireHolder::Airlock, &airlock_set("/area/station/hallway"));
        let door = world
            .spawn((
                crate::doors::test_airlock(req_access),
                wires,
                Appearance {
                    icon_state: ss13_shared::IconStateId(0),
                    layer: ss13_shared::Layer(3.0),
                    plane: 0,
                    color: None,
                    pixel_offset: [0, 0],
                    rotation: 0,
                    overlays: vec![],
                    mouse_opacity: default(),
                },
                TilePos::new(5, 5, 1),
            ))
            .id();
        (world, door)
    }

    fn door(world: &World, e: Entity) -> &Door {
        world.get::<Door>(e).unwrap()
    }

    #[test]
    fn airlock_cut_and_pulse_effects() {
        let (mut world, e) = world_with_airlock(&["maint_tunnels"]);
        // Bolts: a pulse drops them, a second raises them (powered).
        pulse(&mut world, e, WIRE_BOLTS, None);
        assert!(door(&world, e).locked);
        pulse(&mut world, e, WIRE_BOLTS, None);
        assert!(!door(&world, e).locked);

        // The open wire does nothing while the door checks IDs.
        pulse(&mut world, e, WIRE_OPEN, None);
        assert_eq!(door(&world, e).state, DoorState::Closed);
        // With the ID scan cut nothing opens it by hand, but the open wire does.
        cut(&mut world, e, WIRE_IDSCAN, None);
        assert!(
            !world
                .get::<Door>(e)
                .unwrap()
                .requires_id(world.get::<Wires>(e))
        );
        pulse(&mut world, e, WIRE_OPEN, None);
        assert!(matches!(door(&world, e).state, DoorState::Opening { .. }));

        // A cut wire cannot be pulsed; mending restores it.
        cut(&mut world, e, WIRE_BOLTS, None);
        assert!(door(&world, e).locked, "cutting the bolts drops them");
        let before = door(&world, e).locked;
        pulse(&mut world, e, WIRE_BOLTS, None);
        assert_eq!(door(&world, e).locked, before);
        cut(&mut world, e, WIRE_BOLTS, None);
        assert!(!world.get::<Wires>(e).unwrap().is_cut(WIRE_BOLTS));

        // Main power: a pulse starts a 60 s outage with 10 s of backup outage; the door
        // still has power from backup after that.
        pulse(&mut world, e, WIRE_POWER1, None);
        assert_eq!(door(&world, e).main_outage, 60.0);
        assert_eq!(door(&world, e).backup_outage, 10.0);
        assert!(!door(&world, e).has_power(true));
        assert!(world.get::<PowerOutage>(e).is_some());
        // Cutting both power wires and a backup wire: all power cut.
        cut(&mut world, e, WIRE_POWER2, None);
        cut(&mut world, e, WIRE_BACKUP1, None);
        assert!(Door::all_power_cut(world.get::<Wires>(e).unwrap()));

        // Shock: cut -> permanently electrified, mend -> not; a pulse electrifies for 30 s.
        cut(&mut world, e, WIRE_SHOCK, None);
        assert_eq!(world.get::<Electrified>(e), Some(&Electrified::Permanent));
        cut(&mut world, e, WIRE_SHOCK, None);
        assert!(world.get::<Electrified>(e).is_none());
        pulse(&mut world, e, WIRE_SHOCK, None);
        assert_eq!(world.get::<Electrified>(e), Some(&Electrified::Timed(30.0)));

        // Safety, timing, feedback, AI.
        cut(&mut world, e, WIRE_SAFETY, None);
        assert!(!door(&world, e).safe);
        pulse(&mut world, e, WIRE_TIMING, None);
        assert!(!door(&world, e).normalspeed);
        cut(&mut world, e, WIRE_FEEDBACK, None);
        assert!(!door(&world, e).feedback);
        cut(&mut world, e, WIRE_AI, None);
        assert_eq!(door(&world, e).ai_control_disabled, AI_WIRE_DISABLED);
    }

    #[test]
    fn idscan_pulse_flashes_and_open_wire_needs_power() {
        let (mut world, e) = world_with_airlock(&[]);
        pulse(&mut world, e, WIRE_IDSCAN, None);
        assert!(door(&world, e).denied(), "the red lights flash");
        // A door without access opens on the open wire even with power out (forced).
        cut(&mut world, e, WIRE_POWER1, None);
        cut(&mut world, e, WIRE_BACKUP1, None);
        pulse(&mut world, e, WIRE_OPEN, None);
        assert!(matches!(door(&world, e).state, DoorState::Opening { .. }));
    }

    #[test]
    fn apc_and_vending_effects() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(SpatialIndex::default());
        world.insert_resource(CombatRng(1));
        world.init_resource::<Messages<AreaPowerChanged>>();
        let mut colors = WireColors::default();
        let area = world
            .spawn(AreaPower {
                requires_power: true,
                power: [true; 3],
                ..default()
            })
            .id();
        let apc = world
            .spawn((
                Apc::new(area, None),
                colors.wires_for(WireHolder::Apc, &apc_set()),
                TilePos::new(1, 1, 1),
            ))
            .id();
        // Cutting a channel wire turns the channel and the area's channel off; mending: auto on.
        cut(&mut world, apc, WIRE_LIGHT, None);
        assert_eq!(
            world.get::<Apc>(apc).unwrap().channels[Channel::Light as usize],
            CHANNEL_OFF
        );
        assert!(!world.get::<AreaPower>(area).unwrap().power[Channel::Light as usize]);
        cut(&mut world, apc, WIRE_LIGHT, None);
        assert!(world.get::<AreaPower>(area).unwrap().power[Channel::Light as usize]);
        // Main power: a pulse shorts it (everything off) until the 2 min reset.
        pulse(&mut world, apc, WIRE_POWER1, None);
        assert!(world.get::<Apc>(apc).unwrap().shorted);
        assert_eq!(world.get::<AreaPower>(area).unwrap().power, [false; 3]);
        assert_eq!(world.get::<WireResets>(apc).unwrap().0.len(), 1);
        reset(&mut world, apc, WIRE_POWER1);
        assert!(!world.get::<Apc>(apc).unwrap().shorted);
        // Interface: cut unlocks while cut; AI: cut disables.
        cut(&mut world, apc, WIRE_INTERFACE, None);
        assert!(world.get::<Apc>(apc).unwrap().locked);
        cut(&mut world, apc, WIRE_INTERFACE, None);
        assert!(!world.get::<Apc>(apc).unwrap().locked);
        cut(&mut world, apc, WIRE_AI, None);
        assert!(world.get::<Apc>(apc).unwrap().aidisabled);

        let vendor = world
            .spawn((
                crate::vending::vendor(&default(), &HashMap::new()),
                colors.wires_for(WireHolder::Vending, &vending_set()),
                VendorHack::default(),
                TilePos::new(3, 1, 1),
            ))
            .id();
        pulse(&mut world, vendor, WIRE_CONTRABAND, None);
        assert!(world.get::<Vendor>(vendor).unwrap().extended_inventory);
        // Cut or mended, the contraband wire hides it again.
        cut(&mut world, vendor, WIRE_CONTRABAND, None);
        assert!(!world.get::<Vendor>(vendor).unwrap().extended_inventory);
        cut(&mut world, vendor, WIRE_THROW, None);
        assert!(world.get::<VendorHack>(vendor).unwrap().shoot_inventory);
        assert!(world.get::<ShootsInventory>(vendor).is_some());
        cut(&mut world, vendor, WIRE_IDSCAN, None);
        assert!(!world.get::<VendorHack>(vendor).unwrap().scan_id);
        pulse(&mut world, vendor, WIRE_SHOCK, None);
        assert_eq!(
            world.get::<Electrified>(vendor),
            Some(&Electrified::Timed(30.0))
        );
        cut(&mut world, vendor, WIRE_SHOCK, None);
        assert_eq!(
            world.get::<Electrified>(vendor),
            Some(&Electrified::Permanent)
        );
    }

    #[test]
    fn act_params_need_a_wire() {
        let params = [("wire".to_owned(), String::new())];
        assert!(matches!(
            WiresWindow::parse_act("cut", &UiParams::new(&params)),
            Err(UiReject::BadParams(_))
        ));
        let params = [("wire".to_owned(), "red".to_owned())];
        assert_eq!(
            WiresWindow::parse_act("pulse", &UiParams::new(&params)),
            Ok(WireAct::Pulse("red".into()))
        );
        assert!(matches!(
            WiresWindow::parse_act("explode", &UiParams::new(&params)),
            Err(UiReject::UnknownAction)
        ));
    }
}
