//! Acceptance: the hand-written recipe tables match the dumped DM types. Needs `SS13_ASSETS`
//! (`dm_types.json`).

use std::path::PathBuf;

use serde_json::Value;
use ss13_mapload::atmos_machine::CRYSTALLIZER_RECIPES;
use ss13_mapload::convert::DmTypes;

/// `list(path = number, ...)` as (last path element or whole path, number).
fn assoc(value: Option<&Value>, strip: &str) -> Vec<(String, f64)> {
    value
        .and_then(|v| v.get("list"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let [key, n] = item.get("assoc")?.as_array()?.as_slice() else {
                return None;
            };
            let path = key.get("path")?.as_str()?;
            Some((
                path.strip_prefix(strip).unwrap_or(path).to_owned(),
                n.as_f64()?,
            ))
        })
        .collect()
}

#[test]
#[ignore = "needs generated assets (SS13_ASSETS)"]
fn crystallizer_recipes_match_tg() {
    let assets = std::env::var_os("SS13_ASSETS")
        .map(PathBuf::from)
        .expect("SS13_ASSETS");
    let types = DmTypes::read(&assets.join("dm_types.json")).unwrap();
    let root = "/datum/gas_recipe/crystallizer/";
    let mut paths: Vec<&String> = types.types.keys().filter(|p| p.starts_with(root)).collect();
    paths.sort();
    assert_eq!(paths.len(), CRYSTALLIZER_RECIPES.len());
    for path in paths {
        let num = |var: &str| types.resolved(path, var).and_then(Value::as_f64).unwrap();
        let id = types.resolved(path, "id").and_then(Value::as_str).unwrap();
        let ours = CRYSTALLIZER_RECIPES
            .iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("{path}: no recipe {id}"));
        let name = types.resolved(path, "name").and_then(Value::as_str);
        assert_eq!(Some(ours.name), name, "{path}");
        assert_eq!(ours.min_temp, num("min_temp"), "{path}");
        assert_eq!(ours.max_temp, num("max_temp"), "{path}");
        assert_eq!(ours.energy_release, num("energy_release"), "{path}");
        assert_eq!(ours.dangerous, num("dangerous") != 0.0, "{path}");
        let requirements: Vec<(String, f64)> = ours
            .requirements
            .iter()
            .map(|(g, n)| ((*g).to_owned(), *n))
            .collect();
        assert_eq!(
            requirements,
            assoc(types.resolved(path, "requirements"), "/datum/gas/"),
            "{path}"
        );
        let products: Vec<(String, f64)> = ours
            .products
            .iter()
            .map(|(p, n)| ((*p).to_owned(), f64::from(*n)))
            .collect();
        assert_eq!(
            products,
            assoc(types.resolved(path, "products"), ""),
            "{path}"
        );
    }
}
