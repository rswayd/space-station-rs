//! Security and justice job hooks of [`super::join`] (server):
//!
//! - Security officers' departments (tg `/datum/job/security_officer`: `setup_department`,
//!   `get_officer_department`, `announce_latejoin`): every officer goes to the least staffed of
//!   the four departments (a random one of equals); the department's armband goes on the
//!   uniform, its headset replaces the officer's, its trim (`/datum/id_trim/job/security_officer/*`:
//!   its assignment, `department_access` and `elevated_access`) goes on the ID card and the PDA
//!   follows it. At round start the officer starts at the department's post (the depsec
//!   landmark, else a free tile of its checkpoint area); a latejoiner is announced on the
//!   security channel and to the department's officers' PDAs.
//! - `SSjob.setup_officer_positions`' extra lockers: one officer locker per officer slot the
//!   map lacks, on the `secequipment` landmarks.
//! - The prisoner's crime (`/datum/job/prisoner/handle_prisoner_joining`): a random
//!   `/datum/prisoner_crime` on the crew record, written by Central Command.
//! - The detective's lit cigarette (`/datum/outfit/job/detective/post_equip`).
// shortcut: no `prefered_security_department` / `prisoner_crime` preferences (every player has
// the defaults: "Нет", which still gets the least staffed department, and "Random");
// `DEPSEC_ACCESS_LEVEL` is BandaStation config's 2 (elevated access always) and `SEC_START_BRIG`
// is off, as `config/game_options.txt` ships them. Read them when the round config reads
// `game_options`.
// shortcut: no prison tattoos (persistence), no permabrig crime memory (no memories here), no
// 1% D-boy beanie, no pig cop butcher drops (the gibber's meat is per limb, not per body).

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::jobs::Job;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::Jobs;
use crate::id_access::{CardAccess, Trim, apply_trim};
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::outfit::{IdRegistration, id_label};
use crate::records::manifest::{Crime, Manifest, round_timestamp};
use crate::tools::tell;

/// `JOB_SECURITY_OFFICER`.
pub const SECURITY_OFFICER: &str = "Security Officer";
/// `JOB_PRISONER`.
pub const PRISONER: &str = "Prisoner";
/// `/datum/job/detective`'s `outfit`.
pub const DETECTIVE_OUTFIT: &str = "/datum/outfit/job/detective";
/// `/obj/structure/closet/secure_closet/security/sec`: the lockers BandaStation counts
/// (`GLOB.security_closets_count`).
pub const OFFICER_LOCKER: &str = "/obj/structure/closet/secure_closet/security/sec";
/// `SEC_DEPT_NONE`: every player's `prefered_security_department` here.
const SEC_DEPT_NONE: &str = "Нет";

/// A department a security officer can be assigned to.
#[derive(Debug, PartialEq, Eq)]
pub struct SecDept {
    /// `SEC_DEPT_*` (BandaStation's Russian names, shown to the officer).
    pub name: &'static str,
    /// The `/obj/effect/landmark/start/depsec/*` landmark's `name`.
    pub landmark: &'static str,
    /// `destination`: the checkpoint area.
    pub area: &'static str,
    pub ears: &'static str,
    pub accessory: &'static str,
    /// The trim's `assignment`.
    pub assignment: &'static str,
    /// The trim's `department_access` and `elevated_access`.
    pub access: &'static [&'static str],
}

/// `GLOB.available_depts`, in its order, with `setup_department`'s items and the trims.
pub const DEPARTMENTS: [SecDept; 4] = [
    SecDept {
        name: "Инженерный",
        landmark: "engineering_sec",
        area: "/area/station/security/checkpoint/engineering",
        ears: "/obj/item/radio/headset/headset_sec/alt/department/engi",
        accessory: "/obj/item/clothing/accessory/armband/engine",
        assignment: "Security Officer (Engineering)",
        access: &[
            "atmospherics",
            "engineering",
            "atmos_equip",
            "aux_base",
            "construction",
            "engine_equip",
            "tcomms",
        ],
    },
    SecDept {
        name: "Медицинский",
        landmark: "medical_sec",
        area: "/area/station/security/checkpoint/medical",
        ears: "/obj/item/radio/headset/headset_sec/alt/department/med",
        accessory: "/obj/item/clothing/accessory/armband/medblue",
        assignment: "Security Officer (Medical)",
        access: &[
            "medical",
            "morgue",
            "pharmacy",
            "plumbing",
            "surgery",
            "virology",
            "paramedic",
        ],
    },
    SecDept {
        name: "Исследовательский",
        landmark: "science_sec",
        area: "/area/station/security/checkpoint/science",
        ears: "/obj/item/radio/headset/headset_sec/alt/department/sci",
        accessory: "/obj/item/clothing/accessory/armband/science",
        assignment: "Security Officer (Science)",
        access: &[
            "research",
            "science",
            "aux_base",
            "genetics",
            "ordnance_storage",
            "ordnance",
            "robotics",
            "xenobiology",
        ],
    },
    SecDept {
        name: "Снабжения",
        landmark: "supply_sec",
        area: "/area/station/security/checkpoint/supply",
        ears: "/obj/item/radio/headset/headset_sec/alt/department/supply",
        accessory: "/obj/item/clothing/accessory/armband/cargo",
        assignment: "Security Officer (Cargo)",
        access: &[
            "bit_den",
            "cargo",
            "mining",
            "shipping",
            "aux_base",
            "mining_station",
        ],
    },
];

/// `GLOB.security_officer_distribution`: each officer's department. Kept when the officer's
/// body is gone (tg keys it by ref for that).
#[derive(Resource, Default, Debug)]
pub struct SecurityDistribution(pub HashMap<Entity, &'static str>);

/// tg `get_officer_department(preference, departments, distribution)`: the least staffed of
/// `departments`, the preference on a tie, else the first of the least staffed.
pub fn get_officer_department<'a>(
    preference: &str,
    departments: &[&'a str],
    distribution: impl IntoIterator<Item = &'a str>,
) -> &'a str {
    let mut amounts: Vec<(&str, u32)> = departments.iter().map(|&d| (d, 0)).collect();
    for d in distribution {
        if let Some((_, n)) = amounts.iter_mut().find(|(k, _)| *k == d) {
            *n += 1;
        }
    }
    let mut lowest: Vec<&str> = vec![departments[0]];
    let mut lowest_amount = u32::MAX;
    for &(d, n) in &amounts {
        if n < lowest_amount {
            lowest = vec![d];
            lowest_amount = n;
        } else if n == lowest_amount {
            lowest.push(d);
        }
    }
    let pick = if lowest.contains(&preference) {
        preference
    } else {
        lowest[0]
    };
    departments
        .iter()
        .copied()
        .find(|&d| d == pick)
        .unwrap_or(departments[0])
}

/// `get_my_department` for a new body: [`get_officer_department`] over the shuffled
/// departments.
pub fn pick_department(world: &mut World) -> &'static SecDept {
    let mut order: Vec<&'static str> = DEPARTMENTS.iter().map(|d| d.name).collect();
    crate::round::shuffle(&mut order, &mut world.resource_mut::<Jobs>().rng);
    let taken: Vec<&'static str> = world
        .get_resource::<SecurityDistribution>()
        .map(|d| d.0.values().copied().collect())
        .unwrap_or_default();
    let name = get_officer_department(SEC_DEPT_NONE, &order, taken);
    DEPARTMENTS
        .iter()
        .find(|d| d.name == name)
        .unwrap_or(&DEPARTMENTS[0])
}

/// `setup_department(move_to = TRUE)`'s destination: a depsec landmark of the department, else
/// a random free tile of its checkpoint area; `None` without either.
pub fn post_tile(world: &mut World, dept: &SecDept) -> Option<TilePos> {
    if let Some(pos) = super::job_spawn(world, dept.landmark) {
        return Some(pos);
    }
    // `isopenturf`: here a tile with nothing dense on it (walls are dense).
    let tiles: Vec<TilePos> = {
        let index = world.resource::<ss13_world::SpatialIndex>();
        crate::events::station_tiles(world)
            .into_iter()
            .filter(|&(area, pos)| {
                crate::events::area_type(world, area) == Some(dept.area)
                    && index.dense_on_tile(pos).is_empty()
            })
            .map(|(_, pos)| pos)
            .collect()
    };
    if tiles.is_empty() {
        return None;
    }
    let i = world
        .resource_mut::<Jobs>()
        .rng
        .range(0, tiles.len() as i64 - 1) as usize;
    Some(tiles[i])
}

/// A new `path` item worn by `mob` (`new path(spawning)` + `equip_to_slot_or_del`).
fn spawn_worn(world: &mut World, mob: Entity, path: &str) -> Option<Entity> {
    let item = crate::construction::spawn_detached(world, path)?;
    world.entity_mut(item).insert((Networked, CarriedBy(mob)));
    Some(item)
}

/// `setup_department` for a body that already wears the officer's outfit: the department is
/// recorded, the armband pinned, the headset swapped, the trim applied and the officer told.
/// Returns the card's assignment for the PDA (`pda.imprint_id`).
pub fn setup_department(
    world: &mut World,
    client: Entity,
    mob: Entity,
    officer: &Job,
    dept: &'static SecDept,
) -> String {
    world
        .get_resource_or_insert_with(SecurityDistribution::default)
        .0
        .insert(mob, dept.name);
    let inventory = world.get::<Inventory>(mob).cloned().unwrap_or_default();
    // `worn_under.attach_accessory(new accessory)`.
    if let Some(uniform) = inventory.uniform
        && let Some(armband) = crate::construction::spawn_detached(world, dept.accessory)
    {
        world.entity_mut(armband).insert(Networked);
        if !crate::clothing::accessories::pin(world, armband, uniform) {
            world.despawn(armband);
        }
    }
    // `qdel(spawning.ears)`, then the department's headset.
    if let Some(old) = inventory.ears {
        world.despawn(old);
    }
    let ears = spawn_worn(world, mob, dept.ears);
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        inv.ears = ears;
    }
    let assignment = apply_department_trim(world, &inventory, officer, dept)
        .unwrap_or_else(|| officer.assignment.clone());
    tell(
        world,
        client,
        Span::Bold,
        format!("Вы были закрепленны за {}!", dept.name),
    );
    assignment
}

/// `SSid_access.apply_trim_to_card(worn_id, dep_trim)` and the card's label; the new assignment,
/// or `None` when there is no card or it cannot take the trim.
fn apply_department_trim(
    world: &mut World,
    inventory: &Inventory,
    officer: &Job,
    dept: &SecDept,
) -> Option<String> {
    let card = crate::inventory::get_id(world, inventory.id?)?;
    let mut trim = Trim::of_job(officer);
    trim.assignment = dept.assignment.to_owned();
    for access in dept.access {
        if !trim.access.iter().any(|a| a == access) {
            trim.access.push((*access).to_owned());
        }
    }
    let mut id = world.get::<IdCard>(card)?.clone();
    let mut data = world.get::<CardAccess>(card)?.clone();
    if !apply_trim(&mut id, &mut data, trim) {
        return None;
    }
    let registered = world
        .get::<IdRegistration>(card)
        .map(|r| r.registered_name.clone())
        .unwrap_or_default();
    world.entity_mut(card).insert((id, data));
    if let Some(mut reg) = world.get_mut::<IdRegistration>(card) {
        reg.assignment = dept.assignment.to_owned();
    }
    if let Some(mut name) = world.get_mut::<ss13_mapload::DmName>(card) {
        name.name = id_label(&registered, dept.assignment);
    }
    Some(dept.assignment.to_owned())
}

/// `announce_latejoin`: the AAS line on the security channel, and the same line to the PDAs of
/// the department's officers (tg includes the newcomer). Nothing without a working AAS.
pub fn announce_latejoin(world: &mut World, name: &str, dept: &SecDept) {
    let mut systems = world.query::<(Entity, &DmObj)>();
    let aas = systems
        .iter(world)
        .filter(|(e, o)| {
            o.path == crate::arrivals::ANNOUNCEMENT_SYSTEM
                && world.get::<crate::power::Unpowered>(*e).is_none()
        })
        .map(|(e, _)| e)
        .min();
    let Some(aas) = aas else {
        return;
    };
    // `/datum/aas_config_entry/announce_officer`.
    let text = format!("Офицер {name} назначен(а) в {}.", dept.name);
    crate::arrivals::aas_announce_on(world, ss13_chat::radio::FREQ_SECURITY, &text);
    let mut partners: Vec<(Entity, String)> = world
        .resource::<SecurityDistribution>()
        .0
        .iter()
        .filter(|&(_, &d)| d == dept.name)
        .filter_map(|(&e, _)| Some((e, world.get::<Name>(e)?.to_string())))
        .collect();
    partners.sort();
    for (_, partner) in partners {
        crate::modpc::programs::messenger::send_automated(
            world,
            aas,
            &partner,
            "Security Department Update",
            "Automated Announcement System",
            &text,
        );
    }
}

/// `SSjob.setup_officer_positions`' lockers: as many officer lockers as the officer slots
/// (`max_positions` when unlimited) exceed the mapped ones, one per `secequipment` landmark
/// until they run out.
pub fn spawn_extra_lockers(world: &mut World, max_positions: i32) {
    let Some(total) = world.get_resource::<Jobs>().and_then(|jobs| {
        let j = jobs.data.job(SECURITY_OFFICER)?;
        Some(jobs.data.jobs[j].total_positions)
    }) else {
        return;
    };
    let needed = if total < 0 { max_positions } else { total };
    let mut lockers = world.query::<&DmObj>();
    let mapped = lockers
        .iter(world)
        .filter(|o| o.path == OFFICER_LOCKER)
        .count() as i32;
    for _ in 0..(needed - mapped).max(0) {
        let pos = {
            let mut spawns = world.resource_mut::<super::JobSpawns>();
            if spawns.secequipment.is_empty() {
                break;
            }
            spawns.secequipment.remove(0)
        };
        spawn_locker(world, pos);
    }
}

/// `new /obj/structure/closet/secure_closet/security/sec(spawnloc)`: closed and locked, filled
/// on its first opening.
fn spawn_locker(world: &mut World, pos: TilePos) {
    let Some(locker) = crate::construction::spawn_obj(world, OFFICER_LOCKER, pos) else {
        warn!("jobs: no prototype for {OFFICER_LOCKER}; no extra officer locker");
        return;
    };
    let closet = {
        let icons = world.get_resource::<ss13_mapload::IconIndex>();
        let entity = world.entity(locker);
        match (
            entity.get::<ss13_mapload::DmCloset>(),
            entity.get::<ss13_mapload::DmAtom>(),
        ) {
            (Some(dm), Some(atom)) => Some(crate::closets::Closet::from_dm(dm, atom, icons)),
            _ => None,
        }
    };
    if let Some(closet) = closet {
        world.entity_mut(locker).insert(closet);
        crate::closets::refresh_look(world, locker);
    }
}

/// `GLOB.prisoner_crimes`: `name`, `desc` (tg's English, as BandaStation keeps them).
pub const PRISONER_CRIMES: [(&str, &str); 21] = [
    (
        "Abhorrent Criminal Negligence",
        "Incompetently risked numerous lives.",
    ),
    (
        "Attempted Development of Cloning",
        "Attempted illegal research in the cloning sphere. Cloning, cloning construction, and \
         cloning-related R&D was outlawed in 2560.",
    ),
    ("Attempted Murder", "Attempted to maliciously kill someone."),
    (
        "Biological Terrorism",
        "Created and or released a lethal biological agent/lifeform.",
    ),
    ("Classified", "Consult Legal."),
    (
        "Corporate Espionage",
        "Conducted espionage against Nanotrasen for commercial purposes.",
    ),
    ("Counterfeiting", "Engaged in widespread fraud."),
    (
        "Enemy of the Corporation",
        "Acted as, or knowingly aided, an enemy of Nanotrasen.",
    ),
    (
        "Grand Sabotage",
        "Engaged in malicious destructive actions, seriously threatening Nanotrasen employees \
         and or infrastructure.",
    ),
    (
        "Grand Theft",
        "Stole items of high value or sensitive nature.",
    ),
    (
        "Identity Theft of High-Ranking Figure",
        "Impersonated a high-ranking figure.",
    ),
    (
        "Jaywalker",
        "Jaywalked across non-green tram crossings, shuttle docking zones, and/or through space.",
    ),
    ("Kidnapping", "Planned and executed a kidnapping."),
    (
        "Mass Murder",
        "Maliciously planned and executed the slaughter of many people.",
    ),
    (
        "Medical Malpractice",
        "Engaged in organ harvesting, limb theft, or other malicious medical procedures.",
    ),
    ("Murder", "Maliciously killed someone."),
    ("Mutiny", "Attempted to overthrow/subvert Chain of Command."),
    ("Other", "Consult Legal."),
    (
        "Tampering of Artificial Intelligence",
        "Uploaded malicious, negligent, or otherwise blacklisted directives to an Artificial \
         Intelligence.",
    ),
    ("Tax Evasion", "Outstanding amount of tax evasion."),
    (
        "Worship of Blacklisted Deities",
        "Practiced worship of blacklisted deities.",
    ),
];

/// `handle_prisoner_joining` with the "Random" crime: the crime goes on `name`'s crew record
/// (from Central Command) and the prisoner is told.
pub fn prisoner_joined(world: &mut World, client: Entity, name: &str) {
    let (crime, desc) = {
        let mut jobs = world.resource_mut::<Jobs>();
        let i = jobs.rng.range(0, PRISONER_CRIMES.len() as i64 - 1) as usize;
        PRISONER_CRIMES[i]
    };
    let time = round_timestamp(world);
    {
        let mut manifest = world.resource_mut::<Manifest>();
        let id = manifest.next_id();
        let record_id = manifest
            .general
            .iter_mut()
            .find(|r| r.name == name)
            .map(|record| {
                record.crimes.push(Crime {
                    id,
                    name: crime.to_owned(),
                    details: desc.to_owned(),
                    author: "Central Command".to_owned(),
                    author_mob: None,
                    time,
                    valid: true,
                    voider: None,
                    fine: None,
                    paid: 0,
                });
                record.id
            });
        // `recreate_manifest_photos`.
        if let Some(record_id) = record_id {
            manifest.photos.retain(|&(r, _), _| r != record_id);
        }
    }
    tell(
        world,
        client,
        Span::Warning,
        format!("You are imprisoned for \"{crime}\"."),
    );
}

/// `/datum/outfit/job/detective/post_equip`: `cig.light()` for a cigarette in the mask slot.
pub fn light_mask_cigarette(world: &mut World, mob: Entity) {
    if let Some(mask) = world.get::<Inventory>(mob).and_then(|i| i.mask) {
        crate::smoking::light(world, mask, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEPTS: [&str; 4] = [
        "Инженерный",
        "Медицинский",
        "Исследовательский",
        "Снабжения",
    ];

    #[test]
    fn officers_spread_to_the_least_staffed_department() {
        // Nobody yet: the preference wins, "Нет" gets the first.
        assert_eq!(get_officer_department("Снабжения", &DEPTS, []), "Снабжения");
        assert_eq!(
            get_officer_department(SEC_DEPT_NONE, &DEPTS, []),
            "Инженерный"
        );
        // One in engineering: the next goes elsewhere, even when it prefers engineering.
        assert_eq!(
            get_officer_department("Инженерный", &DEPTS, ["Инженерный"]),
            "Медицинский"
        );
        // Every department has one but science: science, whatever the preference.
        let staffed = ["Инженерный", "Медицинский", "Снабжения"];
        assert_eq!(
            get_officer_department("Снабжения", &DEPTS, staffed),
            "Исследовательский"
        );
        // A tie of two: the preferred one of them.
        assert_eq!(
            get_officer_department("Снабжения", &DEPTS, ["Инженерный", "Медицинский"]),
            "Снабжения"
        );
    }

    #[test]
    fn department_trims_match_tg() {
        let supply = DEPARTMENTS.iter().find(|d| d.name == "Снабжения").unwrap();
        assert_eq!(supply.assignment, "Security Officer (Cargo)");
        assert!(supply.access.contains(&"mining_station"));
        assert_eq!(
            DEPARTMENTS.iter().map(|d| d.name).collect::<Vec<_>>(),
            DEPTS
        );
    }

    #[test]
    fn prisoner_gets_a_crime_on_record() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.insert_resource(Jobs::new(super::super::fallback_jobs(), 5));
        let mob = world.spawn_empty().id();
        let mut manifest = Manifest::default();
        manifest.inject(crate::records::manifest::Joiner {
            name: "Боб Адамс",
            assignment: PRISONER,
            job: None,
            mob,
        });
        world.insert_resource(manifest);
        let client = world.spawn_empty().id();
        prisoner_joined(&mut world, client, "Боб Адамс");
        // Nobody else's record changes when the name is unknown.
        prisoner_joined(&mut world, client, "Никто");
        let manifest = world.resource::<Manifest>();
        let crimes = &manifest.find("Боб Адамс").unwrap().crimes;
        assert_eq!(crimes.len(), 1);
        assert_eq!(crimes[0].author, "Central Command");
        assert!(crimes[0].valid && crimes[0].fine.is_none());
        assert!(
            PRISONER_CRIMES
                .iter()
                .any(|&(n, d)| n == crimes[0].name && d == crimes[0].details)
        );
        assert_eq!(
            world
                .resource::<Messages<ss13_net::chat::server::SendChat>>()
                .len(),
            2
        );
    }
}
