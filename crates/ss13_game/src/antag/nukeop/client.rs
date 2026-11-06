//! The nuclear operatives' window (client): `NuclearBomb.tsx` (the keypad). The infiltrator's
//! console is the shared `ShuttleConsole.jsx` (`crate::shuttle_console_client`).
// shortcut: the `retro` theme and the NT logo are approximated with plain colours.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use super::{ANCHOR, ARM, DIGIT, EJECT_DISK, KEYPAD, NuclearBombData, NuclearBombUi};

pub struct NukeopClientPlugin;

impl Plugin for NukeopClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<NuclearBombUi>();
    }
}

/// `KEYPAD` columns.
const KEYS: [[&str; 4]; 3] = [
    ["1", "4", "7", "C"],
    ["2", "5", "8", "0"],
    ["3", "6", "9", "E"],
];

fn params(pairs: &[(&str, &str)]) -> ActParams {
    ActParams(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    )
}

/// `.NuclearBomb__displayBox`: dark green digits on the retro screen.
fn display(text: String) -> impl Scene + use<> {
    let screen = Color::srgb(0.11, 0.18, 0.11);
    let digits = Color::srgb(0.55, 0.85, 0.45);
    bsn! {
        Node {
            padding: UiRect::all(px(6)),
            margin: UiRect::bottom(px(6)),
            flex_grow: 1.0,
        }
        BackgroundColor(screen)
        Propagate::<Bold>(Bold(true))
        Propagate::<FontPx>(FontPx(20.0))
        Propagate::<TextColor>(TextColor(digits))
        Children [ Text(text) ]
    }
}

fn key(digit: &str) -> impl Scene + use<> {
    let label = digit.to_owned();
    let color = match digit {
        "C" => UiColor::Red,
        "E" => UiColor::Green,
        _ => UiColor::Default,
    };
    let p = params(&[(DIGIT, digit)]);
    bsn! {
        Node { width: px(55), height: px(50), margin: UiRect::bottom(px(6)) }
        Children [
            @ss13_ui_kit::Button { @content: label, @fluid: true }
            ss13_ui_kit::Button { color: color }
            Propagate::<Bold>(Bold(true))
            Propagate::<FontPx>(FontPx(28.0))
            Act({KEYPAD.to_owned()})
            p
        ]
    }
}

impl ClientUi for NuclearBombUi {
    const WIDTH: f32 = 350.0;
    const HEIGHT: f32 = 442.0;

    fn content(_: &(), data: &NuclearBombData) -> impl Scene {
        let columns: Vec<_> = KEYS
            .iter()
            .map(|column| {
                let keys: Vec<_> = column.iter().map(|d| key(d)).collect();
                bsn! {
                    Node { flex_direction: FlexDirection::Column, margin: UiRect::right(px(6)) }
                    Children [ {keys} ]
                }
            })
            .collect();
        let (status1, status2) = (data.status1.clone(), data.status2.clone());
        let anchored = data.anchored;
        bsn! {
            @WindowContent
            Children [
                Node { flex_direction: FlexDirection::Column, padding: UiRect::all(px(6)) }
                Children [
                    {EntityScene(display(status1))}
                    --
                    Node { flex_direction: FlexDirection::Row }
                    Children [
                        {EntityScene(display(status2))}
                        --
                        Node { width: px(43), margin: UiRect::left(px(6)) }
                        Children [
                            @ss13_ui_kit::Button { @icon: "eject", @fluid: true }
                            Act({EJECT_DISK.to_owned()})
                        ]
                    ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(6)) }
                    Children [
                        Node { flex_direction: FlexDirection::Row }
                        Children [ {columns} ]
                        --
                        Node { flex_direction: FlexDirection::Column, width: px(129), margin: UiRect::left(px(6)) }
                        Children [
                            Node { height: px(40), margin: UiRect::bottom(px(6)) }
                            Children [
                                @ss13_ui_kit::Button { @content: "ARM", @fluid: true }
                                ss13_ui_kit::Button { color: UiColor::Red }
                                Propagate::<Bold>(Bold(true))
                                Propagate::<FontPx>(FontPx(24.0))
                                Act({ARM.to_owned()})
                            ]
                            --
                            Node { height: px(40) }
                            Children [
                                @ss13_ui_kit::Button { @content: "ANCHOR", @fluid: true }
                                ss13_ui_kit::Button { color: UiColor::Green, selected: anchored }
                                Propagate::<Bold>(Bold(true))
                                Propagate::<FontPx>(FontPx(24.0))
                                Act({ANCHOR.to_owned()})
                            ]
                            --
                            Node { justify_content: JustifyContent::Center, margin: UiRect::top(px(12)) }
                            Propagate::<FontPx>(FontPx(72.0))
                            Propagate::<TextColor>(TextColor(Color::srgb(0.61, 0.6, 0.53)))
                            Children [ @Icon { @name: "radiation" } ]
                        ]
                    ]
                ]
            ]
        }
    }
}
