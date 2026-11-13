//! The chat panel's own files: its settings (tabs, font, highlights, the emote panel) as `chat.*`
//! client settings, its history (tgui-panel saves the last 250 messages and restores them on
//! the next start) and "Save chat log" pages, both beside the settings file
//! ([`crate::client_settings`]). `ss13_chat` makes the data; this plugin reads and writes it.

use bevy::prelude::*;
use ss13_chat::client::history::{self, MESSAGE_SAVE_INTERVAL_SECS};
use ss13_chat::client::{ChatExported, ChatLog, ChatPrefs};
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};

use crate::client_settings::{ClientSettings, ClientSettingsPlugin, read_file, write_file};

/// The history file beside the settings file.
pub const HISTORY_FILE: &str = "chat_history.bin";
/// The directory of saved chat logs beside the settings file.
pub const CHATLOG_DIR: &str = "chatlogs";

pub struct ChatPrefsPlugin;

/// When the history was last written, and what the newest message was then.
#[derive(Resource, Default)]
struct HistorySaved {
    at: f64,
    newest: Option<(u64, u32)>,
}

impl Plugin for ChatPrefsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ClientSettingsPlugin>() {
            app.add_plugins(ClientSettingsPlugin);
        }
        let settings = app.world().resource::<ClientSettings>();
        let prefs = ChatPrefs::from_settings(|key| settings.get(key).map(str::to_owned));
        app.insert_resource(prefs)
            .init_resource::<HistorySaved>()
            .add_message::<ChatExported>()
            .add_systems(Startup, restore_history)
            .add_systems(Update, (save_prefs, save_chat_log))
            .add_systems(Last, save_history);
    }
}

fn save_prefs(prefs: Res<ChatPrefs>, mut settings: ResMut<ClientSettings>, time: Res<Time<Real>>) {
    if !prefs.is_changed() {
        return;
    }
    let now = time.elapsed_secs_f64();
    for (key, value) in prefs.to_settings() {
        settings.set(key, value, now);
    }
}

/// `loadChatFromStorage`: the last session's messages, before anything this one receives.
fn restore_history(
    settings: Res<ClientSettings>,
    log: Option<ResMut<ChatLog>>,
    mut saved: ResMut<HistorySaved>,
) {
    let Some(mut log) = log else {
        return;
    };
    let Some(bytes) = settings
        .file_beside(HISTORY_FILE)
        .and_then(|path| read_file(&path))
    else {
        return;
    };
    let messages = history::decode(&bytes);
    if messages.is_empty() {
        return;
    }
    log.restore(messages);
    saved.newest = log.entries().back().map(|e| (e.id, e.times));
}

/// `saveChatToStorage` every 30 s, and once more when the client closes.
fn save_history(
    log: Option<Res<ChatLog>>,
    settings: Res<ClientSettings>,
    time: Res<Time<Real>>,
    mut exit: MessageReader<AppExit>,
    mut saved: ResMut<HistorySaved>,
) {
    let Some(log) = log else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let exiting = exit.read().count() > 0;
    if !exiting && now < saved.at + MESSAGE_SAVE_INTERVAL_SECS {
        return;
    }
    saved.at = now;
    let newest = log.entries().back().map(|e| (e.id, e.times));
    if newest == saved.newest {
        return;
    }
    saved.newest = newest;
    let Some(path) = settings.file_beside(HISTORY_FILE) else {
        return;
    };
    if let Err(e) = write_file(&path, &history::encode(&log)) {
        warn!("chat history: cannot write {}: {e}", path.display());
    }
}

/// "Save chat log": the page into the chat log directory, its path into the chat.
// shortcut: BYOND asks where to save (`Byond.saveBlob`); the native client has no file dialog,
// so logs go to a fixed directory beside the settings (the browser client saves nothing yet).
fn save_chat_log(
    mut exported: MessageReader<ChatExported>,
    settings: Res<ClientSettings>,
    log: Option<ResMut<ChatLog>>,
    time: Res<Time<Real>>,
) {
    let Some(mut log) = log else {
        exported.clear();
        return;
    };
    for page in exported.read() {
        let Some(path) = settings
            .file_beside(CHATLOG_DIR)
            .map(|dir| dir.join(&page.file_name))
        else {
            continue;
        };
        let (span, text) = match write_file(&path, page.html.as_bytes()) {
            Ok(()) => (
                Span::Notice,
                format!("Лог чата сохранён: {}", path.display()),
            ),
            Err(e) => (Span::Warning, format!("Не удалось сохранить лог чата: {e}")),
        };
        log.push(
            ChatMessage::simple(ChatChannel::System, span, text),
            time.elapsed_secs_f64(),
        );
    }
}
