//! The ordnance lab's machine windows: tgui `DopplerArray` (`/obj/machinery/doppler_array`)
//! and `TankCompressor` (`/obj/machinery/atmospherics/components/binary/tank_compressor`).
//! The machines are in [`super::ordnance_machines`]; the experiments, papers and partners
//! they feed in [`super::scipaper`].

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `DopplerArray`.
pub struct DopplerArrayUi;

impl UiInterface for DopplerArrayUi {
    const NAME: &'static str = "DopplerArray";
    type Data = DopplerArrayData;
    type StaticData = ();
}

/// A `records` entry: one `/datum/data/tachyon_record`; its `ref` is `id`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DopplerRecord {
    pub id: u32,
    pub name: String,
    pub timestamp: String,
    pub coordinates: String,
    pub displacement: f64,
    /// `factual_*_radius`: epicenter, outer, shockwave.
    pub factual: [f32; 3],
    /// `theory_*_radius`, when the bomb was capped.
    pub theory: Option<[f32; 3]>,
    pub reaction_results: Vec<String>,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DopplerArrayData {
    pub disk: Option<String>,
    /// `"[used] / [max] GQ"`.
    pub storage: String,
    pub records: Vec<DopplerRecord>,
}

/// tgui `TankCompressor`.
pub struct TankCompressorUi;

impl UiInterface for TankCompressorUi {
    const NAME: &'static str = "TankCompressor";
    type Data = TankCompressorData;
    type StaticData = TankCompressorStatic;
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TankCompressorStatic {
    pub max_transfer: f64,
    pub leak_pressure: f64,
    pub fragment_pressure: f64,
    pub eject_pressure: f64,
}

/// A `records` entry: one `/datum/data/compressor_record`; its `ref` is `id`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CompressorRecordUi {
    pub id: u32,
    pub name: String,
    pub source: String,
    pub timestamp: String,
    /// Gas name -> moles.
    pub gases: Vec<(String, f64)>,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TankCompressorData {
    pub tank_present: bool,
    pub tank_pressure: Option<f64>,
    pub leaking: bool,
    pub active: bool,
    pub transfer_rate: f64,
    pub last_pressure: f64,
    pub disk: Option<String>,
    pub storage: String,
    pub records: Vec<CompressorRecordUi>,
}

/// Act names and params of both windows.
pub mod acts {
    pub const DELETE_RECORD: &str = "delete_record";
    pub const SAVE_RECORD: &str = "save_record";
    pub const EJECT_DISK: &str = "eject_disk";
    pub const REF: &str = "ref";
    pub const CHANGE_RATE: &str = "change_rate";
    pub const TARGET: &str = "target";
    pub const TOGGLE_INJECTION: &str = "toggle_injection";
    pub const EJECT_TANK: &str = "eject_tank";
}

#[cfg(feature = "client")]
pub mod client {
    use bevy::scene::{EntityScene, SceneList};
    use ss13_tgui::client::ClientUi;
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;

    use super::*;

    type Part = Box<dyn SceneList>;

    fn button(
        content: String,
        icon: &'static str,
        action: &str,
        params: Vec<(String, String)>,
        disabled: bool,
    ) -> Part {
        let action = action.to_owned();
        let params = ActParams(params);
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @content: content, @icon: icon }
            ss13_ui_kit::Button { disabled: disabled, selected: false }
            Act(action)
            params
        }))
    }

    fn line(text: String) -> Part {
        Box::new(EntityScene(bsn! { Node Children [ Text(text) ] }))
    }

    fn section(title: String, children: Vec<Part>) -> Part {
        Box::new(EntityScene(bsn! {
            @Section { @title: title }
            Children [ {children} ]
        }))
    }

    fn row(children: Vec<Part>) -> Part {
        Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Row }
            Children [ {children} ]
        }))
    }

    /// DM prints numbers with up to 6 significant digits; two decimals are enough here.
    fn num(n: f64) -> String {
        let r = (n * 100.0).round() / 100.0;
        if r.fract() == 0.0 {
            format!("{r:.0}")
        } else {
            format!("{r}")
        }
    }

    fn record_buttons(id: u32, can_save: bool) -> Part {
        let r = || vec![(acts::REF.to_owned(), id.to_string())];
        row(vec![
            button(
                "Save".to_owned(),
                "floppy-disk",
                acts::SAVE_RECORD,
                r(),
                !can_save,
            ),
            button(
                "Delete".to_owned(),
                "trash",
                acts::DELETE_RECORD,
                r(),
                false,
            ),
        ])
    }

    fn disk_section(disk: &Option<String>, storage: &str) -> Part {
        section(
            "Data Disk".to_owned(),
            vec![
                line(format!(
                    "Disk: {}",
                    disk.clone().unwrap_or_else(|| "None".to_owned())
                )),
                line(format!("Storage: {storage}")),
                button(
                    "Eject Disk".to_owned(),
                    "eject",
                    acts::EJECT_DISK,
                    Vec::new(),
                    disk.is_none(),
                ),
            ],
        )
    }

    impl ClientUi for DopplerArrayUi {
        const WIDTH: f32 = 650.0;
        const HEIGHT: f32 = 500.0;

        fn content(_: &(), data: &DopplerArrayData) -> impl Scene {
            let mut parts = vec![disk_section(&data.disk, &data.storage)];
            let mut records: Vec<Part> = Vec::new();
            if data.records.is_empty() {
                records.push(line("No records.".to_owned()));
            }
            for r in &data.records {
                let [e, o, s] = r.factual;
                let mut lines = vec![
                    line(format!("Timestamp: {}", r.timestamp)),
                    line(format!("Coordinates: {}", r.coordinates)),
                    line(format!("Displacement: {} seconds", num(r.displacement))),
                    line(format!(
                        "Epicenter Radius: {e}  Outer Radius: {o}  Shockwave Radius: {s}"
                    )),
                ];
                if let Some([e, o, s]) = r.theory {
                    lines.push(line(format!(
                        "Theoretical: Epicenter Radius: {e}  Outer Radius: {o}  Shockwave \
                         Radius: {s}"
                    )));
                }
                if r.reaction_results.is_empty() {
                    lines.push(line("Reaction Results: none".to_owned()));
                } else {
                    lines.push(line(format!(
                        "Reaction Results: {}",
                        r.reaction_results.join(", ")
                    )));
                }
                lines.push(record_buttons(r.id, data.disk.is_some()));
                records.push(section(r.name.clone(), lines));
            }
            parts.push(section("Records".to_owned(), records));
            bsn! {
                @WindowContent { @scrollable: true }
                Children [ {parts} ]
            }
        }
    }

    impl ClientUi for TankCompressorUi {
        const WIDTH: f32 = 550.0;
        const HEIGHT: f32 = 550.0;

        fn content(s: &TankCompressorStatic, data: &TankCompressorData) -> impl Scene {
            let rate = |n: f64| vec![(acts::TARGET.to_owned(), num(n))];
            let pressure = data
                .tank_pressure
                .map_or_else(|| "-".to_owned(), |p| format!("{} kPa", num(p)));
            let status = if !data.tank_present {
                "No tank"
            } else if data.leaking {
                "Leaking"
            } else if data.tank_pressure.unwrap_or(0.0) >= s.leak_pressure {
                "Over the leak pressure"
            } else {
                "Nominal"
            };
            let tank = section(
                "Tank".to_owned(),
                vec![
                    line(format!("Pressure: {pressure}")),
                    line(format!("Status: {status}")),
                    line(format!(
                        "Leak: {} kPa, fragmentation: {} kPa",
                        num(s.leak_pressure),
                        num(s.fragment_pressure)
                    )),
                    line(format!(
                        "Last recorded pressure: {} kPa",
                        num(data.last_pressure)
                    )),
                    line(format!("Transfer rate: {} L/s", num(data.transfer_rate))),
                    row(vec![
                        button(
                            "-5".to_owned(),
                            "minus",
                            acts::CHANGE_RATE,
                            rate(data.transfer_rate - 5.0),
                            false,
                        ),
                        button(
                            "+5".to_owned(),
                            "plus",
                            acts::CHANGE_RATE,
                            rate(data.transfer_rate + 5.0),
                            false,
                        ),
                        button(
                            "Max".to_owned(),
                            "forward",
                            acts::CHANGE_RATE,
                            rate(s.max_transfer),
                            false,
                        ),
                    ]),
                    row(vec![
                        button(
                            if data.active { "On" } else { "Off" }.to_owned(),
                            "power-off",
                            acts::TOGGLE_INJECTION,
                            Vec::new(),
                            false,
                        ),
                        button(
                            "Eject Tank".to_owned(),
                            "eject",
                            acts::EJECT_TANK,
                            Vec::new(),
                            !data.tank_present
                                || data.tank_pressure.unwrap_or(0.0) >= s.eject_pressure,
                        ),
                    ]),
                ],
            );
            let mut records: Vec<Part> = Vec::new();
            if data.records.is_empty() {
                records.push(line("No records.".to_owned()));
            }
            for r in &data.records {
                let mut lines = vec![
                    line(format!("Source: {}", r.source)),
                    line(format!("Timestamp: {}", r.timestamp)),
                ];
                for (gas, moles) in &r.gases {
                    lines.push(line(format!("{gas}: {} moles", num(*moles))));
                }
                lines.push(record_buttons(r.id, data.disk.is_some()));
                records.push(section(r.name.clone(), lines));
            }
            let parts = vec![
                tank,
                disk_section(&data.disk, &data.storage),
                section("Records".to_owned(), records),
            ];
            bsn! {
                @WindowContent { @scrollable: true }
                Children [ {parts} ]
            }
        }
    }
}
