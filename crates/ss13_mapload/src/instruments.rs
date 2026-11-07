//! Musical instruments from the DM dump, written by `ss13-mapload convert` to
//! [`INSTRUMENTS_FILE`] and read by the game (`ss13_game::instruments`):
//!
//! - every `/datum/instrument` (`SSinstruments.instrument_data`, by `id`): its samples
//!   (`real_samples`, key -> file) or, for a legacy (hardcoded) one, its note directory and the
//!   `.ogg` notes that exist there (tg `fexists` at play time);
//! - the instrument holders: `/obj/item/instrument` and `/obj/structure/musician` subtypes with
//!   their `allowed_instrument_ids`, `instrument_range`, `playable_while_worn`,
//!   `can_play_unanchored`;
//! - the jukeboxes' `req_access`.
//!
//! [`Instruments::sounds`] lists the files the game can name, copied with the other sounds.
// shortcut: legacy instruments with `.mid` notes (violin, saxophone, recorder, ...) list no
// notes: the client's audio decodes no MIDI (BYOND does), so they play nothing; convert the
// notes to `.ogg` to bring them back.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{is_under, num};

/// `<assets>/instruments.ron`.
pub const INSTRUMENTS_FILE: &str = "instruments.ron";
const INSTRUMENT_ROOT: &str = "/datum/instrument";
const ITEM_ROOT: &str = "/obj/item/instrument";
const STRUCTURE_ROOT: &str = "/obj/structure/musician";
const JUKEBOX_ROOT: &str = "/obj/machinery/jukebox";
/// `INSTRUMENT_LEGACY`.
const INSTRUMENT_LEGACY: u32 = 1;
/// The legacy notes' directory (`playkey_legacy`).
pub const LEGACY_DIR: &str = "sound/runtime/instruments";
/// The boombox tapes' tracks (`/datum/looping_sound/boombox` subtypes' `mid_sounds`).
// Deviation: the robocop tape's `sound/music/elevator/robocop-short.ogg` is left out: a remix of
// a copyrighted theme with no licence we can redistribute under (as the lobby's title2).
pub const BOOMBOX_TRACKS: [&str; 5] = [
    "sound/music/boombox/industrial-crush.ogg",
    "sound/music/boombox/bainmack-rock.ogg",
    "sound/music/boombox/groovepad-hiphop.ogg",
    "sound/music/boombox/frankyboomer-jazz.ogg",
    "sound/music/boombox/bubblegum-pop.ogg",
];

/// The literal `playsound` files of the jukebox and boombox procs.
pub const MUSIC_MACHINE_SOUNDS: [&str; 5] = [
    "sound/machines/compiler/compiler-failure.ogg",
    "sound/machines/terminal/terminal_off.ogg",
    "sound/items/taperecorder/taperecorder_close.ogg",
    "sound/machines/click.ogg",
    "sound/items/tools/ratchet.ogg",
];

/// One `/datum/instrument`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstrumentData {
    pub name: String,
    pub category: String,
    /// `real_samples`: MIDI key -> sample file.
    pub samples: BTreeMap<u8, String>,
    /// `legacy_instrument_path` of a legacy instrument (`None`: synthesized).
    pub legacy_dir: Option<String>,
    /// The notes of a legacy instrument that exist as `.ogg` (`Ab3`, `Cn4`, ...).
    pub legacy_notes: BTreeSet<String>,
    pub volume_multiplier: f32,
    pub admin_only: bool,
}

/// An `/obj/item/instrument` or `/obj/structure/musician` type.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstrumentHolder {
    pub allowed: Vec<String>,
    pub range: f32,
    pub playable_while_worn: bool,
    pub can_play_unanchored: bool,
    /// A structure (`/datum/song/stationary`), not a handheld item.
    pub stationary: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Instruments {
    /// By `id`.
    pub instruments: BTreeMap<String, InstrumentData>,
    /// By type path.
    pub holders: BTreeMap<String, InstrumentHolder>,
    /// Jukebox type -> `req_access`.
    pub jukeboxes: BTreeMap<String, Vec<String>>,
}

fn text(types: &DmTypes, path: &str, var: &str) -> Option<String> {
    types
        .resolved(path, var)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn list(v: Option<&Value>) -> Vec<&Value> {
    v.and_then(|v| v.get("list"))
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn abstract_type(types: &DmTypes, path: &str) -> bool {
    types
        .resolved(path, "abstract_type")
        .and_then(|v| v.get("path"))
        .and_then(Value::as_str)
        == Some(path)
}

/// A text or a list of texts (`allowed_instrument_ids = "violin"` or `list(...)`).
fn ids(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => vec![s.clone()],
        v => list(v)
            .into_iter()
            .filter_map(|i| i.as_str().map(str::to_owned))
            .collect(),
    }
}

fn samples(types: &DmTypes, path: &str) -> BTreeMap<u8, String> {
    list(types.resolved(path, "real_samples"))
        .into_iter()
        .filter_map(|item| {
            let [key, file] = item.get("assoc")?.as_array()?.as_slice() else {
                return None;
            };
            let key: u8 = key.as_str()?.parse().ok().filter(|k| *k <= 127)?;
            Some((key, file.get("resource")?.as_str()?.to_owned()))
        })
        .collect()
}

/// The `.ogg` notes in `<source>/sound/runtime/instruments/<dir>/`.
fn legacy_notes(source: &Path, dir: &str) -> BTreeSet<String> {
    let Ok(entries) = std::fs::read_dir(source.join(LEGACY_DIR).join(dir)) else {
        return BTreeSet::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".ogg").map(str::to_owned)
        })
        .collect()
}

/// `SSinstruments.initialize_instrument_data` (`valid_subtypesof(/datum/instrument)`), the
/// holders and jukeboxes. `source` is the BandaStation checkout (legacy notes).
pub fn resolve(types: &DmTypes, source: &Path) -> Instruments {
    let mut out = Instruments::default();
    for path in types.types.keys() {
        if is_under(path, INSTRUMENT_ROOT) && path != INSTRUMENT_ROOT {
            if abstract_type(types, path) {
                continue;
            }
            let id = text(types, path, "id").unwrap_or_else(|| path.clone());
            let flags = num(types.resolved(path, "instrument_flags")).unwrap_or(0.0) as u32;
            let legacy = flags & INSTRUMENT_LEGACY != 0;
            let legacy_dir = legacy
                .then(|| text(types, path, "legacy_instrument_path"))
                .flatten();
            let ext = text(types, path, "legacy_instrument_ext").unwrap_or_default();
            let legacy_notes = match &legacy_dir {
                Some(dir) if ext == "ogg" => legacy_notes(source, dir),
                _ => BTreeSet::new(),
            };
            let data = InstrumentData {
                name: text(types, path, "name").unwrap_or_default(),
                category: text(types, path, "category").unwrap_or_default(),
                samples: if legacy {
                    BTreeMap::new()
                } else {
                    samples(types, path)
                },
                legacy_dir,
                legacy_notes,
                volume_multiplier: types
                    .resolved(path, "volume_multiplier")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.33) as f32,
                admin_only: num(types.resolved(path, "admin_only")).unwrap_or(0.0) != 0.0,
            };
            out.instruments.insert(id, data);
        } else if is_under(path, ITEM_ROOT) || is_under(path, STRUCTURE_ROOT) {
            if abstract_type(types, path) {
                continue;
            }
            let stationary = is_under(path, STRUCTURE_ROOT);
            let flag = |var: &str| num(types.resolved(path, var)).unwrap_or(0.0) != 0.0;
            out.holders.insert(
                path.clone(),
                InstrumentHolder {
                    allowed: ids(types.resolved(path, "allowed_instrument_ids")),
                    // `/datum/song` `instrument_range = 15` for structures.
                    range: types
                        .resolved(path, "instrument_range")
                        .and_then(Value::as_f64)
                        .unwrap_or(15.0) as f32,
                    playable_while_worn: flag("playable_while_worn"),
                    can_play_unanchored: flag("can_play_unanchored"),
                    stationary,
                },
            );
        } else if is_under(path, JUKEBOX_ROOT) {
            out.jukeboxes
                .insert(path.clone(), ids(types.resolved(path, "req_access")));
        }
    }
    out
}

impl Instruments {
    /// Every sample, legacy note, boombox track and machine sound.
    pub fn sounds(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = BOOMBOX_TRACKS
            .iter()
            .chain(&MUSIC_MACHINE_SOUNDS)
            .map(|s| (*s).to_owned())
            .collect();
        for data in self.instruments.values() {
            out.extend(data.samples.values().cloned());
            if let Some(dir) = &data.legacy_dir {
                out.extend(
                    data.legacy_notes
                        .iter()
                        .map(|n| format!("{LEGACY_DIR}/{dir}/{n}.ogg")),
                );
            }
        }
        out
    }

    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let text = anyhow::Context::with_context(ss13_data::read_to_string(path), || {
            format!("reading {}", path.display())
        })?;
        anyhow::Context::with_context(ron::from_str::<Self>(&text), || {
            format!("parsing {}", path.display())
        })
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("instruments serialize"),
        );
        out.push('\n');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/instrument": { "parent": "/datum", "declared_vars": {
            "abstract_type": { "path": "/datum/instrument" }, "id": null, "name": "Generic",
            "instrument_flags": 0, "volume_multiplier": 0.33, "admin_only": 0,
            "legacy_instrument_path": null, "legacy_instrument_ext": null } },
        "/datum/instrument/piano": { "parent": "/datum/instrument", "declared_vars": {
            "abstract_type": { "path": "/datum/instrument/piano" }, "category": "Piano" } },
        "/datum/instrument/piano/fluid": { "parent": "/datum/instrument/piano",
            "declared_vars": { "id": "r3grand", "name": "FluidR3 Grand Piano",
            "real_samples": { "list": [
              { "assoc": ["36", { "resource": "sound/p/c2.ogg" }] },
              { "assoc": ["48", { "resource": "sound/p/c3.ogg" }] } ] } } },
        "/datum/instrument/hardcoded": { "parent": "/datum/instrument", "declared_vars": {
            "abstract_type": { "path": "/datum/instrument/hardcoded" }, "instrument_flags": 1,
            "volume_multiplier": 1 } },
        "/datum/instrument/hardcoded/violin": { "parent": "/datum/instrument/hardcoded",
            "declared_vars": { "id": "violin", "name": "Violin",
            "legacy_instrument_path": "violin", "legacy_instrument_ext": "mid" } },
        "/obj": { "parent": null },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/instrument": { "parent": "/obj/item", "declared_vars": {
            "abstract_type": { "path": "/obj/item/instrument" }, "instrument_range": 15,
            "playable_while_worn": 0, "allowed_instrument_ids": null } },
        "/obj/item/instrument/violin": { "parent": "/obj/item/instrument", "declared_vars": {
            "allowed_instrument_ids": "violin" } },
        "/obj/item/instrument/harmonica": { "parent": "/obj/item/instrument", "declared_vars": {
            "allowed_instrument_ids": { "list": ["crharmony", "harmonica"] },
            "playable_while_worn": 1 } },
        "/obj/structure": { "parent": "/obj" },
        "/obj/structure/musician": { "parent": "/obj/structure", "declared_vars": {
            "allowed_instrument_ids": { "list": ["r3grand", "piano"] },
            "can_play_unanchored": 0 } },
        "/obj/machinery": { "parent": "/obj" },
        "/obj/machinery/jukebox": { "parent": "/obj/machinery", "declared_vars": {
            "req_access": { "list": ["bar"] } } },
        "/obj/machinery/jukebox/no_access": { "parent": "/obj/machinery/jukebox",
            "declared_vars": { "req_access": null } }
      }
    }"##;

    #[test]
    fn instruments_holders_and_jukeboxes_come_from_the_dump() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let data = resolve(&types, Path::new("/nonexistent"));
        // Abstract types are skipped.
        assert_eq!(
            data.instruments.keys().collect::<Vec<_>>(),
            ["r3grand", "violin"]
        );
        let piano = &data.instruments["r3grand"];
        assert_eq!(piano.category, "Piano");
        assert_eq!(piano.samples[&48], "sound/p/c3.ogg");
        assert!((piano.volume_multiplier - 0.33).abs() < 1e-6);
        let violin = &data.instruments["violin"];
        assert_eq!(violin.legacy_dir.as_deref(), Some("violin"));
        assert!(violin.samples.is_empty() && violin.legacy_notes.is_empty());
        assert_eq!(violin.volume_multiplier, 1.0);
        assert_eq!(
            data.holders["/obj/item/instrument/violin"].allowed,
            ["violin"]
        );
        let harmonica = &data.holders["/obj/item/instrument/harmonica"];
        assert!(harmonica.playable_while_worn && !harmonica.stationary);
        assert_eq!(harmonica.allowed, ["crharmony", "harmonica"]);
        let piano = &data.holders["/obj/structure/musician"];
        assert!(piano.stationary && piano.range == 15.0);
        assert!(!data.holders.contains_key("/obj/item/instrument"));
        assert_eq!(data.jukeboxes["/obj/machinery/jukebox"], ["bar"]);
        assert!(data.jukeboxes["/obj/machinery/jukebox/no_access"].is_empty());
        let sounds = data.sounds();
        assert!(sounds.contains("sound/p/c2.ogg"));
        assert!(sounds.contains(BOOMBOX_TRACKS[0]));
        assert_eq!(Instruments::read_str(&data.to_ron()), data);
    }

    impl Instruments {
        fn read_str(text: &str) -> Self {
            ron::from_str(text).unwrap()
        }
    }

    /// The real legacy piano's `.ogg` notes, when the source checkout is there.
    #[test]
    fn legacy_ogg_notes_are_listed() {
        let source = std::env::var_os("SS13_SOURCE")
            .map_or_else(|| std::path::PathBuf::from(env!("SS13_SOURCE")), Into::into);
        let notes = legacy_notes(&source, "piano");
        if notes.is_empty() {
            return;
        }
        assert!(notes.contains("Cn4"), "{notes:?}");
        assert!(legacy_notes(&source, "violin").is_empty());
    }
}
