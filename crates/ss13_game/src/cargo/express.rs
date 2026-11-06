//! The express supply console and the supply pod beacon (server), after tg
//! `/obj/machinery/computer/cargo/express` (`code/modules/cargo/expressconsole.dm`) and
//! `/obj/item/supplypod_beacon` (`supplypod_beacon.dm`), with tgui's `CargoExpress`.
//!
//! - The console starts locked; an ID with cargo access swiped on it locks or unlocks it.
//! - `add`: a pack (pod-only packs included, goodies refused) is paid from the cargo budget at
//!   once and lands in a supply pod ([`super::supplypod`]) on a free floor of the cargo bay
//!   (`/area/station/cargo/storage`, else the console's own area) or on the linked beacon's
//!   tile in beacon mode. The railgun then recalibrates for 5 s.
//! - Emagged (`crate::emag`): 0.72x prices, contraband and emag-only packs, and each order is up
//!   to `MAX_EMAG_ROCKETS` (5) pods on random free floors of a random station area, each paid.
//! - `printBeacon` (500 from the cargo budget, every 10 s) prints a beacon linked to the console;
//!   a beacon used on a console links to it (unlinking the old pair). A beacon is usable on a
//!   tile or in someone's hands, not inside a bag.
// shortcut: no bluespace pod disk, requisition papers, beacon overlays and sounds, alt-click
// unlinking, silicon access or `special_pod` packs; goodies are refused even when emagged.

use std::sync::Arc;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::cargo::{SUPPLYPOD_BEACON, order_flags};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmName, DmObj, MapSeed};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use super::console::machine_say;
use super::economy::{ACCOUNT_CAR, Economy};
use super::supply::{SupplyOrder, SupplyOrders, SupplyPacks};
use super::supplypod::{PodCargo, launch};
use super::*;
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::tools::tell;

pub const EXPRESS_CONSOLE: &str = ss13_mapload::cargo::EXPRESS_CONSOLE;
/// `EXPRESS_EMAG_DISCOUNT`.
pub const EMAG_DISCOUNT: f64 = 0.72;
/// `BEACON_COST`, `BEACON_PRINT_COOLDOWN`.
pub const BEACON_COST: i64 = 500;
const BEACON_PRINT_SECS: f64 = 10.0;
/// `MAX_EMAG_ROCKETS`.
const MAX_EMAG_ROCKETS: usize = 5;
/// `COOLDOWN_EXPRESSPOD_CONSOLE`: after an order, after an emagged one.
const ORDER_COOLDOWN_SECS: f64 = 5.0;
const EMAG_COOLDOWN_SECS: f64 = 10.0;
const LANDING_ZONE: &str = "/area/station/cargo/storage";
const FLOOR: &str = "/turf/open/floor";
const ACCESS_CARGO: &str = "cargo";
const BLOCKADE_WARNING: &str = "Bluespace instability detected. Delivery impossible.";
const EMAG_MESSAGE: &str =
    "(&!#@ERROR: R0UTING_#PRO7O&OL MALF(*CT#ON. $UG%ESTE@ ACT#0N: !^/PULS3-%E)ET CIR*)ITB%ARD.";

/// An express console's state, with the beacon's status refreshed every tick for `ui_data`.
#[derive(Component, Clone, Debug)]
pub struct ExpressConsole {
    pub locked: bool,
    pub beacon: Option<Entity>,
    pub using_beacon: bool,
    pub emagged: bool,
    next_order: f64,
    next_print: f64,
    catalogue: Arc<CargoStatic>,
    /// The linked beacon is on a tile or in hands (`isturf(beacon.loc) || ismob(beacon.loc)`).
    beacon_valid: bool,
    beacon_zone: String,
    beacon_name: String,
    /// The cargo budget, for `points` and `canBuyBeacon`.
    points: i64,
    print_left: u32,
}

/// A supply pod beacon's console (`express_console`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PodBeacon {
    pub console: Option<Entity>,
}

/// `printed_beacons` and the emagged landing rolls.
#[derive(Resource)]
struct ExpressState {
    printed: u32,
    rng: Rng,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpressAct {
    LzCargo,
    LzBeacon,
    PrintBeacon,
    Add(String),
}

pub struct ExpressPlugin;

impl Plugin for ExpressPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 3459, |w, c| {
            crate::cargo::express::interact(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2980, |w, c| {
            c.held.is_some_and(|h| {
                crate::cargo::express::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.register_ui::<CargoExpressUi>()
            .add_systems(Startup, setup_mapped.after(super::supply::load_packs))
            .add_systems(
                Update,
                (
                    refresh.run_if(any_with_component::<ExpressConsole>),
                    acts.in_set(UiSystems::Handle),
                ),
            );
    }
}

fn setup_mapped(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(ExpressState {
        printed: 0,
        rng: Rng::new(seed ^ 0x6578_7072),
    });
    let mut q = world.query::<(Entity, &DmObj)>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, EXPRESS_CONSOLE))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// The express catalogue: what the console may order (pod-only packs too), at its discount.
pub fn catalogue(packs: &SupplyPacks, emagged: bool) -> CargoStatic {
    let mut st = super::console::catalogue_with(packs, emagged, emagged, true);
    if emagged {
        for pack in st.groups.iter_mut().flat_map(|g| g.packs.iter_mut()) {
            pack.cost = (f64::from(pack.cost) * EMAG_DISCOUNT).round() as u32;
        }
    }
    st
}

/// Map load and a console built from its board (`crate::cargo::console::setup_console`).
pub fn setup(world: &mut World, e: Entity) {
    let catalogue = Arc::new(
        world
            .get_resource::<SupplyPacks>()
            .map(|p| catalogue(p, false))
            .unwrap_or_default(),
    );
    world.entity_mut(e).insert(ExpressConsole {
        locked: true,
        beacon: None,
        using_beacon: false,
        emagged: false,
        next_order: 0.0,
        next_print: 0.0,
        catalogue,
        beacon_valid: false,
        beacon_zone: String::new(),
        beacon_name: String::new(),
        points: 0,
        print_left: 0,
    });
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn plain(name: &str) -> &str {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| plain(&n.name).to_owned())
        .unwrap_or_default()
}

/// Where a beacon is usable: its tile, or the tile of whoever holds it.
pub fn beacon_tile(world: &World, beacon: Entity) -> Option<TilePos> {
    if let Some(&pos) = world.get::<TilePos>(beacon) {
        return Some(pos);
    }
    let holder = world.get::<CarriedBy>(beacon)?.0;
    world.get::<TilePos>(holder).copied()
}

/// The beacon and budget fields of `ui_data`, written only when they change.
fn refresh(world: &mut World) {
    let t = now(world);
    let points = world
        .get_resource::<Economy>()
        .and_then(|e| e.department(ACCOUNT_CAR))
        .map_or(0, |a| a.balance);
    let mut q = world.query::<(Entity, &ExpressConsole)>();
    let consoles: Vec<(Entity, Option<Entity>)> =
        q.iter(world).map(|(e, c)| (e, c.beacon)).collect();
    for (e, beacon) in consoles {
        let beacon = beacon.filter(|&b| world.get_entity(b).is_ok());
        let tile = beacon.and_then(|b| beacon_tile(world, b));
        let zone = tile
            .and_then(|p| world.get_resource::<ss13_mapload::Areas>()?.area_at(p))
            .map(|a| crate::shuttle::area_name(world, a))
            .unwrap_or_default();
        let name = beacon.map_or_else(|| "No Beacon Found".to_owned(), |b| name_of(world, b));
        let Some(console) = world.get::<ExpressConsole>(e) else {
            continue;
        };
        let print_left = (console.next_print - t).max(0.0).ceil() as u32;
        let changed = console.beacon != beacon
            || console.beacon_valid != tile.is_some()
            || console.beacon_zone != zone
            || console.beacon_name != name
            || console.points != points
            || console.print_left != print_left;
        if changed && let Some(mut c) = world.get_mut::<ExpressConsole>(e) {
            c.beacon = beacon;
            c.beacon_valid = tile.is_some();
            c.beacon_zone = zone;
            c.beacon_name = name;
            c.points = points;
            c.print_left = print_left;
        }
    }
}

/// `ui_data`.
pub fn express_data(c: &ExpressConsole, supply_blocked: bool) -> CargoExpressData {
    let can_beacon = !c.using_beacon || c.beacon_valid;
    let mut message = "Sales are near-instantaneous - please choose carefully.".to_owned();
    if supply_blocked {
        message = BLOCKADE_WARNING.to_owned();
    }
    if c.using_beacon && c.beacon.is_none() {
        message = "BEACON ERROR: BEACON MISSING".to_owned();
    } else if c.using_beacon && !c.beacon_valid {
        message = "BEACON ERROR: MUST BE EXPOSED".to_owned();
    }
    if c.emagged {
        message = EMAG_MESSAGE.to_owned();
    }
    let print_msg = if c.print_left == 0 {
        format!("Print Beacon for {BEACON_COST}{MONEY_SYMBOL}")
    } else {
        format!(
            "Print Beacon for {BEACON_COST}{MONEY_SYMBOL} ({}s)",
            c.print_left
        )
    };
    CargoExpressData {
        locked: c.locked,
        points: c.points,
        using_beacon: c.using_beacon,
        beaconzone: c.beacon_zone.clone(),
        beacon_name: c.beacon_name.clone(),
        beacon_error: c.using_beacon && !can_beacon,
        can_buy_beacon: c.print_left == 0 && c.points >= BEACON_COST,
        has_beacon: c.beacon.is_some(),
        can_beacon,
        print_msg,
        message,
    }
}

impl ServerUi for CargoExpressUi {
    type Source = &'static ExpressConsole;
    type UserParam = ();
    type Act = ExpressAct;

    fn static_data(c: &ExpressConsole) -> CargoStatic {
        (*c.catalogue).clone()
    }

    fn data(c: &ExpressConsole, _: Entity, _: &()) -> CargoExpressData {
        express_data(c, false)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ExpressAct, UiReject> {
        match action {
            LZ_CARGO => Ok(ExpressAct::LzCargo),
            LZ_BEACON => Ok(ExpressAct::LzBeacon),
            PRINT_BEACON => Ok(ExpressAct::PrintBeacon),
            ADD => params
                .get(ID)
                .map(|id| ExpressAct::Add(id.to_owned()))
                .ok_or_else(|| UiReject::BadParams("id: missing".into())),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// An empty hand opens the window (locked or not: tgui shows the lock screen).
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    if world.get::<ExpressConsole>(e).is_none() || world.get::<Unpowered>(e).is_some() {
        return false;
    }
    let title = name_of(world, e);
    world.write_message(OpenUi::new::<CargoExpressUi>(client, mob, e, title));
    true
}

/// tg `item_interaction`: an ID with access toggles the lock, a beacon links.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    let Some(console) = world.get::<ExpressConsole>(e).cloned() else {
        return false;
    };
    if let Some(card) = crate::inventory::get_id(world, held) {
        // `allowed(user)`: the user's ID (hands first), the swiped card among them.
        let access = world
            .get::<Inventory>(mob)
            .and_then(|i| i.id_card(world))
            .or(Some(card))
            .and_then(|c| world.get::<IdCard>(c))
            .is_some_and(|c| c.access.iter().any(|a| a == ACCESS_CARGO));
        if !access {
            return false;
        }
        let locked = !console.locked;
        if let Some(mut c) = world.get_mut::<ExpressConsole>(e) {
            c.locked = locked;
        }
        let text = if locked {
            "You lock the interface."
        } else {
            "You unlock the interface."
        };
        tell(world, client, Span::Notice, text);
        return true;
    }
    if world.get::<PodBeacon>(held).is_some() {
        if world.get::<PodBeacon>(held).and_then(|b| b.console) == Some(e) {
            let text = format!(
                "{} is already linked to {}.",
                name_of(world, e),
                name_of(world, held)
            );
            tell(world, client, Span::Warning, text);
        } else {
            link(world, held, e);
            let text = format!("{} linked to {}.", name_of(world, held), name_of(world, e));
            tell(world, client, Span::Notice, text);
        }
        return true;
    }
    false
}

/// `supplypod_beacon/link_console`.
pub fn link(world: &mut World, beacon: Entity, console: Entity) {
    let old_beacon = world.get::<ExpressConsole>(console).and_then(|c| c.beacon);
    if let Some(old) = old_beacon
        && old != beacon
        && let Some(mut b) = world.get_mut::<PodBeacon>(old)
    {
        b.console = None;
    }
    let old_console = world.get::<PodBeacon>(beacon).and_then(|b| b.console);
    if let Some(old) = old_console
        && let Some(mut c) = world.get_mut::<ExpressConsole>(old)
    {
        c.beacon = None;
    }
    world.entity_mut(beacon).insert(PodBeacon {
        console: Some(console),
    });
    if let Some(mut c) = world.get_mut::<ExpressConsole>(console) {
        c.beacon = Some(beacon);
    }
}

/// tg `emag_act` (from `crate::emag`). True when it took.
pub fn emag_act(world: &mut World, mob: Entity, console: Entity) -> bool {
    if world
        .get::<ExpressConsole>(console)
        .is_none_or(|c| c.emagged)
    {
        return false;
    }
    let catalogue = Arc::new(
        world
            .get_resource::<SupplyPacks>()
            .map(|p| catalogue(p, true))
            .unwrap_or_default(),
    );
    if let Some(mut c) = world.get_mut::<ExpressConsole>(console) {
        c.emagged = true;
        c.catalogue = catalogue;
    }
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        tell(
            world,
            client,
            Span::Notice,
            "You change the routing protocols, allowing the Supply Pod to land anywhere on the \
             station.",
        );
    }
    let mut windows = world.query::<&mut ss13_tgui::server::UiWindow>();
    for mut window in windows.iter_mut(world) {
        if window.interface == <CargoExpressUi as ss13_tgui::UiInterface>::NAME
            && window.source == console
        {
            window.refresh_static();
        }
    }
    true
}

/// Free open floors of `area` on `z` (`get_turfs_from_all_zlevels` of one level, filtered by
/// `is_blocked_turf`).
// Cost: one pass over the level's area table (~65k tiles) per express order (5 s cooldown).
fn free_floors(world: &World, area: Entity, z: u16) -> Vec<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let Some(areas) = world.get_resource::<ss13_mapload::Areas>() else {
        return Vec::new();
    };
    let Some((w, h)) = index.level_size(z) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for y in 1..=h as i32 {
        for x in 1..=w as i32 {
            let pos = TilePos::new(x, y, z);
            if areas.area_at(pos) != Some(area) || !index.dense_on_tile(pos).is_empty() {
                continue;
            }
            let floor = crate::construction::turf_at(world, pos)
                .and_then(|t| crate::construction::path_of(world, t))
                .is_some_and(|p| is_under(&p, FLOOR));
            if floor {
                out.push(pos);
            }
        }
    }
    out
}

fn pick_index(world: &mut World, len: usize) -> usize {
    (world.resource_mut::<ExpressState>().rng.next_u64() % len as u64) as usize
}

/// The actor's name and rank (`get_authentification_name`, `get_assignment`).
fn orderer(world: &World, mob: Entity) -> (String, String) {
    let id = world.get::<Inventory>(mob).and_then(|i| i.id_card(world));
    let registration = id.and_then(|c| world.get::<crate::outfit::IdRegistration>(c));
    registration.map_or_else(
        || ("*None Provided*".to_owned(), "*None Provided*".to_owned()),
        |r| (r.registered_name.clone(), r.assignment.clone()),
    )
}

fn pay(world: &mut World, cost: i64) -> bool {
    world
        .get_resource_mut::<Economy>()
        .and_then(|mut e| e.department_mut(ACCOUNT_CAR).map(|a| a.adjust_money(-cost)))
        .unwrap_or(false)
}

/// `ui_act` "add".
fn add(world: &mut World, console: Entity, actor: Entity, id: &str) {
    let Some(state) = world.get::<ExpressConsole>(console).cloned() else {
        return;
    };
    let t = now(world);
    if t < state.next_order {
        machine_say(world, console, "Railgun recalibrating. Stand by.");
        return;
    }
    let Some((index, def)) = world.get_resource::<SupplyPacks>().and_then(|p| {
        let i = p.by_id(id)?;
        Some((i, p.packs[i].clone()))
    }) else {
        return;
    };
    // The window only lists what the catalogue has; the server checks again.
    if !super::supply::orderable_with(&def, state.emagged, state.emagged, true) {
        return;
    }
    if def.has(order_flags::GOODY) {
        machine_say(
            world,
            console,
            "ERROR: Small crates may only be purchased by private accounts.",
        );
        return;
    }
    let (name, rank) = orderer(world, actor);
    let order_id = world.resource_mut::<SupplyOrders>().next_id();
    let order = SupplyOrder {
        id: order_id,
        pack: index,
        orderer: name,
        orderer_rank: rank,
        reason: String::new(),
        paying_account: None,
        paying_department: None,
        can_be_cancelled: false,
        coupon: None,
    };
    let Some(&here) = world.get::<TilePos>(console) else {
        return;
    };
    let discount = if state.emagged { EMAG_DISCOUNT } else { 1.0 };
    let cost = (def.cost * discount).round() as i64;
    if state.emagged {
        let areas = crate::spells::teleport_areas(world);
        if areas.is_empty() {
            return;
        }
        let i = pick_index(world, areas.len());
        let mut floors = free_floors(world, areas[i].1, here.z);
        if floors.is_empty() {
            return;
        }
        if let Some(mut c) = world.get_mut::<ExpressConsole>(console) {
            c.next_order = t + EMAG_COOLDOWN_SECS;
        }
        for _ in 0..MAX_EMAG_ROCKETS {
            if floors.is_empty() || !pay(world, cost) {
                break;
            }
            let i = pick_index(world, floors.len());
            let pos = floors.swap_remove(i);
            launch(world, pos, PodCargo::Order(order.clone()));
        }
        info!("cargo: emagged express order #{order_id} ({})", def.name);
        return;
    }
    let landing = if state.using_beacon {
        let Some(pos) = state.beacon.and_then(|b| beacon_tile(world, b)) else {
            return;
        };
        pos
    } else {
        let area = crate::shuttle::area_named(world, LANDING_ZONE).or_else(|| {
            world
                .get_resource::<ss13_mapload::Areas>()
                .and_then(|a| a.area_at(here))
        });
        let Some(area) = area else {
            return;
        };
        let floors = free_floors(world, area, here.z);
        if floors.is_empty() {
            return;
        }
        let i = pick_index(world, floors.len());
        floors[i]
    };
    if !pay(world, cost) {
        return;
    }
    if let Some(mut c) = world.get_mut::<ExpressConsole>(console) {
        c.next_order = t + ORDER_COOLDOWN_SECS;
    }
    info!(
        "cargo: express order #{order_id} ({}) by {} for {cost}{MONEY_SYMBOL}",
        def.name, order.orderer
    );
    launch(world, landing, PodCargo::Order(order));
}

/// tg `ui_act`, checked at the moment of effect: powered, unlocked.
fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<CargoExpressUi>>>) {
    let pending: Vec<(Entity, Entity, ExpressAct)> = cursor
        .read(world.resource::<Messages<UiActed<CargoExpressUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, console, act) in pending {
        let Some(state) = world.get::<ExpressConsole>(console).cloned() else {
            continue;
        };
        if state.locked
            || world.get::<Unpowered>(console).is_some()
            || !world.contains_resource::<SupplyOrders>()
        {
            continue;
        }
        match act {
            ExpressAct::LzCargo => {
                if let Some(mut c) = world.get_mut::<ExpressConsole>(console) {
                    c.using_beacon = false;
                }
            }
            ExpressAct::LzBeacon => {
                if let Some(mut c) = world.get_mut::<ExpressConsole>(console) {
                    c.using_beacon = true;
                }
            }
            ExpressAct::PrintBeacon => print_beacon(world, console),
            ExpressAct::Add(id) => add(world, console, actor, &id),
        }
    }
}

/// `ui_act` "printBeacon".
fn print_beacon(world: &mut World, console: Entity) {
    let t = now(world);
    if world
        .get::<ExpressConsole>(console)
        .is_none_or(|c| t < c.next_print)
    {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(console) else {
        return;
    };
    if !pay(world, BEACON_COST) {
        return;
    }
    if let Some(mut c) = world.get_mut::<ExpressConsole>(console) {
        c.next_print = t + BEACON_PRINT_SECS;
    }
    let Some(beacon) = crate::construction::spawn_obj(world, SUPPLYPOD_BEACON, pos) else {
        warn!("no {SUPPLYPOD_BEACON} prototype (rerun `ss13-mapload convert`)");
        return;
    };
    link(world, beacon, console);
    let printed = {
        let mut state = world.resource_mut::<ExpressState>();
        state.printed += 1;
        state.printed
    };
    let desc = world
        .get::<DmName>(beacon)
        .map(|n| n.desc.clone())
        .unwrap_or_default();
    world.entity_mut(beacon).insert(DmName {
        name: format!("Supply Pod Beacon #{printed}"),
        desc,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console() -> ExpressConsole {
        ExpressConsole {
            locked: false,
            beacon: None,
            using_beacon: false,
            emagged: false,
            next_order: 0.0,
            next_print: 0.0,
            catalogue: Arc::default(),
            beacon_valid: false,
            beacon_zone: String::new(),
            beacon_name: "No Beacon Found".into(),
            points: 600,
            print_left: 0,
        }
    }

    #[test]
    fn data_reports_beacon_errors() {
        let mut c = console();
        let data = express_data(&c, false);
        assert!(data.can_beacon && !data.beacon_error && data.can_buy_beacon);
        assert_eq!(
            data.message,
            "Sales are near-instantaneous - please choose carefully."
        );
        c.using_beacon = true;
        let data = express_data(&c, false);
        assert!(data.beacon_error);
        assert_eq!(data.message, "BEACON ERROR: BEACON MISSING");
        c.beacon = Some(Entity::PLACEHOLDER);
        assert_eq!(
            express_data(&c, false).message,
            "BEACON ERROR: MUST BE EXPOSED"
        );
        c.beacon_valid = true;
        assert!(!express_data(&c, false).beacon_error);
        c.points = 499;
        c.print_left = 3;
        let data = express_data(&c, false);
        assert!(!data.can_buy_beacon);
        assert_eq!(data.print_msg, "Print Beacon for 500¢ (3s)");
        c.emagged = true;
        assert_eq!(express_data(&c, false).message, EMAG_MESSAGE);
    }

    #[test]
    fn emagged_catalogue_is_discounted() {
        let packs = SupplyPacks::new(vec![ss13_mapload::cargo::SupplyPackDef {
            id: "/p/a".into(),
            name: "A".into(),
            group: "G".into(),
            cost: 1000.0,
            crate_type: Some("/obj/structure/closet/crate".into()),
            contains: vec![("/obj/item/wrench".into(), 1)],
            order_flags: order_flags::POD_ONLY,
            ..default()
        }]);
        assert_eq!(catalogue(&packs, false).groups[0].packs[0].cost, 1000);
        assert_eq!(catalogue(&packs, true).groups[0].packs[0].cost, 720);
        assert!(
            super::super::console::catalogue(&packs, false, false)
                .groups
                .is_empty(),
            "pod-only packs are express-only"
        );
    }
}
