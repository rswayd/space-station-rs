//! The network panel's model: a bounded packet log with per-kind counters and a one-second rate
//! window. Plain data, no Bevy, so it is tested on its own.

use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Write};

/// One message on the wire as the log keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct PacketRecord {
    /// Seconds since the client started.
    pub time: f64,
    pub outgoing: bool,
    pub kind: String,
    pub size: usize,
    pub summary: String,
}

/// Totals of one message kind since the last clear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KindCounter {
    pub count_in: u64,
    pub bytes_in: u64,
    pub count_out: u64,
    pub bytes_out: u64,
}

/// Which records the panel shows (and exports).
#[derive(Clone, Debug, PartialEq)]
pub struct PacketFilter {
    /// Exact kind, empty = any.
    pub kind: String,
    /// Case-insensitive substring of the kind or the summary, empty = any.
    pub text: String,
    pub incoming: bool,
    pub outgoing: bool,
}

impl Default for PacketFilter {
    fn default() -> Self {
        Self {
            kind: String::new(),
            text: String::new(),
            incoming: true,
            outgoing: true,
        }
    }
}

impl PacketFilter {
    pub fn matches(&self, record: &PacketRecord) -> bool {
        if !(if record.outgoing {
            self.outgoing
        } else {
            self.incoming
        }) {
            return false;
        }
        if !self.kind.is_empty() && record.kind != self.kind {
            return false;
        }
        if self.text.is_empty() {
            return true;
        }
        let needle = self.text.to_lowercase();
        record.kind.to_lowercase().contains(&needle)
            || record.summary.to_lowercase().contains(&needle)
    }
}

pub struct NetLog {
    capacity: usize,
    records: VecDeque<PacketRecord>,
    /// While set, records are not kept; counters and rates still count.
    pub paused: bool,
    counters: BTreeMap<String, KindCounter>,
    /// `(time, outgoing, size)` of the last second, for the rates.
    window: VecDeque<(f64, bool, usize)>,
}

impl NetLog {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            records: VecDeque::new(),
            paused: false,
            counters: BTreeMap::new(),
            window: VecDeque::new(),
        }
    }

    pub fn push(&mut self, record: PacketRecord) {
        let counter = self.counters.entry(record.kind.clone()).or_default();
        if record.outgoing {
            counter.count_out += 1;
            counter.bytes_out += record.size as u64;
        } else {
            counter.count_in += 1;
            counter.bytes_in += record.size as u64;
        }
        self.window
            .push_back((record.time, record.outgoing, record.size));
        while self
            .window
            .front()
            .is_some_and(|&(t, ..)| t <= record.time - 1.0)
        {
            self.window.pop_front();
        }
        if self.paused {
            return;
        }
        if self.records.len() == self.capacity {
            self.records.pop_front();
        }
        self.records.push_back(record);
    }

    /// Forgets the records and the counters.
    pub fn clear(&mut self) {
        self.records.clear();
        self.counters.clear();
        self.window.clear();
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Oldest first.
    pub fn filtered<'a>(
        &'a self,
        filter: &'a PacketFilter,
    ) -> impl DoubleEndedIterator<Item = &'a PacketRecord> + 'a {
        self.records.iter().filter(move |r| filter.matches(r))
    }

    /// Kind → totals, by kind name.
    pub fn counters(&self) -> &BTreeMap<String, KindCounter> {
        &self.counters
    }

    /// Bytes per second (incoming, outgoing) over the second before `now`.
    pub fn rates(&self, now: f64) -> (f64, f64) {
        self.window.iter().filter(|&&(t, ..)| t > now - 1.0).fold(
            (0.0, 0.0),
            |(i, o), &(_, out, size)| {
                if out {
                    (i, o + size as f64)
                } else {
                    (i + size as f64, o)
                }
            },
        )
    }

    /// One line per shown record: `time  in|out  kind  size  summary`, tab-separated.
    pub fn export(&self, filter: &PacketFilter, mut out: impl Write) -> io::Result<usize> {
        let mut lines = 0;
        for r in self.filtered(filter) {
            let dir = if r.outgoing { "out" } else { "in" };
            writeln!(
                out,
                "{:.3}\t{dir}\t{}\t{}\t{}",
                r.time, r.kind, r.size, r.summary
            )?;
            lines += 1;
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(time: f64, outgoing: bool, kind: &str, size: usize, summary: &str) -> PacketRecord {
        PacketRecord {
            time,
            outgoing,
            kind: kind.into(),
            size,
            summary: summary.into(),
        }
    }

    #[test]
    fn ring_keeps_the_newest_records() {
        let mut log = NetLog::new(3);
        for i in 0..5 {
            log.push(rec(f64::from(i), false, "Chat", 10, &i.to_string()));
        }
        let all = PacketFilter::default();
        let kept: Vec<&str> = log.filtered(&all).map(|r| r.summary.as_str()).collect();
        assert_eq!(kept, ["2", "3", "4"]);
        // Counters count every record, also the evicted ones.
        assert_eq!(log.counters()["Chat"].count_in, 5);
        assert_eq!(log.counters()["Chat"].bytes_in, 50);
    }

    #[test]
    fn pause_stops_records_not_counters() {
        let mut log = NetLog::new(10);
        log.push(rec(0.0, true, "Intent::Move", 8, ""));
        log.paused = true;
        log.push(rec(0.1, true, "Intent::Move", 8, ""));
        assert_eq!(log.len(), 1);
        assert_eq!(log.counters()["Intent::Move"].count_out, 2);
        log.clear();
        assert!(log.is_empty() && log.counters().is_empty());
    }

    #[test]
    fn filter_by_kind_text_and_direction() {
        let mut log = NetLog::new(10);
        log.push(rec(0.0, true, "Intent::Say", 20, "Say(\"Привет\")"));
        log.push(rec(
            0.0,
            false,
            "Chat",
            30,
            "ChatMessage { text: \"привет\" }",
        ));
        log.push(rec(0.0, false, "Replication", 900, "tick 5"));
        let shown = |f: &PacketFilter| log.filtered(f).map(|r| r.kind.clone()).collect::<Vec<_>>();

        let text = PacketFilter {
            text: "ПРИВЕТ".into(),
            ..Default::default()
        };
        assert_eq!(shown(&text), ["Intent::Say", "Chat"]);
        let kind = PacketFilter {
            kind: "Replication".into(),
            ..Default::default()
        };
        assert_eq!(shown(&kind), ["Replication"]);
        let incoming = PacketFilter {
            outgoing: false,
            ..Default::default()
        };
        assert_eq!(shown(&incoming), ["Chat", "Replication"]);
        let kind_text = PacketFilter {
            kind: "Chat".into(),
            text: "tick".into(),
            ..Default::default()
        };
        assert!(shown(&kind_text).is_empty());
    }

    #[test]
    fn rates_cover_the_last_second() {
        let mut log = NetLog::new(10);
        log.push(rec(0.0, false, "Replication", 1000, ""));
        log.push(rec(0.5, false, "Motion", 100, ""));
        log.push(rec(0.9, true, "Intent::Move", 10, ""));
        assert_eq!(log.rates(0.95), (1100.0, 10.0));
        assert_eq!(log.rates(1.2), (100.0, 10.0));
        log.push(rec(2.0, false, "Motion", 50, ""));
        assert_eq!(log.rates(2.0), (50.0, 0.0));
    }

    #[test]
    fn export_writes_the_filtered_lines() {
        let mut log = NetLog::new(10);
        log.push(rec(
            1.25,
            true,
            "Intent::Move",
            9,
            "Move { dir: North, seq: 1 }",
        ));
        log.push(rec(1.5, false, "MoveAck", 14, "MoveAck { seq: 1 }"));
        let mut out = Vec::new();
        let filter = PacketFilter {
            incoming: false,
            ..Default::default()
        };
        assert_eq!(log.export(&filter, &mut out).unwrap(), 1);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "1.250\tout\tIntent::Move\t9\tMove { dir: North, seq: 1 }\n"
        );
    }
}
