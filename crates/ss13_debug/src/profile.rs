//! Per-system time, summed in process: a build with the binary's `profile` feature (Bevy's
//! `trace`: a span per system run) and `SS13_PROFILE=1` prints every 10 s which systems took the
//! time: per system the time per frame on average, its share of the frame, its slowest run, and
//! the frame count and average frame time of the window. Nothing is written to disk, unlike
//! `trace_chrome` (gigabytes a minute for the client).
//!
//! `SS13_PROFILE_TOP` (default 25) sets how many systems a report lists.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::log::BoxedLayer;
use bevy::log::tracing::Subscriber;
use bevy::log::tracing::field::{Field, Visit};
use bevy::log::tracing::span::{Attributes, Id};
use bevy::log::tracing_subscriber::Layer;
use bevy::log::tracing_subscriber::layer::Context;
use bevy::log::tracing_subscriber::registry::LookupSpan;
use bevy::prelude::App;

/// Seconds between reports.
const WINDOW: Duration = Duration::from_secs(10);
/// The schedule whose runs count as frames (server ticks, client frames).
const FRAME_SCHEDULE: &str = "Main";

/// A span this layer times: a system run, or a schedule run (only `Main` is used).
#[derive(Clone)]
enum Timed {
    System(Arc<str>),
    Schedule(bool),
}

struct Entered(Instant);

#[derive(Clone, Copy, Default)]
struct Stat {
    total: Duration,
    runs: u64,
    max: Duration,
}

#[derive(Default)]
struct Totals {
    systems: HashMap<Arc<str>, Stat>,
    frames: u64,
    frame_time: Duration,
    since: Option<Instant>,
}

struct ProfileLayer {
    totals: Mutex<Totals>,
    top: usize,
}

/// `LogPlugin::custom_layer`: the profiling layer when `SS13_PROFILE` is set.
pub fn profile_layer(_app: &mut App) -> Option<BoxedLayer> {
    layer()
}

/// The profiling layer when `SS13_PROFILE` is set, for a subscriber built by hand (the bot
/// runner's). With several apps in one process (bots and their server), `Main` counts every
/// app's frames: read the per-run column there.
pub fn layer() -> Option<BoxedLayer> {
    std::env::var_os("SS13_PROFILE")?;
    let top = std::env::var("SS13_PROFILE_TOP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25);
    eprintln!("profile: on (needs a `--features profile` build for system spans)");
    Some(Box::new(ProfileLayer {
        totals: Mutex::new(Totals::default()),
        top,
    }))
}

struct NameField(Option<String>);

impl Visit for NameField {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "name" {
            self.0 = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "name" {
            self.0 = Some(format!("{value:?}"));
        }
    }
}

impl<S> Layer<S> for ProfileLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let kind = attrs.metadata().name();
        if kind != "system" && kind != "schedule" {
            return;
        }
        let mut name = NameField(None);
        attrs.record(&mut name);
        let Some(name) = name.0 else {
            return;
        };
        let timed = if kind == "system" {
            Timed::System(name.into())
        } else {
            Timed::Schedule(name == FRAME_SCHEDULE)
        };
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(timed);
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut ext = span.extensions_mut();
        if ext.get_mut::<Timed>().is_some() {
            ext.replace(Entered(Instant::now()));
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let (timed, start) = {
            let ext = span.extensions();
            match (ext.get::<Timed>(), ext.get::<Entered>()) {
                (Some(t), Some(e)) => (t.clone(), e.0),
                _ => return,
            }
        };
        let took = start.elapsed();
        let Ok(mut totals) = self.totals.lock() else {
            return;
        };
        match timed {
            Timed::System(name) => {
                let stat = totals.systems.entry(name).or_default();
                stat.total += took;
                stat.runs += 1;
                stat.max = stat.max.max(took);
            }
            Timed::Schedule(true) => {
                totals.frames += 1;
                totals.frame_time += took;
                let now = Instant::now();
                let since = *totals.since.get_or_insert(now);
                if now - since >= WINDOW {
                    eprint!("{}", report(&totals, now - since, self.top));
                    *totals = Totals {
                        since: Some(now),
                        ..Totals::default()
                    };
                }
            }
            Timed::Schedule(false) => {}
        }
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// The report of one window: the frames, then the top systems by total time.
fn report(totals: &Totals, window: Duration, top: usize) -> String {
    let frames = totals.frames.max(1);
    let frame_avg = ms(totals.frame_time) / frames as f64;
    let systems_ms = totals.systems.values().map(|s| ms(s.total)).sum::<f64>() / frames as f64;
    let mut out = format!(
        "profile: {} frames in {:.1} s, Main {:.3} ms/frame on average; {} systems sum to \
         {:.3} ms/frame (render-world systems included)\n",
        totals.frames,
        window.as_secs_f64(),
        frame_avg,
        totals.systems.len(),
        systems_ms
    );
    let mut systems: Vec<(&Arc<str>, &Stat)> = totals.systems.iter().collect();
    systems.sort_by_key(|s| std::cmp::Reverse(s.1.total));
    let _ = writeln!(
        out,
        "profile: {:>9} {:>6} {:>9} {:>9} {:>7}  system",
        "ms/frame", "share", "avg ms", "max ms", "runs"
    );
    for (name, stat) in systems.into_iter().take(top) {
        let per_frame = ms(stat.total) / frames as f64;
        let share = if frame_avg > 0.0 {
            100.0 * per_frame / frame_avg
        } else {
            0.0
        };
        let _ = writeln!(
            out,
            "profile: {per_frame:>9.4} {share:>5.1}% {:>9.3} {:>9.3} {:>7}  {name}",
            ms(stat.total) / stat.runs.max(1) as f64,
            ms(stat.max),
            stat.runs
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_sorts_by_total_and_divides_by_frames() {
        let mut totals = Totals {
            frames: 10,
            frame_time: Duration::from_millis(50),
            ..Totals::default()
        };
        let stat = |ms: u64, max: u64| Stat {
            total: Duration::from_millis(ms),
            runs: 10,
            max: Duration::from_millis(max),
        };
        totals.systems.insert("small".into(), stat(5, 1));
        totals.systems.insert("big".into(), stat(20, 4));
        let text = report(&totals, Duration::from_secs(10), 5);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].contains("10 frames"), "{text}");
        assert!(lines[0].contains("5.000 ms/frame"), "{text}");
        // `big`: 20 ms over 10 frames, 40% of the 5 ms frame.
        assert!(
            lines[2].ends_with("big") && lines[2].contains("2.0000"),
            "{text}"
        );
        assert!(lines[2].contains("40.0%"), "{text}");
        assert!(lines[3].ends_with("small"), "{text}");
    }
}
