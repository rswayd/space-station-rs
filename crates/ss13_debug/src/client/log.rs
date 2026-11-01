//! The log tail: a tracing layer that keeps the newest lines for the overlay's log panel.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use bevy::log::BoxedLayer;
use bevy::log::tracing::field::{Field, Visit};
use bevy::log::tracing::{Event, Level, Subscriber};
use bevy::log::tracing_subscriber::Layer;
use bevy::log::tracing_subscriber::layer::Context;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use super::{DebugUi, Panel};

/// Lines the tail keeps.
const LINES: usize = 2000;

#[derive(Clone, Debug, PartialEq)]
pub struct LogLine {
    pub level: Level,
    pub target: String,
    pub message: String,
}

type Lines = Arc<Mutex<VecDeque<LogLine>>>;

/// Shared with the layer; the panel copies nothing until it draws.
#[derive(Resource, Clone, Default)]
pub struct LogTail(Lines);

/// For `LogPlugin::custom_layer`: the client passes it when the debug overlay is built in.
/// Lines below the log filter (`RUST_LOG`, default `info`) never reach it.
pub fn log_layer(app: &mut App) -> Option<BoxedLayer> {
    let tail = LogTail::default();
    app.insert_resource(tail.clone());
    Some(Box::new(TailLayer(tail.0)))
}

struct TailLayer(Lines);

impl<S: Subscriber> Layer<S> for TailLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = Message(String::new());
        event.record(&mut visitor);
        let meta = event.metadata();
        let line = LogLine {
            level: *meta.level(),
            target: meta.target().to_owned(),
            message: visitor.0,
        };
        let Ok(mut lines) = self.0.lock() else {
            return;
        };
        if lines.len() == LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }
}

struct Message(String);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }
}

#[derive(Resource)]
pub(super) struct LogPanel {
    /// The most verbose level shown.
    level: Level,
    target: String,
    text: String,
}

impl Default for LogPanel {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            target: String::new(),
            text: String::new(),
        }
    }
}

impl LogPanel {
    fn matches(&self, line: &LogLine) -> bool {
        // tracing orders levels by verbosity: TRACE > DEBUG > INFO > WARN > ERROR.
        line.level <= self.level
            && (self.target.is_empty() || line.target.contains(&self.target))
            && (self.text.is_empty()
                || line
                    .message
                    .to_lowercase()
                    .contains(&self.text.to_lowercase()))
    }
}

pub(super) fn panel(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    mut panel: ResMut<LogPanel>,
    tail: Option<Res<LogTail>>,
) -> Result {
    let mut shown = ui_state.shown(Panel::Log);
    if !shown {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Log")
        .open(&mut shown)
        .default_pos([8.0, 600.0])
        .default_size([620.0, 300.0])
        .show(ctx, |ui| {
            let Some(tail) = &tail else {
                ui.label("The log layer is not installed (LogPlugin::custom_layer).");
                return;
            };
            ui.horizontal(|ui| {
                egui::ComboBox::from_label("level")
                    .selected_text(panel.level.as_str())
                    .show_ui(ui, |ui| {
                        for level in [
                            Level::ERROR,
                            Level::WARN,
                            Level::INFO,
                            Level::DEBUG,
                            Level::TRACE,
                        ] {
                            ui.selectable_value(&mut panel.level, level, level.as_str());
                        }
                    });
                ui.label("target");
                ui.add(egui::TextEdit::singleline(&mut panel.target).desired_width(120.0));
                ui.label("text");
                ui.add(egui::TextEdit::singleline(&mut panel.text).desired_width(120.0));
                if ui.button("Clear").clicked()
                    && let Ok(mut lines) = tail.0.lock()
                {
                    lines.clear();
                }
            });
            let lines: Vec<LogLine> = match tail.0.lock() {
                Ok(lines) => lines.iter().filter(|l| panel.matches(l)).cloned().collect(),
                Err(_) => Vec::new(),
            };
            let row = ui.text_style_height(&egui::TextStyle::Monospace);
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink(false)
                .show_rows(ui, row, lines.len(), |ui, range| {
                    for line in &lines[range] {
                        let color = match line.level {
                            Level::ERROR => egui::Color32::LIGHT_RED,
                            Level::WARN => egui::Color32::YELLOW,
                            Level::INFO => egui::Color32::LIGHT_GRAY,
                            _ => egui::Color32::GRAY,
                        };
                        let text = egui::RichText::new(format!(
                            "{:5} {}: {}",
                            line.level.as_str(),
                            line.target,
                            line.message
                        ))
                        .monospace()
                        .color(color);
                        ui.add(egui::Label::new(text).truncate())
                            .on_hover_text(&line.message);
                    }
                });
        });
    ui_state.set(Panel::Log, shown);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_by_level_target_and_text() {
        let line = |level, target: &str, message: &str| LogLine {
            level,
            target: target.into(),
            message: message.into(),
        };
        let panel = LogPanel {
            level: Level::INFO,
            target: "ss13_net".into(),
            text: "HANDSHAKE".into(),
        };
        assert!(panel.matches(&line(Level::INFO, "ss13_net::client", "handshake accepted")));
        assert!(panel.matches(&line(Level::WARN, "ss13_net::client", "handshake slow")));
        assert!(!panel.matches(&line(Level::DEBUG, "ss13_net::client", "handshake")));
        assert!(!panel.matches(&line(Level::INFO, "ss13_render", "handshake")));
        assert!(!panel.matches(&line(Level::INFO, "ss13_net::client", "disconnected")));
    }
}
