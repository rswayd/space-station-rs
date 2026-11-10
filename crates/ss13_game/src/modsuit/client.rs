//! The `MODsuit` window (client), after `MODsuit.tsx`: the suit's parameters (power bar,
//! activation, the ID lock, core, complexity, wearer), its parts with a deploy toggle each, and
//! its modules with their costs, select/toggle and pin buttons and their configuration
//! (numbers as -/+ buttons, lists as one button per option, booleans as one toggle).
// The MODlink row (id, frequency, call/hang up) and the AI assistant row are part of the
// parameters. `MODpaint` (the paint kit's color matrix: -/+ per value, presets, confirm) is
// here too.
// shortcut: no pAI or wire sections, no module custom status panels (health readout,
// radiation counter), no per-theme window style; the layout is one column; the paint window has
// -/+ buttons (5%) instead of sliders and no preview.

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::UiColor;
use ss13_ui_kit::prelude::*;

use super::*;

pub struct ModSuitClientPlugin;

impl Plugin for ModSuitClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<ModSuitUi>()
            .register_ui_view::<ModPaintUi>();
    }
}

fn bar_color(name: &str) -> UiColor {
    match name {
        "good" => UiColor::Good,
        "average" => UiColor::Average,
        "bad" => UiColor::Bad,
        "teal" => UiColor::Teal,
        _ => UiColor::Default,
    }
}

fn params(pairs: &[(&str, String)]) -> ActParams {
    ActParams(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    )
}

fn text_row(label: &'static str, text: String) -> Box<dyn SceneList> {
    Box::new(EntityScene(bsn! {
        @LabeledListItem { @label: label }
        Children [ Text(text) ]
    }))
}

fn power_row(status: &SuitStatus) -> Box<dyn SceneList> {
    let value = if status.charge_max > 0.0 {
        (status.charge_current / status.charge_max) as f32
    } else {
        0.0
    };
    let color = bar_color(&status.chargebar_color);
    let text = status.chargebar_string.clone();
    Box::new(EntityScene(bsn! {
        @LabeledListItem { @label: "Заряд" }
        Children [
            @ProgressBar { @empty: true }
            ProgressBar { value: value, color: color }
            Node { flex_grow: 1.0, width: percent(100) }
            Children [ Text(text) ]
        ]
    }))
}

fn lock_row(locked: bool) -> Box<dyn SceneList> {
    let (icon, text) = if locked {
        ("lock", "Заблокирован")
    } else {
        ("lock-open", "Разблокирован")
    };
    Box::new(EntityScene(bsn! {
        @LabeledListItem { @label: "Блокировка ID" }
        Children [
            @Button { @icon: icon, @content: text }
            Button { selected: locked }
            Act(LOCK)
        ]
    }))
}

/// `MODsuit.tsx` `ModLink`: the id and frequency, and the call button.
fn link_row(status: &SuitStatus) -> Box<dyn SceneList> {
    let freq = status
        .link_freq
        .clone()
        .unwrap_or_else(|| "не задана".to_owned());
    let text = match &status.link_call {
        Some(other) => format!("ID {}, частота {freq}, вызов: {other}", status.link_id),
        None => format!("ID {}, частота {freq}", status.link_id),
    };
    let calling = status.link_call.is_some();
    let (icon, label) = if calling {
        ("phone-slash", "Завершить")
    } else {
        ("phone", "Вызвать")
    };
    let no_freq = status.link_freq.is_none();
    Box::new(EntityScene(bsn! {
        @LabeledListItem { @label: "MODlink" }
        Children [
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
            Children [
                Text(text)
                --
                @Button { @icon: icon, @content: label, @compact: true }
                Button { selected: calling, disabled: no_freq }
                Act(CALL)
            ]
        ]
    }))
}

fn part_row(part: &PartInfo) -> Box<dyn SceneList> {
    let deployed = part.deployed;
    let (icon, state) = if deployed {
        ("arrow-down", "развёрнуто")
    } else {
        ("arrow-up", "свёрнуто")
    };
    let text = format!("{}: {} ({state})", part.slot, part.name);
    let p = params(&[(REF, part.reference.to_string())]);
    Box::new(EntityScene(bsn! {
        Node { margin: UiRect::bottom(px(2)) }
        Children [
            @Button { @icon: icon, @content: text, @fluid: true }
            Button { selected: deployed }
            Act(DEPLOY)
            p
        ]
    }))
}

fn config_buttons(module: &ModuleInfo, config: &ModuleConfig) -> Vec<Box<dyn SceneList>> {
    let reference = module.reference.to_string();
    let button = |label: String, value: String, selected: bool| -> Box<dyn SceneList> {
        let p = params(&[
            (REF, reference.clone()),
            (KEY, config.key.clone()),
            (VALUE, value),
        ]);
        Box::new(EntityScene(bsn! {
            @Button { @content: label, @compact: true }
            Button { selected: selected }
            Act(CONFIGURE)
            p
        }))
    };
    match config.kind.as_str() {
        "number" => {
            let current: f32 = config.value.parse().unwrap_or(0.0);
            vec![
                button("-".to_owned(), format!("{}", current - 1.0), false),
                button(config.value.clone(), config.value.clone(), true),
                button("+".to_owned(), format!("{}", current + 1.0), false),
            ]
        }
        // `add_ui_configuration(.., "bool", ..)`: one toggle sending the other value.
        "bool" => {
            let on = config.value == "1";
            let label = if on { "Вкл" } else { "Выкл" };
            let next = if on { "0" } else { "1" };
            vec![button(label.to_owned(), next.to_owned(), on)]
        }
        _ => config
            .values
            .iter()
            .map(|v| button(v.clone(), v.clone(), *v == config.value))
            .collect(),
    }
}

fn module_row(module: &ModuleInfo, suit_active: bool) -> Box<dyn SceneList> {
    let passive = module.module_type == MODULE_PASSIVE;
    let usable = module.module_type == MODULE_USABLE;
    let (select_icon, select_text) = match (usable, module.module_active) {
        (true, _) => ("hand-pointer", "Использовать"),
        (false, true) => ("toggle-on", "Выключить"),
        (false, false) => ("toggle-off", "Включить"),
    };
    let reference = module.reference.to_string();
    let select_params = params(&[(REF, reference.clone())]);
    let pin_params = params(&[(REF, reference)]);
    let active = module.module_active;
    let pinned = module.pinned;
    let select_disabled = passive || !suit_active;
    let mut info = format!("Сложность: {}", module.module_complexity);
    if module.idle_power > 0.0 {
        info.push_str(&format!(", простой: {} Вт", module.idle_power.round()));
    }
    if module.active_power > 0.0 {
        info.push_str(&format!(", работа: {} Вт", module.active_power.round()));
    }
    if module.use_energy > 0.0 {
        info.push_str(&format!(
            ", использование: {}",
            display_energy(module.use_energy)
        ));
    }
    if module.cooldown > 0.0 {
        info.push_str(&format!(
            ", перезарядка: {} с",
            (module.cooldown / 10.0).round()
        ));
    }
    let title = module.module_name.clone();
    let description = module.description.clone();
    let configs: Vec<Box<dyn SceneList>> = module
        .configuration_data
        .iter()
        .map(|c| {
            let label = c.display_name.clone();
            let buttons = config_buttons(module, c);
            let row: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [
                    Text(label)
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::left(px(6)) }
                    Children [ {buttons} ]
                ]
            }));
            row
        })
        .collect();
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row }
        Children [
            @Button { @icon: select_icon, @content: select_text, @compact: true }
            Button { selected: active, disabled: select_disabled }
            Act(SELECT)
            select_params
            --
            @Button { @icon: "thumbtack", @content: "Закрепить", @compact: true }
            Button { selected: pinned, disabled: passive }
            Act(PIN)
            pin_params
        ]
    }));
    Box::new(EntityScene(bsn! {
        @Section { @title: title, @buttons: buttons }
        Children [
            Text(description)
            --
            Text(info)
            TextColor({UiColor::Label.base()})
            --
            Node { flex_direction: FlexDirection::Column }
            Children [ {configs} ]
        ]
    }))
}

impl ClientUi for ModSuitUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 640.0;

    fn content(st: &ModSuitStatic, data: &ModSuitData) -> impl Scene {
        let status = &data.suit_status;
        let (power_icon, power_text) = match (status.active, status.activating) {
            (_, true) => ("spinner", "Идёт процесс..."),
            (true, false) => ("power-off", "Деактивировать"),
            (false, false) => ("power-off", "Активировать"),
        };
        let active = status.active;
        let activating = status.activating;
        let power: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            @Button { @icon: power_icon, @content: power_text }
            Button { selected: active, disabled: activating }
            Act(ACTIVATE)
        }));
        let state = if status.activating {
            "Запуск/отключение"
        } else if status.active {
            "Активен"
        } else {
            "Неактивен"
        };
        let cover = if status.open {
            "Открыта"
        } else {
            "Закрыта"
        };
        let mut rows: Vec<Box<dyn SceneList>> = vec![
            power_row(status),
            text_row("Статус", state.to_owned()),
            lock_row(status.locked),
            text_row("Крышка", cover.to_owned()),
            text_row(
                "Ядро",
                status.core_name.clone().unwrap_or_else(|| "Нет".to_owned()),
            ),
            text_row(
                "Сложность",
                format!("{} ({})", status.complexity, st.complexity_max),
            ),
            text_row(
                "Носитель",
                if data.user_status.user_name.is_empty() {
                    "Нет".to_owned()
                } else {
                    data.user_status.user_name.clone()
                },
            ),
        ];
        let ai = match (&status.ai_name, status.is_ai) {
            (Some(name), true) => format!("{name} (вы)"),
            (Some(name), false) => name.clone(),
            (None, _) => "Нет".to_owned(),
        };
        rows.push(text_row("ИИ", ai));
        rows.push(link_row(status));
        let parts: Vec<Box<dyn SceneList>> = data.parts.iter().map(part_row).collect();
        let modules: Vec<Box<dyn SceneList>> = if data.module_info.is_empty() {
            vec![Box::new(EntityScene(bsn! {
                @NoticeBox
                Children [ Text("Модули не установлены.") ]
            }))]
        } else {
            data.module_info
                .iter()
                .map(|m| module_row(m, status.active))
                .collect()
        };
        let title = data.control.clone();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: title, @buttons: power }
                Children [
                    @LabeledList
                    Children [ {rows} ]
                ]
                --
                @Section { @title: "Части" }
                Children [ {parts} ]
                --
                @Section { @title: "Модули", @scrollable: true }
                Children [ {modules} ]
            ]
        }
    }
}

/// The paint kit's matrix: `row` (red, green, blue in) × `col` (red, green, blue out).
fn paint_value(data: &ModPaintData, row: usize, col: usize) -> f32 {
    data.current_color
        .get(row * 4 + col)
        .copied()
        .unwrap_or(0.0)
}

fn matrix_params(matrix: &[f32]) -> ActParams {
    let text: Vec<String> = matrix.iter().map(|v| format!("{v}")).collect();
    params(&[(COLOR, text.join(","))])
}

const CHANNELS: [&str; 3] = ["Красный", "Зелёный", "Синий"];

fn paint_row(data: &ModPaintData, row: usize, col: usize) -> Box<dyn SceneList> {
    let value = paint_value(data, row, col);
    let step = |delta: f32| {
        let mut m = if data.current_color.len() == 20 {
            data.current_color.clone()
        } else {
            COLOR_MATRIX_IDENTITY.to_vec()
        };
        m[row * 4 + col] = ((value + delta) * 100.0).round().clamp(0.0, 125.0) / 100.0;
        matrix_params(&m)
    };
    let (minus, plus) = (step(-0.05), step(0.05));
    let label = format!("{}:", CHANNELS[col]);
    let text = format!("{}%", (value * 100.0).round());
    Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            Text(label)
            TextColor({UiColor::Label.base()})
            --
            @Button { @content: "-", @compact: true }
            Act(TRANSITION_COLOR)
            minus
            --
            Text(text)
            --
            @Button { @content: "+", @compact: true }
            Act(TRANSITION_COLOR)
            plus
        ]
    }))
}

impl ClientUi for ModPaintUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 365.0;

    fn content(_: &(), data: &ModPaintData) -> impl Scene {
        let sections: Vec<Box<dyn SceneList>> = (0..3)
            .map(|row| {
                let title = format!("{} превращается в:", CHANNELS[row]);
                let rows: Vec<Box<dyn SceneList>> =
                    (0..3).map(|col| paint_row(data, row, col)).collect();
                let section: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                    @Section { @title: title }
                    Children [ {rows} ]
                }));
                section
            })
            .collect();
        let presets: Vec<Box<dyn SceneList>> = PAINT_PRESETS
            .iter()
            .map(|(name, matrix)| {
                let p = matrix_params(matrix);
                let label = (*name).to_owned();
                let button: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                    @Button { @content: label, @compact: true }
                    Act(TRANSITION_COLOR)
                    p
                }));
                button
            })
            .collect();
        bsn! {
            @WindowContent
            Children [
                Node { flex_direction: FlexDirection::Row }
                Children [
                    Node { flex_direction: FlexDirection::Column, width: percent(55) }
                    Children [ {sections} ]
                    --
                    Node { flex_direction: FlexDirection::Column, width: percent(45) }
                    Children [
                        @Section { @title: "Пресеты" }
                        Children [
                            Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
                            Children [ {presets} ]
                        ]
                        --
                        @Section { @title: "Матрица цвета" }
                        Children [
                            Text("Это матрица цвета: изображение правится тремя слоями — красным, зелёным и синим, а не итоговым RGB.")
                            --
                            @Button { @icon: "check", @content: "Подтвердить" }
                            Act(CONFIRM)
                        ]
                    ]
                ]
            ]
        }
    }
}
