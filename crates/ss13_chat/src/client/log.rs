//! The message store behind the panel (tgui-panel `ChatRenderer` minus the DOM): bounded,
//! combines repeats, assigns stable ids.

use std::collections::VecDeque;

use bevy::prelude::*;
use ss13_shared::chat::ChatMessage;

/// tgui-panel `MAX_VISIBLE_MESSAGES`.
pub const MAX_VISIBLE_MESSAGES: usize = 2500;
/// tgui-panel `COMBINE_MAX_MESSAGES`: a repeat combines with one of the last 5 messages...
pub const COMBINE_MAX_MESSAGES: usize = 5;
/// ...created at most `COMBINE_MAX_TIME_WINDOW` (5 s) ago.
pub const COMBINE_MAX_TIME_WINDOW_SECS: f64 = 5.0;

#[derive(Clone, Debug, PartialEq)]
pub struct ChatEntry {
    /// Increasing in arrival order, never reused.
    pub id: u64,
    pub message: ChatMessage,
    /// How many identical messages this entry stands for (the `xN` badge).
    pub times: u32,
    /// Arrival time of the first of them, in seconds.
    pub created: f64,
}

#[derive(Resource, Debug)]
pub struct ChatLog {
    entries: VecDeque<ChatEntry>,
    next_id: u64,
    capacity: usize,
}

impl Default for ChatLog {
    fn default() -> Self {
        Self::with_capacity(MAX_VISIBLE_MESSAGES)
    }
}

impl ChatLog {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            next_id: 0,
            capacity: capacity.max(1),
        }
    }

    /// Adds a message received at `now`; returns the id of the entry that shows it.
    ///
    /// Never holds more than the capacity: crossing it evicts the oldest tenth (tgui-panel prunes
    /// once a minute instead); the panel keeps a scrolled-up view in place.
    pub fn push(&mut self, message: ChatMessage, now: f64) -> u64 {
        let from = self.entries.len().saturating_sub(COMBINE_MAX_MESSAGES);
        if let Some(entry) = self
            .entries
            .range_mut(from..)
            .rev()
            .find(|e| e.message == message && now < e.created + COMBINE_MAX_TIME_WINDOW_SECS)
        {
            entry.times += 1;
            return entry.id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push_back(ChatEntry {
            id,
            message,
            times: 1,
            created: now,
        });
        if self.entries.len() > self.capacity {
            // Evicting a tenth at once: every eviction shifts and redraws the visible rows, so
            // one per message would redraw the list on every message once the log is full.
            let keep = self.capacity - self.capacity / 10;
            self.entries.drain(..self.entries.len() - keep.max(1));
        }
        id
    }

    pub fn entries(&self) -> &VecDeque<ChatEntry> {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&ChatEntry> {
        let first = self.entries.front()?.id;
        // Ids are dense from the oldest kept entry on.
        self.entries
            .get(usize::try_from(id.checked_sub(first)?).ok()?)
    }
}

#[cfg(test)]
mod tests {
    use ss13_shared::chat::{ChatChannel, Span};

    use super::*;

    fn msg(text: &str) -> ChatMessage {
        ChatMessage::simple(ChatChannel::Local, Span::Say, text)
    }

    #[test]
    fn repeats_combine_within_five_messages_and_five_seconds() {
        let mut log = ChatLog::default();
        let a = log.push(msg("а"), 0.0);
        assert_eq!(log.push(msg("а"), 1.0), a);
        assert_eq!(log.get(a).unwrap().times, 2);
        // Another channel or style is another message.
        let radio = ChatMessage::simple(ChatChannel::Radio, Span::Radio, "а");
        assert_ne!(log.push(radio, 1.5), a);
        for t in ["б", "в", "г"] {
            log.push(msg(t), 2.0);
        }
        // `а` is still among the last five: combines out of order, as in tgui-panel.
        assert_eq!(log.push(msg("а"), 3.0), a);
        assert_eq!(log.get(a).unwrap().times, 3);
        // The window counts from the first arrival, not the last repeat.
        assert_ne!(log.push(msg("а"), 5.0), a);
        // Six messages back is too far.
        let mut log = ChatLog::default();
        let old = log.push(msg("x"), 0.0);
        for i in 0..COMBINE_MAX_MESSAGES {
            log.push(msg(&i.to_string()), 0.0);
        }
        assert_ne!(log.push(msg("x"), 0.1), old);
        assert_eq!(log.len(), COMBINE_MAX_MESSAGES + 2);
    }

    #[test]
    fn capacity_evicts_a_tenth_at_once() {
        let mut log = ChatLog::with_capacity(100);
        for i in 0..100 {
            log.push(msg(&i.to_string()), f64::from(i) * 10.0);
        }
        assert_eq!(log.len(), 100);
        log.push(msg("new"), 2000.0);
        assert_eq!(log.len(), 90);
        assert_eq!(log.entries().front().unwrap().message.plain_text(), "11");
    }

    #[test]
    fn capacity_evicts_oldest_and_ids_stay_addressable() {
        let mut log = ChatLog::with_capacity(3);
        let ids: Vec<u64> = (0..5)
            .map(|i| log.push(msg(&i.to_string()), f64::from(i) * 10.0))
            .collect();
        // 3 + 1 > 3: down to 3 - 3/10 = 3, one at a time for such a small log.
        assert_eq!(log.len(), 3);
        assert!(log.get(ids[1]).is_none());
        assert_eq!(log.get(ids[2]).unwrap().message.plain_text(), "2");
        assert_eq!(log.get(ids[4]).unwrap().message.plain_text(), "4");
        assert!(log.get(ids[4] + 1).is_none());
        assert_eq!(
            log.entries().iter().map(|e| e.id).collect::<Vec<_>>(),
            &ids[2..]
        );
    }
}
