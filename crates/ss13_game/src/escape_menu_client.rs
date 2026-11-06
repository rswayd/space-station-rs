//! Client half of the Escape menu and the Who window ([`crate::escape_menu`]): Esc opens the menu
//! (the host window's [`OPEN`] act) and closes it again, unless a text field has the keyboard or
//! another window took it (tgui modals cancel on Esc themselves).

use bevy::input_focus::InputFocus;
use bevy::scene::EntityScene;
use bevy::text::EditableText;
use ss13_net::client::SendIntent;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, ClientWindow};
use ss13_ui_kit::prelude::*;

use crate::escape_menu::*;

pub struct EscapeMenuClientPlugin;

impl Plugin for EscapeMenuClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<EscapeButtonUi>()
            .register_ui_view::<EscapeMenuUi>()
            .register_ui_view::<WhoUi>()
            .init_resource::<MenuHostWindow>()
            .add_systems(Update, (receive, escape_key).chain());
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

fn line(text: String) -> impl Scene + use<> {
    bsn! { Node Children [ Text(text) ] }
}

impl ClientUi for EscapeMenuUi {
    const WIDTH: f32 = 320.0;
    const HEIGHT: f32 = 230.0;

    fn content(_: &(), data: &EscapeMenuData) -> impl Scene {
        let station = data.station_name.clone();
        let shift = data
            .shift_time
            .clone()
            .unwrap_or_else(|| "Pre-Game".to_owned());
        let players = data.players.to_string();
        let shift = EntityScene(line(shift));
        let players = EntityScene(line(players));
        let help = EntityScene(line("Админ-помощь: F1".to_owned()));
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
                    ]
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
