//! Exploration drones (server), after tg `exodrone.dm`, `control_console.dm` and
//! `scanner_array.dm`.
//!
//! - The scanner array (`/obj/machinery/exoscanner`): an anchored, powered dish with its panel
//!   closed adds its scan power (1 + (scanning module tier - 1) / 12) to the station's; a site's
//!   scan conditions zero or double single dishes. Its console (tgui `ExoscannerConsole`) runs one
//!   scan at a time: wide (finds the next band's sites), point and deep (reveal a site and its
//!   events' scan descriptions). Losing power below the scan's interrupts it.
//! - The drone (`/obj/item/exodrone`): six slots shared by tools and cargo (a storage while
//!   docked). From a fuelled launcher it travels for `fuel coefficient × distance`, explores
//!   sites (random or revisited events), plays their adventures, and comes back to its launcher
//!   (or any, or a random station turf) with what it found.
//! - The control console (tgui `ExodroneConsole`): one console controls one drone.
//!
//! Timers are deadlines checked every tick ([`tick`]); the windows' data is built in
//! [`refresh_views`] for consoles with an open window and copied by `ui_data`.
// shortcut: no launcher fuel overlays, no supply pod for emergency landings (the drone appears
// on the turf), no console alert screen, no adventure images. The drone's cargo blacklist (tg `GLOB.blacklisted_cargo_types`,
// its item entries) is checked at launch, also for the top level (tg's storage refuses those).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmObj, MapLoadPlugin, MapSeed, Turf};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::{Density, SpatialIndex, WorldSystems};

use super::adventure::{AdventureEntry, AdventureResult, ChoiceOutcome, replace_keywords};
use super::sites::{
    ARTISAN_CARD_STATES, BASE_DEEP_SCAN_TIME, BASE_POINT_SCAN_TIME, EventKind, Exploration, Loot,
    MAX_SCAN_DISTANCE, Scan, ScanCondition, ScanKind, Site, Spawn, Strings, TRADE_CONTRACT_PACKS,
    generate_loot, wide_scan_cost,
};
use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::{Anchored, Integrity, is_under};
use crate::power::Unpowered;
use crate::storage::{Storage, Stored, StoredIn};
use crate::tools::{ToolKind, tell, tool_of};

/// `EXODRONE_CARGO_SLOTS`.
pub const CARGO_SLOTS: usize = 6;
/// `EXODRONE_LOG_SIZE`.
const LOG_SIZE: usize = 15;
/// `/obj/item/fuel_pellet::uses`.
const FUEL_USES: u32 = 5;
/// `/obj/item` `max_integrity`.
const ITEM_MAX_INTEGRITY: f32 = 200.0;
/// How often a running scan checks the array's power (tg on every dish change).
const POWER_CHECK_SECS: f64 = 1.0;

/// Item types the drone may not carry (`GLOB.blacklisted_cargo_types`, its item entries).
const BLACKLISTED_CARGO: [&str; 8] = [
    "/obj/item/beacon",
    "/obj/item/disk/nuclear",
    "/obj/item/gps",
    "/obj/item/hilbertshotel",
    "/obj/item/mail",
    "/obj/item/shared_storage",
    "/obj/item/swapper",
    "/obj/item/warp_cube",
];

/// `EXODRONE_*` states.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DroneStatus {
    #[default]
    Idle,
    Travel,
    Busy,
    Exploration,
    Adventure,
}

/// What happens when the busy time ends.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum BusyEnd {
    #[default]
    Nothing,
    /// `resource/delay_finished`: the event's loot.
    Resource(u32),
    /// `adventure/finish_delay`: the choice's exit node.
    Adventure(String),
}

/// `/obj/item/exodrone`.
#[derive(Component, Clone, Debug)]
pub struct Exodrone {
    pub status: DroneStatus,
    /// The console controlling it (`controlled`).
    pub controlled: Option<Entity>,
    /// Site it is at, `None` at the station.
    pub location: Option<u32>,
    /// Site it travels to, `None` going home.
    pub travel_target: Option<u32>,
    /// Seconds.
    pub travel_time: f64,
    pub travel_ends_at: f64,
    pub busy_message: String,
    pub busy_ends_at: f64,
    pub busy_end: BusyEnd,
    /// The adventure event it plays at its location.
    pub adventure: Option<u32>,
    /// The simple event it looks at (`current_event_ui_data`, built at the encounter).
    pub event: Option<EventScreen>,
    pub last_pad: Option<Entity>,
    /// Newest first.
    pub log: Vec<String>,
    pub tools: Vec<Tool>,
    /// Seconds per unit of distance (`travel_cost_coeff`).
    pub travel_coeff: f64,
}

impl Default for Exodrone {
    fn default() -> Self {
        Self {
            status: DroneStatus::Idle,
            controlled: None,
            location: None,
            travel_target: None,
            travel_time: 0.0,
            travel_ends_at: 0.0,
            busy_message: BUSY_MESSAGE.to_owned(),
            busy_ends_at: 0.0,
            busy_end: BusyEnd::Nothing,
            adventure: None,
            event: None,
            last_pad: None,
            log: Vec::new(),
            tools: Vec::new(),
            travel_coeff: Fuel::Basic.coefficient(),
        }
    }
}

const BUSY_MESSAGE: &str = "Doing something...";

/// `FUEL_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fuel {
    Basic,
    Advanced,
    Exotic,
}

impl Fuel {
    fn of(path: &str) -> Self {
        if is_under(path, FUEL_EXOTIC) {
            Self::Exotic
        } else if is_under(path, FUEL_ADVANCED) {
            Self::Advanced
        } else {
            Self::Basic
        }
    }

    /// `*_FUEL_TIME_COST`, seconds per unit of distance.
    pub fn coefficient(self) -> f64 {
        match self {
            Self::Basic => 25.0,
            Self::Advanced => 20.0,
            Self::Exotic => 10.0,
        }
    }
}

/// `/obj/item/fuel_pellet`.
#[derive(Component, Clone, Copy, Debug)]
pub struct FuelPellet {
    pub fuel: Fuel,
    pub uses: u32,
}

/// `/obj/machinery/exodrone_launcher` with its loaded pellet (`fuel_canister`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Launcher {
    pub fuel: Option<Entity>,
}

/// `/obj/machinery/computer/exodrone_control_console`.
#[derive(Component, Clone, Debug, Default)]
pub struct ControlConsole {
    pub drone: Option<Entity>,
    pub signal_lost: bool,
    view: ExodroneData,
}

/// `/obj/machinery/computer/exoscanner_control`.
#[derive(Component, Clone, Debug, Default)]
pub struct ScannerConsole {
    /// `failed_popup`.
    pub failed: bool,
    pub selected_site: Option<u32>,
    view: ExoscannerData,
}

/// `/obj/machinery/exoscanner`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Exoscanner;

/// `/obj/item/trade_chip`'s `unlocked_pack_type`.
// shortcut: using the chip on a cargo console (`try_to_unlock_contract`) is cargo's part.
#[derive(Component, Clone, Debug)]
pub struct TradeChip(pub &'static str);

pub struct ExodronePlugin;

impl Plugin for ExodronePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.insert_resource(Exploration::new(Strings::default(), Vec::new(), 0))
            .register_ui::<ExodroneConsoleUi>()
            .register_ui::<ExoscannerConsoleUi>()
            // The launcher's `item_interaction` and `crowbar_act`.
            .add_click_handler(ClickPhase::HeldOnTarget, 3110, |w, c| {
                c.held
                    .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
            })
            .add_systems(Startup, load_exploration)
            .add_systems(
                Update,
                (
                    init_new.before(UiSystems::Open),
                    open_consoles
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                    (console_acts, scanner_acts).in_set(UiSystems::Handle),
                    (tick, refresh_views)
                        .chain()
                        .after(UiSystems::Handle)
                        .before(UiSystems::Send),
                ),
            );
    }
}

/// `strings/exodrone.json` and `load_adventures` (`strings/exoadventures/`).
fn load_exploration(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let source = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.source.clone());
    let mut strings = Strings::default();
    let mut adventures = Vec::new();
    if let Some(source) = source {
        if let Ok(text) = ss13_data::read_to_string(&source.join("strings/exodrone.json")) {
            strings = Strings::parse(&text);
        }
        let mut files =
            ss13_data::read_dir(&source.join("strings/exoadventures")).unwrap_or_default();
        files.sort();
        for file in files {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            match ss13_data::read_to_string(&file)
                .ok()
                .and_then(|t| AdventureEntry::parse(&name, &t))
            {
                Some(entry) => adventures.push(entry),
                None => warn!("exodrone: invalid adventure {name}"),
            }
        }
    }
    world.insert_resource(Exploration::new(
        strings,
        adventures,
        seed ^ 0x6578_6f64_726f_6e65,
    ));
}

/// The machines' and items' state as they appear (map load, construction, loot).
fn init_new(
    mut commands: Commands,
    items: Query<(Entity, &DmItem, Option<&DmName>), Added<DmItem>>,
    objs: Query<(Entity, &DmObj), Added<DmObj>>,
    mut ex: ResMut<Exploration>,
) {
    for (e, item, name) in &items {
        if is_under(&item.path, EXODRONE) {
            let mut entity = commands.entity(e);
            entity.insert((
                Exodrone::default(),
                Storage {
                    max_slots: CARGO_SLOTS as u32,
                    max_specific_storage: 3,
                    max_total_storage: 14,
                },
            ));
            // `Initialize`: a probe name, numbered when repeated.
            if let Some(name) = name.filter(|n| n.name == "exploration drone")
                && !ex.strings.probe_names.is_empty()
            {
                let ex = &mut *ex;
                let i = ex
                    .rng
                    .range(0, ex.strings.probe_names.len() as i64 - 1)
                    .max(0) as usize;
                let mut probe = ex.strings.probe_names[i].clone();
                let count = ex.name_counter.entry(probe.clone()).or_insert(0);
                *count += 1;
                if *count > 1 {
                    probe = format!("{probe} {}", roman(*count));
                }
                entity.insert((
                    Name::new(probe.clone()),
                    DmName {
                        name: probe,
                        desc: name.desc.clone(),
                    },
                ));
            }
        } else if is_under(&item.path, FUEL_PELLET) {
            commands.entity(e).insert(FuelPellet {
                fuel: Fuel::of(&item.path),
                uses: FUEL_USES,
            });
        }
    }
    for (e, obj) in &objs {
        let path = obj.path.as_str();
        if is_under(path, LAUNCHER) {
            commands.entity(e).insert(Launcher::default());
        } else if is_under(path, CONTROL_CONSOLE) {
            commands.entity(e).insert(ControlConsole::default());
        } else if is_under(path, SCANNER_CONSOLE) {
            // `post_machine_initialize`: an always active handler for exploration experiments.
            commands.entity(e).insert((
                ScannerConsole::default(),
                crate::science::experi_scanner::ExperimentHandler {
                    kind: crate::science::experi_scanner::HandlerKind::Exoscanner,
                    parent_path: path.to_owned(),
                    linked: true,
                    selected: None,
                },
            ));
        } else if is_under(path, SCANNER) {
            commands.entity(e).insert(Exoscanner);
        }
    }
}

/// BYOND `\Roman`.
fn roman(mut n: u32) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, digits) in TABLE {
        while n >= value {
            out.push_str(digits);
            n -= value;
        }
    }
    out
}

use crate::tools::now;

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<Name>(e)
        .map(|n| n.as_str().to_owned())
        .or_else(|| {
            world
                .get::<DmName>(e)
                .map(|n| crate::vending::plain_name(&n.name).to_owned())
        })
        .unwrap_or_default()
}

fn item_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

fn sound(world: &mut World, at: Entity, path: &str, vol: f32, vary: bool) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, path, vol, vary));
    }
}

// --- The launcher ---------------------------------------------------------------------------

/// The launcher on `pos`.
fn pad_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Launcher>(e).is_some())
}

/// The launcher's pellet with uses left.
fn fuel_of(world: &World, pad: Entity) -> Option<(Entity, FuelPellet)> {
    let pellet = world.get::<Launcher>(pad)?.fuel?;
    Some((pellet, *world.get::<FuelPellet>(pellet)?))
}

/// The launcher's `item_interaction` (a pellet loads, a drone is put on it) and `crowbar_act`
/// (the pellet comes out).
fn item_interaction(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(launcher) = world.get::<Launcher>(target).copied() else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    if world.get::<FuelPellet>(held).is_some() {
        if launcher.fuel.is_some() {
            let name = name_of(world, target);
            tell(
                world,
                client,
                Span::Warning,
                format!("There's already fuel loaded inside {name}!"),
            );
            return true;
        }
        if crate::inventory::no_drop(world, held) {
            return true;
        }
        crate::storage::put_inside(world, held, target);
        world
            .entity_mut(target)
            .insert(Launcher { fuel: Some(held) });
        return true;
    }
    if world.get::<Exodrone>(held).is_some() {
        if !crate::inventory::no_drop(world, held) {
            crate::inventory::put_down(world, held, pos);
        }
        return true;
    }
    if tool_of(world, held).is_some_and(|(k, _)| k == ToolKind::Crowbar) {
        let Some(pellet) = launcher.fuel else {
            return false;
        };
        let (pellet_name, name) = (name_of(world, pellet), name_of(world, target));
        tell(
            world,
            client,
            Span::Notice,
            format!("You remove {pellet_name} from {name}."),
        );
        world.entity_mut(target).insert(Launcher { fuel: None });
        crate::storage::take_out(world, pellet, pos);
        return true;
    }
    false
}

/// `fuel_up`: the drone takes the fuel's coefficient, the pellet loses a use.
fn fuel_up(world: &mut World, pad: Entity, drone: Entity) {
    let Some((pellet, mut fuel)) = fuel_of(world, pad) else {
        return;
    };
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.travel_coeff = fuel.fuel.coefficient();
    }
    fuel.uses = fuel.uses.saturating_sub(1);
    if fuel.uses == 0 {
        world.entity_mut(pad).insert(Launcher { fuel: None });
        world.despawn(pellet);
    } else {
        world.entity_mut(pellet).insert(fuel);
    }
}

// --- The drone ------------------------------------------------------------------------------

/// `drone_log`.
fn drone_log(world: &mut World, drone: Entity, message: impl Into<String>) {
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.log.insert(0, message.into());
        // tg `drone_log.Remove()` without arguments removes nothing; the log is meant to keep
        // `EXODRONE_LOG_SIZE` lines.
        d.log.truncate(LOG_SIZE);
    }
}

fn cargo_items(world: &World, drone: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(drone)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

/// `space_left`.
fn space_left(world: &World, drone: Entity) -> usize {
    let tools = world.get::<Exodrone>(drone).map_or(0, |d| d.tools.len());
    CARGO_SLOTS.saturating_sub(cargo_items(world, drone).len() + tools)
}

fn has_tool(world: &World, drone: Entity, tool: Tool) -> bool {
    world
        .get::<Exodrone>(drone)
        .is_some_and(|d| d.tools.contains(&tool))
}

/// `update_storage_size`.
fn update_storage_size(world: &mut World, drone: Entity) {
    let tools = world.get::<Exodrone>(drone).map_or(0, |d| d.tools.len());
    if let Some(mut storage) = world.get_mut::<Storage>(drone) {
        storage.max_slots = CARGO_SLOTS.saturating_sub(tools) as u32;
    }
}

/// `get_integrity` and `max_integrity`.
fn integrity(world: &World, drone: Entity) -> (f32, f32) {
    let max = world
        .get::<DmItem>(drone)
        .map(|d| d.max_integrity)
        .filter(|m| *m > 0.0)
        .unwrap_or(ITEM_MAX_INTEGRITY);
    let current = world.get::<Integrity>(drone).map_or(max, |i| i.current);
    (current, max)
}

/// `damage(amount)`: `take_damage` (gone at 0) and the log line.
fn damage(world: &mut World, drone: Entity, amount: f32) {
    crate::explosions::ex_act::damage_item(world, drone, amount);
    drone_log(world, drone, format!("Sustained {amount} damage."));
}

/// `check_blacklist`.
fn check_blacklist(world: &World, drone: Entity) -> bool {
    let mut stack = cargo_items(world, drone);
    let mut seen = 0;
    while let Some(item) = stack.pop() {
        seen += 1;
        if seen > 1000 {
            break;
        }
        if item_path(world, item).is_some_and(|p| BLACKLISTED_CARGO.iter().any(|b| is_under(p, b)))
        {
            return false;
        }
        stack.extend(cargo_items(world, item));
    }
    true
}

/// `travel_error`: why the drone cannot launch, `None` when it can.
// Not tg: tg returns "" (false) for travelling, busy and adventuring drones, which lets a
// crafted `start_travel` launch them again.
fn travel_error(world: &World, drone: Entity) -> Option<String> {
    let d = world.get::<Exodrone>(drone)?;
    match d.status {
        DroneStatus::Idle => {
            let Some(pad) = world
                .get::<TilePos>(drone)
                .and_then(|&pos| pad_on(world, pos))
            else {
                return Some("No launcher".into());
            };
            if world.get::<Launcher>(pad).and_then(|l| l.fuel).is_none() {
                return Some("No fuel in launcher".into());
            }
            if fuel_of(world, pad).is_none_or(|(_, f)| f.uses == 0) {
                return Some("Launcher fuel used up".into());
            }
            None
        }
        DroneStatus::Exploration if d.event.is_some() => Some("Busy".into()),
        DroneStatus::Exploration => None,
        _ => Some(String::new()),
    }
}

/// `get_travel_coeff`: an idle drone shows its launcher's fuel.
fn travel_coeff(world: &World, drone: Entity) -> f64 {
    let Some(d) = world.get::<Exodrone>(drone) else {
        return 0.0;
    };
    if d.status == DroneStatus::Idle
        && let Some(pad) = world
            .get::<TilePos>(drone)
            .and_then(|&pos| pad_on(world, pos))
        && let Some((_, fuel)) = fuel_of(world, pad)
    {
        return fuel.fuel.coefficient();
    }
    d.travel_coeff
}

fn site_name(world: &World, id: Option<u32>) -> String {
    match id {
        Some(id) => world
            .resource::<Exploration>()
            .site(id)
            .map_or_else(|| "ERROR".to_owned(), |s| s.display_name().to_owned()),
        None => crate::events::station_name(world),
    }
}

/// `launch_for(target_site)`: fuels up from the launcher at the station, then travels.
fn launch_for(world: &mut World, drone: Entity, target: Option<u32>) {
    let Some(d) = world.get::<Exodrone>(drone).cloned() else {
        return;
    };
    let target_name = site_name(world, target);
    if d.location.is_none() {
        let Some(pad) = world
            .get::<TilePos>(drone)
            .and_then(|&pos| pad_on(world, pos))
        else {
            return;
        };
        fuel_up(world, pad, drone);
        // `launch_effect`.
        sound(world, pad, "sound/effects/podwoosh.ogg", 50.0, false);
        if let Some(&pos) = world.get::<TilePos>(pad) {
            crate::effects::smoke::do_smoke(
                world,
                1,
                pos,
                None,
                crate::effects::smoke::SmokeKind::Basic,
            );
        }
        let pad_name = name_of(world, pad);
        if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
            d.last_pad = Some(pad);
        }
        drone_log(
            world,
            drone,
            format!("Launched from {pad_name} and set course for {target_name}"),
        );
    } else {
        let here = site_name(world, d.location);
        drone_log(
            world,
            drone,
            format!("Launched from {here} and set course for {target_name}"),
        );
    }
    // `moveToNullspace`.
    world
        .entity_mut(drone)
        .remove::<(TilePos, CarriedBy, StoredIn)>();
    let ex = world.resource::<Exploration>();
    let distance_of = |id: Option<u32>| id.and_then(|id| ex.site(id)).map_or(1, |s| s.distance);
    let distance = match (d.location, target) {
        // Site to site is faster.
        (Some(here), Some(there)) => distance_of(Some(here))
            .abs_diff(distance_of(Some(there)))
            .max(1),
        (_, Some(there)) => distance_of(Some(there)),
        (here, None) => distance_of(here),
    };
    let t = now(world);
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.status = DroneStatus::Travel;
        d.travel_target = target;
        d.travel_time = d.travel_coeff * f64::from(distance);
        d.travel_ends_at = t + d.travel_time;
    }
}

/// `finish_travel`: arrives at the site, or lands at the station.
fn finish_travel(world: &mut World, drone: Entity) {
    let Some(target) = world.get::<Exodrone>(drone).map(|d| d.travel_target) else {
        return;
    };
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.location = target;
        d.travel_time = 0.0;
    }
    if let Some(site) = target {
        // `on_drone_arrival`.
        let message = world.resource_mut::<Exploration>().site_mut(site).map(|s| {
            let known = s.revealed;
            s.revealed = true;
            if known {
                format!("Arrived at {}.", s.display_name())
            } else {
                format!("Discovered {} at {}.", s.name, s.coordinates)
            }
        });
        if let Some(message) = message {
            drone_log(world, drone, message);
        }
        set_status(world, drone, DroneStatus::Exploration);
        return;
    }
    let station = crate::events::station_name(world);
    let pad = find_landing_pad(world, drone);
    if let Some((pad, pos)) = pad {
        land(world, drone, pos);
        let name = name_of(world, pad);
        drone_log(
            world,
            drone,
            format!("Arrived at {station}. Landing at {name}."),
        );
    } else if let Some(pos) = crate::events::electrical_storm::random_station_turf(world) {
        land(world, drone, pos);
        let area = world
            .get_resource::<ss13_mapload::Areas>()
            .and_then(|a| a.area_at(pos))
            .map(|a| name_of(world, a))
            .unwrap_or_default();
        drone_log(
            world,
            drone,
            format!("Arrived at {station}. Emergency landing at {area}."),
        );
    }
    set_status(world, drone, DroneStatus::Idle);
}

fn land(world: &mut World, drone: Entity, pos: TilePos) {
    let mut entity = world.entity_mut(drone);
    if !entity.contains::<Networked>() {
        entity.insert(Networked);
    }
    entity.insert(pos);
}

/// `find_landing_pad`: the launcher it left from, else any.
fn find_landing_pad(world: &mut World, drone: Entity) -> Option<(Entity, TilePos)> {
    let last = world.get::<Exodrone>(drone).and_then(|d| d.last_pad);
    if let Some(pad) = last
        && world.get::<Launcher>(pad).is_some()
        && let Some(&pos) = world.get::<TilePos>(pad)
    {
        return Some((pad, pos));
    }
    let mut query = world.query_filtered::<(Entity, &TilePos), With<Launcher>>();
    let mut pads: Vec<(Entity, TilePos)> = query.iter(world).map(|(e, p)| (e, *p)).collect();
    pads.sort_by_key(|p| p.0);
    pads.first().copied()
}

/// `set_status` (`COMSIG_EXODRONE_STATUS_CHANGED`: the console beeps).
fn set_status(world: &mut World, drone: Entity, status: DroneStatus) {
    let console = world.get_mut::<Exodrone>(drone).and_then(|mut d| {
        d.status = status;
        d.controlled
    });
    if let Some(console) = console {
        sound(
            world,
            console,
            "sound/machines/terminal/terminal_processing.ogg",
            20.0,
            true,
        );
    }
}

/// `set_busy(message, duration)`.
fn set_busy(world: &mut World, drone: Entity, message: Option<String>, secs: f64, end: BusyEnd) {
    let t = now(world);
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        if let Some(message) = message {
            d.busy_message = message;
        }
        d.busy_ends_at = t + secs;
        d.busy_end = end;
    }
    set_status(world, drone, DroneStatus::Busy);
}

/// `unset_busy(new_status)`.
fn unset_busy(world: &mut World, drone: Entity, status: DroneStatus) {
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.busy_message = BUSY_MESSAGE.to_owned();
        d.busy_end = BusyEnd::Nothing;
    }
    set_status(world, drone, status);
}

/// `try_transfer(loot)`: into the cargo if there is room, else abandoned.
fn try_transfer(world: &mut World, drone: Entity, loot: &Spawn) {
    let Some(item) = crate::construction::spawn_detached(world, loot.path) else {
        warn!("exodrone: no prototype for {}", loot.path);
        return;
    };
    if loot.amount > 0 && world.get::<crate::tools::Stack>(item).is_some() {
        world.entity_mut(item).insert(crate::tools::Stack {
            amount: loot.amount,
        });
    }
    transfer_item(world, drone, item);
}

/// `try_transfer` of an existing item.
fn transfer_item(world: &mut World, drone: Entity, item: Entity) {
    let name = name_of(world, item);
    if space_left(world, drone) > 0 {
        crate::storage::put_inside(world, item, drone);
        drone_log(world, drone, format!("Acquired {name}."));
    } else {
        drone_log(
            world,
            drone,
            format!("Abandoned {name} due to lack of space."),
        );
        world.despawn(item);
    }
}

/// A loot generator's `transfer_loot`.
fn transfer_generated(world: &mut World, drone: Entity, id: &str) {
    let loot = generate_loot(id, &mut world.resource_mut::<Exploration>().rng);
    for spawn in loot {
        let before = cargo_items(world, drone);
        try_transfer(world, drone, &spawn);
        if spawn.path == super::sites::TRADE_CHIP
            && let Some(chip) = cargo_items(world, drone)
                .into_iter()
                .find(|e| !before.contains(e))
        {
            let i = world
                .resource_mut::<Exploration>()
                .rng
                .range(0, TRADE_CONTRACT_PACKS.len() as i64 - 1) as usize;
            world
                .entity_mut(chip)
                .insert(TradeChip(TRADE_CONTRACT_PACKS[i]));
        }
    }
}

/// `explore_site(specific_event)`: a random unvisited event, or the chosen revisitable one.
fn explore_site(world: &mut World, drone: Entity, specific: Option<u32>) {
    let Some(site) = world.get::<Exodrone>(drone).and_then(|d| d.location) else {
        return;
    };
    let picked: Result<Option<u32>, String> = {
        let mut ex = world.resource_mut::<Exploration>();
        let ex = &mut *ex;
        let Some(s) = ex.sites.iter().find(|s| s.id == site) else {
            return;
        };
        match specific {
            Some(id) => Ok(s.event(id).filter(|e| e.is_targetable()).map(|e| e.id)),
            None => {
                let fresh: Vec<u32> = s
                    .events
                    .iter()
                    .filter(|e| !e.visited)
                    .map(|e| e.id)
                    .collect();
                if fresh.is_empty() {
                    Err(s.name.clone())
                } else {
                    Ok(Some(
                        fresh[ex.rng.range(0, fresh.len() as i64 - 1) as usize],
                    ))
                }
            }
        }
    };
    match picked {
        Ok(Some(event)) => encounter(world, drone, site, event),
        Ok(None) => {}
        Err(name) => drone_log(
            world,
            drone,
            format!("It seems there's nothing interesting left around {name}."),
        ),
    }
}

/// `encounter`: the discovery log on the first visit, then the event's own effect.
fn encounter(world: &mut World, drone: Entity, site: u32, event: u32) {
    let Some((visited, discovery, kind)) = world
        .resource::<Exploration>()
        .site(site)
        .and_then(|s| s.event(event))
        .map(|e| (e.visited, e.discovery_log.clone(), e.kind.clone()))
    else {
        return;
    };
    if !visited {
        let log = match &kind {
            EventKind::Fluff => {
                let mut ex = world.resource_mut::<Exploration>();
                let ex = &mut *ex;
                let fluff = ex.site(site).map_or("fluff_generic", |s| s.fluff_type);
                ex.strings
                    .fluff
                    .get(fluff)
                    .filter(|l| !l.is_empty())
                    .map(|lines| lines[ex.rng.range(0, lines.len() as i64 - 1) as usize].clone())
            }
            EventKind::Trader(t) => Some(trader_discovery(world, drone, site, event, t)),
            _ => discovery,
        };
        if let Some(log) = log {
            drone_log(world, drone, log);
        }
        if let Some(e) = world
            .resource_mut::<Exploration>()
            .site_mut(site)
            .and_then(|s| s.event_mut(event))
        {
            e.visited = true;
        }
    }
    match kind {
        EventKind::Fluff => {}
        EventKind::Adventure(_) => start_adventure(world, drone, site, event),
        _ => {
            let screen = event_screen(world, drone, site, event);
            if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
                d.event = screen;
            }
        }
    }
}

/// The name of a type (`initial(path.name)`).
fn type_name(world: &World, path: &str) -> String {
    world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(path))
        .and_then(|p| p.get::<DmName>())
        .map_or_else(
            || path.rsplit('/').next().unwrap_or(path).replace('_', " "),
            |n| crate::vending::plain_name(&n.name).to_owned(),
        )
}

fn event_name(world: &World, site: u32, event: u32) -> String {
    world
        .resource::<Exploration>()
        .site(site)
        .and_then(|s| s.event(event))
        .map(|e| e.name.clone())
        .unwrap_or_default()
}

/// `trader/get_discovery_message`.
fn trader_discovery(
    world: &World,
    drone: Entity,
    site: u32,
    event: u32,
    t: &super::sites::Trader,
) -> String {
    let name = event_name(world, site, event);
    if t.id_artisan {
        return format!("Encountered {name} willing to customise any ID card you bring them.");
    }
    if t.requires_translator && !has_tool(world, drone, Tool::Translator) {
        return format!(
            "You encountered {name} but could not understand what they want without a translator."
        );
    }
    let want = t
        .wanted
        .first()
        .map_or_else(String::new, |p| type_name(world, p));
    let gives = t
        .traded
        .first()
        .map_or_else(String::new, |p| type_name(world, p));
    format!("Encountered {name} willing to trade {gives} for {want}")
}

/// The trader's wanted item in the cargo (`locate(required_path) in drone.contents`).
fn trade_good(world: &World, drone: Entity, t: &super::sites::Trader) -> Option<Entity> {
    let wanted = t.wanted.first()?;
    cargo_items(world, drone)
        .into_iter()
        .find(|&e| item_path(world, e).is_some_and(|p| is_under(p, wanted)))
}

/// `build_ui_event`.
fn event_screen(world: &World, drone: Entity, site: u32, event: u32) -> Option<EventScreen> {
    let e = world.resource::<Exploration>().site(site)?.event(event)?;
    let has = |tool: Option<Tool>| tool.is_none_or(|t| has_tool(world, drone, t));
    let (description, action_text, action_enabled) = match &e.kind {
        EventKind::Resource(r) => {
            let extra = if has(r.required_tool) {
                &r.has_tool_description
            } else {
                &r.no_tool_description
            };
            (
                format!("{}\n{extra}", e.description),
                e.action_text.clone(),
                r.amount > 0 && has(r.required_tool),
            )
        }
        EventKind::Danger(d) => {
            let escape = has(d.required_tool);
            let (extra, action) = if escape {
                (&d.has_tool_description, &d.has_tool_action_text)
            } else {
                (&d.no_tool_description, &d.no_tool_action_text)
            };
            (format!("{}\n{extra}", e.description), action.clone(), true)
        }
        EventKind::Trader(t) => {
            let translated = !t.requires_translator || has_tool(world, drone, Tool::Translator);
            let description = if t.id_artisan {
                "You encounter a local craftsman willing to customise an ID card for you, free \
                 of charge."
                    .to_owned()
            } else if !translated {
                format!(
                    "You encounter {} but cannot understand what they want without a translator.",
                    e.name
                )
            } else {
                let want = t
                    .wanted
                    .first()
                    .map_or_else(String::new, |p| type_name(world, p));
                let gives = t
                    .traded
                    .first()
                    .map_or_else(String::new, |p| type_name(world, p));
                let times = if t.amount > 1 {
                    format!("{} times", t.amount)
                } else {
                    String::new()
                };
                format!(
                    "You encounter {} willing to trade {want} for {gives} {times}.",
                    e.name
                )
            };
            let enabled = t.amount > 0 && trade_good(world, drone, t).is_some() && translated;
            (description, e.action_text.clone(), enabled)
        }
        _ => return None,
    };
    Some(EventScreen {
        description,
        action_enabled,
        action_text,
        skippable: e.skippable(),
        ignore_text: "Ignore".to_owned(),
        id: event,
    })
}

/// `simple/fire`: the player acted on the event (validated again, tg's comment: "done after
/// unknown delay").
fn fire_event(world: &mut World, drone: Entity, site: u32, event: u32) {
    let Some(kind) = world
        .resource::<Exploration>()
        .site(site)
        .and_then(|s| s.event(event))
        .map(|e| e.kind.clone())
    else {
        return;
    };
    let enabled = event_screen(world, drone, site, event).is_some_and(|s| s.action_enabled);
    match kind {
        EventKind::Resource(r) => {
            if !enabled {
                end_event(world, drone);
                return;
            }
            use_up(world, site, event);
            if r.delay > 0.0 {
                set_busy(
                    world,
                    drone,
                    Some(r.delay_message.clone()),
                    r.delay,
                    BusyEnd::Resource(event),
                );
            } else {
                finish_resource(world, drone, site, event);
            }
        }
        EventKind::Danger(d) => {
            if d.required_tool.is_none_or(|t| has_tool(world, drone, t)) {
                drone_log(world, drone, d.avoid_log.clone());
            } else {
                damage(world, drone, d.damage);
            }
            end_event(world, drone);
        }
        EventKind::Trader(t) => {
            if enabled {
                use_up(world, site, event);
                trade(world, drone, &t);
            }
            end_event(world, drone);
        }
        _ => {}
    }
}

/// `amount--` of a resource or trader.
fn use_up(world: &mut World, site: u32, event: u32) {
    if let Some(e) = world
        .resource_mut::<Exploration>()
        .site_mut(site)
        .and_then(|s| s.event_mut(event))
    {
        match &mut e.kind {
            EventKind::Resource(r) => r.amount = r.amount.saturating_sub(1),
            EventKind::Trader(t) => t.amount = t.amount.saturating_sub(1),
            _ => {}
        }
    }
}

/// `trader/trade` (`flame_card/trade`).
fn trade(world: &mut World, drone: Entity, t: &super::sites::Trader) {
    let Some(good) = trade_good(world, drone, t) else {
        return;
    };
    let good_name = name_of(world, good);
    if t.id_artisan {
        let i = world
            .resource_mut::<Exploration>()
            .rng
            .range(0, ARTISAN_CARD_STATES.len() as i64 - 1) as usize;
        crate::obj_damage::set_icon_state(world, good, ARTISAN_CARD_STATES[i]);
        drone_log(world, drone, format!("Let artisan work on {good_name}."));
        return;
    }
    let Some(&traded) = t.traded.first() else {
        return;
    };
    let Some(loot) = crate::construction::spawn_detached(world, traded) else {
        warn!("exodrone: no prototype for {traded}");
        return;
    };
    let loot_name = name_of(world, loot);
    drone_log(world, drone, format!("Traded {good_name} for {loot_name}."));
    world.despawn(good);
    transfer_item(world, drone, loot);
}

/// `resource/finish_event`.
fn finish_resource(world: &mut World, drone: Entity, site: u32, event: u32) {
    let Some(EventKind::Resource(r)) = world
        .resource::<Exploration>()
        .site(site)
        .and_then(|s| s.event(event))
        .map(|e| e.kind.clone())
    else {
        end_event(world, drone);
        return;
    };
    drone_log(world, drone, r.success_log.clone());
    match &r.loot {
        Loot::Item(spawn) => try_transfer(world, drone, spawn),
        Loot::Generator(id) => transfer_generated(world, drone, id),
    }
    end_event(world, drone);
}

/// `simple/end`.
fn end_event(world: &mut World, drone: Entity) {
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.event = None;
    }
}

/// Runs `f` on the drone's adventure with the round's generator.
fn with_adventure<R>(
    world: &mut World,
    site: u32,
    event: u32,
    f: impl FnOnce(&mut super::adventure::Adventure, &mut ss13_mapload::spawner::Rng) -> R,
) -> Option<R> {
    let mut ex = world.resource_mut::<Exploration>();
    let ex = &mut *ex;
    let site = ex.sites.iter_mut().find(|s| s.id == site)?;
    let EventKind::Adventure(adventure) = &mut site.event_mut(event)?.kind else {
        return None;
    };
    Some(f(adventure, &mut ex.rng))
}

/// `start_adventure`.
fn start_adventure(world: &mut World, drone: Entity, site: u32, event: u32) {
    let tools: Vec<&'static str> = world
        .get::<Exodrone>(drone)
        .map(|d| d.tools.iter().map(|t| t.name()).collect())
        .unwrap_or_default();
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.adventure = Some(event);
    }
    set_status(world, drone, DroneStatus::Adventure);
    with_adventure(world, site, event, |a, rng| a.start(&tools, rng));
    check_adventure(world, drone);
}

/// `resolve_adventure` once the adventure reached an end node (`COMSIG_ADVENTURE_FINISHED`).
fn check_adventure(world: &mut World, drone: Entity) {
    let Some((site, event)) = world
        .get::<Exodrone>(drone)
        .and_then(|d| Some((d.location?, d.adventure?)))
    else {
        return;
    };
    let Some(Some(result)) = with_adventure(world, site, event, |a, _| a.result) else {
        return;
    };
    match result {
        AdventureResult::Success => {
            // `award_adventure_loot`.
            let categories = with_adventure(world, site, event, |a, _| a.loot_categories.clone())
                .unwrap_or_default();
            if !categories.is_empty() {
                let i = world
                    .resource_mut::<Exploration>()
                    .rng
                    .range(0, categories.len() as i64 - 1) as usize;
                transfer_generated(world, drone, &categories[i]);
            }
        }
        AdventureResult::Damage => {
            let (_, max) = integrity(world, drone);
            damage(world, drone, max * 0.5);
            if world.get_entity(drone).is_err() {
                return;
            }
        }
        AdventureResult::Death => {
            world.despawn(drone);
            return;
        }
    }
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.adventure = None;
    }
    set_status(world, drone, DroneStatus::Exploration);
}

/// `adventure_choice`.
fn adventure_choice(world: &mut World, drone: Entity, key: &str) {
    let Some((site, event)) = world
        .get::<Exodrone>(drone)
        .and_then(|d| Some((d.location?, d.adventure?)))
    else {
        return;
    };
    match with_adventure(world, site, event, |a, rng| a.select_choice(key, rng)).flatten() {
        Some(ChoiceOutcome::Delay {
            delay_ds,
            message,
            exit,
        }) => set_busy(
            world,
            drone,
            message,
            delay_ds / 10.0,
            BusyEnd::Adventure(exit),
        ),
        Some(ChoiceOutcome::Done) => check_adventure(world, drone),
        None => {}
    }
}

/// The adventure's screen with its keywords (`get_adventure_data`).
fn adventure_screen(world: &World, d: &Exodrone) -> AdventureScreen {
    let ex = world.resource::<Exploration>();
    let Some(site) = d.location.and_then(|s| ex.site(s)) else {
        return AdventureScreen::default();
    };
    let Some(EventKind::Adventure(a)) = d.adventure.and_then(|e| site.event(e)).map(|e| &e.kind)
    else {
        return AdventureScreen::default();
    };
    let name = site.display_name();
    AdventureScreen {
        description: a
            .node()
            .map(|n| replace_keywords(&n.description, name, &a.qualities))
            .unwrap_or_default(),
        choices: a
            .available_choices()
            .into_iter()
            .map(|(k, t)| (k, replace_keywords(&t, name, &a.qualities)))
            .collect(),
    }
}

/// `handle_deconstruct`: a small explosion and sparks where it is, and it is gone.
// Not tg: tg's self-destruct only explodes (in nullspace: nothing) and leaves the drone.
fn self_destruct(world: &mut World, drone: Entity) {
    if let Some(&pos) = world.get::<TilePos>(drone) {
        crate::explosions::explosion(world, pos, crate::explosions::Explosion::new(0, 0, 1));
        crate::effects::particles::do_sparks(world, 5, false, pos);
    }
    if world.get_entity(drone).is_ok() {
        world.despawn(drone);
    }
}

// --- The scanner array ----------------------------------------------------------------------

/// The ready dishes (`tracked_dishes`): anchored, powered, panel closed; with their power.
fn dishes(world: &mut World) -> Vec<(Entity, TilePos, f64)> {
    let mut query = world.query_filtered::<(Entity, &TilePos, &DmObj), With<Exoscanner>>();
    let found: Vec<(Entity, TilePos, bool)> = query
        .iter(world)
        .map(|(e, p, o)| (e, *p, o.anchored))
        .collect();
    let mut out: Vec<(Entity, TilePos, f64)> = found
        .into_iter()
        .filter(|&(e, _, anchored)| {
            world.get::<Anchored>(e).map_or(anchored, |a| a.0)
                && world.get::<Unpowered>(e).is_none()
                && !world
                    .get::<crate::construction::PanelOpen>(e)
                    .is_some_and(|p| p.0)
        })
        .map(|(e, pos, _)| {
            let tiers = crate::stock_parts::ratings(world, e);
            let power = 1.0
                + tiers
                    .tiers(crate::stock_parts::PartKind::ScanningModule)
                    .map(|t| (f64::from(t) - 1.0) / 12.0)
                    .sum::<f64>();
            (e, pos, power)
        })
        .collect();
    out.sort_by_key(|d| d.0);
    out
}

/// `check_dish`: the dish's power multiplier under `condition`.
fn condition_multiplier(
    world: &World,
    dish: (Entity, TilePos),
    condition: ScanCondition,
    all: &[(Entity, TilePos, f64)],
) -> f64 {
    let (e, pos) = dish;
    let index = world.resource::<SpatialIndex>();
    match condition {
        ScanCondition::Nebula => {
            let crowded = all.iter().any(|&(other, p, _)| {
                other != e && p.z == pos.z && (p.x - pos.x).abs().max((p.y - pos.y).abs()) < 15
            });
            if crowded { 0.0 } else { 1.0 }
        }
        ScanCondition::Pulsar => {
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                    let noisy = index.on_tile(tile).iter().any(|&m| {
                        m != e
                            && world
                                .get::<DmObj>(m)
                                .is_some_and(|o| is_under(&o.path, "/obj/machinery"))
                            && world.get::<Unpowered>(m).is_none()
                    });
                    if noisy {
                        return 0.0;
                    }
                }
            }
            1.0
        }
        ScanCondition::AsteroidBelt => {
            if crate::comms_server::on_station_level(world, pos) {
                0.0
            } else {
                1.0
            }
        }
        ScanCondition::BlackHole => {
            let mut walls = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                    if index.on_tile(tile).iter().any(|&t| {
                        world.get::<Turf>(t).is_some() && world.get::<Density>(t).is_some()
                    }) {
                        walls += 1;
                    }
                }
            }
            if walls > 6 { 1.0 } else { 0.0 }
        }
        ScanCondition::Easy => 2.0,
    }
}

/// `calculate_scan_power(conditions)`.
fn scan_power(
    world: &World,
    dishes: &[(Entity, TilePos, f64)],
    conditions: &[ScanCondition],
) -> f64 {
    dishes
        .iter()
        .map(|&(e, pos, power)| {
            let mut p = power;
            for &c in conditions {
                p *= condition_multiplier(world, (e, pos), c, dishes);
                if p == 0.0 {
                    break;
                }
            }
            p
        })
        .sum()
}

fn site_conditions(world: &World, site: Option<u32>) -> Vec<ScanCondition> {
    site.and_then(|s| world.resource::<Exploration>().site(s))
        .map(|s| s.conditions.clone())
        .unwrap_or_default()
}

/// `create_scan` with its time (`/datum/exoscan/New`).
fn start_scan(world: &mut World, console: Entity, kind: ScanKind, target: Option<u32>) {
    if world.resource::<Exploration>().scan.is_some() {
        return;
    }
    let all = dishes(world);
    if all.is_empty() {
        return;
    }
    let power = scan_power(world, &all, &site_conditions(world, target));
    if power <= 0.0 {
        return;
    }
    let ex = world.resource::<Exploration>();
    let distance = target.and_then(|t| ex.site(t)).map_or(1, |s| s.distance);
    let secs = match kind {
        ScanKind::Wide => wide_scan_cost(ex.wide_scan_band, power),
        ScanKind::Point => BASE_POINT_SCAN_TIME / power,
        ScanKind::Deep => BASE_DEEP_SCAN_TIME * f64::from(distance) / power,
    };
    let ends_at = now(world) + secs;
    world.resource_mut::<Exploration>().scan = Some(Scan {
        kind,
        power,
        target,
        ends_at,
        console: Some(console),
    });
    sound(
        world,
        console,
        "sound/machines/terminal/terminal_processing.ogg",
        20.0,
        true,
    );
}

/// `/datum/exoscan/proc/stop`: the starting console shows the failure.
fn interrupt_scan(world: &mut World) {
    let console = world
        .resource_mut::<Exploration>()
        .scan
        .take()
        .and_then(|s| s.console);
    if let Some(mut c) = console.and_then(|c| world.get_mut::<ScannerConsole>(c)) {
        c.failed = true;
    }
}

/// `resolve_scan`.
fn resolve_scan(world: &mut World) {
    let Some(scan) = world.resource_mut::<Exploration>().scan.take() else {
        return;
    };
    let mut ex = world.resource_mut::<Exploration>();
    match scan.kind {
        ScanKind::Wide => ex.generate_sites(),
        ScanKind::Point | ScanKind::Deep => {
            if let Some(site) = scan.target.and_then(|t| ex.site_mut(t)) {
                site.revealed = true;
                if scan.kind == ScanKind::Point {
                    site.point_scan_complete = true;
                } else {
                    site.deep_scan_complete = true;
                }
            }
        }
    }
    complete_exploration_experiments(world);
}

/// `/datum/experiment/exploration_scan` completion: every available one some site fits.
// Not as in tg: there the console's always active handler is never actioned, so these never
// complete; here every resolved scan checks them.
pub fn complete_exploration_experiments(world: &mut World) {
    use ss13_mapload::research::ExperimentKind;
    let done: Vec<String> = {
        let (Some(web), Some(db)) = (
            world.get_resource::<crate::science::techweb::Techweb>(),
            world.get_resource::<crate::science::techweb::ResearchDb>(),
        ) else {
            return;
        };
        let ex = world.resource::<Exploration>();
        web.available_experiments
            .iter()
            .filter(|e| match &db.experiments[e.def].kind {
                ExperimentKind::Exploration { scan_type, .. } => ex
                    .sites
                    .iter()
                    .any(|s| s.fits_exploration(&e.site_type, &e.condition, scan_type)),
                _ => false,
            })
            .map(|e| e.id.clone())
            .collect()
    };
    for id in done {
        crate::science::experi_scanner::complete_and_announce(world, &id);
    }
}

/// The dishes' look (`update_icon_state`).
fn update_dish_states(world: &mut World, ready: &[(Entity, TilePos, f64)]) {
    let scanning = world.resource::<Exploration>().scan.is_some();
    let mut query = world.query_filtered::<Entity, With<Exoscanner>>();
    let all: Vec<Entity> = query.iter(world).collect();
    for e in all {
        let state = if world
            .get::<crate::construction::PanelOpen>(e)
            .is_some_and(|p| p.0)
        {
            "scanner_open"
        } else if ready.iter().any(|d| d.0 == e) {
            if scanning {
                "scanner_on"
            } else {
                "scanner_ready"
            }
        } else {
            "scanner_off"
        };
        crate::obj_damage::set_icon_state(world, e, state);
    }
}

// --- Timers ---------------------------------------------------------------------------------

/// The drones' travel and busy deadlines, the scan's end and power, consoles losing their drone.
pub fn tick(world: &mut World, mut next_check: Local<f64>) {
    let t = now(world);
    let mut query = world.query::<(Entity, &Exodrone)>();
    let due: Vec<(Entity, DroneStatus, f64, f64)> = query
        .iter(world)
        .map(|(e, d)| (e, d.status, d.travel_ends_at, d.busy_ends_at))
        .collect();
    for (drone, status, travel_end, busy_end) in due {
        match status {
            DroneStatus::Travel if t >= travel_end => finish_travel(world, drone),
            DroneStatus::Busy if t >= busy_end => {
                let end = world
                    .get::<Exodrone>(drone)
                    .map(|d| d.busy_end.clone())
                    .unwrap_or_default();
                match end {
                    BusyEnd::Resource(event) => {
                        unset_busy(world, drone, DroneStatus::Exploration);
                        if let Some(site) = world.get::<Exodrone>(drone).and_then(|d| d.location) {
                            finish_resource(world, drone, site, event);
                        }
                    }
                    BusyEnd::Adventure(exit) => {
                        unset_busy(world, drone, DroneStatus::Adventure);
                        if let Some((site, event)) = world
                            .get::<Exodrone>(drone)
                            .and_then(|d| Some((d.location?, d.adventure?)))
                        {
                            with_adventure(world, site, event, |a, rng| a.finish_delay(&exit, rng));
                            check_adventure(world, drone);
                        }
                    }
                    BusyEnd::Nothing => unset_busy(world, drone, DroneStatus::Exploration),
                }
            }
            _ => {}
        }
    }
    // Consoles whose drone is gone (`drone_destroyed`), drones whose console is gone.
    let mut consoles = world.query::<(Entity, &ControlConsole)>();
    let lost: Vec<Entity> = consoles
        .iter(world)
        .filter(|(_, c)| c.drone.is_some_and(|d| world.get::<Exodrone>(d).is_none()))
        .map(|(e, _)| e)
        .collect();
    for console in lost {
        if let Some(mut c) = world.get_mut::<ControlConsole>(console) {
            c.drone = None;
            c.signal_lost = true;
        }
        sound(
            world,
            console,
            "sound/machines/terminal/terminal_alert.ogg",
            20.0,
            true,
        );
    }
    let mut drones = world.query::<(Entity, &Exodrone)>();
    let orphans: Vec<Entity> = drones
        .iter(world)
        .filter(|(e, d)| {
            d.controlled.is_some_and(|c| {
                world
                    .get::<ControlConsole>(c)
                    .is_none_or(|c| c.drone != Some(*e))
            })
        })
        .map(|(e, _)| e)
        .collect();
    for drone in orphans {
        if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
            d.controlled = None;
        }
    }
    // The scan.
    let scan = world.resource::<Exploration>().scan.clone();
    if let Some(scan) = &scan
        && t >= scan.ends_at
    {
        resolve_scan(world);
    }
    if t >= *next_check {
        *next_check = t + POWER_CHECK_SECS;
        let ready = dishes(world);
        if let Some(scan) = world.resource::<Exploration>().scan.clone() {
            // Not tg: a wide scan compares the array's power, where tg compares the number of
            // dishes and so interrupts upgraded arrays on any dish change.
            let conditions = site_conditions(world, scan.target);
            if scan_power(world, &ready, &conditions) < scan.power {
                interrupt_scan(world);
            }
        }
        update_dish_states(world, &ready);
    }
}

// --- Windows --------------------------------------------------------------------------------

fn open_consoles(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<
        (Option<&DmName>, Has<ControlConsole>),
        (
            Or<(With<ControlConsole>, With<ScannerConsole>)>,
            Without<Unpowered>,
        ),
    >,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok((name, control)) = consoles.get(click.target) else {
            continue;
        };
        if !inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            continue;
        }
        let title = name.map_or("", |n| crate::vending::plain_name(&n.name));
        open.write(if control {
            OpenUi::new::<ExodroneConsoleUi>(click.client, click.mob, click.target, title)
        } else {
            OpenUi::new::<ExoscannerConsoleUi>(click.client, click.mob, click.target, title)
        });
    }
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<
        (),
        (
            Or<(With<ControlConsole>, With<ScannerConsole>)>,
            With<Unpowered>,
        ),
    >,
) {
    for mut window in &mut windows {
        if (window.interface == ExodroneConsoleUi::NAME
            || window.interface == ExoscannerConsoleUi::NAME)
            && unpowered.contains(window.source)
        {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// `site_data(exploration)`.
fn site_info(site: &Site, exploration: bool) -> SiteInfo {
    SiteInfo {
        id: site.id,
        name: site.display_name().to_owned(),
        coordinates: site.coordinates.clone(),
        description: site.display_description(),
        distance: site.distance,
        revealed: site.revealed,
        point_scan_complete: site.point_scan_complete,
        deep_scan_complete: site.deep_scan_complete,
        band_info: if site.point_scan_complete {
            site.band_info.clone()
        } else {
            Vec::new()
        },
        events: if exploration {
            site.events
                .iter()
                .filter(|e| e.visited && e.is_targetable())
                .map(|e| (e.name.clone(), e.id))
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// `ui_description`.
fn drone_description(world: &World, d: &Exodrone) -> String {
    match d.status {
        DroneStatus::Travel => match d.travel_target {
            Some(t) => format!("Traveling to {}.", site_name(world, Some(t))),
            None => "Traveling back to station.".to_owned(),
        },
        DroneStatus::Idle => "Idle.".to_owned(),
        _ => format!("Exploring {}.", site_name(world, d.location)),
    }
}

/// `ui_data` of the control console.
fn console_view(world: &World, console: Entity) -> ExodroneData {
    let Some(c) = world.get::<ControlConsole>(console) else {
        return ExodroneData::default();
    };
    let t = now(world);
    let mut data = ExodroneData {
        signal_lost: c.signal_lost,
        ..default()
    };
    let ex = world.resource::<Exploration>();
    let sites = || {
        ex.sites
            .iter()
            .map(|s| site_info(s, false))
            .collect::<Vec<_>>()
    };
    if let Some(drone) = c.drone
        && let Some(d) = world.get::<Exodrone>(drone)
    {
        let (integrity, max_integrity) = integrity(world, drone);
        let mut cargo: Vec<CargoSlot> = d.tools.iter().map(|&t| CargoSlot::Tool(t)).collect();
        cargo.extend(
            cargo_items(world, drone)
                .into_iter()
                .map(|e| CargoSlot::Cargo {
                    name: name_of(world, e),
                    id: e.to_bits(),
                }),
        );
        cargo.extend((0..space_left(world, drone)).map(|_| CargoSlot::Empty));
        let error = travel_error(world, drone);
        let screen = match d.status {
            DroneStatus::Idle => DroneScreen::Idle { sites: sites() },
            DroneStatus::Travel => DroneScreen::Travel {
                travel_time: d.travel_time,
                travel_time_left: (d.travel_ends_at - t).max(0.0),
            },
            DroneStatus::Busy => DroneScreen::Busy {
                wait_time_left: (d.busy_ends_at - t).max(0.0),
                wait_message: d.busy_message.clone(),
            },
            DroneStatus::Exploration => DroneScreen::Exploration {
                sites: sites(),
                site: Box::new(
                    d.location
                        .and_then(|s| ex.site(s))
                        .map(|s| site_info(s, true))
                        .unwrap_or_default(),
                ),
                event: d.event.clone(),
            },
            DroneStatus::Adventure => DroneScreen::Adventure(adventure_screen(world, d)),
        };
        data.drone = Some(ActiveDrone {
            name: name_of(world, drone),
            integrity,
            max_integrity,
            travel_coefficient: travel_coeff(world, drone),
            log: d.log.clone(),
            configurable: d.status == DroneStatus::Idle,
            cargo,
            can_travel: error.is_none(),
            travel_error: error.unwrap_or_default(),
            screen,
        });
    } else {
        let mut query = world.try_query::<(Entity, &Exodrone)>();
        let mut drones: Vec<(Entity, &Exodrone)> = query
            .as_mut()
            .map(|q| q.iter(world).collect())
            .unwrap_or_default();
        drones.sort_by_key(|d| d.0);
        data.all_drones = drones
            .into_iter()
            .map(|(e, d)| DroneListing {
                name: name_of(world, e),
                controlled: d.controlled.is_some(),
                description: drone_description(world, d),
                id: e.to_bits(),
            })
            .collect();
    }
    data
}

/// `ui_data` of the scanner console.
fn scanner_view(
    world: &World,
    console: Entity,
    ready: &[(Entity, TilePos, f64)],
) -> ExoscannerData {
    let Some(c) = world.get::<ScannerConsole>(console) else {
        return ExoscannerData::default();
    };
    let ex = world.resource::<Exploration>();
    let mut data = ExoscannerData {
        failed: c.failed,
        ..default()
    };
    if let Some(site) = c.selected_site.and_then(|s| ex.site(s)) {
        let power = scan_power(world, ready, &site.conditions);
        data.scan_power = power;
        if power > 0.0 {
            data.point_scan_eta = BASE_POINT_SCAN_TIME / power;
            data.deep_scan_eta = BASE_DEEP_SCAN_TIME * f64::from(site.distance) / power;
        }
        data.scan_conditions = site
            .conditions
            .iter()
            .map(|c| c.description().to_owned())
            .collect();
        data.selected_site = Some(site_info(site, false));
    } else {
        let power = scan_power(world, ready, &[]);
        data.scan_power = power;
        if power > 0.0 {
            data.wide_scan_eta = wide_scan_cost(ex.wide_scan_band, power);
        }
        data.possible_sites = ex
            .sites
            .iter()
            .map(|s| (s.id, s.display_name().to_owned()))
            .collect();
    }
    if let Some(scan) = &ex.scan {
        let target = scan
            .target
            .and_then(|t| ex.site(t))
            .map_or("", |s| s.display_name());
        data.scan = Some(ScanProgress {
            time_left: (scan.ends_at - now(world)).max(0.0),
            power: scan.power,
            description: match scan.kind {
                ScanKind::Wide => {
                    "Wide: Scanning sphere starting 1 AU from the station.".to_owned()
                }
                ScanKind::Point => format!("Point scan of {target}"),
                ScanKind::Deep => format!("Deep scan of {target}"),
            },
        });
    }
    data
}

/// Builds the data of consoles with an open window.
fn refresh_views(world: &mut World) {
    let mut windows = world.query::<&UiWindow>();
    let mut control: Vec<Entity> = Vec::new();
    let mut scanner: Vec<Entity> = Vec::new();
    for w in windows.iter(world) {
        if w.interface == ExodroneConsoleUi::NAME {
            control.push(w.source);
        } else if w.interface == ExoscannerConsoleUi::NAME {
            scanner.push(w.source);
        }
    }
    control.sort();
    control.dedup();
    scanner.sort();
    scanner.dedup();
    for console in control {
        let view = console_view(world, console);
        if let Some(mut c) = world.get_mut::<ControlConsole>(console) {
            c.view = view;
        }
    }
    if !scanner.is_empty() {
        let ready = dishes(world);
        for console in scanner {
            let view = scanner_view(world, console, &ready);
            if let Some(mut c) = world.get_mut::<ScannerConsole>(console) {
                c.view = view;
            }
        }
    }
}

// --- Acts -----------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExodroneAct {
    SelectDrone(u64),
    EndControl,
    ConfirmSignalLost,
    SelfDestruct,
    AddTool(Tool),
    RemoveTool(Tool),
    StartTravel(Option<u32>),
    Explore,
    ExploreEvent(u32),
    AdventureChoice(String),
    StartEvent,
    SkipEvent,
    Jettison(u64),
}

fn parse_num<T: std::str::FromStr>(params: &UiParams, key: &str) -> Result<T, UiReject> {
    params
        .get(key)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| UiReject::BadParams(format!("{key}: expected a number")))
}

/// An optional id param: absent or empty is `None`.
fn parse_opt(params: &UiParams, key: &str) -> Result<Option<u32>, UiReject> {
    match params.get(key) {
        None | Some("") | Some("null") => Ok(None),
        Some(v) => v
            .parse()
            .map(Some)
            .map_err(|_| UiReject::BadParams(format!("{key}: expected an id"))),
    }
}

fn parse_tool(params: &UiParams) -> Result<Tool, UiReject> {
    params
        .get(TOOL_TYPE)
        .and_then(Tool::from_name)
        .ok_or_else(|| UiReject::BadParams(format!("{TOOL_TYPE}: unknown tool")))
}

impl ServerUi for ExodroneConsoleUi {
    type Source = &'static ControlConsole;
    type UserParam = ();
    type Act = ExodroneAct;

    fn static_data(_: &ControlConsole) {}

    fn data(console: &ControlConsole, _: Entity, _: &()) -> ExodroneData {
        console.view.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ExodroneAct, UiReject> {
        Ok(match action {
            SELECT_DRONE => ExodroneAct::SelectDrone(parse_num(params, DRONE_REF)?),
            END_CONTROL => ExodroneAct::EndControl,
            CONFIRM_SIGNAL_LOST => ExodroneAct::ConfirmSignalLost,
            SELF_DESTRUCT => ExodroneAct::SelfDestruct,
            ADD_TOOL => ExodroneAct::AddTool(parse_tool(params)?),
            REMOVE_TOOL => ExodroneAct::RemoveTool(parse_tool(params)?),
            START_TRAVEL => ExodroneAct::StartTravel(parse_opt(params, TARGET_SITE)?),
            EXPLORE => ExodroneAct::Explore,
            EXPLORE_EVENT => ExodroneAct::ExploreEvent(parse_num(params, TARGET_EVENT)?),
            ADVENTURE_CHOICE => ExodroneAct::AdventureChoice(
                params
                    .get(CHOICE)
                    .ok_or_else(|| UiReject::BadParams(format!("{CHOICE}: missing")))?
                    .to_owned(),
            ),
            START_EVENT => ExodroneAct::StartEvent,
            SKIP_EVENT => ExodroneAct::SkipEvent,
            JETTISON => ExodroneAct::Jettison(parse_num(params, TARGET_REF)?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `start_drone_control`: only a drone no other console controls.
fn start_control(world: &mut World, console: Entity, drone: Entity) {
    if world
        .get::<Exodrone>(drone)
        .is_none_or(|d| d.controlled.is_some())
    {
        return;
    }
    end_control(world, console);
    if let Some(mut c) = world.get_mut::<ControlConsole>(console) {
        c.drone = Some(drone);
    }
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.controlled = Some(console);
    }
    sound(
        world,
        console,
        "sound/machines/terminal/terminal_on.ogg",
        20.0,
        true,
    );
}

/// `end_drone_control`.
fn end_control(world: &mut World, console: Entity) {
    let Some(drone) = world
        .get_mut::<ControlConsole>(console)
        .and_then(|mut c| c.drone.take())
    else {
        return;
    };
    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
        d.controlled = None;
    }
    sound(
        world,
        console,
        "sound/machines/terminal/terminal_off.ogg",
        20.0,
        true,
    );
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ExodroneConsoleUi>>>) {
    let pending: Vec<(Entity, ExodroneAct)> = acts
        .read(world.resource::<Messages<UiActed<ExodroneConsoleUi>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (console, act) in pending {
        if world.get::<Unpowered>(console).is_some() {
            continue;
        }
        let Some(c) = world.get::<ControlConsole>(console) else {
            continue;
        };
        let drone = c.drone.filter(|&d| {
            world
                .get::<Exodrone>(d)
                .is_some_and(|x| x.controlled == Some(console))
        });
        let status = drone
            .and_then(|d| world.get::<Exodrone>(d))
            .map(|d| d.status);
        match act {
            ExodroneAct::SelectDrone(bits) => {
                // `locate(params["drone_ref"]) in GLOB.exodrones`.
                if let Some(target) = Entity::try_from_bits(bits)
                    && world.get::<Exodrone>(target).is_some()
                {
                    start_control(world, console, target);
                }
            }
            ExodroneAct::EndControl => end_control(world, console),
            ExodroneAct::ConfirmSignalLost => {
                if let Some(mut c) = world.get_mut::<ControlConsole>(console) {
                    c.signal_lost = false;
                }
            }
            ExodroneAct::SelfDestruct => {
                if let Some(drone) = drone {
                    self_destruct(world, drone);
                }
            }
            ExodroneAct::AddTool(tool) => {
                if let Some(drone) = drone
                    && status == Some(DroneStatus::Idle)
                    && space_left(world, drone) > 0
                {
                    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
                        d.tools.push(tool);
                    }
                    update_storage_size(world, drone);
                }
            }
            ExodroneAct::RemoveTool(tool) => {
                if let Some(drone) = drone
                    && status == Some(DroneStatus::Idle)
                {
                    // `tools -= tool_type`: every copy.
                    if let Some(mut d) = world.get_mut::<Exodrone>(drone) {
                        d.tools.retain(|&t| t != tool);
                    }
                    update_storage_size(world, drone);
                }
            }
            ExodroneAct::StartTravel(target) => {
                let Some(drone) = drone else {
                    continue;
                };
                if travel_error(world, drone).is_some() {
                    continue;
                }
                if let Some(site) = target {
                    if world.resource::<Exploration>().site(site).is_none() {
                        continue;
                    }
                    if !check_blacklist(world, drone) {
                        crate::atmos_machinery::machine_say(
                            world,
                            console,
                            "Error - An unauthorized object was found inside the cargo!",
                        );
                        continue;
                    }
                } else if world
                    .get::<Exodrone>(drone)
                    .is_some_and(|d| d.location.is_none())
                {
                    // Home from home: tg's UI never offers it.
                    continue;
                }
                launch_for(world, drone, target);
            }
            ExodroneAct::Explore => {
                if let Some(drone) = drone
                    && status == Some(DroneStatus::Exploration)
                    && world
                        .get::<Exodrone>(drone)
                        .is_some_and(|d| d.event.is_none())
                {
                    explore_site(world, drone, None);
                }
            }
            ExodroneAct::ExploreEvent(event) => {
                if let Some(drone) = drone
                    && status == Some(DroneStatus::Exploration)
                    && world
                        .get::<Exodrone>(drone)
                        .is_some_and(|d| d.event.is_none())
                {
                    explore_site(world, drone, Some(event));
                }
            }
            ExodroneAct::AdventureChoice(key) => {
                if let Some(drone) = drone
                    && status == Some(DroneStatus::Adventure)
                {
                    adventure_choice(world, drone, &key);
                }
            }
            ExodroneAct::StartEvent => {
                if let Some(drone) = drone
                    && status == Some(DroneStatus::Exploration)
                    && let Some(d) = world.get::<Exodrone>(drone)
                    && let (Some(site), Some(event)) = (d.location, d.event.as_ref().map(|e| e.id))
                {
                    fire_event(world, drone, site, event);
                }
            }
            ExodroneAct::SkipEvent => {
                if let Some(drone) = drone
                    && let Some(d) = world.get::<Exodrone>(drone)
                    && let (Some(site), Some(event)) = (d.location, d.event.as_ref().map(|e| e.id))
                    && world
                        .resource::<Exploration>()
                        .site(site)
                        .and_then(|s| s.event(event))
                        .is_some_and(|e| e.skippable())
                {
                    end_event(world, drone);
                }
            }
            ExodroneAct::Jettison(bits) => {
                let Some(drone) = drone else {
                    continue;
                };
                // `locate(params["target_ref"]) in controlled_drone.contents`.
                let Some(item) =
                    Entity::try_from_bits(bits).filter(|e| cargo_items(world, drone).contains(e))
                else {
                    continue;
                };
                let name = name_of(world, item);
                drone_log(world, drone, format!("Jettisoned {name}"));
                let drop = world.get::<TilePos>(drone).copied().or_else(|| {
                    world
                        .get::<CarriedBy>(drone)
                        .and_then(|c| world.get::<TilePos>(c.0))
                        .copied()
                });
                match drop {
                    Some(pos) if status == Some(DroneStatus::Idle) => {
                        crate::storage::take_out(world, item, pos);
                    }
                    _ => {
                        world.despawn(item);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScannerAct {
    SelectSite(Option<u32>),
    StopScan,
    StartWide,
    StartPoint,
    StartDeep,
    ConfirmFail,
    OpenExperiments,
}

impl ServerUi for ExoscannerConsoleUi {
    type Source = &'static ScannerConsole;
    type UserParam = ();
    type Act = ScannerAct;

    fn static_data(_: &ScannerConsole) {}

    fn data(console: &ScannerConsole, _: Entity, _: &()) -> ExoscannerData {
        console.view.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ScannerAct, UiReject> {
        Ok(match action {
            SELECT_SITE => ScannerAct::SelectSite(parse_opt(params, SITE_REF)?),
            STOP_SCAN => ScannerAct::StopScan,
            START_WIDE_SCAN => ScannerAct::StartWide,
            START_POINT_SCAN => ScannerAct::StartPoint,
            START_DEEP_SCAN => ScannerAct::StartDeep,
            CONFIRM_FAIL => ScannerAct::ConfirmFail,
            OPEN_EXPERIMENTS => ScannerAct::OpenExperiments,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn scanner_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ExoscannerConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, ScannerAct)> = acts
        .read(world.resource::<Messages<UiActed<ExoscannerConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if world.get::<Unpowered>(console).is_some() {
            continue;
        }
        let Some(selected) = world
            .get::<ScannerConsole>(console)
            .map(|c| c.selected_site)
        else {
            continue;
        };
        let site =
            |world: &World| selected.and_then(|s| world.resource::<Exploration>().site(s).cloned());
        match act {
            ScannerAct::SelectSite(target) => {
                let valid = target.filter(|&s| world.resource::<Exploration>().site(s).is_some());
                // An unknown ref keeps the selection (tg `if(site)`).
                if (target.is_none() || valid.is_some())
                    && let Some(mut c) = world.get_mut::<ScannerConsole>(console)
                {
                    c.selected_site = valid;
                }
            }
            ScannerAct::StopScan => {
                if world.resource::<Exploration>().scan.is_some() {
                    interrupt_scan(world);
                }
            }
            ScannerAct::StartWide => {
                if world.resource::<Exploration>().wide_scan_band <= MAX_SCAN_DISTANCE {
                    start_scan(world, console, ScanKind::Wide, None);
                }
            }
            ScannerAct::StartPoint => {
                if let Some(s) = site(&*world)
                    && !s.point_scan_complete
                {
                    start_scan(world, console, ScanKind::Point, Some(s.id));
                }
            }
            ScannerAct::StartDeep => {
                if let Some(s) = site(&*world)
                    && !s.deep_scan_complete
                {
                    start_scan(world, console, ScanKind::Deep, Some(s.id));
                }
            }
            ScannerAct::ConfirmFail => {
                if let Some(mut c) = world.get_mut::<ScannerConsole>(console) {
                    c.failed = false;
                }
            }
            ScannerAct::OpenExperiments => {
                if world
                    .get::<crate::science::experi_scanner::ExperimentHandler>(console)
                    .is_some()
                {
                    let name = world.get::<DmName>(console).map_or_else(String::new, |n| {
                        crate::vending::plain_name(&n.name).to_owned()
                    });
                    let title = format!(
                        "{} | Конфигурация Экспериментов",
                        ss13_shared::capitalize(&name)
                    );
                    world.write_message(ss13_tgui::server::OpenUi::new::<
                        crate::science::ExperimentConfigureUi,
                    >(client, actor, console, title));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::super::sites::{Event, Resource, SiteKind};
    use super::*;

    const ADVENTURE: &str = r#"{"adventure_name": "x", "version": 1, "starting_node": "a",
        "required_site_traits": ["in space"], "loot_categories": ["nothing"],
        "nodes": [{"name": "a", "description": "At $$SITE_NAME.", "choices": [
            {"key": "win", "name": "Win", "exit_node": "WIN", "delay": 20},
            {"key": "drill", "name": "Drill", "exit_node": "FAIL",
             "requirements": [{"quality": "drill", "operator": "exists"}]}]}]}"#;

    /// A drone exploring a revealed junkyard (its first event is the adventure) and a second
    /// junkyard.
    fn setup() -> (World, Entity, u32, u32) {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let entry = AdventureEntry::parse("x", ADVENTURE).unwrap();
        let mut ex = Exploration::new(Strings::default(), vec![entry], 5);
        let mut here = ex.new_site(SiteKind::Junkyard, 1);
        here.revealed = true;
        let there = ex.new_site(SiteKind::Junkyard, 3);
        let (a, b) = (here.id, there.id);
        ex.sites.extend([here, there]);
        world.insert_resource(ex);
        let drone = world
            .spawn(Exodrone {
                status: DroneStatus::Exploration,
                location: Some(a),
                tools: vec![Tool::Drill],
                ..default()
            })
            .id();
        (world, drone, a, b)
    }

    fn drone(world: &World, e: Entity) -> &Exodrone {
        world.get::<Exodrone>(e).unwrap()
    }

    #[test]
    fn adventure_with_a_delayed_choice() {
        let (mut world, e, a, _) = setup();
        let event = world.resource::<Exploration>().site(a).unwrap().events[0].id;
        encounter(&mut world, e, a, event);
        assert_eq!(drone(&world, e).status, DroneStatus::Adventure);
        assert_eq!(drone(&world, e).log[0], "Encountered something unexpected");
        let screen = adventure_screen(&world, drone(&world, e));
        assert_eq!(screen.description, "At space junk field.");
        // The drill tool is a quality.
        assert_eq!(screen.choices.len(), 2);
        adventure_choice(&mut world, e, "win");
        assert_eq!(drone(&world, e).status, DroneStatus::Busy);
        assert_eq!(drone(&world, e).busy_end, BusyEnd::Adventure("WIN".into()));
        world.get_mut::<Exodrone>(e).unwrap().busy_ends_at = 0.0;
        world.run_system_once(tick).unwrap();
        assert_eq!(drone(&world, e).status, DroneStatus::Exploration);
        assert_eq!(drone(&world, e).adventure, None);
        // Visited and not revisitable.
        let site = world.resource::<Exploration>().site(a).unwrap().clone();
        assert!(site.events[0].visited && !site.events[0].is_targetable());
    }

    #[test]
    fn resource_event_takes_time_then_runs_out() {
        let (mut world, e, a, _) = setup();
        let event = Event {
            id: 999,
            name: "cache".into(),
            visited: false,
            band_values: Vec::new(),
            discovery_log: Some("Found a cache.".into()),
            point_scan_description: None,
            deep_scan_description: None,
            description: "A cache.".into(),
            action_text: "Mine".into(),
            kind: EventKind::Resource(Resource {
                required_tool: Some(Tool::Drill),
                loot: Loot::Item(Spawn {
                    path: "/obj/item/nothing",
                    amount: 0,
                }),
                success_log: "Mined it.".into(),
                no_tool_description: "No drill.".into(),
                has_tool_description: "Drill it!".into(),
                delay: 30.0,
                delay_message: "Mining...".into(),
                amount: 1,
            }),
        };
        world
            .resource_mut::<Exploration>()
            .site_mut(a)
            .unwrap()
            .events
            .push(event);
        encounter(&mut world, e, a, 999);
        let screen = drone(&world, e).event.clone().unwrap();
        assert!(screen.action_enabled && screen.skippable);
        assert_eq!(screen.description, "A cache.\nDrill it!");
        fire_event(&mut world, e, a, 999);
        assert_eq!(drone(&world, e).status, DroneStatus::Busy);
        assert_eq!(drone(&world, e).busy_message, "Mining...");
        world.get_mut::<Exodrone>(e).unwrap().busy_ends_at = 0.0;
        world.run_system_once(tick).unwrap();
        let d = drone(&world, e);
        assert_eq!(d.status, DroneStatus::Exploration);
        assert!(d.event.is_none());
        assert!(d.log.iter().any(|l| l == "Mined it."));
        let ex = world.resource::<Exploration>();
        assert!(!ex.site(a).unwrap().event(999).unwrap().is_targetable());
    }

    #[test]
    fn site_to_site_travel() {
        let (mut world, e, a, b) = setup();
        let (da, db) = {
            let ex = world.resource::<Exploration>();
            (ex.site(a).unwrap().distance, ex.site(b).unwrap().distance)
        };
        assert_eq!(travel_error(&world, e), None);
        launch_for(&mut world, e, Some(b));
        let d = drone(&world, e);
        assert_eq!(d.status, DroneStatus::Travel);
        assert_eq!(
            d.travel_time,
            Fuel::Basic.coefficient() * f64::from(da.abs_diff(db).max(1))
        );
        assert_eq!(travel_error(&world, e), Some(String::new()));
        world.get_mut::<Exodrone>(e).unwrap().travel_ends_at = 0.0;
        world.run_system_once(tick).unwrap();
        let d = drone(&world, e);
        assert_eq!(d.status, DroneStatus::Exploration);
        assert_eq!(d.location, Some(b));
        assert!(d.log[0].starts_with("Discovered space junk field at "));
        assert!(world.resource::<Exploration>().site(b).unwrap().revealed);
    }

    #[test]
    fn numerals_and_fuel() {
        assert_eq!(roman(2), "II");
        assert_eq!(roman(14), "XIV");
        assert_eq!(Fuel::of(FUEL_PELLET), Fuel::Basic);
        assert_eq!(Fuel::of(FUEL_EXOTIC), Fuel::Exotic);
        assert_eq!(Fuel::Advanced.coefficient(), 20.0);
    }

    #[test]
    fn acts_parse() {
        let p = |v: &[(&str, &str)]| -> Vec<(String, String)> {
            v.iter()
                .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
                .collect()
        };
        let params = p(&[(TOOL_TYPE, "drill")]);
        assert_eq!(
            ExodroneConsoleUi::parse_act(ADD_TOOL, &UiParams::new(&params)),
            Ok(ExodroneAct::AddTool(Tool::Drill))
        );
        let params = p(&[(TOOL_TYPE, "rocket")]);
        assert!(ExodroneConsoleUi::parse_act(ADD_TOOL, &UiParams::new(&params)).is_err());
        let params = p(&[]);
        assert_eq!(
            ExodroneConsoleUi::parse_act(START_TRAVEL, &UiParams::new(&params)),
            Ok(ExodroneAct::StartTravel(None))
        );
        let params = p(&[(SITE_REF, "4")]);
        assert_eq!(
            ExoscannerConsoleUi::parse_act(SELECT_SITE, &UiParams::new(&params)),
            Ok(ScannerAct::SelectSite(Some(4)))
        );
        assert!(ExoscannerConsoleUi::parse_act("nope", &UiParams::new(&params)).is_err());
    }
}
