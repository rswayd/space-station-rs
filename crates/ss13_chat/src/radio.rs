//! tg radio channels and frequencies (`code/__DEFINES/radio.dm`, `GLOB.default_radio_channels`,
//! `GLOB.freqtospan`, the telecomms servers' `frequency_infos`) and the radio chat line
//! (`compose_message` with a `radio_freq`).

use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

pub const FREQ_UPLINK: u16 = 1211;
pub const FREQ_SYNDICATE: u16 = 1213;
pub const FREQ_CTF_RED: u16 = 1215;
pub const FREQ_CTF_BLUE: u16 = 1217;
pub const FREQ_CTF_GREEN: u16 = 1219;
pub const FREQ_CTF_YELLOW: u16 = 1221;
pub const FREQ_CENTCOM: u16 = 1337;
pub const FREQ_SUPPLY: u16 = 1347;
pub const FREQ_SERVICE: u16 = 1349;
pub const FREQ_SCIENCE: u16 = 1351;
pub const FREQ_COMMAND: u16 = 1353;
pub const FREQ_MEDICAL: u16 = 1355;
pub const FREQ_ENGINEERING: u16 = 1357;
pub const FREQ_SECURITY: u16 = 1359;
/// BandaStation jobs module.
pub const FREQ_JUSTICE: u16 = 1361;
pub const FREQ_ENTERTAINMENT: u16 = 1415;
pub const FREQ_INTERROGATION: u16 = 1423;
pub const FREQ_STATUS_DISPLAYS: u16 = 1435;
pub const FREQ_AI_PRIVATE: u16 = 1447;
pub const FREQ_COMMON: u16 = 1459;
pub const FREQ_CONFESSIONAL: u16 = 1481;

pub const MIN_FREE_FREQ: u16 = 1201;
pub const MAX_FREE_FREQ: u16 = 1599;
pub const MIN_FREQ: u16 = 1441;
pub const MAX_FREQ: u16 = 1489;

/// `RADIO_SPECIAL_SYNDIE`, `RADIO_SPECIAL_CENTCOM`, `RADIO_SPECIAL_BINARY`.
pub const RADIO_SPECIAL_SYNDIE: u8 = 1 << 0;
pub const RADIO_SPECIAL_CENTCOM: u8 = 1 << 1;
pub const RADIO_SPECIAL_BINARY: u8 = 1 << 2;

pub const RADIO_CHANNEL_COMMON: &str = "Общий";
pub const RADIO_CHANNEL_SECURITY: &str = "Безопасность";
pub const RADIO_CHANNEL_ENGINEERING: &str = "Инженерия";
pub const RADIO_CHANNEL_COMMAND: &str = "Командование";
pub const RADIO_CHANNEL_SCIENCE: &str = "Исследование";
pub const RADIO_CHANNEL_MEDICAL: &str = "Медицина";
pub const RADIO_CHANNEL_SUPPLY: &str = "Снабжение";
pub const RADIO_CHANNEL_SERVICE: &str = "Обслуживание";
pub const RADIO_CHANNEL_AI_PRIVATE: &str = "ИИ (личный)";
pub const RADIO_CHANNEL_ENTERTAINMENT: &str = "Развлечение";
pub const RADIO_CHANNEL_JUSTICE: &str = "Юридический";
pub const RADIO_CHANNEL_SYNDICATE: &str = "Синдикат";
pub const RADIO_CHANNEL_CENTCOM: &str = "ЦК";
pub const RADIO_CHANNEL_UPLINK: &str = "Аплинк";

/// `GLOB.default_radio_channels` with the `GLOB.freqtospan` class of each frequency.
// shortcut: the CTF team and status display classes are drawn as `.radio`; add spans when CTF or
// status displays exist.
pub const CHANNELS: [(&str, u16, Span); 19] = [
    (RADIO_CHANNEL_COMMON, FREQ_COMMON, Span::Radio),
    (RADIO_CHANNEL_SCIENCE, FREQ_SCIENCE, Span::SciRadio),
    (RADIO_CHANNEL_COMMAND, FREQ_COMMAND, Span::ComRadio),
    (RADIO_CHANNEL_MEDICAL, FREQ_MEDICAL, Span::MedRadio),
    (RADIO_CHANNEL_ENGINEERING, FREQ_ENGINEERING, Span::EngRadio),
    (RADIO_CHANNEL_SECURITY, FREQ_SECURITY, Span::SecRadio),
    (RADIO_CHANNEL_CENTCOM, FREQ_CENTCOM, Span::CentcomRadio),
    (RADIO_CHANNEL_SYNDICATE, FREQ_SYNDICATE, Span::SyndRadio),
    (RADIO_CHANNEL_UPLINK, FREQ_UPLINK, Span::SyndRadio),
    (RADIO_CHANNEL_SUPPLY, FREQ_SUPPLY, Span::SuppRadio),
    (RADIO_CHANNEL_SERVICE, FREQ_SERVICE, Span::ServRadio),
    (RADIO_CHANNEL_AI_PRIVATE, FREQ_AI_PRIVATE, Span::AiPrivRadio),
    (
        RADIO_CHANNEL_ENTERTAINMENT,
        FREQ_ENTERTAINMENT,
        Span::EnteRadio,
    ),
    (RADIO_CHANNEL_JUSTICE, FREQ_JUSTICE, Span::JusticeRadio),
    ("Синие", FREQ_CTF_BLUE, Span::Radio),
    ("Зеленые", FREQ_CTF_GREEN, Span::Radio),
    ("Красные", FREQ_CTF_RED, Span::Radio),
    ("Желтые", FREQ_CTF_YELLOW, Span::Radio),
    ("Captain-Cast", FREQ_STATUS_DISPLAYS, Span::Radio),
];

/// `GLOB.default_radio_channels[name]`.
pub fn channel_freq(name: &str) -> Option<u16> {
    CHANNELS.iter().find(|c| c.0 == name).map(|c| c.1)
}

/// Whether `freq` is one of `GLOB.default_radio_channels` (syndicate radios hear all of them).
pub fn is_channel_freq(freq: u16) -> bool {
    CHANNELS.iter().any(|c| c.1 == freq)
}

/// `get_radio_span`: the channel's class, `.radio` for any other frequency (whose
/// `get_radio_color` is `RADIO_COLOR_COMMON`, the `.radio` color).
pub fn radio_span(freq: u16) -> Span {
    CHANNELS
        .iter()
        .find(|c| c.1 == freq)
        .map_or(Span::Radio, |c| c.2)
}

/// `get_radio_name`: the name the telecomms servers (`frequency_infos`) or
/// `GLOB.reserved_radio_frequencies` give the frequency, else `145.9`.
pub fn radio_name(freq: u16) -> String {
    match CHANNELS.iter().find(|c| c.1 == freq) {
        Some(c) => c.0.to_owned(),
        None => format!("{}.{}", freq / 10, freq % 10),
    }
}

/// tg `sanitize_frequency`: into the allowed band, odd, never the syndicate frequency without
/// syndicate access.
pub fn sanitize_frequency(freq: f64, free: bool, syndie: bool) -> u16 {
    let (min, max) = if free {
        (MIN_FREE_FREQ, MAX_FREE_FREQ)
    } else {
        (MIN_FREQ, MAX_FREQ)
    };
    let mut freq = (freq.round() as i64).clamp(i64::from(min), i64::from(max)) as u16;
    if freq.is_multiple_of(2) {
        freq += 1;
    }
    if freq == FREQ_SYNDICATE && !syndie {
        FREQ_COMMON
    } else {
        freq
    }
}

/// `[Инженерия] Имя говорит, "текст"` in the frequency's class: the bracketed name and the
/// speaker are in the name span (bold), a whispered text is italic. Name and text inherit the
/// line's color on the client, like tg's nested spans.
pub fn radio_message(freq: u16, name: &str, verb: &str, text: &str, italics: bool) -> ChatMessage {
    let span = radio_span(freq);
    let text_span = if italics { Span::Italics } else { span };
    ChatMessage {
        channel: ChatChannel::Radio,
        segments: vec![
            ChatSegment::new(span, ""),
            ChatSegment::new(Span::Name, format!("[{}] ", radio_name(freq))),
            ChatSegment::new(Span::Name, name),
            ChatSegment::new(span, format!(" {verb}, \"")),
            ChatSegment::new(text_span, text),
            ChatSegment::new(span, "\""),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_spans_and_sanitizing_follow_tg() {
        assert_eq!(radio_name(FREQ_ENGINEERING), "Инженерия");
        assert_eq!(radio_name(1453), "145.3");
        assert_eq!(radio_span(FREQ_COMMON), Span::Radio);
        assert_eq!(radio_span(FREQ_JUSTICE), Span::JusticeRadio);
        assert_eq!(radio_span(FREQ_CONFESSIONAL), Span::Radio);
        assert_eq!(channel_freq(RADIO_CHANNEL_SECURITY), Some(FREQ_SECURITY));
        assert_eq!(sanitize_frequency(1459.0, false, false), FREQ_COMMON);
        assert_eq!(sanitize_frequency(1460.0, false, false), 1461);
        assert_eq!(sanitize_frequency(1357.0, false, false), MIN_FREQ);
        assert_eq!(sanitize_frequency(1357.0, true, false), FREQ_ENGINEERING);
        assert_eq!(sanitize_frequency(1213.0, true, false), FREQ_COMMON);
        assert_eq!(sanitize_frequency(1213.0, true, true), FREQ_SYNDICATE);
    }

    #[test]
    fn radio_line_reads_like_tg() {
        let line = radio_message(FREQ_ENGINEERING, "Боб", "говорит", "Привет", false);
        assert_eq!(line.channel, ChatChannel::Radio);
        assert_eq!(line.plain_text(), "[Инженерия] Боб говорит, \"Привет\"");
        assert_eq!(line.segments[0].span, Span::EngRadio);
        let whisper = radio_message(FREQ_COMMON, "Боб", "шепчет", "т*х*", true);
        assert_eq!(whisper.segments[4].span, Span::Italics);
    }
}
