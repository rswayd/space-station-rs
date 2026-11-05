//! Client half of the brig: the `BrigTimer` window (`BrigTimer.tsx`, BandaStation's labelled
//! presets) and the `PrisonerManagement` window (`PrisonerManagement.tsx`).
// shortcut: implant categories are sections instead of tabs (tg `useSharedState` tabs); the
// tooltips of the implant buttons are shown, the dividers between implants are not.

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::{ActParams, UiColor};

use super::*;

pub struct BrigClientPlugin;

impl Plugin for BrigClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<BrigTimerUi>()
            .register_ui_view::<PrisonerManagementUi>();
    }
}

/// A button sending `action` with `params`.
fn act_button(
    icon: &'static str,
    content: String,
    action: &'static str,
    params: Vec<(String, String)>,
) -> Box<dyn SceneList> {
    let params = ActParams(params);
    Box::new(EntityScene(bsn! {
        @Button { @icon: icon, @content: content }
        Act(action)
        params
    }))
}

/// `adjust` of the `time` act, deciseconds.
fn adjust(icon: &'static str, deciseconds: i32) -> Box<dyn SceneList> {
    act_button(
        icon,
        String::new(),
        TIME,
        vec![("adjust".to_owned(), deciseconds.to_string())],
    )
}

fn preset(minutes: u32, key: &str) -> Box<dyn SceneList> {
    act_button(
        "hourglass-start",
        format!("{minutes}m"),
        PRESET,
        vec![("preset".to_owned(), key.to_owned())],
    )
}

impl ClientUi for BrigTimerUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 138.0;

    fn content(presets: &BrigTimerPresets, data: &BrigTimerData) -> impl Scene {
        let timing = data.timing;
        let (start_text, start_act) = if timing {
            ("Stop", STOP)
        } else {
            ("Start", START)
        };
        let charging = data.flash_charging;
        let flash_text = if charging { "Recharging" } else { "Flash" };
        let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Row }
            Children [
                @Button { @icon: "clock-o", @content: start_text }
                Button { selected: timing }
                Act(start_act)
                --
                @Button { @icon: "lightbulb-o", @content: flash_text }
                Button { disabled: charging }
                Act(FLASH)
            ]
        }));
        let time = format!(" {:02}:{:02} ", data.minutes, data.seconds);
        let back = vec![adjust("fast-backward", -600), adjust("backward", -100)];
        let forward = vec![adjust("forward", 100), adjust("fast-forward", 600)];
        let presets = vec![
            preset(presets.short, PRESET_SHORT),
            preset(presets.medium, PRESET_MEDIUM),
            preset(presets.long, PRESET_LONG),
        ];
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Cell Timer", @buttons: buttons }
                Children [
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                    Children [ {back} -- Text(time) -- {forward} ]
                    --
                    Node { flex_direction: FlexDirection::Row }
                    Children [ {presets} ]
                ]
            ]
        }
    }
}

fn color_of(name: &str) -> UiColor {
    match name {
        "bad" => UiColor::Bad,
        "average" => UiColor::Average,
        "good" => UiColor::Good,
        _ => UiColor::Default,
    }
}

/// `ImplantDisplay`: the info lines and the implant's buttons.
fn implant_display(implant: &ImplantInfo) -> Box<dyn SceneList> {
    let mut rows: Vec<Box<dyn SceneList>> = implant
        .info
        .iter()
        .map(|(label, value)| -> Box<dyn SceneList> {
            let (label, value) = (label.clone(), value.clone());
            Box::new(EntityScene(bsn! {
                @LabeledListItem { @label: label, @content: value }
            }))
        })
        .collect();
    if !implant.buttons.is_empty() {
        let buttons: Vec<Box<dyn SceneList>> = implant
            .buttons
            .iter()
            .map(|b| -> Box<dyn SceneList> {
                let mut params = vec![
                    ("implant_ref".to_owned(), implant.implant_ref.clone()),
                    ("implant_action".to_owned(), b.action_key.clone()),
                ];
                params.extend(b.action_params.iter().cloned());
                let params = ActParams(params);
                let (name, tooltip, color) =
                    (b.name.clone(), b.tooltip.clone(), color_of(&b.color));
                Box::new(EntityScene(bsn! {
                    @Button { @content: name, @tooltip: tooltip }
                    Button { color: color }
                    Act(HANDLE_IMPLANT)
                    params
                }))
            })
            .collect();
        rows.push(Box::new(EntityScene(bsn! {
            @LabeledListItem { @label: "Настройки" }
            Children [
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
                Children [ {buttons} ]
            ]
        })));
    }
    Box::new(EntityScene(bsn! {
        Node { margin: UiRect::bottom(px(6)) }
        Children [ @LabeledList Children [ {rows} ] ]
    }))
}

/// `AllImplantDisplay`: the implants by category, in order of appearance.
fn all_implants(implants: &[ImplantInfo]) -> Vec<Box<dyn SceneList>> {
    if implants.is_empty() {
        return vec![Box::new(EntityScene(bsn! {
            @NoticeBox
            Children [ Text("Импланты не обнаружены.") ]
        }))];
    }
    let mut categories: Vec<&str> = Vec::new();
    for implant in implants {
        if !categories.contains(&implant.category.as_str()) {
            categories.push(&implant.category);
        }
    }
    categories
        .into_iter()
        .map(|category| -> Box<dyn SceneList> {
            let shown: Vec<Box<dyn SceneList>> = implants
                .iter()
                .filter(|i| i.category == category)
                .map(implant_display)
                .collect();
            let title = capitalized(category);
            Box::new(EntityScene(bsn! {
                @Section { @title: title }
                Children [ {shown} ]
            }))
        })
        .collect()
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect::<String>())
        .unwrap_or_default()
}

/// `IdShowcase`.
fn id_showcase(id: Option<&PrisonerIdInfo>) -> Box<dyn SceneList> {
    let Some(id) = id else {
        return Box::new(EntityScene(bsn! {
            @LabeledList
            Children [
                @LabeledListItem { @label: "ID" }
                Children [ @Button { @content: "ID не вставлена" } Act(INSERT_ID) ]
            ]
        }));
    };
    let (name, points, goal) = (id.name.clone(), id.points.to_string(), id.goal.to_string());
    let bad = UiColor::Bad;
    Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            @LabeledList
            Children [
                @LabeledListItem { @label: "ID" }
                Children [
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                    Children [ @Button { @icon: "eject" } Act(EJECT_ID) -- Text(name) ]
                ]
                --
                @LabeledListItem { @label: "Points" }
                Children [
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                    Children [
                        @Button { @icon: "times" } Button { color: bad } Act(RESET_ID)
                        --
                        Text(points)
                    ]
                ]
                --
                @LabeledListItem { @label: "Goal" }
                Children [
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                    Children [ @Button { @icon: "check" } Act(SET_ID_GOAL) -- Text(goal) ]
                ]
            ]
            --
            @NoticeBox
            Children [
                Text("Космический закон рекомендует квоты в размере 100 очков за каждую минуту, которую бы отсидел нарушитель.")
            ]
        ]
    }))
}

impl ClientUi for PrisonerManagementUi {
    const WIDTH: f32 = 465.0;
    const HEIGHT: f32 = 565.0;

    fn content(_: &(), data: &PrisonerManagementData) -> impl Scene {
        let body = if data.authorized {
            console_body(data)
        } else {
            login_body()
        };
        bsn! {
            @WindowContent
            Children [ {body} ]
        }
    }
}

/// `LogIn`.
fn login_body() -> Vec<Box<dyn SceneList>> {
    let average = UiColor::Average;
    let red = UiColor::Red.base();
    vec![Box::new(EntityScene(bsn! {
        @Section { @fill: true }
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem { @grow: 1.0 }
                --
                @StackItem { @grow: 1.0 }
                Children [
                    Node { justify_content: JustifyContent::Center }
                    Children [
                        @Icon { @name: "exclamation-triangle", @color: average, @size: 15.0 }
                    ]
                ]
                --
                @StackItem { @grow: 1.0 }
                Children [
                    Node { justify_content: JustifyContent::Center }
                    Children [ Text("Nanotrasen SecurityHUB") TextColor(red) ]
                ]
                --
                @StackItem
                Children [
                    @NoticeBox
                    Children [
                        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                        Children [
                            Text("Вы не вошли.")
                            --
                            @Button { @icon: "lock-open", @content: "Войти" }
                            Act(LOGIN)
                        ]
                    ]
                ]
            ]
        ]
    }))]
}

/// `ManagementConsole`.
fn console_body(data: &PrisonerManagementData) -> Vec<Box<dyn SceneList>> {
    let id = vec![id_showcase(data.inserted_id.as_ref())];
    let implants = all_implants(&data.implants);
    let good = UiColor::Good;
    vec![Box::new(EntityScene(bsn! {
        @Stack { @vertical: true, @fill: true }
        Children [
            @StackItem
            Children [
                @Section { @title: "Управление ID" }
                Children [ {id} ]
            ]
            --
            @StackItem { @grow: 1.0 }
            Children [
                @Section { @title: "Охранные импланты", @scrollable: true, @fill: true }
                Children [ {implants} ]
            ]
            --
            @StackItem
            Children [
                @NoticeBox { @info: true }
                Children [
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                    Children [
                        Text("Обеспечивайте безопасность рабочего места.")
                        --
                        @Button { @icon: "lock", @content: "Выйти" }
                        Button { color: good }
                        Act(LOGOUT)
                    ]
                ]
            ]
        ]
    }))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_capitalize_their_first_letter() {
        assert_eq!(capitalized("имплант слежения"), "Имплант слежения");
        assert_eq!(capitalized(""), "");
        assert_eq!(color_of("bad"), UiColor::Bad);
        assert_eq!(color_of("?"), UiColor::Default);
    }
}
