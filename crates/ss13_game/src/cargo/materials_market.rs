//! The galactic materials market and stock blocks (server), after tg
//! `/obj/machinery/materials_market` and `/obj/item/stock_block`
//! (`code/modules/cargo/materials_market.dm`), `/datum/supply_pack/custom/minerals` and
//! `/datum/supply_order/disposable/materials`, with tgui's `MatMarket`.
//!
//! - Sheets of one tradable material ([`super::stock_market::MATERIALS`]) used on a powered
//!   market become a stock block on its tile, worth the sheets at the price of that moment.
//!   After 90 s the block warns, after 180 s it is liquid: worth the live price. Sold on the
//!   supply shuttle it goes through the material's market export (elasticity applies) and moves
//!   the market as sold sheets do.
//! - The window buys sheets for the next supply shuttle: from the user's account, or with cargo
//!   access from the cargo budget (`toggle_budget`). One order per buyer (or one for the
//!   budget) gathers the purchases; it costs the sheets plus `CARGO_CRATE_VALUE`, at most 10
//!   full stacks of a sheet, never more than the market holds or the payer can pay. `clear`
//!   drops the order. Shipping (`adjust_market`) takes the sheets off the market (cutting an
//!   order to what is left) and raises the price.
// shortcut: each new order adds its own invisible pack to `SupplyPacks::packs` (a few per round);
// no wrench anchoring, light or icon states of the machine; the stock block's icon follows its
// state only when the assets have `stock_block_fluid`/`_liquid`.

use std::collections::HashMap;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::cargo::{MATERIALS_MARKET, STOCK_BLOCK, SupplyPackDef, order_flags};
use ss13_mapload::{DmItem, DmName, DmObj, IconIndex};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::console::machine_say;
use super::economy::{ACCOUNT_CAR, Economy};
use super::export::Exports;
use super::stock_market::{MATERIALS, StockMarket};
use super::supply::{CARGO_CRATE_VALUE, SupplyOrder, SupplyOrders, SupplyPacks};
use super::*;
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::tools::tell;

/// The custom pack of a market order (`/datum/supply_pack/custom/minerals`).
pub const MATERIALS_ORDER: &str = "/datum/supply_pack/custom/minerals";
/// `GALATIC_MATERIAL_ORDER`: the orderer rank marking market orders.
pub const GALACTIC_MATERIAL_ORDER: &str = "Galactic Materials Market";
/// The order's crate (`custom/minerals` `crate_type`).
pub const MINING_CRATE: &str = "/obj/structure/closet/crate/cargo/mining";
/// `MAX_STACK_LIMIT`, `MAX_STACK_SIZE`.
const MAX_STACK_LIMIT: u32 = 10;
pub const MAX_STACK_SIZE: u32 = 50;
/// `value_warning` and `become_liquid` timers.
const WARNING_SECS: f64 = 90.0;
const LIQUID_SECS: f64 = 180.0;
const ACCESS_CARGO: &str = "cargo";
const ECONOMY_DMI: &str = "icons/obj/economy.dmi";

/// `sheet_type` and the color (`greyscale_color || color`) of each of [`MATERIALS`].
pub const SHEETS: [(&str, [u8; 3]); 8] = [
    ("/obj/item/stack/sheet/iron", [0xB6, 0xBE, 0xC2]),
    ("/obj/item/stack/sheet/glass", [0x62, 0x92, 0xAF]),
    ("/obj/item/stack/sheet/mineral/silver", [0xB5, 0xBC, 0xBB]),
    ("/obj/item/stack/sheet/mineral/gold", [0xE6, 0xBB, 0x45]),
    ("/obj/item/stack/sheet/mineral/diamond", [0xC9, 0xD8, 0xF2]),
    ("/obj/item/stack/sheet/mineral/uranium", [0x2C, 0x99, 0x2C]),
    (
        "/obj/item/stack/sheet/bluespace_crystal",
        [0x2E, 0x50, 0xB7],
    ),
    ("/obj/item/stack/sheet/mineral/titanium", [0xEF, 0xEF, 0xEF]),
];

/// A market machine's `ordering_private`, and each viewer's `ui_data` (computed with the world,
/// [`views`]).
#[derive(Component, Clone, Debug, Default)]
pub struct MaterialsMarket {
    pub ordering_private: bool,
    views: HashMap<Entity, MatMarketData>,
}

/// A stock block: `sheets` of `MATERIALS[material]` worth `export_value` each.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct StockBlock {
    pub material: usize,
    pub sheets: u32,
    pub export_value: f64,
    pub created: f64,
    pub warned: bool,
    pub fluid: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MarketAct {
    Buy(String, u32),
    ToggleBudget,
    Clear,
}

pub struct MaterialsMarketPlugin;

impl Plugin for MaterialsMarketPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 3461, |w, c| {
            crate::cargo::materials_market::interact(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2985, |w, c| {
            c.held.is_some_and(|h| {
                crate::cargo::materials_market::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.register_ui::<MatMarketUi>()
            .add_systems(Startup, setup_mapped)
            .add_systems(
                Update,
                (
                    stock_blocks.run_if(any_with_component::<StockBlock>),
                    acts.in_set(UiSystems::Handle),
                    views
                        .after(UiSystems::Handle)
                        .before(UiSystems::Send)
                        .run_if(any_with_component::<MaterialsMarket>),
                ),
            );
    }
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query::<(Entity, &DmObj)>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, MATERIALS_MARKET))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// Map load and `crate::machine_frame::setup_built`: `ordering_private = TRUE`.
pub fn setup(world: &mut World, e: Entity) {
    world.entity_mut(e).insert_if_new(MaterialsMarket {
        ordering_private: true,
        views: HashMap::new(),
    });
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| n.name.trim_start_matches("\\improper ").to_owned())
        .unwrap_or_default()
}

/// The material of a sheet stack (`custom_materials` of exactly one tradable material).
pub fn sheet_material(path: &str) -> Option<usize> {
    SHEETS.iter().position(|(sheet, _)| is_under(path, sheet))
}

/// The material named `name` (`initial(mat.name)`).
fn material_named(name: &str) -> Option<usize> {
    MATERIALS.iter().position(|m| m.name == name)
}

/// tg `item_interaction`: a sheet stack becomes a stock block.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    if world.get::<MaterialsMarket>(e).is_none() {
        return false;
    }
    let Some(material) = world
        .get::<DmItem>(held)
        .and_then(|i| sheet_material(&i.path))
    else {
        return false;
    };
    if world.get::<Unpowered>(e).is_some() {
        tell(world, client, Span::Warning, "no power!");
        return true;
    }
    let price = world
        .get_resource::<StockMarket>()
        .map_or(0.0, |m| m.price(material));
    if price <= 0.0 {
        tell(
            world,
            client,
            Span::Warning,
            "materials in stack are worthless",
        );
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return true;
    };
    let sheets = crate::tools::stack_amount(world, held);
    world.entity_mut(held).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
    {
        *slot = None;
    }
    world.despawn(held);
    let Some(block) = crate::construction::spawn_obj(world, STOCK_BLOCK, pos) else {
        warn!("no {STOCK_BLOCK} prototype (rerun `ss13-mapload convert`)");
        return true;
    };
    world.entity_mut(block).insert(StockBlock {
        material,
        sheets,
        export_value: price,
        created: now(world),
        warned: false,
        fluid: false,
    });
    let worth = (price * f64::from(sheets)).round() as i64;
    let text = format!(
        "You have created a stock block worth {worth}{MONEY_SYMBOL}! Sell it before it becomes \
         liquid!"
    );
    tell(world, client, Span::Notice, text);
    true
}

/// The tile a stock block is seen from: its own, or its holder's.
fn seen_at(world: &World, e: Entity) -> Option<TilePos> {
    world.get::<TilePos>(e).copied().or_else(|| {
        let holder = world.get::<CarriedBy>(e)?.0;
        world.get::<TilePos>(holder).copied()
    })
}

fn set_block_icon(world: &mut World, e: Entity, state: &str) {
    let id = world.get_resource::<IconIndex>().and_then(|i| {
        i.0.get(&(ECONOMY_DMI.to_owned(), state.to_owned()))
            .copied()
    });
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(e)
    {
        look.icon_state = id;
    }
}

fn block_message(world: &mut World, e: Entity, text: String) {
    let Some(pos) = seen_at(world, e) else {
        return;
    };
    let chat =
        crate::pull::visible_message(world, pos, ChatChannel::Local, (Span::Warning, text), &[]);
    for message in chat {
        world.write_message(message);
    }
}

/// `value_warning` and `become_liquid`.
// Cost: a pass over the stock blocks (a handful) per tick.
fn stock_blocks(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &StockBlock)>();
    let due: Vec<(Entity, StockBlock)> = q
        .iter(world)
        .filter(|(_, b)| {
            (!b.warned && t - b.created >= WARNING_SECS)
                || (!b.fluid && t - b.created >= LIQUID_SECS)
        })
        .map(|(e, b)| (e, *b))
        .collect();
    for (e, block) in due {
        let name = name_of(world, e);
        if !block.warned {
            if let Some(mut b) = world.get_mut::<StockBlock>(e) {
                b.warned = true;
            }
            set_block_icon(world, e, "stock_block_fluid");
            block_message(
                world,
                e,
                format!("The {name} is starting to become liquid!"),
            );
        }
        if t - block.created >= LIQUID_SECS {
            if let Some(mut b) = world.get_mut::<StockBlock>(e) {
                b.fluid = true;
            }
            set_block_icon(world, e, "stock_block_liquid");
            block_message(world, e, format!("The {name} becomes liquid!"));
        }
    }
}

/// A block's price per sheet now (`update_value` when fluid).
pub fn unit_value(world: &World, block: &StockBlock) -> f64 {
    if block.fluid {
        world
            .get_resource::<StockMarket>()
            .map_or(block.export_value, |m| m.price(block.material))
    } else {
        block.export_value
    }
}

/// From `crate::cargo::export::export_item_and_contents`: a stock block sold through its
/// material's market export, which then moves the market. True when `e` was a block.
pub(crate) fn sell_block(
    world: &mut World,
    exports: &mut Exports,
    e: Entity,
    now: f64,
    report: &mut super::export::ExportReport,
) -> bool {
    let Some(block) = world.get::<StockBlock>(e).copied() else {
        return false;
    };
    let unit = unit_value(world, &block);
    let name = name_of(world, e);
    let Some(index) = exports.market_export(block.material) else {
        return true;
    };
    let sheets = f64::from(block.sheets);
    if exports.sell_block(index, sheets, unit, &name, now, report)
        && let Some(mut market) = world.get_resource_mut::<StockMarket>()
    {
        market.sold(block.material, sheets);
        exports.sync_market(&market);
    }
    true
}

/// A stock block's dry-run value (the export scanner), `None` when `e` is not one.
pub(crate) fn block_value(world: &World, exports: &Exports, e: Entity, now: f64) -> Option<i64> {
    let block = world.get::<StockBlock>(e)?;
    let index = exports.market_export(block.material)?;
    Some(exports.block_cost(
        index,
        f64::from(block.sheets),
        unit_value(world, block),
        now,
    ))
}

/// tg `stock_block/examine` and nothing for the machine.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(block) = world.get::<StockBlock>(e) else {
        return;
    };
    let name = name_of(world, e);
    if block.fluid {
        lines.push((
            Span::Warning,
            format!("The {name} is currently liquid! Its value is based on the market price."),
        ));
    } else {
        lines.push((
            Span::Notice,
            format!(
                "The {name}'s value is still locked in. Sell it before its value becomes liquid!"
            ),
        ));
    }
    let worth = (f64::from(block.sheets) * unit_value(world, block)).round() as i64;
    lines.push((
        Span::Notice,
        format!(
            "The {name} is worth {worth} {MONEY_SYMBOL}, from selling {} sheets of {}.",
            block.sheets, MATERIALS[block.material].name
        ),
    ));
}

/// An empty hand opens the window.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    if world.get::<MaterialsMarket>(e).is_none() {
        return false;
    }
    let title = name_of(world, e);
    world.write_message(OpenUi::new::<MatMarketUi>(client, mob, e, title));
    true
}

/// The user's ID: cargo access and the bank account.
struct UserId {
    cargo: bool,
    account: Option<u32>,
    name: String,
}

fn user_id(world: &World, mob: Entity) -> Option<UserId> {
    let card = world.get::<Inventory>(mob)?.id_card(world)?;
    Some(UserId {
        cargo: world
            .get::<IdCard>(card)
            .is_some_and(|c| c.access.iter().any(|a| a == ACCESS_CARGO)),
        account: super::economy::account_of(world, card),
        name: world
            .get::<Name>(mob)
            .map_or_else(|| name_of(world, mob), |n| n.as_str().to_owned()),
    })
}

/// `find_order`: the market order of the budget (`!private`) or of the buyer's account.
fn find_order(orders: &SupplyOrders, packs: &SupplyPacks, private: Option<u32>) -> Option<usize> {
    orders.shopping_list.iter().position(|o| {
        o.orderer_rank == GALACTIC_MATERIAL_ORDER
            && packs
                .packs
                .get(o.pack)
                .is_some_and(|p| p.id == MATERIALS_ORDER)
            && match private {
                None => o.paying_account.is_none(),
                Some(account) => o.paying_account == Some(account),
            }
    })
}

/// Who pays: the account when ordering privately, else `None` (the cargo budget). `Err` when
/// a private buyer has no account.
fn payer(market: &MaterialsMarket, id: &UserId) -> Result<Option<u32>, ()> {
    if market.ordering_private || !id.cargo {
        id.account.map(Some).ok_or(())
    } else {
        Ok(None)
    }
}

fn balance(world: &World, payer: Option<u32>) -> i64 {
    let Some(economy) = world.get_resource::<Economy>() else {
        return 0;
    };
    match payer {
        Some(id) => economy.account(id).map_or(0, |a| a.balance),
        None => economy.department(ACCOUNT_CAR).map_or(0, |a| a.balance),
    }
}

/// `ui_data` for `user`.
fn market_data(world: &World, market: &MaterialsMarket, user: Entity) -> MatMarketData {
    let id = user_id(world, user);
    let can_order_cargo = id.as_ref().is_some_and(|i| i.cargo);
    let private = market.ordering_private || !can_order_cargo;
    let account = id.as_ref().and_then(|i| i.account);
    let order = match (
        world.get_resource::<SupplyOrders>(),
        world.get_resource::<SupplyPacks>(),
    ) {
        (Some(orders), Some(packs)) => {
            let key = if private {
                account.or(Some(u32::MAX))
            } else {
                None
            };
            find_order(orders, packs, key).map(|i| {
                let o = &orders.shopping_list[i];
                (packs.packs[o.pack].contains.clone(), o.final_cost(packs))
            })
        }
        _ => None,
    };
    let t = now(world);
    let stock = world.get_resource::<StockMarket>();
    let exports = world.get_resource::<Exports>();
    let mut materials: Vec<MarketMaterial> = MATERIALS
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let entry = stock.map(|s| s.entries[i]);
            let elastic = exports
                .and_then(|e| Some(e.elasticity(e.market_export(i)?, t) * 100.0))
                .unwrap_or(100.0);
            MarketMaterial {
                name: m.name.to_owned(),
                price: entry.map_or(m.sheet_value, |e| e.price),
                rarity: m.sheet_value,
                min_threshold: m.price_minimum(),
                max_threshold: m.price_maximum(),
                quantity: entry.map_or(0.0, |e| e.quantity),
                trend: match entry.map_or(0, |e| e.trend) {
                    0 => "neutral",
                    1 => "up",
                    _ => "down",
                }
                .to_owned(),
                color: SHEETS[i].1,
                requested: order.as_ref().map_or(0, |(contains, _)| {
                    contains
                        .iter()
                        .filter(|(p, _)| p == SHEETS[i].0)
                        .map(|(_, n)| *n)
                        .sum()
                }),
                elastic,
            }
        })
        .collect();
    materials.sort_by(|a, b| a.rarity.total_cmp(&b.rarity));
    // tg shows the budget's balance only when not ordering privately (the flag, not the access).
    let credit_balance = if market.ordering_private {
        account.map_or(0, |a| balance(world, Some(a)))
    } else {
        balance(world, None)
    };
    MatMarketData {
        ordering_private: market.ordering_private,
        can_order_cargo,
        credit_balance,
        order_balance: order.map_or(0, |(_, cost)| cost),
        materials,
        catastrophe: stock.is_some_and(|s| s.crashing),
        update_time: stock.map_or(0.0, |s| s.next_fire_in(t) * 10.0),
    }
}

/// Each open window's data, with the world (the user's ID and account, the orders).
// Cost: one `market_data` per open market window per tick (a few at most).
fn views(world: &mut World) {
    let mut windows = world.query::<&ss13_tgui::server::UiWindow>();
    let open: Vec<(Entity, Entity)> = windows
        .iter(world)
        .filter(|w| w.interface == <MatMarketUi as ss13_tgui::UiInterface>::NAME)
        .map(|w| (w.source, w.actor))
        .collect();
    let mut by_market: HashMap<Entity, HashMap<Entity, MatMarketData>> = HashMap::new();
    for (source, actor) in open {
        let Some(market) = world.get::<MaterialsMarket>(source) else {
            continue;
        };
        let data = market_data(world, market, actor);
        by_market.entry(source).or_default().insert(actor, data);
    }
    let mut q = world.query::<(Entity, &mut MaterialsMarket)>();
    for (e, mut market) in q.iter_mut(world) {
        let views = by_market.remove(&e).unwrap_or_default();
        if market.views != views {
            market.views = views;
        }
    }
}

impl ServerUi for MatMarketUi {
    type Source = &'static MaterialsMarket;
    type UserParam = ();
    type Act = MarketAct;

    fn static_data(_: &MaterialsMarket) {}

    fn data(market: &MaterialsMarket, user: Entity, _: &()) -> MatMarketData {
        market.views.get(&user).cloned().unwrap_or_default()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<MarketAct, UiReject> {
        match action {
            MARKET_BUY => {
                let material = params
                    .get(MARKET_MATERIAL)
                    .ok_or_else(|| UiReject::BadParams("material: missing".into()))?;
                let quantity = params.number(MARKET_QUANTITY)?;
                if !(1.0..=f64::from(MAX_STACK_SIZE)).contains(&quantity) {
                    return Err(UiReject::BadParams("quantity: 1..=50".into()));
                }
                Ok(MarketAct::Buy(material.to_owned(), quantity as u32))
            }
            MARKET_TOGGLE_BUDGET => Ok(MarketAct::ToggleBudget),
            MARKET_CLEAR => Ok(MarketAct::Clear),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `ui_act`, every check at the moment of effect.
fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<MatMarketUi>>>) {
    let pending: Vec<(Entity, Entity, MarketAct)> = cursor
        .read(world.resource::<Messages<UiActed<MatMarketUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, e, act) in pending {
        if world.get::<Unpowered>(e).is_some() || !world.contains_resource::<SupplyOrders>() {
            continue;
        }
        let Some(market) = world.get::<MaterialsMarket>(e).cloned() else {
            continue;
        };
        let Some(id) = user_id(world, actor) else {
            machine_say(world, e, "No ID Found");
            continue;
        };
        match act {
            MarketAct::Buy(name, quantity) => buy(world, e, &market, &id, &name, quantity),
            MarketAct::ToggleBudget if id.cargo => {
                if let Some(mut m) = world.get_mut::<MaterialsMarket>(e) {
                    m.ordering_private = !m.ordering_private;
                }
            }
            MarketAct::ToggleBudget => {}
            MarketAct::Clear => {
                let Ok(key) = payer(&market, &id) else {
                    continue;
                };
                let found = find_order(
                    world.resource::<SupplyOrders>(),
                    world.resource::<SupplyPacks>(),
                    key,
                );
                if let Some(i) = found {
                    world.resource_mut::<SupplyOrders>().shopping_list.remove(i);
                }
            }
        }
    }
}

/// `ui_act` "buy".
fn buy(
    world: &mut World,
    e: Entity,
    market: &MaterialsMarket,
    id: &UserId,
    name: &str,
    quantity: u32,
) {
    let Some(material) = material_named(name) else {
        return;
    };
    let sheet = SHEETS[material].0;
    let Ok(key) = payer(market, id) else {
        machine_say(world, e, "No bank account detected!");
        return;
    };
    let Some(stock) = world.get_resource::<StockMarket>() else {
        return;
    };
    let entry = stock.entries[material];
    if stock.crashing || entry.price < MATERIALS[material].price_minimum() {
        return;
    }
    if f64::from(quantity) > entry.quantity {
        machine_say(world, e, "Not enough materials on the market to purchase!");
        return;
    }
    let cost = entry.price * f64::from(quantity);
    let available = balance(world, key);
    let found = find_order(
        world.resource::<SupplyOrders>(),
        world.resource::<SupplyPacks>(),
        key,
    );
    if let Some(i) = found {
        let (pack, final_cost) = {
            let orders = world.resource::<SupplyOrders>();
            let packs = world.resource::<SupplyPacks>();
            let o = &orders.shopping_list[i];
            (o.pack, o.final_cost(packs))
        };
        let prior: u32 = world.resource::<SupplyPacks>().packs[pack]
            .contains
            .iter()
            .filter(|(p, _)| p == sheet)
            .map(|(_, n)| *n)
            .sum();
        if f64::from(prior + quantity) > entry.quantity {
            machine_say(
                world,
                e,
                "There aren't enough sheets on the market! Please wait for more sheets to be \
                 traded before adding more.",
            );
            return;
        }
        if prior.div_ceil(MAX_STACK_SIZE) >= MAX_STACK_LIMIT {
            machine_say(
                world,
                e,
                "There are already 10 stacks of sheets on order! Please wait for them to arrive \
                 before ordering more.",
            );
            return;
        }
        if final_cost as f64 + cost > available as f64 {
            machine_say(
                world,
                e,
                "Order exceeds available budget! Please send it before purchasing more.",
            );
            return;
        }
        // `append_order`.
        let mut packs = world.resource_mut::<SupplyPacks>();
        let def = &mut packs.packs[pack];
        match def.contains.iter_mut().find(|(p, _)| p == sheet) {
            Some((_, n)) => *n += quantity,
            None => def.contains.push((sheet.to_owned(), quantity)),
        }
        def.cost += cost;
        return;
    }
    let purchaser = if key.is_some() {
        id.name.clone()
    } else {
        "Cargo".to_owned()
    };
    let def = SupplyPackDef {
        id: MATERIALS_ORDER.to_owned(),
        name: format!("{purchaser}'s Materials Order"),
        group: String::new(),
        cost,
        contains: vec![(sheet.to_owned(), quantity)],
        crate_name: "galactic materials market delivery crate".to_owned(),
        crate_type: Some(MINING_CRATE.to_owned()),
        order_flags: order_flags::INVISIBLE,
        ..default()
    };
    let pack = {
        let mut packs = world.resource_mut::<SupplyPacks>();
        packs.packs.push(def);
        packs.packs.len() - 1
    };
    let order_id = world.resource_mut::<SupplyOrders>().next_id();
    let order = SupplyOrder {
        id: order_id,
        pack,
        orderer: id.name.clone(),
        orderer_rank: GALACTIC_MATERIAL_ORDER.to_owned(),
        reason: String::new(),
        paying_account: key,
        paying_department: None,
        can_be_cancelled: false,
        coupon: None,
    };
    if order.final_cost(world.resource::<SupplyPacks>()) > available {
        machine_say(world, e, "Not enough money to start purchase!");
        world.resource_mut::<SupplyPacks>().packs.pop();
        return;
    }
    machine_say(
        world,
        e,
        "Thank you for your purchase! It will arrive on the next cargo shuttle!",
    );
    world
        .resource_mut::<SupplyOrders>()
        .shopping_list
        .push(order);
}

/// `custom/minerals/adjust_market` before a market order ships (from `crate::cargo::supply::buy`):
/// each sheet cut to what the market holds, the price raised, the sheets taken. False when
/// nothing was left: the order is cancelled.
pub(crate) fn adjust_market(world: &mut World, order: &SupplyOrder) -> bool {
    let is_market = world
        .get_resource::<SupplyPacks>()
        .and_then(|p| p.packs.get(order.pack))
        .is_some_and(|p| p.id == MATERIALS_ORDER);
    if !is_market {
        return true;
    }
    let contains = world.resource::<SupplyPacks>().packs[order.pack]
        .contains
        .clone();
    let mut kept = Vec::new();
    let mut notes = Vec::new();
    for (sheet, wanted) in contains {
        let Some(material) = sheet_material(&sheet) else {
            continue;
        };
        let Some(mut stock) = world.get_resource_mut::<StockMarket>() else {
            kept.push((sheet, wanted));
            continue;
        };
        let market_quantity = stock.entries[material].quantity.max(0.0).floor() as u32;
        let mut available = wanted;
        let name = MATERIALS[material].name;
        if wanted > market_quantity {
            available = market_quantity;
            if available == 0 {
                notes.push(format!(
                    "{name}: order cancelled due to insufficient sheets in the market."
                ));
                continue;
            }
            notes.push(format!(
                "{name}: requested={wanted} sheets, available={market_quantity} sheets, \
                 adjusted={} sheets.",
                i64::from(market_quantity) - i64::from(wanted)
            ));
        }
        // Prices go up as the material becomes scarce.
        let mut fraction = f64::from(available);
        if market_quantity != available {
            fraction /= f64::from(market_quantity - available);
        }
        let price = stock.price(material);
        stock.adjust_price(material, price * fraction);
        stock.adjust_quantity(material, -f64::from(available));
        kept.push((sheet, available));
    }
    let empty = kept.is_empty();
    let pack_name = {
        let mut packs = world.resource_mut::<SupplyPacks>();
        packs.packs[order.pack].contains = kept;
        packs.packs[order.pack].name.clone()
    };
    if world.contains_resource::<StockMarket>() {
        world.resource_scope(|world, market: Mut<StockMarket>| {
            if let Some(mut exports) = world.get_resource_mut::<Exports>() {
                exports.sync_market(&market);
            }
        });
    }
    if !notes.is_empty()
        && let Some(account) = order.paying_account
    {
        let text = if empty {
            format!(
                "Order #{} ({pack_name}) was cancelled due to insufficient materials in the market!.",
                order.id
            )
        } else {
            format!(
                "Order #{} ({pack_name}) had the following orders adjusted\n{}\n.",
                order.id,
                notes.join("\n")
            )
        };
        super::economy::bank_card_talk(world, account, &text);
    }
    !empty
}

/// The stacks of `sheets` sheets a market crate holds (`fill`: `MAX_STACK_SIZE` each).
pub fn stack_sizes(sheets: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut left = sheets;
    while left > 0 {
        let n = left.min(MAX_STACK_SIZE);
        out.push(n);
        left -= n;
    }
    out
}

/// The final cost's crate charge of a market order (`disposable/materials/get_final_cost`).
pub fn crate_charge(pack: &SupplyPackDef) -> i64 {
    if pack.id == MATERIALS_ORDER {
        CARGO_CRATE_VALUE as i64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheets_map_to_materials() {
        assert_eq!(sheet_material("/obj/item/stack/sheet/iron/fifty"), Some(0));
        assert_eq!(
            sheet_material("/obj/item/stack/sheet/mineral/gold"),
            Some(3)
        );
        assert_eq!(sheet_material("/obj/item/stack/sheet/mineral/plasma"), None);
        assert_eq!(sheet_material("/obj/item/stack/sheet/rglass"), None);
        assert_eq!(material_named("bluespace crystal"), Some(6));
        assert_eq!(SHEETS.len(), MATERIALS.len());
    }

    #[test]
    fn crates_hold_full_stacks() {
        assert_eq!(stack_sizes(120), vec![50, 50, 20]);
        assert!(stack_sizes(0).is_empty());
    }

    #[test]
    fn market_orders_are_found_by_payer() {
        let mut packs = SupplyPacks::new(Vec::new());
        packs.packs.push(SupplyPackDef {
            id: MATERIALS_ORDER.into(),
            cost: 250.0,
            ..default()
        });
        let mut orders = SupplyOrders::new(1);
        orders.shopping_list.push(SupplyOrder {
            id: 1,
            pack: 0,
            orderer: "Bob".into(),
            orderer_rank: GALACTIC_MATERIAL_ORDER.into(),
            reason: String::new(),
            paying_account: Some(7),
            paying_department: None,
            can_be_cancelled: false,
            coupon: None,
        });
        assert_eq!(find_order(&orders, &packs, Some(7)), Some(0));
        assert_eq!(find_order(&orders, &packs, Some(8)), None);
        assert_eq!(find_order(&orders, &packs, None), None);
        assert_eq!(
            orders.shopping_list[0].final_cost(&packs),
            275 + 200,
            "1.1x privately, plus the crate"
        );
    }
}
