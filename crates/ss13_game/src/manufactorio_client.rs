//! The manufacturing sorter's window (client): tgui `ManufacturingSorter.tsx`. The data is in
//! [`crate::manufactorio_ui`], the server half in `crate::eng_machines::manufactorio`.
// shortcut: icon-only buttons stand in for tgui's coloured ones (green "New filter", red trash).

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;

use crate::manufactorio_ui::*;

pub struct ManufactorioClientPlugin;

impl Plugin for ManufactorioClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<ManufacturingSorterUi>();
    }
}

/// `dir2icon`.
fn dir_icon(dir: u8) -> &'static str {
    match dir {
        2 => "arrow-down",
        4 => "arrow-right",
        8 => "arrow-left",
        _ => "arrow-up",
    }
}

impl ClientUi for ManufacturingSorterUi {
    const WIDTH: f32 = 450.0;
    const HEIGHT: f32 = 350.0;

    fn content(_: &(), data: &SorterData) -> impl Scene {
        sorter_view(data)
    }
}

/// One filter's buttons: output side, edit, delete, up, down.
fn filter_buttons(row: &SorterFilterRow) -> Box<dyn SceneList> {
    let by_ref = || ActParams(vec![(SORTER_REF.to_owned(), row.r#ref.clone())]);
    let shift = |amount: &str| {
        ActParams(vec![
            (SORTER_REF.to_owned(), row.r#ref.clone()),
            (SORTER_AMOUNT.to_owned(), amount.to_owned()),
        ])
    };
    let (rotate, edit, delete) = (by_ref(), by_ref(), by_ref());
    let (up, down) = (shift("-1"), shift("1"));
    let icon = dir_icon(row.dir);
    Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row }
        Children [
            @ss13_ui_kit::Button { @icon: icon, @content: "Output" }
            Act({SORTER_ROTATE.to_owned()})
            rotate
            --
            @ss13_ui_kit::Button { @icon: "pencil" }
            Act({SORTER_EDIT.to_owned()})
            edit
            --
            @ss13_ui_kit::Button { @icon: "trash" }
            Act({SORTER_DEL.to_owned()})
            delete
            --
            @ss13_ui_kit::Button { @icon: "arrow-up" }
            Act({SORTER_SHIFT.to_owned()})
            up
            --
            @ss13_ui_kit::Button { @icon: "arrow-down" }
            Act({SORTER_SHIFT.to_owned()})
            down
        ]
    }))
}

fn sorter_view(data: &SorterData) -> impl Scene + use<> {
    let rows: Vec<_> = data
        .filters
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let label = format!("{}. {}", i + 1, row.name);
            let buttons = filter_buttons(row);
            bsn! { @LabeledListItem { @label: label, @buttons: buttons } }
        })
        .collect();
    let new_filter: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @icon: "plus", @content: "New filter" }
        Act({SORTER_NEW.to_owned()})
    }));
    let unmet_icon = dir_icon(data.unmet_dir);
    bsn! {
        @WindowContent
        Children [
            Node { flex_direction: FlexDirection::Column }
            Children [
                @Section { @title: "Filters", @buttons: new_filter, @fill: true, @scrollable: true }
                Children [
                    @LabeledList
                    Children [ {rows} ]
                ]
                --
                Node { flex_direction: FlexDirection::Row }
                Children [
                    Node Children [ Text("If no criteria is met, outputting to:") ]
                    --
                    @ss13_ui_kit::Button { @icon: unmet_icon }
                    Act({SORTER_ROTATE_UNMET.to_owned()})
                ]
            ]
        ]
    }
}
