//! Client half of the records: the `SecurityRecords` and `MedicalRecords` windows (tgui
//! `SecurityRecords/*`, `MedicalRecords/*`) and the data HUD images over mobs.
// shortcut: no character preview, no print screen, no search by fingerprint; crimes are listed
// open (tg: collapsibles); the editable fields save on Enter (tg `EditableText` on blur).

use bevy::app::Propagate;
use bevy::scene::{EntityScene, SceneList};
use bevy::ui_widgets::Activate;
use ss13_net::client::{NetEntityMap, SendIntent};
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClientIntent, NetId};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{ABOVE_LIGHTING_PLANE, Appearance, Dir, IconStateId, Overlay};
use ss13_tgui::client::{
    AppTguiClientExt, ClientUi, ClientWindow, LocalOnInput, SetLocal, UiLocalState,
};
use ss13_tgui::decode;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::Bold;

use super::*;
use ss13_tgui::client::ActOnSubmit;

pub struct RecordsClientPlugin;

impl Plugin for RecordsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<SecurityRecordsUi>()
            .register_ui_view::<MedicalRecordsUi>()
            .register_ui_headless::<DataHudUi>()
            .init_resource::<DataHud>()
            .add_observer(act_with_local)
            .add_systems(Update, (receive_hud, draw_hud).chain());
    }
}

/// Local state keys.
const SEARCH: &str = "search";
const RECORD: &str = "record";
const CRIME_TAB: &str = "selectedTab";
const CRIME_NAME: &str = "crimeName";
const CRIME_DETAILS: &str = "crimeDetails";
const CRIME_FINE: &str = "crimeFine";

/// On a button: `act(action, {...params, [param]: local[key]...})`, then the keys are cleared
/// and `then` is set (tg `CrimeAuthor`'s `createCrime` + `reset`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ActWithLocal {
    pub action: String,
    pub params: Vec<(String, String)>,
    /// (param, local key).
    pub local: Vec<(String, String)>,
    /// Local key and value set after sending.
    pub then: Option<(String, String)>,
}

fn act_with_local(
    activate: On<Activate>,
    acts: Query<&ActWithLocal>,
    parents: Query<&ChildOf>,
    mut windows: Query<(&ClientWindow, &mut UiLocalState)>,
    mut intents: MessageWriter<SendIntent>,
) {
    let Ok(act) = acts.get(activate.entity) else {
        return;
    };
    let Some(root) = parents
        .iter_ancestors(activate.entity)
        .find(|e| windows.contains(*e))
    else {
        return;
    };
    let Ok((window, mut local)) = windows.get_mut(root) else {
        return;
    };
    let mut params = act.params.clone();
    for (param, key) in &act.local {
        params.push((param.clone(), local.get(key).to_owned()));
        local.set(key, "");
    }
    if let Some((key, value)) = &act.then {
        local.set(key, value);
    }
    intents.write(SendIntent(ClientIntent::UiAct {
        window: window.id,
        action: act.action.clone(),
        params,
    }));
}

fn p(pairs: &[(&str, &str)]) -> ActParams {
    ActParams(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    )
}

/// `CRIMESTATUS2COLOR`.
fn wanted_color(status: &str) -> UiColor {
    match status {
        WANTED_ARREST => UiColor::Bad,
        WANTED_DISCHARGED => UiColor::Blue,
        WANTED_PRISONER => UiColor::Average,
        WANTED_PAROLE => UiColor::Good,
        WANTED_SUSPECT => UiColor::Teal,
        _ => UiColor::Default,
    }
}

fn colored(text: String, color: UiColor) -> impl Scene {
    bsn! {
        Node
        Propagate::<TextColor>(TextColor({color.text().unwrap_or(Color::WHITE)}))
        Children [ Text(text) ]
    }
}

/// `RestrictedView`: the login screen.
fn restricted(title: &'static str) -> impl Scene {
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, justify_content: JustifyContent::Center, align_items: AlignItems::Center, row_gap: px(16) }
        Children [
            @Icon { @name: "exclamation-triangle" }
            --
            Node Propagate::<Bold>(Bold(true)) Children [ {EntityScene(colored(title.to_owned(), UiColor::Red))} ]
            --
            @NoticeBox
            Children [
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8) }
                Children [
                    Node Children [ Text("Вы не вошли.") ]
                    --
                    @ss13_ui_kit::Button { @icon: "lock-open", @content: "Войти" }
                    Act({LOGIN.to_owned()})
                    Name("records: login")
                ]
            ]
        ]
    }
}

/// The record list on the left (`RecordTabs`): search, one tab per matching record.
fn record_tabs(
    entries: Vec<(String, String, String, UiColor)>,
    local: &UiLocalState,
    purge: Option<bool>,
) -> impl Scene + use<> {
    let search = local.get(SEARCH).to_lowercase();
    let selected = local.get(RECORD).to_owned();
    let mut shown: Vec<_> = entries
        .into_iter()
        .filter(|(_, name, rank, _)| {
            search.is_empty()
                || name.to_lowercase().contains(&search)
                || rank.to_lowercase().contains(&search)
        })
        .collect();
    shown.sort_by(|a, b| a.1.cmp(&b.1));
    let empty = shown
        .is_empty()
        .then(|| EntityScene(bsn! { @NoticeBox Children [ Text("Записи не найдены.") ] }));
    let tabs: Vec<_> = shown
        .into_iter()
        .map(|(crew_ref, name, _, color)| {
            let sel = crew_ref == selected;
            let value = if sel { String::new() } else { crew_ref.clone() };
            bsn! {
                @TabsTab { @content: name, @icon: "user" }
                TabsTab { selected: sel, color: color }
                SetLocal { key: {RECORD.to_owned()}, value: value, rerender: true }
                Name("records: record")
            }
        })
        .collect();
    let purge = purge.map(|enabled| {
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "trash", @content: "Очистить", @tooltip: "Очищает всю базу данных преступников." }
            ss13_ui_kit::Button { disabled: {!enabled} }
            Act({PURGE_RECORDS.to_owned()})
        })
    });
    bsn! {
        @Stack { @vertical: true, @fill: true }
        Children [
            @StackItem
            Children [
                Input { placeholder: {"Имя/Должность".to_owned()}, fluid: true }
                LocalOnInput({SEARCH.to_owned()})
            ]
            --
            @StackItem { @grow: 1.0 }
            Node { min_height: px(0) }
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [ {empty} -- @Tabs { @vertical: true } Children [ {tabs} ] ]
            ]
            --
            @StackItem
            Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::Center }
            Children [
                @ss13_ui_kit::Button { @icon: "plus", @content: "Создать", @tooltip: "Вставьте фотографию в терминал, чтобы добавить запись." }
                ss13_ui_kit::Button { disabled: true }
                --
                {purge}
            ]
        ]
    }
}

/// `EditableText`: the value; Enter saves it as `edit_field`.
fn editable(crew_ref: &str, field: &str, value: String) -> impl Scene + use<> {
    let params = vec![
        ("ref".to_owned(), crew_ref.to_owned()),
        ("field".to_owned(), field.to_owned()),
    ];
    let node = format!("records: {field}");
    bsn! {
        Input { value: value, fluid: true, max_length: {Some(512)} }
        ActOnSubmit { action: {EDIT_FIELD.to_owned()}, param: {"value".to_owned()}, params: params }
        Name(node)
    }
}

fn field(label: &str, body: impl Scene) -> impl Scene {
    let label = label.to_owned();
    bsn! {
        @LabeledListItem { @label: label }
        Children [ {EntityScene(body)} ]
    }
}

/// The logged-in frame: tabs, the view, the logout notice.
fn auth_frame(tabs: impl Scene, view: impl Scene, footer: &'static str) -> impl Scene {
    bsn! {
        Node { flex_direction: FlexDirection::Row, flex_grow: 1.0, column_gap: px(6), min_height: px(0) }
        Children [
            Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, min_width: px(0) }
            Children [ {EntityScene(tabs)} ]
            --
            Node { flex_grow: 2.0, flex_basis: px(0), flex_direction: FlexDirection::Column, min_width: px(0), row_gap: px(6) }
            Children [
                Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, min_height: px(0) }
                Children [ {EntityScene(view)} ]
                --
                @NoticeBox { @info: true }
                Children [
                    Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::FlexEnd, align_items: AlignItems::Center, column_gap: px(8) }
                    Children [
                        Node Children [ Text(footer) ]
                        --
                        @ss13_ui_kit::Button { @icon: "lock", @content: "Выйти" }
                        ss13_ui_kit::Button { color: UiColor::Good }
                        Act({LOGOUT.to_owned()})
                        Name("records: logout")
                    ]
                ]
            ]
        ]
    }
}

fn nothing_selected() -> impl Scene {
    bsn! { @NoticeBox Children [ Text("Ничего не выбрано.") ] }
}

impl ClientUi for SecurityRecordsUi {
    const WIDTH: f32 = 750.0;
    const HEIGHT: f32 = 550.0;

    fn content(st: &AgeLimits, data: &SecurityRecordsData) -> impl Scene {
        security_view(st, data, &UiLocalState::default())
    }

    fn content_local(
        st: &AgeLimits,
        data: &SecurityRecordsData,
        local: &UiLocalState,
    ) -> impl Scene {
        security_view(st, data, local)
    }
}

fn security_view(
    _: &AgeLimits,
    data: &SecurityRecordsData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = if !data.authenticated {
        Box::new(EntityScene(restricted("Nanotrasen SecurityHUB")))
    } else {
        let entries = data
            .records
            .iter()
            .map(|r| {
                (
                    r.crew_ref.clone(),
                    r.name.clone(),
                    r.rank.clone(),
                    wanted_color(&r.wanted_status),
                )
            })
            .collect();
        let tabs = record_tabs(entries, local, Some(data.higher_access));
        let view: Box<dyn SceneList> = match data
            .records
            .iter()
            .find(|r| r.crew_ref == local.get(RECORD))
        {
            Some(record) => Box::new(EntityScene(security_record(data, record, local))),
            None => Box::new(EntityScene(nothing_selected())),
        };
        Box::new(EntityScene(auth_frame(
            tabs,
            bsn! { Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 } Children [ {view} ] },
            "Обеспечивайте безопасность рабочего места.",
        )))
    };
    bsn! {
        @WindowContent
        Children [ Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 } Children [ {body} ] ]
    }
}

/// `SecurityRecordView`: `CrimeWatcher` above `RecordInfo`.
fn security_record(
    data: &SecurityRecordsData,
    r: &SecurityRecord,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let crew = r.crew_ref.clone();
    let status_buttons: Vec<_> = data
        .available_statuses
        .iter()
        .map(|status| {
            let sel = *status == r.wanted_status;
            let color = if sel {
                wanted_color(status)
            } else {
                UiColor::Grey
            };
            let letter: String = status.chars().take(1).collect();
            let icon = if sel { "check" } else { "" };
            let params = p(&[("crew_ref", &crew), ("status", status)]);
            let node = format!("wanted: {status}");
            let tooltip = status.clone();
            bsn! {
                @ss13_ui_kit::Button { @content: letter, @icon: icon, @tooltip: tooltip }
                ss13_ui_kit::Button { color: color }
                Act({SET_WANTED.to_owned()})
                params
                Name(node)
            }
        })
        .collect();
    let status_row: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row } Children [ {status_buttons} ]
    }));
    let title = r.name.clone();
    let delete = p(&[("crew_ref", &crew)]);
    let header_buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row }
        Children [
            @ss13_ui_kit::Button { @icon: "print", @content: "Распечатать" }
            ss13_ui_kit::Button { disabled: true }
            --
            @ss13_ui_kit::Button { @icon: "trash", @content: "Удалить", @tooltip: "Удаляет запись." }
            Act({DELETE_RECORD.to_owned()})
            delete
        ]
    }));
    let status = EntityScene(colored(
        r.wanted_status.clone(),
        wanted_color(&r.wanted_status),
    ));
    let fields = vec![
        field("Имя", editable(&crew, "name", r.name.clone())),
        field("Должность", editable(&crew, "rank", r.rank.clone())),
        field("Возраст", editable(&crew, "age", r.age.to_string())),
        field("Вид", editable(&crew, "species", r.species.clone())),
        field("Пол", editable(&crew, "gender", r.gender.clone())),
        field(
            "Отпечатки",
            editable(&crew, "fingerprint", r.fingerprint.clone()),
        ),
        field("Голос", editable(&crew, "voice", r.voice.clone())),
        field(
            "Примечание",
            editable(&crew, "security_note", r.note.clone()),
        ),
    ];
    let watcher = EntityScene(crime_watcher(data, r, local));
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, row_gap: px(6), min_height: px(0) }
        Children [
            Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, min_height: px(0) }
            Children [ {watcher} ]
            --
            @Section { @title: title, @buttons: header_buttons }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Статус", @buttons: status_row }
                    Children [ {status} ]
                ]
            ]
            --
            Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, min_height: px(0) }
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [ @LabeledList Children [ {fields} ] ]
            ]
        ]
    }
}

/// `CrimeWatcher`: crimes, citations, and the new crime form.
fn crime_watcher(
    data: &SecurityRecordsData,
    r: &SecurityRecord,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let tab: u8 = match local.get(CRIME_TAB) {
        "1" => 1,
        "2" => 2,
        _ => 0,
    };
    let tab_button = |i: u8, label: String| {
        let sel = i == tab;
        let node = format!("crimes: tab {i}");
        bsn! {
            @TabsTab { @content: label }
            TabsTab { selected: sel }
            SetLocal { key: {CRIME_TAB.to_owned()}, value: {i.to_string()}, rerender: true }
            Name(node)
        }
    };
    let tabs = vec![
        tab_button(0, format!("Преступления: {}", r.crimes.len())),
        tab_button(1, format!("Штрафы: {}", r.citations.len())),
        tab_button(2, "+".to_owned()),
    ];
    let body: Box<dyn SceneList> = if tab == 2 {
        Box::new(EntityScene(crime_author(&r.crew_ref)))
    } else {
        let list = if tab == 0 { &r.crimes } else { &r.citations };
        let empty = list.is_empty().then(|| {
            let what = if tab == 0 {
                "преступления"
            } else {
                "штрафы"
            };
            let text = format!("Не найдены {what}.");
            EntityScene(bsn! { @NoticeBox Children [ Text(text) ] })
        });
        let items: Vec<_> = list
            .iter()
            .map(|c| crime_display(data, &r.crew_ref, c))
            .collect();
        Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column, row_gap: px(6) }
            Children [ {empty} -- {items} ]
        }))
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
        Children [
            @Tabs { @fluid: true } Children [ {tabs} ]
            --
            Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, min_height: px(0) }
            Children [ @Section { @fill: true, @scrollable: true } Children [ {body} ] ]
        ]
    }
}

/// `CrimeDisplay`.
fn crime_display(data: &SecurityRecordsData, crew: &str, c: &CrimeEntry) -> impl Scene + use<> {
    let short: String = c.name.chars().take(18).collect();
    let title = match c.fine {
        Some(fine) if fine > 0 => format!("{short}: {fine} кр"),
        Some(_) => format!("{short}: ШТРАФ ОПЛАЧЕН"),
        None => c.name.clone(),
    };
    let can_edit = c.valid && (data.higher_access || c.author == data.current_user);
    let mut lines = vec![
        ("Время".to_owned(), c.time.clone()),
        ("Автор".to_owned(), c.author.clone()),
        (
            "Статус".to_owned(),
            if c.valid {
                "Активно"
            } else {
                "Аннулировано"
            }
            .to_owned(),
        ),
    ];
    if !c.valid {
        lines.push((
            "Аннулировано".to_owned(),
            c.voider
                .clone()
                .unwrap_or_else(|| "Автоматически".to_owned()),
        ));
    }
    if let Some(fine) = c.fine.filter(|f| *f > 0) {
        lines.push(("Fine".to_owned(), format!("{fine}кр")));
        lines.push(("Paid".to_owned(), format!("{}кр", c.paid)));
    }
    let items: Vec<_> = lines
        .into_iter()
        .map(|(label, value)| bsn! { @LabeledListItem { @label: label, @content: value } })
        .collect();
    let details = c.details.clone();
    let invalidate = p(&[("crew_ref", crew), ("crime_ref", &c.crime_ref)]);
    let edit_params = vec![
        ("crew_ref".to_owned(), crew.to_owned()),
        ("crime_ref".to_owned(), c.crime_ref.clone()),
    ];
    let edits = can_edit.then(|| {
        let params2 = edit_params.clone();
        EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column, row_gap: px(4), margin: UiRect::top(px(4)) }
            Children [
                Input { placeholder: {"Введите новое имя".to_owned()}, fluid: true, max_length: {Some(25)}, self_clear: true }
                ActOnSubmit { action: {EDIT_CRIME.to_owned()}, param: {"name".to_owned()}, params: edit_params }
                --
                Input { placeholder: {"Введите новое описание".to_owned()}, fluid: true, max_length: {Some(1025)}, self_clear: true }
                ActOnSubmit { action: {EDIT_CRIME.to_owned()}, param: {"description".to_owned()}, params: params2 }
            ]
        })
    });
    let color = if !c.valid {
        UiColor::Grey
    } else if c.fine.is_some_and(|f| f > 0) {
        UiColor::Average
    } else {
        UiColor::Default
    };
    bsn! {
        @Section { @title: title }
        Propagate::<TextColor>(TextColor({color.text().unwrap_or(Color::WHITE)}))
        Children [
            @LabeledList Children [ {items} ]
            --
            Node { margin: UiRect::vertical(px(4)) } Children [ Text("Детали:") ]
            --
            Node { padding: UiRect::left(px(8)) } Children [ Text(details) ]
            --
            @ss13_ui_kit::Button { @icon: "ban", @content: "Аннулировать" }
            ss13_ui_kit::Button { disabled: {!can_edit} }
            Act({INVALIDATE_CRIME.to_owned()})
            invalidate
            Name("crime: invalidate")
            --
            {edits}
        ]
    }
}

/// `CrimeAuthor`: the new crime (or citation, with a fine).
fn crime_author(crew: &str) -> impl Scene + use<> {
    let submit = ActWithLocal {
        action: ADD_CRIME.to_owned(),
        params: vec![("crew_ref".to_owned(), crew.to_owned())],
        local: vec![
            ("name".to_owned(), CRIME_NAME.to_owned()),
            ("details".to_owned(), CRIME_DETAILS.to_owned()),
            ("fine".to_owned(), CRIME_FINE.to_owned()),
        ],
        then: Some((CRIME_TAB.to_owned(), "0".to_owned())),
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(6) }
        Children [
            Node Children [ Text("Имя") ]
            --
            Input { placeholder: {"Краткое описание".to_owned()}, fluid: true, max_length: {Some(25)} }
            LocalOnInput({CRIME_NAME.to_owned()})
            Name("crime: name")
            --
            Node Children [ Text("Детали") ]
            --
            Input { placeholder: {"Напишите детали...".to_owned()}, fluid: true, max_length: {Some(1025)} }
            ss13_ui_kit::TextArea { rows: 2.0 }
            LocalOnInput({CRIME_DETAILS.to_owned()})
            Name("crime: details")
            --
            Node Children [ Text("Штраф (оставьте пустым для ареста)") ]
            --
            Input { placeholder: {"0".to_owned()}, fluid: true, max_length: {Some(4)} }
            LocalOnInput({CRIME_FINE.to_owned()})
            Name("crime: fine")
            --
            @ss13_ui_kit::Button { @icon: "plus", @content: "Создать" }
            submit
            Name("crime: create")
        ]
    }
}

impl ClientUi for MedicalRecordsUi {
    const WIDTH: f32 = 750.0;
    const HEIGHT: f32 = 550.0;

    fn content(st: &MedicalRecordsStatic, data: &MedicalRecordsData) -> impl Scene {
        medical_view(st, data, &UiLocalState::default())
    }

    fn content_local(
        st: &MedicalRecordsStatic,
        data: &MedicalRecordsData,
        local: &UiLocalState,
    ) -> impl Scene {
        medical_view(st, data, local)
    }
}

fn medical_view(
    st: &MedicalRecordsStatic,
    data: &MedicalRecordsData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = if !data.authenticated {
        Box::new(EntityScene(restricted("Nanotrasen MedicalHUB")))
    } else {
        let entries = data
            .records
            .iter()
            .map(|r| {
                (
                    r.crew_ref.clone(),
                    r.name.clone(),
                    r.rank.clone(),
                    UiColor::Default,
                )
            })
            .collect();
        let tabs = record_tabs(entries, local, None);
        let view: Box<dyn SceneList> = match data
            .records
            .iter()
            .find(|r| r.crew_ref == local.get(RECORD))
        {
            Some(record) => Box::new(EntityScene(medical_record(st, record))),
            None => Box::new(EntityScene(nothing_selected())),
        };
        Box::new(EntityScene(auth_frame(
            tabs,
            bsn! { Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 } Children [ {view} ] },
            "Конфиденциальность пациентов — наш приоритет.",
        )))
    };
    bsn! {
        @WindowContent
        Children [ Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 } Children [ {body} ] ]
    }
}

/// Status buttons (first letter each) of `MedicalRecordView`.
fn status_buttons(
    crew: &str,
    options: &[String],
    current: &str,
    action: &'static str,
    param: &str,
) -> Box<dyn SceneList> {
    let buttons: Vec<_> = options
        .iter()
        .map(|status| {
            let sel = status == current;
            let color = if sel { UiColor::Good } else { UiColor::Grey };
            let letter: String = status.chars().take(1).collect();
            let params = p(&[("crew_ref", crew), (param, status)]);
            let node = format!("{action}: {status}");
            let tooltip = status.clone();
            bsn! {
                @ss13_ui_kit::Button { @content: letter, @tooltip: tooltip }
                ss13_ui_kit::Button { color: color }
                Act({action.to_owned()})
                params
                Name(node)
            }
        })
        .collect();
    Box::new(EntityScene(
        bsn! { Node { flex_direction: FlexDirection::Row } Children [ {buttons} ] },
    ))
}

/// `MedicalRecordView`: `NoteKeeper` above the record.
fn medical_record(st: &MedicalRecordsStatic, r: &MedicalRecord) -> impl Scene + use<> {
    let crew = r.crew_ref.clone();
    let notes: Vec<_> = r
        .notes
        .iter()
        .map(|n| {
            let title = format!("{} — {}", n.author, n.time);
            let content = n.content.clone();
            let params = p(&[("crew_ref", &crew), ("note_ref", &n.note_ref)]);
            let delete: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "trash" }
                ss13_ui_kit::Button { color: UiColor::Bad }
                Act({DELETE_NOTE.to_owned()})
                params
            }));
            bsn! {
                @Section { @title: title, @buttons: delete }
                Children [ Node Children [ Text(content) ] ]
            }
        })
        .collect();
    let empty = r
        .notes
        .is_empty()
        .then(|| EntityScene(bsn! { @NoticeBox Children [ Text("Заметок нет.") ] }));
    let note_params = vec![("crew_ref".to_owned(), crew.clone())];
    let blood = r.blood_type.clone();
    let blood_options = st.blood_types.clone();
    let blood_params = p(&[("crew_ref", &crew)]);
    let physical = status_buttons(
        &crew,
        &st.physical_statuses,
        &r.physical_status,
        SET_PHYSICAL_STATUS,
        "physical_status",
    );
    let mental = status_buttons(
        &crew,
        &st.mental_statuses,
        &r.mental_status,
        SET_MENTAL_STATUS,
        "mental_status",
    );
    let physical_text = r.physical_status.clone();
    let mental_text = r.mental_status.clone();
    let death = (r.physical_status == PHYSICAL_DECEASED).then(|| {
        let params = vec![("crew_ref".to_owned(), crew.clone())];
        let cause = r.cause_of_death.clone();
        EntityScene(bsn! {
            @LabeledListItem { @label: "Причина смерти" }
            Children [
                Input { value: cause, placeholder: {"Причина смерти...".to_owned()}, fluid: true }
                ActOnSubmit { action: {SET_CAUSE_OF_DEATH.to_owned()}, param: {"cause".to_owned()}, params: params }
            ]
        })
    });
    let title = r.name.clone();
    let expunge_params = p(&[("crew_ref", &crew)]);
    let expunge: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @icon: "trash", @content: "Удалить", @tooltip: "Удаляет запись." }
        Act({EXPUNGE_RECORD.to_owned()})
        expunge_params
    }));
    let fields = vec![
        field("Имя", editable(&crew, "name", r.name.clone())),
        field("Должность", editable(&crew, "rank", r.rank.clone())),
        field("Возраст", editable(&crew, "age", r.age.to_string())),
        field("Вид", editable(&crew, "species", r.species.clone())),
        field("Пол", editable(&crew, "gender", r.gender.clone())),
        field("ДНК", editable(&crew, "dna_string", r.dna.clone())),
    ];
    let minor = r.minor_disabilities.clone();
    let major = r.major_disabilities.clone();
    let quirks = r.quirk_notes.clone();
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, row_gap: px(6), min_height: px(0) }
        Children [
            Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, min_height: px(0) }
            Children [
                @Section { @title: "Заметки", @fill: true, @scrollable: true }
                Children [
                    {empty}
                    --
                    {notes}
                    --
                    Input { placeholder: {"Новая заметка...".to_owned()}, fluid: true, max_length: {Some(1024)}, self_clear: true }
                    ActOnSubmit { action: {ADD_NOTE.to_owned()}, param: {"content".to_owned()}, params: note_params }
                    Name("records: note")
                ]
            ]
            --
            Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, min_height: px(0) }
            Children [
                @Section { @title: title, @buttons: expunge, @fill: true, @scrollable: true }
                Children [
                    @LabeledList
                    Children [
                        {fields}
                        --
                        @LabeledListItem { @label: "Тип крови" }
                        Children [
                            @Dropdown { @value_param: "blood_type", @width: 6.0 }
                            Dropdown { options: blood_options, selected: blood }
                            Act({SET_BLOOD_TYPE.to_owned()})
                            blood_params
                        ]
                        --
                        @LabeledListItem { @label: "Физический статус", @buttons: physical, @content: physical_text }
                        --
                        {death}
                        --
                        @LabeledListItem { @label: "Ментальный статус", @buttons: mental, @content: mental_text }
                        --
                        @LabeledListItem { @label: "Инвалидности малой степени", @content: minor }
                        --
                        @LabeledListItem { @label: "Инвалидности высокой степени", @content: major }
                        --
                        @LabeledListItem { @label: "Особенности", @content: quirks }
                    ]
                ]
            ]
        ]
    }
}

/// The server's data HUD window and its last data.
#[derive(Resource, Default, Debug)]
pub struct DataHud {
    pub window: Option<u32>,
    pub data: DataHudData,
    /// The mobs whose overlays this drew, to clear them when they leave the data.
    drawn: Vec<(Entity, Vec<IconStateId>)>,
}

fn receive_hud(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<DataHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == DataHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                hud.window = None;
                hud.data = DataHudData::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<DataHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable data HUD: {e}"),
            }
        }
    }
}

/// The HUD images as overlays above the lighting on the seen mobs, added again whenever a
/// replicated appearance replaced them; overlays of mobs that left the data are removed.
/// Cost per frame: the entries' overlay lists (a few icons per seen human).
fn draw_hud(mut hud: ResMut<DataHud>, map: Res<NetEntityMap>, mut mobs: Query<&mut Appearance>) {
    let wanted: Vec<(Entity, Vec<IconStateId>)> = hud
        .data
        .entries
        .iter()
        .filter_map(|e| {
            let mob = map.get(NetId(e.net_id))?;
            Some((mob, e.icons.iter().map(|i| i.icon_state).collect()))
        })
        .collect();
    for (mob, icons) in &hud.drawn {
        let keep = wanted.iter().find(|(m, _)| m == mob).map(|(_, i)| i);
        let stale: Vec<IconStateId> = icons
            .iter()
            .filter(|i| keep.is_none_or(|k| !k.contains(i)))
            .copied()
            .collect();
        if stale.is_empty() {
            continue;
        }
        if let Ok(mut appearance) = mobs.get_mut(*mob) {
            appearance.overlays.retain(|o| {
                !(o.plane == Some(ABOVE_LIGHTING_PLANE) && stale.contains(&o.icon_state))
            });
        }
    }
    for (mob, icons) in &wanted {
        let Ok(mut appearance) = mobs.get_mut(*mob) else {
            continue;
        };
        for &icon in icons {
            if !appearance.overlays.iter().any(|o| o.icon_state == icon) {
                appearance.overlays.push(Overlay {
                    plane: Some(ABOVE_LIGHTING_PLANE),
                    dir: Some(Dir::South),
                    ..Overlay::new(icon)
                });
            }
        }
    }
    hud.drawn = wanted;
}
