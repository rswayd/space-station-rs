//! The shuttle console windows (client): `EmergencyShuttleConsole.tsx` and `ShuttleConsole.jsx`.
//! The data is `crate::shuttle_console`, the server halves `crate::shuttle_console_server` and
//! `crate::shuttle_nav`.
// shortcut: "Repeal All" sits in the button row, not the section header; no candystripe rows.
// The shuttle console's authorization modal is not ported (no admin-controlled shuttle here).

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use crate::shuttle_console::{
    ABORT, AUTHORIZE, DESTINATION, EmergencyConsoleData, EmergencyConsoleUi, MOVE, REPEAL,
    SET_DESTINATION, SHUTTLE_ID, ShuttleConsoleData, ShuttleConsoleUi,
};

pub struct ShuttleConsoleClientPlugin;

impl Plugin for ShuttleConsoleClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<EmergencyConsoleUi>()
            .register_ui_view::<ShuttleConsoleUi>();
    }
}

fn button(
    label: &'static str,
    icon: &'static str,
    color: UiColor,
    disabled: bool,
    action: &'static str,
) -> impl Scene + use<> {
    let (content, action) = (label.to_owned(), action.to_owned());
    bsn! {
        Node { flex_grow: 1.0, margin: UiRect::right(px(4)) }
        Children [
            @ss13_ui_kit::Button { @icon: icon, @content: content, @fluid: true }
            ss13_ui_kit::Button { color: color, disabled: disabled }
            Act(action)
        ]
    }
}

impl ClientUi for EmergencyConsoleUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 350.0;

    fn content(_: &(), data: &EmergencyConsoleData) -> impl Scene {
        let timer = data.timer_str.clone();
        let (engines, engines_color) = if data.engines_started {
            ("Online", UiColor::Good)
        } else {
            ("Idle", UiColor::Average)
        };
        let engines = engines.to_owned();
        let engines_text = engines_color.text().unwrap_or_default();
        let (remaining, remaining_color) = if data.emagged {
            ("ERROR".to_owned(), UiColor::Bad)
        } else {
            (
                format!("Remaining: {}", data.authorizations_remaining),
                UiColor::Good,
            )
        };
        let remaining_text = remaining_color.text().unwrap_or_default();
        let disabled = !data.enabled;
        let authorize = EntityScene(button(
            "AUTHORIZE",
            "exclamation-triangle",
            UiColor::Good,
            disabled,
            AUTHORIZE,
        ));
        let repeal = EntityScene(button(
            "REPEAL",
            "minus",
            UiColor::Default,
            disabled,
            REPEAL,
        ));
        let abort = EntityScene(button("Repeal All", "times", UiColor::Bad, disabled, ABORT));
        let average = UiColor::Average.text().unwrap_or_default();
        let none = data.authorizations.is_empty().then(|| {
            EntityScene(bsn! {
                Node { justify_content: JustifyContent::Center }
                Propagate::<Bold>(Bold(true))
                Propagate::<FontPx>(FontPx(16.0))
                Propagate::<TextColor>(TextColor(average))
                Children [ Text("No Active Authorizations") ]
            })
        });
        let rows: Vec<_> = data
            .authorizations
            .iter()
            .map(|(name, job)| {
                let line = format!("{name} ({job})");
                bsn! {
                    Node
                    Propagate::<Bold>(Bold(true))
                    Propagate::<FontPx>(FontPx(16.0))
                    Children [ Text(line) ]
                }
            })
            .collect();
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [
                    Node { justify_content: JustifyContent::Center }
                    Propagate::<Bold>(Bold(true))
                    Propagate::<FontPx>(FontPx(40.0))
                    Children [ Text(timer) ]
                    --
                    Node { justify_content: JustifyContent::Center, margin: UiRect::bottom(px(6)) }
                    Propagate::<FontPx>(FontPx(16.0))
                    Children [
                        Node Propagate::<Bold>(Bold(true)) Children [ Text("ENGINES:") ]
                        --
                        Node { margin: UiRect::left(px(6)) }
                        Propagate::<TextColor>(TextColor(engines_text))
                        Children [ Text(engines) ]
                    ]
                    --
                    @Section { @title: "Early Launch Authorization" }
                    Children [
                        Node { flex_direction: FlexDirection::Row }
                        Children [ {authorize} -- {repeal} -- {abort} ]
                        --
                        @Section { @title: "Authorizations" }
                        Children [
                            Node { justify_content: JustifyContent::FlexEnd }
                            Propagate::<Bold>(Bold(true))
                            Propagate::<TextColor>(TextColor(remaining_text))
                            Children [ Text(remaining) ]
                            --
                            Node { flex_direction: FlexDirection::Column, min_height: px(150) }
                            Children [ {none} -- {rows} ]
                        ]
                    ]
                ]
            ]
        }
    }
}

fn params(pairs: &[(&str, &str)]) -> ActParams {
    ActParams(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    )
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
