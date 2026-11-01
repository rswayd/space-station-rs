//! The seed suite (`tests/cases`) on generated assets: needs `SS13_ASSETS` (docs/RUNNING.md,
//! "Functional test bots"), hence ignored by default. `SS13_BOTS_JOBS` parallel servers, default
//! 6: several agents run the suite at once on one machine, and timing-sensitive cases flake when
//! the cores are oversubscribed.

use std::path::Path;

use ss13_bots::case;
use ss13_bots::pool::{RunConfig, run_all};

#[test]
#[ignore = "needs generated assets: SS13_ASSETS=... cargo test -p ss13_bots --test seed -- --ignored"]
fn seed_suite_passes() {
    ss13_bots::logcap::install();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let files = case::discover(&[root.join("tests/cases")]).unwrap();
    let cases: Vec<_> = files
        .iter()
        .map(|f| case::parse_file(f).unwrap_or_else(|e| panic!("{e}")))
        .collect();
    let config = RunConfig {
        jobs: std::env::var("SS13_BOTS_JOBS")
            .ok()
            .and_then(|j| j.parse().ok())
            .unwrap_or(6),
        assets: std::env::var("SS13_ASSETS")
            .unwrap_or_else(|_| root.join("assets/generated").display().to_string())
            .into(),
        source: std::env::var("SS13_SOURCE")
            .unwrap_or_else(|_| env!("SS13_SOURCE").into())
            .into(),
        out: Path::new(env!("CARGO_TARGET_TMPDIR")).join("bots_seed"),
        staging: ss13_bots::server::STAGING_MAP.into(),
    };
    let results = run_all(&cases, &config, &|r| {
        eprintln!("{}", ss13_bots::report::progress_line(r))
    })
    .unwrap();
    let failed: Vec<&str> = results
        .iter()
        .filter(|r| !r.passed())
        .map(|r| r.name.as_str())
        .collect();
    assert!(failed.is_empty(), "failed cases: {failed:?}");
    assert!(results.iter().map(|r| r.tests.len()).sum::<usize>() >= 25);
}
