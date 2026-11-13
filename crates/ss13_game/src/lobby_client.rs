//! Client half of the lobby: tg's title screen (`icons/runtime/default_title.dmi`) over the map
//! with BandaStation's lobby buttons (`title_screen_html.dm`): the character name, Ready (before
//! the round starts) or Join game (after), Observe, Character setup, and the round info box
//! (`SStitle.update_info`). Drawn from the headless [`LobbyUi`] the server keeps open while the
//! client is a new player; buttons send its acts (Join game: `ClientIntent::LateJoinMenu`, which
//! opens the `JobSelection` window). Character setup opens the [`JobPrefsUi`] window, the jobs
//! page of tg's preferences menu.

use bevy::app::Propagate;
use bevy::scene::{CommandsSceneExt, EntityScene, Scene, SceneList};
use ss13_net::client::{NetClientState, SendIntent};
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, UiSprite};
use ss13_tgui::interfaces::job_selection::{DepartmentStatic, darken, parse_hex};
use ss13_tgui::interfaces::ru_jobs::{department_ru, job_ru};
use ss13_tgui::{UiInterface, decode};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx, root_text_style};

use crate::lobby::{
    APPEARANCE_EYE_COLOR, APPEARANCE_FEMALE, APPEARANCE_HAIR_COLOR, APPEARANCE_HAIR_STYLE,
    APPEARANCE_MUTANT_COLOR, APPEARANCE_SKIN_TONE, APPEARANCE_SPECIES, AppearanceData,
    CHARACTER_SETUP, GIVE_QUIRK, GameState, JOBLESS_ROLE, JobPrefsData, JobPrefsStatic, JobPrefsUi,
    JobPriority, JoblessRole, LobbyData, LobbyUi, OBSERVE, RANDOMIZE_APPEARANCE, REMOVE_QUIRK,
    SET_ANTAGS, SET_APPEARANCE, SET_JOB_PREFERENCE, SET_PREFERENCE, TOGGLE_ANTAG_ROLL,
    TOGGLE_READY,
};
use crate::lobby::{CLEAR_PERSONALITIES, HANDLE_PERSONALITY};
use crate::lobby::{
    CrewManifestData, CrewManifestUi, DELAY, MANIFEST, NOTICE, RESET_HUD, START_NOW, TRAIT_PARAM,
    TRAIT_SIGNUP,
};

/// tg `/datum/hud/new_player` title: `icons/runtime/default_title.dmi`, the unnamed state.
const TITLE_DMI: &str = "icons/runtime/default_title.dmi";
/// Client-only acts of the lobby screen.
const LATE_JOIN: &str = "late_join";
const OBSERVE_ASK: &str = "observe_ask";
const OBSERVE_CANCEL: &str = "observe_cancel";
/// Button texts. Scripts click the buttons by their `Name`s (`click_ui "lobby: ready"`; the
/// lobby greeting in the chat contains "Готов" too) and the job preference buttons as
/// `"<title>: <text>"` (`click_ui "Security Officer: Выс."`).
const READY_TEXT: &str = "Готов";
const JOIN_TEXT: &str = "Присоединиться";
const OBSERVE_TEXT: &str = "Наблюдать";
const SETUP_TEXT: &str = "Настройка персонажа";
const MANIFEST_TEXT: &str = "Манифест персонала";

pub struct LobbyClientPlugin;

impl Plugin for LobbyClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<LobbyUi>()
            .register_ui_view::<JobPrefsUi>()
            .register_ui_view::<CrewManifestUi>()
            .init_resource::<Lobby>()
            .add_systems(Startup, spawn_lobby)
            .add_systems(
                Update,
                (
                    receive_lobby,
                    forget_on_disconnect.run_if(resource_changed::<NetClientState>),
                    draw_lobby.run_if(resource_changed::<Lobby>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                )
                    .chain(),
            )
            .add_systems(PostUpdate, find_title);
    }
}

/// The server's lobby window and its last data; `window` is `None` outside the lobby.
#[derive(Resource, Default, Debug)]
pub struct Lobby {
    pub window: Option<u32>,
    pub data: LobbyData,
    /// tg's "Are you sure you wish to observe?" is being asked.
    confirm_observe: bool,
}

#[derive(Component, Clone, Default)]
struct LobbyRoot;

#[derive(Component, Clone, Default)]
struct LobbyPanel;

#[derive(Component, Clone, Default)]
struct TitleImage;

fn spawn_lobby(mut commands: Commands) {
    commands
        .spawn_scene(bsn! {
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                display: Display::None,
            }
            BackgroundColor(Color::BLACK)
            LobbyRoot
            ActScope
            @root_text_style()
            Children [
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    top: px(0),
                    bottom: px(0),
                }
                UiSprite
                TitleImage
                --
                Node {
                    position_type: PositionType::Absolute,
                    left: px(16),
                    top: px(16),
                    width: px(280),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(10)),
                    border_radius: BorderRadius::all(px(4)),
                }
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7))
                LobbyPanel
            ]
        })
        .observe(on_act);
}

/// The title's icon state, once the atlas manifest is there.
fn find_title(
    atlas: Option<Res<IconAtlas>>,
    mut titles: Query<&mut UiSprite, With<TitleImage>>,
    mut done: Local<bool>,
) {
    let Some(atlas) = atlas.filter(|_| !*done) else {
        return;
    };
    if titles.is_empty() {
        return;
    }
    *done = true;
    let id = atlas.manifest.index().get(&(TITLE_DMI, "")).copied();
    if id.is_none() {
        warn!("no {TITLE_DMI} in the atlas: the lobby has a plain background");
    }
    for mut sprite in &mut titles {
        sprite.icon_state = id;
    }
}

fn receive_lobby(mut received: MessageReader<UiMessageReceived>, mut lobby: ResMut<Lobby>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == LobbyUi::NAME => {
                lobby.window = Some(*window);
                lobby.confirm_observe = false;
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if lobby.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if lobby.window == Some(*window) => {
                *lobby = Lobby::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data {
            match decode::<LobbyData>(data) {
                Ok(data) => lobby.data = data,
                Err(e) => warn!("undecodable lobby data: {e}"),
            }
        }
    }
}

/// A new session starts outside the lobby until the server says otherwise.
fn forget_on_disconnect(state: Res<NetClientState>, mut lobby: ResMut<Lobby>) {
    if *state != NetClientState::Connected && lobby.window.is_some() {
        *lobby = Lobby::default();
    }
}

fn on_act(act: On<UiAct>, mut lobby: ResMut<Lobby>, mut intents: MessageWriter<SendIntent>) {
    let Some(window) = lobby.window else {
        return;
    };
    match act.action.as_str() {
        LATE_JOIN => {
            intents.write(SendIntent(ClientIntent::LateJoinMenu));
        }
        OBSERVE_ASK => lobby.confirm_observe = true,
        OBSERVE_CANCEL => lobby.confirm_observe = false,
        action => {
            lobby.confirm_observe = false;
            intents.write(SendIntent(ClientIntent::UiAct {
                window,
                action: action.to_owned(),
                params: act.params.clone(),
            }));
        }
    }
}

fn draw_lobby(
    lobby: Res<Lobby>,
    mut roots: Query<&mut Node, With<LobbyRoot>>,
    panels: Query<(Entity, Option<&Children>), With<LobbyPanel>>,
    mut commands: Commands,
) {
    let display = if lobby.window.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut roots {
        if node.display != display {
            node.display = display;
        }
    }
    if lobby.window.is_none() {
        return;
    }
    for (panel, children) in &panels {
        for child in children.into_iter().flatten() {
            commands.entity(*child).despawn();
        }
        commands
            .spawn_scene(panel_scene(&lobby.data, lobby.confirm_observe))
            .insert(ChildOf(panel));
    }
}

/// BandaStation `SStitle.update_info`: the time left or the round's state.
fn time_remaining(data: &LobbyData) -> String {
    match data.state {
        GameState::Startup => "Загрузка...".into(),
        GameState::Pregame => match data.time_left {
            Some(secs) if secs > 0 => format!("{secs} сек."),
            _ => "-".into(),
        },
        GameState::SettingUp => "Раунд начинается...".into(),
        GameState::Playing => "Раунд идёт".into(),
        GameState::Finished => "Раунд закончился".into(),
    }
}

fn panel_scene(data: &LobbyData, confirm_observe: bool) -> impl Scene + use<> {
    let name = data.name.clone();
    let before_start = data.state <= GameState::Pregame;
    let (icon, color) = if data.ready {
        ("check-square-o", UiColor::Good)
    } else {
        ("square-o", UiColor::Bad)
    };
    let ready = before_start.then(|| {
        EntityScene(bsn! {
            @Button { @content: READY_TEXT, @icon: icon, @fluid: true }
            Button { color: color }
            Act({TOGGLE_READY.to_owned()})
            Name("lobby: ready")
        })
    });
    let join = (!before_start).then(|| {
        EntityScene(bsn! {
            @Button { @content: JOIN_TEXT, @icon: "sign-in-alt", @fluid: true }
            Act({LATE_JOIN.to_owned()})
            Name("lobby: join")
        })
    });
    // BandaStation's "Антагонисты" checkbox, before and after the round start.
    let (antag_icon, antag_color) = if data.antag_roll {
        ("check-square-o", UiColor::Good)
    } else {
        ("square-o", UiColor::Bad)
    };
    let antag_roll = bsn! {
        @Button {
            @content: "Антагонисты",
            @icon: antag_icon,
            @fluid: true,
            @tooltip: "Разрешить выбирать вас для ролей антагонистов",
        }
        Button { color: antag_color }
        Act({TOGGLE_ANTAG_ROLL.to_owned()})
        Name("lobby: antag roll")
    };
    let confirm = confirm_observe.then(|| {
        EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                Node { margin: UiRect::bottom(px(4)) }
                Children [ Text("Вы уверены? В этом раунде вы уже не сможете играть!") ]
                --
                Node { flex_direction: FlexDirection::Row }
                Children [
                    @Button { @content: "Да" }
                    Button { color: UiColor::Bad }
                    Act({OBSERVE.to_owned()})
                    Name("lobby: observe yes")
                    --
                    @Button { @content: "Нет" }
                    Act({OBSERVE_CANCEL.to_owned()})
                ]
            ]
        })
    });
    let observe = (!confirm_observe).then(|| {
        EntityScene(bsn! {
            @Button { @content: OBSERVE_TEXT, @icon: "eye", @fluid: true }
            Act({OBSERVE_ASK.to_owned()})
            Name("lobby: observe")
        })
    });
    let mut info = if before_start {
        vec![format!(
            "Игроков готово: {} / {}",
            data.ready_players, data.players
        )]
    } else {
        vec![
            format!("Всего игроков: {}", data.players),
            format!("Игроков в лобби: {}", data.in_lobby),
        ]
    };
    if let Some(url) = &data.source_url {
        info.push(format!("Исходный код: {url}"));
    }
    let info: Vec<_> = info
        .into_iter()
        .map(|line| bsn! { Node Children [ Text(line) ] })
        .collect();
    let time = time_remaining(data);
    // BandaStation `updateNotice`: big red text.
    let notice = data.notice.clone().map(|text| {
        EntityScene(bsn! {
            Node { margin: UiRect::bottom(px(8)) }
            Propagate::<FontPx>(FontPx(16.0))
            Propagate::<Bold>(Bold(true))
            Propagate::<TextColor>(TextColor(Color::srgb(1.0, 0.2, 0.2)))
            Children [ Text(text) ]
        })
    });
    // BandaStation `create_trait_buttons`.
    let traits: Vec<_> = data
        .traits
        .iter()
        .map(|t| {
            let (icon, color) = if t.signed_up {
                ("check-square-o", UiColor::Good)
            } else {
                ("square-o", UiColor::Bad)
            };
            let (content, tooltip, label) = (
                t.name.clone(),
                t.desc.clone(),
                format!("lobby: trait {}", t.name),
            );
            let params = vec![(TRAIT_PARAM.to_owned(), t.name.clone())];
            bsn! {
                @Button { @content: content, @icon: icon, @fluid: true, @tooltip: tooltip }
                Button { color: color }
                Act({TRAIT_SIGNUP.to_owned()})
                ActParams(params)
                Name(label)
            }
        })
        .collect();
    // BandaStation's admin buttons (`toggleAdmin`); the server checks the rights again.
    let admin = data.admin.then(|| {
        EntityScene(bsn! {
            Node {
                flex_direction: FlexDirection::Column,
                margin: UiRect::top(px(8)),
                padding: UiRect::top(px(6)),
                border: UiRect::top(px(1)),
            }
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.3))
            Children [
                @Button { @content: "Запустить раунд", @icon: "play", @fluid: true }
                Act({START_NOW.to_owned()})
                Name("lobby: start now")
                --
                @Button { @content: "Отложить начало", @icon: "hourglass", @fluid: true }
                Act({DELAY.to_owned()})
                Name("lobby: delay")
                --
                @Button { @content: "Уведомление", @icon: "exclamation", @fluid: true }
                Act({NOTICE.to_owned()})
                Name("lobby: notice")
            ]
        })
    });
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            Node { margin: UiRect::bottom(px(8)) }
            Propagate::<FontPx>(FontPx(18.0))
            Propagate::<Bold>(Bold(true))
            Children [ Text(name) ]
            --
            {notice}
            --
            {ready}
            --
            {join}
            --
            {EntityScene(antag_roll)}
            --
            {confirm}
            --
            {observe}
            --
            @Button { @content: SETUP_TEXT, @icon: "user-cog", @fluid: true }
            Act({CHARACTER_SETUP.to_owned()})
            Name("lobby: setup")
            --
            @Button { @content: MANIFEST_TEXT, @icon: "clipboard-list", @fluid: true }
            Act({MANIFEST.to_owned()})
            Name("lobby: manifest")
            --
            {traits}
            --
            @Button {
                @content: "Сбросить HUD лобби",
                @icon: "redo",
                @fluid: true,
                @tooltip: "Reset Lobby Menu HUD",
            }
            Act({RESET_HUD.to_owned()})
            Name("lobby: reset hud")
            --
            Node {
                flex_direction: FlexDirection::Column,
                margin: UiRect::top(px(8)),
                padding: UiRect::top(px(6)),
                border: UiRect::top(px(1)),
            }
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.3))
            Children [
                Node
                Propagate::<Bold>(Bold(true))
                Children [ Text("До начала раунда") ]
                --
                Node Children [ Text(time) ]
                --
                {info}
            ]
            --
            {admin}
        ]
    }
}

impl ClientUi for CrewManifestUi {
    const WIDTH: f32 = 350.0;
    const HEIGHT: f32 = 500.0;

    /// tg `CrewManifest.jsx`: a section per department with its open positions (none for
    /// "Misc"), name and rank rows, an infinity mark on unlimited jobs.
    // shortcut: no captain star or command chevron (the entries carry no trim).
    fn content(_: &(), data: &CrewManifestData) -> impl Scene {
        let sections: Vec<_> = data
            .manifest
            .iter()
            .map(|dept| {
                let positions = data.positions.iter().find(|p| p.name == dept.name);
                let mut title = department_ru(&dept.name).unwrap_or(&dept.name).to_owned();
                if dept.name != "Misc" {
                    let open = positions.map_or(0, |p| p.open);
                    title.push_str(&format!(" (позиций открыто: {open})"));
                }
                let rows: Vec<_> = dept
                    .entries
                    .iter()
                    .map(|(name, rank)| {
                        let unlimited = positions.is_some_and(|p| p.exceptions.contains(rank));
                        let mark = if unlimited { "∞" } else { "" }.to_owned();
                        let (name, rank) = (name.clone(), rank.clone());
                        bsn! {
                            @TableRow
                            Children [
                                @TableCell { @content: name }
                                --
                                @TableCell { @content: mark }
                                --
                                @TableCell { @content: rank }
                            ]
                        }
                    })
                    .collect();
                bsn! {
                    @Section { @title: title }
                    Children [ @Table Children [ {rows} ] ]
                }
            })
            .collect();
        bsn! {
            @Section { @fill: true, @scrollable: true }
            Children [ {sections} ]
        }
    }
}

fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<LobbyRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

/// `JobsPage`: three columns of departments, as tg lays them out.
const COLUMNS: usize = 3;
const ROW_PX: f32 = 24.0;
const TITLE_PX: f32 = 30.0;

impl ClientUi for JobPrefsUi {
    const WIDTH: f32 = 1012.0;
    const HEIGHT: f32 = 700.0;

    fn content(st: &JobPrefsStatic, data: &JobPrefsData) -> impl Scene {
        let jobless: Vec<_> = JoblessRole::ALL
            .into_iter()
            .map(|role| {
                let text = match role {
                    JoblessRole::BeOverflow => format!(
                        "Присоединиться за {}",
                        job_ru(&st.overflow).unwrap_or(&st.overflow)
                    ),
                    JoblessRole::BeRandomJob => "Выбрать случайную должность".to_owned(),
                    JoblessRole::ReturnToLobby => "Вернуться в лобби".to_owned(),
                };
                let params = vec![
                    ("preference".to_owned(), JOBLESS_ROLE.to_owned()),
                    ("value".to_owned(), (role as u8).to_string()),
                ];
                let selected = data.jobless_role == role;
                bsn! {
                    @Button { @content: text }
                    Button { selected: selected }
                    Act({SET_PREFERENCE.to_owned()})
                    ActParams(params)
                }
            })
            .collect();
        // CSS-like columns: departments in order, split where the running height passes each
        // column's share.
        let heights: Vec<f32> = st
            .departments
            .iter()
            .map(|d| TITLE_PX + ROW_PX * d.jobs.len() as f32)
            .collect();
        let total: f32 = heights.iter().sum::<f32>().max(1.0);
        let mut columns: Vec<Vec<usize>> = vec![Vec::new(); COLUMNS];
        let mut running = 0.0;
        for (i, h) in heights.iter().enumerate() {
            let column = ((running + h / 2.0) / (total / COLUMNS as f32)) as usize;
            columns[column.min(COLUMNS - 1)].push(i);
            running += h;
        }
        let columns: Vec<_> = columns
            .into_iter()
            .map(|indexes| {
                let departments: Vec<_> = indexes
                    .into_iter()
                    .map(|i| department(&st.departments[i], st, data))
                    .collect();
                bsn! {
                    Node {
                        flex_direction: FlexDirection::Column,
                        flex_basis: px(0),
                        flex_grow: 1.0,
                        min_width: px(0),
                    }
                    Children [ {departments} ]
                }
            })
            .collect();
        // tg's antagonists page, reduced to the roles this server has.
        let antags: Vec<_> = [
            (crate::antag::ROLE_TRAITOR, "Предатель"),
            (crate::antag::ROLE_BROTHER, "Кровный брат"),
            (crate::antag::vampire::ROLE_VAMPIRE, "Вампир"),
            (crate::antag::changeling::ROLE_CHANGELING, "Генокрад"),
            (crate::antag::nukeop::ROLE_OPERATIVE, "Ядерный оперативник"),
            (crate::antag::cult::ROLE_CULTIST, "Культист"),
            (crate::antag::heretic::ROLE_HERETIC, "Еретик"),
            (crate::antag::revolution::ROLE_REV_HEAD, "Глава революции"),
            (crate::antag::ROLE_MALF, "Сбойный ИИ"),
            (
                crate::antag::ROLE_SYNDICATE_INFILTRATOR,
                "Диверсант Синдиката",
            ),
            (crate::antag::ROLE_SLEEPER_AGENT, "Спящий агент Синдиката"),
            (
                crate::antag::ROLE_STOWAWAY_CHANGELING,
                "Генокрад-безбилетник",
            ),
            (crate::antag::revolution::ROLE_PROVOCATEUR, "Провокатор"),
            (crate::antag::ROLE_OPERATIVE_MIDROUND, "Ядерный штурмовик"),
            (crate::antag::wizard::ROLE_WIZARD, "Волшебник"),
            (
                crate::antag::wizard::ROLE_WIZARD_MIDROUND,
                "Волшебник (мидраунд)",
            ),
            (crate::antag::ROLE_PARADOX_CLONE, "Парадокс-клон"),
            (crate::antag::ROLE_NIGHTMARE, "Кошмар"),
            (crate::antag::ROLE_SPACE_DRAGON, "Космический дракон"),
            (crate::antag::ROLE_REVENANT, "Ревенант"),
            (crate::antag::ROLE_BLOB, "Блоб"),
            (crate::antag::ROLE_BLOB_INFECTION, "Блоб-носитель"),
            (crate::antag::ROLE_ALIEN, "Ксеноморф"),
            (crate::antag::ROLE_NINJA, "Космический ниндзя"),
            (crate::antag::ROLE_FUGITIVE, "Беглец"),
            (crate::antag::ROLE_SPY, "Шпион"),
            (
                crate::antag::changeling::ROLE_CHANGELING_MIDROUND,
                "Генокрад (мидраунд)",
            ),
        ]
        .into_iter()
        .map(|(role, label)| {
            let on = data.be_special.iter().any(|r| r == role);
            let params = vec![
                ("antags".to_owned(), role.to_owned()),
                ("toggled".to_owned(), if on { "0" } else { "1" }.to_owned()),
            ];
            bsn! {
                @Button {
                    @content: label,
                    @icon: {if on { "check-square-o" } else { "square-o" }},
                    @fluid: true,
                    @ellipsis: true,
                    @tooltip: label,
                }
                Button { selected: on }
                Act({SET_ANTAGS.to_owned()})
                ActParams(params)
                Name({format!("antag: {role}")})
            }
        })
        .collect();
        // tg's quirks page (`QuirksPage.tsx`) as a grid of toggles; the server checks the points.
        let quirks: Vec<_> = st
            .quirks
            .iter()
            .map(|quirk| {
                let on = data.quirks.contains(&quirk.name);
                let label = format!("{} ({:+})", quirk.name, quirk.value);
                let tooltip = quirk.description.clone();
                let action = if on { REMOVE_QUIRK } else { GIVE_QUIRK };
                let params = vec![("quirk".to_owned(), quirk.name.clone())];
                let name = format!("quirk: {}", quirk.name);
                bsn! {
                    @Button {
                        @content: label,
                        @icon: {if on { "check-square-o" } else { "square-o" }},
                        @fluid: true,
                        @ellipsis: true,
                        @tooltip: tooltip,
                    }
                    Button { selected: on }
                    Act({action.to_owned()})
                    ActParams(params)
                    Name(name)
                }
            })
            .collect();
        let quirk_balance = format!(
            "Очки квирков: {} (положительных не больше {})",
            data.quirk_balance, st.max_positive_quirks
        );
        // tg's personality page (`PersonalityPage.tsx`) as a grid of toggles; the server checks
        // the limit and the groups.
        let personalities: Vec<_> = st
            .personalities
            .iter()
            .map(|p| {
                let on = data.personalities.contains(&p.key);
                let mut tooltip = p.description.clone();
                for (sign, text) in [
                    ("+", &p.pos_gameplay_description),
                    ("-", &p.neg_gameplay_description),
                    ("±", &p.neut_gameplay_description),
                ] {
                    if !text.is_empty() {
                        tooltip.push_str(&format!("\n{sign} {text}"));
                    }
                }
                let params = vec![("personality_type".to_owned(), p.key.clone())];
                let name = format!("personality: {}", p.key);
                bsn! {
                    @Button {
                        @content: {p.name.clone()},
                        @icon: {if on { "check-square-o" } else { "square-o" }},
                        @fluid: true,
                        @ellipsis: true,
                        @tooltip: tooltip,
                    }
                    Button { selected: on }
                    Act({HANDLE_PERSONALITY.to_owned()})
                    ActParams(params)
                    Name(name)
                }
            })
            .collect();
        let personality_count = if st.max_personalities < 0 {
            format!("Выбрано личностей: {}", data.personalities.len())
        } else {
            format!(
                "Выбрано личностей: {} из {}",
                data.personalities.len(),
                st.max_personalities
            )
        };
        let appearance = appearance_rows(st, &data.appearance);
        bsn! {
            // The whole page scrolls: the sections above the jobs outgrow the window.
            @WindowContent { @scrollable: true }
            Children [
                // shortcut: tg's character page is its own tab with a preview and color pickers;
                // here one section with the random tables' choices.
                @Section { @title: "Внешность" }
                Children [
                    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(12), row_gap: px(4) }
                    Children [ {appearance} ]
                ]
                --
                @Section { @title: "Антагонисты" }
                Children [
                    // Equal cells, four to a row (tg's antagonists page is a grid of cards).
                    Node {
                        display: Display::Grid,
                        grid_template_columns: {RepeatedGridTrack::flex::<Vec<RepeatedGridTrack>>(4, 1.0)},
                        column_gap: px(4),
                        row_gap: px(2),
                    }
                    Children [ {antags} ]
                ]
                --
                @Section { @title: "Квирки" }
                Children [
                    Node { margin: UiRect::bottom(px(4)) }
                    Children [ Text(quirk_balance) ]
                    --
                    Node {
                        display: Display::Grid,
                        grid_template_columns: {RepeatedGridTrack::flex::<Vec<RepeatedGridTrack>>(4, 1.0)},
                        column_gap: px(4),
                        row_gap: px(2),
                    }
                    Children [ {quirks} ]
                ]
                --
                @Section { @title: "Личность" }
                Children [
                    Node { flex_direction: FlexDirection::Row, column_gap: px(8), margin: UiRect::bottom(px(4)) }
                    Children [
                        Text(personality_count)
                        --
                        @Button { @content: "Сбросить", @compact: true }
                        Act({CLEAR_PERSONALITIES.to_owned()})
                        Name("personality: clear")
                    ]
                    --
                    Node {
                        display: Display::Grid,
                        grid_template_columns: {RepeatedGridTrack::flex::<Vec<RepeatedGridTrack>>(4, 1.0)},
                        column_gap: px(4),
                        row_gap: px(2),
                    }
                    Children [ {personalities} ]
                ]
                --
                @Section { @title: "Что делать если не удалось войти?" }
                Children [
                    Node { flex_direction: FlexDirection::Row }
                    Children [ {jobless} ]
                ]
                --
                @Section
                Children [
                    Node { flex_direction: FlexDirection::Row, column_gap: px(8) }
                    Children [ {columns} ]
                ]
            ]
        }
    }
}

fn department(
    department: &DepartmentStatic,
    st: &JobPrefsStatic,
    data: &JobPrefsData,
) -> impl Scene + use<> {
    let rgb = parse_hex(&department.color).unwrap_or([0x96, 0x89, 0xdb]);
    // `--department-color`: the department color darkened by 10%.
    let color = darken(rgb, 10.0);
    let name = department_ru(&department.name)
        .unwrap_or(&department.name)
        .to_owned();
    let rows: Vec<_> = department
        .jobs
        .iter()
        .map(|job| {
            let unavailable = st.unavailable.contains(&job.title);
            let overflow = job.title == st.overflow;
            job_row(
                &job.title,
                &job.description,
                color,
                unavailable,
                overflow,
                data,
            )
        })
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Column, margin: UiRect::bottom(px(8)) }
        Children [
            Node { padding: UiRect::axes(px(4), px(2)) }
            Propagate::<Bold>(Bold(true))
            Children [ Text(name) ]
            --
            Node { flex_direction: FlexDirection::Column }
            Children [ {rows} ]
        ]
    }
}

fn job_row(
    title: &str,
    description: &str,
    color: [u8; 3],
    unavailable: bool,
    overflow: bool,
    data: &JobPrefsData,
) -> impl Scene + use<> {
    let name = job_ru(title).unwrap_or(title).to_owned();
    let level = data.level(title);
    let options: Vec<(&'static str, Option<JobPriority>)> = if overflow {
        vec![("Откл.", None), ("Вкл.", Some(JobPriority::High))]
    } else {
        vec![
            ("Откл.", None),
            ("Низк.", Some(JobPriority::Low)),
            ("Сред.", Some(JobPriority::Medium)),
            ("Выс.", Some(JobPriority::High)),
        ]
    };
    let unavailable_text =
        unavailable.then(|| EntityScene(bsn! { Node Children [ Text("Недоступно") ] }));
    let buttons: Vec<_> = if unavailable {
        Vec::new()
    } else {
        options
            .into_iter()
            .map(|(text, option)| {
                let selected = if overflow {
                    level.is_some() == option.is_some()
                } else {
                    level == option
                };
                let params = vec![
                    ("job".to_owned(), title.to_owned()),
                    ("level".to_owned(), JobPriority::param(option)),
                ];
                let name = format!("{title}: {text}");
                bsn! {
                    @Button { @content: text, @compact: true }
                    Button { selected: selected }
                    Act({SET_JOB_PREFERENCE.to_owned()})
                    ActParams(params)
                    Name(name)
                }
            })
            .collect()
    };
    let tooltip = description.to_owned();
    let bg = Color::srgb_u8(color[0], color[1], color[2]);
    let fg = darken(color, 90.0);
    let fg = Color::srgb_u8(fg[0], fg[1], fg[2]);
    let title = title.to_owned();
    bsn! {
        Node {
            min_height: px(ROW_PX),
            padding: UiRect::horizontal(px(4)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
        }
        BackgroundColor(bg)
        Name(title)
        Children [
            Node { flex_grow: 1.0, min_width: px(0) }
            Propagate::<TextColor>(TextColor(fg))
            Tooltip { content: tooltip, position: Placement::Bottom }
            Children [ Text(name) ]
            --
            {unavailable_text}
            --
            Node { flex_direction: FlexDirection::Row }
            Children [ {buttons} ]
        ]
    }
}

/// One `set_appearance` button.
fn appearance_button(
    text: &str,
    key: &str,
    value: String,
    selected: bool,
) -> EntityScene<impl Scene + use<>> {
    let params = vec![
        ("key".to_owned(), key.to_owned()),
        ("value".to_owned(), value),
    ];
    let name = format!("appearance: {key} {text}");
    let text = text.to_owned();
    EntityScene(bsn! {
        @Button { @content: text, @compact: true }
        Button { selected: selected }
        Act({SET_APPEARANCE.to_owned()})
        ActParams(params)
        Name(name)
    })
}

/// A label, ◀, a color swatch (if any) with the current choice, ▶: the arrows step through `len`
/// choices (wrapping); `value` turns a choice into the act's value.
fn appearance_stepper(
    label: &str,
    key: &str,
    (current, len): (usize, usize),
    value: &dyn Fn(usize) -> String,
    text: String,
    swatch: Option<[u8; 3]>,
) -> EntityScene<impl Scene + use<>> {
    let prev = appearance_button("◀", key, value((current + len - 1) % len), false);
    let next = appearance_button("▶", key, value((current + 1) % len), false);
    let swatch = swatch.map(|[r, g, b]| {
        EntityScene(bsn! {
            Node { width: px(14), height: px(14), margin: UiRect::horizontal(px(4)) }
            BackgroundColor(Color::srgb_u8(r, g, b))
        })
    });
    let label = format!("{label}:");
    // Names get a fixed width so the arrows do not jump; a bare swatch needs none.
    let text_width = if text.is_empty() { 0.0 } else { 96.0 };
    EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(4) }
        Children [
            Node Children [ Text(label) ]
            --
            {prev}
            --
            {swatch}
            --
            Node { min_width: px(text_width) } Children [ Text(text) ]
            --
            {next}
        ]
    })
}

/// The appearance section: species, body type, skin tone, mutant color, hair style and color,
/// eye color, randomize.
fn appearance_rows(st: &JobPrefsStatic, look: &AppearanceData) -> impl SceneList + use<> {
    let female = look.female;
    let body = EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, column_gap: px(4), align_items: AlignItems::Center }
        Children [
            Node Children [ Text("Тело:") ]
            --
            {appearance_button("Мужское", APPEARANCE_FEMALE, "0".to_owned(), !female)}
            --
            {appearance_button("Женское", APPEARANCE_FEMALE, "1".to_owned(), female)}
        ]
    });
    let index = |i: usize| i.to_string();
    let species = appearance_stepper(
        "Вид",
        APPEARANCE_SPECIES,
        (look.species as usize, st.species.len().max(1)),
        &index,
        st.species
            .get(look.species as usize)
            .cloned()
            .unwrap_or_default(),
        None,
    );
    // shortcut: shown for every species; only those with `TRAIT_MUTANT_COLORS` use it.
    let mutant_color = appearance_stepper(
        "Цвет тела",
        APPEARANCE_MUTANT_COLOR,
        (look.mutant_color as usize, st.mutant_colors.len().max(1)),
        &index,
        String::new(),
        st.mutant_colors.get(look.mutant_color as usize).copied(),
    );
    let skin = st.skin_tones.get(look.skin_tone as usize);
    let skin = appearance_stepper(
        "Тон кожи",
        APPEARANCE_SKIN_TONE,
        (look.skin_tone as usize, st.skin_tones.len().max(1)),
        &index,
        skin.map_or_else(String::new, |(name, _)| name.clone()),
        skin.map(|(_, rgb)| *rgb),
    );
    // Choice 0 is bald, then the styles.
    let styles = st.hair_styles.len() + 1;
    let style = look.hair_style.map_or(0, |h| h as usize + 1);
    let hair = appearance_stepper(
        "Причёска",
        APPEARANCE_HAIR_STYLE,
        (style, styles),
        &|i: usize| {
            if i == 0 {
                String::new()
            } else {
                (i - 1).to_string()
            }
        },
        look.hair_style
            .and_then(|h| st.hair_styles.get(h as usize).cloned())
            .unwrap_or_else(|| "Лысый".to_owned()),
        None,
    );
    let hair_color = appearance_stepper(
        "Цвет волос",
        APPEARANCE_HAIR_COLOR,
        (look.hair_color as usize, st.hair_colors.len().max(1)),
        &index,
        String::new(),
        st.hair_colors.get(look.hair_color as usize).copied(),
    );
    let eye_color = appearance_stepper(
        "Цвет глаз",
        APPEARANCE_EYE_COLOR,
        (look.eye_color as usize, st.eye_colors.len().max(1)),
        &index,
        String::new(),
        st.eye_colors.get(look.eye_color as usize).copied(),
    );
    let random = EntityScene(bsn! {
        @Button { @content: "Случайная внешность", @compact: true }
        Act({RANDOMIZE_APPEARANCE.to_owned()})
        Name("appearance: random")
    });
    (
        species,
        body,
        skin,
        mutant_color,
        hair,
        hair_color,
        eye_color,
        random,
    )
}
