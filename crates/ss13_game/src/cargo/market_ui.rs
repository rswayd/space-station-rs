//! The black market's windows: tgui `BlackMarketUplink` and `LTSRBT`. `black_market` serves
//! them, `black_market_client` draws them.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `BlackMarketUplink`.
pub struct BlackMarketUi;

impl UiInterface for BlackMarketUi {
    const NAME: &'static str = "BlackMarketUplink";
    type Data = UplinkData;
    type StaticData = ();
}

/// An `items` entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MarketItemView {
    /// The `select` act's `item`.
    pub id: String,
    pub name: String,
    pub cost: i64,
    pub amount: u32,
    pub desc: String,
}

/// `/obj/item/market_uplink/ui_data` with its static data (`markets`, `ltsrbt_built`,
/// `delivery_method_description`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UplinkData {
    pub categories: Vec<String>,
    /// (method, fee) of the selected item.
    pub delivery_methods: Vec<(String, i64)>,
    /// The ID's account balance; `None`: "N/A".
    pub money: Option<i64>,
    pub buying: bool,
    pub items: Vec<MarketItemView>,
    pub viewing_category: Option<String>,
    pub viewing_market: String,
    /// (id, name).
    pub markets: Vec<(String, String)>,
    /// (method, description).
    pub delivery_method_description: Vec<(String, String)>,
    pub ltsrbt_built: bool,
}

/// tgui `LTSRBT`.
pub struct LtsrbtUi;

impl UiInterface for LtsrbtUi {
    const NAME: &'static str = "LTSRBT";
    type Data = LtsrbtData;
    type StaticData = ();
}

/// `ui_data` with `ui_static_data` (`min_price`, `max_price`; the loaded item's name for its
/// icon).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LtsrbtData {
    pub name: String,
    pub price: i64,
    pub desc: String,
    pub min_price: i64,
    pub max_price: i64,
    pub loaded: String,
}
