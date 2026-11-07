//! The telecomms windows (client): tgui `Telecomms.jsx`, `TelecommsMonitor.tsx`,
//! `ServerMonitor.tsx` and `NtnetRelay.tsx`. The data is `crate::telecomms_ui`, the server half
//! `crate::telecomms_config` and `crate::ntnet_relay`.
// shortcut: plain lists instead of tgui's machine sprites and tables; the frequency fields are
// text inputs in kHz (Enter applies) instead of tgui's number spinners.

use bevy::scene::EntityScene;
use ss13_tgui::client::{ActOnSubmit, AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use crate::telecomms_ui::*;

pub struct TelecommsClientPlugin;

impl Plugin for TelecommsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<TelecommsUi>()
            .register_ui_view::<TelecommsMonitorUi>()
            .register_ui_view::<ServerMonitorUi>()
            .register_ui_view::<NtnetRelayUi>();
    }
}

fn row(label: &str, content: Box<dyn Scene>) -> Box<dyn Scene> {
    let label = label.to_owned();
    let content = EntityScene(content);
    Box::new(bsn! {
        @LabeledListItem { @label: label }
        Children [ {content} ]
    })
}

fn text(t: impl Into<String>) -> Box<dyn Scene> {
    let t: String = t.into();
    Box::new(bsn! { Text(t) })
}

fn button(
    label: &str,
    action: &str,
    params: Vec<(String, String)>,
    selected: bool,
) -> Box<dyn Scene> {
    let label = label.to_owned();
    let action = action.to_owned();
    let params = ActParams(params);
    Box::new(bsn! {
        @Button { @content: label }
        Button { selected: selected }
        Act(action)
        params
    })
}

/// A text field that sends `action` with `param` on Enter.
fn field(value: String, max: usize, action: &str, param: &str) -> Box<dyn Scene> {
    let submit = ActOnSubmit {
        action: action.to_owned(),
        param: param.to_owned(),
        params: Vec::new(),
    };
    let max = Some(max);
    Box::new(bsn! {
        Node { width: px(160) }
        Input { value: value, max_length: max }
        submit
    })
}

fn khz(freq: u16) -> String {
    format!("{:.1}", f64::from(freq) / 10.0)
}

impl ClientUi for TelecommsUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 600.0;

    fn content(_: &(), data: &TelecommsData) -> impl Scene {
        machine_view(data)
    }
}

fn machine_view(d: &TelecommsData) -> impl Scene + use<> {
    let power = button(
        if d.toggled { "On" } else { "Off" },
        TOGGLE,
        Vec::new(),
        d.toggled,
    );
    let mut settings: Vec<Box<dyn Scene>> = vec![
        row("Power", power),
        row("Identification", field(d.id.clone(), 32, ID, VALUE)),
        row("Network", field(d.network.clone(), 15, NETWORK, VALUE)),
        row(
            "Prefabrication",
            text(if d.prefab { "TRUE" } else { "FALSE" }),
        ),
    ];
    match d.kind.as_str() {
        "relay" => {
            settings.push(row(
                "Broadcasting",
                button(
                    if d.broadcasting { "Yes" } else { "No" },
                    BROADCAST,
                    Vec::new(),
                    d.broadcasting,
                ),
            ));
            settings.push(row(
                "Receiving",
                button(
                    if d.receiving { "Yes" } else { "No" },
                    RECEIVE,
                    Vec::new(),
                    d.receiving,
                ),
            ));
        }
        "bus" => {
            let current = if d.changefrequency == 0 {
                String::new()
            } else {
                khz(d.changefrequency)
            };
            settings.push(row(
                "Change Signal Frequency",
                field(current, 6, CHANGE_FREQ, VALUE),
            ));
        }
        _ => {}
    }
    let links: Vec<Box<dyn Scene>> = d
        .linked
        .iter()
        .map(|l| {
            let label = format!("{}. {} ({})", l.index, l.name, l.id);
            let unlink = button(
                "Remove",
                UNLINK,
                vec![(VALUE.to_owned(), l.index.to_string())],
                false,
            );
            row(&label, unlink)
        })
        .collect();
    let freqs: Vec<Box<dyn Scene>> = d
        .frequencies
        .iter()
        .map(|&f| {
            let remove = button(
                "Remove",
                DELETE,
                vec![(VALUE.to_owned(), f.to_string())],
                false,
            );
            row(&khz(f), remove)
        })
        .collect();
    let add_freq: Vec<Box<dyn Scene>> = vec![
        row(
            &format!("Add ({}-{} kHz)", khz(d.minfreq), khz(d.maxfreq)),
            field(khz(d.frequency), 6, TEMPFREQ, VALUE),
        ),
        row("", button("Add frequency", FREQ, Vec::new(), false)),
    ];
    let multitool: Vec<Box<dyn Scene>> = if d.multitool {
        vec![
            row(
                "Multitool buffer",
                text(d.multibuff.clone().unwrap_or_else(|| "-".to_owned())),
            ),
            row("", button("Link", LINK, Vec::new(), false)),
            row("", button("Buffer", BUFFER, Vec::new(), false)),
            row("", button("Flush", FLUSH, Vec::new(), false)),
        ]
    } else {
        Vec::new()
    };
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Settings" }
            Children [ @LabeledList Children [ {settings} -- {multitool} ] ]
            --
            @Section { @title: "Links" }
            Children [ @LabeledList Children [ {links} ] ]
            --
            @Section { @title: "Filtering Frequencies" }
            Children [ @LabeledList Children [ {freqs} -- {add_freq} ] ]
        ]
    }
}

impl ClientUi for TelecommsMonitorUi {
    const WIDTH: f32 = 575.0;
    const HEIGHT: f32 = 400.0;

    fn content(_: &(), data: &TelecommsMonitorData) -> impl Scene {
        monitor_view(data)
    }
}

fn machine_rows(list: &[MonitorMachine], view: bool) -> Vec<Box<dyn Scene>> {
    list.iter()
        .map(|m| {
            let label = format!("{} ({})", m.name, m.id);
            if view {
                row(
                    &label,
                    button(
                        "View",
                        VIEW,
                        vec![(MACHINE_ID.to_owned(), m.id.clone())],
                        false,
                    ),
                )
            } else {
                row(&label, text(""))
            }
        })
        .collect()
}

fn monitor_view(d: &TelecommsMonitorData) -> impl Scene + use<> {
    let status = d.status_message.clone().map(|s| EntityScene(text(s)));
    let body: Vec<Box<dyn Scene>> = match &d.machine {
        Some(m) => {
            let mut rows = vec![
                row("Machine", text(format!("{} ({})", m.name, m.id))),
                row("Network", text(m.network.clone())),
                row("", button("Home", HOME, Vec::new(), false)),
            ];
            rows.extend(machine_rows(&m.linked_machines, false));
            rows
        }
        None => {
            let mut rows = vec![
                row("Network", field(d.network.clone(), 15, PROBE, MACHINE_ID)),
                row("", button("Flush buffer", FLUSH, Vec::new(), false)),
            ];
            rows.extend(machine_rows(&d.machines, true));
            rows
        }
    };
    bsn! {
        @WindowContent
        Children [
            {status}
            --
            @Section { @title: "Telecommunications Monitor" }
            Children [ @LabeledList Children [ {body} ] ]
        ]
    }
}

impl ClientUi for ServerMonitorUi {
    const WIDTH: f32 = 575.0;
    const HEIGHT: f32 = 400.0;

    fn content(_: &(), data: &ServerMonitorData) -> impl Scene {
        browser_view(data)
    }
}

fn browser_view(d: &ServerMonitorData) -> impl Scene + use<> {
    let error = (!d.error.is_empty()).then(|| EntityScene(text(d.error.clone())));
    let body: Vec<Box<dyn Scene>> = match &d.server {
        Some(s) => {
            let mut rows = vec![
                row("Server", text(s.name.clone())),
                row("Total recorded traffic", text(s.traffic.to_string())),
                row("", button("Return", RETURN_HOME, Vec::new(), false)),
            ];
            rows.extend(s.packets.iter().map(|p| {
                let label = format!(
                    "{} [{}] {} ({}, {}, {})",
                    p.name, p.kind, p.source, p.job, p.race, p.language
                );
                let line = format!("{}  ", p.message);
                let delete = button(
                    "Delete",
                    DELETE_PACKET,
                    vec![(PACKET_REF.to_owned(), p.packet_ref.clone())],
                    false,
                );
                let line = EntityScene(text(line));
                let delete = EntityScene(delete);
                row(
                    &label,
                    Box::new(bsn! { Node Children [ {line} -- {delete} ] }),
                )
            }));
            rows
        }
        None => {
            let mut rows = vec![
                row(
                    "Network",
                    field(d.network.clone(), 15, SCAN_NETWORK, NETWORK_ID),
                ),
                row("", button("Clear buffer", CLEAR_BUFFER, Vec::new(), false)),
            ];
            rows.extend(d.servers.iter().map(|s| {
                row(
                    &format!("{} ({})", s.name, s.id),
                    button(
                        "View",
                        VIEW_SERVER,
                        vec![(SERVER.to_owned(), s.server_ref.clone())],
                        false,
                    ),
                )
            }));
            rows
        }
    };
    bsn! {
        @WindowContent
        Children [
            {error}
            --
            @Section { @title: "Server Monitor" }
            Children [ @LabeledList Children [ {body} ] ]
        ]
    }
}

impl ClientUi for NtnetRelayUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 300.0;

    fn content(_: &(), data: &NtnetRelayData) -> impl Scene {
        relay_view(data)
    }
}

fn relay_view(d: &NtnetRelayData) -> impl Scene + use<> {
    let power = button(
        if d.enabled { "ENABLED" } else { "DISABLED" },
        TOGGLE,
        Vec::new(),
        d.enabled,
    );
    let mut rows = vec![
        row(
            "Network buffer",
            text(format!("{:.0} / {:.0} GQ", d.dos_overload, d.dos_capacity)),
        ),
        row("Power", power),
    ];
    if d.dos_crashed {
        rows.push(row(
            "NETWORK BUFFER OVERFLOW",
            button("Purge network blacklist", RESTART, Vec::new(), false),
        ));
    }
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Network Buffer" }
            Children [ @LabeledList Children [ {rows} ] ]
        ]
    }
}
