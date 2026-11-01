//! `ss13-import dmi [--source <repo root>] [--out <dir>] [--types <dm_types.json>]`
//!
//! Source defaults to `$SS13_SOURCE`, then `../BandaStation`; out defaults to
//! `assets/generated`. With `--types` (the output of `ss13-dm-extract`) the `.dmi` files drawn
//! by the maps under `<source>/_maps/map_files` are packed first, most used first
//! (`ss13_assets::usage`), so a client that loads atlas pages on demand loads few of them, and
//! the GAGS icons every type with `greyscale_config` + `greyscale_colors` uses are rendered into
//! the atlases too (`ss13_assets::gags`); without `--types` there are none.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use ss13_assets::import::import_dmi;
use ss13_assets::usage::{DmTypes, DmiUsage, find_files, map_usage};

const USAGE: &str =
    "usage: ss13-import dmi [--source <repo root>] [--out <dir>] [--types <dm_types.json>]";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("dmi") {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    let mut source =
        PathBuf::from(std::env::var("SS13_SOURCE").unwrap_or_else(|_| env!("SS13_SOURCE").into()));
    let mut out = PathBuf::from("assets/generated");
    let mut types = None;
    while let Some(flag) = args.next() {
        let Some(value) = args.next() else {
            eprintln!("{flag} needs a value\n{USAGE}");
            return ExitCode::FAILURE;
        };
        match flag.as_str() {
            "--source" => source = value.into(),
            "--out" => out = value.into(),
            "--types" => types = Some(PathBuf::from(value)),
            _ => {
                eprintln!("unknown argument {flag:?}\n{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }

    let start = Instant::now();
    let types = match types.map(|t| read_types(&t)).transpose() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let usage = match types.as_ref().map(|t| usage(&source, t)).transpose() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("map usage failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    if usage.is_none() {
        eprintln!(
            "no --types: atlas pages follow .dmi path order (more pages per client), no GAGS icons"
        );
    }
    let gags = types.as_ref().map(|t| t.gags(&source));
    if let Some(g) = &gags {
        println!(
            "GAGS: {} configs, {} icons to render",
            g.library.len(),
            g.keys.len()
        );
    }
    let report = match import_dmi(&source, &out, usage.as_ref(), gags.as_ref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("import failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    for f in &report.failures {
        eprintln!("skipped {}: {}", f.dmi, f.error);
    }
    println!(
        "{} files ({} GAGS icons), {} states, {} icons -> {} stored frames in {} atlases at {} \
         ({:.2?}), {} failures; {} and {} attribution files",
        report.files,
        report.gags,
        report.states,
        report.icons,
        report.stored_frames,
        report.atlases,
        out.display(),
        start.elapsed(),
        report.failures.len(),
        ss13_assets::import::ICON_SOURCES_FILE,
        report.attributions,
    );
    if report.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn read_types(path: &std::path::Path) -> Result<DmTypes, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    DmTypes::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn usage(source: &std::path::Path, types: &DmTypes) -> Result<DmiUsage, String> {
    let dir = source.join("_maps/map_files");
    let maps = find_files(&dir, "dmm").map_err(|e| format!("{}: {e}", dir.display()))?;
    let usage = map_usage(&dir, &maps, types).map_err(|e| e.to_string())?;
    println!(
        "map usage: {} .dmi files drawn by {} maps",
        usage.len(),
        maps.len()
    );
    Ok(usage)
}
