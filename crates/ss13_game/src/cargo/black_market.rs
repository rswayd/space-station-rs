//! The black market (server): tg `SSmarket` (`controllers/subsystem/market.dm`), `/datum/market`
//! and `/datum/market_item` (`modules/cargo/markets/`), the market uplink
//! (`/obj/item/market_uplink`, tgui `BlackMarketUplink`) and the LTSRBT
//! (`/obj/machinery/ltsrbt`, tgui `LTSRBT`).
//!
//! - [`BlackMarket`] is the one market (`/datum/market/blackmarket`, illegal): at round start
//!   every `/datum/market_item` from `ss13-mapload convert` (`market_items` of `cargo.ron`) is
//!   stocked at its `availability_prob`, at a price and stock rolled in its ranges. Restocking
//!   tops up what is there (by `availability_prob`, `rand(1, max - stock)`) and rolls the rest
//!   again.
//! - An uplink in hand opens its window: the categories, the items of one, the user's ID account
//!   balance. Buying an item with a shipping method (LTSRBT 40 cr, launch free, teleport 75 cr;
//!   an item's own overrides) needs an ID with an account that can pay price plus fee and stock
//!   left (LTSRBT shipping also needs a pad); then it is charged ("Other: Third Party
//!   Transaction") and queued. Black market goods arrive with [`Contraband`].
//! - Shipping: the LTSRBT pad that is ready (else the one closest to ready) receives it, and on
//!   its next tick sends it to the uplink's turf; teleported goods land on a random safe station
//!   turf after 60 s (with sparks); launched goods come in a market crate thrown from the edge of
//!   space at the uplink's side of the level.
//! - The LTSRBT: an empty hand opens and closes it (closing takes an item off its tile); an item
//!   goes in while open. Closed, money worth the restock price (675 cr, doubling every time)
//!   restocks the market (holochips pay exactly, other money gives change as a holochip). Right
//!   click with an item inside opens its window: name, description and price (25-10000 cr) of a
//!   local good; placing it costs 30 cr from the ID account (at most 40 local goods), the item
//!   leaves for the market and the pad cools down for 3 × its recharge time. A sold local good
//!   pays its seller the price less 15% and leaves the list 10 minutes later.
//! - Recharge: 10 s minus 1 s per scanning module tier; 10 kJ a teleport divided by the
//!   micro-lasers' tiers.
// shortcut: no supply pod shipping (only hostages, not ported, offer it), no hostages or
// stolen goods (spy bounties), no item icons in the uplink, no `html_icon`; the uplink's
// crafting recipe waits for crafting (../items/). Donk pocket boxes come as the plain box.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::cargo::MarketItemDef;
use ss13_mapload::{DmItem, DmName, DmObj, MapSeed};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::contraband::Contraband;
use super::economy::{Economy, account_of};
use super::market_ui::*;
use super::supply::CARGO_CRATE_VALUE;
use crate::combat::CombatRng;
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::power::{MachineTicks, Unpowered};
use crate::sound::{PlaySound, playsound};
use crate::stock_parts::PartKind;
use crate::storage::{StoredIn, put_inside, take_out};
use crate::tools::tell;

pub const UPLINK: &str = "/obj/item/market_uplink";
pub const LTSRBT: &str = "/obj/machinery/ltsrbt";
pub const MARKET_CRATE: &str = "/obj/structure/closet/crate/market";
pub const BLACKMARKET: &str = "/datum/market/blackmarket";
const MARKET_NAME: &str = "Black Market";
/// `SHIPPING_METHOD_*`.
pub const LTSRBT_METHOD: &str = "LTSRBT";
pub const LAUNCH: &str = "Launch";
pub const TELEPORT: &str = "Teleport";
pub const SUPPLY_POD: &str = "Supply Pod";
/// `/datum/market/blackmarket` `shipping`.
const SHIPPING: [(&str, i64); 3] = [(LTSRBT_METHOD, 40), (LAUNCH, 0), (TELEPORT, 75)];
/// `SSmarket.shipping_method_descriptions`.
const DESCRIPTIONS: [(&str, &str); 4] = [
    (
        LAUNCH,
        "Launches the item at the station from space, with free shipping! However, you'll need to risk a spacewalk to receive your goods.",
    ),
    (
        LTSRBT_METHOD,
        "Long-To-Short-Range-Bluespace-Transceiver, a machine that receives items outside the station and then teleports them to the location of the uplink.",
    ),
    (
        TELEPORT,
        "Teleports the item in a random area in the station, you get 60 seconds to get there first though.",
    ),
    (
        SUPPLY_POD,
        "Ships the item inside a supply pod at your exact location. Showy, speedy and expensive.",
    ),
];
pub const LOCAL_GOODS: &str = "Local Goods";
/// `MARKET_WITHHOLDING_TAX`.
const WITHHOLDING_TAX: f64 = 0.15;
/// `DEFAULT_RESTOCK_COST`: `CARGO_CRATE_VALUE * 3.375`.
pub const RESTOCK_COST: i64 = (CARGO_CRATE_VALUE * 3.375) as i64;
/// `PLACE_ON_MARKET_COST`: `PAYCHECK_LOWER * 1.2`.
pub const PLACE_COST: i64 = 30;
/// `LTSRBT_MIN_PRICE` (`PAYCHECK_LOWER`), `LTSRBT_MAX_PRICE` (`CARGO_CRATE_VALUE * 50`).
pub const MIN_PRICE: i64 = 25;
pub const MAX_PRICE: i64 = (CARGO_CRATE_VALUE * 50.0) as i64;
/// `LTSRBT_MAX_MARKET_ITEMS`.
const MAX_LOCAL_GOODS: usize = 40;
const TELEPORT_DELAY: f64 = 60.0;
/// `QDEL_IN(src, 10 MINUTES)` of a sold local good.
const SOLD_LIFETIME: f64 = 600.0;
/// `energy_usage_per_teleport`, joules.
const TELEPORT_ENERGY: f64 = 10_000.0;
const BASE_RECHARGE: f64 = 10.0;
/// `spaceDebrisStartLoc` (`TRANSITIONEDGE + 1`).
const SPACE_EDGE: i32 = 8;
const CASH_REGISTER: &str = "sound/effects/cashregister.ogg";
const BUZZ: &str = "sound/machines/buzz/buzz-sigh.ogg";
const BEEP: &str = "sound/machines/beep/twobeep_high.ogg";
const OVEN_OPEN: &str = "sound/machines/oven/oven_open.ogg";
const OVEN_CLOSE: &str = "sound/machines/oven/oven_close.ogg";
const SELF_SURGERY: &str = "/datum/market_item/misc/self_surgery_skillchip";

/// What a market entry sells.
#[derive(Clone, Debug, PartialEq)]
pub enum Good {
    /// `new item(loc)`.
    Path(String),
    /// A local good: the item itself, off the map until sold.
    Instance(Entity),
}

/// A `/datum/market_item` on the market.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// `identifier`.
    pub id: String,
    /// Its [`MarketItemDef`], `None` for local goods.
    pub def: Option<usize>,
    pub name: String,
    pub desc: String,
    pub category: String,
    pub price: i64,
    pub stock: u32,
    pub availability_prob: f64,
    pub restockable: bool,
    pub shipping_override: Vec<(String, i64)>,
    pub good: Good,
    /// A local good's seller account.
    pub seller: Option<u32>,
}

/// A `/datum/market_purchase`.
#[derive(Clone, Debug, PartialEq)]
pub struct Purchase {
    pub entry: String,
    pub def: Option<usize>,
    pub good: Good,
    pub uplink: Entity,
    pub method: String,
    /// `legallity`: an illegal market's goods become contraband.
    pub legal: bool,
    /// The spawned item, while a pad sends it.
    pub item: Option<Entity>,
}

/// `SSmarket` with its one market.
#[derive(Resource)]
pub struct BlackMarket {
    pub entries: Vec<Entry>,
    /// In the order they appeared.
    pub categories: Vec<String>,
    pub defs: Vec<MarketItemDef>,
    pub queue: Vec<Purchase>,
    /// Teleported goods: (lands at, purchase, turf).
    teleports: Vec<(f64, Purchase, TilePos)>,
    /// Sold local goods leave at these times.
    expiring: Vec<(f64, String)>,
    /// `restock_cost` of the LTSRBTs (static: shared by all).
    pub restock_cost: i64,
    rng: CombatRng,
    next_local: u64,
}

impl BlackMarket {
    /// `SSmarket.Initialize`: every item rolled in.
    pub fn new(defs: Vec<MarketItemDef>, seed: u64) -> Self {
        let mut market = Self {
            entries: Vec::new(),
            categories: Vec::new(),
            defs,
            queue: Vec::new(),
            teleports: Vec::new(),
            expiring: Vec::new(),
            restock_cost: RESTOCK_COST,
            rng: CombatRng(seed ^ 0x6d61_726b_6574),
            next_local: 0,
        };
        for i in 0..market.defs.len() {
            market.initialize_item(i);
        }
        market
    }

    /// `initialize_item`: in at its `availability_prob`, price and stock rolled.
    fn initialize_item(&mut self, i: usize) {
        let def = self.defs[i].clone();
        if def.item.is_empty() || !def.markets.iter().any(|m| m == BLACKMARKET) {
            return;
        }
        if !self.rng.chance(def.availability_prob as f32) {
            return;
        }
        let roll = |rng: &mut CombatRng, fixed: Option<f64>, min: f64, max: f64| -> i64 {
            fixed.map_or_else(
                || {
                    let (lo, hi) = (min.round().max(0.0) as u32, max.round().max(0.0) as u32);
                    i64::from(rng.range(lo.min(hi), hi.max(lo)))
                },
                |v| v as i64,
            )
        };
        let price = roll(&mut self.rng, def.price, def.price_min, def.price_max);
        let stock = roll(&mut self.rng, def.stock, def.stock_min, def.stock_max).max(0) as u32;
        let name = if def.name.is_empty() {
            def.item.clone()
        } else {
            def.name.clone()
        };
        self.add(Entry {
            id: def.id.clone(),
            def: Some(i),
            name,
            desc: def.desc.clone(),
            category: def.category.clone(),
            price,
            stock,
            availability_prob: def.availability_prob,
            restockable: def.restockable,
            shipping_override: def
                .shipping_override
                .iter()
                .map(|(m, f)| (m.clone(), *f as i64))
                .collect(),
            good: Good::Path(def.item.clone()),
            seller: None,
        });
    }

    /// `/datum/market/add_item`.
    pub fn add(&mut self, entry: Entry) {
        if !self.categories.contains(&entry.category) {
            self.categories.push(entry.category.clone());
        }
        self.entries.retain(|e| e.id != entry.id);
        self.entries.push(entry);
    }

    /// `on_item_del`: the entry leaves, and its category when it was the last.
    pub fn remove(&mut self, id: &str) {
        let Some(i) = self.entries.iter().position(|e| e.id == id) else {
            return;
        };
        let category = self.entries.remove(i).category;
        if !self.entries.iter().any(|e| e.category == category) {
            self.categories.retain(|c| *c != category);
        }
    }

    pub fn entry(&self, category: &str, id: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.id == id && e.category == category)
    }

    /// `SSmarket.restock()`: the stocked items topped up, the others rolled again.
    pub fn restock(&mut self) {
        let mut existing: Vec<Option<usize>> = Vec::new();
        for i in 0..self.entries.len() {
            existing.push(self.entries[i].def);
            let e = &self.entries[i];
            let max = e.def.map_or(0.0, |d| self.defs[d].stock_max) as u32;
            let prob = e.availability_prob as f32;
            if !e.restockable || e.stock >= max || !self.rng.chance(prob) {
                continue;
            }
            let add = self.rng.range(1, max - e.stock);
            self.entries[i].stock += add;
        }
        for i in 0..self.defs.len() {
            if existing.contains(&Some(i)) || !self.defs[i].restockable {
                continue;
            }
            self.initialize_item(i);
        }
    }

    /// The fee of `method` for `entry` (`shipping_override`, else the market's).
    pub fn fee(entry: &Entry, method: &str) -> Option<i64> {
        if !entry.shipping_override.is_empty() {
            return entry
                .shipping_override
                .iter()
                .find(|(m, _)| m == method)
                .map(|&(_, f)| f);
        }
        SHIPPING.iter().find(|(m, _)| *m == method).map(|&(_, f)| f)
    }

    pub fn methods(entry: &Entry) -> Vec<(String, i64)> {
        if entry.shipping_override.is_empty() {
            SHIPPING
                .iter()
                .map(|(m, f)| ((*m).to_owned(), *f))
                .collect()
        } else {
            entry.shipping_override.clone()
        }
    }

    fn local_goods(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.category == LOCAL_GOODS)
            .count()
    }
}

/// A market uplink's window state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(crate::chem_machines::Views<UplinkData>)]
pub struct MarketUplink {
    pub viewing_category: Option<String>,
    pub selected: Option<String>,
    pub buying: bool,
}

/// An LTSRBT pad.
#[derive(Component, Clone, Debug, PartialEq)]
#[require(crate::chem_machines::Views<LtsrbtData>)]
pub struct Ltsrbt {
    /// `state_open`.
    pub open: bool,
    pub occupant: Option<Entity>,
    pub current_name: String,
    pub current_desc: String,
    pub current_price: i64,
    /// `recharge_cooldown`: ready at this game time.
    pub ready_at: f64,
    /// Seconds.
    pub recharge_time: f64,
    pub power_efficiency: f64,
    pub receiving: Option<Purchase>,
    pub transmitting: Option<Purchase>,
    pub queue: Vec<Purchase>,
}

impl Default for Ltsrbt {
    fn default() -> Self {
        Self {
            open: false,
            occupant: None,
            current_name: String::new(),
            current_desc: String::new(),
            current_price: CARGO_CRATE_VALUE as i64,
            ready_at: 0.0,
            recharge_time: BASE_RECHARGE,
            power_efficiency: 1.0,
            receiving: None,
            transmitting: None,
            queue: Vec::new(),
        }
    }
}

pub struct BlackMarketPlugin;

impl Plugin for BlackMarketPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 3325, |w, c| {
            crate::cargo::black_market::attack_self(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::EmptyHand, 3585, |w, c| {
            crate::cargo::black_market::ltsrbt_hand(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 1515, |w, c| {
            c.held.is_some_and(|h| {
                crate::cargo::black_market::ltsrbt_item(w, c.client, c.mob, h, c.target)
            })
        });
        app.register_ui::<BlackMarketUi>()
            .register_ui::<LtsrbtUi>()
            .add_systems(Startup, load.after(super::supply::load_packs))
            .add_systems(ss13_world::MapInit, setup_mapped)
            .add_systems(
                Update,
                (
                    (uplink_acts, ltsrbt_acts).in_set(UiSystems::Handle),
                    ship.run_if(resource_exists::<BlackMarket>),
                    pads.after(crate::power::PowerSystems),
                    (
                        |w: &mut World| {
                            crate::chem_machines::refresh_views::<BlackMarketUi>(w, uplink_view)
                        },
                        |w: &mut World| {
                            crate::chem_machines::refresh_views::<LtsrbtUi>(w, ltsrbt_view)
                        },
                    )
                        .after(UiSystems::Handle)
                        .before(UiSystems::Send),
                ),
            );
    }
}

/// `SSmarket.Initialize` from `cargo.ron` (unless a test inserted the market).
fn load(world: &mut World) {
    if world.contains_resource::<BlackMarket>() {
        return;
    }
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let defs = world
        .get_resource::<ss13_mapload::MapLoadPlugin>()
        .map(|m| m.assets.join(ss13_mapload::cargo::CARGO_FILE))
        .and_then(|p| ss13_mapload::cargo::CargoData::read(&p).ok())
        .map(|d| d.market_items)
        .unwrap_or_default();
    let market = BlackMarket::new(defs, seed);
    info!(
        "black market: {} items in {} categories",
        market.entries.len(),
        market.categories.len()
    );
    world.insert_resource(market);
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, LTSRBT))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// A pad's state and `RefreshParts` (mapped, built from a frame or on first use).
pub fn setup(world: &mut World, e: Entity) {
    if world.get::<Ltsrbt>(e).is_some() {
        return;
    }
    crate::stock_parts::ensure_parts(world, e);
    world.entity_mut(e).insert(Ltsrbt::default());
    refresh_parts(world, e);
}

fn ensure_pad(world: &mut World, e: Entity) -> bool {
    if world.get::<Ltsrbt>(e).is_some() {
        return true;
    }
    if !world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, LTSRBT))
    {
        return false;
    }
    setup(world, e);
    true
}

/// tg `RefreshParts`: recharge 10 s minus 1 s per scanning module tier; efficiency the
/// micro-lasers' tiers.
pub fn refresh_parts(world: &mut World, e: Entity) {
    if world.get::<Ltsrbt>(e).is_none() {
        return;
    }
    let r = crate::stock_parts::ratings(world, e);
    let recharge = r
        .tiers(PartKind::ScanningModule)
        .fold(BASE_RECHARGE, |t, tier| t - f64::from(tier));
    let efficiency = r.tiers(PartKind::MicroLaser).map(f64::from).sum::<f64>();
    if let Some(mut pad) = world.get_mut::<Ltsrbt>(e) {
        pad.recharge_time = recharge.max(0.0);
        pad.power_efficiency = if efficiency > 0.0 { efficiency } else { 1.0 };
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn say(world: &mut World, e: Entity, text: &str) {
    crate::cargo::console::machine_say(world, e, text);
}

fn sound(world: &mut World, e: Entity, file: &str, vol: f32) {
    if let Some(pos) = crate::chem::map_pos(world, e) {
        playsound(world, PlaySound::new(pos, file, vol, false));
    }
}

/// The account of the ID `mob` carries (`get_idcard()?.registered_account`).
fn account(world: &World, mob: Entity) -> Option<u32> {
    let card = world.get::<Inventory>(mob)?.id_card(world)?;
    account_of(world, card)
}

/// `attack_self` of a market uplink: its window.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, UPLINK))
    {
        return false;
    }
    if world.get::<MarketUplink>(item).is_none() {
        world.entity_mut(item).insert(MarketUplink::default());
    }
    // `update_viewing_category`.
    let first = world
        .get_resource::<BlackMarket>()
        .and_then(|m| m.categories.first().cloned());
    if let Some(mut u) = world.get_mut::<MarketUplink>(item)
        && u.viewing_category.is_none()
    {
        u.viewing_category = first;
    }
    let title = world
        .get::<DmName>(item)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<BlackMarketUi>(client, mob, item, title));
    true
}

fn uplink_view(world: &mut World, e: Entity, user: Entity) -> Option<UplinkData> {
    let u = world.get::<MarketUplink>(e)?.clone();
    let market = world.get_resource::<BlackMarket>()?;
    let money = account(world, user).and_then(|id| {
        world
            .get_resource::<Economy>()
            .and_then(|e| e.account(id))
            .map(|a| a.balance)
    });
    let category = u
        .viewing_category
        .clone()
        .filter(|c| market.categories.contains(c));
    let items = category
        .as_ref()
        .map(|c| {
            market
                .entries
                .iter()
                .filter(|e| e.category == *c && !gone(world, e))
                .map(|e| MarketItemView {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    cost: e.price,
                    amount: e.stock,
                    desc: if e.desc.is_empty() {
                        e.name.clone()
                    } else {
                        e.desc.clone()
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    let delivery_methods = if u.buying {
        u.selected
            .as_ref()
            .zip(category.as_ref())
            .and_then(|(id, c)| market.entry(c, id))
            .map(BlackMarket::methods)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let pads = {
        let mut q = world.query_filtered::<(), With<Ltsrbt>>();
        q.iter(world).count()
    };
    let market = world.get_resource::<BlackMarket>()?;
    Some(UplinkData {
        categories: market.categories.clone(),
        delivery_methods,
        money,
        buying: u.buying,
        items,
        viewing_category: category,
        viewing_market: BLACKMARKET.to_owned(),
        markets: vec![(BLACKMARKET.to_owned(), MARKET_NAME.to_owned())],
        delivery_method_description: DESCRIPTIONS
            .iter()
            .map(|(m, d)| ((*m).to_owned(), (*d).to_owned()))
            .collect(),
        ltsrbt_built: pads > 0,
    })
}

/// A local good whose item is gone (tg deletes the entry with it).
fn gone(world: &World, e: &Entry) -> bool {
    matches!(e.good, Good::Instance(item) if world.get_entity(item).is_err())
}

impl ServerUi for BlackMarketUi {
    type Source = &'static crate::chem_machines::Views<UplinkData>;
    type UserParam = ();
    type Act = UplinkAct;

    fn static_data(_: &crate::chem_machines::Views<UplinkData>) {}

    fn data(views: &crate::chem_machines::Views<UplinkData>, user: Entity, _: &()) -> UplinkData {
        views.of(user)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<UplinkAct, UiReject> {
        Ok(match action {
            "set_category" => UplinkAct::SetCategory(params.text("category")?),
            "set_market" => UplinkAct::SetMarket(params.text("market")?),
            "select" => UplinkAct::Select(params.text("item")?),
            "cancel" => UplinkAct::Cancel,
            "buy" => UplinkAct::Buy(params.text("method")?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UplinkAct {
    SetCategory(String),
    SetMarket(String),
    Select(String),
    Cancel,
    Buy(String),
}

fn uplink_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<BlackMarketUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, UplinkAct)> = cursor
        .read(world.resource::<Messages<UiActed<BlackMarketUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, e, act) in pending {
        uplink_act(world, client, actor, e, act);
    }
}

/// tg `market_uplink/ui_act`, checked at the moment of effect.
pub fn uplink_act(world: &mut World, client: Entity, actor: Entity, e: Entity, act: UplinkAct) {
    let Some(u) = world.get::<MarketUplink>(e).cloned() else {
        return;
    };
    let Some(categories) = world
        .get_resource::<BlackMarket>()
        .map(|m| m.categories.clone())
    else {
        return;
    };
    let set = |world: &mut World, f: &dyn Fn(&mut MarketUplink)| {
        if let Some(mut u) = world.get_mut::<MarketUplink>(e) {
            f(&mut *u);
        }
    };
    match act {
        UplinkAct::SetCategory(c) => {
            if categories.contains(&c) {
                set(world, &|u| u.viewing_category = Some(c.clone()));
            }
        }
        UplinkAct::SetMarket(m) => {
            if m == BLACKMARKET {
                let first = categories.first().cloned();
                set(world, &|u| u.viewing_category = first.clone());
            }
        }
        UplinkAct::Select(id) => set(world, &|u| {
            u.selected = Some(id.clone());
            u.buying = true;
        }),
        UplinkAct::Cancel => set(world, &|u| {
            u.selected = None;
            u.buying = false;
        }),
        UplinkAct::Buy(method) => {
            if let (Some(id), Some(category)) = (u.selected.clone(), u.viewing_category.clone()) {
                purchase(world, client, actor, e, &category, &id, &method);
            }
            set(world, &|u| {
                u.selected = None;
                u.buying = false;
            });
        }
    }
}

/// `/datum/market/purchase` and `/datum/market_item/buy`: true when it was bought.
pub fn purchase(
    world: &mut World,
    client: Entity,
    buyer: Entity,
    uplink: Entity,
    category: &str,
    id: &str,
    method: &str,
) -> bool {
    let Some(entry) = world
        .get_resource::<BlackMarket>()
        .and_then(|m| m.entry(category, id).cloned())
    else {
        return false;
    };
    if gone(world, &entry) {
        return false;
    }
    let Some(fee) = BlackMarket::fee(&entry, method) else {
        return false;
    };
    let price = entry.price + fee;
    let Some(account) = account(world, buyer) else {
        tell(
            world,
            client,
            Span::Warning,
            "Аплинк искрит: он не может найти у вас ID-карту с банковским счётом.",
        );
        return false;
    };
    let balance = world
        .get_resource::<Economy>()
        .and_then(|e| e.account(account))
        .map_or(0, |a| a.balance);
    if balance < price {
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "На счёте недостаточно кредитов для {} с доставкой «{method}».",
                entry.name
            ),
        );
        return false;
    }
    if entry.stock == 0 {
        return false;
    }
    let pads = {
        let mut q = world.query_filtered::<(), With<Ltsrbt>>();
        q.iter(world).count()
    };
    if method == LTSRBT_METHOD && pads == 0 {
        return false;
    }
    let paid = world
        .get_resource_mut::<Economy>()
        .is_some_and(|mut economy| {
            economy
                .account_mut(account)
                .is_some_and(|a| a.adjust_money(-price))
        });
    if !paid {
        return false;
    }
    {
        let mut market = world.resource_mut::<BlackMarket>();
        if let Some(e) = market.entries.iter_mut().find(|e| e.id == entry.id) {
            e.stock -= 1;
        }
        // `self_surgery_skillchip/buy`: rarer on every restock after a sale.
        if let Some(d) = entry.def
            && market.defs[d].id == SELF_SURGERY
        {
            market.defs[d].availability_prob *= 0.5;
            if let Some(e) = market.entries.iter_mut().find(|e| e.id == entry.id) {
                e.availability_prob *= 0.5;
            }
        }
        market.queue.push(Purchase {
            entry: entry.id.clone(),
            def: entry.def,
            good: entry.good.clone(),
            uplink,
            method: method.to_owned(),
            legal: false,
            item: None,
        });
    }
    // `local_good/buy`: the seller is paid, the entry leaves in 10 minutes.
    if entry.category == LOCAL_GOODS {
        let t = world.resource::<Time>().elapsed_secs_f64();
        world
            .resource_mut::<BlackMarket>()
            .expiring
            .push((t + SOLD_LIFETIME, entry.id.clone()));
        if let Some(seller) = entry.seller
            && let Some(mut economy) = world.get_resource_mut::<Economy>()
            && let Some(a) = economy.account_mut(seller)
        {
            a.adjust_money((entry.price as f64 * (1.0 - WITHHOLDING_TAX)).round() as i64);
        }
    }
    info!("black market: purchased {} using {method}", entry.name);
    if let Some(pos) = crate::chem::map_pos(world, buyer) {
        playsound(world, PlaySound::new(pos, BEEP, 50.0, true));
    }
    true
}

/// `spawn_item` with the item's own overrides, and `post_purchase_effects`.
fn spawn_good(world: &mut World, purchase: &Purchase, pos: TilePos) -> Option<Entity> {
    let item = match &purchase.good {
        Good::Instance(item) => {
            let item = *item;
            world.get_entity(item).ok()?;
            take_out(world, item, pos);
            crate::effects::particles::do_sparks(world, 8, false, pos);
            item
        }
        Good::Path(path) => {
            let def_id = purchase
                .def
                .and_then(|d| {
                    world
                        .get_resource::<BlackMarket>()
                        .map(|m| m.defs[d].id.clone())
                })
                .unwrap_or_default();
            spawn_custom(world, &def_id, path, pos)?
        }
    };
    if !purchase.legal {
        world.entity_mut(item).insert(Contraband);
    }
    Some(item)
}

/// The market items whose `spawn_item` makes something else than `new item(loc)`.
fn spawn_custom(world: &mut World, def: &str, path: &str, pos: TilePos) -> Option<Entity> {
    let roll = |world: &mut World, n: u32| {
        world.resource_mut::<BlackMarket>().rng.range(0, n - 1) as usize
    };
    match def {
        "/datum/market_item/misc/holywater" => {
            let unholy = world.resource_mut::<BlackMarket>().rng.chance(6.66);
            let path = if unholy {
                "/obj/item/reagent_containers/cup/beaker/unholywater"
            } else {
                path
            };
            crate::construction::spawn_obj(world, path, pos)
        }
        "/datum/market_item/consumable/suspicious_pills" => {
            const PILLS: [&str; 6] = [
                "/obj/item/storage/pill_bottle/zoom",
                "/obj/item/storage/pill_bottle/happy",
                "/obj/item/storage/pill_bottle/lsd",
                "/obj/item/storage/pill_bottle/aranesp",
                "/obj/item/storage/pill_bottle/stimulant",
                "/obj/item/storage/pill_bottle/maintenance_pill",
            ];
            let i = roll(world, PILLS.len() as u32);
            crate::construction::spawn_obj(world, PILLS[i], pos)
        }
        "/datum/market_item/clothing/full_spacesuit_set" => {
            let boxed = crate::construction::spawn_obj(world, path, pos)?;
            if let Some(mut n) = world.get_mut::<DmName>(boxed) {
                n.name = "Spacesuit Box".into();
                n.desc = "It has an NT logo on it.".into();
            }
            for part in [
                "/obj/item/clothing/suit/space",
                "/obj/item/clothing/head/helmet/space",
            ] {
                if let Some(p) = crate::construction::spawn_detached(world, part) {
                    put_inside(world, p, boxed);
                }
            }
            Some(boxed)
        }
        "/datum/market_item/misc/girlypop" => {
            crate::construction::spawn_obj(world, path, pos);
            let glitter =
                crate::construction::spawn_obj(world, "/obj/structure/closet/crate/glitter", pos)?;
            for poster in GIRLYPOP {
                if let Some(p) = crate::construction::spawn_detached(world, poster) {
                    put_inside(world, p, glitter);
                }
            }
            Some(glitter)
        }
        _ => crate::construction::spawn_obj(world, path, pos),
    }
}

const GIRLYPOP: [&str; 13] = [
    "/obj/item/poster/contraband/dream",
    "/obj/item/poster/contraband/beekind",
    "/obj/item/poster/contraband/heart",
    "/obj/item/poster/contraband/dolphin",
    "/obj/item/poster/contraband/principles",
    "/obj/item/poster/contraband/trigger",
    "/obj/item/poster/contraband/barbaro",
    "/obj/item/poster/contraband/seabiscuit",
    "/obj/item/poster/contraband/pharlap",
    "/obj/item/poster/contraband/waradmiral",
    "/obj/item/poster/contraband/silver",
    "/obj/item/poster/contraband/jovial",
    "/obj/item/poster/contraband/bojack",
];

/// The buyer: the mob holding the uplink (`recursive_loc_check(uplink.loc, /mob)`).
fn holder_mob(world: &World, mut e: Entity) -> Option<Entity> {
    for _ in 0..8 {
        if let Some(c) = world.get::<CarriedBy>(e) {
            return Some(c.0);
        }
        e = world.get::<StoredIn>(e)?.0;
    }
    None
}

fn tell_buyer(world: &mut World, uplink: Entity, text: String) {
    if let Some(mob) = holder_mob(world, uplink) {
        crate::tools::tell_mob(world, mob, Span::Notice, text);
    }
}

/// `SSmarket.fire`: the queued purchases shipped, teleports landing, sold goods leaving.
// Cost: per tick, the queue (empty but for a moment after a purchase) and two short lists.
fn ship(world: &mut World) {
    let t = now(world);
    let (queue, landing) = {
        let mut market = world.resource_mut::<BlackMarket>();
        let queue = std::mem::take(&mut market.queue);
        let (landing, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut market.teleports)
            .into_iter()
            .partition(|(at, ..)| *at <= t);
        market.teleports = waiting;
        let (expired, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut market.expiring)
            .into_iter()
            .partition(|(at, _)| *at <= t);
        market.expiring = kept;
        for (_, id) in expired {
            market.remove(&id);
        }
        (queue, landing)
    };
    for (_, purchase, pos) in landing {
        if spawn_good(world, &purchase, pos).is_some() {
            crate::effects::particles::do_sparks(world, 5, true, pos);
        }
    }
    for purchase in queue {
        if world.get_entity(purchase.uplink).is_err() {
            continue;
        }
        match purchase.method.as_str() {
            LTSRBT_METHOD => to_pad(world, purchase),
            TELEPORT => {
                let Some(pos) = crate::events::electrical_storm::random_station_turf(world) else {
                    continue;
                };
                let area = crate::bots::area_name(world, pos);
                tell_buyer(
                    world,
                    purchase.uplink,
                    format!(
                        "Аплинк сообщает: заказ будет телепортирован в {area} через 60 секунд."
                    ),
                );
                world.resource_mut::<BlackMarket>().teleports.push((
                    t + TELEPORT_DELAY,
                    purchase,
                    pos,
                ));
            }
            LAUNCH => launch(world, purchase),
            _ => {
                if let Some(pos) = crate::chem::map_pos(world, purchase.uplink) {
                    if let Good::Path(path) = &purchase.good {
                        super::supplypod::launch(
                            world,
                            pos,
                            super::supplypod::PodCargo::Items(vec![path.clone()]),
                        );
                    } else {
                        spawn_good(world, &purchase, pos);
                    }
                    tell_buyer(
                        world,
                        purchase.uplink,
                        "Аплинк сообщает: заказ запущен прямо к вам.".to_owned(),
                    );
                }
            }
        }
    }
}

/// `SHIPPING_METHOD_LTSRBT`: the ready pad, else the one ready soonest (unpowered last).
fn to_pad(world: &mut World, purchase: Purchase) {
    let t = now(world);
    let mut q = world.query::<(Entity, &Ltsrbt, Has<Unpowered>)>();
    let mut best: Option<(Entity, f64)> = None;
    for (e, pad, unpowered) in q.iter(world) {
        if pad.ready_at <= t && !unpowered {
            best = Some((e, -1.0));
            break;
        }
        let left = if unpowered {
            f64::MAX / 2.0
        } else {
            pad.ready_at - t
        };
        if best.is_none_or(|(_, b)| left <= b) {
            best = Some((e, left));
        }
    }
    let Some((pad, _)) = best else {
        return;
    };
    let uplink = purchase.uplink;
    if let Some(mut p) = world.get_mut::<Ltsrbt>(pad) {
        if p.ready_at <= t && p.receiving.is_none() && p.transmitting.is_none() {
            p.receiving = Some(purchase);
        } else {
            p.queue.push(purchase);
        }
    }
    let name = crate::chem_machines::title(world, pad);
    tell_buyer(
        world,
        uplink,
        format!("Аплинк сообщает: заказ обрабатывает {name}."),
    );
}

/// `SHIPPING_METHOD_LAUNCH`: a market crate from the edge of space, thrown 3 tiles at the
/// uplink.
fn launch(world: &mut World, purchase: Purchase) {
    let Some(at) = crate::chem::map_pos(world, purchase.uplink) else {
        return;
    };
    let Some((w, h)) = world
        .resource::<ss13_world::SpatialIndex>()
        .level_size(at.z)
    else {
        return;
    };
    let (w, h) = (w as i32, h as i32);
    let (start, dir) = {
        let mut market = world.resource_mut::<BlackMarket>();
        let rng = &mut market.rng;
        let side = rng.range(0, 3);
        let mut along = |len: i32| {
            rng.range(SPACE_EDGE as u32, (len - SPACE_EDGE).max(SPACE_EDGE) as u32) as i32
        };
        match side {
            0 => (TilePos::new(along(w), h - SPACE_EDGE, at.z), "севера"),
            1 => (TilePos::new(along(w), SPACE_EDGE, at.z), "юга"),
            2 => (TilePos::new(w - SPACE_EDGE, along(h), at.z), "востока"),
            _ => (TilePos::new(SPACE_EDGE, along(h), at.z), "запада"),
        }
    };
    let Some(pod) = crate::construction::spawn_obj(world, MARKET_CRATE, start) else {
        return;
    };
    if let Some(item) = spawn_good(world, &purchase, start) {
        world.entity_mut(item).remove::<TilePos>();
        put_inside(world, item, pod);
    }
    crate::throw::throw_at(world, pod, at, 3, 3.0);
    tell_buyer(
        world,
        purchase.uplink,
        format!("Аплинк сообщает: заказ запущен к станции с {dir}."),
    );
}

/// `ltsrbt/process`, every machine tick.
// Cost: one pass over the pads (a few at most) per machine tick (2 s).
fn pads(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let mut q = world.query_filtered::<Entity, With<Ltsrbt>>();
    let all: Vec<Entity> = q.iter(world).collect();
    for e in all {
        pad_tick(world, e);
    }
}

fn pad_tick(world: &mut World, e: Entity) {
    if world.get::<Unpowered>(e).is_some() {
        return;
    }
    let t = now(world);
    let Some(mut pad) = world.get::<Ltsrbt>(e).cloned() else {
        return;
    };
    let Some(&turf) = world.get::<TilePos>(e) else {
        return;
    };
    let energy = TELEPORT_ENERGY / pad.power_efficiency;
    if pad.ready_at > t && pad.receiving.is_none() && pad.transmitting.is_none() {
        return;
    }
    if let Some(mut receiving) = pad.receiving.take() {
        receiving.item = spawn_good(world, &receiving, turf);
        crate::kitchen::machine_use_energy(world, e, energy);
        crate::effects::particles::do_sparks(world, 5, true, turf);
        pad.transmitting = Some(receiving);
    } else if let Some(sending) = pad.transmitting.take() {
        if let Some(item) = sending.item
            && world.get::<TilePos>(item) == Some(&turf)
            && let Some(dest) = crate::chem::map_pos(world, sending.uplink)
        {
            crate::teleport::do_teleport(world, item, dest, crate::teleport::Tele::default());
            crate::kitchen::machine_use_energy(world, e, energy);
        }
        pad.ready_at = t + pad.recharge_time;
    } else if !pad.queue.is_empty() {
        let i = world
            .get_resource_or_init::<CombatRng>()
            .range(0, pad.queue.len() as u32 - 1) as usize;
        pad.receiving = Some(pad.queue.remove(i));
    }
    world.entity_mut(e).insert(pad);
}

fn occupant(world: &World, e: Entity) -> Option<Entity> {
    crate::chem_machines::inside(world, e, world.get::<Ltsrbt>(e)?.occupant)
}

/// `open_machine`: the occupant out onto the tile.
fn open_pad(world: &mut World, e: Entity) {
    if let Some(item) = occupant(world, e)
        && let Some(&pos) = world.get::<TilePos>(e)
    {
        take_out(world, item, pos);
    }
    if let Some(mut pad) = world.get_mut::<Ltsrbt>(e) {
        pad.open = true;
        pad.occupant = None;
        pad.current_name.clear();
        pad.current_desc.clear();
        pad.current_price = CARGO_CRATE_VALUE as i64;
    }
    sound(world, e, OVEN_OPEN, 75.0);
}

/// `close_machine(target)`: without one, an unanchored item on the tile goes in.
fn close_pad(world: &mut World, e: Entity, target: Option<Entity>) {
    let target = target.or_else(|| {
        let pos = *world.get::<TilePos>(e)?;
        world
            .resource::<ss13_world::SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&i| {
                world.get::<DmItem>(i).is_some_and(|d| !d.anchored)
                    && world.get::<CarriedBy>(i).is_none()
            })
    });
    if let Some(item) = target {
        put_inside(world, item, e);
    }
    let (name, desc) = target
        .and_then(|i| world.get::<DmName>(i))
        .map(|n| {
            (
                crate::vending::plain_name(&n.name).to_owned(),
                n.desc.clone(),
            )
        })
        .unwrap_or_default();
    if let Some(mut pad) = world.get_mut::<Ltsrbt>(e) {
        pad.open = false;
        pad.occupant = target;
        if target.is_some() {
            pad.current_name = name;
            pad.current_desc = desc;
        }
    }
    sound(world, e, OVEN_CLOSE, 75.0);
}

/// `attack_hand` (open or close) and `attack_hand_secondary` (the window).
pub fn ltsrbt_hand(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) -> bool {
    if !ensure_pad(world, e) {
        return false;
    }
    let Some(pad) = world.get::<Ltsrbt>(e).cloned() else {
        return false;
    };
    if !right {
        if pad.open {
            close_pad(world, e, None);
        } else {
            open_pad(world, e);
        }
        return true;
    }
    let text = if pad.open {
        "сначала закройте!"
    } else if occupant(world, e).is_none() {
        "внутри ничего нет!"
    } else if world.get::<Unpowered>(e).is_some() {
        "нет питания!"
    } else if pad.ready_at > now(world) {
        "перезаряжается!"
    } else {
        let title = crate::chem_machines::title(world, e);
        world.write_message(OpenUi::new::<LtsrbtUi>(client, mob, e, title));
        return true;
    };
    crate::balloon::balloon_alert(world, mob, e, text);
    true
}

/// `item_interaction`: an item in while open; closed, money restocks the market.
pub fn ltsrbt_item(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    if !ensure_pad(world, e) {
        return false;
    }
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    let open = world.get::<Ltsrbt>(e).is_some_and(|p| p.open);
    let value = super::money::credit_value(world, held);
    if open {
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
            && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
        {
            *slot = None;
        }
        world.entity_mut(held).remove::<CarriedBy>();
        crate::balloon::balloon_alert(world, mob, e, "предмет загружен");
        close_pad(world, e, Some(held));
        return true;
    }
    if value <= 0 {
        // Tools and the like work on it as on any machine (`machine_frame`).
        if crate::tools::tool_of(world, held).is_some() {
            return false;
        }
        crate::balloon::balloon_alert(world, mob, e, "откройте машину!");
        return true;
    }
    if world.get::<Unpowered>(e).is_some() {
        return true;
    }
    let Some(cost) = world.get_resource::<BlackMarket>().map(|m| m.restock_cost) else {
        return true;
    };
    if value < cost {
        say(world, e, "Insufficient credits!");
        sound(world, e, BUZZ, 40.0);
        return true;
    }
    let chip = world.get::<super::money::Holochip>(held).map(|c| c.credits);
    if let Some(credits) = chip {
        let left = credits.saturating_sub(cost as u32);
        if left == 0 {
            crate::chem_machines::consume_held(world, mob, held);
        } else {
            world
                .entity_mut(held)
                .insert(super::money::Holochip { credits: left });
            super::money::refresh_holochip(world, held);
        }
    } else {
        crate::chem_machines::consume_held(world, mob, held);
        if value > cost {
            super::money::give_holochip(world, mob, (value - cost) as u32);
        }
    }
    restock(world);
    world.resource_mut::<BlackMarket>().restock_cost *= 2;
    tell(world, client, Span::Notice, "Рынок пополнен.");
    true
}

/// `SSmarket.restock()` with the pads' announcement.
pub fn restock(world: &mut World) {
    if let Some(mut m) = world.get_resource_mut::<BlackMarket>() {
        m.restock();
    }
    let mut q = world.query_filtered::<Entity, With<Ltsrbt>>();
    let pads: Vec<Entity> = q.iter(world).collect();
    for pad in pads {
        say(world, pad, "Markets restocked!");
        sound(world, pad, CASH_REGISTER, 40.0);
    }
}

fn ltsrbt_view(world: &mut World, e: Entity, _user: Entity) -> Option<LtsrbtData> {
    let pad = world.get::<Ltsrbt>(e)?.clone();
    let loaded = occupant(world, e)
        .and_then(|i| world.get::<DmName>(i))
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    Some(LtsrbtData {
        name: pad.current_name,
        price: pad.current_price,
        desc: pad.current_desc,
        min_price: MIN_PRICE,
        max_price: MAX_PRICE,
        loaded,
    })
}

impl ServerUi for LtsrbtUi {
    type Source = &'static crate::chem_machines::Views<LtsrbtData>;
    type UserParam = ();
    type Act = LtsrbtAct;

    fn static_data(_: &crate::chem_machines::Views<LtsrbtData>) {}

    fn data(views: &crate::chem_machines::Views<LtsrbtData>, user: Entity, _: &()) -> LtsrbtData {
        views.of(user)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<LtsrbtAct, UiReject> {
        Ok(match action {
            "change_name" => LtsrbtAct::Name(params.text("value")?),
            "change_desc" => LtsrbtAct::Desc(params.text("value")?),
            "change_price" => LtsrbtAct::Price(params.number("value")?),
            "place_on_market" => LtsrbtAct::Place,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LtsrbtAct {
    Name(String),
    Desc(String),
    Price(f64),
    Place,
}

fn ltsrbt_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<LtsrbtUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, LtsrbtAct)> = cursor
        .read(world.resource::<Messages<UiActed<LtsrbtUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, e, act) in pending {
        ltsrbt_act(world, client, actor, e, act);
    }
}

/// `MAX_NAME_LEN`, `MAX_DESC_LEN`.
const MAX_NAME_LEN: usize = 42;
const MAX_DESC_LEN: usize = 1024;

fn trimmed(text: &str, max: usize) -> String {
    text.trim()
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(max)
        .collect()
}

/// tg `ltsrbt/ui_act` (its `ui_state` closes the window without an item or while recharging):
/// each value checked by its own kind.
pub fn ltsrbt_act(world: &mut World, client: Entity, actor: Entity, e: Entity, act: LtsrbtAct) {
    let Some(pad) = world.get::<Ltsrbt>(e).cloned() else {
        return;
    };
    if occupant(world, e).is_none() || pad.ready_at > now(world) {
        return;
    }
    match act {
        LtsrbtAct::Name(v) => {
            let v = trimmed(&v, MAX_NAME_LEN);
            if let Some(mut p) = world.get_mut::<Ltsrbt>(e) {
                p.current_name = v;
            }
        }
        LtsrbtAct::Desc(v) => {
            let v = trimmed(&v, MAX_DESC_LEN);
            if let Some(mut p) = world.get_mut::<Ltsrbt>(e) {
                p.current_desc = v;
            }
        }
        LtsrbtAct::Price(v) => {
            if v.is_finite()
                && let Some(mut p) = world.get_mut::<Ltsrbt>(e)
            {
                p.current_price = (v as i64).clamp(MIN_PRICE, MAX_PRICE);
            }
        }
        LtsrbtAct::Place => place_on_market(world, client, actor, e),
    }
}

/// `place_on_market(user)`.
pub fn place_on_market(world: &mut World, _client: Entity, user: Entity, e: Entity) {
    let Some(item) = occupant(world, e) else {
        return;
    };
    let Some(pad) = world.get::<Ltsrbt>(e).cloned() else {
        return;
    };
    let local = world
        .get_resource::<BlackMarket>()
        .map_or(0, BlackMarket::local_goods);
    if local >= MAX_LOCAL_GOODS {
        say(world, e, "Local market saturated, buy some goods first!");
        sound(world, e, BUZZ, 40.0);
        return;
    }
    let Some(account) = account(world, user) else {
        say(world, e, "No bank account to charge market fees detected!");
        sound(world, e, BUZZ, 40.0);
        return;
    };
    let paid = world
        .get_resource_mut::<Economy>()
        .is_some_and(|mut economy| {
            economy
                .account_mut(account)
                .is_some_and(|a| a.adjust_money(-PLACE_COST))
        });
    if !paid {
        say(world, e, "Insufficient credits!");
        sound(world, e, BUZZ, 40.0);
        return;
    }
    let holder = world
        .get_resource::<Economy>()
        .and_then(|e| e.account(account))
        .map(|a| a.holder.clone())
        .unwrap_or_default();
    let item_name = world
        .get::<DmName>(item)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    let name = if pad.current_name.is_empty() {
        item_name
    } else {
        pad.current_name.clone()
    };
    let separator = if pad.current_desc.is_empty() {
        ""
    } else {
        " - "
    };
    let desc = format!("{}{separator}Seller: {holder}", pad.current_desc);
    // `moveToNullspace`: off the map and unseen until it is bought.
    world
        .entity_mut(item)
        .remove::<(StoredIn, TilePos, CarriedBy, ss13_net::server::Networked)>();
    let mut market = world.resource_mut::<BlackMarket>();
    market.next_local += 1;
    let id = format!("local-{}", market.next_local);
    market.add(Entry {
        id,
        def: None,
        name,
        desc,
        category: LOCAL_GOODS.to_owned(),
        price: pad.current_price,
        stock: 1,
        availability_prob: 100.0,
        restockable: false,
        shipping_override: Vec::new(),
        good: Good::Instance(item),
        seller: Some(account),
    });
    let t = now(world);
    if let Some(mut p) = world.get_mut::<Ltsrbt>(e) {
        p.occupant = None;
        p.current_name.clear();
        p.current_desc.clear();
        p.current_price = CARGO_CRATE_VALUE as i64;
        p.ready_at = t + p.recharge_time * 3.0;
    }
    say(world, e, "Item placed on the market!");
    sound(world, e, CASH_REGISTER, 40.0);
}

/// `examine` of a pad: the restock and placement prices.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<Ltsrbt>(e).is_none() || world.get::<Unpowered>(e).is_some() {
        return;
    }
    let cost = world
        .get_resource::<BlackMarket>()
        .map_or(RESTOCK_COST, |m| m.restock_cost);
    lines.push((Span::Info, "На маленьком дисплее написано:".to_owned()));
    lines.push((
        Span::Notice,
        format!("Цена пополнения рынка: {cost}{}.", super::MONEY_SYMBOL),
    ));
    lines.push((
        Span::Notice,
        format!("Сбор за размещение: {PLACE_COST}{}.", super::MONEY_SYMBOL),
    ));
    lines.push((
        Span::Notice,
        format!(
            "Удерживаемый налог с местных товаров: {}%.",
            (WITHHOLDING_TAX * 100.0).round()
        ),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, category: &str, prob: f64) -> MarketItemDef {
        MarketItemDef {
            id: id.into(),
            name: id.into(),
            category: category.into(),
            item: "/obj/item/toy".into(),
            price_min: 100.0,
            price_max: 200.0,
            stock_min: 1.0,
            stock_max: 3.0,
            availability_prob: prob,
            markets: vec![BLACKMARKET.into()],
            restockable: true,
            ..Default::default()
        }
    }

    #[test]
    fn stock_rolls_within_ranges_and_categories_follow() {
        let market = BlackMarket::new(
            vec![
                def("/datum/market_item/a", "Tools", 100.0),
                def("/datum/market_item/b", "Weapons", 100.0),
                def("/datum/market_item/never", "Never", 0.0),
            ],
            7,
        );
        assert_eq!(
            market.categories,
            vec!["Tools".to_owned(), "Weapons".into()]
        );
        for e in &market.entries {
            assert!((100..=200).contains(&e.price), "{}", e.price);
            assert!((1..=3).contains(&e.stock), "{}", e.stock);
        }
        let entry = market.entry("Tools", "/datum/market_item/a").unwrap();
        assert_eq!(BlackMarket::fee(entry, LTSRBT_METHOD), Some(40));
        assert_eq!(BlackMarket::fee(entry, SUPPLY_POD), None);
    }

    #[test]
    fn restock_tops_up_and_rolls_the_missing_again() {
        let mut market = BlackMarket::new(
            vec![
                def("/datum/market_item/a", "Tools", 100.0),
                def("/datum/market_item/b", "Weapons", 100.0),
            ],
            3,
        );
        market.entries[0].stock = 0;
        market.remove("/datum/market_item/b");
        market.restock();
        let a = market.entry("Tools", "/datum/market_item/a").unwrap();
        assert!((1..=3).contains(&a.stock), "{}", a.stock);
        assert!(market.entry("Weapons", "/datum/market_item/b").is_some());
        assert_eq!(market.categories.len(), 2);
    }

    #[test]
    fn removing_the_last_entry_drops_its_category() {
        let mut market = BlackMarket::new(vec![def("/datum/market_item/a", "Tools", 100.0)], 1);
        market.remove("/datum/market_item/a");
        assert!(market.categories.is_empty());
    }
}
