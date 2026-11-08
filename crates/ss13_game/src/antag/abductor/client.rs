//! The abductors' windows (client): tgui `AbductorConsole.tsx`, `ProbingConsole.jsx` and
//! `GlandDispenser.tsx`. The data is [`super`], the server half [`super::server`].
// shortcut: the `abductor` theme is the default one; the shop is a category list and a labeled
// list instead of `GenericUplink`'s cards; the gland buttons show their colour as a square.

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use super::{
    AbductorConsoleData, AbductorConsoleStatic, AbductorConsoleUi, BUY, CATEGORY, DISPENSE, DOOR,
    EXPERIMENT, EXPERIMENT_TYPE, FLIP_VEST, GLAND_ID, GlandDispenserData, GlandDispenserUi, NAME,
    ProbingConsoleData, ProbingConsoleUi, SELECT, SELECT_DISGUISE, TELEPORTER_RETRIEVE,
    TELEPORTER_SEND, TOGGLE_VEST, VEST_STEALTH,
};

pub struct AbductorClientPlugin;

impl Plugin for AbductorClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<AbductorConsoleUi>()
            .register_ui_view::<ProbingConsoleUi>()
            .register_ui_view::<GlandDispenserUi>();
    }
}

/// Local state key: the console's tab (`1` Abductsoft, `2` mission settings).
const TAB: &str = "tab";

fn params(pairs: &[(&str, &str)]) -> ActParams {
    ActParams(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    )
}

fn row(label: &str, text: String) -> impl Scene + use<> {
    let label = label.to_owned();
    bsn! {
        @LabeledListItem { @label: label }
        Children [ Text(text) ]
    }
}

fn notice(text: &str) -> impl Scene + use<> {
    let text = text.to_owned();
    bsn! { @NoticeBox { @color: UiColor::Red } Children [ Text(text) ] }
}

fn button(icon: &str, content: &str, action: &str) -> impl Scene + use<> {
    let (icon, content, action) = (icon.to_owned(), content.to_owned(), action.to_owned());
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: content }
        Act(action)
    }
}

// ---------------------------------------------------------------------------------------------
// AbductorConsole

impl ClientUi for AbductorConsoleUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 532.0;

    fn content(s: &AbductorConsoleStatic, data: &AbductorConsoleData) -> impl Scene {
        console_view(s, data, &UiLocalState::default())
    }

    fn content_local(
        s: &AbductorConsoleStatic,
        data: &AbductorConsoleData,
        local: &UiLocalState,
    ) -> impl Scene {
        console_view(s, data, local)
    }
}

fn tab(label: &str, value: &str, selected: bool) -> impl Scene + use<> {
    let (label, value) = (label.to_owned(), value.to_owned());
    bsn! {
        @TabsTab { @content: label }
        TabsTab { selected: selected }
        SetLocal { key: {TAB.to_owned()}, value: value, rerender: true }
    }
}

fn console_view(
    s: &AbductorConsoleStatic,
    data: &AbductorConsoleData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let settings = local.get(TAB) == "2";
    let body: Box<dyn SceneList> = if settings {
        let (teleporter, vest) = (teleporter_view(data), vest_view(data));
        Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [ {EntityScene(teleporter)} -- {EntityScene(vest)} ]
        }))
    } else {
        abductsoft_view(s, data)
    };
    let (shop_tab, settings_tab) = (
        tab("Abductsoft 3000", "1", !settings),
        tab("Mission Settings", "2", settings),
    );
    bsn! {
        @WindowContent
        Children [
            Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
            Children [
                @Tabs { @fluid: true }
                Children [ {EntityScene(shop_tab)} -- {EntityScene(settings_tab)} ]
                --
                Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
                Children [ {body} ]
            ]
        ]
    }
}

fn abductsoft_view(s: &AbductorConsoleStatic, data: &AbductorConsoleData) -> Box<dyn SceneList> {
    if !data.experiment {
        return Box::new(EntityScene(notice("No Experiment Machine Detected")));
    }
    let points = data.points.unwrap_or(0).to_string();
    let credits = format!("{} Credits", data.credits.unwrap_or(0));
    let categories: Vec<_> = s
        .categories
        .iter()
        .map(|c| {
            let selected = data.selected.as_deref() == Some(c.name.as_str());
            let label = c.name.clone();
            let p = params(&[(CATEGORY, &c.name)]);
            bsn! {
                @ss13_ui_kit::Button { @content: label }
                ss13_ui_kit::Button { selected: selected }
                Act({SELECT.to_owned()})
                p
            }
        })
        .collect();
    let items: Vec<_> = s
        .categories
        .iter()
        .filter(|c| data.selected.as_deref() == Some(c.name.as_str()))
        .flat_map(|c| c.items.iter())
        .map(|item| {
            let label = item.name.clone();
            let desc = item.desc.clone();
            let cost = format!("{} Credits", item.cost);
            let disabled = !data.can_buy(item);
            let p = params(&[(NAME, &item.name)]);
            let buy: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "shopping-cart", @content: cost }
                ss13_ui_kit::Button { disabled: disabled }
                Act({BUY.to_owned()})
                p
            }));
            bsn! {
                @LabeledListItem { @label: label, @buttons: buy }
                Children [ Text(desc) ]
            }
        })
        .collect();
    Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
        Children [
            @Section
            Children [
                @LabeledList
                Children [
                    {EntityScene(row("Collected Samples", points))}
                    --
                    {EntityScene(row("Credits", credits))}
                ]
            ]
            --
            @Section { @title: "Categories" }
            Children [
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
                Children [ {categories} ]
            ]
            --
            @Section { @fill: true, @scrollable: true }
            Children [
                @LabeledList
                Children [ {items} ]
            ]
        ]
    }))
}

fn teleporter_view(data: &AbductorConsoleData) -> Box<dyn Scene> {
    if !data.pad {
        return Box::new(notice("No Telepad Detected"));
    }
    let activate: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @icon: "exclamation-circle", @content: "Activate" }
        ss13_ui_kit::Button { color: UiColor::Bad }
        Act({TELEPORTER_SEND.to_owned()})
    }));
    let marked = data.gizmo;
    let unmarked = !marked;
    let (icon, label) = if marked {
        ("user-plus", "Retrieve")
    } else {
        ("user-slash", "No Mark")
    };
    Box::new(bsn! {
        @Section { @title: "Emergency Teleport", @buttons: activate }
        Children [
            @LabeledList
            Children [
                @LabeledListItem { @label: "Mark Retrieval" }
                Children [
                    @ss13_ui_kit::Button { @icon: icon, @content: label }
                    ss13_ui_kit::Button { disabled: unmarked }
                    Act({TELEPORTER_RETRIEVE.to_owned()})
                ]
            ]
        ]
    })
}

fn vest_view(data: &AbductorConsoleData) -> Box<dyn Scene> {
    if !data.vest {
        return Box::new(notice("No Agent Vest Detected"));
    }
    let locked = data.vest_lock.unwrap_or(false);
    let lock: Box<dyn SceneList> = Box::new(EntityScene(button(
        if locked { "lock" } else { "unlock" },
        if locked { "Locked" } else { "Unlocked" },
        TOGGLE_VEST,
    )));
    let stealth = data.vest_mode == Some(VEST_STEALTH);
    let mode = button(
        if stealth { "eye-slash" } else { "fist-raised" },
        if stealth { "Stealth" } else { "Combat" },
        FLIP_VEST,
    );
    let disguise = button("user-secret", "Select", SELECT_DISGUISE);
    Box::new(bsn! {
        @Section { @title: "Agent Vest Settings", @buttons: lock }
        Children [
            @LabeledList
            Children [
                @LabeledListItem { @label: "Mode" }
                Children [ {EntityScene(mode)} ]
                --
                @LabeledListItem { @label: "Disguise" }
                Children [ {EntityScene(disguise)} ]
            ]
        ]
    })
}

// ---------------------------------------------------------------------------------------------
// ProbingConsole

impl ClientUi for ProbingConsoleUi {
    const WIDTH: f32 = 330.0;
    const HEIGHT: f32 = 207.0;

    fn content(_: &(), data: &ProbingConsoleData) -> impl Scene {
        probing_view(data)
    }
}

fn experiment_button(icon: &str, label: &str, kind: &str) -> impl Scene + use<> {
    let (icon, label) = (icon.to_owned(), label.to_owned());
    let p = params(&[(EXPERIMENT_TYPE, kind)]);
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: label }
        Act({EXPERIMENT.to_owned()})
        p
    }
}

fn probing_view(data: &ProbingConsoleData) -> impl Scene + use<> {
    let door: Box<dyn SceneList> = Box::new(EntityScene(button(
        if data.open {
            "sign-out-alt"
        } else {
            "sign-in-alt"
        },
        if data.open { "Close" } else { "Open" },
        DOOR,
    )));
    let scanner: Box<dyn SceneList> = if data.occupant {
        let name = data.occupant_name.clone().unwrap_or_default();
        let status = match data.occupant_status {
            Some(3) => "Deceased",
            Some(2) => "Unconscious",
            _ => "Conscious",
        };
        Box::new(EntityScene(bsn! {
            @LabeledList
            Children [
                {EntityScene(row("Name", name))}
                --
                {EntityScene(row("Status", status.to_owned()))}
                --
                @LabeledListItem { @label: "Experiments" }
                Children [
                    Node { flex_direction: FlexDirection::Row }
                    Children [
                        {EntityScene(experiment_button("thermometer", "Probe", "1"))}
                        --
                        {EntityScene(experiment_button("brain", "Dissect", "2"))}
                        --
                        {EntityScene(experiment_button("search", "Analyze", "3"))}
                    ]
                ]
            ]
        }))
    } else {
        Box::new(EntityScene(bsn! {
            @NoticeBox Children [ Text({"No Subject".to_owned()}) ]
        }))
    };
    let feedback = data.feedback.clone();
    bsn! {
        @WindowContent
        Children [
            @Section
            Children [
                @LabeledList
                Children [ {EntityScene(row("Machine Report", feedback))} ]
            ]
            --
            @Section { @title: "Scanner", @buttons: door }
            Children [ {scanner} ]
        ]
    }
}

// ---------------------------------------------------------------------------------------------
// GlandDispenser

impl ClientUi for GlandDispenserUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 338.0;

    fn content(_: &(), data: &GlandDispenserData) -> impl Scene {
        dispenser_view(data)
    }
}

/// `#rrggbb` as a colour (white when malformed).
fn parse_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let byte = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|b| u8::from_str_radix(b, 16).ok())
            .unwrap_or(255)
    };
    Color::srgb_u8(byte(0), byte(2), byte(4))
}

fn dispenser_view(data: &GlandDispenserData) -> impl Scene + use<> {
    let glands: Vec<_> = data
        .glands
        .iter()
        .map(|g| {
            let color = parse_color(&g.color);
            let amount = g.amount.to_string();
            let disabled = g.amount == 0;
            let p = params(&[(GLAND_ID, &g.id.to_string())]);
            bsn! {
                Node { width: px(60), height: px(60), margin: UiRect::all(px(6)), flex_direction: FlexDirection::Column }
                Children [
                    Node { height: px(12) }
                    BackgroundColor(color)
                    --
                    @ss13_ui_kit::Button { @icon: "eject", @content: amount, @fluid: true }
                    ss13_ui_kit::Button { disabled: disabled }
                    Act({DISPENSE.to_owned()})
                    p
                ]
            }
        })
        .collect();
    bsn! {
        @WindowContent
        Children [
            @Section
            Children [
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
                Children [ {glands} ]
            ]
        ]
    }
}
