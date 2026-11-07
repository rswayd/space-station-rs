//! The music windows' data, shared by both halves: tgui `InstrumentEditor220.tsx` (the song
//! editor of `/datum/song`) and `Jukebox220.tsx` (`/obj/machinery/jukebox`). The server half is
//! `crate::instruments`, the client views `crate::instruments_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `InstrumentEditor220`.
pub struct InstrumentEditorUi;

impl UiInterface for InstrumentEditorUi {
    const NAME: &'static str = "InstrumentEditor220";
    type Data = InstrumentEditorData;
    type StaticData = InstrumentEditorStatic;
}

/// A `possible_instruments` entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PossibleInstrument {
    pub name: String,
    pub id: String,
}

/// `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstrumentEditorStatic {
    pub can_switch_instrument: bool,
    pub possible_instruments: Vec<PossibleInstrument>,
    pub sustain_modes: Vec<String>,
    pub max_repeats: u32,
    pub min_volume: u32,
    pub max_volume: u32,
    pub note_shift_min: i32,
    pub note_shift_max: i32,
    pub max_line_chars: u32,
    pub max_lines: u32,
    pub min_bpm: u32,
    pub max_bpm: u32,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstrumentEditorData {
    /// The sync id.
    pub id: String,
    pub using_instrument: String,
    pub note_shift: i32,
    pub sustain_mode: String,
    pub sustain_mode_button: String,
    pub sustain_mode_duration: f32,
    pub sustain_mode_min: f32,
    pub sustain_mode_max: f32,
    pub instrument_ready: bool,
    pub volume: u32,
    pub volume_dropoff_threshold: f32,
    pub sustain_indefinitely: bool,
    pub playing: bool,
    pub repeat: u32,
    pub bpm: u32,
    pub lines: Vec<String>,
}

/// tgui `Jukebox220`.
pub struct JukeboxUi;

impl UiInterface for JukeboxUi {
    const NAME: &'static str = "Jukebox220";
    type Data = JukeboxData;
    type StaticData = ();
}

/// A `songs` entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct JukeboxSong {
    pub name: String,
    /// Deciseconds (BandaStation sends the number, the window formats it).
    pub length: u32,
    /// `song_beat_deciseconds`, 0 unknown.
    pub beat: f32,
}

/// `get_ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct JukeboxData {
    pub active: bool,
    pub songs: Vec<JukeboxSong>,
    pub track_selected: Option<String>,
    pub looping: bool,
    pub volume: f32,
}
