//! The nuclear operatives' windows (client): `NuclearBomb.tsx` (the keypad) and
//! `ShuttleConsole.jsx` (the infiltrator's console).
// shortcut: the `retro` theme and the NT logo are approximated with plain colours; the console's
// authorization modal is not ported (nothing here needs admin authorization).

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use super::{
    ANCHOR, ARM, DESTINATION, DIGIT, EJECT_DISK, KEYPAD, MOVE, NuclearBombData, NuclearBombUi,
    SET_DESTINATION, SHUTTLE_ID, ShuttleConsoleData, ShuttleConsoleUi,
};

pub struct NukeopClientPlugin;

impl Plugin for NukeopClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<NuclearBombUi>()
            .register_ui_view::<ShuttleConsoleUi>();
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

/// `STATUS_COLOR_KEYS`.
fn status_color(status: &str) -> UiColor {
    match status {
        "In Transit" => UiColor::Good,
        "Idle" | "Igniting" | "Recharging" => UiColor::Average,
        _ => UiColor::Bad,
    }
}

impl ClientUi for ShuttleConsoleUi {
    const WIDTH: f32 = 350.0;
    const HEIGHT: f32 = 230.0;

    fn content(_: &(), data: &ShuttleConsoleData) -> impl Scene {
        let name_of = |id: &Option<String>| {
            id.as_ref().and_then(|id| {
                data.locations
                    .iter()
                    .find(|(i, _)| i == id)
                    .map(|(_, n)| n.clone())
            })
        };
        let selected = name_of(&data.destination);
        let timer = if data.timer_str.is_empty() {
            "00:00".to_owned()
        } else {
            data.timer_str.clone()
        };
        let status = if data.status.is_empty() {
            "Not Available".to_owned()
        } else {
            data.status.clone()
        };
        let status_text = status_color(&status).text().unwrap_or_default();
        let location = if data.docked_location.is_empty() {
            "Not Available".to_owned()
        } else {
            data.docked_location.clone()
        };
        let bad = UiColor::Bad.text().unwrap_or_default();
        let average = UiColor::Average.text().unwrap_or_default();
        let none = data.locations.is_empty().then(|| {
            EntityScene(bsn! {
                Node Propagate::<TextColor>(TextColor(bad))
                Children [ Text("Not Available") ]
            })
        });
        let one = (data.locations.len() == 1).then(|| {
            let name = data.locations[0].1.clone();
            EntityScene(bsn! {
                Node Propagate::<TextColor>(TextColor(average))
                Children [ Text(name) ]
            })
        });
        let many = (data.locations.len() > 1).then(|| {
            let options: Vec<String> = data.locations.iter().map(|(_, n)| n.clone()).collect();
            let shown = selected
                .clone()
                .unwrap_or_else(|| "Select a Destination".to_owned());
            let disabled = data.locked || data.authorization_required;
            EntityScene(bsn! {
                @Dropdown { @value_param: DESTINATION, @width: 15.0, @over: true }
                Dropdown { options: options, selected: shown, disabled: disabled }
                Act({SET_DESTINATION.to_owned()})
            })
        });
        let depart_disabled = selected.is_none() || data.locked || data.authorization_required;
        let dest_id = data.destination.clone().unwrap_or_default();
        let depart = params(&[(SHUTTLE_ID, &dest_id)]);
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [
                    Node { justify_content: JustifyContent::Center }
                    Propagate::<Bold>(Bold(true))
                    Propagate::<FontPx>(FontPx(26.0))
                    Children [ Text(timer) ]
                    --
                    Node { justify_content: JustifyContent::Center, margin: UiRect::bottom(px(6)) }
                    Propagate::<FontPx>(FontPx(14.0))
                    Children [
                        Node Propagate::<Bold>(Bold(true)) Children [ Text("STATUS:") ]
                        --
                        Node { margin: UiRect::left(px(6)) }
                        Propagate::<TextColor>(TextColor(status_text))
                        Children [ Text(status) ]
                    ]
                    --
                    @Section { @title: "Shuttle Controls" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Location", @content: location }
                            --
                            @LabeledListItem { @label: "Destination" }
                            Children [ {none} -- {one} -- {many} ]
                        ]
                        --
                        @ss13_ui_kit::Button { @content: "Depart", @icon: "arrow-up", @fluid: true }
                        ss13_ui_kit::Button { disabled: depart_disabled }
                        Act({MOVE.to_owned()})
                        depart
                    ]
                ]
            ]
        }
    }
}
