//! Parses real maps from the BYOND checkout (`SS13_SOURCE`, default `../BandaStation`).
//! Ignored by default: they need the checkout on disk. Run with `cargo test -p ss13_assets -- --ignored`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ss13_assets::dmm::DmmMap;

fn source() -> PathBuf {
    std::env::var_os("SS13_SOURCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("SS13_SOURCE")))
}

fn collect_dmm(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_dmm(&path, out);
        } else if path.extension().is_some_and(|e| e == "dmm") {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "needs the BYOND checkout"]
fn every_map_parses() {
    let mut files = Vec::new();
    collect_dmm(&source().join("_maps"), &mut files);
    assert!(
        !files.is_empty(),
        "no .dmm found under {}",
        source().display()
    );
    let start = Instant::now();
    let (mut bytes, mut cells, mut failures) = (0u64, 0usize, Vec::new());
    for f in &files {
        bytes += std::fs::metadata(f).unwrap().len();
        match DmmMap::from_file(f) {
            Ok(m) => cells += m.grid.len(),
            Err(e) => failures.push(format!("{}: {e}", f.display())),
        }
    }
    eprintln!(
        "parsed {} maps, {:.1} MB, {cells} cells in {:?}",
        files.len(),
        bytes as f64 / 1e6,
        start.elapsed()
    );
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Expected values were counted independently with a regex script over the same files.
fn check(rel: &str, size: (u32, u32, u32), prefabs: usize, cell: Option<((u32, u32, u32), &str)>) {
    let m = DmmMap::from_file(source().join(rel)).unwrap();
    assert_eq!(m.key_len, 3, "{rel}");
    assert_eq!(m.size, size, "{rel}");
    assert_eq!(m.prefabs.len(), prefabs, "{rel}");
    assert!(
        m.grid.iter().all(|&i| i != ss13_assets::dmm::NO_PREFAB),
        "{rel} has holes"
    );
    for p in &m.prefabs {
        let last = p.objects.last().map(|o| o.path.as_str()).unwrap_or("");
        assert!(
            last.starts_with("/area"),
            "{rel} prefab {} ends with {last}",
            p.key
        );
    }
    if let Some(((x, y, z), key)) = cell {
        assert_eq!(m.get(x, y, z).unwrap().key, key, "{rel} ({x},{y},{z})");
    }
}

#[test]
#[ignore = "needs the BYOND checkout"]
fn metastation() {
    check(
        "_maps/map_files/MetaStation/MetaStation.dmm",
        (255, 255, 1),
        8760,
        Some(((100, 128, 1), "vpg")),
    );
}

#[test]
#[ignore = "needs the BYOND checkout"]
fn tramstation() {
    check(
        "_maps/map_files/tramstation/tramstation.dmm",
        (255, 255, 2),
        8699,
        Some(((100, 1, 1), "vXM")),
    );
}

#[test]
#[ignore = "needs the BYOND checkout"]
fn deltastation() {
    check(
        "_maps/map_files/Deltastation/DeltaStation2.dmm",
        (255, 255, 1),
        11398,
        None,
    );
}

#[test]
#[ignore = "needs the BYOND checkout"]
fn cyberiad() {
    check(
        "_maps/map_files/Cyberiad/Cyberiad.dmm",
        (255, 255, 2),
        13639,
        None,
    );
}
