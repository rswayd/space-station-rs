//! ID card access after tg `SSid_access` (`code/controllers/subsystem/networks/id_access.dm`)
//! and the access procs of `/obj/item/card/id` (`cards_ids.dm`): access flags, regions and
//! their descriptions, trims, wildcard slots, and who may edit which access in the ID console
//! (`/datum/computer_file/program/card_mod/proc/authenticate`).
//!
//! The tables are `code/__DEFINES/access.dm` and `id_cards.dm` as data (with BandaStation's
//! Justice and NT Representation regions).
// shortcut: wildcard slot limits come from a table of card types (`WILDCARD_LIMIT_*` per
// `/obj/item/card/id` subtype) instead of the converted `wildcard_slots` var; extend the table
// when a new card type gets its own limits.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::jobs::{Job, JobsData};

use crate::inventory::IdCard;

pub const ACCESS_CHANGE_IDS: &str = "change_ids";
pub const ACCESS_ARMORY: &str = "armory";
pub const ACCESS_SECURITY: &str = "security";
pub const ACCESS_HOP: &str = "hop";

pub const ACCESS_FLAG_COMMON: u32 = 1 << 0;
pub const ACCESS_FLAG_COMMAND: u32 = 1 << 1;
pub const ACCESS_FLAG_PRV_COMMAND: u32 = 1 << 2;
pub const ACCESS_FLAG_CAPTAIN: u32 = 1 << 3;
pub const ACCESS_FLAG_CENTCOM: u32 = 1 << 4;
pub const ACCESS_FLAG_SYNDICATE: u32 = 1 << 5;
pub const ACCESS_FLAG_AWAY: u32 = 1 << 6;
pub const ACCESS_FLAG_SPECIAL: u32 = 1 << 7;

/// `access_flag_string_by_flag`.
pub const ACCESS_FLAG_NAMES: [(u32, &str); 8] = [
    (ACCESS_FLAG_COMMON, "Common"),
    (ACCESS_FLAG_COMMAND, "Command"),
    (ACCESS_FLAG_PRV_COMMAND, "Private Command"),
    (ACCESS_FLAG_CAPTAIN, "Captain"),
    (ACCESS_FLAG_CENTCOM, "Centcom"),
    (ACCESS_FLAG_SYNDICATE, "Syndicate"),
    (ACCESS_FLAG_AWAY, "Away"),
    (ACCESS_FLAG_SPECIAL, "Special"),
];

/// `WILDCARD_NAME_FORCED`: wildcards forced onto a card that had no slot for them.
pub const WILDCARD_NAME_FORCED: &str = "Hacked";

/// `wildcard_flags_by_wildcard`.
pub const WILDCARD_FLAGS: [(&str, u32); 10] = [
    ("All", u32::MAX),
    ("Common", ACCESS_FLAG_COMMON),
    ("Command", ACCESS_FLAG_COMMON | ACCESS_FLAG_COMMAND),
    (
        "Private Command",
        ACCESS_FLAG_COMMON | ACCESS_FLAG_COMMAND | ACCESS_FLAG_PRV_COMMAND,
    ),
    (
        "Captain",
        ACCESS_FLAG_COMMON | ACCESS_FLAG_COMMAND | ACCESS_FLAG_PRV_COMMAND | ACCESS_FLAG_CAPTAIN,
    ),
    (
        "Centcom",
        ACCESS_FLAG_COMMON
            | ACCESS_FLAG_COMMAND
            | ACCESS_FLAG_PRV_COMMAND
            | ACCESS_FLAG_CAPTAIN
            | ACCESS_FLAG_CENTCOM,
    ),
    (
        "Syndicate",
        ACCESS_FLAG_COMMON
            | ACCESS_FLAG_COMMAND
            | ACCESS_FLAG_PRV_COMMAND
            | ACCESS_FLAG_CAPTAIN
            | ACCESS_FLAG_SYNDICATE,
    ),
    ("Away", ACCESS_FLAG_AWAY),
    ("Special", ACCESS_FLAG_SPECIAL),
    (WILDCARD_NAME_FORCED, u32::MAX),
];

pub fn wildcard_flags(name: &str) -> u32 {
    WILDCARD_FLAGS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(0, |&(_, f)| f)
}

const COMMON_ACCESS: &[&str] = &[
    "atmospherics",
    "atmos_equip",
    "aux_base",
    "bar",
    "bit_den",
    "brig",
    "brig_entrance",
    "department_budget",
    "cargo",
    "chapel_office",
    "construction",
    "court",
    "crematorium",
    "detective",
    "engine_equip",
    "engineering",
    "eva",
    "external airlocks",
    "explorer",
    "gateway",
    "genetics",
    "hydroponics",
    "janitor",
    "kitchen",
    "lawyer",
    "library",
    "maint_tunnels",
    "mech_mining",
    "mech_medical",
    "mech_security",
    "mech_science",
    "mech_engine",
    "medical",
    "mineral_storeroom",
    "mining",
    "mining_station",
    "morgue_secure",
    "morgue",
    "network",
    "ordnance",
    "ordnance_storage",
    "paramedic",
    "pharmacy",
    "plumbing",
    "psychology",
    "research",
    "robotics",
    "science",
    "security",
    "service",
    "shipping",
    "surgery",
    "tech_storage",
    "theatre",
    "virology",
    "weapons",
    "xenobiology",
];
const COMMAND_ACCESS: &[&str] = &[
    "ai_upload",
    "all_personal_lockers",
    "armory",
    "department_budget",
    "change_ids",
    "command",
    "eva",
    "keycard_auth",
    "minisat",
    "rc_announce",
    "tcomms",
    "teleporter",
    "vault",
];
const PRIVATE_COMMAND_ACCESS: &[&str] = &[
    "ce",
    "cmo",
    "hos",
    "hop",
    "qm",
    "rd",
    "blueshield",
    "nanotrasen_representative",
    "magistrate",
];
const CAPTAIN_ACCESS: &[&str] = &["captain"];
const CENTCOM_ACCESS: &[&str] = &[
    "cent_bar",
    "cent_captain",
    "cent_general",
    "cent_living",
    "cent_medical",
    "cent_specops",
    "cent_storage",
    "cent_teleporter",
    "cent_thunder",
    "cent_officer",
];
const SYNDICATE_ACCESS: &[&str] = &["syndicate", "syndicate_leader", "syndicate_command"];
const AWAY_ACCESS: &[&str] = &[
    "away_command",
    "away_engineering",
    "away_general",
    "away_generic1",
    "away_generic2",
    "away_generic3",
    "away_generic4",
    "away_maintenance",
    "away_medical",
    "away_science",
    "away_sec",
    "away_supply",
];
const CULT_ACCESS: &[&str] = &["bloodcult"];

/// `setup_access_flags`, in its order: `flags_by_access |= list(access = flag)` keeps the first
/// flag of an access listed twice (`department_budget`, `eva`: common).
const FLAG_GROUPS: [(u32, &[&str]); 8] = [
    (ACCESS_FLAG_COMMON, COMMON_ACCESS),
    (ACCESS_FLAG_COMMAND, COMMAND_ACCESS),
    (ACCESS_FLAG_PRV_COMMAND, PRIVATE_COMMAND_ACCESS),
    (ACCESS_FLAG_CAPTAIN, CAPTAIN_ACCESS),
    (ACCESS_FLAG_CENTCOM, CENTCOM_ACCESS),
    (ACCESS_FLAG_SYNDICATE, SYNDICATE_ACCESS),
    (ACCESS_FLAG_AWAY, AWAY_ACCESS),
    (ACCESS_FLAG_SPECIAL, CULT_ACCESS),
];

/// `SSid_access.get_access_flag`: 0 for an access no flag group lists.
pub fn access_flag(access: &str) -> u32 {
    FLAG_GROUPS
        .iter()
        .find(|(_, group)| group.contains(&access))
        .map_or(0, |&(flag, _)| flag)
}

/// `REGION_ALL_STATION`: `COMMON_ACCESS + COMMAND_ACCESS + PRIVATE_COMMAND_ACCESS +
/// CAPTAIN_ACCESS`, without repeats.
pub fn all_station_access() -> Vec<&'static str> {
    let mut out: Vec<&str> = Vec::new();
    for a in [
        COMMON_ACCESS,
        COMMAND_ACCESS,
        PRIVATE_COMMAND_ACCESS,
        CAPTAIN_ACCESS,
    ]
    .concat()
    {
        if !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

pub const REGION_COMMAND: &str = "Command";
pub const REGION_GENERAL: &str = "General";

/// `REGION_AREA_STATION` with `accesses_by_region`, in tg's order.
pub const STATION_REGIONS: [(&str, &[&str]); 9] = [
    (
        REGION_COMMAND,
        &[
            "ai_upload",
            "all_personal_lockers",
            "department_budget",
            "captain",
            "change_ids",
            "command",
            "eva",
            "gateway",
            "hop",
            "keycard_auth",
            "rc_announce",
            "teleporter",
            "vault",
        ],
    ),
    (
        "Engineering",
        &[
            "atmospherics",
            "atmos_equip",
            "aux_base",
            "department_budget",
            "ce",
            "construction",
            "engineering",
            "engine_equip",
            "external airlocks",
            "maint_tunnels",
            "mech_engine",
            "minisat",
            "tcomms",
            "tech_storage",
        ],
    ),
    (
        REGION_GENERAL,
        &[
            "bar",
            "department_budget",
            "chapel_office",
            "crematorium",
            "hydroponics",
            "janitor",
            "kitchen",
            "library",
            "service",
            "theatre",
        ],
    ),
    (
        "Medbay",
        &[
            "department_budget",
            "cmo",
            "mech_medical",
            "medical",
            "morgue",
            "morgue_secure",
            "paramedic",
            "pharmacy",
            "plumbing",
            "psychology",
            "surgery",
            "virology",
        ],
    ),
    (
        "Research",
        &[
            "ai_upload",
            "department_budget",
            "genetics",
            "mech_science",
            "minisat",
            "network",
            "ordnance",
            "ordnance_storage",
            "rd",
            "research",
            "robotics",
            "science",
            "xenobiology",
        ],
    ),
    (
        "Security",
        &[
            "armory",
            "brig",
            "brig_entrance",
            "department_budget",
            "court",
            "detective",
            "hos",
            "mech_security",
            "security",
            "weapons",
        ],
    ),
    (
        "Supply",
        &[
            "bit_den",
            "department_budget",
            "cargo",
            "mech_mining",
            "mineral_storeroom",
            "mining",
            "mining_station",
            "qm",
            "shipping",
            "vault",
            "explorer",
        ],
    ),
    ("Justice", &["lawyer", "magistrate"]),
    (
        "NT Representation",
        &["blueshield", "nanotrasen_representative"],
    ),
];

/// `get_region_access_list`: the accesses of `regions`, without repeats.
pub fn region_access_list(regions: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (name, access) in STATION_REGIONS {
        if !regions.contains(&name) {
            continue;
        }
        for a in access {
            if !out.iter().any(|o| o == a) {
                out.push((*a).to_owned());
            }
        }
    }
    out
}

/// `sub_department_managers_tgui`: a head's access and the regions it manages.
pub const MANAGERS: [(&str, &str); 9] = [
    ("captain", REGION_COMMAND),
    ("hop", REGION_GENERAL),
    ("hos", "Security"),
    ("cmo", "Medbay"),
    ("rd", "Research"),
    ("ce", "Engineering"),
    ("qm", "Supply"),
    ("magistrate", "Justice"),
    ("nanotrasen_representative", "NT Representation"),
];

/// `desc_by_access` (`setup_access_descriptions`); accesses without one are not listed in the
/// ID console.
pub fn access_desc(access: &str) -> Option<&'static str> {
    Some(match access {
        "cargo" => "Cargo Bay",
        "security" => "Security",
        "brig" => "Holding Cells",
        "court" => "Courtroom",
        "detective" => "Detective Office",
        "medical" => "Medical",
        "genetics" => "Genetics Lab",
        "morgue" => "Morgue",
        "morgue_secure" => "Coroner",
        "science" => "R&D Lab",
        "ordnance" => "Ordnance Lab",
        "ordnance_storage" => "Ordnance Storage",
        "plumbing" => "Chemistry Lab",
        "rd" => "RD Office",
        "bar" => "Bar",
        "janitor" => "Custodial Closet",
        "engineering" => "Engineering",
        "engine_equip" => "Power and Engineering Equipment",
        "maint_tunnels" => "Maintenance",
        "external airlocks" => "External Airlocks",
        "change_ids" => "ID Console",
        "ai_upload" => "AI Chambers",
        "teleporter" => "Teleporter",
        "eva" => "EVA",
        "department_budget" => "Department Budget",
        "command" => "Command",
        "captain" => "Captain",
        "all_personal_lockers" => "Personal Lockers",
        "chapel_office" => "Chapel Office",
        "tech_storage" => "Technical Storage",
        "atmospherics" => "Atmospherics",
        "atmos_equip" => "Atmospherics Equipment",
        "crematorium" => "Crematorium",
        "armory" => "Armory",
        "construction" => "Construction",
        "kitchen" => "Kitchen",
        "hydroponics" => "Hydroponics",
        "library" => "Library",
        "lawyer" => "Law Office",
        "robotics" => "Robotics",
        "virology" => "Virology",
        "psychology" => "Psychology",
        "cmo" => "CMO Office",
        "paramedic" => "Paramedic Office",
        "qm" => "QM Office",
        "surgery" => "Surgery",
        "theatre" => "Theatre",
        "research" => "Science",
        "mining" => "Mining Dock",
        "shipping" => "Cargo Shipping",
        "vault" => "Main Vault",
        "mining_station" => "Mining Outpost",
        "xenobiology" => "Xenobiology Lab",
        "hop" => "HoP Office",
        "hos" => "HoS Office",
        "ce" => "CE Office",
        "pharmacy" => "Pharmacy",
        "rc_announce" => "RC Announcements",
        "keycard_auth" => "Keycode Auth.",
        "tcomms" => "Telecommunications",
        "gateway" => "Gateway",
        "brig_entrance" => "Brig",
        "mineral_storeroom" => "Mineral Storage",
        "minisat" => "AI Satellite",
        "weapons" => "Weapon Permit",
        "network" => "Network Access",
        "mech_mining" => "Mining Mech Access",
        "mech_medical" => "Medical Mech Access",
        "mech_security" => "Security Mech Access",
        "mech_science" => "Science Mech Access",
        "mech_engine" => "Engineering Mech Access",
        "aux_base" => "Auxiliary Base",
        "service" => "Service Hallway",
        "cent_general" => "CentCom General Access",
        "cent_thunder" => "CentCom Thunderdome",
        "cent_storage" => "CentCom Storage",
        "cent_living" => "CentCom Living Quarters",
        "cent_medical" => "CentCom Medical",
        "cent_teleporter" => "CentCom Teleporter",
        "cent_specops" => "CentCom SpecOps",
        "cent_captain" => "CentCom Captain",
        "cent_bar" => "CentCom Bar",
        "cent_officer" => "CentCom Officer",
        "bit_den" => "Bitrunner Den",
        _ => return None,
    })
}

/// One `wildcard_slots` entry: `limit` (negative: unlimited) and the wildcards it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WildcardSlot {
    pub name: String,
    pub limit: i32,
    pub usage: Vec<String>,
}

fn slots(limits: &[(&str, i32)]) -> Vec<WildcardSlot> {
    limits
        .iter()
        .map(|&(name, limit)| WildcardSlot {
            name: name.to_owned(),
            limit,
            usage: Vec::new(),
        })
        .collect()
}

/// `wildcard_slots` of a card type: the `WILDCARD_LIMIT_*` its type (or nearest parent) sets.
pub fn wildcard_slots_for(path: &str) -> Vec<WildcardSlot> {
    const GREY: &[(&str, i32)] = &[("Common", 4)];
    const SILVER: &[(&str, i32)] = &[("Common", 6), ("Command", 2), ("Private Command", 1)];
    const PLATINUM: &[(&str, i32)] = &[("Common", -1), ("Command", 2), ("Private Command", 1)];
    const GOLD: &[(&str, i32)] = &[("Captain", -1)];
    const CENTCOM: &[(&str, i32)] = &[("Centcom", -1)];
    const SYNDICATE: &[(&str, i32)] = &[("Syndicate", -1)];
    const ADMIN: &[(&str, i32)] = &[("All", -1)];
    // Most specific first.
    const TABLE: [(&str, &[(&str, i32)]); 14] = [
        ("/obj/item/card/id/advanced/black/deathsquad", CENTCOM),
        (
            "/obj/item/card/id/advanced/black/syndicate_command",
            SYNDICATE,
        ),
        ("/obj/item/card/id/advanced/chameleon/elite", GOLD),
        ("/obj/item/card/id/advanced/black", GOLD),
        ("/obj/item/card/id/advanced/silver", SILVER),
        ("/obj/item/card/id/advanced/nanotrasen_official", SILVER),
        ("/obj/item/card/id/advanced/platinum", PLATINUM),
        ("/obj/item/card/id/advanced/gold", GOLD),
        ("/obj/item/card/id/advanced/centcom", CENTCOM),
        ("/obj/item/card/id/advanced/debug", ADMIN),
        ("/obj/item/card/id/advanced/highlander", ADMIN),
        ("/obj/item/card/id/advanced/simple_bot", ADMIN),
        ("/obj/item/card/id/advanced/prisoner", &[]),
        ("/obj/item/card/id/advanced", GREY),
    ];
    TABLE
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map(|(_, limits)| slots(limits))
        .unwrap_or_default()
}

/// A card's trim (`/datum/id_trim`): the job, its assignment and the accesses that need no
/// wildcard slot on this card.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trim {
    /// `trim.find_job().title`.
    pub job: String,
    pub assignment: String,
    pub access: Vec<String>,
    pub wildcard_access: Vec<String>,
    pub sechud_icon_state: String,
}

impl Trim {
    pub fn of_job(job: &Job) -> Self {
        Self {
            job: job.title.clone(),
            assignment: job.assignment.clone(),
            access: job
                .access
                .iter()
                .filter(|a| !job.wildcard_access.contains(a))
                .cloned()
                .collect(),
            wildcard_access: job.wildcard_access.clone(),
            sechud_icon_state: job.sechud_icon_state.clone(),
        }
    }
}

/// The rest of a card's access state next to [`IdCard`]: `trim`, `wildcard_slots`,
/// `registered_age`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct CardAccess {
    pub trim: Option<Trim>,
    pub wildcard_slots: Vec<WildcardSlot>,
    pub registered_age: u32,
}

impl CardAccess {
    /// A card of type `path` without a trim.
    pub fn of_type(path: &str) -> Self {
        Self {
            trim: None,
            wildcard_slots: wildcard_slots_for(path),
            registered_age: 30,
        }
    }
}

/// How `add_access`/`add_wildcards` treat what does not fit (`TRY_ADD_ALL`, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddMode {
    /// Use free wildcard slots, skip the rest.
    TryAddAll,
    /// Add only accesses of the trim, never wildcards.
    TryAddAllNoWildcard,
    /// Add everything, forcing what does not fit into the "Hacked" slot.
    ForceAddAll,
    /// All or nothing (tg crashes; here `false` and nothing changes).
    ErrorOnFail,
}

/// `build_access_lists`: accesses in the trim are basic, the rest are wildcards.
fn split(trim: Option<&Trim>, accesses: &[String]) -> (Vec<String>, Vec<String>) {
    let mut basic = Vec::new();
    let mut wild = Vec::new();
    for a in accesses {
        let list = if trim.is_some_and(|t| t.access.contains(a)) {
            &mut basic
        } else {
            &mut wild
        };
        if !list.contains(a) {
            list.push(a.clone());
        }
    }
    (basic, wild)
}

fn push_unique(list: &mut Vec<String>, a: &str) {
    if !list.iter().any(|x| x == a) {
        list.push(a.to_owned());
    }
}

/// `can_add_wildcards`.
pub fn can_add_wildcards(
    data: &CardAccess,
    wildcards: &[String],
    try_wildcard: Option<&str>,
) -> bool {
    if wildcards.is_empty() {
        return true;
    }
    let mut left: Vec<(&str, i32)> = data
        .wildcard_slots
        .iter()
        .filter(|s| try_wildcard.is_none_or(|t| t == s.name))
        .map(|s| (s.name.as_str(), s.limit - s.usage.len() as i32))
        .collect();
    if left.is_empty() {
        return false;
    }
    wildcards.iter().all(|w| {
        let flag = access_flag(w);
        left.iter_mut()
            .find(|(name, n)| flag & wildcard_flags(name) != 0 && *n != 0)
            .map(|(_, n)| *n -= 1)
            .is_some()
    })
}

/// `add_wildcards`.
fn add_wildcards(
    card: &mut IdCard,
    data: &mut CardAccess,
    wildcards: &[String],
    try_wildcard: Option<&str>,
    mode: AddMode,
) {
    for w in wildcards {
        let flag = access_flag(w);
        let slot = data.wildcard_slots.iter_mut().find(|s| {
            s.name != WILDCARD_NAME_FORCED
                && try_wildcard.is_none_or(|t| t == s.name)
                && flag & wildcard_flags(&s.name) != 0
                && s.limit - s.usage.len() as i32 != 0
        });
        if let Some(slot) = slot {
            push_unique(&mut slot.usage, w);
            push_unique(&mut card.access, w);
            continue;
        }
        if mode != AddMode::ForceAddAll {
            continue;
        }
        let forced = match data
            .wildcard_slots
            .iter()
            .position(|s| s.name == WILDCARD_NAME_FORCED)
        {
            Some(i) => &mut data.wildcard_slots[i],
            None => {
                data.wildcard_slots.push(WildcardSlot {
                    name: WILDCARD_NAME_FORCED.to_owned(),
                    limit: 0,
                    usage: Vec::new(),
                });
                data.wildcard_slots.last_mut().expect("just pushed")
            }
        };
        push_unique(&mut forced.usage, w);
        forced.limit = forced.usage.len() as i32;
        push_unique(&mut card.access, w);
    }
}

/// `remove_wildcards`.
fn remove_wildcards(card: &mut IdCard, data: &mut CardAccess, wildcards: &[String]) {
    for w in wildcards {
        card.access.retain(|a| a != w);
        for slot in &mut data.wildcard_slots {
            slot.usage.retain(|a| a != w);
            if slot.name == WILDCARD_NAME_FORCED {
                slot.limit = slot.usage.len() as i32;
            }
        }
    }
    data.wildcard_slots
        .retain(|s| s.name != WILDCARD_NAME_FORCED || s.limit > 0);
}

/// `add_access`: basic accesses go on, wildcards take slots (all of `try_wildcard` when set).
/// `false` (nothing changed) when `ErrorOnFail` cannot place every wildcard.
pub fn add_access(
    card: &mut IdCard,
    data: &mut CardAccess,
    accesses: &[String],
    try_wildcard: Option<&str>,
    mode: AddMode,
) -> bool {
    let (basic, wild) = split(data.trim.as_ref(), accesses);
    if mode == AddMode::ErrorOnFail && !can_add_wildcards(data, &wild, try_wildcard) {
        return false;
    }
    for a in &basic {
        push_unique(&mut card.access, a);
    }
    if mode != AddMode::TryAddAllNoWildcard {
        add_wildcards(card, data, &wild, try_wildcard, mode);
    }
    true
}

/// `remove_access`.
pub fn remove_access(card: &mut IdCard, data: &mut CardAccess, accesses: &[String]) {
    let (basic, wild) = split(data.trim.as_ref(), accesses);
    card.access.retain(|a| !basic.contains(a));
    remove_wildcards(card, data, &wild);
}

/// `clear_access`.
pub fn clear_access(card: &mut IdCard, data: &mut CardAccess) {
    for slot in &mut data.wildcard_slots {
        slot.usage.clear();
    }
    card.access.clear();
}

/// `apply_trim_to_card` (with `copy_access`): the trim's access and wildcards replace the
/// card's. `false` (unchanged) when the card cannot hold the trim's wildcards.
pub fn apply_trim(card: &mut IdCard, data: &mut CardAccess, trim: Trim) -> bool {
    if !can_add_wildcards(data, &trim.wildcard_access, None) {
        return false;
    }
    clear_access(card, data);
    card.access = trim.access.clone();
    let wild = trim.wildcard_access.clone();
    data.trim = Some(trim);
    add_wildcards(card, data, &wild, None, AddMode::ErrorOnFail);
    true
}

/// `add_trim_access_to_card`: the template's basic access (what the card's own trim covers)
/// and its wildcards where slots are free.
pub fn apply_template(card: &mut IdCard, data: &mut CardAccess, template: &Trim) {
    clear_access(card, data);
    add_access(
        card,
        data,
        &template.access,
        None,
        AddMode::TryAddAllNoWildcard,
    );
    add_wildcards(
        card,
        data,
        &template.wildcard_access,
        None,
        AddMode::TryAddAll,
    );
}

/// `remove_trim_from_card`.
pub fn remove_trim(card: &mut IdCard, data: &mut CardAccess) {
    data.trim = None;
    clear_access(card, data);
}

/// What an authorizing card may do in the ID console (`card_mod` after `authenticate`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardModAuth {
    /// Only some regions (a head of staff), not `ACCESS_CHANGE_IDS`.
    pub minor: bool,
    /// `region_access`: empty unless `minor`.
    pub region_access: Vec<&'static str>,
    /// `valid_access`: what this login may add or remove.
    pub valid_access: Vec<String>,
    /// `job_templates`: indices into the jobs.
    pub job_templates: Vec<usize>,
}

/// `card_mod/authenticate(auth_card)` for a station console (no `target_dept`, not CentCom):
/// `ACCESS_CHANGE_IDS` edits every station access with every template; a head edits the
/// regions its office access manages with the templates listing that access. `None`: neither.
pub fn authenticate(access: &[String], jobs: &JobsData) -> Option<CardModAuth> {
    let has = |a: &str| access.iter().any(|x| x == a);
    let templates = |filter: &dyn Fn(&Job) -> bool| -> Vec<usize> {
        jobs.jobs
            .iter()
            .enumerate()
            .filter(|(_, j)| !j.template_access.is_empty() && filter(j))
            .map(|(i, _)| i)
            .collect()
    };
    if has(ACCESS_CHANGE_IDS) {
        return Some(CardModAuth {
            minor: false,
            region_access: Vec::new(),
            valid_access: all_station_access()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            job_templates: templates(&|_| true),
        });
    }
    let managers: Vec<(&str, &str)> = MANAGERS.into_iter().filter(|(a, _)| has(a)).collect();
    if managers.is_empty() {
        return None;
    }
    let mut region_access: Vec<&'static str> = Vec::new();
    for (_, region) in &managers {
        if !region_access.contains(region) {
            region_access.push(region);
        }
    }
    let job_templates = templates(&|j| {
        managers
            .iter()
            .any(|(a, _)| j.template_access.iter().any(|t| t == a))
    });
    Some(CardModAuth {
        minor: true,
        valid_access: region_access_list(&region_access),
        region_access,
        job_templates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn assistant_card() -> (IdCard, CardAccess) {
        let mut data = CardAccess::of_type("/obj/item/card/id/advanced");
        let mut card = IdCard { access: Vec::new() };
        let trim = Trim {
            job: "Assistant".into(),
            assignment: "Ассистент".into(),
            access: v(&["maint_tunnels"]),
            ..default()
        };
        assert!(apply_trim(&mut card, &mut data, trim));
        (card, data)
    }

    #[test]
    fn flags_keep_the_first_group() {
        assert_eq!(access_flag("eva"), ACCESS_FLAG_COMMON);
        assert_eq!(access_flag("department_budget"), ACCESS_FLAG_COMMON);
        assert_eq!(access_flag("armory"), ACCESS_FLAG_COMMAND);
        assert_eq!(access_flag("hop"), ACCESS_FLAG_PRV_COMMAND);
        assert_eq!(access_flag("nonsense"), 0);
        assert_eq!(all_station_access().len(), 57 + 11 + 9 + 1);
    }

    #[test]
    fn a_grey_card_takes_four_common_wildcards() {
        let (mut card, mut data) = assistant_card();
        assert_eq!(card.access, ["maint_tunnels"]);
        for a in ["security", "medical", "bar", "kitchen"] {
            assert!(add_access(
                &mut card,
                &mut data,
                &v(&[a]),
                None,
                AddMode::ErrorOnFail
            ));
        }
        assert_eq!(data.wildcard_slots[0].usage.len(), 4);
        // The fifth common wildcard and any command access do not fit.
        assert!(!add_access(
            &mut card,
            &mut data,
            &v(&["janitor"]),
            None,
            AddMode::ErrorOnFail
        ));
        assert!(!add_access(
            &mut card,
            &mut data,
            &v(&["armory"]),
            None,
            AddMode::ErrorOnFail
        ));
        assert!(!card.access.contains(&"janitor".to_owned()));
        // Removing one frees its slot.
        remove_access(&mut card, &mut data, &v(&["bar"]));
        assert_eq!(data.wildcard_slots[0].usage.len(), 3);
        assert!(add_access(
            &mut card,
            &mut data,
            &v(&["janitor"]),
            None,
            AddMode::ErrorOnFail
        ));
        // A basic access never takes a slot.
        remove_access(&mut card, &mut data, &v(&["maint_tunnels"]));
        assert!(add_access(
            &mut card,
            &mut data,
            &v(&["maint_tunnels"]),
            None,
            AddMode::ErrorOnFail
        ));
        assert_eq!(data.wildcard_slots[0].usage.len(), 4);
    }

    #[test]
    fn a_silver_card_uses_the_named_slot() {
        let mut data = CardAccess::of_type("/obj/item/card/id/advanced/silver");
        let mut card = IdCard { access: Vec::new() };
        // A common access fits a command slot when asked for it.
        assert!(add_access(
            &mut card,
            &mut data,
            &v(&["bar"]),
            Some("Command"),
            AddMode::ErrorOnFail
        ));
        assert_eq!(data.wildcard_slots[1].usage, ["bar"]);
        assert!(add_access(
            &mut card,
            &mut data,
            &v(&["hos"]),
            None,
            AddMode::ErrorOnFail
        ));
        assert!(!add_access(
            &mut card,
            &mut data,
            &v(&["cmo"]),
            None,
            AddMode::ErrorOnFail
        ));
        assert!(!add_access(
            &mut card,
            &mut data,
            &v(&["captain"]),
            None,
            AddMode::ErrorOnFail
        ));
        // Forced wildcards go into "Hacked" and leave with it.
        add_access(
            &mut card,
            &mut data,
            &v(&["captain"]),
            None,
            AddMode::ForceAddAll,
        );
        assert!(
            data.wildcard_slots
                .iter()
                .any(|s| s.name == WILDCARD_NAME_FORCED)
        );
        remove_access(&mut card, &mut data, &v(&["captain"]));
        assert!(
            !data
                .wildcard_slots
                .iter()
                .any(|s| s.name == WILDCARD_NAME_FORCED)
        );
    }

    #[test]
    fn templates_add_only_what_the_trim_covers() {
        let (mut card, mut data) = assistant_card();
        let officer = Trim {
            access: v(&["brig", "security", "maint_tunnels"]),
            ..default()
        };
        apply_template(&mut card, &mut data, &officer);
        assert_eq!(card.access, ["maint_tunnels"]);
    }

    fn jobs() -> JobsData {
        let job = |title: &str, templates: &[&str]| Job {
            title: title.into(),
            template_access: v(templates),
            ..default()
        };
        JobsData {
            jobs: vec![
                job("Assistant", &["captain", "change_ids", "hop"]),
                job("Security Officer", &["captain", "change_ids", "hos"]),
                job("Captain", &["change_ids"]),
                job("Cyborg", &[]),
            ],
            ..default()
        }
    }

    #[test]
    fn change_ids_edits_everything_and_heads_their_regions() {
        let jobs = jobs();
        let hop = authenticate(&v(&["change_ids", "hop"]), &jobs).unwrap();
        assert!(!hop.minor);
        assert!(hop.valid_access.contains(&"armory".to_owned()));
        assert_eq!(hop.job_templates, [0, 1, 2]);
        let hos = authenticate(&v(&["hos", "armory"]), &jobs).unwrap();
        assert!(hos.minor);
        assert_eq!(hos.region_access, ["Security"]);
        assert!(hos.valid_access.contains(&"armory".to_owned()));
        assert!(!hos.valid_access.contains(&"bar".to_owned()));
        assert_eq!(hos.job_templates, [1]);
        assert!(authenticate(&v(&["security", "maint_tunnels"]), &jobs).is_none());
    }
}
