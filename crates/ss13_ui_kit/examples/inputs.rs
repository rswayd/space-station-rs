//! `cargo run -p ss13_ui_kit --example inputs`
//!
//! Input, TextArea and NumberInput side by side; every event is printed to the log line at the
//! bottom. Try Cyrillic, IME, Ctrl+arrows, Shift-selection, Ctrl+A/C/X/V, Tab, Esc, click outside,
//! dragging the number up/down and clicking it to type.

use bevy::input_focus::tab_navigation::TabGroup;
use bevy::prelude::*;
use bevy::text::FontSource;
use ss13_ui_kit::input::{
    Input, InputChanged, InputSubmitted, NumberInput, NumberInputChanged, TextArea,
};
use ss13_ui_kit::{UiKitPlugin, theme};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, UiKitPlugin))
        .add_systems(Startup, setup)
        .add_observer(
            |e: On<InputChanged>, mut log: Single<&mut Text, With<Log>>| {
                log.0 = format!("changed {:?}: {:?}", e.entity, e.value);
            },
        )
        .add_observer(
            |e: On<InputSubmitted>, mut log: Single<&mut Text, With<Log>>| {
                log.0 = format!("submitted {:?}: {:?}", e.entity, e.value);
            },
        )
        .add_observer(
            |e: On<NumberInputChanged>, mut log: Single<&mut Text, With<Log>>| {
                log.0 = format!("number {:?}: {}", e.entity, e.value);
            },
        )
        .run();
}

#[derive(Component)]
struct Log;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    // The theme's bundled font has Cyrillic; Bevy's embedded default font does not.
    let font = TextFont {
        font: FontSource::Family(theme::FONT_FAMILY.into()),
        font_size: FontSize::Px(theme::FONT_SIZE),
        ..default()
    };
    let label = |text: &str| {
        (
            Text::new(text),
            font.clone(),
            TextColor(theme::UiColor::Label.base()),
        )
    };

    commands.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            padding: px(16).all(),
            width: px(420),
            ..default()
        },
        BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
        TabGroup::new(0),
        children![
            label("Input (maxLength 20, Enter submits)"),
            (
                Input {
                    placeholder: "Введите имя...".into(),
                    max_length: Some(20),
                    ..default()
                },
                font.clone(),
            ),
            label("Input (fluid, autoSelect, selfClear)"),
            (
                Input {
                    value: "Привет, станция!".into(),
                    auto_select: true,
                    self_clear: true,
                    fluid: true,
                    ..default()
                },
                font.clone(),
            ),
            label("TextArea (Shift+Enter newline, Tab indents)"),
            (
                TextArea {
                    rows: 4.0,
                    ..default()
                },
                Input {
                    placeholder: "Многострочный текст".into(),
                    fluid: true,
                    ..default()
                },
                font.clone(),
            ),
            label("NumberInput 0..100 step 5, unit kPa; and -1..1 step 0.1"),
            (
                Node {
                    column_gap: px(8),
                    ..default()
                },
                children![
                    (
                        NumberInput {
                            value: 50.0,
                            min_value: 0.0,
                            max_value: 100.0,
                            step: 5.0,
                            unit: "кПа".into(),
                            tick_while_dragging: true,
                            ..default()
                        },
                        Node {
                            width: px(90),
                            ..default()
                        },
                        font.clone(),
                    ),
                    (
                        NumberInput {
                            value: 0.0,
                            min_value: -1.0,
                            max_value: 1.0,
                            step: 0.1,
                            step_pixel_size: 4.0,
                            format: Some(|v| format!("{v:.1}")),
                            ..default()
                        },
                        Node {
                            width: px(70),
                            ..default()
                        },
                        font.clone(),
                    ),
                ],
            ),
            (Text::new("events appear here"), font.clone(), Log),
        ],
    ));
}
