//! The flatpacker window (client): tgui `Flatpacker.tsx`. The data is `crate::flatpacker_ui`,
//! the server half `crate::eng_machines::flatpacker`.
// shortcut: plain material rows with 1/10 sheet eject buttons instead of `MaterialAccessBar`,
// no machine icon; add them with a shared material bar view.

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use crate::flatpacker_ui::{
    AMOUNT, BUILD, EJECT, EJECT_BOARD, FlatpackerData, FlatpackerUi, MATERIAL_REF,
};

/// `SHEET_MATERIAL_AMOUNT`.
const SHEET: u32 = 100;

pub struct FlatpackerClientPlugin;

impl Plugin for FlatpackerClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<FlatpackerUi>();
    }
}

impl ClientUi for FlatpackerUi {
    const WIDTH: f32 = 450.0;
    const HEIGHT: f32 = 400.0;

    fn content(_: &(), data: &FlatpackerData) -> impl Scene {
        view(data)
    }
}

fn eject_button(index: u32, sheets: u32, have: u32) -> Box<dyn Scene> {
    let params = ActParams(vec![
        (MATERIAL_REF.to_owned(), index.to_string()),
        (AMOUNT.to_owned(), sheets.to_string()),
    ]);
    let disabled = have < sheets * SHEET;
    let label = format!("x{sheets}");
    Box::new(bsn! {
        @Button { @icon: "eject", @content: label }
        Button { disabled: disabled }
        Act({EJECT.to_owned()})
        params
    })
}

fn view(data: &FlatpackerData) -> impl Scene + use<> {
    let materials: Vec<Box<dyn Scene>> = data
        .materials
        .iter()
        .map(|m| {
            let label = m.name.clone();
            let text = format!("{} sheets", m.amount / SHEET);
            let one = eject_button(m.index, 1, m.amount);
            let ten = eject_button(m.index, 10, m.amount);
            let row: Box<dyn Scene> = Box::new(bsn! {
                @LabeledListItem { @label: label }
                Children [ Text(text) -- {EntityScene(one)} -- {EntityScene(ten)} ]
            });
            row
        })
        .collect();
    let design: Box<dyn Scene> = match &data.design {
        Some(d) => {
            let name = d.name.clone();
            let costs: Vec<Box<dyn Scene>> = d
                .required
                .iter()
                .map(|m| {
                    let label = m.name.clone();
                    let text = format!("{} units", m.amount);
                    let row: Box<dyn Scene> = Box::new(bsn! {
                        @LabeledListItem { @label: label }
                        Children [ Text(text) ]
                    });
                    row
                })
                .collect();
            let reason = d.disable_reason.clone();
            let disabled = data.busy || !reason.is_empty();
            let busy = data.busy;
            Box::new(bsn! {
                Node { flex_direction: FlexDirection::Column }
                Children [
                    Text(name)
                    --
                    @LabeledList
                    Children [ {costs} ]
                    --
                    Text(reason)
                    TextColor({UiColor::Bad.base()})
                    --
                    @Button { @icon: "hammer", @content: "Build" }
                    Button { disabled: disabled, selected: busy }
                    Act({BUILD.to_owned()})
                    --
                    @Button { @icon: "eject", @content: "Eject board" }
                    Button { disabled: busy }
                    Act({EJECT_BOARD.to_owned()})
                ]
            })
        }
        None => Box::new(bsn! { Text("No board inserted.") }),
    };
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Materials" }
            Children [
                @LabeledList
                Children [ {materials} ]
            ]
            --
            @Section { @title: "Design" }
            Children [ {EntityScene(design)} ]
        ]
    }
}
