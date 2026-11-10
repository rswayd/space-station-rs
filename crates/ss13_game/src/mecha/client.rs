//! The exosuit windows (client). The pilot's `Mecha` window ([`MechaUi`]: name, integrity,
//! power, the safety/ID lock/lights toggles, the diagnostic button, the equipment with the
//! sleeper's and the syringe gun's controls) and the exosuit control console
//! (`ExosuitControlConsole.tsx`: a section per tracked mech with "Message" and "EMP Burst") are
//! at the end of this file.
//!
//! The exosuit fabricator window (client): tgui `ExosuitFabricator` (`ExosuitFabricator.tsx`
//! with `Fabrication/DesignBrowser.tsx`, `MaterialAccessBar.tsx`): the researched designs by
//! category (a click builds now; `+` queues, `▶` builds now; "Всё в очередь" queues the
//! category), the materials with eject buttons, and the queue with its total cost, start/stop,
//! clear and per-part removal. The data is in [`super`], the server half in [`super::fab`].
// shortcut: no search bar, no queue progress bar (the time left is text), costs as tooltip text.

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use super::*;
use crate::science::client::{
    ALL_CATEGORY, CATEGORY, material_bar, sheets_text, sprite, top_categories,
};
use crate::science::{FabDesign, FabricatorData, FabricatorStatic};

pub struct MechaClientPlugin;

impl Plugin for MechaClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<ExosuitFabricatorUi>()
            .register_ui_view::<MechaUi>()
            .register_ui_view::<ExosuitControlConsoleUi>();
    }
}

impl ClientUi for ExosuitFabricatorUi {
    const WIDTH: f32 = 1100.0;
    const HEIGHT: f32 = 600.0;

    fn content(st: &ExoFabStatic, data: &ExoFabData) -> impl Scene {
        view(st, data, &UiLocalState::default())
    }

    fn content_local(st: &ExoFabStatic, data: &ExoFabData, local: &UiLocalState) -> impl Scene {
        view(st, data, local)
    }
}

fn categories_of(design: &ExoDesign) -> Vec<String> {
    top_categories(&FabDesign {
        categories: design.categories.clone(),
        ..default()
    })
}

fn build_params(paths: Vec<String>, now: bool) -> ActParams {
    let mut params = vec![(DESIGNS.to_owned(), paths.join(","))];
    if now {
        params.push((NOW.to_owned(), "1".to_owned()));
    }
    ActParams(params)
}

fn cost_text(st: &ExoFabStatic, design: &ExoDesign) -> String {
    let mut out = design.desc.clone();
    for (m, n) in &design.cost {
        let name = st
            .materials
            .get(usize::from(*m))
            .map_or("?", |m| m.name.as_str());
        out.push_str(&format!(
            "\n{name}: {}",
            sheets_text(f64::from(*n) / st.sheet_amount)
        ));
    }
    out
}

fn view(st: &ExoFabStatic, data: &ExoFabData, local: &UiLocalState) -> impl Scene + use<> {
    let designs: Vec<&ExoDesign> = data
        .researched
        .iter()
        .filter_map(|i| st.designs.get(usize::from(*i)))
        .collect();
    let mut categories: Vec<String> = designs.iter().flat_map(|d| categories_of(d)).collect();
    categories.sort();
    categories.dedup();
    let selected = match local.get(CATEGORY) {
        "" => ALL_CATEGORY.to_owned(),
        c => c.to_owned(),
    };
    let tab = |key: String, count: usize| {
        let label = key.trim_start_matches('/').to_owned();
        let is_selected = key == selected;
        let count = count.to_string();
        bsn! {
            @TabsTab
            TabsTab { selected: is_selected }
            SetLocal { key: {CATEGORY.to_owned()}, value: key, rerender: true }
            Children [
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    flex_grow: 1.0,
                }
                Children [ Text(label) -- Text(count) ]
            ]
        }
    };
    let mut tabs = vec![tab(ALL_CATEGORY.to_owned(), designs.len())];
    for c in &categories {
        let count = designs
            .iter()
            .filter(|d| categories_of(d).contains(c))
            .count();
        tabs.push(tab(c.clone(), count));
    }
    let mut shown: Vec<&ExoDesign> = designs
        .iter()
        .copied()
        .filter(|d| selected == ALL_CATEGORY || categories_of(d).contains(&selected))
        .collect();
    shown.sort_by(|a, b| a.name.cmp(&b.name));
    let all_paths: Vec<String> = shown.iter().map(|d| d.path.clone()).collect();
    let rows: Vec<_> = shown.into_iter().map(|d| recipe(st, data, d)).collect();
    let queue_params = build_params(all_paths, false);
    let queue_all: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @content: "Всё в очередь" }
        ss13_ui_kit::Button { color: UiColor::Transparent }
        Act({BUILD.to_owned()})
        queue_params
        Name("exofab: queue all")
    }));
    let materials = {
        let fst = FabricatorStatic {
            materials: st.materials.clone(),
            sheet_amount: st.sheet_amount,
            ..default()
        };
        let fdata = FabricatorData {
            materials: data.materials.clone(),
            ..default()
        };
        EntityScene(material_bar(&fst, &fdata))
    };
    bsn! {
        @WindowContent
        Children [
            @Stack { @fill: true }
            Children [
                @StackItem { @grow: 1.0 }
                Node { min_width: px(0) }
                Children [
                    @Stack { @vertical: true, @fill: true }
                    Children [
                        @StackItem { @grow: 1.0 }
                        Node { min_height: px(0) }
                        Children [
                            @Stack { @fill: true }
                            Children [
                                @StackItem
                                Node { width: px(220) }
                                Children [
                                    @Section { @fill: true, @scrollable: true }
                                    Children [ @Tabs { @vertical: true } Children [ {tabs} ] ]
                                ]
                                --
                                @StackItem { @grow: 1.0 }
                                Node { min_width: px(0) }
                                Children [
                                    @Section { @fill: true, @scrollable: true, @title: {selected.trim_start_matches('/').to_owned()}, @buttons: queue_all }
                                    Children [ {rows} ]
                                ]
                            ]
                        ]
                        --
                        @StackItem
                        Children [ @Section Children [ {materials} ] ]
                    ]
                ]
                --
                @StackItem
                Node { width: px(420) }
                Children [ {EntityScene(queue(st, data))} ]
            ]
        ]
    }
}

/// `Recipe`: a click builds now; `+` queues, `▶` builds now.
fn recipe(st: &ExoFabStatic, data: &ExoFabData, design: &ExoDesign) -> impl Scene + use<> {
    let can_print = data.can_print(design);
    let button = |label: &str, now: bool, name: String| {
        let params = build_params(vec![design.path.clone()], now);
        let label = label.to_owned();
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @content: label, @compact: true }
            ss13_ui_kit::Button { color: UiColor::Transparent, disabled: {!can_print} }
            Act({BUILD.to_owned()})
            params
            Name(name)
        })
    };
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            {button("+", false, format!("queue: {}", design.name))}
            --
            {button("▶", true, format!("build now: {}", design.name))}
        ]
    }));
    let image = sprite(design.icon);
    let name = design.name.clone();
    let tooltip = cost_text(st, design);
    let params = build_params(vec![design.path.clone()], true);
    let button_name = Name::new(format!("print: {}", design.name));
    bsn! {
        @ImageButton {
            @image: image,
            @image_size: 32.0,
            @fluid: true,
            @tooltip: tooltip,
            @buttons_alt: buttons,
        }
        ImageButton { disabled: {!can_print} }
        Act({BUILD.to_owned()})
        params
        button_name
        Children [ Text(name) ]
    }
}

/// `Queue` and `QueueList`.
fn queue(st: &ExoFabStatic, data: &ExoFabData) -> impl Scene + use<> {
    let mut totals: Vec<u32> = vec![0; st.materials.len()];
    for entry in &data.queue {
        if let Some(d) = st.designs.get(usize::from(entry.design)) {
            for &(m, n) in &d.cost {
                if let Some(t) = totals.get_mut(usize::from(m)) {
                    *t += n;
                }
            }
        }
    }
    let total_text: Vec<String> = totals
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| {
            format!(
                "{}: {}",
                st.materials[i].name,
                sheets_text(f64::from(*n) / st.sheet_amount)
            )
        })
        .collect();
    let empty = data.queue.is_empty();
    let control: Box<dyn SceneList> = if data.processing {
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "stop", @content: "Остановить" }
            ss13_ui_kit::Button { disabled: empty }
            Act({STOP_QUEUE.to_owned()})
            Name("exofab: stop")
        }))
    } else {
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "play", @content: "Начать сборку" }
            ss13_ui_kit::Button { disabled: empty }
            Act({BUILD_QUEUE.to_owned()})
            Name("exofab: start")
        }))
    };
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row }
        Children [
            @ss13_ui_kit::Button { @icon: "minus-circle", @content: "Очистить очередь" }
            ss13_ui_kit::Button { color: UiColor::Bad, disabled: empty }
            Act({CLEAR_QUEUE.to_owned()})
            Name("exofab: clear")
            --
            {control}
        ]
    }));
    let first_processing = data.queue.first().is_some_and(|e| e.processing);
    let rows: Vec<_> = data
        .queue
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let design = st.designs.get(usize::from(entry.design)).cloned().unwrap_or_default();
            let label = if entry.processing {
                format!("{} ({:.0} с)", design.name, entry.time_left / 10.0)
            } else {
                design.name.clone()
            };
            // `del_queue_part` indexes the queue without the part being built, from 1.
            let index = if first_processing { i } else { i + 1 };
            let remove = (!entry.processing).then(|| {
                let params = ActParams(vec![(INDEX.to_owned(), index.to_string())]);
                EntityScene(bsn! {
                    @ss13_ui_kit::Button { @icon: "minus-circle", @compact: true }
                    ss13_ui_kit::Button { color: UiColor::Transparent }
                    Act({DEL_QUEUE_PART.to_owned()})
                    params
                })
            });
            let image = sprite(design.icon);
            bsn! {
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, margin: UiRect::bottom(px(2)) }
                Children [
                    Node { width: px(32), height: px(32) } Children [ {image} ]
                    --
                    Node { flex_grow: 1.0, margin: UiRect::left(px(4)) } Children [ Text(label) ]
                    --
                    {remove}
                ]
            }
        })
        .collect();
    let totals = total_text.join("\n");
    bsn! {
        @Section { @fill: true }
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem
                Children [
                    @Section { @title: "Очередь", @buttons: buttons }
                    Children [ Text(totals) ]
                ]
                --
                @StackItem { @grow: 1.0 }
                Node { min_height: px(0) }
                Children [ @Section { @fill: true, @scrollable: true } Children [ {rows} ] ]
            ]
        ]
    }
}

// --- Mecha ----------------------------------------------------------------------------------

fn gear_params(reference: u64, action: String) -> ActParams {
    ActParams(vec![
        (REF.to_owned(), reference.to_string()),
        (GEAR_ACTION.to_owned(), action),
    ])
}

fn text_item(label: &'static str, text: String) -> Box<dyn SceneList> {
    Box::new(EntityScene(bsn! {
        @LabeledListItem { @label: label }
        Children [ Text(text) ]
    }))
}

fn bar_item(label: &'static str, value: f32, text: String) -> Box<dyn SceneList> {
    let ranges = vec![
        (UiColor::Good, 0.7, f32::INFINITY),
        (UiColor::Average, 0.3, 0.7),
        (UiColor::Bad, f32::NEG_INFINITY, 0.3),
    ];
    Box::new(EntityScene(bsn! {
        @LabeledListItem { @label: label }
        Children [
            @ProgressBar { @empty: true }
            ProgressBar { value: value, ranges: ranges }
            Node { flex_grow: 1.0, width: percent(100) }
            Children [ Text(text) ]
        ]
    }))
}

fn toggle_item(
    label: &'static str,
    on: bool,
    on_text: &'static str,
    off_text: &'static str,
    act: &'static str,
) -> Box<dyn SceneList> {
    let text = if on { on_text } else { off_text };
    let act = act.to_owned();
    Box::new(EntityScene(bsn! {
        @LabeledListItem { @label: label }
        Children [
            @ss13_ui_kit::Button { @icon: "power-off", @content: text }
            ss13_ui_kit::Button { selected: on }
            Act(act)
        ]
    }))
}

fn gear_button(
    reference: u64,
    action: String,
    label: String,
    selected: bool,
) -> Box<dyn SceneList> {
    let params = gear_params(reference, action);
    Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @content: label, @compact: true }
        ss13_ui_kit::Button { selected: selected }
        Act(EQUIP_ACT)
        params
    }))
}

fn reagent_text(reagents: &[GearReagent]) -> String {
    if reagents.is_empty() {
        return "Нет".to_owned();
    }
    reagents
        .iter()
        .map(|r| format!("{} ({} ед.)", r.name, r.volume))
        .collect::<Vec<_>>()
        .join(", ")
}

fn row(children: Vec<Box<dyn SceneList>>) -> Box<dyn SceneList> {
    Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
        Children [ {children} ]
    }))
}

/// `ModulesPane`'s sleeper snowflake.
fn sleeper_view(reference: u64, sleeper: &SleeperGear) -> Vec<Box<dyn SceneList>> {
    let Some(p) = &sleeper.patient else {
        return vec![text_item("Пациент", "Нет".to_owned())];
    };
    let mut items = vec![
        text_item("Пациент", p.patient_name.clone()),
        text_item("Состояние", p.patient_state.clone()),
        bar_item(
            "Здоровье",
            p.patient_health,
            format!("{:.0}%", p.patient_health * 100.0),
        ),
        text_item(
            "Урон",
            format!(
                "Травмы {:.0} / Ожоги {:.0} / Токсины {:.0} / Удушье {:.0}",
                p.brute_loss, p.burn_loss, p.toxin_loss, p.oxygen_loss
            ),
        ),
        text_item("Реагенты", reagent_text(&p.contained_reagents)),
    ];
    if let Some(t) = p.core_temp {
        items.push(text_item("Температура", format!("{t:.1} °C")));
    }
    if p.has_brain_damage {
        items.push(text_item("Мозг", "Обнаружены повреждения".to_owned()));
    }
    if p.has_traumas {
        items.push(text_item("Травмы мозга", "Обнаружены".to_owned()));
    }
    let mut buttons = vec![gear_button(
        reference,
        "eject".to_owned(),
        "Извлечь".to_owned(),
        false,
    )];
    for r in sleeper.injectible_reagents.iter().flatten() {
        buttons.push(gear_button(
            reference,
            format!("inject_reagent_{}", r.name),
            format!("Ввести: {} ({} ед.)", r.name, r.volume),
            false,
        ));
    }
    items.push(row(buttons));
    items
}

/// `ModulesPane`'s syringe gun snowflake.
fn syringe_view(reference: u64, gun: &SyringeGear) -> Vec<Box<dyn SceneList>> {
    let mode = gear_button(
        reference,
        "change_mode".to_owned(),
        format!("Режим: {}", gun.mode),
        false,
    );
    let mut items = vec![
        row(vec![mode]),
        text_item("Шприцы", format!("{} / {}", gun.syringe, gun.max_syringe)),
        text_item(
            "Объём",
            format!("{:.1} / {:.0} ед.", gun.reagents, gun.total_reagents),
        ),
        text_item("Содержимое", reagent_text(&gun.contained_reagents)),
    ];
    let mut synth = Vec::new();
    for (name, enabled) in &gun.analyzed_reagents {
        synth.push(gear_button(
            reference,
            format!("toggle_reagent_{name}"),
            name.clone(),
            *enabled,
        ));
        synth.push(gear_button(
            reference,
            format!("purge_reagent_{name}"),
            "Слить".to_owned(),
            false,
        ));
    }
    synth.push(gear_button(
        reference,
        "purge_all".to_owned(),
        "Слить всё".to_owned(),
        false,
    ));
    items.push(row(synth));
    items
}

fn gear_view(gear: &MechaGear) -> Box<dyn SceneList> {
    let title = format!("{}: {}", gear.slot, gear.name);
    let mut items: Vec<Box<dyn SceneList>> = Vec::new();
    if let Some(sleeper) = &gear.sleeper {
        items.extend(sleeper_view(gear.reference, sleeper));
    }
    if let Some(gun) = &gear.syringe {
        items.extend(syringe_view(gear.reference, gun));
    }
    Box::new(EntityScene(bsn! {
        @Section { @title: title }
        Children [
            @LabeledList
            Children [ {items} ]
        ]
    }))
}

impl ClientUi for MechaUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 600.0;

    fn content(_: &MechaStatic, data: &MechaData) -> impl Scene {
        let diagnostic: Vec<Box<dyn SceneList>> = if data.diagnostic_status {
            Vec::new()
        } else {
            vec![Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button {
                    @icon: "tachograph-digital",
                    @tooltip: "Диагностика",
                }
                ss13_ui_kit::Button { color: UiColor::Violet }
                Act(DIAGNOSTIC)
            }))]
        };
        let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Row }
            Children [ {diagnostic} ]
        }));
        let integrity = data.integrity / data.integrity_max.max(1.0);
        let mut rows = vec![bar_item(
            "Целостность",
            integrity,
            format!("{:.0} / {:.0}", data.integrity, data.integrity_max),
        )];
        rows.push(match (data.power_level, data.power_max) {
            (Some(level), Some(max)) => bar_item(
                "Энергия",
                (level / max.max(1.0)) as f32,
                format!("{:.0} / {:.0} Дж", level, max),
            ),
            _ => text_item("Энергия", "Нет ячейки".to_owned()),
        });
        rows.push(toggle_item(
            "Оружие",
            !data.weapons_safety,
            "Готово",
            "На предохранителе",
            TOGGLE_SAFETY,
        ));
        rows.push(toggle_item(
            "Блокировка ID",
            data.id_lock,
            "Включена",
            "Выключена",
            TOGGLE_ID_LOCK,
        ));
        if data.has_lights {
            rows.push(toggle_item(
                "Фары",
                data.lights_on,
                "Включены",
                "Выключены",
                TOGGLE_LIGHTS,
            ));
        }
        let gear: Vec<Box<dyn SceneList>> = if data.equipment.is_empty() {
            vec![Box::new(EntityScene(bsn! {
                @NoticeBox
                Children [ Text("Оборудование не установлено.") ]
            }))]
        } else {
            data.equipment.iter().map(gear_view).collect()
        };
        let title = data.name.clone();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: title, @buttons: buttons }
                Children [
                    @LabeledList
                    Children [ {rows} ]
                ]
                --
                @Section { @title: "Оборудование", @scrollable: true }
                Children [ {gear} ]
            ]
        }
    }
}

// --- ExosuitControlConsole ------------------------------------------------------------------

/// tg's percent colors: `<= 30` bad, `<= 70` average, else good (`reverse` for cargo space).
fn percent_color(value: i32, reverse: bool) -> UiColor {
    let (low, high) = if reverse {
        (UiColor::Good, UiColor::Bad)
    } else {
        (UiColor::Bad, UiColor::Good)
    };
    if value <= 30 {
        low
    } else if value <= 70 {
        UiColor::Average
    } else {
        high
    }
}

fn colored_item(label: &'static str, text: String, color: UiColor) -> Box<dyn SceneList> {
    Box::new(EntityScene(bsn! {
        @LabeledListItem { @label: label }
        Children [ Text(text) TextColor({color.base()}) ]
    }))
}

fn tracked_view(mech: &TrackedMech) -> Box<dyn SceneList> {
    let reference = mech.tracker_ref.to_string();
    let message_params = ActParams(vec![(TRACKER_REF.to_owned(), reference.clone())]);
    let shock_params = ActParams(vec![(TRACKER_REF.to_owned(), reference)]);
    let no_pilot = mech.pilot.is_empty();
    let recharging = mech.emp_recharging;
    let shock_text = if recharging {
        "Перезарядка..."
    } else {
        "ЭМИ-импульс"
    };
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row }
        Children [
            @ss13_ui_kit::Button { @icon: "envelope", @content: "Сообщение" }
            ss13_ui_kit::Button { disabled: no_pilot }
            Act(SEND_MESSAGE)
            message_params
            --
            @ss13_ui_kit::Button { @icon: "wifi", @content: shock_text }
            ss13_ui_kit::Button { color: UiColor::Bad, disabled: recharging }
            Act(SHOCK)
            shock_params
        ]
    }));
    let mut rows = vec![colored_item(
        "Целостность",
        format!("{}%", mech.integrity),
        percent_color(mech.integrity, false),
    )];
    rows.push(match mech.charge {
        Some(c) => colored_item("Заряд", format!("{c}%"), percent_color(c, false)),
        None => text_item("Заряд", "Не найдена".to_owned()),
    });
    rows.push(text_item(
        "Воздух",
        mech.airtank
            .map_or_else(|| "Не установлен".to_owned(), |p| format!("{p:.2} кПа")),
    ));
    rows.push(text_item(
        "Пилот",
        if no_pilot {
            "Нет".to_owned()
        } else {
            mech.pilot.join(" | ")
        },
    ));
    rows.push(text_item(
        "Местоположение",
        mech.location
            .clone()
            .unwrap_or_else(|| "Неизвестно".to_owned()),
    ));
    if let Some(cargo) = mech.cargo_space {
        rows.push(colored_item(
            "Занято в грузовом отсеке",
            format!("{cargo}%"),
            percent_color(cargo, true),
        ));
    }
    let title = mech.name.clone();
    Box::new(EntityScene(bsn! {
        @Section { @title: title, @buttons: buttons }
        Children [
            @LabeledList
            Children [ {rows} ]
        ]
    }))
}

impl ClientUi for ExosuitControlConsoleUi {
    const WIDTH: f32 = 500.0;
    const HEIGHT: f32 = 500.0;

    fn content(_: &(), data: &ExoControlData) -> impl Scene {
        let sections: Vec<Box<dyn SceneList>> = if data.mechs.is_empty() {
            vec![Box::new(EntityScene(bsn! {
                @NoticeBox
                Children [ Text("Экзокостюмы не обнаружены") ]
            }))]
        } else {
            data.mechs.iter().map(tracked_view).collect()
        };
        bsn! {
            @WindowContent { @scrollable: true }
            Children [ {sections} ]
        }
    }
}
