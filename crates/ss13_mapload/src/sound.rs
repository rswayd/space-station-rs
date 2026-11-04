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
    /// Atmos: the analyzer's barometer (`pop`, `ping`'s click), a stationary tank over its
    /// limit (`breaking_sounds`), a canister breaking (`canister_break`).
    pub const ATMOS: [&str; 9] = [
        "sound/effects/pop.ogg",
        "sound/machines/click.ogg",
        "sound/effects/structure_stress/pop1.ogg",
        "sound/effects/structure_stress/pop2.ogg",
        "sound/effects/structure_stress/pop3.ogg",
        "sound/effects/spray.ogg",
        // `/datum/looping_sound/vent_pump_overclock`.
        "sound/machines/fan/fan_start.ogg",
        "sound/machines/fan/fan_loop.ogg",
        "sound/machines/fan/fan_stop.ogg",
    ];
    /// Wall mounts (`ss13_game::wallmount`): `/datum/looping_sound/firealarm` `mid_sounds`, the
    /// light switch's `sound_on`/`sound_off` and BandaStation's click
    /// (`modular_bandastation/aesthetics/lightswitch`), the mounted flasher's flash, the fire
    /// axe cabinet's `toggle_lock`, a firelock's `bash_sound`.
    pub const WALLMOUNT: [&str; 10] = [
        "sound/machines/fire_alarm/fire_alarm1.ogg",
        "sound/machines/fire_alarm/fire_alarm2.ogg",
        "sound/machines/fire_alarm/fire_alarm3.ogg",
        "sound/machines/fire_alarm/fire_alarm4.ogg",
        "sound/items/weapons/magin.ogg",
        "sound/items/weapons/magout.ogg",
        "modular_bandastation/aesthetics/lightswitch/sound/lightswitch.ogg",
        "sound/items/weapons/flash.ogg",
        "sound/machines/locktoggle.ogg",
        "sound/effects/glass/glassbash.ogg",
    ];
    /// Security levels (`ss13_game::security_level`): the levels' `sound`s, `minor_announce`'s
    /// notices, BandaStation's Epsilon intro and law notice, the airlocks lifting their access,
    /// the communications console's swipe prompts.
    pub const SECURITY_LEVELS: [&str; 11] = [
        "sound/announcer/notice/notice1.ogg",
        "sound/announcer/notice/notice2.ogg",
        "sound/announcer/notice/notice3.ogg",
        "sound/announcer/alarm/airraid.ogg",
        "modular_bandastation/security_levels/sound/gamma.ogg",
        "modular_bandastation/security_levels/sound/epsilon.ogg",
        "modular_bandastation/aesthetics_sounds/sound/powerloss.ogg",
        "modular_bandastation/aesthetics_sounds/sound/epsilon_laws.ogg",
        "sound/machines/airlock/boltsup.ogg",
        "sound/machines/terminal/terminal_prompt_deny.ogg",
        "sound/machines/terminal/terminal_prompt_confirm.ogg",
    ];
    /// The communications console (`ss13_game::comms_server`): its terminal sounds and
    /// `make_announcement`'s captain and Syndicate sounds (the announcers' are in `sfx.ron`), the
    /// admins' request notification (`ss13_admin`).
    pub const COMMS: [&str; 7] = [
        "sound/machines/terminal/terminal_prompt.ogg",
        "sound/machines/terminal/terminal_alert.ogg",
        "sound/machines/terminal/terminal_on.ogg",
        "sound/machines/terminal/terminal_off.ogg",
        "sound/announcer/announcement/announce.ogg",
        "sound/announcer/announcement/announce_syndi.ogg",
        "sound/misc/server-ready.ogg",
    ];
    /// Vending machines (`ss13_game::vending`): `dispense`, the payment component's register,
    /// `fall_and_crush`.
    pub const VENDING: [&str; 5] = [
        "sound/machines/machine_vend.ogg",
        "sound/effects/cashregister.ogg",
        "sound/effects/bang.ogg",
        "sound/effects/splat.ogg",
        "sound/effects/blob/blobattack.ogg",
    ];
    /// Station bots (`ss13_game::bots`): the voiced lines of the kinds (`generate_speak_list`),
    /// the MULEbot's `buzz`, the stun, cuff and honk sounds of the secbots and honkbots, the
    /// ED-209's shot, the dedbot's blades, the cleanbot's acid spray.
    pub const BOTS: [&str; 62] = [
        "sound/machines/buzz/buzz-sigh.ogg",
        "sound/machines/buzz/buzz-two.ogg",
        "sound/machines/ping.ogg",
        "sound/machines/chime.ogg",
        "sound/items/weapons/egloves.ogg",
        "sound/items/weapons/cablecuff.ogg",
        "sound/items/bikehorn.ogg",
        "sound/misc/sadtrombone.ogg",
        "sound/items/airhorn/airhorn.ogg",
        "sound/mobs/non-humanoids/honkbot/honkbot_evil_laugh.ogg",
        "sound/items/weapons/laser.ogg",
        "sound/items/weapons/bladeslice.ogg",
        "sound/items/weapons/drill.ogg",
        "sound/items/weapons/blade1.ogg",
        "sound/effects/spray2.ogg",
        "sound/mobs/non-humanoids/beepsky/criminal.ogg",
        "sound/mobs/non-humanoids/beepsky/freeze.ogg",
        "sound/mobs/non-humanoids/beepsky/justice.ogg",
        "sound/mobs/non-humanoids/beepsky/creep.ogg",
        "sound/mobs/non-humanoids/beepsky/iamthelaw.ogg",
        "sound/mobs/non-humanoids/beepsky/secureday.ogg",
        "sound/mobs/non-humanoids/medbot/coming.ogg",
        "sound/mobs/non-humanoids/medbot/help.ogg",
        "sound/mobs/non-humanoids/medbot/injured.ogg",
        "sound/mobs/non-humanoids/medbot/patchedup.ogg",
        "sound/mobs/non-humanoids/medbot/apple.ogg",
        "sound/mobs/non-humanoids/medbot/feelbetter.ogg",
        "sound/mobs/non-humanoids/medbot/no.ogg",
        "sound/mobs/non-humanoids/medbot/live.ogg",
        "sound/mobs/non-humanoids/medbot/lost.ogg",
        "sound/mobs/non-humanoids/medbot/delicious.ogg",
        "sound/mobs/non-humanoids/medbot/surgeon.ogg",
        "sound/mobs/non-humanoids/medbot/radar.ogg",
        "sound/mobs/non-humanoids/medbot/catch.ogg",
        "sound/mobs/non-humanoids/medbot/flies.ogg",
        "sound/mobs/non-humanoids/medbot/why.ogg",
        "sound/mobs/non-humanoids/medbot/fuck_you.ogg",
        "sound/mobs/non-humanoids/medbot/turn_off.ogg",
        "sound/mobs/non-humanoids/medbot/im_different.ogg",
        "sound/mobs/non-humanoids/medbot/close.ogg",
        "sound/mobs/non-humanoids/medbot/shindemashou.ogg",
        "sound/mobs/non-humanoids/firebot/nofires.ogg",
        "sound/mobs/non-humanoids/firebot/onlyyou.ogg",
        "sound/mobs/non-humanoids/firebot/tempnominal.ogg",
        "sound/mobs/non-humanoids/firebot/keepitcool.ogg",
        "sound/mobs/non-humanoids/firebot/detected.ogg",
        "sound/mobs/non-humanoids/firebot/stopdropnroll.ogg",
        "sound/mobs/non-humanoids/firebot/extinguishing.ogg",
        "sound/mobs/non-humanoids/firebot/candle_tip.ogg",
        "sound/mobs/non-humanoids/firebot/electric_fire_tip.ogg",
        "sound/mobs/non-humanoids/firebot/gasoline_tip.ogg",
        "sound/mobs/non-humanoids/hygienebot/unhygienicclient.ogg",
        "sound/mobs/non-humanoids/hygienebot/dragyouout.ogg",
        "sound/mobs/non-humanoids/hygienebot/foulsmelling.ogg",
        "sound/mobs/non-humanoids/hygienebot/troglodyte.ogg",
        "sound/mobs/non-humanoids/hygienebot/greencloud.ogg",
        "sound/mobs/non-humanoids/hygienebot/letmeclean.ogg",
        "sound/mobs/non-humanoids/hygienebot/cutarteries.ogg",
        "sound/mobs/non-humanoids/hygienebot/stoprunning.ogg",
        "sound/mobs/non-humanoids/hygienebot/finally.ogg",
        "sound/mobs/non-humanoids/hygienebot/thankgod.ogg",
        "sound/mobs/non-humanoids/hygienebot/degenerate.ogg",
    ];
    /// `SSexplosions.shake_the_room`: the far and distant explosion sounds (the near one, the
    /// creaking and the hull creaking are SFX keys).
    pub const EXPLOSION: [&str; 2] = [
        "sound/effects/explosion/explosionfar.ogg",
        "sound/effects/explosion/explosion_distant.ogg",
    ];
    /// `/datum/looping_sound/fire` `mid_sounds` (equal weights).
    pub const FIRE_LOOP: [&str; 7] = [
        "sound/effects/fireclip1.ogg",
        "sound/effects/fireclip2.ogg",
        "sound/effects/fireclip3.ogg",
        "sound/effects/fireclip4.ogg",
        "sound/effects/fireclip5.ogg",
        "sound/effects/fireclip6.ogg",
        "sound/effects/fireclip7.ogg",
    ];
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
    /// `/datum/centcom_announcer` subtype -> its sounds.
    #[serde(default)]
    pub announcers: BTreeMap<String, AnnouncerSounds>,
    /// tg `GLOB.vox_sounds`: the AI's VOX words ([`resolve_vox`]).
    #[serde(default)]
    pub vox: BTreeMap<String, VoxWord>,
}

/// One VOX word: its file and how long it plays (the client queues the words, tg `wait = 1`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoxWord {
    pub file: String,
    pub secs: f32,
}

/// tg `GLOB.vox_sounds`' directory (`vox_sounds.dm`: every word is its file's name).
pub const VOX_DIR: &str = "sound/announcer/vox_fem";

/// The VOX words: every `.ogg` of [`VOX_DIR`] under `source`, by lowercase name, with its
/// length. Empty without the directory.
pub fn resolve_vox(source: &Path) -> BTreeMap<String, VoxWord> {
    let Ok(dir) = std::fs::read_dir(source.join(VOX_DIR)) else {
        return BTreeMap::new();
    };
    dir.filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let word = name.strip_suffix(".ogg")?.to_lowercase();
            let bytes = std::fs::read(e.path()).ok()?;
            let secs = ogg_duration(&bytes)? as f32;
            Some((
                word,
                VoxWord {
                    file: format!("{VOX_DIR}/{name}"),
                    secs,
                },
            ))
        })
        .collect()
}

/// The length of an Ogg Vorbis file: the last page's granule position (samples) over the
/// identification header's sample rate.
pub fn ogg_duration(bytes: &[u8]) -> Option<f64> {
    const CAPTURE: &[u8] = b"OggS";
    if !bytes.starts_with(CAPTURE) {
        return None;
    }
    // First page: 27-byte header, the segment table, then the `\x01vorbis` packet.
    let segments = usize::from(*bytes.get(26)?);
    let packet = bytes.get(27 + segments..)?;
    if packet.get(..7)? != b"\x01vorbis" {
        return None;
    }
    let rate = u32::from_le_bytes(packet.get(12..16)?.try_into().ok()?);
    let last = bytes.windows(4).rposition(|w| w == CAPTURE)?;
    let granule = u64::from_le_bytes(bytes.get(last + 6..last + 14)?.try_into().ok()?);
    (rate > 0).then(|| granule as f64 / f64::from(rate))
}

/// A `/datum/centcom_announcer`: the sound sets of announcements.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnouncerSounds {
    /// `welcome_sounds`: the round start.
    pub welcome: Vec<String>,
    /// `alert_sounds`: a priority announcement without its own sound.
    pub alert: Vec<String>,
    /// `command_report_sounds`.
    pub command_report: Vec<String>,
    /// `event_sounds`: `ANNOUNCER_*` key -> file.
    pub events: BTreeMap<String, String>,
    /// `custom_alert_message`, its markup stripped: replaces the text of announcements that are
    /// not `has_important_message`.
    pub custom_alert_message: Option<String>,
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
        let announcers = self.announcers.values().flat_map(|a| {
            (a.welcome.iter().chain(&a.alert).chain(&a.command_report))
                .chain(a.events.values())
                .map(String::as_str)
        });
        (self.sfx.values().flatten().map(|(f, _)| f.as_str()))
            .chain(steps.flat_map(|s| s.files.iter().map(String::as_str)))
            .chain(announcers)
            .chain(self.vox.values().map(|w| w.file.as_str()))
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
        announcers: resolve_announcers(types),
        vox: BTreeMap::new(),
    }
}

const ANNOUNCER_ROOT: &str = "/datum/centcom_announcer";

/// Every `/datum/centcom_announcer` subtype with its sound lists.
fn resolve_announcers(types: &DmTypes) -> BTreeMap<String, AnnouncerSounds> {
    let files = |path: &str, var: &str| -> Vec<String> {
        types
            .resolved(path, var)
            .map(list)
            .unwrap_or_default()
            .iter()
            .filter_map(|f| resource(f).map(str::to_owned))
            .collect()
    };
    types
        .types
        .keys()
        .filter(|p| is_under(p, ANNOUNCER_ROOT) && p.as_str() != ANNOUNCER_ROOT)
        .map(|path| {
            let events = types
                .resolved(path, "event_sounds")
                .map(list)
                .unwrap_or_default()
                .iter()
                .filter_map(|item| match item.get("assoc")?.as_array()?.as_slice() {
                    [key, file] => Some((key.as_str()?.to_owned(), resource(file)?.to_owned())),
                    _ => None,
                })
                .collect();
            let custom_alert_message = types
                .resolved(path, "custom_alert_message")
                .and_then(Value::as_str)
                .map(strip_markup)
                .filter(|m| !m.is_empty());
            let sounds = AnnouncerSounds {
                welcome: files(path, "welcome_sounds"),
                alert: files(path, "alert_sounds"),
                command_report: files(path, "command_report_sounds"),
                events,
                custom_alert_message,
            };
            (path.clone(), sounds)
        })
        .collect()
}

/// Chat is not HTML: `<br>` becomes a line break, other tags go.
fn strip_markup(html: &str) -> String {
    let html = html.replace("<br>", "\n");
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_owned()
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

/// The files the server can name: SFX files, [`SOUND_VARS`] of `converted`, [`RUNTIME_SOUNDS`],
/// the fire loop and the other `files` lists.
pub fn referenced<'a>(
    types: &DmTypes,
    sfx: &SfxData,
    converted: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = sfx.files().map(str::to_owned).collect();
    out.extend(RUNTIME_SOUNDS.map(str::to_owned));
    out.extend(files::FIRE_LOOP.map(str::to_owned));
    out.extend(files::ATMOS.map(str::to_owned));
    out.extend(files::WALLMOUNT.map(str::to_owned));
    out.extend(files::SECURITY_LEVELS.map(str::to_owned));
    out.extend(files::VENDING.map(str::to_owned));
    out.extend(files::COMMS.map(str::to_owned));
    out.extend(files::BOTS.map(str::to_owned));
    out.extend(files::EXPLOSION.map(str::to_owned));
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
        "/datum/centcom_announcer": { "parent": "/datum", "declared_vars": {
            "welcome_sounds": { "list": [] }, "alert_sounds": { "list": [] },
            "command_report_sounds": { "list": [] }, "event_sounds": { "list": [] },
            "custom_alert_message": null } },
        "/datum/centcom_announcer/intern": { "parent": "/datum/centcom_announcer",
            "declared_vars": { "alert_sounds": { "list": [ { "resource": "sound/a1.ogg" } ] },
            "event_sounds": { "list": [ { "assoc": [ "announcer_meteors",
            { "resource": "sound/meteors.ogg" } ] } ] },
            "custom_alert_message": "<span class='alert'>Wait for the intern.<br></span>" } },
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
        // `/datum/centcom_announcer/intern`: its lists, the message without markup, its files.
        assert_eq!(sfx.announcers.len(), 1);
        let intern = &sfx.announcers["/datum/centcom_announcer/intern"];
        assert_eq!(intern.alert, ["sound/a1.ogg"]);
        assert_eq!(intern.events["announcer_meteors"], "sound/meteors.ogg");
        assert_eq!(
            intern.custom_alert_message.as_deref(),
            Some("Wait for the intern.")
        );
        assert!(files.contains("sound/meteors.ogg"));
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

    /// A first page with the Vorbis identification header (22050 Hz) and a last page at
    /// 44100 samples: two seconds.
    fn ogg(rate: u32, granule: u64) -> Vec<u8> {
        let page = |granule: u64, packet: &[u8]| {
            let mut p = b"OggS".to_vec();
            p.extend([0, 0]);
            p.extend(granule.to_le_bytes());
            p.extend([0; 12]);
            p.push(1);
            p.push(packet.len() as u8);
            p.extend(packet);
            p
        };
        let mut id = b"\x01vorbis".to_vec();
        id.extend(0u32.to_le_bytes());
        id.push(1);
        id.extend(rate.to_le_bytes());
        id.extend([0; 13]);
        let mut out = page(0, &id);
        out.extend(page(granule / 2, b"audio"));
        out.extend(page(granule, b"audio"));
        out
    }

    #[test]
    fn ogg_duration_is_the_last_granule_over_the_rate() {
        assert_eq!(ogg_duration(&ogg(22050, 44100)), Some(2.0));
        assert_eq!(ogg_duration(b"RIFF...."), None);
        assert_eq!(ogg_duration(&ogg(0, 44100)), None);
        // A real VOX word, when the source checkout is there.
        let source = std::env::var_os("SS13_SOURCE")
            .map_or_else(|| PathBuf::from(env!("SS13_SOURCE")), PathBuf::from);
        let vox = resolve_vox(&source);
        if let Some(word) = vox.get("alert") {
            assert_eq!(word.file, "sound/announcer/vox_fem/alert.ogg");
            assert!((0.2..3.0).contains(&word.secs), "{}", word.secs);
            assert!(vox.contains_key(",") && vox.len() > 1000);
        }
    }
}
