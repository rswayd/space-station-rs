//! The operating computer window (client): tgui `OperatingComputer/*`. The data is
//! `crate::operating`, the server half `crate::medical::operating`.
// shortcut: no experiments tab, no catalog search, pinning or tool filter; the body zone picker
// is a row of buttons instead of `BodyZoneSelector`'s doll; numbers are not animated.

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use crate::operating::{
    CHANGE_ZONE, NEW_ZONE, OperatingComputerData, OperatingComputerUi, OperationData, PatientData,
};

pub struct OperatingClientPlugin;

impl Plugin for OperatingClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<OperatingComputerUi>();
    }
}

/// Local state key: the selected tab (`1` patient, `2` catalog).
const TAB: &str = "tab";

impl ClientUi for OperatingComputerUi {
    const WIDTH: f32 = 430.0;
    const HEIGHT: f32 = 610.0;

    fn content(_: &(), data: &OperatingComputerData) -> impl Scene {
        view(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &OperatingComputerData, local: &UiLocalState) -> impl Scene {
        view(data, local)
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

fn view(data: &OperatingComputerData, local: &UiLocalState) -> impl Scene + use<> {
    let catalog = local.get(TAB) == "2";
    let body: Box<dyn SceneList> = if catalog {
        Box::new(EntityScene(catalog_view(data)))
    } else {
        patient_view(data)
    };
    let (patient_tab, catalog_tab) = (
        tab("Состояние пациента", "1", !catalog),
        tab("Каталог операций", "2", catalog),
    );
    bsn! {
        @WindowContent
        Children [
            Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
            Children [
                @Tabs { @fluid: true }
                Children [ {EntityScene(patient_tab)} -- {EntityScene(catalog_tab)} ]
                --
                Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
                Children [ {body} ]
                --
                @Section
                Children [
                    Text({"DefOS 1.0 © Корпорация «Nanotrasen-Deforest». Все права защищены.".to_owned()})
                ]
            ]
        ]
    }
}

fn notice(text: &str, color: UiColor) -> impl Scene + use<> {
    let text = text.to_owned();
    bsn! {
        @Section { @fill: true }
        Children [ @NoticeBox { @color: color } Children [ Text(text) ] ]
    }
}

fn patient_view(data: &OperatingComputerData) -> Box<dyn SceneList> {
    if !data.has_table {
        return Box::new(EntityScene(notice("No table detected", UiColor::Yellow)));
    }
    let Some(patient) = &data.patient else {
        return Box::new(EntityScene(notice("Пациент не обнаружен", UiColor::Red)));
    };
    let main = main_state(patient);
    let zones = zone_picker(&data.target_zone);
    let states: Vec<_> = patient
        .surgery_state
        .iter()
        .map(|s| {
            let line = format!("- {s}");
            bsn! { Node Children [ Text(line) ] }
        })
        .collect();
    let next = next_operations(&data.surgeries);
    Box::new(EntityScene(bsn! {
        @Section { @fill: true }
        Children [
            Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
            Children [
                {EntityScene(main)}
                --
                Node { flex_direction: FlexDirection::Column, padding: UiRect::all(px(6)) }
                Children [
                    {EntityScene(zones)}
                    --
                    Node { flex_direction: FlexDirection::Column }
                    Children [ {states} ]
                ]
                --
                Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
                Children [ {EntityScene(next)} ]
            ]
        ]
    }))
}

fn bar(value: f32, min: f32, max: f32, color: UiColor, label: String) -> impl Scene + use<> {
    bsn! {
        @ProgressBar { @empty: true }
        ProgressBar { value: value, min_value: min, max_value: max, color: color }
        Children [ Text(label) ]
    }
}

fn main_state(p: &PatientData) -> impl Scene + use<> {
    let stat_color = match p.statstate.as_str() {
        "good" => UiColor::Good,
        "average" => UiColor::Average,
        _ => UiColor::Bad,
    };
    let health_color = if p.health >= 0.0 {
        UiColor::Good
    } else {
        UiColor::Average
    };
    let blood_color = if p.blood_level >= p.standard_blood_level * 0.7 {
        UiColor::Good
    } else if p.blood_level >= p.standard_blood_level * 0.45 {
        UiColor::Average
    } else {
        UiColor::Bad
    };
    let health = bar(
        p.health,
        p.min_health,
        p.max_health,
        health_color,
        format!("{}%", p.health.round()),
    );
    let blood = bar(
        p.blood_level,
        0.0,
        p.standard_blood_level,
        blood_color,
        format!(
            "{}%",
            (p.blood_level / p.standard_blood_level * 100.0).round()
        ),
    );
    let damage: Vec<_> = [
        ("Ушибы", p.brute_loss),
        ("Ожоги", p.fire_loss),
        ("Токсины", p.tox_loss),
        ("Удушье", p.oxy_loss),
    ]
    .into_iter()
    .map(|(label, value)| {
        let bar = bar(
            value / p.max_health,
            0.0,
            1.0,
            UiColor::Bad,
            format!("{}", value.round()),
        );
        let label = label.to_owned();
        bsn! { @LabeledListItem { @label: label } Children [ {EntityScene(bar)} ] }
    })
    .collect();
    let stat = p.stat.clone();
    let blood_type = if p.blood_type.is_empty() {
        "Не удалось определить группу крови".to_owned()
    } else {
        p.blood_type.clone()
    };
    bsn! {
        @LabeledList
        Children [
            @LabeledListItem { @label: "Состояние", @color: stat_color, @content: stat }
            --
            @LabeledListItem { @label: "Группа крови", @content: blood_type }
            --
            @LabeledListItem { @label: "Здоровье" }
            Children [ {EntityScene(health)} ]
            --
            @LabeledListItem { @label: "Уровень крови" }
            Children [ {EntityScene(blood)} ]
            --
            {damage}
        ]
    }
}

/// `BodyZoneSelector`: the zones the server accepts, the selected one highlighted.
fn zone_picker(selected: &str) -> impl Scene + use<> {
    const ZONES: [(&str, &str); 9] = [
        ("head", "Голова"),
        ("eyes", "Глаза"),
        ("mouth", "Рот"),
        ("chest", "Грудь"),
        ("groin", "Пах"),
        ("l_arm", "Л. рука"),
        ("r_arm", "П. рука"),
        ("l_leg", "Л. нога"),
        ("r_leg", "П. нога"),
    ];
    let buttons: Vec<_> = ZONES
        .iter()
        .map(|&(zone, label)| {
            let is = zone == selected;
            let (zone, label) = (zone.to_owned(), label.to_owned());
            bsn! {
                @ss13_ui_kit::Button { @content: label, @compact: true }
                ss13_ui_kit::Button { selected: is }
                Act({CHANGE_ZONE.to_owned()})
                ActParams({vec![(NEW_ZONE.to_owned(), zone)]})
            }
        })
        .collect();
    bsn! {
        Node { flex_wrap: FlexWrap::Wrap }
        Children [ {buttons} ]
    }
}

/// `PatientStateNextOperationsView`: priority steps first.
fn next_operations(surgeries: &[OperationData]) -> impl Scene + use<> {
    let mut next: Vec<&OperationData> = surgeries.iter().filter(|o| o.show_as_next).collect();
    next.sort_by_key(|o| !o.priority);
    let empty = next.is_empty().then(|| {
        EntityScene(bsn! {
            @NoticeBox { @color: UiColor::Green }
            Children [ Text({"Нет доступных операций".to_owned()}) ]
        })
    });
    let rows: Vec<_> = next
        .into_iter()
        .map(|op| {
            let (name, tool) = short_name(op);
            let color = if op.priority {
                UiColor::Caution
            } else {
                UiColor::Default
            };
            let tooltip = if op.priority {
                format!("Рекомендуемый следующий шаг\n{}", op.desc)
            } else {
                op.desc.clone()
            };
            let content = format!("{name} — {tool}");
            bsn! {
                @ss13_ui_kit::Button { @content: content, @fluid: true, @tooltip: tooltip }
                ss13_ui_kit::Button { color: color }
            }
        })
        .collect();
    bsn! {
        @Section { @title: "Возможные операции", @fill: true, @scrollable: true }
        Children [ {empty} -- {rows} ]
    }
}

/// `extractSurgeryName`: the part in parentheses outside the catalog.
fn short_name(op: &OperationData) -> (String, String) {
    let name = match (op.name.find('('), op.name.find(')')) {
        (Some(open), Some(close)) if close > open => op.name[open + 1..close].trim().to_owned(),
        _ => op.name.clone(),
    };
    (capitalize(&name), op.tool_rec.clone())
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// `SurgeryProceduresView`: every operation the computer knows, with its requirements.
fn catalog_view(data: &OperatingComputerData) -> impl Scene + use<> {
    let entries: Vec<_> = data
        .surgeries
        .iter()
        .filter(|o| o.show_in_list)
        .map(|op| {
            let title = capitalize(&op.name);
            let lines: Vec<_> = requirement_lines(op)
                .into_iter()
                .map(|line| bsn! { Node Children [ Text(line) ] })
                .collect();
            let (desc, tool) = (op.desc.clone(), format!("Инструмент: {}", op.tool_rec));
            bsn! {
                @Section { @title: title }
                Children [
                    Node { flex_direction: FlexDirection::Column }
                    Children [
                        Node Children [ Text(desc) ]
                        --
                        Node Children [ Text(tool) ]
                        --
                        {lines}
                    ]
                ]
            }
        })
        .collect();
    bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [ {entries} ]
    }
}

/// `extractRequirementMap`: titled groups, a lone "one of" joining "all of".
fn requirement_lines(op: &OperationData) -> Vec<String> {
    let [mut all, mut any, optional, blocked] = match op.requirements.as_slice() {
        [a, b, c, d] => [a.clone(), b.clone(), c.clone(), d.clone()],
        _ => return Vec::new(),
    };
    if any.len() == 1 {
        all.append(&mut any);
    }
    let no_hard = all.is_empty();
    let groups = [
        (
            if all.len() == 1 {
                "Требуется следующее:"
            } else {
                "Все следующие требования обязательны:"
            },
            all,
        ),
        ("Дополнительно, требуется одно из следующего:", any),
        (
            match (optional.len() == 1, no_hard) {
                (true, true) => "Необязательное требование:",
                (true, false) => "Дополнительно, необязательное требование:",
                (false, true) => "Все следующие требования необязательны:",
                (false, false) => "Дополнительно, все следующие требования необязательны:",
            },
            optional,
        ),
        (
            match (blocked.len() == 1, no_hard) {
                (true, true) => "Следующее заблокирует процедуру:",
                (true, false) => "Однако, следующее заблокирует процедуру:",
                (false, true) => "Любое из следующего заблокирует процедуру:",
                (false, false) => "Однако, любое из следующего заблокирует процедуру:",
            },
            blocked,
        ),
    ];
    let mut lines = Vec::new();
    for (title, items) in groups {
        if items.is_empty() {
            continue;
        }
        lines.push(title.to_owned());
        lines.extend(items.into_iter().map(|i| format!("• {i}")));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_groups_follow_tgui() {
        let op = OperationData {
            requirements: vec![
                vec!["пациент должен лежать".into()],
                vec!["кожа должна быть разрезана или вскрыта".into()],
                vec![],
                vec!["кость должна быть целой".into()],
            ],
            ..default()
        };
        let lines = requirement_lines(&op);
        assert_eq!(lines[0], "Все следующие требования обязательны:");
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[3], "Однако, следующее заблокирует процедуру:");
        let named = OperationData {
            name: "Дефибриляция мозга (Оживление)".into(),
            ..default()
        };
        assert_eq!(short_name(&named).0, "Оживление");
    }
}
