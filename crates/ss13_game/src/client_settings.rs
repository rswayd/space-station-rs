//! The client's own settings, kept between launches in a plain `key = value` file: the master
//! volume and the volume of each kind of sound (tg's volume preferences, which tg keeps in the
//! player's preferences on the server; the port has none, and they only shape what this
//! client plays).
//!
//! The file is `SS13_CLIENT_SETTINGS`, else `$XDG_CONFIG_HOME/ss13-rs/client.cfg`, else
//! `~/.config/ss13-rs/client.cfg` (`%APPDATA%\ss13-rs\client.cfg` on Windows). A change is written
//! [`SAVE_DELAY_SECS`] after the last one, so dragging a slider does not write every frame.
// shortcut: the browser client has no file; it starts from the defaults each time (use
// `localStorage` when the web build needs it).

use std::collections::BTreeMap;
use std::path::PathBuf;

use bevy::prelude::*;

/// Seconds after the last change before the file is written.
pub const SAVE_DELAY_SECS: f64 = 1.0;

/// The settings read at startup and written back on change.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct ClientSettings {
    values: BTreeMap<String, String>,
    path: Option<PathBuf>,
    /// When the pending change is written (real seconds).
    save_at: Option<f64>,
}

impl ClientSettings {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// The value under `key` parsed, if it is there and parses.
    pub fn parsed<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.get(key).and_then(|v| v.parse().ok())
    }

    /// Sets `key`; a new value schedules a save at `now + SAVE_DELAY_SECS`.
    pub fn set(&mut self, key: &str, value: impl ToString, now: f64) {
        let value = value.to_string();
        if self.get(key) == Some(value.as_str()) {
            return;
        }
        self.values.insert(key.to_owned(), value);
        self.save_at = Some(now + SAVE_DELAY_SECS);
    }

    /// Reads the settings file (empty when there is none or it cannot be read).
    pub fn load() -> Self {
        let path = settings_path();
        let values = path
            .as_ref()
            .and_then(|p| read(p))
            .map(|text| parse(&text))
            .unwrap_or_default();
        Self {
            values,
            path,
            save_at: None,
        }
    }
}

/// `key = value` lines; blank lines and `#` comments are skipped.
pub fn parse(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (key, value) = l.split_once('=')?;
            Some((key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// The file text for `values`.
pub fn render(values: &BTreeMap<String, String>) -> String {
    let mut out = String::from("# ss13-rs client settings\n");
    for (key, value) in values {
        out.push_str(&format!("{key} = {value}\n"));
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn settings_path() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(path) = env("SS13_CLIENT_SETTINGS") {
        return Some(path);
    }
    let dir = if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }?;
    Some(dir.join("ss13-rs").join("client.cfg"))
}

#[cfg(target_arch = "wasm32")]
fn settings_path() -> Option<PathBuf> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn read(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[cfg(target_arch = "wasm32")]
fn read(_: &std::path::Path) -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn write(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Written beside and renamed, so a crash mid-write never leaves half a file.
    let tmp = path.with_extension("cfg.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(target_arch = "wasm32")]
fn write(_: &std::path::Path, _: &str) -> std::io::Result<()> {
    Ok(())
}

/// Writes the settings once their change has settled.
fn save(mut settings: ResMut<ClientSettings>, time: Res<Time<Real>>) {
    let Some(at) = settings.save_at else {
        return;
    };
    if time.elapsed_secs_f64() < at {
        return;
    }
    settings.save_at = None;
    let Some(path) = settings.path.clone() else {
        return;
    };
    if let Err(e) = write(&path, &render(&settings.values)) {
        warn!("client settings: cannot write {}: {e}", path.display());
    }
}

pub struct ClientSettingsPlugin;

impl Plugin for ClientSettingsPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<ClientSettings>() {
            app.insert_resource(ClientSettings::load());
        }
        app.add_systems(Last, save);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_round_trip_and_a_change_schedules_a_save() {
        let text = "# comment\nvolume.master = 40\n\n volume.ambience=70 \nbroken line\n";
        let values = parse(text);
        assert_eq!(values.get("volume.master").map(String::as_str), Some("40"));
        assert_eq!(
            values.get("volume.ambience").map(String::as_str),
            Some("70")
        );
        assert_eq!(values.len(), 2);
        assert_eq!(parse(&render(&values)), values);

        let mut s = ClientSettings::default();
        s.set("volume.master", 40, 5.0);
        assert_eq!(s.save_at, Some(5.0 + SAVE_DELAY_SECS));
        s.save_at = None;
        s.set("volume.master", 40, 9.0);
        assert_eq!(s.save_at, None, "the same value changes nothing");
        assert_eq!(s.parsed::<u8>("volume.master"), Some(40));
    }
}
