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

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod console;
#[cfg(feature = "server")]
pub mod economy;
#[cfg(feature = "server")]
pub mod export;
#[cfg(feature = "server")]
pub mod scanner;
#[cfg(feature = "server")]
pub mod stock_market;
#[cfg(feature = "server")]
pub mod supply;

/// Cargo on the server: the economy, the supply shuttle, the consoles.
#[cfg(feature = "server")]
pub struct CargoServerPlugin;

#[cfg(feature = "server")]
impl bevy::prelude::Plugin for CargoServerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((
            economy::EconomyPlugin,
            stock_market::StockMarketPlugin,
            supply::SupplyPlugin,
            console::ConsolePlugin,
        ));
    }
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
