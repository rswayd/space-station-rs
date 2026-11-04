//! Vending machines as prototype data: the product lists and prices of
//! `/obj/machinery/vending` and its subtypes, resolved the way tg builds a vendor's records
//! (`code/modules/vending/vendor/inventory.dm`: `RefreshParts` -> `build_inventories` ->
//! `build_inventory`). Only vendor types get [`DmVending`].

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::Field;

pub const VENDING_ROOT: &str = "/obj/machinery/vending";

/// `ui_data.dm` `default_category` and the two categories `build_inventories` adds.
pub const DEFAULT_CATEGORY: (&str, &str) = ("Products", "cart-shopping");
pub const CONTRABAND_CATEGORY: (&str, &str) = ("Contraband", "mask");
pub const PREMIUM_CATEGORY: (&str, &str) = ("Premium", "coins");

const COMPONENT: &str = "dm_vending";

/// One `datum/data/vending_product` as `build_inventory` creates it.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct VendProduct {
    pub path: String,
    /// Round-start stock, also `max_amount`.
    pub amount: u32,
    pub price: u32,
    /// Index into [`DmVending::categories`].
    pub category: u16,
    /// The item's `age_restricted` (alcohol, tobacco).
    pub age_restricted: bool,
}

#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct VendCategory {
    pub name: String,
    /// Font Awesome name.
    pub icon: String,
}

/// A vendor type's resolved static data (prototype data, the whole model per type).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmVending {
    /// `product_records`.
    pub products: Vec<VendProduct>,
    /// `hidden_records` (`contraband`, sold only with `extended_inventory`).
    pub contraband: Vec<VendProduct>,
    /// `coin_records` (`premium`).
    pub premium: Vec<VendProduct>,
    pub categories: Vec<VendCategory>,
    /// DM `null`: decided at load (`TRUE` off station).
    pub all_products_free: Option<bool>,
    pub payment_department: String,
    /// `product_ads`, `;`-separated.
    pub product_ads: String,
    pub displayed_currency_icon: String,
    pub displayed_currency_name: String,
    /// Why the product lists are not static (empty = resolved).
    pub unresolved: String,
    /// `product_slogans`, `;`-separated.
    pub product_slogans: String,
    /// `vend_reply`; empty: tg's default thanks.
    pub vend_reply: String,
    /// `refill_canister` type; empty: no circuit (cannot be restocked or taken apart).
    pub refill_canister: String,
    pub tiltable: bool,
    pub squish_damage: f32,
    /// Percent.
    pub crit_chance: f32,
    pub age_restrictions: bool,
    /// The type's `onstation` (map load also clears it off the station levels).
    pub onstation: bool,
    /// `req_access`: who may load items back (`compartmentLoadAccessCheck`).
    pub req_access: Vec<String>,
    /// `allow_custom`: a brand the vendor board's dial offers.
    pub allow_custom: bool,
    /// `/obj/machinery/vending/custom::max_loaded_items`.
    pub max_loaded_items: u32,
}

/// `/obj/item/vending_refill`: the vendor it belongs to (`machine_name`, its name's prefix).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmVendingRefill {
    pub machine_name: String,
}

/// An item's `custom_price` (custom vendors sell only priced items).
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq)]
pub struct DmPrice {
    pub custom_price: u32,
}

pub const REFILL_ROOT: &str = "/obj/item/vending_refill";
const ITEM_ROOT: &str = "/obj/item";
const REFILL_COMPONENT: &str = "dm_vending_refill";
const PRICE_COMPONENT: &str = "dm_price";

/// The whole model for `path`, inherited vars resolved.
pub fn vending(types: &DmTypes, path: &str) -> DmVending {
    let text = |var: &str| match types.resolved(path, var) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    let mut v = DmVending {
        all_products_free: match types.resolved(path, "all_products_free") {
            None | Some(Value::Null) => None,
            value => Some(truthy(value)),
        },
        payment_department: text("payment_department"),
        product_ads: text("product_ads"),
        displayed_currency_icon: text("displayed_currency_icon"),
        displayed_currency_name: text("displayed_currency_name"),
        product_slogans: text("product_slogans"),
        vend_reply: text("vend_reply"),
        refill_canister: path_var(types.resolved(path, "refill_canister")),
        tiltable: truthy(types.resolved(path, "tiltable")),
        squish_damage: num(types.resolved(path, "squish_damage")).unwrap_or(0.0) as f32,
        crit_chance: num(types.resolved(path, "crit_chance")).unwrap_or(0.0) as f32,
        age_restrictions: truthy(types.resolved(path, "age_restrictions")),
        onstation: truthy(types.resolved(path, "onstation")),
        req_access: string_list(types.resolved(path, "req_access")),
        allow_custom: truthy(types.resolved(path, "allow_custom")),
        max_loaded_items: num(types.resolved(path, "max_loaded_items"))
            .unwrap_or(0.0)
            .max(0.0) as u32,
        ..default()
    };
    // `build_inventory` rounds both after the (here absent) inflation multiplier.
    let default_price = dm_round(num(types.resolved(path, "default_price")).unwrap_or(0.0));
    let extra_price = dm_round(num(types.resolved(path, "extra_price")).unwrap_or(0.0));

    let mut unresolved = Vec::new();
    // `RefreshParts`: categories, when set, replace `products` with the union of theirs.
    let mut products: Vec<(String, u32, u16)> = Vec::new();
    match types.resolved(path, "product_categories") {
        None | Some(Value::Null) => {
            match assoc_list(types.resolved(path, "products")) {
                Some(list) => products = list.into_iter().map(|(p, n)| (p, n, 0)).collect(),
                None => unresolved.push("products"),
            }
            v.categories.push(category(DEFAULT_CATEGORY));
        }
        Some(cats) => match categories(cats) {
            Some(cats) => {
                for (i, (name, icon, list)) in cats.into_iter().enumerate() {
                    v.categories.push(VendCategory { name, icon });
                    for (product, amount) in list {
                        // `products |= category["products"]` keeps the first amount;
                        // `product_to_category` the last category.
                        match products.iter_mut().find(|(p, ..)| *p == product) {
                            Some(entry) => entry.2 = i as u16,
                            None => products.push((product, amount, i as u16)),
                        }
                    }
                }
            }
            None => unresolved.push("product_categories"),
        },
    }
    let price = |product: &str, premium: bool| {
        let custom = dm_round(num(types.resolved(product, "custom_price")).unwrap_or(0.0));
        if !premium {
            return if custom != 0 { custom } else { default_price };
        }
        let premium_custom =
            dm_round(num(types.resolved(product, "custom_premium_price")).unwrap_or(0.0));
        match (premium_custom, custom) {
            (0, 0) => extra_price,
            (0, custom) => extra_price + custom,
            (premium_custom, _) => premium_custom,
        }
    };
    let record = |(path, amount, category): (String, u32, u16), premium: bool| VendProduct {
        price: price(&path, premium),
        age_restricted: truthy(types.resolved(&path, "age_restricted")),
        path,
        amount,
        category,
    };
    v.products = products.into_iter().map(|p| record(p, false)).collect();
    for (var, cat, out) in [
        ("contraband", CONTRABAND_CATEGORY, &mut v.contraband),
        ("premium", PREMIUM_CATEGORY, &mut v.premium),
    ] {
        let index = v.categories.len() as u16;
        match assoc_list(types.resolved(path, var)) {
            Some(list) if !list.is_empty() => {
                v.categories.push(category(cat));
                *out = list
                    .into_iter()
                    .map(|(p, n)| record((p, n, index), true))
                    .collect();
            }
            Some(_) => {}
            None => unresolved.push(var),
        }
    }
    // Vendors that fill their lists in `Initialize` (AutoDrobe) declare none statically.
    let initialize_override = types
        .chain(path)
        .take_while(|t| *t != VENDING_ROOT)
        .any(|t| types.types[t].procs.iter().any(|p| p == "Initialize"));
    if v.products.is_empty() && v.premium.is_empty() && initialize_override {
        unresolved.push("lists filled in Initialize()");
    }
    // Abstract or unknown product types cannot be spawned.
    let unknown: Vec<&str> = [&v.products, &v.contraband, &v.premium]
        .into_iter()
        .flatten()
        .map(|p| p.path.as_str())
        .filter(|p| !types.types.contains_key(*p))
        .collect();
    if !unknown.is_empty() {
        unresolved.push("unknown product types");
    }
    v.unresolved = unresolved.join(", ");
    v
}

/// Item types every vendor among `needed` sells: the converter emits their prototypes.
pub fn product_closure<'a>(
    types: &DmTypes,
    needed: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    needed
        .into_iter()
        .filter(|p| is_under(p, VENDING_ROOT))
        .flat_map(|p| {
            let v = vending(types, p);
            let canister = (!v.refill_canister.is_empty()).then_some(v.refill_canister);
            [v.products, v.contraband, v.premium]
                .into_iter()
                .flatten()
                .map(|p| p.path)
                .chain(canister)
        })
        .filter(|p| types.types.contains_key(p))
        .collect()
}

/// Converter fields of a vendor type (the whole model), of a refill canister, and an item's
/// declared `custom_price`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let declared = |var: &str| types.types.get(path).and_then(|t| t.declared_vars.get(var));
    if is_under(path, ITEM_ROOT)
        && let Some(price) = declared("custom_price").and_then(|v| num(Some(v)))
    {
        out.push(ext(
            PRICE_COMPONENT,
            "custom_price",
            dm_round(price).to_string(),
        ));
    }
    if is_under(path, REFILL_ROOT) && (path == REFILL_ROOT || declared("machine_name").is_some()) {
        let name = match types.resolved(path, "machine_name") {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        out.push(ext(
            REFILL_COMPONENT,
            "machine_name",
            ron::to_string(&name).expect("strings serialize"),
        ));
    }
    if !is_under(path, VENDING_ROOT) {
        return out;
    }
    let v = vending(types, path);
    let s = |s: &str| ron::to_string(s).expect("strings serialize");
    let products = |list: &[VendProduct]| {
        let entries: Vec<String> = list
            .iter()
            .map(|p| {
                format!(
                    "(path: {}, amount: {}, price: {}, category: {}, age_restricted: {})",
                    s(&p.path),
                    p.amount,
                    p.price,
                    p.category,
                    p.age_restricted
                )
            })
            .collect();
        format!("[{}]", entries.join(", "))
    };
    let categories: Vec<String> = v
        .categories
        .iter()
        .map(|c| format!("(name: {}, icon: {})", s(&c.name), s(&c.icon)))
        .collect();
    out.extend([
        ext(COMPONENT, "products", products(&v.products)),
        ext(COMPONENT, "contraband", products(&v.contraband)),
        ext(COMPONENT, "premium", products(&v.premium)),
        ext(
            COMPONENT,
            "categories",
            format!("[{}]", categories.join(", ")),
        ),
        ext(
            COMPONENT,
            "all_products_free",
            format!("{:?}", v.all_products_free),
        ),
        ext(COMPONENT, "payment_department", s(&v.payment_department)),
        ext(COMPONENT, "product_ads", s(&v.product_ads)),
        ext(
            COMPONENT,
            "displayed_currency_icon",
            s(&v.displayed_currency_icon),
        ),
        ext(
            COMPONENT,
            "displayed_currency_name",
            s(&v.displayed_currency_name),
        ),
        ext(COMPONENT, "unresolved", s(&v.unresolved)),
        ext(COMPONENT, "product_slogans", s(&v.product_slogans)),
        ext(COMPONENT, "vend_reply", s(&v.vend_reply)),
        ext(COMPONENT, "refill_canister", s(&v.refill_canister)),
        ext(COMPONENT, "tiltable", v.tiltable.to_string()),
        ext(COMPONENT, "squish_damage", format!("{:?}", v.squish_damage)),
        ext(COMPONENT, "crit_chance", format!("{:?}", v.crit_chance)),
        ext(
            COMPONENT,
            "age_restrictions",
            v.age_restrictions.to_string(),
        ),
        ext(COMPONENT, "onstation", v.onstation.to_string()),
        ext(
            COMPONENT,
            "req_access",
            ron::to_string(&v.req_access).expect("string lists serialize"),
        ),
        ext(COMPONENT, "allow_custom", v.allow_custom.to_string()),
        ext(
            COMPONENT,
            "max_loaded_items",
            v.max_loaded_items.to_string(),
        ),
    ]);
    out
}

/// A path var (`{ "path": ... }`); empty for `null`.
fn path_var(value: Option<&Value>) -> String {
    value
        .and_then(|v| v.get("path"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// `list("a", "b")` of strings (access lists); anything else is empty.
fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.get("list"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn category((name, icon): (&str, &str)) -> VendCategory {
    VendCategory {
        name: name.into(),
        icon: icon.into(),
    }
}

/// DM `round(x)` with one argument: `floor(x + 0.5)`.
fn dm_round(x: f64) -> u32 {
    (x + 0.5).floor().max(0.0) as u32
}

/// `list(/type = amount, ...)` in key order, a repeated key keeping its first place and its last
/// amount (DM assoc semantics). `None` when not a constant list of paths; `null` is empty.
fn assoc_list(value: Option<&Value>) -> Option<Vec<(String, u32)>> {
    let items = match value {
        None | Some(Value::Null) => return Some(Vec::new()),
        Some(Value::Object(o)) => o.get("list")?.as_array()?,
        _ => return None,
    };
    let mut out: Vec<(String, u32)> = Vec::new();
    for item in items {
        let (key, amount) = match item.get("assoc").and_then(Value::as_array) {
            Some(pair) if pair.len() == 2 => (&pair[0], num(Some(&pair[1]))?),
            // A bare key: DM `null` value, stock 0.
            _ => (item, 0.0),
        };
        let path = key.get("path")?.as_str()?.to_owned();
        let amount = amount.max(0.0) as u32;
        match out.iter_mut().find(|(p, _)| *p == path) {
            Some(entry) => entry.1 = amount,
            None => out.push((path, amount)),
        }
    }
    Some(out)
}

/// `product_categories`: `list(list("name" = ..., "icon" = ..., "products" = list(...)), ...)`.
fn categories(value: &Value) -> Option<Vec<(String, String, Vec<(String, u32)>)>> {
    let mut out = Vec::new();
    for cat in value.get("list")?.as_array()? {
        let (mut name, mut icon, mut products) = (String::new(), String::new(), Vec::new());
        for entry in cat.get("list")?.as_array()? {
            let pair = entry.get("assoc")?.as_array()?;
            let (key, value) = (pair.first()?.as_str()?, pair.get(1)?);
            match key {
                "name" => name = value.as_str()?.to_owned(),
                "icon" => icon = value.as_str()?.to_owned(),
                "products" => products = assoc_list(Some(value))?,
                _ => {}
            }
        }
        out.push((name, icon, products));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(json: Value) -> DmTypes {
        let mut doc = serde_json::json!({ "schema": crate::convert::DM_TYPES_SCHEMA });
        doc["types"] = json;
        serde_json::from_value(doc).unwrap()
    }

    fn path(p: &str) -> Value {
        serde_json::json!({ "path": p })
    }

    fn list(entries: &[(&str, f64)]) -> Value {
        let items: Vec<Value> = entries
            .iter()
            .map(|(p, n)| serde_json::json!({ "assoc": [path(p), n] }))
            .collect();
        serde_json::json!({ "list": items })
    }

    /// tg prices: `custom_price || default_price`; premium `custom_premium_price`, else
    /// `extra_price + custom_price`, else `extra_price`; repeated keys keep the last amount.
    #[test]
    fn records_follow_build_inventory() {
        let t = types(serde_json::json!({
            "/obj/machinery/vending": { "parent": null, "declared_vars": {
                "default_price": 25, "extra_price": 50, "products": { "list": [] } } },
            "/obj/machinery/vending/snack": { "parent": "/obj/machinery/vending", "declared_vars": {
                "default_price": 30.000002,
                "products": list(&[("/obj/item/a", 6.0), ("/obj/item/b", 2.0), ("/obj/item/a", 4.0)]),
                "contraband": list(&[("/obj/item/b", 1.0)]),
                "premium": list(&[("/obj/item/a", 3.0), ("/obj/item/c", 3.0)]) } },
            "/obj/item/a": { "parent": null, "declared_vars": { "custom_price": 10 } },
            "/obj/item/b": { "parent": null, "declared_vars": {} },
            "/obj/item/c": { "parent": null, "declared_vars": { "custom_premium_price": 70 } },
        }));
        let v = vending(&t, "/obj/machinery/vending/snack");
        assert_eq!(v.unresolved, "");
        let got = |l: &[VendProduct]| -> Vec<(String, u32, u32, u16)> {
            l.iter()
                .map(|p| (p.path.clone(), p.amount, p.price, p.category))
                .collect()
        };
        let row = |p: &str, n, price, c| (p.to_owned(), n, price, c);
        assert_eq!(
            got(&v.products),
            [row("/obj/item/a", 4, 10, 0), row("/obj/item/b", 2, 30, 0)]
        );
        assert_eq!(got(&v.contraband), [row("/obj/item/b", 1, 50, 1)]);
        assert_eq!(
            got(&v.premium),
            [row("/obj/item/a", 3, 60, 2), row("/obj/item/c", 3, 70, 2)]
        );
        let names: Vec<&str> = v.categories.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Products", "Contraband", "Premium"]);
        assert_eq!(
            product_closure(&t, ["/obj/machinery/vending/snack"])
                .into_iter()
                .collect::<Vec<_>>(),
            ["/obj/item/a", "/obj/item/b", "/obj/item/c"]
        );
    }

    #[test]
    fn categories_replace_products() {
        let cat = |name: &str, entries: &[(&str, f64)]| {
            serde_json::json!({ "list": [
                { "assoc": ["name", name] },
                { "assoc": ["icon", "x"] },
                { "assoc": ["products", list(entries)] },
            ] })
        };
        let t = types(serde_json::json!({
            "/obj/machinery/vending": { "parent": null, "declared_vars": {
                "default_price": 5,
                "products": list(&[("/obj/item/ignored", 1.0)]),
                "product_categories": { "list": [
                    cat("One", &[("/obj/item/a", 2.0)]),
                    cat("Two", &[("/obj/item/b", 1.0), ("/obj/item/a", 9.0)]),
                ] } } },
            "/obj/item/a": { "parent": null, "declared_vars": {} },
            "/obj/item/b": { "parent": null, "declared_vars": {} },
        }));
        let v = vending(&t, VENDING_ROOT);
        let got: Vec<(&str, u32, u16)> = v
            .products
            .iter()
            .map(|p| (p.path.as_str(), p.amount, p.category))
            .collect();
        assert_eq!(got, [("/obj/item/a", 2, 1), ("/obj/item/b", 1, 1)]);
        assert_eq!(v.categories.len(), 2);
    }

    #[test]
    fn lists_set_at_runtime_are_unresolved() {
        let t = types(serde_json::json!({
            "/obj/machinery/vending": { "parent": null, "declared_vars": { "products": { "list": [] } } },
            "/obj/machinery/vending/autodrobe": { "parent": "/obj/machinery/vending",
                "declared_vars": {}, "procs": ["Initialize"] },
            "/obj/machinery/vending/expr": { "parent": "/obj/machinery/vending",
                "declared_vars": { "products": { "expr": "GLOB.x" } } },
        }));
        assert_eq!(
            vending(&t, "/obj/machinery/vending/autodrobe").unresolved,
            "lists filled in Initialize()"
        );
        assert_eq!(
            vending(&t, "/obj/machinery/vending/expr").unresolved,
            "products"
        );
    }

    /// The vars the game reads past the records: slogans, reply, canister (also in the
    /// closure), tilting, age checks, access; a canister's machine name, an item's price.
    #[test]
    fn behaviour_vars_canisters_and_prices() {
        let t = types(serde_json::json!({
            "/obj/machinery/vending": { "parent": null, "declared_vars": {
                "default_price": 25, "extra_price": 50, "tiltable": 1, "squish_damage": 75,
                "crit_chance": 15, "age_restrictions": 1, "onstation": 1, "allow_custom": 1,
                "refill_canister": null, "products": { "list": [] } } },
            "/obj/machinery/vending/snack": { "parent": "/obj/machinery/vending",
                "declared_vars": {
                "product_slogans": "a;b", "vend_reply": "Bye!", "req_access": { "list": ["kitchen"] },
                "refill_canister": { "path": "/obj/item/vending_refill/snack" },
                "products": list(&[("/obj/item/beer", 2.0)]) } },
            "/obj/machinery/vending/wallmed": { "parent": "/obj/machinery/vending",
                "declared_vars": { "tiltable": 0, "age_restrictions": 0 } },
            "/obj/item": { "parent": null, "declared_vars": { "age_restricted": 0 } },
            "/obj/item/beer": { "parent": "/obj/item", "declared_vars": {
                "age_restricted": 1, "custom_price": 49.6 } },
            "/obj/item/vending_refill": { "parent": "/obj/item", "declared_vars": {
                "machine_name": "Generic" } },
            "/obj/item/vending_refill/snack": { "parent": "/obj/item/vending_refill",
                "declared_vars": { "machine_name": "Getmore" } },
        }));
        let v = vending(&t, "/obj/machinery/vending/snack");
        assert_eq!(
            (v.product_slogans.as_str(), v.vend_reply.as_str()),
            ("a;b", "Bye!")
        );
        assert_eq!(v.refill_canister, "/obj/item/vending_refill/snack");
        assert_eq!(v.req_access, ["kitchen"]);
        assert!(v.tiltable && v.age_restrictions && v.onstation && v.allow_custom);
        assert_eq!((v.squish_damage, v.crit_chance), (75.0, 15.0));
        assert!(v.products[0].age_restricted);
        assert_eq!(v.products[0].price, 50);
        let wall = vending(&t, "/obj/machinery/vending/wallmed");
        assert!(!wall.tiltable && !wall.age_restrictions && wall.refill_canister.is_empty());
        assert!(
            product_closure(&t, ["/obj/machinery/vending/snack"])
                .contains("/obj/item/vending_refill/snack")
        );
        let ext = |path: &str, component: &str| -> Vec<(String, String)> {
            type_fields(&t, path)
                .into_iter()
                .filter_map(|f| match f {
                    Field::Ext {
                        component: c,
                        key,
                        ron,
                    } if c == component => Some((key.to_owned(), ron)),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            ext("/obj/item/vending_refill/snack", REFILL_COMPONENT),
            [("machine_name".to_owned(), "\"Getmore\"".to_owned())]
        );
        assert_eq!(
            ext("/obj/item/beer", PRICE_COMPONENT),
            [("custom_price".to_owned(), "50".to_owned())]
        );
        assert!(ext("/obj/item/vending_refill", PRICE_COMPONENT).is_empty());
    }
}
