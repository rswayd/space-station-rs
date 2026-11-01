//! Selling (server), after tg `code/modules/cargo/exports.dm`: `export_item_and_contents` walks a
//! thing and everything inside it, innermost first; the first [`ExportDef`] that applies sells
//! each one into an [`ExportReport`], and everything walked is deleted, sold or not
//! (`delete_unsold`). Elasticity: a sale lowers the export's `k_elasticity` by
//! `amount * k_hit_percentile`; it recovers linearly over `k_recovery_time`.
// shortcut: no boosted exports (`SSeconomy.boosted_exports`), no stock market moves after market
// sales (prices stay at their start), no `COMSIG_ITEM_EXPORTED` handlers (bounties, sales tags),
// no hologram check.

use bevy::prelude::*;
use ss13_mapload::cargo::{ExportAmount, ExportDef};
use ss13_mapload::{DmItem, DmName, DmObj};

use super::MONEY_SYMBOL;
use crate::obj_damage::is_under;
use crate::storage::Stored;

/// The exports and their live elasticity (tg `GLOB.exports_list`).
#[derive(Resource, Debug, Default)]
pub struct Exports {
    pub defs: Vec<ExportDef>,
    /// `(k_elasticity, at)`: the value at server time `at`.
    elasticity: Vec<(f64, f64)>,
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

impl Exports {
    pub fn new(defs: Vec<ExportDef>) -> Self {
        let elasticity = defs.iter().map(|d| (d.k_elasticity, 0.0)).collect();
        Self { defs, elasticity }
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

    /// tg `get_cost`: `ROUND_UP(base * amount * k_elasticity)`.
    pub fn cost(&self, index: usize, path: &str, stack: Option<u32>, now: f64) -> i64 {
        let def = &self.defs[index];
        let mut total = def.cost * amount(def, path, stack);
        if def.k_elasticity > 0.0 {
            total *= self.elasticity(index, now);
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
    for &e in all.iter().rev() {
        let Some(path) = path_of(world, e) else {
            continue;
        };
        let stack = stack_of(world, e);
        if let Some(index) = exports.find(&path, stack, now) {
            let name = name_of(world, e);
            exports.sell(index, &path, stack, &name, now, report);
        }
    }
    if let Ok(entity) = world.get_entity_mut(thing) {
        entity.despawn();
    }
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
}
