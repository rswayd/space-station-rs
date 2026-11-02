//! Draws one of tg's input modals from fixed data, for a look at it without a server.
//!
//! `cargo run -p ss13_tgui --features client --example modals_gallery -- alert`
//! (`alert`, `text`, `number`, `list`, `checkboxes`, `keycombo`, `color`), plus
//! `--screenshot out.png` to save a PNG after it settles and exit.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use ss13_net::client::{NetClientState, SendIntent};
use ss13_net::ui::client::{SendUi, UiMessageReceived};
use ss13_shared::ui::{UiServerMessage, UiStatus};
use ss13_tgui::client::TguiClientPlugin;
use ss13_tgui::interfaces::alert::{AlertStatic, AlertUi, PromptTimer};
use ss13_tgui::interfaces::checkbox_input::{CheckboxInputStatic, CheckboxInputUi};
use ss13_tgui::interfaces::color_picker::{ColorPickerStatic, ColorPickerUi};
use ss13_tgui::interfaces::key_combo::{KeyComboStatic, KeyComboUi};
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::interfaces::number_input::{NumberInputStatic, NumberInputUi};
use ss13_tgui::interfaces::text_input::{TextInputStatic, TextInputUi};
use ss13_tgui::{UiInterface, encode};
use ss13_ui_kit::UiKitPlugin;

fn message(which: &str) -> UiServerMessage {
    let timer = encode(&PromptTimer { timeout: Some(0.6) });
    let (interface, static_data, data) = match which {
        "text" => (
            TextInputUi::NAME,
            encode(&TextInputStatic {
                message: "Сообщение для остальных служителей".into(),
                placeholder: "Слава Геометру".into(),
                max_length: 1024,
                ..Default::default()
            }),
            timer,
        ),
        "number" => (
            NumberInputUi::NAME,
            encode(&NumberInputStatic {
                message: "Сколько кредитов снять?".into(),
                init_value: 100.0,
                min_value: 0.0,
                max_value: 500.0,
                round_value: true,
                swapped_buttons: true,
                ..Default::default()
            }),
            timer,
        ),
        "list" => (
            ListInputUi::NAME,
            encode(&()),
            encode(&ListInputData {
                title: String::new(),
                message: "Выберите руну".into(),
                init_value: "Руна предложения".into(),
                items: [
                    "Руна предложения",
                    "Руна усиления",
                    "Руна телепортации",
                    "Руна воскрешения",
                    "Руна барьера",
                    "Руна призыва",
                    "Руна кипения крови",
                    "Руна Явления духа",
                    "Руна апокалипсиса",
                    "Руна Нар'Си",
                ]
                .map(String::from)
                .to_vec(),
                timeout: Some(0.6),
            }),
        ),
        "checkboxes" => (
            CheckboxInputUi::NAME,
            encode(&CheckboxInputStatic {
                message: "Какие отделы оповестить?".into(),
                items: ["Инженерия", "Медицина", "Наука", "Охрана", "Снабжение"]
                    .map(String::from)
                    .to_vec(),
                min_checked: 1,
                max_checked: 3,
                swapped_buttons: true,
                ..Default::default()
            }),
            timer,
        ),
        "keycombo" => (
            KeyComboUi::NAME,
            encode(&KeyComboStatic {
                message: "Нажмите новую клавишу для «Говорить»".into(),
                init_value: "T".into(),
                swapped_buttons: true,
                ..Default::default()
            }),
            timer,
        ),
        "color" => (
            ColorPickerUi::NAME,
            encode(&ColorPickerStatic {
                message: "Выберите цвет волос".into(),
                default_color: "#a0522d".into(),
                autofocus: true,
                swapped_buttons: true,
                ..Default::default()
            }),
            timer,
        ),
        _ => (
            AlertUi::NAME,
            encode(&AlertStatic {
                message: "Вы уверены, что хотите наблюдать? Вы не сможете играть в этом раунде!"
                    .into(),
                buttons: vec!["Да".into(), "Нет".into()],
                autofocus: true,
                large_buttons: false,
                swapped_buttons: true,
            }),
            timer,
        ),
    };
    UiServerMessage::Open {
        window: 1,
        interface: interface.into(),
        title: "Вопрос".into(),
        status: UiStatus::Interactive,
        static_data,
        data,
    }
}

#[derive(Resource)]
struct Shot(Option<String>, u32);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let which = args.get(1).cloned().unwrap_or_default();
    let shot = args
        .iter()
        .skip_while(|a| *a != "--screenshot")
        .nth(1)
        .cloned();
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(bevy::window::Window {
                title: "ss13_tgui: input modals".into(),
                resolution: (760, 540).into(),
                ..default()
            }),
            ..default()
        }),
        UiKitPlugin,
        TguiClientPlugin,
    ))
    .insert_resource(NetClientState::Connected)
    .insert_resource(Shot(shot, 0))
    .add_message::<UiMessageReceived>()
    .add_message::<SendUi>()
    .add_message::<SendIntent>()
    .add_systems(
        Startup,
        move |mut commands: Commands, mut out: MessageWriter<UiMessageReceived>| {
            commands.spawn(Camera2d);
            out.write(UiMessageReceived(message(&which)));
        },
    )
    .add_systems(
        Update,
        |mut shot: ResMut<Shot>,
         mut sent: MessageReader<SendIntent>,
         mut commands: Commands,
         mut exit: MessageWriter<AppExit>| {
            for SendIntent(intent) in sent.read() {
                info!("act: {intent:?}");
            }
            let Some(path) = shot.0.clone() else {
                return;
            };
            shot.1 += 1;
            if shot.1 == 30 {
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(path));
            }
            if shot.1 == 40 {
                exit.write(AppExit::Success);
            }
        },
    );
    app.run();
}
