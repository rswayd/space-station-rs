//! tg's client keybinds with no game state behind them (`code/datums/keybinding/client.dm`): F11
//! fullscreen, F12 HUD modes, Home closes every window.
// shortcut: no screenshot binds (BYOND's own; unbound in BandaStation apart from Shift+F2).

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use ss13_chat::client::ChatLog;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::client::ClientWindow;
use ss13_ui_kit::WindowClose;

use crate::statpanel::StatPanelUi;

/// HUD slots and buttons (tg `static_inventory`, `toggleable_inventory`, `hotkeybuttons`).
#[derive(Component, Default)]
pub struct HudInventory;

/// HUD readouts (tg `infodisplay`): kept in the reduced mode.
#[derive(Component, Default)]
pub struct HudInfo;

/// tg `hud_version`, `HUD_STYLE_*`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HudMode {
    #[default]
    Standard,
    Reduced,
    None,
}

impl HudMode {
    fn next(self) -> Self {
        match self {
            HudMode::Standard => HudMode::Reduced,
            HudMode::Reduced => HudMode::None,
            HudMode::None => HudMode::Standard,
        }
    }
}

pub struct ClientKeysPlugin;

impl Plugin for ClientKeysPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudMode>().add_systems(
            Update,
            (
                handle_keys,
                apply_hud_mode.run_if(resource_changed::<HudMode>),
            )
                .chain(),
        );
    }
}

fn handle_keys(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    text_fields: Query<(), With<EditableText>>,
    mut windows: Query<&mut bevy::window::Window, With<PrimaryWindow>>,
    tgui: Query<(Entity, &ClientWindow)>,
    mut mode: ResMut<HudMode>,
    mut log: ResMut<ChatLog>,
    time: Res<Time<Real>>,
    mut commands: Commands,
) {
    if focus
        .and_then(|f| f.get())
        .is_some_and(|e| text_fields.contains(e))
    {
        return;
    }
    if keys.just_pressed(KeyCode::F11)
        && let Ok(mut window) = windows.single_mut()
    {
        window.mode = match window.mode {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            _ => WindowMode::Windowed,
        };
    }
    if keys.just_pressed(KeyCode::F12) {
        *mode = mode.next();
        log.push(
            ChatMessage::simple(
                ChatChannel::System,
                Span::Info,
                "Режим HUD переключён. Нажмите F12, чтобы сменить его.",
            ),
            time.elapsed_secs_f64(),
        );
    }
    // tg `SStgui.close_user_uis`: every window but the stat panel, which is the panel of tg's
    // client, not a tgui window.
    if keys.just_pressed(KeyCode::Home) {
        for (entity, window) in &tgui {
            if window.interface != StatPanelUi::NAME {
                commands.trigger(WindowClose { entity });
                commands.entity(entity).despawn();
            }
        }
    }
}

fn apply_hud_mode(
    mode: Res<HudMode>,
    mut inventory: Query<&mut Visibility, (With<HudInventory>, Without<HudInfo>)>,
    mut info: Query<&mut Visibility, (With<HudInfo>, Without<HudInventory>)>,
) {
    let shown = |on: bool| {
        if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    };
    for mut v in &mut inventory {
        *v = shown(*mode == HudMode::Standard);
    }
    for mut v in &mut info {
        *v = shown(*mode != HudMode::None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_modes_cycle() {
        assert_eq!(HudMode::Standard.next(), HudMode::Reduced);
        assert_eq!(HudMode::Reduced.next(), HudMode::None);
        assert_eq!(HudMode::None.next(), HudMode::Standard);
    }
}
