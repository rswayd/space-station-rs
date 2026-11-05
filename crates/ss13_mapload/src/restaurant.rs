//! Restaurant data from the DM dump, written by `ss13-mapload convert` to [`RESTAURANT_FILE`]:
//! tg's venues (`/datum/venue/*`, `code/modules/food_and_drinks/restaurant/generic_venues.dm`)
//! and robot customer types (`/datum/customer_data/*`, `restaurant/customers/_customer.dm`,
//! with BandaStation's lines from `modular_bandastation/translations/code/
//! restaurant_customer.dm`), their orders priced as tg's `venue_price` element would price the
//! sold thing, and the name prefixes from `strings/names/*_prefix.txt`. The game
//! (`ss13_game::restaurant`) reads it at startup.

use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::is_under;
use crate::research::{list, path, plain, subtypes, text, texts};

pub const RESTAURANT_FILE: &str = "restaurant.ron";
const VENUE_ROOT: &str = "/datum/venue";
const CUSTOMER_ROOT: &str = "/datum/customer_data";
const PORTAL_ROOT: &str = "/obj/machinery/restaurant_portal";
const SEAT_ROOT: &str = "/obj/structure/holosign/robot_seat";
/// The mob, the cash it pays with, the dirt it trails and its seats (`ss13_game::restaurant`).
pub const ROBOT_CUSTOMER: &str = "/mob/living/basic/robot_customer";
pub const HOLOCHIP: &str = "/obj/item/holochip";
pub const DIRT: &str = "/obj/effect/decal/cleanable/dirt";
pub const RESTAURANT_RUNTIME_TYPES: [&str; 6] = [
    ROBOT_CUSTOMER,
    HOLOCHIP,
    DIRT,
    "/obj/structure/holosign/robot_seat/restaurant",
    "/obj/structure/holosign/robot_seat/bar",
    // A portal built from its board (`build_path`).
    "/obj/machinery/restaurant_portal",
];
/// `FOOD_PRICE_CHEAP` (`/obj/item/clothing/Initialize`), `FOOD_PRICE_EXOTIC`
/// (`/obj/item/toy/crayon/Initialize`).
pub const CLOTHING_PRICE: u32 = 70;
const CRAYON_PRICE: u32 = 550;
const DRINK_ORDER: &str = "/datum/custom_order/reagent/drink";
const SOUP_ORDER: &str = "/datum/custom_order/reagent/soup";
const ICECREAM_ORDER: &str = "/datum/custom_order/icecream";
const MOTH_ORDER: &str = "/datum/custom_order/moth_clothing";

/// BandaStation's bar orders added in `New()` (`modular_bandastation/objects/code/items/
/// consumables/customer_data.dm`, `orderable_objects[VENUE_BAR] |= list(...)`), by the type
/// whose `New()` adds them (subtypes inherit them through `..()`).
const FORK_BAR_ORDERS: [(&str, &str, u32); 8] = [
    (
        "/datum/customer_data/american",
        "/datum/reagent/consumable/ethanol/soundhand",
        5,
    ),
    (
        "/datum/customer_data/american",
        "/datum/reagent/consumable/ethanol/silverhand",
        6,
    ),
    (
        "/datum/customer_data/british/gent",
        "/datum/reagent/consumable/ethanol/pegu_club",
        5,
    ),
    (
        "/datum/customer_data/british",
        "/datum/reagent/consumable/ethanol/brandy_crusta",
        6,
    ),
    (
        "/datum/customer_data/french",
        "/datum/reagent/consumable/ethanol/vampiro",
        4,
    ),
    (
        "/datum/customer_data/japanese",
        "/datum/reagent/consumable/ethanol/rainbow_sky",
        2,
    ),
    (
        "/datum/customer_data/japanese",
        "/datum/reagent/consumable/ethanol/innocent_erp",
        4,
    ),
    (
        "/datum/customer_data/mexican",
        "/datum/reagent/consumable/ethanol/black_blood",
        5,
    ),
];

/// One `/datum/venue` subtype.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct VenueDef {
    pub path: String,
    pub name: String,
    /// `venue_type` (`VENUE_RESTAURANT`, `VENUE_BAR`): the key of the customers' orders.
    pub venue_type: String,
    pub req_access: String,
    pub max_guests: u32,
    /// `min_time_between_visitor`, `max_time_between_visitor`, seconds.
    pub min_visit_secs: f32,
    pub max_visit_secs: f32,
    /// `customer_types`: customer path and weight.
    pub customers: Vec<(String, u32)>,
}

/// What a customer can order (tg `orderable_objects` entries).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OrderKind {
    /// An item of exactly this type.
    #[default]
    Item,
    /// `/datum/custom_order/reagent/drink`: 10 units, mostly the reagent, in a drinking glass.
    Drink,
    /// `/datum/custom_order/reagent/soup`: a 15/20/25 unit serving in a bowl.
    Soup,
    /// `/datum/custom_order/icecream`: random flavours in a cone.
    IceCream,
    /// `/datum/custom_order/moth_clothing`: what the venue's opener wears.
    MothClothing,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct OrderDef {
    pub kind: OrderKind,
    /// The item or reagent type (empty for ice cream and moth clothing).
    pub path: String,
    /// Its `initial(name)`.
    pub name: String,
    /// What it pays (`venue_value`, `glass_price`, the clothing and crayon prices); 0: no
    /// `venue_price` element, so no sale.
    pub price: u32,
    pub weight: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CustomerDef {
    pub path: String,
    /// `total_patience`, seconds.
    pub patience_secs: f32,
    pub found_seat_lines: Vec<String>,
    pub cant_find_seat_lines: Vec<String>,
    pub leave_mad_lines: Vec<String>,
    pub leave_happy_lines: Vec<String>,
    pub wait_for_food_lines: Vec<String>,
    pub friendly_pull_line: String,
    pub first_warning_line: String,
    pub second_warning_line: String,
    pub self_defense_line: String,
    pub wrong_item_line: String,
    pub clothing_sets: Vec<String>,
    /// The lines of its `prefix_file`.
    pub name_prefixes: Vec<String>,
    pub base_icon: String,
    pub base_icon_state: String,
    pub is_unique: bool,
    /// `orderable_objects`: venue type -> weighted orders.
    pub orders: Vec<(String, Vec<OrderDef>)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RestaurantData {
    pub venues: Vec<VenueDef>,
    pub customers: Vec<CustomerDef>,
    /// Portal types -> their venue (`linked_venue`, else their board's `venue_type`).
    #[serde(default)]
    pub portals: Vec<(String, String)>,
    /// Seat types -> their venue (`linked_venue`).
    #[serde(default)]
    pub seats: Vec<(String, String)>,
}

impl RestaurantData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let text = ss13_data::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        ron::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("restaurant data serializes"),
        );
        out.push('\n');
        out
    }
}

fn num(v: Option<&Value>) -> f64 {
    v.and_then(Value::as_f64).unwrap_or(0.0)
}

fn resource(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o
            .get("resource")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

/// What tg's `venue_price` element pays for an item of `item_path`.
pub fn item_price(types: &DmTypes, item_path: &str) -> u32 {
    if is_under(item_path, "/obj/item/clothing") {
        CLOTHING_PRICE
    } else if is_under(item_path, "/obj/item/toy/crayon") {
        CRAYON_PRICE
    } else {
        num(types.resolved(item_path, "venue_value")).max(0.0) as u32
    }
}

fn order(types: &DmTypes, key: &Value, weight: u32) -> Option<OrderDef> {
    let p = path(Some(key))?;
    let name = |p: &str| plain(&text(types.resolved(p, "name")));
    let def = if is_under(&p, "/datum/reagent") {
        let kind = match path(types.resolved(&p, "restaurant_order")).as_deref() {
            Some(SOUP_ORDER) => OrderKind::Soup,
            Some(DRINK_ORDER) | None => OrderKind::Drink,
            Some(_) => return None,
        };
        OrderDef {
            kind,
            name: name(&p),
            price: num(types.resolved(&p, "glass_price")).max(0.0) as u32,
            path: p,
            weight,
        }
    } else if p == ICECREAM_ORDER || p == MOTH_ORDER {
        OrderDef {
            kind: if p == ICECREAM_ORDER {
                OrderKind::IceCream
            } else {
                OrderKind::MothClothing
            },
            weight,
            ..Default::default()
        }
    } else if types.types.contains_key(&p) {
        OrderDef {
            kind: OrderKind::Item,
            name: name(&p),
            price: item_price(types, &p),
            path: p,
            weight,
        }
    } else {
        return None;
    };
    Some(def)
}

fn orders(types: &DmTypes, customer: &str) -> Vec<(String, Vec<OrderDef>)> {
    let mut out: Vec<(String, Vec<OrderDef>)> = list(types.resolved(customer, "orderable_objects"))
        .into_iter()
        .filter_map(|(venue, entries)| {
            let venue = venue.as_str()?.to_owned();
            let entries = list(entries)
                .into_iter()
                .filter_map(|(k, w)| order(types, k, num(w).max(0.0) as u32))
                .collect();
            Some((venue, entries))
        })
        .collect();
    // `|=` adds only what is not there yet, to the bar's list (created if missing).
    for (owner, reagent, weight) in FORK_BAR_ORDERS {
        if !is_under(customer, owner) || !types.types.contains_key(reagent) {
            continue;
        }
        let key = serde_json::json!({ "path": reagent });
        let Some(def) = order(types, &key, weight) else {
            continue;
        };
        let bar = match out.iter().position(|(v, _)| v == "Bar Venue") {
            Some(i) => &mut out[i].1,
            None => {
                out.push(("Bar Venue".to_owned(), Vec::new()));
                &mut out.last_mut().expect("just pushed").1
            }
        };
        if !bar.iter().any(|o| o.path == def.path) {
            bar.push(def);
        }
    }
    out
}

fn prefixes(source: &Path, file: &str) -> Vec<String> {
    std::fs::read_to_string(source.join(file))
        .map(|t| {
            t.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Every venue and customer type; `source` is the DM checkout (the prefix files).
pub fn resolve(types: &DmTypes, source: &Path) -> RestaurantData {
    let venues = subtypes(types, VENUE_ROOT)
        .into_iter()
        .map(|p| {
            let get = |var: &str| types.resolved(p, var);
            VenueDef {
                path: p.to_owned(),
                name: text(get("name")),
                venue_type: text(get("venue_type")),
                req_access: text(get("req_access")),
                max_guests: num(get("max_guests")) as u32,
                min_visit_secs: (num(get("min_time_between_visitor")) / 10.0) as f32,
                max_visit_secs: (num(get("max_time_between_visitor")) / 10.0) as f32,
                customers: list(get("customer_types"))
                    .into_iter()
                    .filter_map(|(k, w)| Some((path(Some(k))?, num(w).max(0.0) as u32)))
                    .collect(),
            }
        })
        .collect();
    let customers = subtypes(types, CUSTOMER_ROOT)
        .into_iter()
        .map(|p| {
            let get = |var: &str| types.resolved(p, var);
            CustomerDef {
                path: p.to_owned(),
                patience_secs: (num(get("total_patience")) / 10.0) as f32,
                found_seat_lines: texts(get("found_seat_lines")),
                cant_find_seat_lines: texts(get("cant_find_seat_lines")),
                leave_mad_lines: texts(get("leave_mad_lines")),
                leave_happy_lines: texts(get("leave_happy_lines")),
                wait_for_food_lines: texts(get("wait_for_food_lines")),
                friendly_pull_line: text(get("friendly_pull_line")),
                first_warning_line: text(get("first_warning_line")),
                second_warning_line: text(get("second_warning_line")),
                self_defense_line: text(get("self_defense_line")),
                wrong_item_line: text(get("wrong_item_line")),
                clothing_sets: texts(get("clothing_sets")),
                name_prefixes: prefixes(source, &text(get("prefix_file"))),
                base_icon: resource(get("base_icon")),
                base_icon_state: text(get("base_icon_state")),
                is_unique: num(get("is_unique")) != 0.0,
                orders: orders(types, p),
            }
        })
        .collect();
    let venue_paths: Vec<&str> = subtypes(types, VENUE_ROOT);
    let linked = |root: &str| -> Vec<(String, String)> {
        let mut paths: Vec<&String> = types.types.keys().filter(|p| is_under(p, root)).collect();
        paths.sort();
        paths
            .into_iter()
            .filter_map(|p| {
                let venue = path(types.resolved(p, "linked_venue")).or_else(|| {
                    let board = path(types.resolved(p, "circuit"))?;
                    path(types.resolved(&board, "venue_type"))
                })?;
                venue_paths
                    .contains(&venue.as_str())
                    .then(|| (p.clone(), venue))
            })
            .collect()
    };
    RestaurantData {
        venues,
        customers,
        portals: linked(PORTAL_ROOT),
        seats: linked(SEAT_ROOT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/datum/venue": { "parent": "/datum", "declared_vars": { "name": "unnamed venue",
            "venue_type": "Restaurant Venue", "req_access": "kitchen", "max_guests": 6,
            "min_time_between_visitor": 600, "max_time_between_visitor": 900,
            "customer_types": null } },
        "/datum/venue/bar": { "parent": "/datum/venue", "declared_vars": { "name": "bar",
            "venue_type": "Bar Venue", "req_access": "bar", "min_time_between_visitor": 400,
            "max_time_between_visitor": 600, "customer_types": { "list": [
                { "assoc": [{ "path": "/datum/customer_data/american" }, 50] } ] } } },
        "/datum/customer_data": { "parent": "/datum", "declared_vars": {
            "total_patience": 6000, "wrong_item_line": "Нет.", "prefix_file": "missing.txt",
            "base_icon": { "resource": "icons/mob/simple/tourists.dmi" },
            "base_icon_state": "amerifat", "is_unique": 0,
            "clothing_sets": { "list": ["amerifat_clothes"] },
            "orderable_objects": { "list": [] } } },
        "/datum/customer_data/american": { "parent": "/datum/customer_data", "declared_vars": {
            "found_seat_lines": { "list": ["Сяду тут."] },
            "orderable_objects": { "list": [
                { "assoc": ["Restaurant Venue", { "list": [
                    { "assoc": [{ "path": "/obj/item/food/burger/plain" }, 25] },
                    { "assoc": [{ "path": "/datum/custom_order/icecream" }, 14] },
                    { "assoc": [{ "path": "/datum/reagent/consumable/nutriment/soup/stew" }, 3] }
                ] }] },
                { "assoc": ["Bar Venue", { "list": [
                    { "assoc": [{ "path": "/datum/reagent/consumable/ethanol/beer" }, 25] }
                ] }] } ] } } },
        "/datum/reagent": { "parent": "/datum", "declared_vars": { "name": "",
            "restaurant_order": { "path": "/datum/custom_order/reagent/drink" },
            "glass_price": null } },
        "/datum/reagent/consumable/ethanol/beer": { "parent": "/datum/reagent",
            "declared_vars": { "name": "Beer", "glass_price": 30 } },
        "/datum/reagent/consumable/ethanol/soundhand": { "parent": "/datum/reagent",
            "declared_vars": { "name": "Soundhand", "glass_price": 90 } },
        "/datum/reagent/consumable/nutriment/soup/stew": { "parent": "/datum/reagent",
            "declared_vars": { "name": "Stew", "glass_price": 220,
            "restaurant_order": { "path": "/datum/custom_order/reagent/soup" } } },
        "/obj/item/food/burger/plain": { "parent": "/datum", "declared_vars": {
            "name": "plain burger", "venue_value": 70 } },
        "/obj/item/circuitboard/machine/restaurant_portal": { "parent": "/datum",
            "declared_vars": { "venue_type": { "path": "/datum/venue/bar" } } },
        "/obj/machinery/restaurant_portal": { "parent": "/datum", "declared_vars": {
            "linked_venue": null,
            "circuit": { "path": "/obj/item/circuitboard/machine/restaurant_portal" } } },
        "/obj/structure/holosign/robot_seat": { "parent": "/datum", "declared_vars": {
            "linked_venue": { "path": "/datum/venue" } } },
        "/obj/structure/holosign/robot_seat/bar": {
            "parent": "/obj/structure/holosign/robot_seat", "declared_vars": {
            "linked_venue": { "path": "/datum/venue/bar" } } }
    } }"##;

    #[test]
    fn venues_customers_and_prices() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let data = resolve(&types, Path::new("/nonexistent"));
        assert_eq!(data.venues.len(), 1);
        let bar = &data.venues[0];
        assert_eq!(
            (bar.name.as_str(), bar.req_access.as_str(), bar.max_guests),
            ("bar", "bar", 6)
        );
        assert_eq!((bar.min_visit_secs, bar.max_visit_secs), (40.0, 60.0));
        assert_eq!(
            bar.customers,
            [("/datum/customer_data/american".into(), 50)]
        );
        let american = &data.customers[0];
        assert_eq!(american.patience_secs, 600.0);
        assert_eq!(american.wrong_item_line, "Нет.");
        assert_eq!(american.base_icon, "icons/mob/simple/tourists.dmi");
        let food = &american.orders[0];
        assert_eq!(food.0, "Restaurant Venue");
        assert_eq!(food.1[0].kind, OrderKind::Item);
        assert_eq!(
            (food.1[0].name.as_str(), food.1[0].price),
            ("plain burger", 70)
        );
        assert_eq!(food.1[1].kind, OrderKind::IceCream);
        assert_eq!((food.1[2].kind, food.1[2].price), (OrderKind::Soup, 220));
        // The fork's soundhand is added to the bar list in `New()`.
        let drinks: Vec<&str> = american.orders[1]
            .1
            .iter()
            .map(|o| o.name.as_str())
            .collect();
        assert_eq!(drinks, ["Beer", "Soundhand"]);
        assert_eq!(american.orders[1].1[0].kind, OrderKind::Drink);
        // The bare portal takes its board's venue; the base seat's `/datum/venue` is none.
        assert_eq!(
            data.portals,
            [(
                "/obj/machinery/restaurant_portal".into(),
                "/datum/venue/bar".into()
            )]
        );
        assert_eq!(
            data.seats,
            [(
                "/obj/structure/holosign/robot_seat/bar".into(),
                "/datum/venue/bar".into()
            )]
        );
        let back: RestaurantData = ron::from_str(&data.to_ron()).unwrap();
        assert_eq!(back, data);
    }
}
