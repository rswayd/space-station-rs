//! The materials stock market (server), after tg `SSstock_market`
//! (`code/controllers/subsystem/stock_market.dm`) and its events
//! (`code/datums/stock_market_events.dm`), with BandaStation's edits: a tick every 180 s instead
//! of 60, prices kept within `0.8x`-`2x` of the material's value instead of `0.5x`-`3x`.
//!
//! - Every tradable `/datum/material` ([`MATERIALS`]) has a price per sheet, a trend (up, stable,
//!   down) with a life of a few ticks and a quantity on the market. Each tick moves the price and
//!   the quantity along the trend; when the trend's life runs out a price below its baseline
//!   tends to turn up, one above it to turn down.
//! - Each material has an 8% chance per tick of a market event (reset, large boost, large drop,
//!   selling like hotcakes, lockdown), which forces the trend and moves the price.
//! - Selling sheets through a market export (`/datum/export/material/market/*`) lowers the price
//!   by `price * sheets / (sheets + quantity)` and adds the sheets to the quantity; the export's
//!   value follows the live price ([`super::export::Exports::sync_market`]).
//!
//! The eight tradable materials are listed here rather than converted: tg reads them from
//! `subtypesof(/datum/material)` with `tradable = TRUE` (`code/datums/materials/basemats.dm`).
//! - While [`StockMarket::crashing`] (tg `TRAIT_MARKET_CRASHING` on `SSeconomy`, set by the
//!   market crash event) every tick drops each price to its minimum and doubles the quantity.
// shortcut: the news string is only logged (no newscaster), and there is no materials market
// machine or stock blocks.

use bevy::prelude::*;
use ss13_mapload::MapSeed;
use ss13_mapload::spawner::Rng;

use super::export::Exports;
use crate::lobby::GameState;
use crate::round::Ticker;

/// `SSstock_market.wait` (BandaStation: 180 s).
pub const MARKET_SECS: f64 = 180.0;
/// `MARKET_EVENT_PROBABILITY`, percent per material per tick.
pub const MARKET_EVENT_PROBABILITY: f32 = 8.0;
pub const MARKET_TREND_UPWARD: i8 = 1;
pub const MARKET_TREND_STABLE: i8 = 0;
pub const MARKET_TREND_DOWNWARD: i8 = -1;
/// `MATERIAL_QUANTITY_*`.
const QUANTITY_COMMON: f64 = 5000.0;
const QUANTITY_UNCOMMON: f64 = 1000.0;
const QUANTITY_RARE: f64 = 200.0;
const QUANTITY_EXOTIC: f64 = 50.0;

/// A tradable `/datum/material`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TradableMaterial {
    pub id: &'static str,
    pub name: &'static str,
    /// The `/datum/export/material/market` subtype selling it.
    pub export: &'static str,
    /// `value_per_unit * SHEET_MATERIAL_AMOUNT`: the baseline price of a sheet.
    pub sheet_value: f64,
    /// `minimum_value_override * SHEET_MATERIAL_AMOUNT`.
    pub minimum_value_override: Option<f64>,
    /// `tradable_base_quantity`.
    pub base_quantity: f64,
}

impl TradableMaterial {
    /// The lowest price (BandaStation: `round(value * 0.8)`, or the override).
    pub fn price_minimum(&self) -> f64 {
        self.minimum_value_override
            .unwrap_or(self.sheet_value * 0.8)
            .floor()
    }

    /// The highest price (BandaStation: `round(value * 2)`).
    pub fn price_maximum(&self) -> f64 {
        (self.sheet_value * 2.0).floor()
    }
}

/// The tradable materials in tg's definition order (`basemats.dm`).
pub static MATERIALS: [TradableMaterial; 8] = [
    TradableMaterial {
        id: "/datum/material/iron",
        name: "iron",
        export: "/datum/export/material/market/iron",
        sheet_value: 5.0,
        minimum_value_override: Some(0.0),
        base_quantity: QUANTITY_COMMON,
    },
    TradableMaterial {
        id: "/datum/material/glass",
        name: "glass",
        export: "/datum/export/material/market/glass",
        sheet_value: 5.0,
        minimum_value_override: Some(0.0),
        base_quantity: QUANTITY_COMMON,
    },
    TradableMaterial {
        id: "/datum/material/silver",
        name: "silver",
        export: "/datum/export/material/market/silver",
        sheet_value: 50.0,
        minimum_value_override: None,
        base_quantity: QUANTITY_UNCOMMON,
    },
    TradableMaterial {
        id: "/datum/material/gold",
        name: "gold",
        export: "/datum/export/material/market/gold",
        sheet_value: 125.0,
        minimum_value_override: None,
        base_quantity: QUANTITY_RARE,
    },
    TradableMaterial {
        id: "/datum/material/diamond",
        name: "diamond",
        export: "/datum/export/material/market/diamond",
        sheet_value: 500.0,
        minimum_value_override: None,
        base_quantity: QUANTITY_EXOTIC,
    },
    TradableMaterial {
        id: "/datum/material/uranium",
        name: "uranium",
        export: "/datum/export/material/market/uranium",
        sheet_value: 100.0,
        minimum_value_override: None,
        base_quantity: QUANTITY_RARE,
    },
    TradableMaterial {
        id: "/datum/material/bluespace",
        name: "bluespace crystal",
        export: "/datum/export/material/market/bscrystal",
        sheet_value: 300.0,
        minimum_value_override: None,
        base_quantity: QUANTITY_EXOTIC,
    },
    TradableMaterial {
        id: "/datum/material/titanium",
        name: "titanium",
        export: "/datum/export/material/market/titanium",
        sheet_value: 125.0,
        minimum_value_override: None,
        base_quantity: QUANTITY_UNCOMMON,
    },
];

/// The index into [`MATERIALS`] of the material a market export sells.
pub fn material_of_export(export: &str) -> Option<usize> {
    MATERIALS.iter().position(|m| m.export == export)
}

/// One material's market state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarketEntry {
    /// `materials_prices`: per sheet.
    pub price: f64,
    /// `materials_trends`.
    pub trend: i8,
    /// `materials_trend_life`.
    pub trend_life: i32,
    /// `materials_quantity`.
    pub quantity: f64,
}

/// `/datum/stock_market_event` subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketEventKind {
    Reset,
    LargeBoost,
    LargeDrop,
    Hotcakes,
    Lockdown,
}

impl MarketEventKind {
    /// `subtypesof(/datum/stock_market_event)`.
    pub const ALL: [Self; 5] = [
        Self::Reset,
        Self::LargeBoost,
        Self::LargeDrop,
        Self::Hotcakes,
        Self::Lockdown,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Reset => "Market Reset!",
            Self::LargeBoost => "Large Boost!",
            Self::LargeDrop => "Large Drop!",
            Self::Hotcakes => "Selling like Hotcakes!",
            Self::Lockdown => "Lockdown!",
        }
    }

    /// `trend_value`, `trend_duration`.
    fn trend(self) -> (i8, i32) {
        match self {
            Self::Reset => (MARKET_TREND_STABLE, 1),
            Self::LargeBoost => (MARKET_TREND_UPWARD, 4),
            Self::LargeDrop => (MARKET_TREND_DOWNWARD, 4),
            Self::Hotcakes => (MARKET_TREND_UPWARD, 1),
            Self::Lockdown => (MARKET_TREND_DOWNWARD, 2),
        }
    }

    fn circumstances(self) -> &'static [&'static str] {
        match self {
            Self::Reset => &[
                "was purchased by a private investment firm, resetting the price of ",
                "restructured, resetting the price of ",
                "has been rolled into a larger company, resetting the price of ",
            ],
            Self::LargeBoost => &[
                "has just released a new product that raised the price of ",
                "discovered a new valuable use for ",
                "has produced a report that raised the price of ",
            ],
            Self::LargeDrop => &[
                "'s latest product has seen major controversy, and resulted in a price drop for ",
                "has been hit with a major lawsuit, resulting in a price drop for ",
                "has produced a report that lowered the price of ",
            ],
            Self::Hotcakes => &[
                "has just released a new product that is dominating the market for  ",
                "is hitting it big! Dramatically stocking and raising the price of ",
                ", in a surprise move, monopolized supply and has raised the price of ",
            ],
            Self::Lockdown => &[
                "is being investigated by the Galactic Trade Commission, resulting in a halt of \
                 trade for ",
                ", in a stunning move, has been embargoed by TerraGov, resulting in a halt of \
                 trade of ",
            ],
        }
    }
}

/// `/datum/stock_market_event` `company_name`.
const COMPANIES: [&str; 8] = [
    "Nakamura Engineering",
    "Robust Industries, LLC",
    "MODular Solutions",
    "SolGov",
    "Australicus Industrial Mining",
    "Nanotrasen-DeForest Corporation",
    "Aussec Armory",
    "Dreamland Robotics",
];

/// An ongoing event (`active_events`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarketEvent {
    pub kind: MarketEventKind,
    pub material: usize,
    /// `trend_duration`, counted down by `handle()`.
    pub duration: i32,
}

/// `SSstock_market`.
#[derive(Resource)]
pub struct StockMarket {
    /// Indexed like [`MATERIALS`].
    pub entries: Vec<MarketEntry>,
    pub active_events: Vec<MarketEvent>,
    /// `news_string`, one line per event; cleared by each economy tick.
    pub news: Vec<String>,
    /// `HAS_TRAIT(SSeconomy, TRAIT_MARKET_CRASHING)`.
    pub crashing: bool,
    rng: Rng,
    last_fire: Option<f64>,
}

/// tg `gaussian(mean, stddev)` (Marsaglia polar, without the cached second value).
fn gaussian(rng: &mut Rng, mean: f64, stddev: f64) -> f64 {
    loop {
        let r1 = rng.range(-10_000, 10_000) as f64 / 10_000.0;
        let r2 = rng.range(-10_000, 10_000) as f64 / 10_000.0;
        let working = r1 * r1 + r2 * r2;
        if working < 1.0 && working != 0.0 {
            return mean + stddev * r1 * (-2.0 * working.ln() / working).sqrt();
        }
    }
}

impl StockMarket {
    /// `SSstock_market.Initialize`.
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed ^ 0x7374_6f63);
        let entries = MATERIALS
            .iter()
            .map(|m| {
                let spread = (m.base_quantity * 0.5) as i64;
                MarketEntry {
                    price: m.sheet_value,
                    trend: rng.range(-1, 1) as i8,
                    trend_life: rng.range(1, 3) as i32,
                    quantity: m.base_quantity + rng.range(-spread, spread) as f64,
                }
            })
            .collect();
        Self {
            entries,
            active_events: Vec::new(),
            news: Vec::new(),
            crashing: false,
            rng,
            last_fire: None,
        }
    }

    /// The price of a sheet of `MATERIALS[material]`.
    pub fn price(&self, material: usize) -> f64 {
        self.entries[material].price
    }

    /// `adjust_material_price`: kept within the material's limits, rounded down.
    pub fn adjust_price(&mut self, material: usize, delta: f64) {
        let m = &MATERIALS[material];
        let entry = &mut self.entries[material];
        entry.price = (entry.price + delta)
            .clamp(m.price_minimum(), m.price_maximum())
            .floor();
    }

    /// `adjust_material_quantity`: within `0..=2 * base`, rounded down.
    pub fn adjust_quantity(&mut self, material: usize, delta: f64) {
        let m = &MATERIALS[material];
        let entry = &mut self.entries[material];
        entry.quantity = (entry.quantity + delta)
            .clamp(0.0, m.base_quantity * 2.0)
            .floor();
    }

    /// `/datum/export/material/market/sell_object` after the sale: `sheets` sold lower the
    /// price and stock the market.
    pub fn sold(&mut self, material: usize, sheets: f64) {
        if sheets <= 0.0 {
            return;
        }
        let entry = self.entries[material];
        self.adjust_price(
            material,
            -entry.price * (sheets / (sheets + entry.quantity)),
        );
        self.adjust_quantity(material, sheets);
    }

    /// `SSstock_market.fire`.
    pub fn fire(&mut self) {
        for material in 0..MATERIALS.len() {
            self.handle_trends_and_price(material);
        }
        // Events started by this tick's trends are handled in the same tick, as in tg.
        let mut events = std::mem::take(&mut self.active_events);
        events.retain_mut(|event| self.handle_event(event));
        self.active_events = events;
    }

    /// `handle_trends_and_price`.
    fn handle_trends_and_price(&mut self, material: usize) {
        if self.rng.prob(MARKET_EVENT_PROBABILITY) {
            let kind = MarketEventKind::ALL[self.rng.range(0, 4) as usize];
            self.start_event(kind, material);
        }
        let m = &MATERIALS[material];
        let entry = self.entries[material];
        let (price, trend, trend_life) = (entry.price, entry.trend, entry.trend_life);
        let (minimum, maximum) = (m.price_minimum(), m.price_maximum());
        let (baseline, quantity_baseline) = (m.sheet_value, m.base_quantity);
        if self.crashing {
            // The worst price and the least impact of a sale.
            let entry = &mut self.entries[material];
            entry.price = minimum;
            entry.quantity *= 2.0;
            entry.trend = MARKET_TREND_DOWNWARD;
            entry.trend_life = 1;
            return;
        }
        if trend_life == 0 {
            // Closer to a limit, likelier to turn back. tg measures the upper side as
            // `price - maximum` (never positive, clamped to 1); the distance to the maximum is
            // what mirrors the lower side.
            let new_trend = if price < baseline {
                let chance =
                    100.0 - ((price - minimum).clamp(1.0, 1000.0) / (baseline - minimum)) * 100.0;
                Some(if self.rng.prob(chance as f32) {
                    MARKET_TREND_UPWARD
                } else {
                    MARKET_TREND_STABLE
                })
            } else if price > baseline {
                let chance =
                    100.0 - ((maximum - price).clamp(1.0, 1000.0) / (maximum - baseline)) * 100.0;
                Some(if self.rng.prob(chance as f32) {
                    MARKET_TREND_DOWNWARD
                } else {
                    MARKET_TREND_STABLE
                })
            } else {
                None
            };
            let life = self.rng.range(1, 3) as i32;
            let entry = &mut self.entries[material];
            if let Some(new_trend) = new_trend {
                entry.trend = new_trend;
            }
            entry.trend_life = life;
        } else {
            self.entries[material].trend_life -= 1;
        }
        let rng = &mut self.rng;
        let (price_change, quantity_change) = match trend {
            MARKET_TREND_UPWARD => (
                gaussian(rng, price * 0.30, baseline * 0.15).ceil(),
                -gaussian(rng, quantity_baseline * 0.15, quantity_baseline * 0.15).floor(),
            ),
            MARKET_TREND_DOWNWARD => (
                -gaussian(rng, price * 0.3, baseline * 0.15).ceil(),
                gaussian(rng, quantity_baseline * 0.15, quantity_baseline * 0.15).floor(),
            ),
            _ => (
                gaussian(rng, 0.0, baseline * 0.01).floor(),
                gaussian(rng, 0.0, quantity_baseline * 0.5).floor(),
            ),
        };
        let entry = &mut self.entries[material];
        entry.price = (price + price_change).clamp(minimum, maximum).floor();
        entry.quantity = (entry.quantity + quantity_change)
            .clamp(0.0, quantity_baseline * 2.0)
            .floor();
    }

    /// `/datum/stock_market_event/start_event` of `kind` on `material`.
    pub fn start_event(&mut self, kind: MarketEventKind, material: usize) {
        let m = &MATERIALS[material];
        let (trend, duration) = kind.trend();
        self.entries[material].trend = trend;
        self.entries[material].trend_life = duration;
        // The events' own limits: `SHEET_MATERIAL_AMOUNT * 0.5` and `* 3` of `value_per_unit`.
        // tg reads `mat.value_per_unit` off the type path, which fails at runtime; the
        // material's value is what is meant.
        let (low, high) = (m.sheet_value * 0.5, m.sheet_value * 3.0);
        let price = self.entries[material].price;
        let new_price = match kind {
            MarketEventKind::Reset => Some(m.sheet_value),
            MarketEventKind::LargeBoost => {
                let boost = gaussian(&mut self.rng, price, price * 0.15).floor();
                Some((price + boost).clamp(low, high))
            }
            MarketEventKind::LargeDrop => {
                let drop = gaussian(&mut self.rng, price * 1.5, price * 0.15).floor();
                Some((price - drop).clamp(low, high))
            }
            MarketEventKind::Hotcakes => Some(high.floor()),
            MarketEventKind::Lockdown => None,
        };
        let news = new_price.is_some();
        if let Some(new_price) = new_price {
            self.entries[material].price = new_price;
        }
        if news {
            self.create_news(kind, material);
        }
        self.active_events.push(MarketEvent {
            kind,
            material,
            duration,
        });
    }

    /// `/datum/stock_market_event/handle`: `false` when the event ended.
    fn handle_event(&mut self, event: &mut MarketEvent) -> bool {
        event.duration -= 1;
        let ended = event.duration <= 0;
        if event.kind == MarketEventKind::Lockdown {
            let m = &MATERIALS[event.material];
            let entry = &mut self.entries[event.material];
            if ended {
                // `lockdown/end_event`: trade resumes at the material's value. (tg's `handle()`
                // zeroes the quantity again right after; the end is meant to restore it.)
                entry.quantity = m.base_quantity;
                entry.price = m.sheet_value;
                self.create_news(event.kind, event.material);
            } else {
                entry.quantity = 0.0;
            }
        }
        !ended
    }

    /// `create_news`.
    fn create_news(&mut self, kind: MarketEventKind, material: usize) {
        let company = COMPANIES[self.rng.range(0, COMPANIES.len() as i64 - 1) as usize];
        let circumstances = kind.circumstances();
        let circumstance =
            circumstances[self.rng.range(0, circumstances.len() as i64 - 1) as usize];
        let line = format!(
            "{} {company} {circumstance}{}.",
            kind.name(),
            MATERIALS[material].name
        );
        info!("stock market: {line}");
        self.news.push(line);
    }
}

pub struct StockMarketPlugin;

impl Plugin for StockMarketPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init).add_systems(Update, fire);
    }
}

fn init(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(StockMarket::new(seed));
}

/// `SSstock_market.fire` every [`MARKET_SECS`] of a running round (`RUNLEVEL_GAME`); the
/// market exports follow the new prices.
fn fire(world: &mut World) {
    let Some(ticker) = world.get_resource::<Ticker>() else {
        return;
    };
    if ticker.state() != GameState::Playing || !world.contains_resource::<StockMarket>() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut market = world.resource_mut::<StockMarket>();
    let Some(last) = market.last_fire else {
        market.last_fire = Some(now);
        return;
    };
    if now - last < MARKET_SECS {
        return;
    }
    market.last_fire = Some(now);
    market.fire();
    world.resource_scope(|world, market: Mut<StockMarket>| {
        if let Some(mut exports) = world.get_resource_mut::<Exports>() {
            exports.sync_market(&market);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_follow_bandastation() {
        let iron = &MATERIALS[0];
        assert_eq!((iron.price_minimum(), iron.price_maximum()), (0.0, 10.0));
        let gold = &MATERIALS[3];
        assert_eq!((gold.price_minimum(), gold.price_maximum()), (100.0, 250.0));
        assert_eq!(
            material_of_export("/datum/export/material/market/bscrystal"),
            Some(6)
        );
        assert_eq!(material_of_export("/datum/export/material/plasma"), None);
    }

    #[test]
    fn starts_at_value_and_stays_in_limits() {
        let mut market = StockMarket::new(42);
        for (entry, m) in market.entries.iter().zip(&MATERIALS) {
            assert_eq!(entry.price, m.sheet_value);
            assert!((-1..=1).contains(&entry.trend));
            assert!((1..=3).contains(&entry.trend_life));
            assert!(entry.quantity >= m.base_quantity * 0.5);
            assert!(entry.quantity <= m.base_quantity * 1.5);
        }
        for _ in 0..500 {
            market.fire();
            for (i, (entry, m)) in market.entries.iter().zip(&MATERIALS).enumerate() {
                let (low, high) = (m.sheet_value * 0.5, m.sheet_value * 3.0);
                assert!(
                    entry.price >= low.min(m.price_minimum()) && entry.price <= high,
                    "{i}: {entry:?}"
                );
                assert!(entry.quantity >= 0.0 && entry.quantity <= m.base_quantity * 2.0);
            }
        }
    }

    #[test]
    fn selling_lowers_the_price_and_stocks_the_market() {
        let mut market = StockMarket::new(1);
        let gold = 3;
        market.entries[gold].quantity = 200.0;
        market.sold(gold, 50.0);
        // 125 - 125 * 50 / 250 = 100, at the minimum.
        assert_eq!(market.price(gold), 100.0);
        assert_eq!(market.entries[gold].quantity, 250.0);
        market.sold(gold, 50.0);
        assert_eq!(market.price(gold), 100.0, "clamped to 0.8x");
        market.sold(gold, 1000.0);
        assert_eq!(
            market.entries[gold].quantity, 400.0,
            "at most twice the base"
        );
    }

    #[test]
    fn a_crash_bottoms_out_prices() {
        let mut market = StockMarket::new(8);
        market.crashing = true;
        let before = market.entries.clone();
        market.fire();
        for (i, m) in MATERIALS.iter().enumerate() {
            assert_eq!(market.price(i), m.price_minimum());
            assert_eq!(market.entries[i].trend, MARKET_TREND_DOWNWARD);
            // A lockdown that started this tick zeroes the quantity.
            if !market.active_events.iter().any(|e| e.material == i) {
                assert_eq!(market.entries[i].quantity, before[i].quantity * 2.0);
            }
        }
    }

    #[test]
    fn events() {
        let mut market = StockMarket::new(5);
        let silver = 2;
        market.start_event(MarketEventKind::Hotcakes, silver);
        assert_eq!(market.price(silver), 150.0);
        assert_eq!(market.entries[silver].trend, MARKET_TREND_UPWARD);
        assert_eq!(market.news.len(), 1);
        assert!(market.news[0].starts_with("Selling like Hotcakes! "));
        assert!(market.news[0].ends_with("silver."));
        market.start_event(MarketEventKind::Reset, silver);
        assert_eq!(market.price(silver), 50.0);

        let mut market = StockMarket::new(5);
        market.start_event(MarketEventKind::Lockdown, silver);
        assert!(market.news.is_empty(), "a lockdown is news when it ends");
        let mut event = market.active_events.pop().unwrap();
        assert!(market.handle_event(&mut event));
        assert_eq!(market.entries[silver].quantity, 0.0);
        assert!(!market.handle_event(&mut event));
        assert_eq!(market.entries[silver].quantity, 1000.0);
        assert_eq!(market.price(silver), 50.0);
        assert_eq!(market.news.len(), 1);
    }
}
