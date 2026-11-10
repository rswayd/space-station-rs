//! The transport windows (client): `TramControl.tsx` and `ElevatorPanel.tsx`. The data is
//! `crate::transport_ui`, the server halves `crate::transport::tram` and
//! `crate::transport::elevator`.
// shortcut: the tram console has a send button per platform instead of the line map with a
// selected stop and one "Send Tram"; no department icons. The elevator panel lists the floors as
// buttons without the shaft drawing.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use crate::transport_ui::{
    DESTINATION, EMERGENCY_DOOR, ElevatorPanelData, ElevatorPanelStatic, ElevatorPanelUi,
    MOVE_LIFT, RESET_DOORS, SEND, TramControlData, TramControlStatic, TramControlUi, Z,
};

pub struct TransportClientPlugin;

impl Plugin for TransportClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<TramControlUi>()
            .register_ui_view::<ElevatorPanelUi>();
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

fn banner(text: String, color: UiColor) -> impl Scene + use<> {
    let color = color.text().unwrap_or_default();
    bsn! {
        Node { justify_content: JustifyContent::Center, margin: UiRect::bottom(px(6)) }
        Propagate::<Bold>(Bold(true))
        Propagate::<FontPx>(FontPx(16.0))
        Propagate::<TextColor>(TextColor(color))
        Children [ Text(text) ]
    }
}

fn act_button(
    label: String,
    icon: &'static str,
    disabled: bool,
    action: &'static str,
    act_params: ActParams,
) -> impl Scene + use<> {
    bsn! {
        Node { margin: UiRect::bottom(px(4)) }
        Children [
            @ss13_ui_kit::Button { @icon: icon, @content: label, @fluid: true }
            ss13_ui_kit::Button { disabled: disabled }
            Act({action.to_owned()})
            act_params
        ]
    }
}

impl ClientUi for TramControlUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 300.0;

    fn content(statics: &TramControlStatic, data: &TramControlData) -> impl Scene {
        let status = if data.broken {
            EntityScene(banner("Check Tram Controller!".to_owned(), UiColor::Bad))
        } else if data.moving {
            EntityScene(banner("Tram in Transit".to_owned(), UiColor::Average))
        } else {
            EntityScene(banner(
                "Nanotrasen Transit System".to_owned(),
                UiColor::Default,
            ))
        };
        let location = data.tram_location.clone();
        let buttons: Vec<_> = statics
            .destinations
            .iter()
            .map(|(name, code)| {
                let here = location.as_deref() == Some(name.as_str());
                act_button(
                    if here {
                        format!("{name} (here)")
                    } else {
                        format!("Send Tram: {name}")
                    },
                    "train-tram",
                    here || data.broken || data.moving,
                    SEND,
                    params(&[(DESTINATION, code)]),
                )
            })
            .collect();
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [ {status} -- {buttons} ]
            ]
        }
    }
}

impl ClientUi for ElevatorPanelUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 340.0;

    fn content(statics: &ElevatorPanelStatic, data: &ElevatorPanelData) -> impl Scene {
        let floor_name = |z: u16| {
            statics
                .all_floor_data
                .iter()
                .find(|(_, f)| *f == z)
                .map_or_else(|| format!("Floor {z}"), |(n, _)| n.clone())
        };
        let status = if !data.lift_exists {
            EntityScene(banner("No elevator connected".to_owned(), UiColor::Bad))
        } else if data.currently_moving {
            let to = data
                .currently_moving_to_floor
                .map_or_else(String::new, |z| format!(" to {}", floor_name(z)));
            EntityScene(banner(format!("Moving{to}"), UiColor::Average))
        } else {
            EntityScene(banner(
                format!("At {}", floor_name(data.current_floor)),
                UiColor::Good,
            ))
        };
        let level = EntityScene(banner(
            format!("Alert level: {}", data.emergency_level),
            if data.is_emergency {
                UiColor::Bad
            } else {
                UiColor::Default
            },
        ));
        let floors: Vec<_> = statics
            .all_floor_data
            .iter()
            .map(|(name, z)| {
                let here = *z == data.current_floor;
                let z_text = z.to_string();
                act_button(
                    name.clone(),
                    if here { "circle" } else { "arrow-right" },
                    here || data.currently_moving || !data.lift_exists,
                    MOVE_LIFT,
                    params(&[(Z, &z_text)]),
                )
            })
            .collect();
        let doors = if data.doors_open {
            Some(EntityScene(act_button(
                "Reset Doors".to_owned(),
                "door-closed",
                false,
                RESET_DOORS,
                params(&[]),
            )))
        } else if data.is_emergency {
            Some(EntityScene(act_button(
                "Emergency Door Open".to_owned(),
                "door-open",
                !data.lift_exists,
                EMERGENCY_DOOR,
                params(&[]),
            )))
        } else {
            None
        };
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [ {status} -- {level} -- {floors} -- {doors} ]
            ]
        }
    }
}
