//! `Wires` (the hacking window): `tgui/packages/tgui/interfaces/Wires.tsx` and
//! `/datum/wires/ui_data` (`code/datums/wires/_wires.dm`). The server half (wire sets, cut and
//! pulse effects) lives with the game (`ss13_game::wires`); this module has the data both halves
//! share and the client view.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct WiresUi;

/// One `ui_data` `wires` entry.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WireEntry {
    /// The color key the acts send back (`params["wire"]`).
    pub color: String,
    /// `shownColor`: the greyscale variant for colorblind users, else the color.
    pub shown_color: String,
    /// The wire's purpose when revealed (blueprints, ...); `None`: hidden.
    pub wire: Option<String>,
    pub cut: bool,
    pub attached: bool,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct WiresData {
    /// `None` when the set's `proper_name` is `"Unknown"`.
    pub proper_name: Option<String>,
    pub wires: Vec<WireEntry>,
    /// `get_status()` lines.
    pub status: Vec<String>,
}

impl UiInterface for WiresUi {
    const NAME: &'static str = "Wires";
    type Data = WiresData;
    type StaticData = ();
}

/// `const dynamicHeight = 150 + wires.length * 30 + (proper_name ? 30 : 0)`.
pub fn window_height(data: &WiresData) -> f32 {
    150.0
        + data.wires.len() as f32 * 30.0
        + if data.proper_name.is_some() {
            30.0
        } else {
            0.0
        }
}

/// What `color={wire.shownColor.replace(' ', '')}` resolves to: a tgui color class for the
/// names in tgui's color map, else the CSS named color (tgui-core sets it as the inline style).
#[cfg(feature = "client")]
pub fn shown_color(name: &str) -> ss13_ui_kit::UiColor {
    use ss13_ui_kit::UiColor;
    // JS `replace` with a string pattern replaces the first space only.
    let name = name.replacen(' ', "", 1);
    match name.as_str() {
        "black" => UiColor::Black,
        "white" => UiColor::White,
        "red" => UiColor::Red,
        "orange" => UiColor::Orange,
        "yellow" => UiColor::Yellow,
        "olive" => UiColor::Olive,
        "green" => UiColor::Green,
        "teal" => UiColor::Teal,
        "blue" => UiColor::Blue,
        "violet" => UiColor::Violet,
        "purple" => UiColor::Purple,
        "pink" => UiColor::Pink,
        "brown" => UiColor::Brown,
        "grey" | "gray" => UiColor::Grey,
        // CSS named colors.
        "crimson" => UiColor::Css([0xDC, 0x14, 0x3C]),
        "cyan" => UiColor::Css([0x00, 0xFF, 0xFF]),
        "gold" => UiColor::Css([0xFF, 0xD7, 0x00]),
        "lime" => UiColor::Css([0x00, 0xFF, 0x00]),
        "magenta" => UiColor::Css([0xFF, 0x00, 0xFF]),
        "silver" => UiColor::Css([0xC0, 0xC0, 0xC0]),
        "lightgrey" => UiColor::Css([0xD3, 0xD3, 0xD3]),
        "dimgrey" => UiColor::Css([0x69, 0x69, 0x69]),
        _ => UiColor::Default,
    }
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::app::Propagate;
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::{self, Italic};

    use super::{WiresData, WiresUi, shown_color, window_height};
    use crate::client::ClientUi;

    impl ClientUi for WiresUi {
        const WIDTH: f32 = 350.0;
        const HEIGHT: f32 = 150.0;

        fn size(_: &(), data: &WiresData) -> (f32, f32) {
            (Self::WIDTH, window_height(data))
        }

        fn content(_: &(), data: &WiresData) -> impl Scene {
            let notice = data.proper_name.as_ref().map(|name| {
                let text = format!("{name} Wire Configuration");
                EntityScene(bsn! {
                    @StackItem
                    Children [
                        @NoticeBox
                        Propagate::<theme::TextAlign>(theme::TextAlign(Justify::Center))
                        Children [ Text(text) ]
                    ]
                })
            });
            let status = (!data.status.is_empty()).then(|| {
                let lines: Vec<_> = data
                    .status
                    .iter()
                    .map(|line| {
                        let line = line.clone();
                        bsn! { @UiBox Children [ Text(line) ] }
                    })
                    .collect();
                EntityScene(bsn! {
                    @StackItem
                    Children [ @Section Children [ {lines} ] ]
                })
            });
            let wire_map = wire_map(data);
            bsn! {
                @WindowContent
                Children [
                    @Stack { @vertical: true, @fill: true }
                    Children [
                        {notice}
                        --
                        @StackItem { @grow: 1.0 }
                        Children [
                            @Section { @fill: true }
                            Children [ {EntityScene(wire_map)} ]
                        ]
                        --
                        {status}
                    ]
                ]
            }
        }
    }

    /// `WireMap`: a labeled list of the wires, candystriped.
    fn wire_map(data: &WiresData) -> impl Scene + use<> {
        let items: Vec<_> = data
            .wires
            .iter()
            .enumerate()
            .map(|(i, wire)| {
                let color = shown_color(&wire.shown_color);
                let label = wire.shown_color.clone();
                let param = |c: &str| ActParams(vec![("wire".into(), c.to_owned())]);
                let (cut, pulse, attach) =
                    (param(&wire.color), param(&wire.color), param(&wire.color));
                let cut_text = if wire.cut { "Mend" } else { "Cut" };
                let attach_text = if wire.attached { "Detach" } else { "Attach" };
                // `.candystripe:nth-child(odd)`.
                let stripe = if i % 2 == 0 {
                    theme::CANDYSTRIPE_ODD
                } else {
                    Color::NONE
                };
                let name = wire.wire.as_ref().map(|w| {
                    let text = format!("({w})");
                    EntityScene(bsn! {
                        Node
                        Propagate::<Italic>(Italic(true))
                        Children [ Text(text) ]
                    })
                });
                bsn! {
                    @LabeledListItem {
                        @label: label,
                        @label_color: color,
                        @color: color,
                        @buttons: bsn_list! {
                            @ss13_ui_kit::Button { @content: cut_text }
                            Act("cut")
                            cut
                            --
                            @ss13_ui_kit::Button { @content: "Pulse" }
                            Act("pulse")
                            pulse
                            --
                            @ss13_ui_kit::Button { @content: attach_text }
                            Act("attach")
                            attach
                        },
                    }
                    BackgroundColor(stripe)
                    Children [ {name} ]
                }
            })
            .collect();
        bsn! {
            @LabeledList
            Children [ {items} ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_follows_the_wire_count() {
        let wire = WireEntry {
            color: "red".into(),
            shown_color: "red".into(),
            wire: None,
            cut: false,
            attached: false,
        };
        let mut data = WiresData {
            proper_name: Some("Generic Airlock".into()),
            wires: vec![wire; 17],
            status: vec![],
        };
        assert_eq!(window_height(&data), 150.0 + 17.0 * 30.0 + 30.0);
        data.proper_name = None;
        assert_eq!(window_height(&data), 150.0 + 17.0 * 30.0);
    }

    #[cfg(feature = "client")]
    #[test]
    fn colors_are_tgui_classes_or_css() {
        use ss13_ui_kit::UiColor;
        assert_eq!(shown_color("red"), UiColor::Red);
        assert_eq!(shown_color("crimson"), UiColor::Css([0xDC, 0x14, 0x3C]));
        assert_eq!(shown_color("light grey"), UiColor::Css([0xD3, 0xD3, 0xD3]));
        assert_eq!(shown_color("dim grey"), UiColor::Css([0x69, 0x69, 0x69]));
    }
}
