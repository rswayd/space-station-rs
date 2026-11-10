//! The overlay's windows. Each is a system in `EguiPrimaryContextPass` that returns early when
//! its panel is hidden.

use std::io::Write as _;

use bevy::diagnostic::{
    DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use ss13_game::fullscreen_client::FullscreenHud;
use ss13_game::hud::{ClientColour, FullscreenHudData, FullscreenScreen, ScreenCategory};
use ss13_game::prediction::Prediction;
use ss13_net::client::{LightBlocks, LocalControl, NetClientState, NetEntity, NetEntityMap};
use ss13_net::debug::client::SendDebugRequest;
use ss13_net::tap::NetLinkStats;
use ss13_render::lighting::LightingStats;
use ss13_render::{ActiveZ, IconAtlas, WorldCamera};
use ss13_shared::debug::{DebugRequest, TileReport};
use ss13_shared::{Appearance, Facing, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::client::ClientWindows;

use super::{
    DebugUi, DevTools, FrameTimes, Inspector, NetPanel, Panel, ServerData, UiTracker, export_dir,
};

/// Rows of the packet log drawn per frame at most (the log itself keeps more).
const LOG_ROWS: usize = 2000;

/// The window of `panel`, shown while the panel is on; closing the window turns it off.
fn window(
    contexts: &mut EguiContexts,
    ui_state: &mut DebugUi,
    panel: Panel,
    size: [f32; 2],
    content: impl FnOnce(&mut egui::Ui),
) -> Result {
    let mut shown = ui_state.shown(panel);
    if !shown {
        return Ok(());
    }
    egui::Window::new(panel.title())
        .open(&mut shown)
        .default_pos(default_pos(panel))
        .default_size(size)
        .show(contexts.ctx_mut()?, content);
    ui_state.set(panel, shown);
    Ok(())
}

/// Where a panel first opens, so the usual pairs (network + entities, perf + tile) do not
/// overlap on the 1216x960 map area.
fn default_pos(panel: Panel) -> [f32; 2] {
    match panel {
        Panel::Network | Panel::Perf | Panel::Ui => [8.0, 110.0],
        Panel::Entities | Panel::Server => [740.0, 110.0],
        Panel::Tile => [740.0, 420.0],
        Panel::Prediction => [740.0, 760.0],
        Panel::Log => [8.0, 600.0],
        Panel::ScreenFx => [740.0, 420.0],
    }
}

pub(super) fn menu(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    #[cfg(not(target_arch = "wasm32"))] mut recorder: ResMut<super::record::Recorder>,
) -> Result {
    egui::Window::new("ss13 debug")
        .default_pos([8.0, 8.0])
        .resizable(false)
        .show(contexts.ctx_mut()?, |ui| {
            ui.horizontal_wrapped(|ui| {
                for panel in Panel::ALL {
                    let mut shown = ui_state.shown(panel);
                    if ui.toggle_value(&mut shown, panel.title()).changed() {
                        ui_state.set(panel, shown);
                    }
                }
            });
            #[cfg(not(target_arch = "wasm32"))]
            super::record::button(ui, &mut recorder, export_dir());
            ui.small("F9 / ` closes the overlay");
        });
    Ok(())
}

pub(super) fn perf(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    diagnostics: Res<DiagnosticsStore>,
    frames: Res<FrameTimes>,
    atlas: Option<Res<IconAtlas>>,
    lighting: Option<Res<LightingStats>>,
    map: Res<NetEntityMap>,
    lights: Res<LightBlocks>,
) -> Result {
    window(
        &mut contexts,
        &mut ui_state,
        Panel::Perf,
        [360.0, 320.0],
        |ui| {
            let value = |path: &bevy::diagnostic::DiagnosticPath| {
                diagnostics
                    .get(path)
                    .and_then(|d| d.smoothed())
                    .unwrap_or(0.0)
            };
            egui::Grid::new("perf").striped(true).show(ui, |ui| {
                row(
                    ui,
                    "fps",
                    format!("{:.0}", value(&FrameTimeDiagnosticsPlugin::FPS)),
                );
                row(
                    ui,
                    "frame time",
                    format!("{:.2} ms", value(&FrameTimeDiagnosticsPlugin::FRAME_TIME)),
                );
                let max = frames.0.iter().copied().fold(0.0, f32::max);
                row(ui, "frame max (graph)", format!("{max:.2} ms"));
                row(
                    ui,
                    "entities (client)",
                    format!("{:.0}", value(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)),
                );
                row(ui, "replicated", map.len().to_string());
                row(
                    ui,
                    "cached appearances",
                    map.cached_appearances().to_string(),
                );
                row(ui, "light blocks", lights.len().to_string());
                if let Some(atlas) = &atlas {
                    let s = atlas.stats();
                    row(
                        ui,
                        "atlas pages",
                        format!(
                            "{}/{} loaded, {} loading, {:.1} MiB (budget {} MiB)",
                            s.loaded,
                            s.pages,
                            s.loading,
                            s.resident_bytes as f64 / (1024.0 * 1024.0),
                            atlas.budget >> 20
                        ),
                    );
                }
                if let Some(l) = &lighting {
                    row(
                        ui,
                        "lighting sources",
                        format!("{} ({} in region)", l.sources, l.sources_in_region),
                    );
                    row(
                        ui,
                        "lighting frame",
                        format!("{:?} (max {:?} / 10 s)", l.frame_time, l.frame_time_max),
                    );
                    row(
                        ui,
                        "lighting updates",
                        format!("{} sources, {} rebuilds", l.source_updates, l.rebuilds),
                    );
                }
            });
            ui.label("frame time, ms");
            graph(ui, &frames.0);
        },
    )
}

fn row(ui: &mut egui::Ui, name: &str, value: impl Into<egui::WidgetText>) {
    ui.label(name);
    ui.label(value);
    ui.end_row();
}

/// A line graph of `values`, scaled to their maximum (at least 33 ms).
fn graph(ui: &mut egui::Ui, values: &std::collections::VecDeque<f32>) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(200.0), 80.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(120));
    let top = values.iter().copied().fold(33.3, f32::max);
    let y = |ms: f32| rect.bottom() - rect.height() * (ms / top);
    for line in [16.7, 33.3] {
        painter.hline(
            rect.x_range(),
            y(line),
            egui::Stroke::new(1.0, egui::Color32::DARK_GRAY),
        );
    }
    let n = values.len().max(2) - 1;
    let points: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, &ms)| egui::pos2(rect.left() + rect.width() * i as f32 / n as f32, y(ms)))
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.0, egui::Color32::LIGHT_GREEN),
    ));
}

pub(super) fn network(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    mut panel: ResMut<NetPanel>,
    state: Res<NetClientState>,
    link: Res<NetLinkStats>,
    time: Res<Time<Real>>,
) -> Result {
    let now = time.elapsed_secs_f64();
    window(
        &mut contexts,
        &mut ui_state,
        Panel::Network,
        [720.0, 480.0],
        |ui| {
            let panel = &mut *panel;
            let (rate_in, rate_out) = panel.log.rates(now);
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{:?}", *state));
                ui.separator();
                ui.label(format!(
                    "RTT {}",
                    link.rtt.map_or("-".into(), |r| format!(
                        "{:.1} ms",
                        r.as_secs_f64() * 1000.0
                    ))
                ));
                ui.separator();
                ui.label(format!(
                    "in {:.1} KiB/s, out {:.2} KiB/s",
                    rate_in / 1024.0,
                    rate_out / 1024.0
                ));
                ui.separator();
                ui.label(format!(
                    "UDP rx {} KiB / tx {} KiB, lost {}",
                    link.udp_rx_bytes >> 10,
                    link.udp_tx_bytes >> 10,
                    link.lost_packets
                ));
            });
            egui::CollapsingHeader::new("per message type")
                .default_open(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("kinds")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            egui::Grid::new("kinds").striped(true).show(ui, |ui| {
                                for h in ["kind", "in", "in bytes", "out", "out bytes", ""] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                let kinds: Vec<_> = panel
                                    .log
                                    .counters()
                                    .iter()
                                    .map(|(k, c)| (k.clone(), *c))
                                    .collect();
                                for (kind, c) in kinds {
                                    ui.monospace(&kind);
                                    ui.label(c.count_in.to_string());
                                    ui.label(bytes(c.bytes_in));
                                    ui.label(c.count_out.to_string());
                                    ui.label(bytes(c.bytes_out));
                                    let selected = panel.filter.kind == kind;
                                    if ui.selectable_label(selected, "filter").clicked() {
                                        panel.filter.kind =
                                            if selected { String::new() } else { kind };
                                    }
                                    ui.end_row();
                                }
                            });
                        });
                });
            ui.horizontal_wrapped(|ui| {
                ui.toggle_value(&mut panel.log.paused, "Pause");
                if ui.button("Clear").clicked() {
                    panel.log.clear();
                }
                ui.checkbox(&mut panel.filter.incoming, "in");
                ui.checkbox(&mut panel.filter.outgoing, "out");
                ui.label("kind");
                ui.add(egui::TextEdit::singleline(&mut panel.filter.kind).desired_width(110.0));
                ui.label("text");
                ui.add(egui::TextEdit::singleline(&mut panel.filter.text).desired_width(130.0));
                if ui.button("Export").clicked() {
                    panel.status = export(panel);
                }
            });
            if !panel.status.is_empty() {
                ui.small(&panel.status);
            }
            let shown: Vec<_> = panel
                .log
                .filtered(&panel.filter)
                .rev()
                .take(LOG_ROWS)
                .collect();
            ui.small(format!("{} of {} packets", shown.len(), panel.log.len()));
            let row = ui.text_style_height(&egui::TextStyle::Monospace);
            egui::ScrollArea::vertical()
                .id_salt("packets")
                .auto_shrink(false)
                .show_rows(ui, row, shown.len(), |ui, range| {
                    for r in &shown[range] {
                        let (dir, color) = if r.outgoing {
                            ("->", egui::Color32::LIGHT_BLUE)
                        } else {
                            ("<-", egui::Color32::LIGHT_GREEN)
                        };
                        let text = egui::RichText::new(format!(
                            "{:9.3} {dir} {:<22} {:>6}  {}",
                            r.time, r.kind, r.size, r.summary
                        ))
                        .monospace()
                        .color(color);
                        // One line per row: `show_rows` assumes a fixed row height.
                        ui.add(egui::Label::new(text).truncate())
                            .on_hover_text(&r.summary);
                    }
                });
        },
    )
}

fn bytes(n: u64) -> String {
    if n >= 1 << 20 {
        format!("{:.1} MiB", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1 << 10 {
        format!("{:.1} KiB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

fn export(panel: &NetPanel) -> String {
    let dir = export_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let path = dir.join(format!("ss13-packets-{stamp}.tsv"));
    let written = std::fs::File::create(&path).and_then(|file| {
        let mut out = std::io::BufWriter::new(file);
        let lines = panel.log.export(&panel.filter, &mut out)?;
        out.flush()?;
        Ok(lines)
    });
    match written {
        Ok(lines) => format!("{lines} packets written to {}", path.display()),
        Err(e) => format!("export to {} failed: {e}", path.display()),
    }
}

type Replicated<'a> = (
    Entity,
    &'a NetEntity,
    &'a TilePos,
    Option<&'a Appearance>,
    Option<&'a Facing>,
    Has<LocalControl>,
);

pub(super) fn entities(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    mut inspector: ResMut<Inspector>,
    map: Res<NetEntityMap>,
    replicated: Query<Replicated>,
    cameras: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    active_z: Res<ActiveZ>,
    atlas: Option<Res<IconAtlas>>,
    server: Res<ServerData>,
) -> Result {
    window(
        &mut contexts,
        &mut ui_state,
        Panel::Entities,
        [520.0, 560.0],
        |ui| {
            let view = cameras.iter().find_map(|(camera, transform)| {
                let rect = camera.logical_viewport_rect()?;
                let a = camera.viewport_to_world_2d(transform, Vec2::ZERO).ok()?;
                let b = camera.viewport_to_world_2d(transform, rect.size()).ok()?;
                Some(Rect::from_corners(a, b))
            });
            let in_view = view.map_or(0, |view| {
                replicated
                    .iter()
                    .filter(|(_, _, pos, ..)| {
                        pos.z == active_z.0
                            && view.contains(ss13_render::sprites::tile_translation(**pos))
                    })
                    .count()
            });
            ui.label(format!(
                "replicated {}, in view {in_view}, z {}",
                map.len(),
                active_z.0
            ));
            ui.checkbox(
                &mut inspector.pick,
                "click on the map selects a tile (the game does not get the click)",
            );
            if let Some(t) = inspector.hovered {
                ui.label(format!("hovered ({}, {}, {})", t.x, t.y, t.z));
            }
            let Some(tile) = inspector.selected else {
                ui.label("Click a tile.");
                return;
            };
            ui.heading(format!("tile ({}, {}, {})", tile.x, tile.y, tile.z));
            let report = server.tiles.get(&tile);
            let mut here: Vec<_> = replicated
                .iter()
                .filter(|(_, _, p, ..)| **p == tile)
                .collect();
            // What a click picks first; click-through atoms (`mouse_opacity = 0`) last.
            here.sort_by(|a, b| {
                let key = |a: Option<&Appearance>| {
                    a.map_or((false, i16::MIN, 0.0), |a| {
                        (clickable(Some(a)), a.plane, a.layer.0)
                    })
                };
                key(b.3)
                    .partial_cmp(&key(a.3))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            egui::ScrollArea::vertical()
                .id_salt("tile entities")
                .max_height(260.0)
                .show(ui, |ui| {
                    for (i, (entity, net, _, appearance, facing, controlled)) in
                        here.into_iter().enumerate()
                    {
                        let server_side = report
                            .and_then(|r| r.entities.iter().find(|e| e.net_id == Some(net.0)));
                        let name = server_side.map_or("?", |e| e.name.as_str());
                        let mut title = format!("#{} {name}", net.0.0);
                        if controlled {
                            title.push_str(" (you)");
                        }
                        if !clickable(appearance) {
                            title.push_str(" (click-through)");
                        }
                        let selected = inspector.entity == Some(entity);
                        egui::CollapsingHeader::new(title)
                            .id_salt(net.0.0)
                            .default_open(i == 0)
                            .show(ui, |ui| {
                                if ui
                                    .selectable_label(selected, "inspect components")
                                    .clicked()
                                {
                                    inspector.entity = Some(entity);
                                    inspector.components_at = f64::MIN;
                                }
                                if let Some(e) = server_side {
                                    ui.small(format!("server: {} / \"{}\"", e.icon, e.icon_state));
                                    ui.small(format!("components: {}", e.components.join(", ")));
                                }
                                ui.small(format!(
                                    "dir {:?}",
                                    facing.map_or(ss13_shared::Dir::South, |f| f.0)
                                ));
                                if let Some(a) = appearance {
                                    appearance_ui(ui, a, atlas.as_deref());
                                }
                            });
                    }
                });
            if let Some(entity) = inspector.entity {
                ui.separator();
                ui.strong(format!("components of {entity}"));
                egui::ScrollArea::vertical()
                    .id_salt("components")
                    .show(ui, |ui| {
                        for (name, value) in &inspector.components {
                            match value {
                                Some(value) => {
                                    egui::CollapsingHeader::new(name.as_str())
                                        .id_salt(name)
                                        .show(ui, |ui| {
                                            ui.monospace(value);
                                        });
                                }
                                None => {
                                    ui.label(format!("{name} (not reflected)"));
                                }
                            }
                        }
                    });
            }
        },
    )
}

fn state_name(atlas: Option<&IconAtlas>, id: ss13_shared::IconStateId) -> String {
    atlas.and_then(|a| a.manifest.get(id)).map_or_else(
        || format!("#{}", id.0),
        |s| format!("{} / \"{}\"", s.dmi, s.state),
    )
}

fn appearance_ui(ui: &mut egui::Ui, a: &Appearance, atlas: Option<&IconAtlas>) {
    ui.small(format!("icon_state {}", state_name(atlas, a.icon_state)));
    ui.small(format!(
        "layer {} plane {} pixel {:?} rotation {}",
        a.layer.0, a.plane, a.pixel_offset, a.rotation
    ));
    ui.small(format!("color {:?}", a.color));
    for (i, o) in a.overlays.iter().enumerate() {
        ui.small(format!(
            "overlay {i}: {} dir {:?} plane {:?} pixel {:?} color {:?}",
            state_name(atlas, o.icon_state),
            o.dir,
            o.plane,
            o.pixel_offset,
            o.color
        ));
    }
}

pub(super) fn prediction(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    prediction: Option<Res<Prediction>>,
) -> Result {
    window(
        &mut contexts,
        &mut ui_state,
        Panel::Prediction,
        [300.0, 160.0],
        |ui| {
            let Some(p) = prediction else {
                ui.label("no prediction");
                return;
            };
            let acked = p.authoritative();
            let acked_seq = acked.map_or(0, |a| a.seq);
            egui::Grid::new("prediction").show(ui, |ui| {
                row(ui, "sent seq", (acked_seq + p.unacked() as u32).to_string());
                row(ui, "acked seq", acked_seq.to_string());
                row(ui, "pending moves", p.unacked().to_string());
                row(ui, "corrections", p.corrections().to_string());
                row(ui, "predicted", format!("{:?}", p.predicted()));
                row(ui, "server", format!("{:?}", acked.map(|a| a.pos)));
            });
        },
    )
}

pub(super) fn tile(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    inspector: Res<Inspector>,
    server: Res<ServerData>,
) -> Result {
    window(
        &mut contexts,
        &mut ui_state,
        Panel::Tile,
        [320.0, 300.0],
        |ui| {
            let Some(t) = inspector.hovered.or(inspector.selected) else {
                ui.label("Hover the map.");
                return;
            };
            ui.heading(format!("({}, {}) z {}", t.x, t.y, t.z));
            match server.tiles.get(&t) {
                Some(report) => tile_report_ui(ui, report),
                None => {
                    ui.label("waiting for the server (debug queries must be open)");
                }
            }
            if !server.last_reply.is_empty() {
                ui.small(&server.last_reply);
            }
        },
    )
}

fn tile_report_ui(ui: &mut egui::Ui, report: &TileReport) {
    egui::Grid::new("tile").striped(true).show(ui, |ui| {
        row(ui, "turf", report.turf.clone());
        match &report.area {
            Some(area) => {
                row(ui, "area", area.name.clone());
                let on = |b: bool| if b { "on" } else { "off" };
                row(
                    ui,
                    "power",
                    if area.requires_power {
                        format!(
                            "equip {}, light {}, environ {}{}",
                            on(area.powered[0]),
                            on(area.powered[1]),
                            on(area.powered[2]),
                            if area.has_apc { "" } else { " (no APC)" }
                        )
                    } else {
                        "not required".into()
                    },
                );
                row(
                    ui,
                    "load W",
                    format!(
                        "{:.0} / {:.0} / {:.0}",
                        area.load[0], area.load[1], area.load[2]
                    ),
                );
            }
            None => row(ui, "area", "-"),
        }
        match &report.air {
            Some(air) => {
                row(ui, "pressure", format!("{:.2} kPa", air.pressure_kpa));
                row(
                    ui,
                    "temperature",
                    format!(
                        "{:.1} K ({:.1} °C)",
                        air.temperature_k,
                        air.temperature_k - 273.15
                    ),
                );
                row(ui, "moles", format!("{:.2}", air.total_moles));
                for (gas, moles, kpa) in &air.gases {
                    row(ui, gas, format!("{moles:.2} mol, {kpa:.2} kPa"));
                }
            }
            None => row(ui, "air", "none"),
        }
    });
}

pub(super) fn ui(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    tracker: Res<UiTracker>,
    windows: Option<Res<ClientWindows>>,
    sizes: Query<&bevy::ui::ComputedNode>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    fields: Query<(
        &bevy::text::EditableText,
        &bevy::ui::ComputedNode,
        &InheritedVisibility,
    )>,
) -> Result {
    window(
        &mut contexts,
        &mut ui_state,
        Panel::Ui,
        [460.0, 420.0],
        |ui| {
            let drawn = windows.as_ref().map_or(0, |w| w.0.len());
            ui.label(format!(
                "{} server windows ({drawn} drawn as tgui windows)",
                tracker.windows.len()
            ));
            egui::ScrollArea::vertical()
                .id_salt("ui windows")
                .max_height(300.0)
                .show(ui, |ui| {
                    for (id, w) in &tracker.windows {
                        // The drawn window's laid out size, logical px (what a resize sets).
                        let size = windows
                            .as_ref()
                            .and_then(|ws| ws.0.get(id))
                            .and_then(|e| sizes.get(*e).ok())
                            .map(|n| n.size * n.inverse_scale_factor)
                            .map_or_else(String::new, |s| format!(" {:.0}×{:.0}", s.x, s.y));
                        egui::CollapsingHeader::new(format!(
                            "#{id} {} \"{}\" {:?}{size}",
                            w.interface, w.title, w.status
                        ))
                        .id_salt(id)
                        .show(ui, |ui| {
                            // The window's text inputs: their value, the editor's text and widths.
                            if let Some(root) = windows.as_ref().and_then(|ws| ws.0.get(id)) {
                                let width =
                                    |n: &bevy::ui::ComputedNode| n.size.x * n.inverse_scale_factor;
                                for e in children.iter_descendants(*root) {
                                    if let Ok((text, node, visible)) = fields.get(e) {
                                        // Widths up the tree: the `Input` root, the item's slot,
                                        // the item row, the list.
                                        let chain: Vec<String> = parents
                                            .iter_ancestors(e)
                                            .take(5)
                                            .map(|p| {
                                                sizes.get(p).map_or("?".to_owned(), |n| {
                                                    format!("{:.0}", width(n))
                                                })
                                            })
                                            .collect();
                                        ui.monospace(format!(
                                            "input '{}' {:.0}px <- {}{}",
                                            text.value(),
                                            width(node),
                                            chain.join(" <- "),
                                            if visible.get() { "" } else { " hidden" },
                                        ));
                                    }
                                }
                            }
                            match ui_json(&w.interface, &w.static_data, &w.data) {
                                Ok((static_data, data)) => {
                                    egui::CollapsingHeader::new("static data")
                                        .id_salt((id, "static"))
                                        .show(ui, |ui| {
                                            ui.monospace(static_data);
                                        });
                                    egui::CollapsingHeader::new("data")
                                        .id_salt((id, "data"))
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            ui.monospace(data);
                                        });
                                }
                                Err(e) => {
                                    ui.label(format!(
                                        "{e}; {} static bytes, {} data bytes",
                                        w.static_data.len(),
                                        w.data.len()
                                    ));
                                }
                            }
                        });
                    }
                });
            ui.separator();
            ui.strong("last acts");
            for (time, window, action, params) in tracker.acts.iter().rev() {
                ui.monospace(format!("{time:9.3} #{window} {action} {params:?}"));
            }
        },
    )
}

/// Pretty JSON of a window's data, for the interfaces this client knows.
fn ui_json(interface: &str, static_data: &[u8], data: &[u8]) -> Result<(String, String), String> {
    fn json<T: UiInterface>(static_data: &[u8], data: &[u8]) -> Result<(String, String), String> {
        let s: T::StaticData = ss13_tgui::decode(static_data).map_err(|e| e.to_string())?;
        // A window below `UiStatus::Update` opens without data.
        let d = if data.is_empty() {
            None
        } else {
            Some(ss13_tgui::decode::<T::Data>(data).map_err(|e| e.to_string())?)
        };
        let pretty = |v: serde_json::Result<String>| v.unwrap_or_else(|e| e.to_string());
        Ok((
            pretty(serde_json::to_string_pretty(&s)),
            pretty(serde_json::to_string_pretty(&d)),
        ))
    }
    use ss13_game::{hud, lobby};
    use ss13_tgui::interfaces::{air_alarm, atmos, job_selection, thermo_machine, vending};
    let decode = match interface {
        thermo_machine::ThermoMachineUi::NAME => json::<thermo_machine::ThermoMachineUi>,
        vending::VendingUi::NAME => json::<vending::VendingUi>,
        job_selection::JobSelectionUi::NAME => json::<job_selection::JobSelectionUi>,
        atmos::AtmosPumpUi::NAME => json::<atmos::AtmosPumpUi>,
        atmos::AtmosFilterUi::NAME => json::<atmos::AtmosFilterUi>,
        atmos::AtmosMixerUi::NAME => json::<atmos::AtmosMixerUi>,
        atmos::CanisterUi::NAME => json::<atmos::CanisterUi>,
        atmos::PortablePumpUi::NAME => json::<atmos::PortablePumpUi>,
        atmos::PortableScrubberUi::NAME => json::<atmos::PortableScrubberUi>,
        air_alarm::AirAlarmUi::NAME => json::<air_alarm::AirAlarmUi>,
        lobby::LobbyUi::NAME => json::<lobby::LobbyUi>,
        lobby::JobPrefsUi::NAME => json::<lobby::JobPrefsUi>,
        hud::InventoryHudUi::NAME => json::<hud::InventoryHudUi>,
        hud::StorageUi::NAME => json::<hud::StorageUi>,
        hud::HealthHudUi::NAME => json::<hud::HealthHudUi>,
        hud::InteractHudUi::NAME => json::<hud::InteractHudUi>,
        hud::AlertHudUi::NAME => json::<hud::AlertHudUi>,
        _ => return Err("no decoder for this interface".into()),
    };
    decode(static_data, data)
}

pub(super) fn server(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    server: Res<ServerData>,
    inspector: Res<Inspector>,
    mut dev: ResMut<DevTools>,
    mut requests: MessageWriter<SendDebugRequest>,
) -> Result {
    window(
        &mut contexts,
        &mut ui_state,
        Panel::Server,
        [420.0, 480.0],
        |ui| {
            let mut send = |r| {
                requests.write(SendDebugRequest(r));
            };
            match &server.stats {
                Some(s) => {
                    egui::Grid::new("server stats")
                        .striped(true)
                        .show(ui, |ui| {
                            row(
                                ui,
                                "tick ms",
                                format!(
                                    "p50 {:.2}, p99 {:.2}, max {:.2}",
                                    s.tick_p50_ms, s.tick_p99_ms, s.tick_max_ms
                                ),
                            );
                            row(ui, "entities", s.entities.to_string());
                            row(ui, "round", s.round_state.clone());
                            row(ui, "time scale", format!("{}", s.time_scale));
                        });
                    egui::Grid::new("server clients")
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["client", "KiB/s", "packets/s", "visible"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for c in &s.clients {
                                ui.label(&c.name);
                                ui.label(format!("{:.1}", c.bytes_per_sec / 1024.0));
                                ui.label(format!("{:.0}", c.packets_per_sec));
                                ui.label(c.visible.to_string());
                                ui.end_row();
                            }
                        });
                }
                None => {
                    ui.label("no stats yet (debug queries must be open on the server)");
                }
            }
            ui.separator();
            ui.strong("dev tools (debug server only)");
            let target = inspector.selected;
            ui.label(match target {
                Some(t) => format!("target: selected tile ({}, {}, {})", t.x, t.y, t.z),
                None => "target: select a tile in the Entities panel".into(),
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(target.is_some(), egui::Button::new("Teleport here"))
                    .clicked()
                {
                    send(DebugRequest::Teleport(target.expect("enabled")));
                }
                if ui.checkbox(&mut dev.god, "god mode").changed() {
                    send(DebugRequest::GodMode(dev.god));
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut dev.time_scale, 0.1..=10.0).logarithmic(true));
                if ui.button("Set time scale").clicked() {
                    send(DebugRequest::TimeScale(dev.time_scale));
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut dev.confirm_end, "really");
                if ui
                    .add_enabled(dev.confirm_end, egui::Button::new("End round"))
                    .clicked()
                {
                    dev.confirm_end = false;
                    send(DebugRequest::EndRound);
                }
            });
            ui.horizontal(|ui| {
                ui.label("spawn");
                ui.add(egui::TextEdit::singleline(&mut dev.search).hint_text("/obj/item/tool"));
            });
            if dev.search.len() >= 3 && dev.search != dev.searched {
                dev.searched = dev.search.clone();
                send(DebugRequest::FindPrototypes(dev.search.clone()));
            }
            egui::ScrollArea::vertical()
                .id_salt("prototypes")
                .max_height(160.0)
                .show(ui, |ui| {
                    for id in &server.prototypes {
                        let button = egui::Button::new(egui::RichText::new(id).monospace());
                        if ui.add_enabled(target.is_some(), button).clicked() {
                            send(DebugRequest::Spawn {
                                path: id.clone(),
                                pos: target.expect("enabled"),
                            });
                        }
                    }
                });
            if !server.last_reply.is_empty() {
                ui.label(&server.last_reply);
            }
        },
    )
}

/// Not `mouse_opacity = MOUSE_OPACITY_TRANSPARENT`: clicks can stop at it.
fn clickable(a: Option<&Appearance>) -> bool {
    a.is_none_or(|a| a.mouse_opacity != ss13_shared::MouseOpacity::Transparent)
}

/// Fullscreen effect previews: each button draws a sample of what the server would send
/// (`FullscreenHud::preview`), on this client only, until "Off".
pub(super) fn screen_fx(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<DebugUi>,
    hud: Option<ResMut<FullscreenHud>>,
) -> Result {
    window(
        &mut contexts,
        &mut ui_state,
        Panel::ScreenFx,
        [260.0, 320.0],
        |ui| {
            let Some(mut hud) = hud else {
                ui.label("no fullscreen HUD");
                return;
            };
            ui.label(if hud.preview.is_some() {
                "preview on (server data hidden)"
            } else {
                "showing server data"
            });
            if ui.button("Off").clicked() {
                hud.preview = None;
            }
            ui.separator();
            for (name, data) in screen_fx_presets() {
                if ui.button(name).clicked() {
                    hud.preview = Some(data);
                }
            }
        },
    )
}

/// The previews: one per overlay family the server can send, at telling severities.
fn screen_fx_presets() -> Vec<(&'static str, FullscreenHudData)> {
    use ScreenCategory as C;
    let screens = |list: &[(C, Option<u8>)]| FullscreenHudData {
        screens: list
            .iter()
            .map(|&(c, s)| FullscreenScreen::new(c, s))
            .collect(),
        ..default()
    };
    let mut blind = screens(&[]);
    let mut flicker = FullscreenScreen::new(C::Blind, None);
    flicker.flicker = true;
    blind.screens.push(flicker);
    blind.colours = vec![ClientColour::Blindness];
    vec![
        ("Tint 1 (sunglasses)", screens(&[(C::Tint, Some(1))])),
        ("Tint 2 (welding mask)", screens(&[(C::Tint, Some(2))])),
        ("Nearsighted 2", screens(&[(C::Nearsighted, Some(2))])),
        ("Flash", screens(&[(C::Flash, None)])),
        ("Brute 3", screens(&[(C::Brute, Some(3))])),
        ("Brute 6", screens(&[(C::Brute, Some(6))])),
        ("Oxy 5", screens(&[(C::Oxy, Some(5))])),
        (
            "Crit 7",
            screens(&[(C::CritVision, Some(7)), (C::Crit, Some(7))]),
        ),
        ("Blind (temporary)", blind),
        ("Blind (static)", screens(&[(C::Blind, None)])),
        (
            "Monochrome",
            FullscreenHudData {
                colours: vec![ClientColour::Monochrome],
                ..default()
            },
        ),
        (
            "Blur 3",
            FullscreenHudData {
                blur: 3.0,
                ..default()
            },
        ),
        (
            "Blur 8",
            FullscreenHudData {
                blur: 8.0,
                ..default()
            },
        ),
    ]
}
