//! Client half of the Escape menu and the Who window ([`crate::escape_menu`]): Esc opens the menu
//! (the host window's [`OPEN`] act) and closes it again, unless a text field has the keyboard or
//! another window took it (tgui modals cancel on Esc themselves). The menu sets the master volume
//! and the volume of each kind of sound (tg's volume preferences, [`SoundVolumes`]).

use bevy::audio::{
    AudioSink, AudioSinkPlayback, GlobalVolume, PlaybackSettings, SpatialAudioSink, Volume,
};
use bevy::input_focus::InputFocus;
use bevy::scene::EntityScene;
use bevy::text::EditableText;
use ss13_net::client::SendIntent;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClientIntent, SoundCategory};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, ClientWindow, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use crate::escape_menu::*;
use crate::sound_client::SoundVolumes;

pub struct EscapeMenuClientPlugin;

impl Plugin for EscapeMenuClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<EscapeButtonUi>()
            .register_ui_view::<EscapeMenuUi>()
            .register_ui_view::<WhoUi>()
            .init_resource::<MenuHostWindow>()
            .init_resource::<SoundVolumes>()
            .add_systems(
                Update,
                (receive, escape_key, sync_volume, sync_kind_volumes).chain(),
            );
    }
}

/// The server's host window.
#[derive(Resource, Default, Debug)]
pub struct MenuHostWindow(pub Option<u32>);

fn receive(mut received: MessageReader<UiMessageReceived>, mut host: ResMut<MenuHostWindow>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window, interface, ..
            } if interface == EscapeButtonUi::NAME => host.0 = Some(*window),
            UiServerMessage::Close { window } if host.0 == Some(*window) => host.0 = None,
            _ => {}
        }
    }
}

/// tg's `Escape` macro: the menu opens, or closes when it is open.
fn escape_key(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    text_fields: Query<(), With<EditableText>>,
    windows: Query<(Entity, &ClientWindow)>,
    host: Res<MenuHostWindow>,
    mut intents: MessageWriter<SendIntent>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    let focused = focus.and_then(|f| f.get());
    if focused.is_some_and(|e| text_fields.contains(e)) {
        return;
    }
    if let Some((menu, _)) = windows
        .iter()
        .find(|(_, w)| w.interface == EscapeMenuUi::NAME)
    {
        // As its close button does (`ss13_ui_kit::window`): the server hears `close`.
        commands.trigger(WindowClose { entity: menu });
        commands.entity(menu).despawn();
        return;
    }
    if focused.is_some() {
        return;
    }
    if let Some(window) = host.0 {
        intents.write(SendIntent(ClientIntent::UiAct {
            window,
            action: OPEN.into(),
            params: Vec::new(),
        }));
    }
}

/// The menu's [`UiLocalState`] key for the master volume, in percent.
const LOCAL_VOLUME: &str = "volume";
const VOLUME_STEP: u32 = 10;

/// The master volume, in percent.
fn volume_percent(global: &GlobalVolume) -> u32 {
    (global.volume.to_linear() * 100.0)
        .round()
        .clamp(0.0, 100.0) as u32
}

/// The menu's volume buttons set [`LOCAL_VOLUME`]; this applies it to [`GlobalVolume`] and to
/// the sounds already playing (bevy reads `GlobalVolume` only when a sound starts). A freshly
/// opened menu starts from the current volume.
// shortcut: not saved; the next launch starts from `SS13_VOLUME` again. Persist it with the
// first client settings file.
fn sync_volume(
    mut menus: Query<(&mut ClientWindow, &mut UiLocalState)>,
    mut global: ResMut<GlobalVolume>,
    mut sinks: Query<(&mut AudioSink, &PlaybackSettings)>,
    mut spatial: Query<(&mut SpatialAudioSink, &PlaybackSettings)>,
) {
    for (mut window, mut local) in &mut menus {
        if window.interface != EscapeMenuUi::NAME {
            continue;
        }
        let Some(wanted) = local
            .value(LOCAL_VOLUME)
            .and_then(|v| v.parse::<u32>().ok())
        else {
            local.set(LOCAL_VOLUME, &volume_percent(&global).to_string());
            window.mark_dirty();
            continue;
        };
        if wanted == volume_percent(&global) {
            continue;
        }
        let master = wanted.min(100) as f32 / 100.0;
        global.volume = Volume::Linear(master);
        for (mut sink, settings) in &mut sinks {
            sink.set_volume(Volume::Linear(settings.volume.to_linear() * master));
        }
        for (mut sink, settings) in &mut spatial {
            sink.set_volume(Volume::Linear(settings.volume.to_linear() * master));
        }
    }
}

/// The menu's [`UiLocalState`] key for the volume of a kind of sound (tg's volume preferences).
fn kind_key(category: SoundCategory) -> String {
    format!("{LOCAL_VOLUME}_{}", category.key())
}

/// The menu's volume rows of each kind set [`SoundVolumes`] (`crate::sound_client` re-levels
/// what plays); a freshly opened menu starts from them.
fn sync_kind_volumes(
    mut menus: Query<(&mut ClientWindow, &mut UiLocalState)>,
    mut volumes: ResMut<SoundVolumes>,
) {
    for (mut window, mut local) in &mut menus {
        if window.interface != EscapeMenuUi::NAME {
            continue;
        }
        for category in SoundCategory::PREFERENCES {
            let key = kind_key(category);
            let Some(wanted) = local.value(&key).and_then(|v| v.parse::<u8>().ok()) else {
                local.set(&key, &volumes.get(category).to_string());
                window.mark_dirty();
                continue;
            };
            if wanted.min(100) != volumes.get(category) {
                volumes.set(category, wanted);
            }
        }
    }
}

/// A `−`/`+` button that sets the menu's volume to `value` percent.
fn volume_button(icon: &'static str, tooltip: &'static str, value: u32) -> impl Scene + use<> {
    key_volume_button(LOCAL_VOLUME.to_owned(), icon, tooltip, value)
}

/// A `−`/`+` button that sets the volume under `key` to `value` percent.
fn key_volume_button(
    key: String,
    icon: &'static str,
    tooltip: &'static str,
    value: u32,
) -> impl Scene + use<> {
    let set = SetLocal {
        key,
        value: value.to_string(),
        rerender: true,
    };
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @tooltip: tooltip, @compact: true }
        set
    }
}

fn line(text: String) -> impl Scene + use<> {
    bsn! { Node Children [ Text(text) ] }
}

/// One kind's volume: its preference name, `−`, the percent, `+`.
fn kind_volume_row(category: SoundCategory, local: &UiLocalState) -> impl Scene + use<> {
    let key = kind_key(category);
    let value = local
        .get(&key)
        .parse::<u32>()
        .unwrap_or_else(|_| u32::from(category.default_percent()))
        .min(100);
    let quieter = EntityScene(key_volume_button(
        key.clone(),
        "volume-low",
        "Тише",
        value.saturating_sub(VOLUME_STEP),
    ));
    let louder = EntityScene(key_volume_button(
        key,
        "volume-high",
        "Громче",
        (value + VOLUME_STEP).min(100),
    ));
    let shown = EntityScene(line(format!("{value}%")));
    let label = category.label().to_owned();
    bsn! {
        @LabeledListItem { @label: label }
        Children [
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6) }
            Children [ {quieter} -- {shown} -- {louder} ]
        ]
    }
}

impl ClientUi for EscapeMenuUi {
    const WIDTH: f32 = 420.0;
    const HEIGHT: f32 = 640.0;

    fn content(_: &(), data: &EscapeMenuData) -> impl Scene {
        menu(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &EscapeMenuData, local: &UiLocalState) -> impl Scene {
        menu(data, local)
    }
}

fn menu(data: &EscapeMenuData, local: &UiLocalState) -> impl Scene + use<> {
    let station = data.station_name.clone();
    let shift = data
        .shift_time
        .clone()
        .unwrap_or_else(|| "Pre-Game".to_owned());
    let players = data.players.to_string();
    let shift = EntityScene(line(shift));
    let players = EntityScene(line(players));
    let help = EntityScene(line("Админ-помощь: F1".to_owned()));
    let volume = local
        .get(LOCAL_VOLUME)
        .parse::<u32>()
        .unwrap_or(100)
        .min(100);
    let quieter = EntityScene(volume_button(
        "volume-low",
        "Тише",
        volume.saturating_sub(VOLUME_STEP),
    ));
    let louder = EntityScene(volume_button(
        "volume-high",
        "Громче",
        (volume + VOLUME_STEP).min(100),
    ));
    let volume = EntityScene(line(format!("{volume}%")));
    let kinds: Vec<_> = SoundCategory::PREFERENCES
        .into_iter()
        .map(|category| kind_volume_row(category, local))
        .collect();
    let who = WHO.to_owned();
    bsn! {
        @WindowContent
        Children [
            @Section { @title: station }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Время смены" }
                    Children [ {shift} ]
                    --
                    @LabeledListItem { @label: "Игроков онлайн" }
                    Children [ {players} ]
                    --
                    @LabeledListItem { @label: "Громкость" }
                    Children [
                        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6) }
                        Children [ {quieter} -- {volume} -- {louder} ]
                    ]
                ]
            ]
            --
            @Section { @title: "Звук" }
            Children [
                @LabeledList
                Children [ {kinds} ]
            ]
            --
            @Section
            Children [
                Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
                Children [
                    @ss13_ui_kit::Button { @icon: "users", @content: "Кто онлайн", @fluid: true }
                    Act(who)
                    --
                    {help}
                ]
            ]
        ]
    }
}

impl ClientUi for WhoUi {
    const WIDTH: f32 = 550.0;
    const HEIGHT: f32 = 650.0;

    fn content(_: &(), data: &WhoData) -> impl Scene {
        let title = format!("Онлайн: {}", data.clients.len());
        let rows: Vec<_> = data.clients.iter().map(|key| line(key.clone())).collect();
        let user = EntityScene(line(data.user.clone()));
        let update = UPDATE.to_owned();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: title, @fill: true, @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                    Children [ {rows} ]
                ]
                --
                @Section
                Children [
                    Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween }
                    Children [
                        {user}
                        --
                        @ss13_ui_kit::Button { @icon: "arrows-rotate", @tooltip: "Обновить" }
                        Act(update)
                    ]
                ]
            ]
        }
    }
}
