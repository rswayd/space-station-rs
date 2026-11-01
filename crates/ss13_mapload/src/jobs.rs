//! Station jobs as data: the `/datum/job` subtypes a new player can join as, with their outfit
//! (`/datum/outfit/job/*`) and ID trim (`/datum/id_trim/job/*`), resolved from `dm_types.json`
//! the way tg builds them at round start (`SSjob.setup_occupations`, `/datum/outfit/job/pre_equip`,
//! `/datum/id_trim/job/refresh_trim_access`). `ss13-mapload convert` writes it to [`JOBS_FILE`]
//! in the assets dir; the server reads it with [`JobsData::read`].
//!
//! Resolution assumes tg's defaults where a round would read preferences or config:
//! - backpack preference `DBACKPACK` (department backpack: `back = backpack`), jumpsuit `PREF_SUIT`;
//! - `JOBS_HAVE_MINIMAL_ACCESS` off: card access is `minimal | extra` (plus the wildcard lists,
//!   which a card adds to its access); the config-gated additions of `refresh_trim_access`
//!   overrides (`ASSISTANTS_HAVE_MAINT_ACCESS`, `SECURITY_HAS_MAINT_ACCESS`, ...) are off by
//!   default and not applied;
//! - no `LOAD_JOBS_FROM_TXT` and no map `job_changes`: positions are the code defaults, except
//!   the overflow role (`/datum/job/assistant`), which `SSjob.set_overflow_role` makes unlimited.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::is_under;

pub const JOB_ROOT: &str = "/datum/job";
/// File name under the assets dir.
pub const JOBS_FILE: &str = "jobs.ron";
/// `SSjob.overflow_role` default.
pub const OVERFLOW_JOB: &str = "/datum/job/assistant";
/// Where `SSjob` files jobs without `departments_list`.
const UNDEFINED_DEPARTMENT: &str = "/datum/job_department/undefined";
/// `JOB_NEW_PLAYER_JOINABLE`.
const JOB_NEW_PLAYER_JOINABLE: i64 = 1 << 4;
const JOB_ANTAG_BLACKLISTED: i64 = 1 << 13;
const JOB_ANTAG_PROTECTED: i64 = 1 << 14;
/// `DEPARTMENT_BITFLAG_COMMAND`.
const DEPARTMENT_BITFLAG_COMMAND: i64 = 1 << 1;
/// `strings/names/` files of `GLOB.first_names_male`, `first_names_female`, `last_names`.
pub const NAME_FILES: [&str; 3] = ["first_male.txt", "first_female.txt", "last.txt"];

/// `/datum/outfit` item vars, in tg `equip` order.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OutfitSlot {
    Uniform,
    Suit,
    Belt,
    Gloves,
    Shoes,
    Head,
    Mask,
    Neck,
    Ears,
    Glasses,
    Id,
    SuitStore,
    Back,
    LHand,
    RHand,
    LPocket,
    RPocket,
}

impl OutfitSlot {
    pub const ALL: [Self; 17] = [
        Self::Uniform,
        Self::Suit,
        Self::Belt,
        Self::Gloves,
        Self::Shoes,
        Self::Head,
        Self::Mask,
        Self::Neck,
        Self::Ears,
        Self::Glasses,
        Self::Id,
        Self::SuitStore,
        Self::Back,
        Self::LHand,
        Self::RHand,
        Self::LPocket,
        Self::RPocket,
    ];

    /// The `/datum/outfit` var.
    pub fn var(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::Suit => "suit",
            Self::Belt => "belt",
            Self::Gloves => "gloves",
            Self::Shoes => "shoes",
            Self::Head => "head",
            Self::Mask => "mask",
            Self::Neck => "neck",
            Self::Ears => "ears",
            Self::Glasses => "glasses",
            Self::Id => "id",
            Self::SuitStore => "suit_store",
            Self::Back => "back",
            Self::LHand => "l_hand",
            Self::RHand => "r_hand",
            Self::LPocket => "l_pocket",
            Self::RPocket => "r_pocket",
        }
    }
}

/// A resolved `/datum/outfit` with the default preferences.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Outfit {
    pub path: String,
    /// Item type per filled slot, in [`OutfitSlot::ALL`] order.
    pub items: Vec<(OutfitSlot, String)>,
    /// `box` first, then `backpack_contents` (type, count).
    pub backpack_contents: Vec<(String, u32)>,
}

impl Outfit {
    pub fn item(&self, slot: OutfitSlot) -> Option<&str> {
        self.items
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, p)| p.as_str())
    }
}

/// A `/datum/job_department` with joinable jobs, in `display_order`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Department {
    pub path: String,
    /// `department_name` (English, as tgui receives it).
    pub name: String,
    /// `ui_color`, `#rrggbb`.
    pub color: String,
    /// Indexes into [`JobsData::jobs`] the latejoin menu lists here, in job `display_order`.
    pub jobs: Vec<u16>,
}

/// A joinable `/datum/job`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Job {
    pub path: String,
    /// `title`: the job's name everywhere (landmark names, card assignment lookups, tgui keys).
    pub title: String,
    pub description: String,
    /// In the command department (`DEPARTMENT_BITFLAG_COMMAND`): shown bold.
    pub command: bool,
    /// `-1`: unlimited.
    pub total_positions: i32,
    pub spawn_positions: i32,
    pub outfit: Option<Outfit>,
    /// The outfit's `id_trim`.
    pub trim: Option<String>,
    /// The trim's `assignment` (the job name on the card), else the title.
    pub assignment: String,
    /// The card's access: trim `minimal | extra`, wildcard lists, `New()` flag groups.
    pub access: Vec<String>,
    /// Why this job cannot be played as tg does it here (empty: fully resolved).
    pub unresolved: Vec<String>,
    /// BandaStation `staffing_priority` (1 critical .. 6 negligible): round start fills jobs in
    /// this order (`cmp_job_staffing_priority`).
    #[serde(default = "negligible")]
    pub staffing_priority: u8,
    /// `JOB_ANTAG_PROTECTED`: no antagonist rolls while `PROTECT_ROLES_FROM_ANTAGONIST` is on.
    #[serde(default)]
    pub antag_protected: bool,
    /// `JOB_ANTAG_BLACKLISTED`: never an antagonist.
    #[serde(default)]
    pub antag_blacklisted: bool,
    /// `paycheck`: credits per payday (`PAYCHECK_*`).
    #[serde(default)]
    pub paycheck: u32,
    /// `paycheck_department`: the budget paying it (`ACCOUNT_CIV`, ...); empty: no bank account.
    #[serde(default)]
    pub paycheck_department: String,
    /// The trim's `wildcard_access` (`minimal | extra` wildcard lists and their `New()` flag
    /// groups): the part of [`Self::access`] a card holds in wildcard slots.
    #[serde(default)]
    pub wildcard_access: Vec<String>,
    /// The trim's `template_access`: the accesses whose holders may apply this job as an ID
    /// console template (`SSid_access.sub_department_managers_tgui`); empty: no template.
    #[serde(default)]
    pub template_access: Vec<String>,
    /// The trim's `sechud_icon_state` (`icons/mob/huds/hud.dmi`).
    #[serde(default)]
    pub sechud_icon_state: String,
}

/// `STAFFING_PRIORITY_NEGLIGIBLE`, the `/datum/job` default.
pub const STAFFING_PRIORITY_NEGLIGIBLE: u8 = 6;

fn negligible() -> u8 {
    STAFFING_PRIORITY_NEGLIGIBLE
}

/// Name lists for random character names (`strings/names/`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Names {
    pub first_male: Vec<String>,
    pub first_female: Vec<String>,
    pub last: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct JobsData {
    pub departments: Vec<Department>,
    pub jobs: Vec<Job>,
    pub names: Names,
}

impl JobsData {
    pub fn job(&self, title: &str) -> Option<usize> {
        self.jobs.iter().position(|j| j.title == title)
    }

    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let text = ss13_data::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        ron::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("jobs data serializes"),
        );
        out.push('\n');
        out
    }

    /// Item types the outfits put into slots: the converter emits prototypes for them.
    /// Every item an outfit spawns: its slots, `box` and `backpack_contents`.
    pub fn outfit_items(&self) -> BTreeSet<&str> {
        self.jobs
            .iter()
            .filter_map(|j| j.outfit.as_ref())
            .flat_map(|o| {
                let slots = o.items.iter().map(|(_, p)| p.as_str());
                slots.chain(o.backpack_contents.iter().map(|(p, _)| p.as_str()))
            })
            .collect()
    }
}

fn path_value(v: Option<&Value>) -> Option<String> {
    v?.get("path")?.as_str().map(str::to_owned)
}

fn text_value(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn int_value(v: Option<&Value>) -> Option<i64> {
    match v {
        None | Some(Value::Null) => Some(0),
        Some(v) => v.as_f64().map(|f| f as i64),
    }
}

/// The items of a dumped list: plain entries, or keys of `assoc` entries with their values.
fn list_items(v: Option<&Value>) -> Vec<(&Value, Option<&Value>)> {
    v.and_then(|v| v.get("list"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| match item.get("assoc").and_then(Value::as_array) {
                    Some(pair) if pair.len() == 2 => (&pair[0], Some(&pair[1])),
                    _ => (item, None),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn string_list(types: &DmTypes, path: &str, var: &str) -> Vec<String> {
    list_items(types.resolved(path, var))
        .into_iter()
        .filter_map(|(k, _)| k.as_str().map(str::to_owned))
        .collect()
}

/// `outfit` with `pre_equip` defaults (see the module docs).
pub fn outfit(types: &DmTypes, path: &str) -> Outfit {
    let slot = |var: &str| path_value(types.resolved(path, var));
    let mut items = Vec::new();
    for s in OutfitSlot::ALL {
        let mut item = slot(s.var());
        // `/datum/outfit/job/pre_equip`, DBACKPACK: `back = backpack` when `back` is a backpack.
        if s == OutfitSlot::Back
            && item
                .as_deref()
                .is_some_and(|b| is_under(b, "/obj/item/storage/backpack"))
            && let Some(department) = slot("backpack")
        {
            item = Some(department);
        }
        if let Some(item) = item {
            items.push((s, item));
        }
    }
    let mut backpack_contents: Vec<(String, u32)> =
        slot("box").map(|b| (b, 1)).into_iter().collect();
    for (key, count) in list_items(types.resolved(path, "backpack_contents")) {
        if let Some(item) = path_value(Some(key)) {
            let n = count.and_then(Value::as_f64).unwrap_or(1.0).max(1.0) as u32;
            backpack_contents.push((item, n));
        }
    }
    Outfit {
        path: path.to_owned(),
        items,
        backpack_contents,
    }
}

/// The card access of `trim` (see the module docs) and what could not be read.
pub fn trim_access(
    types: &DmTypes,
    trim: &str,
    flag_groups: &FlagGroups,
) -> (Vec<String>, Vec<String>) {
    let mut access: Vec<String> = Vec::new();
    let mut add = |a: String| {
        if !access.contains(&a) {
            access.push(a);
        }
    };
    for var in [
        "minimal_access",
        "extra_access",
        "minimal_wildcard_access",
        "extra_wildcard_access",
    ] {
        string_list(types, trim, var).into_iter().for_each(&mut add);
    }
    let mut unresolved = Vec::new();
    // Each trim in the chain runs its own `New()` (they all call the parent).
    for ty in types.chain(trim).collect::<Vec<_>>().into_iter().rev() {
        let t = &types.types[ty];
        for flags in t.new_access_flags.values() {
            for flag in flags {
                match flag_groups.get(flag) {
                    Some(group) => group.iter().cloned().for_each(&mut add),
                    None => unresolved.push(format!("{ty}: access flag {flag} has no group")),
                }
            }
        }
        // The roots' `New()` handle map `job_changes`, not read here.
        let own_new = t.procs.iter().any(|p| p == "New");
        if own_new
            && t.new_access_flags.is_empty()
            && is_under(ty, "/datum/id_trim/job")
            && ty != "/datum/id_trim/job"
        {
            unresolved.push(format!("{ty}: New() not read"));
        }
    }
    (access, unresolved)
}

/// The trim's `wildcard_access`: `minimal_wildcard_access | extra_wildcard_access` plus the flag
/// groups its `New()` adds to those two lists.
pub fn trim_wildcard_access(types: &DmTypes, trim: &str, flag_groups: &FlagGroups) -> Vec<String> {
    let mut access: Vec<String> = Vec::new();
    let mut add = |a: String| {
        if !access.contains(&a) {
            access.push(a);
        }
    };
    for var in ["minimal_wildcard_access", "extra_wildcard_access"] {
        string_list(types, trim, var).into_iter().for_each(&mut add);
    }
    for ty in types.chain(trim).collect::<Vec<_>>().into_iter().rev() {
        for (var, flags) in &types.types[ty].new_access_flags {
            if !var.ends_with("wildcard_access") {
                continue;
            }
            for group in flags.iter().filter_map(|f| flag_groups.get(f)) {
                group.iter().cloned().for_each(&mut add);
            }
        }
    }
    access
}

/// `SSid_access.accesses_by_flag` from the dump.
pub type FlagGroups = std::collections::HashMap<i64, Vec<String>>;

/// Every joinable job and its departments.
pub fn resolve(types: &DmTypes) -> JobsData {
    let flag_groups: FlagGroups = types
        .access_flags
        .iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, v.clone())))
        .collect();
    let mut paths: Vec<&str> = types
        .types
        .keys()
        .map(String::as_str)
        .filter(|p| is_under(p, JOB_ROOT) && *p != JOB_ROOT)
        .filter(|p| path_value(types.resolved(p, "abstract_type")).as_deref() != Some(*p))
        .filter(|p| {
            int_value(types.resolved(p, "job_flags")).unwrap_or(0) & JOB_NEW_PLAYER_JOINABLE != 0
        })
        .collect();
    paths.sort_unstable();

    let mut jobs = Vec::new();
    // (department path, job index), in job order.
    let mut memberships: Vec<(String, usize)> = Vec::new();
    for path in paths {
        let mut departments: Vec<String> = list_items(types.resolved(path, "departments_list"))
            .into_iter()
            .filter_map(|(k, _)| path_value(Some(k)))
            .collect();
        if departments.is_empty() {
            departments.push(UNDEFINED_DEPARTMENT.to_owned());
        }
        let bitflags = departments.iter().fold(0, |acc, d| {
            acc | int_value(types.resolved(d, "department_bitflags")).unwrap_or(0)
        });
        let command = bitflags & DEPARTMENT_BITFLAG_COMMAND != 0;
        let index = jobs.len();
        // `latejoin_menu`: a job of several departments shows in its first one, and in all of
        // them when it is a command job.
        for (i, d) in departments.iter().enumerate() {
            if i == 0 || command {
                memberships.push((d.clone(), index));
            }
        }
        let title = text_value(types.resolved(path, "title"));
        let job_flags = int_value(types.resolved(path, "job_flags")).unwrap_or(0);
        let unlimited = path == OVERFLOW_JOB;
        let positions = |var| {
            if unlimited {
                -1
            } else {
                int_value(types.resolved(path, var)).unwrap_or(0) as i32
            }
        };
        let mut unresolved = Vec::new();
        let outfit = path_value(types.resolved(path, "outfit"))
            .filter(|o| types.types.contains_key(o))
            .map(|o| outfit(types, &o));
        if outfit.is_none() {
            unresolved.push("no outfit (not a human job)".to_owned());
        }
        let trim = outfit
            .as_ref()
            .and_then(|o| path_value(types.resolved(&o.path, "id_trim")))
            .filter(|t| types.types.contains_key(t));
        let (access, assignment) = match &trim {
            Some(trim) => {
                let (access, mut missing) = trim_access(types, trim, &flag_groups);
                unresolved.append(&mut missing);
                let assignment = text_value(types.resolved(trim, "assignment"));
                (access, assignment)
            }
            None => {
                if outfit.is_some() {
                    unresolved.push("outfit has no id_trim".to_owned());
                }
                (Vec::new(), String::new())
            }
        };
        let wildcard_access = trim
            .as_deref()
            .map(|t| trim_wildcard_access(types, t, &flag_groups))
            .unwrap_or_default();
        let template_access = trim
            .as_deref()
            .map(|t| string_list(types, t, "template_access"))
            .unwrap_or_default();
        let sechud_icon_state = trim
            .as_deref()
            .map(|t| text_value(types.resolved(t, "sechud_icon_state")))
            .unwrap_or_default();
        if let Some(o) = &outfit {
            for (slot, item) in &o.items {
                if !types.types.contains_key(item) {
                    unresolved.push(format!("{} {item} is not a DM type", slot.var()));
                }
            }
        }
        jobs.push(Job {
            path: path.to_owned(),
            assignment: if assignment.is_empty() {
                title.clone()
            } else {
                assignment
            },
            title,
            description: text_value(types.resolved(path, "description")),
            command,
            total_positions: positions("total_positions"),
            spawn_positions: positions("spawn_positions"),
            outfit,
            trim,
            access,
            unresolved,
            staffing_priority: types
                .resolved(path, "staffing_priority")
                .and_then(Value::as_f64)
                .map(|p| p as u8)
                .unwrap_or(STAFFING_PRIORITY_NEGLIGIBLE),
            antag_protected: job_flags & JOB_ANTAG_PROTECTED != 0,
            antag_blacklisted: job_flags & JOB_ANTAG_BLACKLISTED != 0,
            paycheck: int_value(types.resolved(path, "paycheck"))
                .unwrap_or(0)
                .max(0) as u32,
            paycheck_department: text_value(types.resolved(path, "paycheck_department")),
            wildcard_access,
            template_access,
            sechud_icon_state,
        });
    }

    let order = |path: &str, var| {
        types
            .resolved(path, var)
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let mut department_paths: Vec<String> = memberships.iter().map(|(d, _)| d.clone()).collect();
    department_paths.sort_unstable();
    department_paths.dedup();
    department_paths
        .sort_by(|a, b| order(a, "display_order").total_cmp(&order(b, "display_order")));
    let departments = department_paths
        .into_iter()
        .map(|path| {
            let mut members: Vec<u16> = memberships
                .iter()
                .filter(|(d, _)| *d == path)
                .map(|&(_, j)| j as u16)
                .collect();
            members.sort_by(|&a, &b| {
                let (a, b) = (&jobs[a as usize], &jobs[b as usize]);
                order(&a.path, "display_order").total_cmp(&order(&b.path, "display_order"))
            });
            Department {
                name: text_value(types.resolved(&path, "department_name")),
                color: text_value(types.resolved(&path, "ui_color")),
                path,
                jobs: members,
            }
        })
        .collect();
    JobsData {
        departments,
        jobs,
        names: Names::default(),
    }
}

/// `strings/names/` of the source checkout; a missing file leaves its list empty.
pub fn read_names(source: &Path) -> Names {
    let read = |file: &str| -> Vec<String> {
        std::fs::read_to_string(source.join("strings/names").join(file))
            .map(|text| {
                text.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let [male, female, last] = NAME_FILES.map(read);
    Names {
        first_male: male,
        first_female: female,
        last,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "access_flags": { "1": ["common_a", "common_b"], "2": ["command"] },
      "types": {
        "/datum": { "parent": null },
        "/datum/job_department": { "parent": "/datum", "declared_vars": {
            "department_name": "No Department", "ui_color": "#9689db", "display_order": 0,
            "department_bitflags": 0 } },
        "/datum/job_department/undefined": { "parent": "/datum/job_department",
            "declared_vars": { "display_order": 10 } },
        "/datum/job_department/command": { "parent": "/datum/job_department", "declared_vars": {
            "department_name": "Command", "ui_color": "#6681a5", "display_order": 1,
            "department_bitflags": 2 } },
        "/datum/job_department/security": { "parent": "/datum/job_department", "declared_vars": {
            "department_name": "Security", "ui_color": "#d46a78", "display_order": 2,
            "department_bitflags": 1 } },
        "/datum/job": { "parent": "/datum", "declared_vars": {
            "abstract_type": { "path": "/datum/job" }, "title": "NOPE", "job_flags": 0,
            "total_positions": 0, "spawn_positions": 0, "outfit": null, "display_order": 0,
            "departments_list": null } },
        "/datum/job/assistant": { "parent": "/datum/job", "declared_vars": {
            "title": "Assistant", "job_flags": 479, "total_positions": 5, "spawn_positions": 5,
            "outfit": { "path": "/datum/outfit/job/assistant" }, "display_order": 1 } },
        "/datum/job/security_officer": { "parent": "/datum/job", "declared_vars": {
            "title": "Security Officer", "job_flags": 16863, "total_positions": 5,
            "spawn_positions": 5, "display_order": 5,
            "outfit": { "path": "/datum/outfit/job/security" },
            "departments_list": { "list": [{ "path": "/datum/job_department/security" }] } } },
        "/datum/job/head_of_security": { "parent": "/datum/job", "declared_vars": {
            "title": "Head of Security", "job_flags": 16863, "total_positions": 1,
            "display_order": 4, "outfit": { "path": "/datum/outfit/job/hos" },
            "departments_list": { "list": [{ "path": "/datum/job_department/security" },
                { "path": "/datum/job_department/command" }] } } },
        "/datum/job/ai": { "parent": "/datum/job", "declared_vars": {
            "title": "AI", "job_flags": 16, "total_positions": 1, "display_order": 9,
            "departments_list": { "list": [{ "path": "/datum/job_department/command" }] } } },
        "/datum/job/ghost_role": { "parent": "/datum/job", "declared_vars": {
            "title": "Ghost", "job_flags": 0 } },
        "/datum/outfit": { "parent": "/datum", "declared_vars": {
            "uniform": null, "shoes": null, "back": null, "id": null, "id_trim": null,
            "box": null, "backpack_contents": null } },
        "/datum/outfit/job": { "parent": "/datum/outfit", "declared_vars": {
            "back": { "path": "/obj/item/storage/backpack" },
            "backpack": { "path": "/obj/item/storage/backpack" },
            "box": { "path": "/obj/item/storage/box/survival" },
            "id": { "path": "/obj/item/card/id/advanced" },
            "uniform": { "path": "/obj/item/clothing/under/color/grey" },
            "shoes": { "path": "/obj/item/clothing/shoes/sneakers/black" } } },
        "/datum/outfit/job/assistant": { "parent": "/datum/outfit/job", "declared_vars": {
            "id_trim": { "path": "/datum/id_trim/job/assistant" } } },
        "/datum/outfit/job/security": { "parent": "/datum/outfit/job", "declared_vars": {
            "backpack": { "path": "/obj/item/storage/backpack/security" },
            "backpack_contents": { "list": [
                { "assoc": [{ "path": "/obj/item/choice_beacon" }, 2] }] },
            "id_trim": { "path": "/datum/id_trim/job/security_officer" },
            "uniform": { "path": "/obj/item/clothing/under/rank/security/officer" },
            "shoes": { "path": "/obj/item/clothing/shoes/jackboots/sec" } } },
        "/datum/outfit/job/hos": { "parent": "/datum/outfit/job/security", "declared_vars": {
            "id": { "path": "/obj/item/card/id/advanced/silver" },
            "id_trim": { "path": "/datum/id_trim/job/head_of_security" } } },
        "/datum/id_trim": { "parent": "/datum", "declared_vars": { "assignment": null } },
        "/datum/id_trim/job": { "parent": "/datum/id_trim", "declared_vars": {
            "minimal_access": { "list": [] }, "extra_access": { "list": [] },
            "minimal_wildcard_access": { "list": [] }, "extra_wildcard_access": { "list": [] } },
            "procs": ["New"] },
        "/datum/id_trim/job/assistant": { "parent": "/datum/id_trim/job", "declared_vars": {
            "assignment": "Assistant", "extra_access": { "list": ["maint_tunnels"] } } },
        "/datum/id_trim/job/security_officer": { "parent": "/datum/id_trim/job",
            "declared_vars": { "assignment": "Офицер",
            "minimal_access": { "list": ["brig", "security"] },
            "extra_access": { "list": ["maint_tunnels", "security"] } } },
        "/datum/id_trim/job/head_of_security": { "parent": "/datum/id_trim/job",
            "declared_vars": { "assignment": "Head of Security",
            "minimal_access": { "list": ["hos"] },
            "minimal_wildcard_access": { "list": ["armory"] } },
            "procs": ["New"], "new_access_flags": { "minimal_access": [1, 2] } },
        "/obj": { "parent": "/datum" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/storage": { "parent": "/obj/item" },
        "/obj/item/storage/backpack": { "parent": "/obj/item/storage" },
        "/obj/item/storage/backpack/security": { "parent": "/obj/item/storage/backpack" },
        "/obj/item/storage/box": { "parent": "/obj/item/storage" },
        "/obj/item/storage/box/survival": { "parent": "/obj/item/storage/box" },
        "/obj/item/card": { "parent": "/obj/item" },
        "/obj/item/card/id": { "parent": "/obj/item/card" },
        "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
        "/obj/item/card/id/advanced/silver": { "parent": "/obj/item/card/id/advanced" },
        "/obj/item/clothing": { "parent": "/obj/item" },
        "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing" },
        "/obj/item/clothing/under/rank/security/officer": { "parent": "/obj/item/clothing" },
        "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing" },
        "/obj/item/clothing/shoes/jackboots/sec": { "parent": "/obj/item/clothing" },
        "/obj/item/choice_beacon": { "parent": "/obj/item" }
      }
    }"##;

    fn data() -> JobsData {
        resolve(&DmTypes::parse(TYPES.as_bytes()).unwrap())
    }

    #[test]
    fn joinable_jobs_departments_and_positions() {
        let data = data();
        let titles: Vec<&str> = data.jobs.iter().map(|j| j.title.as_str()).collect();
        assert_eq!(
            titles,
            ["AI", "Assistant", "Head of Security", "Security Officer"]
        );
        let assistant = &data.jobs[data.job("Assistant").unwrap()];
        assert_eq!(
            (assistant.total_positions, assistant.spawn_positions),
            (-1, -1)
        );
        let officer = &data.jobs[data.job("Security Officer").unwrap()];
        assert_eq!(officer.total_positions, 5);
        assert!(!officer.command);
        // `JOB_ANTAG_PROTECTED` in 16863; the assistant (479) has neither antag flag.
        assert!(officer.antag_protected && !officer.antag_blacklisted);
        assert!(!assistant.antag_protected && !assistant.antag_blacklisted);
        // Display order of departments; HoS (command) is listed in both of its departments.
        let listing: Vec<(&str, Vec<&str>)> = data
            .departments
            .iter()
            .map(|d| {
                let jobs = d
                    .jobs
                    .iter()
                    .map(|&j| data.jobs[j as usize].title.as_str())
                    .collect();
                (d.name.as_str(), jobs)
            })
            .collect();
        assert_eq!(
            listing,
            [
                ("Command", vec!["Head of Security", "AI"]),
                ("Security", vec!["Head of Security", "Security Officer"]),
                ("No Department", vec!["Assistant"]),
            ]
        );
        assert_eq!(data.departments[1].color, "#d46a78");
        let ai = &data.jobs[data.job("AI").unwrap()];
        assert!(ai.outfit.is_none() && !ai.unresolved.is_empty());
    }

    #[test]
    fn outfit_follows_pre_equip_defaults() {
        let data = data();
        let officer = &data.jobs[data.job("Security Officer").unwrap()];
        let outfit = officer.outfit.as_ref().unwrap();
        assert_eq!(
            outfit.item(OutfitSlot::Uniform),
            Some("/obj/item/clothing/under/rank/security/officer")
        );
        assert_eq!(
            outfit.item(OutfitSlot::Id),
            Some("/obj/item/card/id/advanced")
        );
        // DBACKPACK: the department backpack replaces the grey one.
        assert_eq!(
            outfit.item(OutfitSlot::Back),
            Some("/obj/item/storage/backpack/security")
        );
        assert_eq!(
            outfit.backpack_contents,
            [
                ("/obj/item/storage/box/survival".to_owned(), 1),
                ("/obj/item/choice_beacon".to_owned(), 2)
            ]
        );
        assert!(officer.unresolved.is_empty(), "{:?}", officer.unresolved);
        let hos = &data.jobs[data.job("Head of Security").unwrap()];
        assert_eq!(
            hos.outfit.as_ref().unwrap().item(OutfitSlot::Id),
            Some("/obj/item/card/id/advanced/silver")
        );
    }

    #[test]
    fn trim_access_is_minimal_extra_wildcard_and_flag_groups() {
        let data = data();
        let access = |title| data.jobs[data.job(title).unwrap()].access.clone();
        assert_eq!(access("Assistant"), ["maint_tunnels"]);
        assert_eq!(
            access("Security Officer"),
            ["brig", "security", "maint_tunnels"]
        );
        assert_eq!(
            access("Head of Security"),
            ["hos", "armory", "common_a", "common_b", "command"]
        );
        let hos = &data.jobs[data.job("Head of Security").unwrap()];
        assert_eq!(hos.wildcard_access, ["armory"]);
        let officer = &data.jobs[data.job("Security Officer").unwrap()];
        assert!(officer.wildcard_access.is_empty());
        assert_eq!(officer.assignment, "Офицер");
        assert_eq!(
            officer.trim.as_deref(),
            Some("/datum/id_trim/job/security_officer")
        );
    }

    #[test]
    fn ron_round_trip() {
        let data = data();
        let ron = data.to_ron();
        assert_eq!(
            ron::from_str::<JobsData>(ron.split_once('\n').unwrap().1).unwrap(),
            data
        );
        let back: JobsData = ron::from_str(&ron).unwrap();
        assert_eq!(back, data);
    }
}
