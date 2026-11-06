//! `Gps`: `tgui/packages/tgui/interfaces/Gps.tsx` and `/datum/component/gps/item/ui_data`
//! (`code/datums/components/gps.dm`). The server half lives with the game
//! (`ss13_game::devices::gps`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct GpsUi;

/// `ui_data`. `current_area`, `current_coords` and `signals` only while tracking and not EMPed.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct GpsData {
    pub power: bool,
    pub tag: String,
    pub updating: bool,
    pub globalmode: bool,
    pub current_area: String,
    /// `"x, y, z"`.
    pub current_coords: String,
    pub signals: Vec<GpsSignal>,
}

/// One other GPS (`signals`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct GpsSignal {
    pub entrytag: String,
    /// `"x, y, z"`.
    pub coords: String,
    /// Same level only. The window's own figure: the Euclidean distance, rounded (tgui
    /// recomputes it from the coordinates because BYOND's `get_dist` caps at 127).
    pub dist: Option<u32>,
    /// `round(get_angle(curr, pos))`: 0-360 clockwise from north, same level only.
    pub degrees: Option<f32>,
}

impl UiInterface for GpsUi {
    const NAME: &'static str = "Gps";
    type Data = GpsData;
    type StaticData = ();
}

/// The window's order: signals with a distance first, then by name.
pub fn sorted_signals(signals: &[GpsSignal]) -> Vec<&GpsSignal> {
    let mut out: Vec<&GpsSignal> = signals.iter().collect();
    out.sort_by(|a, b| (a.dist.is_none(), &a.entrytag).cmp(&(b.dist.is_none(), &b.entrytag)));
    out
}

/// The direction cell's opacity: `clamp(1.2 / ln(e + dist / 20), 0.4, 1)`.
pub fn direction_opacity(dist: u32) -> f32 {
    let value = 1.2 / (std::f32::consts::E + dist as f32 / 20.0).ln();
    value.clamp(0.4, 1.0)
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{GpsData, GpsSignal, GpsUi, direction_opacity, sorted_signals};
    use crate::client::ClientUi;

    fn direction(signal: &GpsSignal) -> Box<dyn Scene> {
        let text = signal.dist.map(|d| format!("{d}m")).unwrap_or_default();
        let alpha = signal.dist.map_or(1.0, direction_opacity);
        let color = Color::WHITE.with_alpha(alpha);
        match signal.degrees {
            Some(degrees) => Box::new(bsn! {
                Node { flex_direction: FlexDirection::Row }
                Children [
                    Node { margin: UiRect::right(px(6)) }
                    Children [ @Icon { @name: "arrow-up", @rotation: degrees, @size: 1.2 } ]
                    --
                    Text(text) TextColor(color)
                ]
            }),
            None => Box::new(bsn! { Text(text) TextColor(color) }),
        }
    }

    fn row(signal: &GpsSignal) -> impl Scene + use<> {
        let name = signal.entrytag.clone();
        let coords = signal.coords.clone();
        let dir = direction(signal);
        bsn! {
            @TableRow
            Children [
                @TableCell { @content: name, @bold: true, @color: UiColor::Label }
                --
                @TableCell { @collapsing: true }
                Children [ {EntityScene(dir)} ]
                --
                @TableCell { @collapsing: true, @content: coords }
            ]
        }
    }

    impl ClientUi for GpsUi {
        const WIDTH: f32 = 470.0;
        const HEIGHT: f32 = 700.0;

        // shortcut: no name search box (the list is short); the title is the item's name.
        fn content(_: &(), data: &GpsData) -> impl Scene {
            let power = data.power;
            let power_text = if power { "On" } else { "Off" };
            let tag = data.tag.clone();
            let (scan_icon, scan_text, scan_color) = if data.updating {
                ("unlock", "AUTO", UiColor::Default)
            } else {
                ("lock", "MANUAL", UiColor::Bad)
            };
            let local = !data.globalmode;
            let range_text = if data.globalmode { "MAXIMUM" } else { "LOCAL" };
            let location = format!("{} ({})", data.current_area, data.current_coords);
            let shown = if power { Display::Flex } else { Display::None };
            let rows: Vec<_> = sorted_signals(&data.signals).into_iter().map(row).collect();
            bsn! {
                @WindowContent
                Children [
                    @Section {
                        @title: "Control",
                        @buttons: bsn! {
                            @Button { @icon: "power-off", @content: power_text }
                            Button { selected: power }
                            Act("power")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Tag" }
                            Children [
                                @Button { @icon: "pencil-alt", @content: tag }
                                Act("rename")
                            ]
                            --
                            @LabeledListItem { @label: "Scan Mode" }
                            Children [
                                @Button { @icon: scan_icon, @content: scan_text }
                                Button { color: scan_color }
                                Act("updating")
                            ]
                            --
                            @LabeledListItem { @label: "Range" }
                            Children [
                                @Button { @icon: "sync", @content: range_text }
                                Button { selected: local }
                                Act("globalmode")
                            ]
                        ]
                    ]
                    --
                    Node { display: shown, flex_direction: FlexDirection::Column }
                    Children [
                        @Section { @title: "Current Location" }
                        Children [ Text(location) ]
                        --
                        @Section { @title: "Detected Signals" }
                        Children [
                            @Table
                            Children [
                                @TableRow { @header: true }
                                Children [
                                    @TableCell { @header: true, @content: "Name" }
                                    --
                                    @TableCell {
                                        @header: true,
                                        @collapsing: true,
                                        @content: "Direction",
                                    }
                                    --
                                    @TableCell {
                                        @header: true,
                                        @collapsing: true,
                                        @content: "Coordinates",
                                    }
                                ]
                                --
                                {rows}
                            ]
                        ]
                    ]
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(tag: &str, dist: Option<u32>) -> GpsSignal {
        GpsSignal {
            entrytag: tag.into(),
            coords: "1, 1, 1".into(),
            dist,
            degrees: dist.map(|_| 0.0),
        }
    }

    #[test]
    fn signals_with_distance_come_first_then_by_name() {
        let signals = [
            signal("MINE0", None),
            signal("SCI0", Some(5)),
            signal("ENG0", Some(40)),
            signal("COM0", None),
        ];
        let tags: Vec<&str> = sorted_signals(&signals)
            .into_iter()
            .map(|s| s.entrytag.as_str())
            .collect();
        assert_eq!(tags, ["ENG0", "SCI0", "COM0", "MINE0"]);
    }

    #[test]
    fn far_signals_fade() {
        assert_eq!(direction_opacity(0), 1.0);
        assert!(direction_opacity(200) < 0.6);
        assert_eq!(direction_opacity(100_000), 0.4);
    }

    #[test]
    fn data_round_trips() {
        let data = GpsData {
            power: true,
            tag: "COM0".into(),
            updating: true,
            globalmode: false,
            current_area: "Bridge".into(),
            current_coords: "100, 120, 2".into(),
            signals: vec![signal("ENG0", Some(12))],
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<GpsData>(&bytes).unwrap(), data);
    }
}
