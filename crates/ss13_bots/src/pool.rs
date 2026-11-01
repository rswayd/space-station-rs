//! The scheduler: `--jobs N` worker threads take cases from one queue. Each worker keeps one
//! long-lived pooled server holding a level per pooled case (every pooled case runs at most once
//! per server, so its level is fresh); an exclusive case gets a server of its own, started for it
//! and dropped after it.
// shortcut: case levels are loaded with the server and abandoned after their case, not loaded
// when the case starts and dropped after it: maps load only at startup (the game's load passes,
// power nets, atmos and doors set up once). Load per case once `ss13_mapload` and the game can
// set up a template loaded later.

use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Instant;

use crate::bot::IconNames;
use crate::case::{Case, MapSpec};
use crate::logcap::CaseLog;
use crate::run::{CaseResult, run_case};
use crate::server::{POOL_SEED, Server, ServerSpec};

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub jobs: usize,
    pub assets: PathBuf,
    pub source: PathBuf,
    /// Per-case logs (`<out>/logs/<case>.log`) and the staging map.
    pub out: PathBuf,
    /// The staging level's `.dmm` text ([`crate::server::STAGING_MAP`] unless the assets lack
    /// its types, as fixture tests do).
    pub staging: String,
}

/// Default `--jobs`: half the cores (each server ticks atmos; bots and servers share them).
pub fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub workers: usize,
    /// Queue order: exclusive cases first (each starts a server, the slowest part), then the
    /// pooled ones in file order.
    pub order: Vec<usize>,
    /// Pooled cases in level order: `pooled[i]` has the pooled server's level `i`.
    pub pooled: Vec<usize>,
}

pub fn plan(cases: &[Case], jobs: usize) -> Plan {
    let (exclusive, pooled): (Vec<usize>, Vec<usize>) =
        (0..cases.len()).partition(|&i| cases[i].exclusive);
    let order = exclusive.iter().chain(&pooled).copied().collect();
    Plan {
        workers: jobs.clamp(1, cases.len().max(1)),
        order,
        pooled,
    }
}

fn map_arg(map: &MapSpec) -> String {
    match map {
        MapSpec::File(path) => path.display().to_string(),
        MapSpec::Station(name) => name.clone(),
    }
}

/// Runs every case; `progress` sees each result as it finishes. Results come back in case order.
pub fn run_all(
    cases: &[Case],
    config: &RunConfig,
    progress: &(dyn Fn(&CaseResult) + Sync),
) -> anyhow::Result<Vec<CaseResult>> {
    let plan = plan(cases, config.jobs);
    let icons = IconNames::load(&config.assets)?;
    let staging = std::path::absolute(crate::server::write_staging(&config.out, &config.staging)?)?;
    let base = ServerSpec {
        assets: config.assets.clone(),
        source: config.source.clone(),
        staging,
        maps: Vec::new(),
        seed: POOL_SEED,
    };
    let pooled_spec = ServerSpec {
        maps: plan
            .pooled
            .iter()
            .map(|&i| map_arg(&cases[i].map))
            .collect(),
        ..base.clone()
    };
    let level_of = |case: usize| plan.pooled.iter().position(|&p| p == case);
    let queue = Mutex::new(plan.order.iter().copied().collect::<VecDeque<usize>>());
    let (tx, rx) = mpsc::channel::<(usize, CaseResult)>();
    std::thread::scope(|scope| {
        for worker in 0..plan.workers {
            let tx = tx.clone();
            let (queue, icons, base, pooled_spec) = (&queue, &icons, &base, &pooled_spec);
            std::thread::Builder::new()
                .name(format!("bots-{worker}"))
                .spawn_scoped(scope, move || {
                    let mut pooled: Option<Server> = None;
                    loop {
                        let Some(i) = queue.lock().expect("queue").pop_front() else {
                            break;
                        };
                        let case = &cases[i];
                        let start = Instant::now();
                        let log_path = config.out.join("logs").join(format!("{}.log", case.name));
                        let mut result = CaseResult {
                            name: case.name.clone(),
                            file: case.file.clone(),
                            exclusive: case.exclusive,
                            worker,
                            error: None,
                            tests: Vec::new(),
                            secs: 0.0,
                            log: log_path.clone(),
                        };
                        let mut log = match CaseLog::create(&log_path) {
                            Ok(log) => log,
                            Err(e) => {
                                result.error = Some(format!("log {}: {e}", log_path.display()));
                                let _ = tx.send((i, result));
                                continue;
                            }
                        };
                        log.line(&format!("case {} on worker {worker}", case.name));
                        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
                            let mut own;
                            let (server, z) = if case.exclusive {
                                let spec = ServerSpec {
                                    maps: vec![map_arg(&case.map)],
                                    seed: case.seed.unwrap_or(POOL_SEED),
                                    ..base.clone()
                                };
                                own = Server::start(&spec).map_err(|e| format!("server: {e:#}"))?;
                                let z = own.levels[0];
                                (&mut own, z)
                            } else {
                                if pooled.is_none() {
                                    let started = Instant::now();
                                    pooled = Some(
                                        Server::start(pooled_spec)
                                            .map_err(|e| format!("pooled server: {e:#}"))?,
                                    );
                                    log.line(&format!(
                                        "pooled server started in {:.2}s",
                                        started.elapsed().as_secs_f64()
                                    ));
                                }
                                let server = pooled.as_mut().expect("started");
                                let z = server.levels[level_of(i).expect("pooled case")];
                                (server, z)
                            };
                            log.line(&format!("level z={z}"));
                            Ok::<_, String>(run_case(server, z, case, icons, &mut log))
                        }));
                        match outcome {
                            Ok(Ok((error, tests))) => {
                                result.error = error;
                                result.tests = tests;
                            }
                            Ok(Err(e)) => result.error = Some(e),
                            Err(panic) => {
                                let why = panic
                                    .downcast_ref::<String>()
                                    .cloned()
                                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                                    .unwrap_or_else(|| "panic".into());
                                log.line(&format!("PANIC: {why}"));
                                result.error = Some(format!("panicked: {why}"));
                                // Its state is unknown now: the next pooled case gets a new one.
                                pooled = None;
                            }
                        }
                        result.secs = start.elapsed().as_secs_f64();
                        drop(log);
                        let _ = tx.send((i, result));
                    }
                })
                .expect("spawn worker");
        }
        drop(tx);
        let mut results: Vec<Option<CaseResult>> = vec![None; cases.len()];
        for (i, result) in rx {
            progress(&result);
            results[i] = Some(result);
        }
        Ok(results.into_iter().flatten().collect())
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn case(name: &str, extra: &str) -> Case {
        crate::case::parse(
            Path::new(&format!("/c/{name}.case")),
            &format!("map m.dmm\n{extra}\nbot a 1,1\ntest t\n"),
        )
        .unwrap()
    }

    #[test]
    fn exclusive_cases_go_first_and_pooled_get_levels_in_order() {
        let cases = [
            case("a", ""),
            case("b", "exclusive"),
            case("c", ""),
            case("d", "seed 4"),
        ];
        let p = plan(&cases, 8);
        assert_eq!(p.order, vec![1, 3, 0, 2]);
        assert_eq!(p.pooled, vec![0, 2]);
        assert_eq!(p.workers, 4, "no more workers than cases");
        assert_eq!(plan(&cases, 0).workers, 1);
        assert_eq!(plan(&[], 3).workers, 1);
    }
}
