//! The modular shield windows (client): tgui `ModularShieldGen.tsx` and
//! `ModularShieldConsole.tsx`. The data is `crate::modular_shield_ui`, the server half
//! `crate::eng_machines::modular_shield`.
// shortcut: plain labelled values instead of tgui's progress bars, no rename field in the
// console (the server takes `rename`); add them with ui_kit progress bars and inputs.

use bevy::scene::EntityScene;
use ss13_tgui::client::{ActOnChange, AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use crate::modular_shield_ui::{
    ID, ModularShieldConsoleData, ModularShieldConsoleUi, ModularShieldGenData, ModularShieldGenUi,
    NEW_RADIUS, SET_RADIUS, TOGGLE_EXTERIOR, TOGGLE_SHIELDS,
};

pub struct ModularShieldClientPlugin;

impl Plugin for ModularShieldClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<ModularShieldGenUi>()
            .register_ui_view::<ModularShieldConsoleUi>();
    }
}

fn row(label: &str, text: String) -> Box<dyn Scene> {
    let label = label.to_owned();
    Box::new(bsn! {
        @LabeledListItem { @label: label }
        Children [ Text(text) ]
    })
}

impl ClientUi for ModularShieldGenUi {
    const WIDTH: f32 = 450.0;
    const HEIGHT: f32 = 300.0;

    fn content(_: &(), data: &ModularShieldGenData) -> impl Scene {
        gen_view(data)
    }
}

fn gen_view(d: &ModularShieldGenData) -> impl Scene + use<> {
    let rows: Vec<Box<dyn Scene>> = vec![
        row(
            "Strength",
            format!("{:.0} / {:.0}", d.current_strength, d.max_strength),
        ),
        row(
            "Regeneration",
            format!(
                "{:.1} / {:.1} per second",
                d.current_regeneration, d.max_regeneration
            ),
        ),
        row("Max radius", format!("{:.2}", d.max_radius)),
    ];
    let input = NumberInput {
        value: f64::from(d.current_radius),
        min_value: 1.0,
        max_value: f64::from(d.max_radius),
        step: 1.0,
        ..Default::default()
    };
    let on_change = ActOnChange {
        action: SET_RADIUS.into(),
        param: NEW_RADIUS.into(),
    };
    let radius_locked = d.active;
    let radius: Box<dyn Scene> = if radius_locked {
        Box::new(bsn! { Text({format!("{:.2}", d.current_radius)}) })
    } else {
        Box::new(bsn! { Node { width: px(65) } input on_change })
    };
    let (power_text, power_icon) = if d.active {
        ("Turn off", "power-off")
    } else {
        ("Turn on", "power-off")
    };
    let power_disabled = d.initiating_field || (!d.active && d.recovering);
    let exterior = d.exterior_only;
    let status = if d.recovering {
        "Recovering"
    } else if d.initiating_field {
        "Initiating"
    } else if d.active {
        "Active"
    } else {
        "Ready"
    };
    bsn! {
        @WindowContent
        Children [
            @Section {
                @title: "Shield",
                @buttons: bsn_list! {
                    @Button { @icon: power_icon, @content: power_text }
                    Button { disabled: power_disabled, selected: exterior }
                    Act({TOGGLE_SHIELDS.to_owned()})
                    --
                    @Button { @icon: "home", @content: "Exterior only" }
                    Button { selected: exterior }
                    Act({TOGGLE_EXTERIOR.to_owned()})
                },
            }
            Children [
                @LabeledList
                Children [
                    {EntityScene(row("Status", status.to_owned()))}
                    --
                    {rows}
                    --
                    @LabeledListItem { @label: "Radius" }
                    Children [ {EntityScene(radius)} ]
                ]
            ]
        ]
    }
}

impl ClientUi for ModularShieldConsoleUi {
    const WIDTH: f32 = 450.0;
    const HEIGHT: f32 = 400.0;

    fn content(_: &(), data: &ModularShieldConsoleData) -> impl Scene {
        console_view(data)
    }
}

fn console_view(d: &ModularShieldConsoleData) -> impl Scene + use<> {
    let gens: Vec<Box<dyn Scene>> = d
        .generators
        .iter()
        .map(|g| {
            let title = g.name.clone();
            let params = ActParams(vec![(ID.to_owned(), g.id.to_string())]);
            let text = if g.active { "Turn off" } else { "Turn on" };
            let disabled = g.inactive || g.recovering;
            let rows: Vec<Box<dyn Scene>> = vec![
                row(
                    "Strength",
                    format!("{:.0} / {:.0}", g.current_strength, g.max_strength),
                ),
                row(
                    "Regeneration",
                    format!("{:.1} per second", g.current_regeneration),
                ),
                row(
                    "Status",
                    if g.inactive {
                        "No power".to_owned()
                    } else if g.recovering {
                        "Recovering".to_owned()
                    } else if g.active {
                        "Active".to_owned()
                    } else {
                        "Inactive".to_owned()
                    },
                ),
            ];
            let section: Box<dyn Scene> = Box::new(bsn! {
                @Section {
                    @title: title,
                    @buttons: bsn_list! {
                        @Button { @icon: "power-off", @content: text }
                        Button { disabled: disabled }
                        Act({TOGGLE_SHIELDS.to_owned()})
                        params
                    },
                }
                Children [ @LabeledList Children [ {rows} ] ]
            });
            section
        })
        .collect();
    let empty = d
        .generators
        .is_empty()
        .then(|| EntityScene(bsn! { Text("No generators linked.") }));
    bsn! {
        @WindowContent
        Children [ {empty} -- {gens} ]
    }
}
