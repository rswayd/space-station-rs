//! Sounds from the DM dump, written by `ss13-mapload convert`:
//!
//! - [`SFX_FILE`]: tg's SFX keys (`/datum/sound_effect` and `/assoc` subtypes,
//!   `code/game/sound/sound_keys/`): key -> files with their `pickweight` weights. The game
//!   (`ss13_game::sound`) reads it at startup for `get_sfx`. It also holds the footstep tables
//!   (`GLOB.footstep`, `barefootstep`, `clawfootstep`, `heavyfootstep`), dumped as globals.
//! - [`SOUND_DIR`]: the `.ogg` files the server can name — every SFX file, the sound vars
//!   ([`SOUND_VARS`]) of the converted types and [`RUNTIME_SOUNDS`] — copied at their tg path,
//!   with the `attributions.txt` / `attribution.txt` / `license.txt` of every directory they came
//!   from and a `SOURCES.txt` (file, source path, source commit).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use ss13_assets::license::Attributions;
pub use ss13_assets::license::source_commit;

use crate::convert::DmTypes;
use crate::fidelity::is_under;

/// `<assets>/sfx.ron`.
pub const SFX_FILE: &str = "sfx.ron";
/// `<assets>/sound/<tg path>`.
pub const SOUND_DIR: &str = "sound";
const SFX_ROOT: &str = "/datum/sound_effect";

/// Vars holding a sound file (or an SFX key) on the types the game ports.
pub const SOUND_VARS: [&str; 17] = [
    "fire_sound",
    "dry_fire_sound",
    "load_sound",
    "load_empty_sound",
    "rack_sound",
    "lock_back_sound",
    "eject_sound",
    "eject_empty_sound",
    "bolt_drop_sound",
    "hitsound",
    "doorOpen",
    "doorClose",
    "doorDeni",
    "boltUp",
    "boltDown",
    "animation_sound",
    "empty_alarm_sound",
];

/// Literal `playsound` files of the ported procs.
pub mod files {
    /// `/obj/item/proc/attack` of a forceless item.
    pub const TAP: &str = "sound/items/weapons/tap.ogg";
    /// `/obj/item/Initialize`: the hitsound of `BURN` items without one.
    pub const WELDER: &str = "sound/items/tools/welder.ogg";
    /// `SFX_SWING_HIT`: the hitsound of `BRUTE` items without one.
    pub const SFX_SWING_HIT: &str = "swing_hit";
    /// Human arms and legs: `unarmed_attack_sound`, `unarmed_miss_sound`.
    pub const PUNCH: &str = "sound/items/weapons/punch1.ogg";
    pub const PUNCH_MISS: &str = "sound/items/weapons/punchmiss.ogg";
    /// `/obj/machinery/door/window` `open()` / `close()`.
    pub const WINDOOR: &str = "sound/machines/windowdoor.ogg";
    /// `/obj/item/gun/ballistic/fire_sounds` on low ammo.
    pub const BALLISTIC_CLICK: &str = "sound/items/weapons/gun/general/ballistic_click.ogg";
    /// Disposals: `disposaloutlet/expel`, `expel_holder`, `disposal/flush`.
    pub const WARNING_BUZZER: &str = "sound/machines/warning-buzzer.ogg";
    pub const HISS: &str = "sound/machines/hiss.ogg";
    pub const DISPOSAL_FLUSH: &str = "sound/machines/disposalflush.ogg";
    /// `/obj/machinery/door/airlock/animation_effects` with `BYPASS_DOOR_CHECKS`.
    pub const AIRLOCK_FORCED: &str = "sound/machines/airlock/airlockforced.ogg";
    /// `/datum/element/footstep/play_craw_step`.
    pub const CRAWL: &str = "sound/effects/footstep/crawl1.ogg";
    /// `/obj/structure/table`: `flipped_table_sound`, `unflip_table_sound`.
    pub const TABLE_FLIP: &str = "sound/items/trayhit/trayhit1.ogg";
    pub const TABLE_UNFLIP: &str = "sound/items/trayhit/trayhit2.ogg";
    /// `/datum/element/table_smash/tablepush`.
    pub const TABLE_SLAM: &str = "sound/effects/tableslam.ogg";
    /// `wrench_act` of tables and table frames.
    pub const DECONSTRUCT: &str = "sound/items/deconstruct.ogg";
}

pub const RUNTIME_SOUNDS: [&str; 15] = [
    files::TAP,
    files::WELDER,
    files::PUNCH,
    files::PUNCH_MISS,
    files::WINDOOR,
    files::BALLISTIC_CLICK,
    files::WARNING_BUZZER,
    files::HISS,
    files::DISPOSAL_FLUSH,
    files::AIRLOCK_FORCED,
    files::CRAWL,
    files::TABLE_FLIP,
    files::TABLE_UNFLIP,
    files::TABLE_SLAM,
    files::DECONSTRUCT,
];

/// The footstep tables, by global name (`ss13_dm_extract::DUMPED_GLOBALS`).
pub const FOOTSTEP_TABLES: [&str; 4] =
    ["footstep", "barefootstep", "clawfootstep", "heavyfootstep"];

/// One footstep table entry, `list(list(files), base volume, extra range)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StepSound {
    pub files: Vec<String>,
    pub volume: f32,
    pub range: f32,
}

/// SFX key -> (file, weight).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SfxData {
    pub sfx: BTreeMap<String, Vec<(String, u32)>>,
    /// Footstep table name -> turf footstep key (`/turf/open` `footstep` etc.) -> sound.
    #[serde(default)]
    pub footsteps: BTreeMap<String, BTreeMap<String, StepSound>>,
}

impl SfxData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let text = ss13_data::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        ron::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(&ron::to_string(self).expect("sfx data serializes"));
        out.push('\n');
        out
    }

    pub fn files(&self) -> impl Iterator<Item = &str> {
        let steps = self.footsteps.values().flat_map(|t| t.values());
        (self.sfx.values().flatten().map(|(f, _)| f.as_str()))
            .chain(steps.flat_map(|s| s.files.iter().map(String::as_str)))
    }
}

fn resource(v: &Value) -> Option<&str> {
    v.get("resource").and_then(Value::as_str)
}

/// Every `/datum/sound_effect` with a `key`: `file_paths` as a plain list (weight 1) or an
/// assoc list of weights (`/datum/sound_effect/assoc`).
pub fn resolve_sfx(types: &DmTypes) -> SfxData {
    let mut sfx = BTreeMap::new();
    for path in types.types.keys().filter(|p| is_under(p, SFX_ROOT)) {
        let Some(key) = types.resolved(path, "key").and_then(Value::as_str) else {
            continue;
        };
        let Some(Value::Array(items)) = types
            .resolved(path, "file_paths")
            .and_then(|v| v.get("list"))
        else {
            continue;
        };
        let entries: Vec<(String, u32)> = items
            .iter()
            .filter_map(|item| match item.get("assoc") {
                Some(Value::Array(pair)) => Some((
                    resource(pair.first()?)?.to_owned(),
                    pair.get(1).and_then(Value::as_f64).unwrap_or(1.0).max(0.0) as u32,
                )),
                _ => Some((resource(item)?.to_owned(), 1)),
            })
            .collect();
        if !entries.is_empty() {
            sfx.insert(key.to_owned(), entries);
        }
    }
    SfxData {
        sfx,
        footsteps: resolve_footsteps(types),
    }
}

fn list(v: &Value) -> &[Value] {
    v.get("list")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// [`FOOTSTEP_TABLES`] from the dumped globals: `list(key = list(list(files), volume, range))`.
fn resolve_footsteps(types: &DmTypes) -> BTreeMap<String, BTreeMap<String, StepSound>> {
    let mut out = BTreeMap::new();
    for name in FOOTSTEP_TABLES {
        let Some(table) = types.globals.get(name) else {
            continue;
        };
        let mut entries = BTreeMap::new();
        for item in list(table) {
            let Some([key, value]) = item
                .get("assoc")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
            else {
                continue;
            };
            let (Some(key), [files, volume, range]) = (key.as_str(), list(value)) else {
                continue;
            };
            entries.insert(
                key.to_owned(),
                StepSound {
                    files: list(files)
                        .iter()
                        .filter_map(|f| resource(f).map(str::to_owned))
                        .collect(),
                    volume: volume.as_f64().unwrap_or_default() as f32,
                    range: range.as_f64().unwrap_or_default() as f32,
                },
            );
        }
        out.insert(name.to_owned(), entries);
    }
    out
}

/// A sound var as the game uses it: a file path, or an SFX key; empty for `null`.
pub fn sound_var(types: &DmTypes, path: &str, var: &str) -> String {
    match types.resolved(path, var) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => resource(v).unwrap_or_default().to_owned(),
        None => String::new(),
    }
}

/// The files the server can name: SFX files, [`SOUND_VARS`] of `converted`, [`RUNTIME_SOUNDS`].
pub fn referenced<'a>(
    types: &DmTypes,
    sfx: &SfxData,
    converted: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = sfx.files().map(str::to_owned).collect();
    out.extend(RUNTIME_SOUNDS.map(str::to_owned));
    for path in converted {
        for var in SOUND_VARS {
            let s = sound_var(types, path, var);
            if s.ends_with(".ogg") {
                out.insert(s);
            }
        }
    }
    out
}

/// `rel` under `root`, matching each component case-insensitively when the exact one is
/// missing (BYOND resolves resources case-insensitively).
fn resolve_case(root: &Path, rel: &str) -> Option<PathBuf> {
    let exact = root.join(rel);
    if exact.is_file() {
        return Some(PathBuf::from(rel));
    }
    let mut found = PathBuf::new();
    for part in rel.split('/') {
        let dir = root.join(&found);
        let name = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .find(|n| n.to_string_lossy().eq_ignore_ascii_case(part))?;
        found.push(name);
    }
    Some(found)
}

/// What [`copy_sounds`] did.
#[derive(Debug, Default)]
pub struct CopyReport {
    pub copied: usize,
    pub bytes: u64,
    pub attributions: usize,
    pub missing: Vec<String>,
}

/// Copies `files` from `source` to `<assets>/sound/<file>`, with the attribution files of every
/// directory on each file's path, and writes `SOURCES.txt`.
pub fn copy_sounds(
    source: &Path,
    assets: &Path,
    files: &BTreeSet<String>,
    commit: &str,
) -> anyhow::Result<CopyReport> {
    let out = assets.join(SOUND_DIR);
    let mut report = CopyReport::default();
    let mut sources = format!(
        "# Sound files copied by `ss13-mapload convert` from BandaStation (tgstation fork), \
         commit {commit}.\n# Licenses: the attribution/license files next to them, and the \
         source repository's.\n# file\tsource path\n"
    );
    let mut attributions = Attributions::default();
    for rel in files {
        let Some(found) = resolve_case(source, rel) else {
            report.missing.push(rel.clone());
            continue;
        };
        let dest = out.join(rel);
        std::fs::create_dir_all(dest.parent().expect("file under the sound dir"))?;
        report.bytes +=
            std::fs::copy(source.join(&found), &dest).with_context(|| format!("copying {rel}"))?;
        report.copied += 1;
        sources.push_str(&format!("{rel}\t{}\n", found.display()));
        attributions.add(&found, rel);
    }
    report.attributions = attributions.copy(source, &out)?;
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("SOURCES.txt"), sources)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/sound_effect": { "parent": "/datum", "declared_vars": { "key": null,
            "file_paths": null } },
        "/datum/sound_effect/punch": { "parent": "/datum/sound_effect", "declared_vars": {
            "key": "punch", "file_paths": { "list": [ { "resource": "sound/p1.ogg" },
            { "resource": "sound/p2.ogg" } ] } } },
        "/datum/sound_effect/assoc": { "parent": "/datum/sound_effect" },
        "/datum/sound_effect/assoc/meow": { "parent": "/datum/sound_effect/assoc",
            "declared_vars": { "key": "cat_meow", "file_paths": { "list": [
            { "assoc": [ { "resource": "sound/m1.ogg" }, 8 ] },
            { "assoc": [ { "resource": "sound/m2.ogg" }, 23 ] } ] } } },
        "/obj": { "parent": null },
        "/obj/item": { "parent": "/obj", "declared_vars": { "hitsound": null } },
        "/obj/item/gun": { "parent": "/obj/item", "declared_vars": {
            "fire_sound": { "resource": "sound/weapons/shot.ogg" }, "hitsound": "swing_hit" } }
      },
      "globals": { "footstep": { "list": [ { "assoc": [ "floor", { "list": [ { "list": [
          { "resource": "sound/effects/footstep/floor1.ogg" },
          { "resource": "sound/effects/footstep/floor2.ogg" } ] }, 75, -1 ] } ] } ] } }
    }"##;

    #[test]
    fn sfx_keys_and_referenced_files_come_from_the_dump() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let sfx = resolve_sfx(&types);
        assert_eq!(
            sfx.sfx["punch"],
            vec![("sound/p1.ogg".into(), 1), ("sound/p2.ogg".into(), 1)]
        );
        assert_eq!(sfx.sfx["cat_meow"][1], ("sound/m2.ogg".into(), 23));
        assert_eq!(sound_var(&types, "/obj/item/gun", "hitsound"), "swing_hit");
        let files = referenced(&types, &sfx, ["/obj/item/gun"]);
        assert!(files.contains("sound/weapons/shot.ogg"));
        assert!(files.contains("sound/m1.ogg"));
        assert!(!files.contains("swing_hit"));
        // `GLOB.footstep[FOOTSTEP_FLOOR]`: its files are copied too.
        let floor = &sfx.footsteps["footstep"]["floor"];
        assert_eq!(
            (floor.volume, floor.range, floor.files.len()),
            (75.0, -1.0, 2)
        );
        assert!(files.contains("sound/effects/footstep/floor2.ogg"));
        assert_eq!(SfxData::read_str(&sfx.to_ron()), sfx);
    }

    impl SfxData {
        fn read_str(text: &str) -> Self {
            ron::from_str(text).unwrap()
        }
    }

    #[test]
    fn copied_files_bring_their_attributions() {
        let dir = std::env::temp_dir().join(format!("ss13-sound-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (source, assets) = (dir.join("src"), dir.join("assets"));
        std::fs::create_dir_all(source.join("sound/items/weapons")).unwrap();
        std::fs::write(source.join("sound/attributions.txt"), "root").unwrap();
        std::fs::write(source.join("sound/items/license.txt"), "items").unwrap();
        std::fs::write(source.join("sound/items/weapons/Shot.ogg"), "ogg").unwrap();
        let files = BTreeSet::from(["sound/items/weapons/shot.ogg".to_owned()]);
        let report = copy_sounds(&source, &assets, &files, "abc").unwrap();
        assert_eq!((report.copied, report.attributions), (1, 2));
        let out = assets.join(SOUND_DIR);
        // Case-insensitive like BYOND, stored at the referenced path.
        assert!(out.join("sound/items/weapons/shot.ogg").is_file());
        assert!(out.join("sound/attributions.txt").is_file());
        assert!(out.join("sound/items/license.txt").is_file());
        let sources = std::fs::read_to_string(out.join("SOURCES.txt")).unwrap();
        assert!(sources.contains("commit abc"));
        assert!(sources.contains("sound/items/weapons/shot.ogg\tsound/items/weapons/Shot.ogg"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
