//! BandaStation `/datum/element/speech_filter` (`modular_bandastation/speech_filter`): a living
//! speaker's forbidden words ("brainrot", `config/bandastation/brainrot_filter.json`) become
//! "цветочек"; the speaker is told off and drools, the admins are told. Off unless the server
//! enables it (`ENABLE_SPEECH_FILTER`); ckeys of `SPEECH_FILTER_BYPASS` are never filtered. The
//! game loads [`SpeechFilter`]; [`crate::server`] applies it where tg sends `COMSIG_MOB_SAY`.

use bevy::prelude::*;

/// What a forbidden word becomes.
pub const REPLACEMENT: &str = "цветочек";

/// `brainrot_notifications`: one is shown to the speaker.
pub const NOTIFICATIONS: [&str; 4] = [
    "Почему у меня такой скудный словарный запас? Стоит сходить в библиотеку и прочесть книгу...",
    "Что, черт побери, я несу?",
    "Я в своём уме? Надо следить за языком.",
    "Неужели я не могу подобрать нужных слов? Позор мне...",
];

/// The filter of this server. Absent, or without a pattern: nothing is filtered.
#[derive(Resource, Clone, Debug, Default)]
pub struct SpeechFilter {
    /// `brainrot_filter` entries joined with `|`, case-insensitive.
    pattern: Option<regex::Regex>,
    /// `SPEECH_FILTER_BYPASS`, as ckeys.
    bypass: Vec<String>,
}

/// A speaker's message was filtered: the game makes it drool and tells the admins.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct SpeechFiltered {
    pub client: Entity,
    pub speaker: Entity,
    /// What the player typed.
    pub original: String,
}

/// tg `ckey()`, as `ss13_admin::rights::ckey` (the chat is below the admin crate).
fn ckey(key: &str) -> String {
    key.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '@')
        .collect()
}

impl SpeechFilter {
    /// The filter of the `brainrot_filter` regexes (empty entries skipped: BYOND's empty pattern
    /// is false and filters nothing) and the bypassing keys.
    pub fn new(filters: &[String], bypass: &[String]) -> Result<Self, regex::Error> {
        let mut unique: Vec<&str> = Vec::new();
        for f in filters.iter().map(String::as_str).filter(|f| !f.is_empty()) {
            if !unique.contains(&f) {
                unique.push(f);
            }
        }
        let pattern = if unique.is_empty() {
            None
        } else {
            Some(regex::Regex::new(&format!("(?i){}", unique.join("|")))?)
        };
        Ok(Self {
            pattern,
            bypass: bypass.iter().map(|k| ckey(k)).collect(),
        })
    }

    /// `filter_speech`: the message with its forbidden words replaced, `None` when it stays as
    /// it is (nothing matched, an emote, a bypassing key).
    pub fn filter(&self, key: &str, message: &str) -> Option<String> {
        let pattern = self.pattern.as_ref()?;
        if message.is_empty() || message.starts_with('*') || self.bypass.contains(&ckey(key)) {
            return None;
        }
        let filtered = pattern.replace_all(message, REPLACEMENT);
        (filtered != message).then(|| filtered.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_replaces_forbidden_words() {
        let words = ["скибиди", "", "rizz\\w*", "скибиди"].map(String::from);
        let filter = SpeechFilter::new(&words, &["Some Admin".to_owned()]).unwrap();
        assert_eq!(
            filter.filter("player", "Ну ты СКИБИДИ, rizzler"),
            Some("Ну ты цветочек, цветочек".to_owned())
        );
        assert_eq!(filter.filter("player", "привет"), None);
        assert_eq!(filter.filter("player", "*скибиди"), None);
        assert_eq!(filter.filter("someadmin", "скибиди"), None);
        let empty = SpeechFilter::new(&[String::new()], &[]).unwrap();
        assert_eq!(empty.filter("player", "скибиди"), None);
    }
}
