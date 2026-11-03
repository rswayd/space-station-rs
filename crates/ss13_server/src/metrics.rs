//! Prometheus metrics: `SS13_METRICS_ADDR=127.0.0.1:9100` serves `GET /metrics` in the
//! Prometheus text format. Values go through the `metrics` facade (any code may add its own with
//! `metrics::counter!`/`gauge!`/`histogram!`); `metrics-exporter-prometheus` only keeps and
//! renders them, the HTTP side is a plain blocking listener on its own thread. Unset, no recorder
//! is installed and nothing here runs.
//!
//! Every tick: its duration (`ss13_tick_seconds`), clients, entities, replication (pass time,
//! traffic, spawn/despawn totals), the round state and duration; every atmos fire: its time, each
//! level's time, active tiles, excited groups and superconducting tiles by z, and the machinery
//! time. Histograms are rendered as summaries (quantiles 0, 0.5, 0.9, 0.95, 0.99, 0.999, 1).

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// The installed recorder; the server keeps it across round reboots (one per process).
static HANDLE: OnceLock<Option<PrometheusHandle>> = OnceLock::new();

/// Requests slower than this to send their request line are dropped.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

pub struct MetricsPlugin;

impl Plugin for MetricsPlugin {
    fn build(&self, app: &mut App) {
        if HANDLE.get_or_init(install).is_none() {
            return;
        }
        app.init_resource::<TickStart>()
            .add_systems(First, start_tick)
            .add_systems(Last, (end_tick, collect));
    }
}

/// The recorder and the listener, when `SS13_METRICS_ADDR` is set.
fn install() -> Option<PrometheusHandle> {
    let addr: SocketAddr = match std::env::var("SS13_METRICS_ADDR").ok()?.parse() {
        Ok(addr) => addr,
        Err(e) => {
            error!("SS13_METRICS_ADDR: {e}; metrics are off");
            return None;
        }
    };
    let handle = match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => handle,
        Err(e) => {
            error!("metrics recorder: {e}; metrics are off");
            return None;
        }
    };
    let listener = match TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(e) => {
            error!("metrics on {addr}: {e}; metrics are off");
            return None;
        }
    };
    let served = handle.clone();
    let spawned = std::thread::Builder::new()
        .name("metrics".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                serve(stream, &served);
            }
        });
    if let Err(e) = spawned {
        error!("metrics thread: {e}; metrics are off");
        return None;
    }
    info!("metrics: http://{addr}/metrics");
    Some(handle)
}

/// One request: `GET /metrics` gets the rendered text, anything else a 404.
fn serve(mut stream: TcpStream, handle: &PrometheusHandle) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let (status, body) = if response_for(&line) {
        ("200 OK", handle.render())
    } else {
        ("404 Not Found", "not found\n".to_owned())
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
}

/// Whether a request line asks for the metrics (`GET /metrics`, with or without a query).
fn response_for(request_line: &str) -> bool {
    let mut parts = request_line.split_whitespace();
    parts.next() == Some("GET")
        && parts
            .next()
            .is_some_and(|path| path == "/metrics" || path.starts_with("/metrics?"))
}

#[derive(Resource, Default)]
struct TickStart(Option<Instant>);

fn start_tick(mut start: ResMut<TickStart>) {
    start.0 = Some(Instant::now());
}

fn end_tick(start: Res<TickStart>) {
    if let Some(start) = start.0 {
        histogram!("ss13_tick_seconds").record(start.elapsed().as_secs_f64());
    }
}

fn collect(world: &mut World) {
    gauge!("ss13_entities").set(world.entities().count_spawned() as f64);
    if let Some(stats) = world.get_resource::<ss13_net::server::ReplicationStats>() {
        gauge!("ss13_clients").set(stats.clients.len() as f64);
        histogram!("ss13_replication_seconds").record(stats.replicate_time.as_secs_f64());
        let bytes: f64 = stats.clients.values().map(|t| t.bytes_per_sec).sum();
        gauge!("ss13_network_sent_bytes_per_second").set(bytes);
        counter!("ss13_replication_spawned_total").absolute(stats.spawned_total);
        counter!("ss13_replication_despawned_total").absolute(stats.despawned_total);
        counter!("ss13_replication_appearances_sent_total").absolute(stats.appearances_sent_total);
    }
    if let Some(ticker) = world.get_resource::<ss13_game::round::Ticker>() {
        let now = world.resource::<Time>().elapsed_secs_f64();
        gauge!("ss13_round_state").set(ticker.state() as u8 as f64);
        gauge!("ss13_round_duration_seconds").set(ticker.round_duration(now));
    }
    let fired = world
        .get_resource::<ss13_atmos::AirTick>()
        .is_some_and(|t| t.due);
    if !fired {
        return;
    }
    if let Some(atmos) = world.get_resource::<ss13_atmos::Atmos>() {
        histogram!("ss13_atmos_fire_seconds").record(atmos.last_fire.as_secs_f64());
        for &(z, took) in &atmos.last_fire_levels {
            gauge!("ss13_atmos_level_fire_seconds", "z" => z.to_string()).set(took.as_secs_f64());
        }
        for (z, level) in atmos.levels() {
            let z = z.to_string();
            gauge!("ss13_atmos_active_tiles", "z" => z.clone()).set(level.active_count() as f64);
            gauge!("ss13_atmos_excited_groups", "z" => z.clone()).set(level.group_count() as f64);
            gauge!("ss13_atmos_superconducting_tiles", "z" => z)
                .set(level.conducting_count() as f64);
        }
    }
    if let Some(report) = world.get_resource::<ss13_game::atmos_machinery::AtmosMachineryReport>() {
        histogram!("ss13_atmos_machinery_seconds").record(report.last_machinery.as_secs_f64());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_get_metrics_is_served() {
        assert!(response_for("GET /metrics HTTP/1.1\r\n"));
        assert!(response_for("GET /metrics?x=1 HTTP/1.1\r\n"));
        assert!(!response_for("GET / HTTP/1.1\r\n"));
        assert!(!response_for("POST /metrics HTTP/1.1\r\n"));
        assert!(!response_for("GET /metricsfoo HTTP/1.1\r\n"));
        assert!(!response_for(""));
    }
}
