//! `ss13-bots`: runs functional test cases (docs/RUNNING.md, "Functional test bots").

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use ss13_bots::{case, logcap, pool, report};

const USAGE: &str = "usage: ss13-bots [--jobs N] [--filter TEXT] [--list] [--out DIR] [--json FILE]
                 [--junit FILE] [--assets DIR] [--source DIR] [PATH...]
  PATH      case files or directories (default tests/cases)
  --jobs    parallel servers (default: half the cores)
  --filter  only cases whose name contains TEXT (repeatable: any)
  --list    print the cases and their tests, run nothing
  --out     logs and reports (default target/ss13-bots)
  --json, --junit   reports (relative paths go under --out)
  --assets  generated assets (default $SS13_ASSETS or assets/generated)
  --source  BYOND checkout for station maps (default $SS13_SOURCE or ../BandaStation)
exit: 0 all passed, 1 a test failed, 2 bad arguments or case files";

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("ss13-bots: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> anyhow::Result<bool> {
    let env = |key: &str, default: &str| {
        PathBuf::from(std::env::var(key).unwrap_or_else(|_| default.to_owned()))
    };
    let mut jobs = pool::default_jobs();
    let mut filters: Vec<String> = Vec::new();
    let mut list = false;
    let mut out = PathBuf::from("target/ss13-bots");
    let mut json_path: Option<PathBuf> = None;
    let mut junit_path: Option<PathBuf> = None;
    let mut assets = env("SS13_ASSETS", "assets/generated");
    let mut source = env("SS13_SOURCE", env!("SS13_SOURCE"));
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| anyhow::anyhow!("{arg} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--jobs" | "-j" => jobs = value()?.parse()?,
            "--filter" => filters.push(value()?),
            "--list" => list = true,
            "--out" => out = value()?.into(),
            "--json" => json_path = Some(value()?.into()),
            "--junit" => junit_path = Some(value()?.into()),
            "--assets" => assets = value()?.into(),
            "--source" => source = value()?.into(),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(true);
            }
            a if a.starts_with('-') => anyhow::bail!("unknown option {a}\n{USAGE}"),
            _ => paths.push(arg.into()),
        }
    }
    if paths.is_empty() {
        paths.push("tests/cases".into());
    }
    let files = case::discover(&paths)?;
    let mut cases = Vec::new();
    for file in files {
        cases.push(case::parse_file(&file).map_err(|e| anyhow::anyhow!("{e}"))?);
    }
    cases.retain(|c| filters.is_empty() || filters.iter().any(|f| c.name.contains(f.as_str())));
    if list {
        for c in &cases {
            let tests: Vec<&str> = c.tests.iter().map(|t| t.name.as_str()).collect();
            println!(
                "{}{}: {}",
                c.name,
                if c.exclusive { " (exclusive)" } else { "" },
                tests.join(", ")
            );
        }
        return Ok(true);
    }
    anyhow::ensure!(!cases.is_empty(), "no cases match");
    logcap::install();
    let config = pool::RunConfig {
        jobs,
        assets,
        source,
        out: out.clone(),
        staging: ss13_bots::server::STAGING_MAP.to_owned(),
    };
    let start = Instant::now();
    let results = pool::run_all(&cases, &config, &|r| {
        println!("{}", report::progress_line(r))
    })?;
    let wall = start.elapsed().as_secs_f64();
    println!("{}", report::summary(&results, wall, jobs));
    if let Some(path) = json_path {
        let path = out.join(path);
        let text = serde_json::to_string_pretty(&report::json(&results, wall, jobs))?;
        report::write(&path, &text)?;
        println!("json report: {}", path.display());
    }
    if let Some(path) = junit_path {
        let path = out.join(path);
        report::write(&path, &report::junit(&results))?;
        println!("junit report: {}", path.display());
    }
    Ok(results.iter().all(|r| r.passed()))
}
