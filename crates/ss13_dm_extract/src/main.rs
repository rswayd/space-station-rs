use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, bail};
use ss13_dm_extract::{ACCEPTANCE_MAP_DIRS, ALL_MAPS_DIR, check_maps, extract, load};

const USAGE: &str = "\
usage:
  ss13-dm-extract [--source <repo>] [--out <file>]
      dump static vars of every type (default out: assets/generated/dm_types.json)
  ss13-dm-extract check-maps [--source <repo>] [--all-maps | --maps <dir>...]
      resolve every type used by .dmm files under the dirs
      (default: _maps/map_files/MetaStation and _maps/map_files/tramstation;
      --all-maps: everything under _maps/map_files)
<repo> defaults to $SS13_SOURCE, then ../BandaStation";

struct Args {
    check_maps: bool,
    source: PathBuf,
    out: PathBuf,
    maps: Vec<PathBuf>,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = std::env::args().skip(1).peekable();
    let check_maps = args.next_if(|a| a == "check-maps").is_some();
    let mut source = None;
    let mut out = None;
    let mut maps = Vec::new();
    let mut all_maps = false;
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--source" => source = Some(PathBuf::from(value()?)),
            "--out" if !check_maps => out = Some(PathBuf::from(value()?)),
            "--maps" if check_maps => maps.push(PathBuf::from(value()?)),
            "--all-maps" if check_maps => all_maps = true,
            _ => bail!("unknown argument {flag}\n{USAGE}"),
        }
    }
    let source = source
        .or_else(|| std::env::var_os("SS13_SOURCE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(env!("SS13_SOURCE")));
    if all_maps {
        maps.push(source.join(ALL_MAPS_DIR));
    } else if maps.is_empty() {
        maps = ACCEPTANCE_MAP_DIRS.iter().map(|d| source.join(d)).collect();
    }
    Ok(Args {
        check_maps,
        source,
        out: out.unwrap_or_else(|| PathBuf::from("assets/generated/dm_types.json")),
        maps,
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let args = parse_args()?;
    let start = Instant::now();
    let env = load(&args.source)?;
    let parsed = start.elapsed();
    let (dump, non_const) = extract(&env);
    eprintln!(
        "parsed {} in {:.1?} (diagnostics: {:?}), extracted {} types",
        env.dme.display(),
        parsed,
        env.diagnostics(),
        dump.types.len()
    );

    if !args.check_maps {
        dump.ensure_complete()?;
        if let Some(dir) = args.out.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let mut json = serde_json::to_string_pretty(&dump)?;
        json.push('\n');
        std::fs::write(&args.out, json)
            .with_context(|| format!("writing {}", args.out.display()))?;
        eprintln!("non-const values by var (all types): {non_const:?}");
        eprintln!("wrote {} in {:.1?}", args.out.display(), start.elapsed());
        return Ok(ExitCode::SUCCESS);
    }

    let check = check_maps(&env.tree, &args.maps)?;
    let mut used_non_const: BTreeMap<&str, usize> = BTreeMap::new();
    for path in &check.used {
        let Some(entry) = dump.types.get(path) else {
            continue;
        };
        for (var, value) in &entry.vars {
            if value.get("expr").is_some() {
                *used_non_const.entry(var).or_default() += 1;
            }
        }
    }
    println!("maps scanned: {}", check.maps.len());
    println!("total types: {}", dump.types.len());
    println!("types used by maps: {}", check.used.len());
    println!("unresolved map types: {}", check.unresolved.len());
    for (path, map) in &check.unresolved {
        println!("  {path} (in {})", map.display());
    }
    let mut by_map: BTreeMap<&std::path::Path, usize> = BTreeMap::new();
    for map in check.unresolved.values() {
        *by_map.entry(map).or_default() += 1;
    }
    if !by_map.is_empty() {
        println!("unresolved types by first map that uses them: {by_map:?}");
    }
    println!("non-const values by var, all types: {non_const:?}");
    println!("non-const values by var, map-used types: {used_non_const:?}");
    println!("run time: {:.1?}", start.elapsed());
    Ok(if check.unresolved.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
