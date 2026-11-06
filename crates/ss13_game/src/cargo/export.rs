//! Selling (server), after tg `code/modules/cargo/exports.dm`: `export_item_and_contents` walks a
//! thing and everything inside it, innermost first; the first [`ExportDef`] that applies sells
//! each one into an [`ExportReport`], and everything walked is deleted, sold or not
//! (`delete_unsold`). Elasticity: a sale lowers the export's `k_elasticity` by
//! `amount * k_hit_percentile`; it recovers linearly over `k_recovery_time`.
//!
//! - Boosted exports (`SSeconomy.boosted_exports`): every economy tick picks 2-4 exports that pay
//!   `EXPORT_BOOST_MULT` times their value until the next tick.
//! - Market exports (`/datum/export/material/market/*`) are worth the live
//!   [`StockMarket`] price per sheet, and each sale moves the market.
//! - [`export_value`] is the dry run the export scanner shows.
//! - A bounty cube sells through `crate::cargo::bounty::sell_cube` (its pricetag pays the
//!   holders first); a stock block through its material's market export
//!   (`crate::cargo::materials_market::sell_block`).
// shortcut: no other `COMSIG_ITEM_EXPORTED` handlers (sales tags), no hologram check.

use bevy::prelude::*;
use ss13_mapload::cargo::{ExportAmount, ExportDef};
use ss13_mapload::{DmItem, DmName, DmObj};

use ss13_mapload::spawner::Rng;

use super::MONEY_SYMBOL;
use super::stock_market::{MATERIALS, StockMarket, material_of_export};
use crate::obj_damage::is_under;
use crate::storage::Stored;

/// `EXPORT_BOOST_MIN_AMOUNT`, `EXPORT_BOOST_MAX_AMOUNT`.
pub const EXPORT_BOOST_AMOUNT: (i64, i64) = (2, 4);
/// `EXPORT_BOOST_MULT`.
pub const EXPORT_BOOST_MULT: f64 = 3.0;

/// The exports and their live elasticity (tg `GLOB.exports_list`).
#[derive(Resource, Debug, Default)]
pub struct Exports {
    pub defs: Vec<ExportDef>,
    /// `(k_elasticity, at)`: the value at server time `at`.
    elasticity: Vec<(f64, f64)>,
    /// `SSeconomy.boosted_exports`, indices into `defs`.
    boosted: Vec<usize>,
    /// For market exports: the material (index into [`MATERIALS`]) and the export's own `cost`
    /// multiplier of its price.
    market: Vec<Option<(usize, f64)>>,
    /// `SSstock_market.materials_prices`, kept in step by [`Exports::sync_market`].
    market_prices: Vec<f64>,
}

/// tg `/datum/export_report`, indexed like [`Exports::defs`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportReport {
    pub total_amount: Vec<f64>,
    pub total_value: Vec<i64>,
    /// Names of what was sold.
    pub exported_atoms: Vec<String>,
}

impl ExportReport {
    pub fn total(&self) -> i64 {
        self.total_value.iter().sum()
    }
}

/// tg `is_type_in_typecache` with `include_subtypes` and `exclude_types`.
pub fn type_matches(def: &ExportDef, path: &str) -> bool {
    let listed = def.export_types.iter().any(|t| {
        if def.include_subtypes {
            is_under(path, t)
        } else {
            path == t
        }
    });
    listed && !(def.include_subtypes && def.exclude_types.iter().any(|t| is_under(path, t)))
}

/// tg `get_amount`: `stack` is the item's stack amount, if it is a stack.
pub fn amount(def: &ExportDef, path: &str, stack: Option<u32>) -> f64 {
    match &def.amount {
        ExportAmount::One => 1.0,
        ExportAmount::Stack => stack.map_or(0.0, f64::from),
        ExportAmount::Material(units) => {
            let Some((_, per)) = units.iter().find(|(t, _)| t == path) else {
                return 0.0;
            };
            // tg `round(amount / SHEET_MATERIAL_AMOUNT)` of the composition.
            (per * f64::from(stack.unwrap_or(1))).round()
        }
    }
}

/// `ROUND_UP`, tolerant of float noise (`0.41 * 200` is 82, not 82.00000000000001).
fn round_up(x: f64) -> i64 {
    (x - 1e-9).ceil() as i64
}

/// `/datum/export/bounty_box`: its value is the cube's (`crate::cargo::bounty::sell_cube`), so
/// it is a report line here and never found by type.
pub const BOUNTY_EXPORT: &str = "/datum/export/bounty_box";

fn bounty_export() -> ExportDef {
    ExportDef {
        id: BOUNTY_EXPORT.into(),
        unit_name: "completed bounty cube".into(),
        message: String::new(),
        cost: 1.0,
        k_elasticity: 0.0,
        k_hit_percentile: 0.0,
        k_recovery_secs: 0.0,
        amount_report_multiplier: 1.0,
        include_subtypes: true,
        export_types: Vec::new(),
        exclude_types: Vec::new(),
        amount: ExportAmount::One,
    }
}

impl Exports {
    pub fn new(mut defs: Vec<ExportDef>) -> Self {
        defs.retain(|d| d.id != BOUNTY_EXPORT);
        defs.push(bounty_export());
        let elasticity = defs.iter().map(|d| (d.k_elasticity, 0.0)).collect();
        // The converter priced market exports at the material's starting price (`cost * price`);
        // the multiplier is what is left of that.
        let market = defs
            .iter()
            .map(|d| {
                let material = material_of_export(&d.id)?;
                let value = MATERIALS[material].sheet_value;
                Some((material, if value > 0.0 { d.cost / value } else { 1.0 }))
            })
            .collect();
        Self {
            defs,
            elasticity,
            boosted: Vec::new(),
            market,
            market_prices: MATERIALS.iter().map(|m| m.sheet_value).collect(),
        }
    }

    /// `SSeconomy.update_boosted_exports`: 2-4 random exports pay triple until the next call.
    pub fn update_boosted(&mut self, rng: &mut Rng) {
        self.boosted.clear();
        let mut valid: Vec<usize> = (0..self.defs.len())
            .filter(|&i| self.defs[i].id != BOUNTY_EXPORT)
            .collect();
        let n = rng.range(EXPORT_BOOST_AMOUNT.0, EXPORT_BOOST_AMOUNT.1);
        for _ in 0..n {
            if valid.is_empty() {
                break;
            }
            let i = rng.range(0, valid.len() as i64 - 1) as usize;
            self.boosted.push(valid.swap_remove(i));
        }
    }

    /// No boosted exports (tg skips them under `UNIT_TESTS`).
    pub fn clear_boosted(&mut self) {
        self.boosted.clear();
    }

    pub fn boosted(&self) -> &[usize] {
        &self.boosted
    }

    /// The material a market export sells.
    pub fn market_material(&self, index: usize) -> Option<usize> {
        self.market.get(index).copied().flatten().map(|(m, _)| m)
    }

    /// The market export selling `MATERIALS[material]`.
    pub fn market_export(&self, material: usize) -> Option<usize> {
        self.market
            .iter()
            .position(|m| m.is_some_and(|(mat, _)| mat == material))
    }

    /// A stock block's `get_cost` through market export `index`: `sheets` at `unit` (the
    /// block's locked or live price) times the export's own `cost`, with elasticity and boost.
    pub fn block_cost(&self, index: usize, sheets: f64, unit: f64, now: f64) -> i64 {
        let def = &self.defs[index];
        let multiplier = self.market[index].map_or(def.cost, |(_, m)| m);
        let mut total = multiplier * unit * sheets;
        if def.k_elasticity > 0.0 {
            total *= self.elasticity(index, now);
        }
        if self.boosted.contains(&index) {
            total *= EXPORT_BOOST_MULT;
        }
        round_up(total)
    }

    /// [`Exports::sell`] of a stock block. `false`: worth nothing, not sold.
    pub fn sell_block(
        &mut self,
        index: usize,
        sheets: f64,
        unit: f64,
        name: &str,
        now: f64,
        report: &mut ExportReport,
    ) -> bool {
        let value = self.block_cost(index, sheets, unit, now);
        if sheets <= 0.0 || value <= 0 {
            return false;
        }
        let def = &self.defs[index];
        let n = self.defs.len();
        report.total_value.resize(n, 0);
        report.total_amount.resize(n, 0.0);
        report.total_value[index] += value;
        report.total_amount[index] += sheets * def.amount_report_multiplier;
        report.exported_atoms.push(name.to_owned());
        if def.k_elasticity > 0.0 {
            let k = (self.elasticity(index, now) - sheets * def.k_hit_percentile).max(0.0);
            self.elasticity[index] = (k, now);
        }
        true
    }

    /// Takes the stock market's current prices.
    pub fn sync_market(&mut self, market: &StockMarket) {
        for (i, price) in self.market_prices.iter_mut().enumerate() {
            *price = market.price(i);
        }
    }

    /// `k_elasticity` at `now`, recovered since the last sale (tg `process`).
    pub fn elasticity(&self, index: usize, now: f64) -> f64 {
        let def = &self.defs[index];
        let (k, at) = self.elasticity[index];
        if def.k_elasticity <= 0.0 || def.k_recovery_secs <= 0.0 {
            return k;
        }
        (k + (now - at).max(0.0) / def.k_recovery_secs).min(1.0)
    }

    /// tg `get_cost`: `ROUND_UP(base * amount * k_elasticity)`, tripled while boosted; a
    /// market export's base is its multiplier of the live price.
    pub fn cost(&self, index: usize, path: &str, stack: Option<u32>, now: f64) -> i64 {
        let def = &self.defs[index];
        let base = match self.market.get(index).copied().flatten() {
            Some((material, multiplier)) => multiplier * self.market_prices[material],
            None => def.cost,
        };
        let mut total = base * amount(def, path, stack);
        if def.k_elasticity > 0.0 {
            total *= self.elasticity(index, now);
        }
        if self.boosted.contains(&index) {
            total *= EXPORT_BOOST_MULT;
        }
        round_up(total)
    }

    /// The export that sells `path` (tg `_export_loop`: the first whose `applies_to` passes).
    pub fn find(&self, path: &str, stack: Option<u32>, now: f64) -> Option<usize> {
        (0..self.defs.len())
            .find(|&i| type_matches(&self.defs[i], path) && self.cost(i, path, stack, now) != 0)
    }

    /// tg `sell_object`: into the report, then elasticity drops. `false`: not sold.
    pub fn sell(
        &mut self,
        index: usize,
        path: &str,
        stack: Option<u32>,
        name: &str,
        now: f64,
        report: &mut ExportReport,
    ) -> bool {
        let value = self.cost(index, path, stack, now);
        let def = &self.defs[index];
        let units = amount(def, path, stack);
        if units <= 0.0 || value <= 0 {
            return false;
        }
        let n = self.defs.len();
        report.total_value.resize(n, 0);
        report.total_amount.resize(n, 0.0);
        report.total_value[index] += value;
        report.total_amount[index] += units * def.amount_report_multiplier;
        report.exported_atoms.push(name.to_owned());
        if def.k_elasticity > 0.0 {
            let k = (self.elasticity(index, now) - units * def.k_hit_percentile).max(0.0);
            self.elasticity[index] = (k, now);
        }
        true
    }

    /// A bounty cube sold for `value` after its holders' cuts (`COMSIG_ITEM_EXPORTED` of the
    /// pricetag: one unit, the rest of the value).
    pub fn report_bounty(&self, value: i64, name: &str, report: &mut ExportReport) {
        let Some(index) = self.defs.iter().position(|d| d.id == BOUNTY_EXPORT) else {
            return;
        };
        let n = self.defs.len();
        report.total_value.resize(n, 0);
        report.total_amount.resize(n, 0.0);
        report.total_value[index] += value;
        report.total_amount[index] += 1.0;
        report.exported_atoms.push(name.to_owned());
    }

    /// tg `total_printout` of every export that sold something, in export order.
    pub fn printout(&self, report: &ExportReport) -> Vec<String> {
        (0..self.defs.len())
            .filter_map(|i| {
                let value = *report.total_value.get(i)?;
                let amount = *report.total_amount.get(i)?;
                (value != 0 && amount != 0.0).then(|| total_printout(&self.defs[i], amount, value))
            })
            .collect()
    }
}

/// tg `/datum/export/total_printout` (`crate` adds its recycling note).
pub fn total_printout(def: &ExportDef, amount: f64, value: i64) -> String {
    let amount_text = if amount.fract() == 0.0 {
        format!("{amount}")
    } else {
        format!("{amount:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    };
    let mut msg = format!("{value}{MONEY_SYMBOL}: Received {amount_text} ");
    if value > 0 {
        msg.insert(0, '+');
    }
    if !def.unit_name.is_empty() {
        msg.push_str(&def.unit_name);
        if amount > 1.0 {
            msg.push_str(plural_s(&def.unit_name));
        }
        if !def.message.is_empty() {
            msg.push(' ');
        }
    }
    msg.push_str(&def.message);
    msg.push('.');
    if def.id == "/datum/export/crate" || def.id.starts_with("/datum/export/crate/") {
        msg.push_str(" Thanks for participating in Nanotrasen Crates Recycling Program.");
    }
    msg
}

/// tg `plural_s`.
fn plural_s(word: &str) -> &'static str {
    let lower = word.to_lowercase();
    if ["ss", "sh", "ch", "s", "x", "z"]
        .iter()
        .any(|end| lower.ends_with(end))
    {
        "es"
    } else {
        "s"
    }
}

/// The DM type of a sellable thing.
pub fn path_of(world: &World, e: Entity) -> Option<String> {
    let entity = world.get_entity(e).ok()?;
    entity
        .get::<DmItem>()
        .map(|i| i.path.clone())
        .or_else(|| entity.get::<DmObj>().map(|o| o.path.clone()))
}

fn stack_of(world: &World, e: Entity) -> Option<u32> {
    world
        .get::<crate::tools::Stack>(e)
        .map(|s| s.amount)
        .or_else(|| world.get::<ss13_mapload::DmStack>(e).map(|s| s.amount))
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| n.name.clone())
        .unwrap_or_default()
}

/// tg `export_item_and_contents(apply_elastic = TRUE, delete_unsold = TRUE)`: `thing` and what
/// it holds are sold into `report`, innermost first, and despawned.
pub fn export_item_and_contents(
    world: &mut World,
    exports: &mut Exports,
    thing: Entity,
    now: f64,
    report: &mut ExportReport,
) {
    // `get_all_contents`: the thing first, then breadth-first; sold in reverse.
    let mut all = vec![thing];
    let mut i = 0;
    while i < all.len() {
        if let Some(stored) = world.get::<Stored>(all[i]) {
            all.extend_from_slice(stored.items());
        }
        i += 1;
    }
    if let Some(market) = world.get_resource::<StockMarket>() {
        exports.sync_market(market);
    }
    for &e in all.iter().rev() {
        let Some(path) = path_of(world, e) else {
            continue;
        };
        if super::bounty::sell_cube(world, exports, e, report) {
            continue;
        }
        if super::materials_market::sell_block(world, exports, e, now, report) {
            continue;
        }
        let stack = stack_of(world, e);
        let Some(index) = exports.find(&path, stack, now) else {
            continue;
        };
        let name = name_of(world, e);
        if !exports.sell(index, &path, stack, &name, now, report) {
            continue;
        }
        // `/datum/export/material/market/sell_object`: the sale moves the market.
        if let Some(material) = exports.market_material(index) {
            let sheets = amount(&exports.defs[index], &path, stack);
            if let Some(mut market) = world.get_resource_mut::<StockMarket>() {
                market.sold(material, sheets);
                exports.sync_market(&market);
            }
        }
    }
    if let Ok(entity) = world.get_entity_mut(thing) {
        entity.despawn();
    }
}

/// tg `export_item_and_contents(dry_run = TRUE)` summed: what `thing` and its contents would
/// sell for now, and whether it holds anything. Nothing changes.
pub fn export_value(world: &World, exports: &Exports, thing: Entity, now: f64) -> (i64, bool) {
    let mut all = vec![thing];
    let mut i = 0;
    while i < all.len() {
        if let Some(stored) = world.get::<Stored>(all[i]) {
            all.extend_from_slice(stored.items());
        }
        i += 1;
    }
    let value = all
        .iter()
        .filter_map(|&e| {
            if let Some(cube) = world.get::<super::bounty::BountyCube>(e) {
                return Some(cube.sale_value());
            }
            if let Some(value) = super::materials_market::block_value(world, exports, e, now) {
                return Some(value);
            }
            let path = path_of(world, e)?;
            let stack = stack_of(world, e);
            let index = exports.find(&path, stack, now)?;
            Some(exports.cost(index, &path, stack, now))
        })
        .sum();
    (value, all.len() > 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, cost: f64, amount: ExportAmount) -> ExportDef {
        ExportDef {
            id: id.into(),
            unit_name: String::new(),
            message: String::new(),
            cost,
            k_elasticity: 1.0,
            k_hit_percentile: 0.05,
            k_recovery_secs: 600.0,
            amount_report_multiplier: 1.0,
            include_subtypes: true,
            export_types: vec![],
            exclude_types: vec![],
            amount,
        }
    }

    #[test]
    fn crates_match_with_their_exclusions() {
        let crate_export = ExportDef {
            unit_name: "crate".into(),
            k_elasticity: 0.0,
            export_types: vec!["/obj/structure/closet/crate".into()],
            exclude_types: vec!["/obj/structure/closet/crate/large".into()],
            ..def("/datum/export/crate", 200.0, ExportAmount::One)
        };
        assert!(type_matches(&crate_export, "/obj/structure/closet/crate"));
        assert!(type_matches(
            &crate_export,
            "/obj/structure/closet/crate/engineering"
        ));
        assert!(!type_matches(
            &crate_export,
            "/obj/structure/closet/crate/large"
        ));
        assert!(!type_matches(&crate_export, "/obj/structure/closet"));
        let exports = Exports::new(vec![crate_export]);
        assert_eq!(
            exports.cost(0, "/obj/structure/closet/crate", None, 0.0),
            200
        );
        let mut report = ExportReport::default();
        let mut exports = exports;
        assert!(exports.sell(
            0,
            "/obj/structure/closet/crate",
            None,
            "crate",
            0.0,
            &mut report
        ));
        assert!(exports.sell(
            0,
            "/obj/structure/closet/crate",
            None,
            "crate",
            0.0,
            &mut report
        ));
        assert_eq!(report.total(), 400, "k_elasticity 0: no elasticity");
        assert_eq!(
            exports.printout(&report),
            [
                "+400¢: Received 2 crates. Thanks for participating in Nanotrasen Crates Recycling Program."
            ]
        );
    }

    #[test]
    fn stacks_and_elasticity() {
        let plasteel = ExportDef {
            message: "of plasteel".into(),
            unit_name: "sheet".into(),
            export_types: vec!["/obj/item/stack/sheet/plasteel".into()],
            ..def("/datum/export/stack/plasteel", 82.0, ExportAmount::Stack)
        };
        let mut exports = Exports::new(vec![plasteel]);
        let path = "/obj/item/stack/sheet/plasteel";
        assert_eq!(
            exports.find(path, Some(0), 0.0),
            None,
            "an empty stack is worth 0"
        );
        assert_eq!(
            exports.find("/obj/item/stack/sheet/iron", Some(5), 0.0),
            None
        );
        assert_eq!(exports.cost(0, path, Some(10), 0.0), 820);
        let mut report = ExportReport::default();
        assert!(exports.sell(0, path, Some(10), "plasteel", 0.0, &mut report));
        // 10 sheets * 0.05: elasticity 0.5, back to 1 after half the recovery time... per tg's
        // linear `seconds_per_tick / k_recovery_time`.
        assert!((exports.elasticity(0, 0.0) - 0.5).abs() < 1e-9);
        assert_eq!(exports.cost(0, path, Some(10), 0.0), 410);
        assert!((exports.elasticity(0, 150.0) - 0.75).abs() < 1e-9);
        assert_eq!(exports.elasticity(0, 10_000.0), 1.0);
        assert_eq!(
            exports.printout(&report),
            ["+820¢: Received 10 sheets of plasteel."]
        );
    }

    #[test]
    fn materials_count_sheets_of_their_material() {
        let plasma = ExportDef {
            message: "cm3 of plasma".into(),
            k_elasticity: 0.0,
            amount_report_multiplier: 100.0,
            export_types: vec!["/obj/item/stack/sheet/mineral".into()],
            ..def(
                "/datum/export/material/plasma",
                80.0,
                ExportAmount::Material(vec![("/obj/item/stack/sheet/mineral/plasma".into(), 1.0)]),
            )
        };
        let mut exports = Exports::new(vec![plasma]);
        let path = "/obj/item/stack/sheet/mineral/plasma";
        assert_eq!(
            exports.find("/obj/item/stack/sheet/mineral/gold", Some(5), 0.0),
            None
        );
        let mut report = ExportReport::default();
        assert!(exports.sell(0, path, Some(5), "solid plasma", 0.0, &mut report));
        assert_eq!(report.total(), 400);
        assert_eq!(
            exports.printout(&report),
            ["+400¢: Received 500 cm3 of plasma."]
        );
    }

    #[test]
    fn boosted_exports_pay_triple() {
        let wrench = ExportDef {
            k_elasticity: 0.0,
            export_types: vec!["/obj/item/wrench".into()],
            ..def("/datum/export/wrench", 2.0, ExportAmount::One)
        };
        let defs: Vec<ExportDef> = (0..6).map(|_| wrench.clone()).collect();
        let mut exports = Exports::new(defs);
        let mut rng = Rng::new(9);
        for _ in 0..20 {
            exports.update_boosted(&mut rng);
            let boosted = exports.boosted().to_vec();
            assert!((2..=4).contains(&boosted.len()), "{boosted:?}");
            let mut unique = boosted.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), boosted.len());
        }
        let first = exports.boosted()[0];
        assert_eq!(exports.cost(first, "/obj/item/wrench", None, 0.0), 6);
        exports.clear_boosted();
        assert_eq!(exports.cost(first, "/obj/item/wrench", None, 0.0), 2);
    }

    #[test]
    fn market_exports_follow_the_stock_market() {
        // The converter's price: `cost` 1 times gold's 125 per sheet.
        let gold = ExportDef {
            message: "cm3 of gold".into(),
            amount_report_multiplier: 100.0,
            k_elasticity: 0.0,
            export_types: vec!["/obj/item/stack/sheet/mineral/gold".into()],
            ..def(
                "/datum/export/material/market/gold",
                125.0,
                ExportAmount::Material(vec![("/obj/item/stack/sheet/mineral/gold".into(), 1.0)]),
            )
        };
        let mut exports = Exports::new(vec![gold]);
        let path = "/obj/item/stack/sheet/mineral/gold";
        assert_eq!(exports.market_material(0), Some(3));
        assert_eq!(exports.cost(0, path, Some(2), 0.0), 250);
        let mut market = StockMarket::new(3);
        market.entries[3].quantity = 200.0;
        market.sold(3, 50.0);
        exports.sync_market(&market);
        assert_eq!(
            exports.cost(0, path, Some(2), 0.0),
            200,
            "100 per sheet now"
        );
    }
}
