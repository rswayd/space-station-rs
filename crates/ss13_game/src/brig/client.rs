//! Client half of the brig: the `BrigTimer` window (`BrigTimer.tsx`, BandaStation's labelled
//! presets), the `PrisonerManagement` window (`PrisonerManagement.tsx`), the labor camp
//! teleporter console (`GulagTeleporterConsole.jsx`) and the item reclaimer
//! (`GulagItemReclaimer.tsx`), the labor camp claim console (`LaborClaimConsole.jsx`).
// shortcut: implant categories are sections instead of tabs (tg `useSharedState` tabs); the
// tooltips of the implant buttons are shown, the dividers between implants are not.

use bevy::scene::EntityScene;
use ss13_tgui::client::{ActOnChange, AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::{ActParams, UiColor};

use super::*;

pub struct BrigClientPlugin;

impl Plugin for BrigClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<BrigTimerUi>()
            .register_ui_view::<PrisonerManagementUi>()
            .register_ui_view::<GulagTeleporterUi>()
            .register_ui_view::<GulagItemReclaimerUi>()
            .register_ui_view::<LaborClaimUi>();
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

/// A `LabeledList.Item` with a text value.
fn text_item(label: &'static str, value: String, color: UiColor) -> Box<dyn SceneList> {
    let label = label.to_owned();
    Box::new(EntityScene(bsn! {
        @LabeledListItem { @label: label, @color: color, @content: value }
    }))
}

/// "Teleporter Unit"/"Receiver Beacon": the location, or "Not Connected" and "Reconnect".
fn link_item(
    label: &'static str,
    location: Option<&String>,
    rescan: &'static str,
) -> Box<dyn SceneList> {
    match location {
        Some(at) => text_item(label, at.clone(), UiColor::Good),
        None => {
            let (bad, label) = (UiColor::Bad, label.to_owned());
            Box::new(EntityScene(bsn! {
                @LabeledListItem { @label: label, @color: bad }
                Children [
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                    Children [
                        Text("Not Connected")
                        --
                        @Button { @content: "Reconnect" }
                        Act(rescan)
                    ]
                ]
            }))
        }
    }
}

impl ClientUi for GulagTeleporterUi {
    const WIDTH: f32 = 350.0;
    const HEIGHT: f32 = 295.0;

    fn content(_: &(), data: &GulagTeleporterData) -> impl Scene {
        let (open, lock) = (data.teleporter_state_open, data.teleporter_lock);
        let open_text = if open { "Open" } else { "Closed" };
        let (lock_icon, lock_text) = if lock {
            ("lock", "Locked")
        } else {
            ("unlock", "Unlocked")
        };
        let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Row }
            Children [
                @Button { @content: open_text }
                Button { selected: open, disabled: lock }
                Act(TOGGLE_OPEN)
                --
                @Button { @icon: lock_icon, @content: lock_text }
                Button { selected: lock, disabled: open }
                Act(TELEPORTER_LOCK)
            ]
        }));
        let links = vec![
            link_item(
                "Teleporter Unit",
                data.teleporter_location.as_ref(),
                SCAN_TELEPORTER,
            ),
            link_item(
                "Receiver Beacon",
                data.beacon_location.as_ref(),
                SCAN_BEACON,
            ),
        ];
        let id_text = data.id_name.clone().unwrap_or_else(|| "No ID".to_owned());
        let goal = NumberInput {
            value: f64::from(data.goal),
            min_value: 1.0,
            max_value: 1000.0,
            step: 1.0,
            ..Default::default()
        };
        let on_goal = ActOnChange {
            action: SET_GOAL.into(),
            param: "value".into(),
        };
        let details = vec![
            text_item(
                "Occupant",
                data.prisoner_name
                    .clone()
                    .unwrap_or_else(|| "No Occupant".to_owned()),
                UiColor::Default,
            ),
            text_item(
                "Criminal Status",
                data.prisoner_crimstat
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "No Status".to_owned()),
                UiColor::Default,
            ),
        ];
        let blocked = !data.can_teleport;
        let bad = UiColor::Bad;
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Teleporter Console", @buttons: buttons }
                Children [ @LabeledList Children [ {links} ] ]
                --
                @Section { @title: "Prisoner Details" }
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Prisoner ID" }
                        Children [ @Button { @content: id_text, @fluid: true } Act(HANDLE_ID) ]
                        --
                        @LabeledListItem { @label: "Point Goal" }
                        Children [ Node { width: px(48) } goal on_goal ]
                        --
                        {details}
                    ]
                ]
                --
                @Button { @content: "Process Prisoner", @fluid: true }
                Button { color: bad, disabled: blocked }
                Act(TELEPORT)
            ]
        }
    }
}

impl ClientUi for GulagItemReclaimerUi {
    const WIDTH: f32 = 325.0;
    const HEIGHT: f32 = 400.0;

    fn content(_: &(), data: &GulagItemReclaimerData) -> impl Scene {
        let body: Vec<Box<dyn SceneList>> = if data.mobs.is_empty() {
            vec![Box::new(EntityScene(bsn! {
                @NoticeBox Children [ Text("No stored items") ]
            }))]
        } else {
            let blocked = !data.can_reclaim;
            let rows: Vec<Box<dyn SceneList>> = data
                .mobs
                .iter()
                .map(|(mob, name)| -> Box<dyn SceneList> {
                    let name = name.clone();
                    let params = ActParams(vec![("mobref".to_owned(), mob.clone())]);
                    Box::new(EntityScene(bsn! {
                        Node {
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                        }
                        Children [
                            Text(name)
                            --
                            @Button { @content: "Retrieve Items" }
                            Button { disabled: blocked }
                            Act(RELEASE_ITEMS)
                            params
                        ]
                    }))
                })
                .collect();
            vec![Box::new(EntityScene(bsn! {
                @Section { @title: "Stored Items" }
                Children [ {rows} ]
            }))]
        };
        bsn! {
            @WindowContent
            Children [ {body} ]
        }
    }
}

impl ClientUi for LaborClaimUi {
    const WIDTH: f32 = 315.0;
    const HEIGHT: f32 = 440.0;

    fn content(_: &(), data: &LaborClaimData) -> impl Scene {
        let mut rows = vec![text_item(
            "Status",
            data.status_info.clone(),
            UiColor::Default,
        )];
        if data.shuttle_exists {
            let blocked = !data.can_go_home;
            rows.push(Box::new(EntityScene(bsn! {
                @LabeledListItem { @label: "Shuttle controls" }
                Children [
                    @Button { @content: "Move shuttle" }
                    Button { disabled: blocked }
                    Act(MOVE_SHUTTLE)
                ]
            })));
        }
        rows.push(text_item(
            "Points",
            data.id_points.to_string(),
            UiColor::Default,
        ));
        let unclaimed = data.unclaimed_points.to_string();
        let none = data.unclaimed_points == 0;
        rows.push(Box::new(EntityScene(bsn! {
            @LabeledListItem { @label: "Unclaimed points" }
            Children [
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [
                    Text(unclaimed)
                    --
                    @Button { @content: "Claim points" }
                    Button { disabled: none }
                    Act(CLAIM_POINTS)
                ]
            ]
        })));
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [ @LabeledList Children [ {rows} ] ]
                --
                @Section { @title: "Directions" }
                Children [
                    Text("Collect boulders and ores from the mining area and bring them to the unloading machine. Then, pull the lever to smelt them into sheets. Use the production console to dispense the sheets, and then bring bring them to the claim console to earn points.\n\nBoulders cannot be smelted directly, and will require additional manual processing - strike them with a pickaxe to break them down further.\n\nSheets smelted from the labor camp's furnace will be stamped with an official seal of quality. Only stamped sheets can be claimed for points - so don't bother trying to strip down tables and chairs for bonus metal.")
                ]
            ]
        }
    }
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
