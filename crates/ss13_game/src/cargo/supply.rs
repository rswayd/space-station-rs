//! The supply shuttle and orders (server), after tg `SSshuttle.supply`
//! (`/obj/docking_port/mobile/supply`, `code/modules/shuttle/mobile_port/variants/supply.dm`),
//! `/datum/supply_pack` and `/datum/supply_order` (`code/modules/cargo/`).
//!
//! - The map json's `"cargo"` shuttle (MetaStation: `cargo_box`) loads with the station onto its
//!   own level, which stands in for CentCom: the shuttle starts there, docked at `cargo_away`
//!   ("CentCom", tg's roundstart `json_key = "cargo"` dock). The station's `cargo_home` dock is
//!   where it lands. Moves reuse the emergency shuttle's [`initiate_docking`].
//! - Modes (tg `request`/`check`): idle -> igniting (`ignitionTime` 5.5 s) -> in transit
//!   (`callTime` 60 s) -> docked at the destination, idle. The supply shuttle only takes a
//!   request while idle.
//! - Leaving CentCom it buys (`buy()`): every order of the shopping list whose payer can pay
//!   becomes a crate on a free floor tile of the shuttle (`supply_pack/generate`: the crate type,
//!   named `"<crate_name> - #<id>"`, `req_access` from the pack's `access`, the contents, the
//!   shipping manifest inside). Docking at CentCom it sells (`sell()`): everything unanchored
//!   aboard goes through [`export_item_and_contents`], the money into the cargo budget, the
//!   report to `centcom_message`.
//! - The shuttle refuses to leave the station with living mobs aboard (tg `check_blacklist`).
//! - A request approved with a department's budget is paid by that department at purchase.
//! - A private or department order's crate is named "<crate> - Purchased by <holder>" and is
//!   account-locked to the payer ([`AccountLock`], tg `locked_to_account`).
//! - Goodies (`ORDER_GOODY`, private purchases only) come per buyer: up to
//!   `GOODY_FREE_SHIPPING_MAX` orders in a goody case (`lockbox/order`, unlocked by the buyer's
//!   card), more in an account-locked secure crate, the first order past the limit charged the
//!   `CRATE_TAX`; one manifest lists all the buyer's orders.
//! - Leaving CentCom it also loads the waiting mail ([`super::mail::create_mail`]).
//! - A shuttle loan ([`LoanOffer`], `crate::events::shuttle_loan`) calls it from CentCom
//!   straight to the station dock, without transit.
// shortcut: departmental order consoles, chef groceries, the materials market, requisition
// papers and manifest errors are not ported. Packs with their own `fill()` are not offered.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use ss13_mapload::cargo::{CargoData as PackData, MANIFEST_PAPER, SupplyPackDef, order_flags};
use ss13_mapload::dock::{map_shuttle, shuttle_template_path};
use ss13_mapload::spawner::{DmSpawner, Rng, pick};
use ss13_mapload::{
    DmAtom, DmCloset, DmItem, DmName, DmObj, IconIndex, MapLoadPlugin, MapSeed, ShuttleTemplates,
    Template, Turf,
};
use ss13_net::server::Networked;
use ss13_proto::Prototypes;
use ss13_shared::TilePos;
use ss13_world::{Density, SpatialIndex};

use super::economy::{ACCOUNT_CAR, Economy};
use super::export::{ExportReport, Exports, export_item_and_contents};
use crate::closets::{AccountLock, Closet};
use crate::construction::path_of;
use crate::obj_damage::{Anchored, is_under};
use crate::shuttle::{
    MobileDock, ShuttleTimer, StationaryDock, initiate_docking, ordered_turfs,
    setup_template_shuttle,
};
use crate::storage::StoredIn;

/// tg `/obj/docking_port/mobile/supply` `callTime`.
pub const CALL_SECS: f64 = 60.0;
/// tg `/obj/docking_port/mobile` `ignitionTime`.
pub const IGNITION_SECS: f64 = 5.5;
/// `CARGO_MAX_ORDER`.
pub const CARGO_MAX_ORDER: u32 = 50;
/// `CARGO_CRATE_VALUE`.
pub const CARGO_CRATE_VALUE: f64 = 200.0;
/// `GOODY_FREE_SHIPPING_MAX`: goody orders per buyer that fit in a free goody case.
pub const GOODY_FREE_SHIPPING_MAX: usize = 5;
/// `CRATE_TAX`: the charge on the first goody order past the free shipping limit.
pub const CRATE_TAX: i64 = 700;
const SUPPLY_PORT: &str = "/obj/docking_port/mobile/supply";
const HOME_DOCK: &str = "cargo_home";
const AWAY_DOCK: &str = "cargo_away";
const FLOOR: &str = "/turf/open/floor";
/// The console's default `message`.
pub const DEFAULT_MESSAGE: &str = "Remember to stamp and send back the supply manifests.";

/// Supply shuttle timing. Insert before `GameServerPlugin` to override; otherwise tg's with
/// `SS13_SUPPLY_CALL_SECONDS` (`callTime`) and `SS13_SUPPLY_IGNITION_SECONDS` (`ignitionTime`).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct SupplyConfig {
    pub call_secs: f64,
    pub ignition_secs: f64,
    /// The supply shuttle `.dmm` (absolute); `None`: the map json's `"cargo"`.
    pub template: Option<PathBuf>,
}

impl Default for SupplyConfig {
    fn default() -> Self {
        let secs = |key: &str, default: f64| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(default)
        };
        Self {
            call_secs: secs("SS13_SUPPLY_CALL_SECONDS", CALL_SECS),
            ignition_secs: secs("SS13_SUPPLY_IGNITION_SECONDS", IGNITION_SECS),
            template: None,
        }
    }
}

/// `SSshuttle.supply_packs` with the prototypes a delivery spawns.
#[derive(Resource, Default)]
pub struct SupplyPacks {
    pub packs: Vec<SupplyPackDef>,
    by_id: HashMap<String, usize>,
    pub templates: HashMap<String, Template>,
}

impl SupplyPacks {
    pub fn new(packs: Vec<SupplyPackDef>) -> Self {
        let by_id = packs
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();
        Self {
            packs,
            by_id,
            templates: HashMap::new(),
        }
    }

    pub fn by_id(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    /// `name_to_id`.
    pub fn by_name(&self, name: &str) -> Option<usize> {
        self.packs.iter().position(|p| p.name == name)
    }
}

/// Whether a console may order `pack` (tg `add_item`'s flag checks and the catalogue filter):
/// not invisible, emag-only on a console that is not emagged, a disabled special, contraband
/// (no hacked boards yet) or pod-only. Packs needing what is not ported (their own `fill()`,
/// live animals, nothing to spawn) are hidden too.
pub fn orderable(pack: &SupplyPackDef, contraband: bool, emagged: bool) -> bool {
    orderable_with(pack, contraband, emagged, false)
}

/// [`orderable`] for a console, `express` (the express console) also offering pod-only packs.
pub fn orderable_with(
    pack: &SupplyPackDef,
    contraband: bool,
    emagged: bool,
    express: bool,
) -> bool {
    use order_flags::*;
    !(pack.has(INVISIBLE)
        || (pack.has(EMAG_ONLY) && !emagged)
        || (pack.has(CONTRABAND) && !contraband)
        || (pack.has(POD_ONLY) && !express)
        || (pack.has(SPECIAL) && !pack.has(SPECIAL_ENABLED))
        || pack.custom_fill
        // Goodies come in the buyer's goody case instead.
        || (pack.crate_type.is_none() && !pack.has(GOODY))
        || pack.contains.is_empty()
        // shortcut: no mobs inside containers yet (critter crates).
        || pack.contains.iter().any(|(t, _)| is_under(t, "/mob")))
}

/// tg `/datum/supply_order`.
#[derive(Clone, Debug, PartialEq)]
pub struct SupplyOrder {
    pub id: u32,
    /// Index into [`SupplyPacks::packs`].
    pub pack: usize,
    pub orderer: String,
    pub orderer_rank: String,
    pub reason: String,
    /// A private purchase (`paying_account`), else the cargo budget pays.
    pub paying_account: Option<u32>,
    /// A request paid by a department budget (tg `paying_account` set to a
    /// `/datum/bank_account/department`): no private surcharge, no handling fee.
    pub paying_department: Option<String>,
    pub can_be_cancelled: bool,
    /// `applied_coupon`: the coupon (held out of the world until shipped or refunded) and its
    /// discount on a 0..1 range.
    pub coupon: Option<(Entity, f64)>,
}

impl SupplyOrder {
    /// tg `get_final_cost`: private purchases of non-goodies cost 1.1x, a coupon takes its
    /// discount off.
    pub fn final_cost(&self, packs: &SupplyPacks) -> i64 {
        let pack = &packs.packs[self.pack];
        let mut cost = pack.cost;
        if self.paying_account.is_some() && !pack.has(order_flags::GOODY) {
            cost *= 1.1;
        }
        if let Some((_, discount)) = self.coupon {
            cost *= 1.0 - discount;
        }
        cost.round() as i64 + super::materials_market::crate_charge(pack)
    }
}

/// `SSshuttle.shopping_list`, `request_list`, `order_number` and `centcom_message`.
#[derive(Resource, Debug)]
pub struct SupplyOrders {
    pub shopping_list: Vec<SupplyOrder>,
    pub request_list: Vec<SupplyOrder>,
    pub order_number: u32,
    pub centcom_message: String,
}

impl SupplyOrders {
    /// `order_number = rand(1, 9000)`.
    pub fn new(seed: u64) -> Self {
        Self {
            shopping_list: Vec::new(),
            request_list: Vec::new(),
            order_number: 1 + (Rng::new(seed ^ 0x6f72_6472).next_u64() % 9000) as u32,
            centcom_message: String::new(),
        }
    }

    /// tg `get_order_count`: orders of `pack` in both lists, `None` at `CARGO_MAX_ORDER`.
    pub fn order_count(&self, pack: usize) -> Option<u32> {
        let n = self
            .shopping_list
            .iter()
            .chain(&self.request_list)
            .filter(|o| o.pack == pack)
            .count() as u32;
        (n < CARGO_MAX_ORDER).then_some(n)
    }

    /// A new order's id (`SSshuttle.order_number++`).
    pub fn next_id(&mut self) -> u32 {
        let id = self.order_number;
        self.order_number += 1;
        id
    }
}

/// tg `SHUTTLE_*` modes the supply shuttle uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SupplyMode {
    #[default]
    Idle,
    Igniting,
    Call,
    /// Held by a trade blockade (tg `SHUTTLE_STRANDED`).
    Stranded,
}

/// Where the supply shuttle is docked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupplyLocation {
    /// `cargo_away`.
    CentCom,
    Transit,
    /// `cargo_home`.
    Station,
}

/// The round's supply shuttle (tg `SSshuttle.supply`).
#[derive(Resource)]
pub struct SupplyShuttle {
    pub port: Entity,
    pub name: String,
    pub home: Option<Entity>,
    pub away: Entity,
    pub transit: Entity,
    pub centcom_z: u16,
    pub mode: SupplyMode,
    pub timer: ShuttleTimer,
    /// Where a request sends it: `true` the station.
    pub to_station: bool,
    pub location: SupplyLocation,
    /// The docks' names (tg `getStatusText` shows the docked port's `name`).
    pub home_name: String,
    pub away_name: String,
    /// tg `SSshuttle.supply_blocked`: a trade blockade is registered.
    pub blocked: bool,
    /// tg `SSshuttle.shuttle_loan`: the offered loan (`crate::events::shuttle_loan`).
    pub loan: Option<LoanOffer>,
    rng: Rng,
}

/// An offered shuttle loan: its situation (index into `crate::events::shuttle_loan`'s table)
/// and whether the shuttle went (tg `dispatched`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoanOffer {
    pub situation: usize,
    pub dispatched: bool,
}

impl SupplyShuttle {
    /// `getDockedId() == "cargo_away"`.
    pub fn at_centcom(&self) -> bool {
        self.location == SupplyLocation::CentCom
    }

    pub fn at_station(&self) -> bool {
        self.location == SupplyLocation::Station
    }

    /// tg `getStatusText`.
    pub fn status_text(&self, now: f64) -> String {
        match self.location {
            SupplyLocation::CentCom => self.away_name.clone(),
            SupplyLocation::Station => self.home_name.clone(),
            SupplyLocation::Transit => {
                let left = self.timer.left(now).unwrap_or(0.0).round() as u64;
                let to = if self.to_station {
                    &self.home_name
                } else {
                    &self.away_name
                };
                format!(
                    "transit towards {to} ({:02}:{:02})",
                    left / 60 % 60,
                    left % 60
                )
            }
        }
    }

    fn destination(&self) -> Option<Entity> {
        if self.to_station {
            self.home
        } else {
            Some(self.away)
        }
    }
}

/// What the cargo budget gained from the last sale (for tests and logs).
#[derive(Resource, Debug, Default, Clone)]
pub struct LastSale {
    pub report: ExportReport,
    pub value: i64,
}

pub struct SupplyPlugin;

impl Plugin for SupplyPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<SupplyConfig>() {
            app.init_resource::<SupplyConfig>();
        }
        let config = app.world().resource::<SupplyConfig>().clone();
        let template = config.template.clone().or_else(|| {
            let map = app.world().get_resource::<MapLoadPlugin>()?;
            let name = map.maps.first().filter(|m| !m.ends_with(".dmm"))?;
            let id = map_shuttle(&map.source, name, "cargo")?;
            Some(shuttle_template_path(&map.source, "cargo", &id))
        });
        if let Some(template) = template {
            info!("supply shuttle template {}", template.display());
            let world = app.world_mut();
            if !world.contains_resource::<ShuttleTemplates>() {
                world.insert_resource(ShuttleTemplates::default());
            }
            world
                .resource_mut::<ShuttleTemplates>()
                .0
                .push(template.clone());
            world.insert_resource(SupplyTemplate(template));
        }
        app.init_resource::<LastSale>()
            .add_systems(
                Startup,
                (
                    load_packs.after(crate::storage::StorageStartup),
                    setup_supply.after(crate::shuttle::setup_shuttle),
                ),
            )
            .add_systems(Update, tick);
    }
}

#[derive(Resource)]
struct SupplyTemplate(PathBuf);

/// `<assets>/cargo.ron` and the prototypes of everything a delivery spawns.
pub(crate) fn load_packs(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(SupplyOrders::new(seed));
    if world.contains_resource::<SupplyPacks>() {
        return; // inserted by a test
    }
    let path = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.join(ss13_mapload::cargo::CARGO_FILE));
    let data = match path.as_deref().map(PackData::read) {
        Some(Ok(data)) => data,
        Some(Err(e)) => {
            warn!("{e:#}; no supply packs or exports (rerun `ss13-mapload convert`)");
            PackData::default()
        }
        None => PackData::default(),
    };
    // Crates and their contents, bounty cubes, mail and its goodies.
    let mut paths: Vec<String> = data
        .spawned_types()
        .into_iter()
        .chain(ss13_mapload::cargo::RUNTIME_TYPES)
        .map(str::to_owned)
        .collect();
    let seed_bounties = !world.contains_resource::<super::bounty::Bounties>();
    if seed_bounties {
        world.insert_resource(super::bounty::Bounties::new(data.bounties, data.jobs, seed));
    }
    world.insert_resource(super::mail::MailGoodies(data.mail_generic_goodies));
    let mut packs = SupplyPacks::new(data.packs);
    if let Some(protos) = world.get_resource::<Prototypes>() {
        let index: HashMap<(&str, &str), ss13_shared::IconStateId> = world
            .get_resource::<IconIndex>()
            .map(|i| {
                i.0.iter()
                    .map(|((dmi, state), id)| ((dmi.as_str(), state.as_str()), *id))
                    .collect()
            })
            .unwrap_or_default();
        while let Some(path) = paths.pop() {
            if packs.templates.contains_key(&path) {
                continue;
            }
            if let Some(proto) = protos.get(&path).cloned() {
                // A mail goodie may be a spawner: its loot is spawned at runtime.
                if let Some(spawner) = proto.get::<ss13_mapload::spawner::DmSpawner>() {
                    paths.extend(
                        spawner
                            .loot
                            .iter()
                            .filter(|e| !e.path.is_empty())
                            .map(|e| e.path.clone()),
                    );
                }
                packs.templates.insert(path, Template::new(proto, &index));
            }
        }
    }
    info!(
        "cargo: {} supply packs, {} exports, {} delivery prototypes",
        packs.packs.len(),
        data.exports.len(),
        packs.templates.len()
    );
    world.insert_resource(packs);
    world.insert_resource(Exports::new(data.exports));
}

/// `SSshuttle.supply` registration: the template's port, `cargo_away` where it was loaded, a
/// transit level, the station's `cargo_home`.
fn setup_supply(world: &mut World) {
    let Some(template) = world.get_resource::<SupplyTemplate>().map(|t| t.0.clone()) else {
        return;
    };
    let Some(shuttle) =
        setup_template_shuttle(world, &template, SUPPLY_PORT, "cargo", AWAY_DOCK, "CentCom")
    else {
        return;
    };
    let mut docks = world.query::<(Entity, &StationaryDock)>();
    let (home, home_name) = docks
        .iter(world)
        .find(|(_, d)| d.id == HOME_DOCK)
        .map_or((None, String::new()), |(e, d)| (Some(e), d.name.clone()));
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    info!(
        "supply shuttle {}: at z{}, home dock {}, transit z{}",
        shuttle.name,
        shuttle.level_z,
        if home.is_some() { "found" } else { "missing" },
        shuttle.transit_z
    );
    world.insert_resource(SupplyShuttle {
        port: shuttle.port,
        name: shuttle.name,
        home,
        away: shuttle.away,
        transit: shuttle.transit,
        centcom_z: shuttle.level_z,
        mode: SupplyMode::Idle,
        timer: ShuttleTimer::default(),
        to_station: true,
        location: SupplyLocation::CentCom,
        home_name,
        away_name: "CentCom".into(),
        blocked: false,
        loan: None,
        rng: Rng::new(seed ^ 0x7375_7070),
    });
}

/// The shuttle's tiles that belong to its areas, where it is now.
fn shuttle_tiles(world: &World, port: Entity) -> Vec<TilePos> {
    let (Some(mobile), Some(&pos)) = (world.get::<MobileDock>(port), world.get::<TilePos>(port))
    else {
        return Vec::new();
    };
    let areas = world.resource::<ss13_mapload::Areas>();
    ordered_turfs(pos, mobile.dir, mobile.size)
        .into_iter()
        .filter(|&p| areas.area_at(p).is_some_and(|a| mobile.areas.contains(&a)))
        .collect()
}

fn on_tile(world: &World, pos: TilePos) -> Vec<Entity> {
    world.resource::<SpatialIndex>().on_tile(pos).to_vec()
}

/// tg `check_blacklist` for what exists here: no living mob and no mail (also inside
/// containers) may ride the supply shuttle away from the station.
// shortcut: of tg's blacklist only `/mob/living` and `/obj/item/mail` exist here.
pub fn can_move(world: &World, shuttle: &SupplyShuttle) -> bool {
    if !shuttle.at_station() {
        return true;
    }
    !shuttle_tiles(world, shuttle.port).into_iter().any(|p| {
        on_tile(world, p).into_iter().any(|e| {
            world.get::<crate::health::Health>(e).is_some() || super::mail::holds_mail(world, e)
        })
    })
}

/// tg `moveShuttle(cargo, dock, timed = TRUE)` -> `request`: only from idle.
pub fn request(world: &mut World, to_station: bool) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let ignition = world.resource::<SupplyConfig>().ignition_secs;
    let Some(mut shuttle) = world.get_resource_mut::<SupplyShuttle>() else {
        return false;
    };
    if shuttle.mode != SupplyMode::Idle || (to_station && shuttle.home.is_none()) {
        return false;
    }
    shuttle.to_station = to_station;
    shuttle.mode = SupplyMode::Igniting;
    shuttle.timer.set(now, ignition);
    true
}

/// `SSshuttle.fire` for the supply shuttle (`check`).
fn tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(shuttle) = world.get_resource::<SupplyShuttle>() else {
        return;
    };
    // `check_effects`: ripples where it is about to land.
    if shuttle.mode == SupplyMode::Call
        && let (Some(dock), Some(left)) = (shuttle.destination(), shuttle.timer.left(now))
    {
        let port = shuttle.port;
        crate::shuttle::check_ripples(world, port, dock, left);
    }
    let Some(shuttle) = world.get_resource::<SupplyShuttle>() else {
        return;
    };
    if shuttle.mode == SupplyMode::Idle || shuttle.timer.left(now).is_none_or(|l| l > 0.0) {
        return;
    }
    let (port, transit, destination, mode) = (
        shuttle.port,
        shuttle.transit,
        shuttle.destination(),
        shuttle.mode,
    );
    let call = world.resource::<SupplyConfig>().call_secs;
    match mode {
        SupplyMode::Igniting => {
            // `initiate_docking` override: buy and load the mail when leaving CentCom.
            if world.resource::<SupplyShuttle>().at_centcom() {
                buy(world);
                let mut turfs = empty_turfs(world, port);
                super::mail::create_mail(world, &mut turfs);
            }
            let result = initiate_docking(world, port, transit);
            log_move("enter transit", &result);
            let mut shuttle = world.resource_mut::<SupplyShuttle>();
            if result.is_err() {
                shuttle.timer.set(now, 2.0);
                return;
            }
            shuttle.location = SupplyLocation::Transit;
            shuttle.mode = SupplyMode::Call;
            shuttle.timer.set(now, call);
        }
        SupplyMode::Call => {
            let Some(destination) = destination else {
                world.resource_mut::<SupplyShuttle>().mode = SupplyMode::Idle;
                return;
            };
            // A loaned shuttle is called straight from CentCom (`initiate_docking` override).
            if world.resource::<SupplyShuttle>().at_centcom() {
                buy(world);
                let mut turfs = empty_turfs(world, port);
                super::mail::create_mail(world, &mut turfs);
            }
            let result = initiate_docking(world, port, destination);
            log_move("dock", &result);
            let mut shuttle = world.resource_mut::<SupplyShuttle>();
            if result.is_err() {
                // tg `setTimer(20)` after a failed docking.
                shuttle.timer.set(now, 2.0);
                return;
            }
            shuttle.mode = SupplyMode::Idle;
            shuttle.timer.clear();
            shuttle.location = if shuttle.to_station {
                SupplyLocation::Station
            } else {
                SupplyLocation::CentCom
            };
            // Sell when we get home.
            if shuttle.at_centcom() {
                sell(world);
            }
        }
        SupplyMode::Idle | SupplyMode::Stranded => {}
    }
}

/// tg `SSshuttle.checkTradeBlockade`: an igniting supply shuttle is stranded while a blockade
/// is registered and docked again once none is.
pub fn check_trade_blockade(world: &mut World) {
    crate::shuttle::prune_blocks(world);
    let blocked = crate::shuttle::supply_blocked(world);
    let Some(mut shuttle) = world.get_resource_mut::<SupplyShuttle>() else {
        return;
    };
    shuttle.blocked = blocked;
    if blocked && shuttle.mode == SupplyMode::Igniting {
        shuttle.mode = SupplyMode::Stranded;
        shuttle.timer.clear();
    }
    // tg sets `SHUTTLE_DOCKED`, which no request leaves; docked and idle are one mode here.
    if !blocked && shuttle.mode == SupplyMode::Stranded {
        shuttle.mode = SupplyMode::Idle;
    }
}

fn log_move(what: &str, result: &Result<crate::shuttle::MoveReport, crate::shuttle::DockingError>) {
    match result {
        Ok(r) => info!(
            "supply shuttle: {what}: {} turfs, {} movables in {:.2?}",
            r.turfs, r.movables, r.elapsed
        ),
        Err(e) => warn!("supply shuttle could not {what}: {e:?}"),
    }
}

/// Free floor tiles of the shuttle (tg `is_blocked_turf`: nothing dense on it).
pub(crate) fn empty_turfs(world: &World, port: Entity) -> Vec<TilePos> {
    shuttle_tiles(world, port)
        .into_iter()
        .filter(|&p| {
            let things = on_tile(world, p);
            let floor = things.iter().any(|&e| {
                world.get::<Turf>(e).is_some()
                    && path_of(world, e).is_some_and(|t| is_under(&t, FLOOR))
            });
            floor && !things.iter().any(|&e| world.get::<Density>(e).is_some())
        })
        .collect()
}

/// tg `buy()`: the shopping list into crates on the shuttle, paid at once.
pub fn buy(world: &mut World) {
    let mut turfs = {
        let shuttle = world.resource::<SupplyShuttle>();
        empty_turfs(world, shuttle.port)
    };
    let orders = std::mem::take(&mut world.resource_mut::<SupplyOrders>().shopping_list);
    let mut kept = Vec::new();
    let (mut value, mut purchases) = (0i64, 0u32);
    // `goodies_by_buyer`, each with the tile its box will take.
    let mut goodies: Vec<(u32, TilePos, Vec<SupplyOrder>)> = Vec::new();
    for order in orders {
        if turfs.is_empty() {
            kept.push(order);
            continue;
        }
        // The galactic materials market's order takes its sheets off the market.
        if !super::materials_market::adjust_market(world, &order) {
            continue;
        }
        let (mut price, pack_cost, goody) = {
            let packs = world.resource::<SupplyPacks>();
            let pack = &packs.packs[order.pack];
            (
                order.final_cost(packs),
                pack.cost.round() as i64,
                pack.has(order_flags::GOODY),
            )
        };
        // Goodies are private purchases only (the console refuses others).
        let goody_buyer = order.paying_account.filter(|_| goody);
        if goody && goody_buyer.is_none() {
            warn!(
                "cargo: goody order #{} without a paying account dropped",
                order.id
            );
            continue;
        }
        let buyer_orders = goody_buyer
            .and_then(|b| goodies.iter().find(|(a, ..)| *a == b))
            .map_or(0, |(.., o)| o.len());
        if let Some(buyer) = goody_buyer
            && buyer_orders == GOODY_FREE_SHIPPING_MAX
        {
            price += CRATE_TAX;
            let text = format!(
                "Goody order size exceeds free shipping limit: Assessing {CRATE_TAX}¢ S&H fee."
            );
            super::economy::bank_card_talk(world, buyer, &text);
        }
        let paid = {
            let mut economy = world.resource_mut::<Economy>();
            let payer = match (&order.paying_department, order.paying_account) {
                (Some(department), _) => economy.department_mut(department),
                (None, Some(id)) => economy.account_mut(id),
                (None, None) => economy.department_mut(ACCOUNT_CAR),
            };
            payer.is_some_and(|p| p.adjust_money(-price))
        };
        if !paid {
            if let Some(id) = order.paying_account {
                let text = format!(
                    "Cargo order #{} rejected due to lack of funds. ¢ required: {price}",
                    order.id
                );
                super::economy::bank_card_talk(world, id, &text);
            }
            kept.push(order);
            continue;
        }
        // `supply_order/Destroy`: `QDEL_NULL(applied_coupon)`.
        if let Some((coupon, _)) = order.coupon
            && world.get_entity(coupon).is_ok()
        {
            world.despawn(coupon);
        }
        if let Some(id) = order.paying_account {
            let text = format!(
                "Cargo order #{} has shipped. {price}¢ have been charged to your bank account",
                order.id
            );
            super::economy::bank_card_talk(world, id, &text);
            // Cargo gets the handling fee.
            if let Some(cargo) = world.resource_mut::<Economy>().department_mut(ACCOUNT_CAR) {
                cargo.adjust_money(price - pack_cost);
            }
        }
        value += pack_cost;
        let i = {
            let mut shuttle = world.resource_mut::<SupplyShuttle>();
            (shuttle.rng.next_u64() % turfs.len() as u64) as usize
        };
        purchases += 1;
        info!(
            "cargo: order #{} ({}) by {} has shipped",
            order.id,
            world.resource::<SupplyPacks>().packs[order.pack].name,
            order.orderer
        );
        if let Some(buyer) = goody_buyer {
            match goodies.iter().position(|(a, ..)| *a == buyer) {
                Some(at) => goodies[at].2.push(order),
                None => goodies.push((buyer, turfs.swap_remove(i), vec![order])),
            }
            continue;
        }
        let pos = turfs.swap_remove(i);
        if let Some(crate_entity) = generate(world, &order, pos) {
            lock_to_payer(world, crate_entity, &order);
        }
    }
    for (buyer, pos, orders) in goodies {
        pack_goodies(world, buyer, pos, &orders);
    }
    world.resource_mut::<SupplyOrders>().shopping_list = kept;
    // The mining console's paid orders (`/datum/supply_order/disposable`).
    crate::mining::console::ship_orders(world, &mut turfs);
    let left = world
        .resource::<Economy>()
        .department(ACCOUNT_CAR)
        .map_or(0, |a| a.balance);
    info!("cargo: {purchases} orders in this shipment, worth {value}¢. {left}¢ left.");
}

/// tg `supply_order/generate` -> `supply_pack/generate` + `generateManifest`: the crate on
/// `pos`, filled, with its manifest. `None` without the crate's prototype.
pub fn generate(world: &mut World, order: &SupplyOrder, pos: TilePos) -> Option<Entity> {
    let pack_name = world.resource::<SupplyPacks>().packs[order.pack]
        .name
        .clone();
    let (crate_entity, contents) = generate_pack(world, order.pack, pos)?;
    // `supply_pack/generate`: " - Purchased by <holder>" for a paying account.
    let purchased = payer_holder(world, order)
        .map(|holder| format!(" - Purchased by {holder}"))
        .unwrap_or_default();
    if let Some(mut n) = world.get_mut::<DmName>(crate_entity) {
        n.name = format!("{}{purchased} - #{}", n.name, order.id);
    }
    if let Some(paper) = spawn_template(world, MANIFEST_PAPER, None) {
        let station = crate::round::station_name(world);
        let mut text = format!(
            "Центральное Командование Shipping Manifest\nOrder: {}\nDestination: {station}\n\
             Item: {}\nContents:",
            order.id, pack_name
        );
        for (name, n) in count_names(contents) {
            text.push_str(&format!("\n- {n} {name}"));
        }
        text.push_str("\nStamp below to confirm receipt of goods:");
        // shortcut: no paper text yet; the manifest's text is its description.
        world.entity_mut(paper).insert((
            Networked,
            StoredIn(crate_entity),
            DmName {
                name: format!("shipping manifest - #{} ({})", order.id, pack_name),
                desc: text,
            },
        ));
    }
    Some(crate_entity)
}

/// tg `supply_pack/generate` without an order: the pack's crate on `pos` (named after
/// `crate_name`), filled, and the names of what it holds. `None` without the crate's prototype.
pub fn generate_pack(
    world: &mut World,
    pack: usize,
    pos: TilePos,
) -> Option<(Entity, Vec<String>)> {
    let pack = world.resource::<SupplyPacks>().packs[pack].clone();
    let crate_type = pack.crate_type.as_deref()?;
    let crate_entity = spawn_template(world, crate_type, Some(pos))?;
    world.entity_mut(crate_entity).insert(Networked);
    let base = world
        .get::<DmName>(crate_entity)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let name = if pack.crate_name.is_empty() {
        base
    } else {
        pack.crate_name.clone()
    };
    let desc = world
        .get::<DmName>(crate_entity)
        .map(|n| n.desc.clone())
        .unwrap_or_default();
    world.entity_mut(crate_entity).insert(DmName { name, desc });
    let closet = {
        let icons = world.get_resource::<IconIndex>();
        let entity = world.entity(crate_entity);
        match (entity.get::<DmCloset>(), entity.get::<DmAtom>()) {
            (Some(dm), Some(atom)) => Some(Closet::from_dm(dm, atom, icons)),
            _ => None,
        }
    };
    if let Some(mut closet) = closet {
        closet.opened = false;
        if !pack.access.is_empty() {
            closet.req_access = vec![pack.access.clone()];
        }
        if !pack.access_any.is_empty() {
            closet.req_one_access = pack.access_any.clone();
        }
        world.entity_mut(crate_entity).insert(closet);
        crate::closets::refresh_look(world, crate_entity);
    }
    let mut contents: Vec<String> = Vec::new();
    // A materials market order holds full stacks of its sheets (`custom/minerals/fill`).
    let sheets = pack.id == super::materials_market::MATERIALS_ORDER;
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let mut rng = Rng::new(seed ^ crate_entity.to_bits());
    for (path, amount) in &pack.contains {
        let stacks: Vec<(String, Option<u32>)> = if sheets {
            super::materials_market::stack_sizes(*amount)
                .into_iter()
                .map(|n| (path.clone(), Some(n)))
                .collect()
        } else {
            (0..*amount)
                .flat_map(|_| loot_of(world.resource::<SupplyPacks>(), path, &mut rng))
                .map(|leaf| (leaf, None))
                .collect()
        };
        for (path, stack) in &stacks {
            let Some(item) = spawn_template(world, path, None) else {
                continue;
            };
            world
                .entity_mut(item)
                .insert((Networked, StoredIn(crate_entity)));
            crate::storage::finish_item(world, item, 1);
            if let Some(amount) = *stack {
                world
                    .entity_mut(item)
                    .insert(crate::tools::Stack { amount });
            }
            let name = world
                .get::<DmName>(item)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| path.clone());
            contents.push(name);
        }
    }
    Some((crate_entity, contents))
}

/// The holder of the account paying `order` (a person or a department), if not cargo's budget.
fn payer_holder(world: &World, order: &SupplyOrder) -> Option<String> {
    let economy = world.resource::<Economy>();
    match (&order.paying_department, order.paying_account) {
        (Some(department), _) => economy.department(department),
        (None, Some(id)) => economy.account(id),
        (None, None) => None,
    }
    .map(|a| a.holder.clone())
}

/// `locked_to_account` of a private or department order's crate.
fn lock_to_payer(world: &mut World, crate_entity: Entity, order: &SupplyOrder) {
    let lock = match (&order.paying_department, order.paying_account) {
        (Some(department), _) => AccountLock::Department(department.clone()),
        (None, Some(id)) => AccountLock::Personal(id),
        (None, None) => return,
    };
    if world.get::<Closet>(crate_entity).is_some() {
        world.entity_mut(crate_entity).insert(lock);
    }
}

/// The goodies of one buyer (tg `buy()`'s `miscboxes` + `generateCombo`): a goody case for up
/// to `GOODY_FREE_SHIPPING_MAX` orders, else an account-locked secure crate, with every
/// order's contents and one manifest of all of them.
fn pack_goodies(world: &mut World, buyer: u32, pos: TilePos, orders: &[SupplyOrder]) {
    let holder = world
        .resource::<Economy>()
        .account(buyer)
        .map(|a| a.holder.clone())
        .unwrap_or_default();
    let (path, name) = if orders.len() > GOODY_FREE_SHIPPING_MAX {
        (
            ss13_mapload::cargo::GOODY_CRATE,
            format!("goody crate - purchased by {holder}"),
        )
    } else {
        (
            ss13_mapload::cargo::GOODY_CASE,
            format!("goody case - purchased by {holder}"),
        )
    };
    let Some(container) = spawn_template(world, path, Some(pos)) else {
        warn!("cargo: no prototype of {path}; {holder}'s goodies are lost");
        return;
    };
    world.entity_mut(container).insert(Networked);
    let desc = world
        .get::<DmName>(container)
        .map(|n| n.desc.clone())
        .unwrap_or_default();
    world.entity_mut(container).insert(DmName { name, desc });
    if orders.len() > GOODY_FREE_SHIPPING_MAX {
        let closet = {
            let icons = world.get_resource::<IconIndex>();
            let entity = world.entity(container);
            match (entity.get::<DmCloset>(), entity.get::<DmAtom>()) {
                (Some(dm), Some(atom)) => Some(Closet::from_dm(dm, atom, icons)),
                _ => None,
            }
        };
        if let Some(mut closet) = closet {
            closet.opened = false;
            world.entity_mut(container).insert(closet);
            crate::closets::refresh_look(world, container);
        }
        world
            .entity_mut(container)
            .insert(AccountLock::Personal(buyer));
    } else {
        // A new storage item: its limits and lock, then the buyer's account on the lock.
        crate::storage::finish_item(world, container, 0);
        if let Some(mut lock) = world.get_mut::<crate::lockbox::Lockbox>(container) {
            lock.buyer_account = Some(buyer);
        }
    }
    let mut ids = String::new();
    let mut cost = 0.0;
    let mut contents: Vec<String> = Vec::new();
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let mut rng = Rng::new(seed ^ container.to_bits());
    for order in orders {
        let pack = world.resource::<SupplyPacks>().packs[order.pack].clone();
        ids.push_str(&format!("#{} ", order.id));
        cost += pack.cost;
        let leaves: Vec<String> = pack
            .contains
            .iter()
            .flat_map(|(path, amount)| (0..*amount).map(move |_| path))
            .flat_map(|path| loot_of(world.resource::<SupplyPacks>(), path, &mut rng))
            .collect();
        for path in &leaves {
            let Some(item) = spawn_template(world, path, None) else {
                continue;
            };
            world
                .entity_mut(item)
                .insert((Networked, StoredIn(container)));
            crate::storage::finish_item(world, item, 1);
            contents.push(
                world
                    .get::<DmName>(item)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| path.clone()),
            );
        }
    }
    if let Some(paper) = spawn_template(world, MANIFEST_PAPER, None) {
        let station = crate::round::station_name(world);
        let mut text = format!(
            "Центральное Командование Shipping Manifest\nOrder: {ids}\nDestination: {station}\n\
             Requested by: {holder}\nCost: {}\nContents:",
            cost.round() as i64
        );
        for (name, n) in count_names(contents) {
            text.push_str(&format!("\n- {n} {name}"));
        }
        text.push_str("\nStamp below to confirm receipt of goods:");
        world.entity_mut(paper).insert((
            Networked,
            StoredIn(container),
            DmName {
                name: format!("shipping manifest - {ids}"),
                desc: text,
            },
        ));
    }
}

/// `"<n> <name>"` lines of a manifest: each name once, in first-seen order.
fn count_names(names: Vec<String>) -> Vec<(String, u32)> {
    let mut counted: Vec<(String, u32)> = Vec::new();
    for name in names {
        match counted.iter_mut().find(|(n, _)| *n == name) {
            Some((_, n)) => *n += 1,
            None => counted.push((name, 1)),
        }
    }
    counted
}

pub(crate) fn spawn_template(
    world: &mut World,
    path: &str,
    pos: Option<TilePos>,
) -> Option<Entity> {
    world
        .try_resource_scope(|world, packs: Mut<SupplyPacks>| {
            Some(packs.templates.get(path)?.spawn(world, pos))
        })
        .flatten()
}

/// Nested spawners deeper than this give nothing.
const MAX_LOOT_DEPTH: u32 = 8;

/// What `path` makes: itself, or a spawner's loot rolled as at map load (`spawn_loot`); a
/// spawner put in a crate or a letter drops its loot there.
pub(crate) fn loot_of(packs: &SupplyPacks, path: &str, rng: &mut Rng) -> Vec<String> {
    let mut out = Vec::new();
    loot(packs, path, rng, 0, &mut out);
    out
}

fn loot(packs: &SupplyPacks, path: &str, rng: &mut Rng, depth: u32, out: &mut Vec<String>) {
    let spawner = packs
        .templates
        .get(path)
        .and_then(|t| t.proto.get::<DmSpawner>())
        .cloned();
    let Some(s) = spawner else {
        out.push(path.to_owned());
        return;
    };
    if depth > MAX_LOOT_DEPTH || !s.unresolved.is_empty() {
        return;
    }
    if !s.random {
        for e in s
            .loot
            .iter()
            .filter(|e| !e.path.is_empty() && e.parent.is_none())
        {
            loot(packs, &e.path, rng, depth + 1, out);
        }
        return;
    }
    if !rng.prob(s.chance) {
        return;
    }
    let count = if s.all {
        s.loot.len()
    } else {
        s.count as usize
    };
    let mut pool: Vec<usize> = (0..s.loot.len())
        .filter(|&i| s.loot[i].parent.is_none())
        .collect();
    for _ in 0..count {
        if pool.is_empty() {
            break;
        }
        let Some((top, leaf)) = pick(&s.loot, &pool, rng) else {
            continue;
        };
        if !s.double {
            pool.remove(top);
        }
        loot(packs, &s.loot[leaf].path, rng, depth + 1, out);
    }
}

/// Unanchored things a sale takes: items and structures, not mobs, turfs or ports.
fn sellable(world: &World, e: Entity) -> bool {
    let entity = world.entity(e);
    if let Some(item) = entity.get::<DmItem>() {
        return !item.anchored && !entity.contains::<crate::inventory::CarriedBy>();
    }
    let Some(obj) = entity.get::<DmObj>() else {
        return false;
    };
    !entity.get::<Anchored>().map_or(obj.anchored, |a| a.0)
}

/// tg `sell()`: everything unanchored aboard is exported, the money goes to the cargo budget.
pub fn sell(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let port = world.resource::<SupplyShuttle>().port;
    let mut things = Vec::new();
    for pos in shuttle_tiles(world, port) {
        things.extend(
            on_tile(world, pos)
                .into_iter()
                .filter(|&e| sellable(world, e)),
        );
    }
    let mut report = ExportReport::default();
    let Some(mut exports) = world.remove_resource::<Exports>() else {
        return;
    };
    for thing in things {
        export_item_and_contents(world, &mut exports, thing, now, &mut report);
    }
    let lines = exports.printout(&report);
    world.insert_resource(exports);
    let value = report.total();
    if let Some(cargo) = world.resource_mut::<Economy>().department_mut(ACCOUNT_CAR) {
        cargo.adjust_money(value);
    }
    let msg: String = lines.iter().map(|l| format!("{l}\n")).collect();
    world.resource_mut::<SupplyOrders>().centcom_message = msg;
    if !report.exported_atoms.is_empty() {
        info!(
            "cargo: contents sold for {value}¢: {}",
            report.exported_atoms.join(",")
        );
    }
    world.insert_resource(LastSale { report, value });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(cost: f64, flags: u32) -> SupplyPackDef {
        SupplyPackDef {
            id: "/datum/supply_pack/test".into(),
            name: "Test".into(),
            cost,
            order_flags: flags,
            crate_type: Some("/obj/structure/closet/crate".into()),
            contains: vec![("/obj/item/wrench".into(), 1)],
            ..default()
        }
    }

    fn order(paying_account: Option<u32>) -> SupplyOrder {
        SupplyOrder {
            id: 1,
            pack: 0,
            orderer: String::new(),
            orderer_rank: String::new(),
            reason: String::new(),
            paying_account,
            paying_department: None,
            can_be_cancelled: true,
            coupon: None,
        }
    }

    #[test]
    fn private_orders_cost_more() {
        let packs = SupplyPacks::new(vec![pack(1000.0, 0)]);
        assert_eq!(order(None).final_cost(&packs), 1000);
        assert_eq!(order(Some(123_456)).final_cost(&packs), 1100);
        let goody = SupplyPacks::new(vec![pack(105.0, order_flags::GOODY)]);
        assert_eq!(order(Some(1)).final_cost(&goody), 105, "goodies: no 1.1x");
        let odd = SupplyPacks::new(vec![pack(285.0, 0)]);
        assert_eq!(order(Some(1)).final_cost(&odd), 314, "round(313.5)");
    }

    #[test]
    fn visibility_flags() {
        use order_flags::*;
        assert!(orderable(&pack(1.0, 0), false, false));
        assert!(!orderable(&pack(1.0, INVISIBLE), false, false));
        assert!(!orderable(&pack(1.0, EMAG_ONLY), false, false));
        assert!(
            !orderable(&pack(1.0, EMAG_ONLY), true, false),
            "contraband is not emag"
        );
        assert!(orderable(&pack(1.0, EMAG_ONLY), true, true));
        assert!(!orderable(&pack(1.0, INVISIBLE | EMAG_ONLY), true, true));
        assert!(!orderable(&pack(1.0, CONTRABAND), false, false));
        assert!(orderable(&pack(1.0, CONTRABAND), true, false));
        assert!(!orderable(&pack(1.0, SPECIAL), false, false));
        assert!(orderable(
            &pack(1.0, SPECIAL | SPECIAL_ENABLED),
            false,
            false
        ));
        assert!(!orderable(&pack(1.0, POD_ONLY), false, false));
        assert!(orderable(&pack(1.0, GOODY), false, false));
        let crateless = |flags| SupplyPackDef {
            crate_type: None,
            ..pack(1.0, flags)
        };
        assert!(orderable(&crateless(GOODY), false, false), "a goody case");
        assert!(!orderable(&crateless(0), false, false));
        assert!(!orderable(
            &SupplyPackDef {
                custom_fill: true,
                ..pack(1.0, 0)
            },
            false,
            false
        ));
    }

    #[test]
    fn manifest_lines_count_names() {
        let names = ["a", "b", "a"].map(str::to_owned).to_vec();
        assert_eq!(
            count_names(names),
            [("a".to_owned(), 2), ("b".to_owned(), 1)]
        );
    }

    #[test]
    fn order_limits_and_ids() {
        let mut orders = SupplyOrders::new(3);
        assert!((1..=9000).contains(&orders.order_number));
        let first = orders.next_id();
        assert_eq!(orders.next_id(), first + 1);
        for _ in 0..49 {
            orders.shopping_list.push(order(None));
        }
        assert_eq!(orders.order_count(0), Some(49));
        orders.request_list.push(order(None));
        assert_eq!(orders.order_count(0), None, "50 in both lists");
        assert_eq!(orders.order_count(1), Some(0));
    }
}
