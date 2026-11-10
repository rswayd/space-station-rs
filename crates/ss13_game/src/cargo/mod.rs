//! Cargo and the economy basics, after tg: `SSeconomy` and `/datum/bank_account`
//! ([`economy`]), `SSshuttle.supply` with `/obj/docking_port/mobile/supply`, `/datum/supply_pack`
//! and `/datum/supply_order` ([`supply`]), `/datum/export` selling ([`export`]) and the supply
//! consoles `/obj/machinery/computer/cargo` and `.../request` with tgui's `Cargo` interface
//! ([`console`] on the server, [`client`] draws it). This module holds the window's data, which
//! both halves share.
//!
//! The pack and export data come from `ss13-mapload convert` (`<assets>/cargo.ron`,
//! [`ss13_mapload::cargo`]); rerun it after updating.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::vending::ProductIcon;

#[cfg(feature = "server")]
pub mod black_market;
#[cfg(feature = "client")]
pub mod black_market_client;
#[cfg(feature = "server")]
pub mod bounty;
#[cfg(feature = "client")]
pub mod bounty_client;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod console;
#[cfg(feature = "server")]
pub mod contraband;
#[cfg(feature = "server")]
pub mod coupon;
#[cfg(feature = "server")]
pub mod economy;
#[cfg(feature = "server")]
pub mod export;
#[cfg(feature = "server")]
pub mod express;
#[cfg(feature = "client")]
pub mod express_client;
#[cfg(feature = "server")]
pub mod mail;
#[cfg(feature = "server")]
pub mod mail_sorter;
pub mod market_ui;
#[cfg(feature = "server")]
pub mod materials_market;
#[cfg(feature = "client")]
pub mod materials_market_client;
#[cfg(feature = "server")]
pub mod money;
#[cfg(feature = "server")]
pub mod scanner;
#[cfg(feature = "server")]
pub mod stock_market;
#[cfg(feature = "server")]
pub mod supply;
#[cfg(feature = "server")]
pub mod supplypod;

/// Cargo on the server: the economy, the supply shuttle, the consoles.
#[cfg(feature = "server")]
pub struct CargoServerPlugin;

#[cfg(feature = "server")]
impl bevy::prelude::Plugin for CargoServerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `interact_with_atom` of the universal scanner: an export scan.
        app.add_click_handler(ClickPhase::HeldOnTarget, 5000, |w, c| {
            c.held
                .is_some_and(|h| crate::cargo::scanner::item_interaction(w, c.client, h, c.target))
        });
        app.add_plugins((
            economy::EconomyPlugin,
            stock_market::StockMarketPlugin,
            supply::SupplyPlugin,
            console::ConsolePlugin,
            bounty::BountyPlugin,
            mail::MailPlugin,
            mail_sorter::MailSorterPlugin,
            coupon::CouponPlugin,
            supplypod::SupplyPodPlugin,
            express::ExpressPlugin,
            materials_market::MaterialsMarketPlugin,
            money::MoneyPlugin,
            black_market::BlackMarketPlugin,
        ));
    }
}

/// Examine lines of the cargo machines and items (from `crate::examine`).
#[cfg(feature = "server")]
pub fn examine_lines(
    world: &bevy::prelude::World,
    target: bevy::prelude::Entity,
    lines: &mut Vec<(ss13_shared::chat::Span, String)>,
) {
    mail_sorter::examine_lines(world, target, lines);
    materials_market::examine_lines(world, target, lines);
    money::examine_lines(world, target, lines);
    black_market::examine_lines(world, target, lines);
}

/// tg `MONEY_SYMBOL` / `MONEY_NAME`.
pub const MONEY_SYMBOL: &str = "¢";

/// tgui `Cargo`.
pub struct CargoUi;

impl UiInterface for CargoUi {
    const NAME: &'static str = "Cargo";
    type Data = CargoData;
    type StaticData = CargoStatic;
}

/// A pack's item, for the contents modal (`get_contents_ui_data`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SupplyItem {
    pub name: String,
    pub icon: Option<ProductIcon>,
    pub amount: u32,
}

/// A catalogue entry (`/datum/asset/json/cargo_catalogue` pack data with its group price).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SupplyRecord {
    /// The pack's type path: the `add` act's `id`.
    pub id: String,
    pub name: String,
    pub desc: String,
    /// `get_cost() * get_discount()`.
    pub cost: u32,
    /// The crate needs access to open.
    pub access: bool,
    pub goody: bool,
    pub contraband: bool,
    /// The first item's icon.
    pub icon: Option<ProductIcon>,
    pub contains: Vec<SupplyItem>,
}

/// A catalogue group.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SupplyGroup {
    pub name: String,
    pub packs: Vec<SupplyRecord>,
}

/// `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CargoStatic {
    /// `CARGO_MAX_ORDER`.
    pub max_order: u32,
    /// Sorted by name, packs sorted by name (the JSX sorts both).
    pub groups: Vec<SupplyGroup>,
    /// With tg's leading space: `" ¢"`.
    pub displayed_currency_name: String,
}

/// `cart` entry: one per pack name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CartEntry {
    pub object: String,
    pub cost: u32,
    pub id: u32,
    pub amount: u32,
    pub orderer: String,
    /// Orders of it bought privately.
    pub paid: u32,
    pub can_be_cancelled: bool,
}

/// `requests` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestEntry {
    pub object: String,
    pub cost: u32,
    pub orderer: String,
    pub reason: String,
    pub id: u32,
    pub account: String,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoData {
    pub department: String,
    /// `SSshuttle.supply.getStatusText()`.
    pub location: String,
    /// The cargo budget.
    pub points: i64,
    /// Docked at CentCom (`cargo_away`).
    pub away: bool,
    /// The shuttle is idle (`SHUTTLE_IDLE`).
    pub docked: bool,
    pub self_paid: bool,
    pub can_send: bool,
    pub can_approve_requests: bool,
    pub requestonly: bool,
    pub message: String,
    pub cart: Vec<CartEntry>,
    pub requests: Vec<RequestEntry>,
}

impl CargoData {
    /// `CargoCartButtons`: items in the cart and their total cost.
    pub fn cart_totals(&self) -> (u32, u32) {
        self.cart
            .iter()
            .fold((0, 0), |(n, c), e| (n + e.amount, c + e.cost))
    }
}

pub const SEND: &str = "send";
pub const ADD: &str = "add";
pub const ADD_BY_NAME: &str = "add_by_name";
pub const REMOVE: &str = "remove";
pub const MODIFY: &str = "modify";
pub const CLEAR: &str = "clear";
pub const APPROVE: &str = "approve";
pub const DENY: &str = "deny";
pub const DENY_ALL: &str = "denyall";
pub const TOGGLE_PRIVATE: &str = "toggleprivate";
/// Params.
pub const ID: &str = "id";
pub const ORDER_NAME: &str = "order_name";
pub const AMOUNT: &str = "amount";
/// The request console's reason (tg asks it with `tgui_input_text` before adding).
pub const REASON: &str = "reason";
/// The request console's budget: present and not `"0"`, the requester's department pays (tg asks
/// "Which department are you requesting this for?" with `tgui_alert`).
pub const DEPARTMENT: &str = "department";

/// tgui `CargoExpress`: the express supply console (`express` on the server, `express_client`
/// draws it). The catalogue is [`CargoStatic`].
pub struct CargoExpressUi;

impl UiInterface for CargoExpressUi {
    const NAME: &'static str = "CargoExpress";
    type Data = CargoExpressData;
    type StaticData = CargoStatic;
}

/// `/obj/machinery/computer/cargo/express/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoExpressData {
    pub locked: bool,
    pub points: i64,
    pub using_beacon: bool,
    /// The beacon's area name.
    pub beaconzone: String,
    pub beacon_name: String,
    pub beacon_error: bool,
    pub can_buy_beacon: bool,
    pub has_beacon: bool,
    pub can_beacon: bool,
    pub print_msg: String,
    pub message: String,
}

pub const LZ_CARGO: &str = "LZCargo";
pub const LZ_BEACON: &str = "LZBeacon";
pub const PRINT_BEACON: &str = "printBeacon";

/// tgui `MatMarket`: the materials market (`materials_market` on the server,
/// `materials_market_client` draws it).
pub struct MatMarketUi;

impl UiInterface for MatMarketUi {
    const NAME: &'static str = "MatMarket";
    type Data = MatMarketData;
    type StaticData = ();
}

/// A `materials` entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MarketMaterial {
    /// The `buy` act's `material`.
    pub name: String,
    pub price: f64,
    /// `value_per_unit`: the list is sorted by it.
    pub rarity: f64,
    pub min_threshold: f64,
    pub max_threshold: f64,
    pub quantity: f64,
    /// `trend`: "up", "down" or "neutral".
    pub trend: String,
    pub color: [u8; 3],
    /// Sheets of it in the user's current order.
    pub requested: u32,
    /// `k_elasticity * 100`.
    pub elastic: f64,
}

/// `/obj/machinery/materials_market/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MatMarketData {
    pub ordering_private: bool,
    pub can_order_cargo: bool,
    pub credit_balance: i64,
    pub order_balance: i64,
    /// Sorted by rarity.
    pub materials: Vec<MarketMaterial>,
    pub catastrophe: bool,
    /// Deciseconds until the next market tick (`updateTime`).
    pub update_time: f64,
}

pub const MARKET_BUY: &str = "buy";
pub const MARKET_TOGGLE_BUDGET: &str = "toggle_budget";
pub const MARKET_CLEAR: &str = "clear";
/// `buy`'s params.
pub const MARKET_MATERIAL: &str = "material";
pub const MARKET_QUANTITY: &str = "quantity";

/// tgui `CivCargoHoldTerminal`: the civilian bounty console (`bounty` on the server,
/// `bounty_client` draws it).
pub struct CivBountyUi;

impl UiInterface for CivBountyUi {
    const NAME: &'static str = "CivCargoHoldTerminal";
    type Data = CivBountyData;
    type StaticData = ();
}

/// A `listBounty` entry: a station-wide bounty.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SharedBounty {
    pub name: String,
    pub description: String,
    pub reward: f64,
    pub shipped: u32,
    pub maximum: u32,
    pub claimed: bool,
    pub priority: bool,
    pub unique: bool,
}

/// A bounty to pick (`id_bounty_names`, `id_bounty_infos`, `id_bounty_values`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BountyChoice {
    pub name: String,
    pub info: String,
    /// The holder's cut.
    pub value: f64,
}

/// `/obj/machinery/computer/piratepad_control/civilian/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CivBountyData {
    pub pad: bool,
    pub sending: bool,
    pub status_report: String,
    pub id_inserted: bool,
    /// The inserted account's bounty: description, `print_required()`, the holder's cut.
    pub id_bounty_info: Option<String>,
    pub id_bounty_num: String,
    pub id_bounty_value: f64,
    pub picking: bool,
    pub choices: Vec<BountyChoice>,
    pub list_bounty: Vec<SharedBounty>,
    pub claimed_bounties: u32,
}

pub const RECALC: &str = "recalc";
pub const STOP: &str = "stop";
pub const PICK: &str = "pick";
pub const BOUNTY: &str = "bounty";
pub const EJECT: &str = "eject";
pub const UPDATE_LIST: &str = "update_list";
pub const PRINT: &str = "print";
/// `send`'s param: the station bounties instead of the personal one.
pub const GLOBAL: &str = "global";
/// `pick`'s param: 1-based.
pub const VALUE: &str = "value";
