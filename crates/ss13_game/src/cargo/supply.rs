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
// shortcut: goodies (lockboxes per buyer), departmental orders, coupons, chef groceries, mail,
// the materials market, shuttle loans, requisition papers, manifest errors and
// `locked_to_account` crates are not ported; private orders are paid on delivery as in tg but
// their crates open with the pack's access only. Packs with their own `fill()` are not offered.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use ss13_mapload::cargo::{CargoData as PackData, MANIFEST_PAPER, SupplyPackDef, order_flags};
use ss13_mapload::dock::{map_shuttle, shuttle_template_path};
use ss13_mapload::spawner::Rng;
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
use crate::closets::Closet;
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
/// not invisible, emag-only, a disabled special, contraband (no hacked boards yet) or pod-only.
/// Packs needing what is not ported (goodies, their own `fill()`, live animals, nothing to spawn)
/// are hidden too.
pub fn orderable(pack: &SupplyPackDef, contraband: bool) -> bool {
    use order_flags::*;
    !(pack.has(INVISIBLE)
        || pack.has(EMAG_ONLY)
        || (pack.has(CONTRABAND) && !contraband)
        || pack.has(POD_ONLY)
        || (pack.has(SPECIAL) && !pack.has(SPECIAL_ENABLED))
        || pack.has(GOODY)
        || pack.custom_fill
        || pack.crate_type.is_none()
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
    pub can_be_cancelled: bool,
}

impl SupplyOrder {
    /// tg `get_final_cost`: private purchases of non-goodies cost 1.1x.
    pub fn final_cost(&self, packs: &SupplyPacks) -> i64 {
        let pack = &packs.packs[self.pack];
        let mut cost = pack.cost;
        if self.paying_account.is_some() && !pack.has(order_flags::GOODY) {
            cost *= 1.1;
        }
        cost.round() as i64
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
    rng: Rng,
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
    let mut packs = SupplyPacks::new(data.packs);
    let mut paths: Vec<String> = vec![MANIFEST_PAPER.to_owned()];
    for pack in &packs.packs {
        paths.extend(pack.contains.iter().map(|(t, _)| t.clone()));
        paths.extend(pack.crate_type.clone());
    }
    if let Some(protos) = world.get_resource::<Prototypes>() {
        let index: HashMap<(&str, &str), ss13_shared::IconStateId> = world
            .get_resource::<IconIndex>()
            .map(|i| {
                i.0.iter()
                    .map(|((dmi, state), id)| ((dmi.as_str(), state.as_str()), *id))
                    .collect()
            })
            .unwrap_or_default();
        for path in paths {
            if packs.templates.contains_key(&path) {
                continue;
            }
            if let Some(proto) = protos.get(&path).cloned() {
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

/// tg `check_blacklist` for what exists here: no living mob may ride the supply shuttle.
// shortcut: of tg's blacklist only `/mob/living` can be on a shuttle here.
pub fn can_move(world: &World, shuttle: &SupplyShuttle) -> bool {
    if !shuttle.at_station() {
        return true;
    }
    !shuttle_tiles(world, shuttle.port).into_iter().any(|p| {
        on_tile(world, p)
            .into_iter()
            .any(|e| world.get::<crate::health::Health>(e).is_some())
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
            // `initiate_docking` override: buy when leaving CentCom.
            if world.resource::<SupplyShuttle>().at_centcom() {
                buy(world);
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
        SupplyMode::Idle => {}
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
fn empty_turfs(world: &World, port: Entity) -> Vec<TilePos> {
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
    for order in orders {
        if turfs.is_empty() {
            kept.push(order);
            continue;
        }
        let (price, pack_cost) = {
            let packs = world.resource::<SupplyPacks>();
            (
                order.final_cost(packs),
                packs.packs[order.pack].cost.round() as i64,
            )
        };
        let paid = {
            let mut economy = world.resource_mut::<Economy>();
            let payer = match order.paying_account {
                Some(id) => economy.account_mut(id),
                None => economy.department_mut(ACCOUNT_CAR),
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
        let pos = turfs.swap_remove(i);
        generate(world, &order, pos);
        purchases += 1;
        info!(
            "cargo: order #{} ({}) by {} has shipped",
            order.id,
            world.resource::<SupplyPacks>().packs[order.pack].name,
            order.orderer
        );
    }
    world.resource_mut::<SupplyOrders>().shopping_list = kept;
    let left = world
        .resource::<Economy>()
        .department(ACCOUNT_CAR)
        .map_or(0, |a| a.balance);
    info!("cargo: {purchases} orders in this shipment, worth {value}¢. {left}¢ left.");
}

/// tg `supply_order/generate` -> `supply_pack/generate` + `generateManifest`: the crate on
/// `pos`, filled, with its manifest. `None` without the crate's prototype.
pub fn generate(world: &mut World, order: &SupplyOrder, pos: TilePos) -> Option<Entity> {
    let pack = world.resource::<SupplyPacks>().packs[order.pack].clone();
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
    world.entity_mut(crate_entity).insert(DmName {
        name: format!("{name} - #{}", order.id),
        desc,
    });
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
    for (path, amount) in &pack.contains {
        for _ in 0..*amount {
            let Some(item) = spawn_template(world, path, None) else {
                continue;
            };
            world
                .entity_mut(item)
                .insert((Networked, StoredIn(crate_entity)));
            crate::storage::finish_item(world, item, 1);
            let name = world
                .get::<DmName>(item)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| path.clone());
            contents.push(name);
        }
    }
    if let Some(paper) = spawn_template(world, MANIFEST_PAPER, None) {
        let station = crate::round::station_name(world);
        let mut text = format!(
            "Центральное Командование Shipping Manifest\nOrder: {}\nDestination: {station}\n\
             Item: {}\nContents:",
            order.id, pack.name
        );
        let mut counted: Vec<(String, u32)> = Vec::new();
        for name in contents {
            match counted.iter_mut().find(|(n, _)| *n == name) {
                Some((_, n)) => *n += 1,
                None => counted.push((name, 1)),
            }
        }
        for (name, n) in counted {
            text.push_str(&format!("\n- {n} {name}"));
        }
        text.push_str("\nStamp below to confirm receipt of goods:");
        // shortcut: no paper text yet; the manifest's text is its description.
        world.entity_mut(paper).insert((
            Networked,
            StoredIn(crate_entity),
            DmName {
                name: format!("shipping manifest - #{} ({})", order.id, pack.name),
                desc: text,
            },
        ));
    }
    Some(crate_entity)
}

fn spawn_template(world: &mut World, path: &str, pos: Option<TilePos>) -> Option<Entity> {
    world
        .try_resource_scope(|world, packs: Mut<SupplyPacks>| {
            Some(packs.templates.get(path)?.spawn(world, pos))
        })
        .flatten()
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
            can_be_cancelled: true,
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
        assert!(orderable(&pack(1.0, 0), false));
        assert!(!orderable(&pack(1.0, INVISIBLE), false));
        assert!(!orderable(&pack(1.0, EMAG_ONLY), false));
        assert!(!orderable(&pack(1.0, CONTRABAND), false));
        assert!(orderable(&pack(1.0, CONTRABAND), true));
        assert!(!orderable(&pack(1.0, SPECIAL), false));
        assert!(orderable(&pack(1.0, SPECIAL | SPECIAL_ENABLED), false));
        assert!(!orderable(&pack(1.0, POD_ONLY), false));
        assert!(!orderable(&pack(1.0, GOODY), false));
        assert!(!orderable(
            &SupplyPackDef {
                custom_fill: true,
                ..pack(1.0, 0)
            },
            false
        ));
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
