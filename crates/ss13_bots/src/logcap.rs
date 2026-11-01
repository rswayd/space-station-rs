//! Per-case log files: the runner's step trace plus every log event raised on the worker thread
//! while the case runs (its server and bots tick on that thread). Transport tasks log from the
//! network runtime's threads and are not captured.

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::log::tracing::{Event, Level, Subscriber, field::Field};
use bevy::log::tracing_subscriber::Layer;
use bevy::log::tracing_subscriber::field::Visit;
use bevy::log::tracing_subscriber::layer::{Context, SubscriberExt};

type Sink = Arc<Mutex<BufWriter<File>>>;

thread_local! {
    static SINK: RefCell<Option<(Sink, Instant)>> = const { RefCell::new(None) };
}

/// The log of the case running on this thread; captures thread log events until dropped.
pub struct CaseLog {
    sink: Sink,
    start: Instant,
}

impl CaseLog {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let sink = Arc::new(Mutex::new(BufWriter::new(File::create(path)?)));
        let start = Instant::now();
        SINK.with(|s| *s.borrow_mut() = Some((sink.clone(), start)));
        Ok(Self { sink, start })
    }

    pub fn line(&mut self, text: &str) {
        let t = self.start.elapsed().as_secs_f64();
        let mut sink = self.sink.lock().expect("log lock");
        let _ = writeln!(sink, "[{t:8.3}] {text}");
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.sink.lock().expect("log lock").flush()
    }
}

impl Drop for CaseLog {
    fn drop(&mut self) {
        let _ = self.flush();
        SINK.with(|s| *s.borrow_mut() = None);
    }
}

struct Capture;

struct Message(String);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn core::fmt::Debug) {
        if field.name() == "message" {
            self.0.insert_str(0, &format!("{value:?}"));
        } else {
            self.0.push_str(&format!(" {}={value:?}", field.name()));
        }
    }
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let ours = meta.target().starts_with("ss13");
        if *meta.level() > Level::INFO || (!ours && *meta.level() > Level::WARN) {
            return;
        }
        SINK.with(|s| {
            let Some((sink, start)) = &*s.borrow() else {
                return;
            };
            let mut message = Message(String::new());
            event.record(&mut message);
            let t = start.elapsed().as_secs_f64();
            let mut sink = sink.lock().expect("log lock");
            let _ = writeln!(
                sink,
                "[{t:8.3}] {} {}: {}",
                meta.level(),
                meta.target(),
                message.0
            );
        });
    }
}

/// Installs the capture as the global subscriber (once per process; later calls do nothing).
pub fn install() {
    let subscriber = bevy::log::tracing_subscriber::registry().with(Capture);
    let _ = bevy::log::tracing::subscriber::set_global_default(subscriber);
}
