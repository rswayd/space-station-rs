//! Acceptance: every type the game spawns by a constant path (`ss13_mapload::runtime`) has a
//! prototype in the generated assets, so spawning it in the real game does not silently return
//! `None`. Needs `SS13_ASSETS` (`prototypes/*.ron`, `dm_types.json`).

use std::collections::BTreeSet;
use std::path::PathBuf;

use ss13_mapload::convert::DmTypes;

/// The `id`s of the prototype files the converter wrote (`    (id: "<path>", ...`).
fn prototype_ids(dir: &std::path::Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "ron") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for rest in text.split("(id: \"").skip(1) {
            if let Some((id, _)) = rest.split_once('"') {
                ids.insert(id.to_owned());
            }
        }
    }
    ids
}

#[test]
#[ignore = "needs generated assets (SS13_ASSETS)"]
fn runtime_types_have_prototypes() {
    let assets = std::env::var_os("SS13_ASSETS")
        .map(PathBuf::from)
        .expect("SS13_ASSETS");
    let ids = prototype_ids(&assets.join("prototypes"));
    assert!(!ids.is_empty(), "no prototypes in {}", assets.display());
    let types = DmTypes::read(&assets.join("dm_types.json")).unwrap();
    let mut missing = Vec::new();
    for path in ss13_mapload::runtime::runtime_types() {
        if !ids.contains(path) {
            let why = if types.types.contains_key(path) {
                "rerun `ss13-mapload convert`"
            } else {
                "not a DM type in dm_types.json"
            };
            missing.push(format!("{path} ({why})"));
        }
    }
    assert!(
        missing.is_empty(),
        "{} runtime types without a prototype:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}
