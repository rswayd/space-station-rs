//! The emergency shuttle console window (client): `EmergencyShuttleConsole.tsx`. The data is
//! `crate::shuttle_console`, the server half `crate::shuttle_console_server`.
// shortcut: "Repeal All" sits in the button row, not the section header; no candystripe rows.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use crate::shuttle_console::{ABORT, AUTHORIZE, EmergencyConsoleData, EmergencyConsoleUi, REPEAL};

pub struct ShuttleConsoleClientPlugin;

impl Plugin for ShuttleConsoleClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<EmergencyConsoleUi>();
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
