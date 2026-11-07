//! The pure half of tg `/datum/song`: the note parser (`compile_synthesized`,
//! `compile_legacy`), the legacy note names (`playkey_legacy`), the synthesized keys
//! (`/datum/instrument/calculate_samples`, `/datum/instrument_key`), the tempo
//! (BandaStation's BPM unlock `tempodiv_to_delay`) and the song import (`ParseSong`).

use std::collections::BTreeMap;

/// `INSTRUMENT_MIN_OCTAVE`, `INSTRUMENT_MAX_OCTAVE`, `INSTRUMENT_MIN_KEY`, `INSTRUMENT_MAX_KEY`.
pub const MIN_OCTAVE: i32 = 1;
pub const MAX_OCTAVE: i32 = 9;
pub const MIN_KEY: i32 = 0;
pub const MAX_KEY: i32 = 127;
/// `SSinstruments.musician_maxlines`, `musician_maxlinechars`.
pub const MAX_LINES: usize = 1000;
pub const MAX_LINE_CHARS: usize = 300;
/// `world.tick_lag` in deciseconds (20 Hz).
pub const TICK_LAG_DS: f64 = 0.5;
/// `INSTRUMENT_STANDART_BPM`, `min_bpm`, `get_max_bpm()`.
pub const DEFAULT_BPM: u32 = 120;
pub const MIN_BPM: u32 = 1;
pub const MAX_BPM: u32 = (600.0 / TICK_LAG_DS) as u32;
/// `note_offset_lookup` (A..G from C).
const NOTE_OFFSET: [i32; 7] = [9, 11, 0, 2, 4, 5, 7];

/// One parsed note: `key` 1..=7 (A..G), its accidental and octave as they stand then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub key: u8,
    pub accent: String,
    pub octave: i32,
}

/// A chord: its notes and the tempo division (0 when the text was not a number: treated as 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Chord {
    pub notes: Vec<Note>,
    pub tempodiv: f64,
}

/// BYOND `text2num`: the leading number of `s` (`"120abc"` -> 120), `None` when there is none.
pub fn text2num(s: &str) -> Option<f64> {
    let s = s.trim_start();
    let mut end = 0;
    let bytes = s.as_bytes();
    if matches!(bytes.first(), Some(b'-' | b'+')) {
        end = 1;
    }
    let digits_start = end;
    let mut dot = false;
    while let Some(&b) = bytes.get(end) {
        match b {
            b'0'..=b'9' => end += 1,
            b'.' if !dot => {
                dot = true;
                end += 1;
            }
            _ => break,
        }
    }
    if end == digits_start || (dot && end == digits_start + 1) {
        return None;
    }
    s[..end].parse().ok()
}

/// `compile_synthesized`/`compile_legacy` up to their outputs: the octave and accidental of each
/// letter carry over from note to note (and line to line); a digit 1-9 sets the octave, anything
/// else the accidental (`!text2num`: also `0`).
pub fn parse(lines: &[String]) -> Vec<Chord> {
    let mut octaves = [3i32; 7];
    let mut accents: [String; 7] = std::array::from_fn(|_| "n".to_owned());
    let mut out = Vec::new();
    for line in lines {
        for chord in line.to_lowercase().split(',') {
            let mut parts = chord.split('/');
            let notes_text = parts.next().unwrap_or_default();
            let tempodiv = match parts.next() {
                Some(t) => text2num(t).unwrap_or(0.0),
                None => 1.0,
            };
            let mut notes = Vec::new();
            for note in notes_text.split('-') {
                let mut chars = note.chars();
                let Some(first) = chars.next() else {
                    continue;
                };
                let key = i64::from(u32::from(first)) - 96;
                if !(1..=7).contains(&key) {
                    continue;
                }
                let k = (key - 1) as usize;
                for c in chars {
                    match c.to_digit(10) {
                        Some(n) if n != 0 => {
                            octaves[k] = (n as i32).clamp(MIN_OCTAVE, MAX_OCTAVE);
                        }
                        _ => accents[k] = c.to_string(),
                    }
                }
                notes.push(Note {
                    key: key as u8,
                    accent: accents[k].clone(),
                    octave: octaves[k],
                });
            }
            out.push(Chord { notes, tempodiv });
        }
    }
    out
}

/// `accent_lookup` (an unknown accidental adds nothing).
fn accent_offset(accent: &str) -> i32 {
    match accent {
        "b" => -1,
        "s" | "#" => 1,
        _ => 0,
    }
}

/// `compile_synthesized`'s key of a note, clamped to the keyboard.
pub fn synth_key(note: &Note) -> i32 {
    (NOTE_OFFSET[usize::from(note.key - 1)] + note.octave * 12 + accent_offset(&note.accent))
        .clamp(MIN_KEY, MAX_KEY)
}

/// `playkey_legacy`'s file name of a note (`Ab3`, `Cn4`), `None` past its octaves.
pub fn legacy_name(note: &Note) -> Option<String> {
    let (mut key, mut acc, mut oct) = (i32::from(note.key), note.accent.clone(), note.octave);
    if acc == "b" && (key == 3 || key == 6) {
        // C or F flat: the white key below.
        if key == 3 {
            oct -= 1;
        }
        key -= 1;
        acc = "n".into();
    } else if acc == "#" && (key == 2 || key == 5) {
        // B or E sharp: the white key above.
        if key == 2 {
            oct += 1;
        }
        key += 1;
        acc = "n".into();
    } else if acc == "#" && key == 7 {
        key = 1;
        acc = "b".into();
    } else if acc == "#" {
        acc = "b".into();
        key += 1;
    }
    let max = if key == 3 { 9 } else { 8 };
    if oct < 1 || oct > max {
        return None;
    }
    let letter = char::from(b'@' + key as u8);
    Some(format!("{letter}{acc}{oct}"))
}

/// `calculate_samples` + `/datum/instrument_key` for one key: the sample and its playback
/// frequency multiplier (`2 ** (deviation / 12)`; 1 on a sampled key).
pub fn key_sample(samples: &BTreeMap<u8, String>, key: i32) -> Option<(&str, f32)> {
    let mut keys = samples.keys().map(|&k| i32::from(k));
    let first = keys.next()?;
    let last = samples.keys().next_back().map(|&k| i32::from(k))?;
    let pick = |k: i32| samples[&(k as u8)].as_str();
    let (sample, deviation) = if key <= first {
        (pick(first), key - first)
    } else if key >= last {
        (pick(last), key - last)
    } else {
        let mut from = first;
        let mut found = (pick(first), 0);
        for to in samples.keys().map(|&k| i32::from(k)).skip(1) {
            if key <= to {
                let pivot = (from + to).div_euclid(2);
                found = if key <= pivot {
                    (pick(from), key - from)
                } else {
                    (pick(to), key - to)
                };
                break;
            }
            from = to;
        }
        found
    };
    Some((sample, 2f32.powf(deviation as f32 / 12.0)))
}

/// BandaStation's `tempodiv_to_delay`: whole ticks until the next chord, carrying the fraction
/// in `residual` so a song keeps its exact BPM on average; at least one tick.
pub fn tempodiv_to_delay(bpm: u32, tempodiv: f64, residual: &mut f64) -> u32 {
    let tempodiv = if tempodiv == 0.0 { 1.0 } else { tempodiv };
    let ds_per_div = 600.0 / f64::from(bpm.max(1)) / tempodiv;
    let with_res = ds_per_div / TICK_LAG_DS + *residual;
    let ticks = with_res.floor().max(1.0);
    *residual = with_res - ticks;
    ticks as u32
}

/// `set_bpm`'s clamp.
pub fn clamp_bpm(bpm: f64) -> u32 {
    bpm.round().clamp(f64::from(MIN_BPM), f64::from(MAX_BPM)) as u32
}

/// What `ParseSong` made of a pasted song.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParsedSong {
    pub lines: Vec<String>,
    pub bpm: u32,
    /// The messages to the user (`Too many lines!`, `Line N too long!`).
    pub warnings: Vec<String>,
}

/// `ParseSong`: a `BPM: N` first line sets the tempo (120 otherwise); at most 1000 lines, and
/// lines over 300 characters are dropped.
pub fn parse_song(text: &str) -> ParsedSong {
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let mut out = ParsedSong {
        bpm: DEFAULT_BPM,
        ..ParsedSong::default()
    };
    if let Some(rest) = lines.first().and_then(|l| l.strip_prefix("BPM: ")) {
        out.bpm = clamp_bpm(
            text2num(rest)
                .filter(|v| *v != 0.0)
                .unwrap_or(f64::from(DEFAULT_BPM)),
        );
        lines.remove(0);
    }
    if lines.len() > MAX_LINES {
        out.warnings.push("Слишком много строк!".to_owned());
        lines.truncate(MAX_LINES);
    }
    let mut linenum = 1;
    for line in lines {
        if line.chars().count() > MAX_LINE_CHARS {
            out.warnings
                .push(format!("Строка {linenum} слишком длинная!"));
        } else {
            out.lines.push(line);
            linenum += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|l| (*l).to_owned()).collect()
    }

    #[test]
    fn notes_keep_their_octave_and_accidental() {
        // "C,E-G/2,A4" : C3 alone, then a chord, then A in octave 4.
        let chords = parse(&lines(&["C,E-G/2,A4b"]));
        assert_eq!(chords.len(), 3);
        assert_eq!(synth_key(&chords[0].notes[0]), 36); // C3 = 0 + 36
        assert_eq!(chords[1].tempodiv, 2.0);
        assert_eq!(chords[1].notes.len(), 2);
        assert_eq!(synth_key(&chords[2].notes[0]), 9 + 48 - 1); // Ab4
        // The flat stays on A until changed, across lines; a rest is a chord without notes.
        let chords = parse(&lines(&["A4b", ",A"]));
        assert_eq!(chords.len(), 3);
        assert!(chords[1].notes.is_empty());
        assert_eq!(chords[2].notes[0].accent, "b");
        assert_eq!(chords[2].notes[0].octave, 4);
        // `0` is not an octave (`!text2num`), letters past G are skipped, bad divs are 0.
        let chords = parse(&lines(&["c0-h-x/abc"]));
        assert_eq!(chords[0].notes.len(), 1);
        assert_eq!(chords[0].notes[0].accent, "0");
        assert_eq!(chords[0].tempodiv, 0.0);
    }

    #[test]
    fn legacy_names_fold_accidentals_like_tg() {
        let note = |key, acc: &str, octave| Note {
            key,
            accent: acc.to_owned(),
            octave,
        };
        assert_eq!(legacy_name(&note(3, "n", 4)).as_deref(), Some("Cn4"));
        // Cb4 -> Bn3, B#3 -> Cn4, G#4 -> Ab4, D#4 -> Eb4.
        assert_eq!(legacy_name(&note(3, "b", 4)).as_deref(), Some("Bn3"));
        assert_eq!(legacy_name(&note(2, "#", 3)).as_deref(), Some("Cn4"));
        assert_eq!(legacy_name(&note(7, "#", 4)).as_deref(), Some("Ab4"));
        assert_eq!(legacy_name(&note(4, "#", 4)).as_deref(), Some("Eb4"));
        // Only C reaches octave 9.
        assert_eq!(legacy_name(&note(3, "n", 9)).as_deref(), Some("Cn9"));
        assert_eq!(legacy_name(&note(1, "n", 9)), None);
    }

    #[test]
    fn keys_use_the_nearest_sample_with_tg_pivots() {
        let samples: BTreeMap<u8, String> = [(36, "c2".to_owned()), (48, "c3".to_owned())].into();
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let (s, f) = key_sample(&samples, 36).unwrap();
        assert!(s == "c2" && close(f, 1.0));
        // Pivot floor(42): 42 still from c2 (+6 semitones), 43 from c3 (-5).
        let (s, f) = key_sample(&samples, 42).unwrap();
        assert!(s == "c2" && close(f, 2f32.powf(0.5)));
        let (s, f) = key_sample(&samples, 43).unwrap();
        assert!(s == "c3" && close(f, 2f32.powf(-5.0 / 12.0)));
        // Below the first and above the last sample.
        let (s, f) = key_sample(&samples, 24).unwrap();
        assert!(s == "c2" && close(f, 0.5));
        let (s, f) = key_sample(&samples, 60).unwrap();
        assert!(s == "c3" && close(f, 2.0));
        assert!(key_sample(&BTreeMap::new(), 60).is_none());
    }

    #[test]
    fn tempo_keeps_the_bpm_on_average() {
        // 120 BPM: 5 ds a beat, 10 ticks; /3: 3.33 ticks -> 3, 3, 4.
        let mut res = 0.0;
        assert_eq!(tempodiv_to_delay(120, 1.0, &mut res), 10);
        let mut res = 0.0;
        let ticks: Vec<u32> = (0..3)
            .map(|_| tempodiv_to_delay(120, 3.0, &mut res))
            .collect();
        assert_eq!(ticks.iter().sum::<u32>(), 10);
        // Faster than a tick: still one tick; a 0 div counts as 1.
        let mut res = 0.0;
        assert_eq!(tempodiv_to_delay(1200, 4.0, &mut res), 1);
        let mut res = 0.0;
        assert_eq!(tempodiv_to_delay(120, 0.0, &mut res), 10);
        assert_eq!(clamp_bpm(5000.0), MAX_BPM);
        assert_eq!(clamp_bpm(0.2), MIN_BPM);
    }

    #[test]
    fn imported_songs_take_their_bpm_line() {
        let song = parse_song("BPM: 90\nA,B\nC");
        assert_eq!(song.bpm, 90);
        assert_eq!(song.lines, ["A,B", "C"]);
        let song = parse_song("A");
        assert_eq!(song.bpm, DEFAULT_BPM);
        let long = "a".repeat(MAX_LINE_CHARS + 1);
        let song = parse_song(&format!("A\n{long}\nB"));
        assert_eq!(song.lines, ["A", "B"]);
        assert_eq!(song.warnings, ["Строка 2 слишком длинная!"]);
        assert_eq!(text2num("120abc"), Some(120.0));
        assert_eq!(text2num("x"), None);
    }
}
