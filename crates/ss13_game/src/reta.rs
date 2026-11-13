//! BandaStation's RETA, Request Emergency Temporary Access (server), after
//! `code/modules/reta/reta_system.dm`, `reta_id_card.dm` and its hooks in
//! `code/game/machinery/requests_console.dm` and `doors/airlock.dm`.
//!
//! - A requests console's emergency call (security, engineering, medical) from a console whose
//!   department RETA knows ([`department_by_name`]) gives every living player's ID card of the
//!   called department's jobs the calling department's access ([`grants`]) for 5 minutes
//!   (`reta_duration_ds`); the holder hears a warning beep and sees "emergency access". A caller
//!   cannot call their own department, and each origin-target pair waits 15 s between calls
//!   (`reta_dept_cooldown_ds`). The call goes out with the RETA result, and the calling
//!   department's channel hears who now has access when anyone got it.
//! - When it runs out, a card loses the flags no other grant still gives; its holder is told.
//! - While a grant is active, closed airlocks whose access it covers show their RETA lights.
// shortcut: no config (`reta_enabled`, duration, cooldown are the defaults), no grants applied
// to cards made or retrimmed during an active grant (`apply_active_reta_grants_to_card`), no
// debug verbs, admin notice of multi-department calls or investigate logs, no airlock examine
// line; job departments are those the jobs list them under (`Department::jobs`: a non-command
// job only under its first department).

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::DoorKind;
use ss13_shared::Appearance;
use ss13_shared::chat::Span;

use crate::doors::Door;
use crate::inventory::{IdCard, Inventory};
use crate::sound::{PlaySound, playsound};
use crate::tools::now;

/// `reta_duration_ds` default (3000).
pub const DURATION_SECS: f64 = 300.0;
/// `reta_dept_cooldown_ds` default (150).
pub const COOLDOWN_SECS: f64 = 15.0;
const WARNING_SOUND: &str = "sound/machines/cryo_warning.ogg";

/// `GLOB.reta_dept_grants`: what each department's call hands out.
const DEPT_GRANTS: [(&str, &[&str]); 8] = [
    ("Medical", &["medical", "surgery"]),
    ("Security", &["security", "brig", "brig_entrance"]),
    ("Engineering", &["engineering", "atmospherics"]),
    ("Science", &["science", "research"]),
    ("Service", &["service", "bar", "kitchen", "hydroponics"]),
    ("Cargo", &["cargo"]),
    ("Mining", &["mining", "mining_station", "cargo"]),
    ("Command", &["command"]),
];

/// The access a call from `dept` grants.
pub fn grants(dept: &str) -> &'static [&'static str] {
    DEPT_GRANTS
        .iter()
        .find(|(d, _)| *d == dept)
        .map(|&(_, g)| g)
        .unwrap_or(&[])
}

/// `reta_get_user_department_by_name`: the RETA department of a console's department name.
pub fn department_by_name(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let table: [(&[&str], &'static str); 22] = [
        (&["engineering", "engine"], "Engineering"),
        (&["science", "research"], "Science"),
        (&["cargo", "supply"], "Cargo"),
        (&["mining", "mine"], "Mining"),
        (&["service", "civilian"], "Service"),
        (&["medical", "medbay"], "Medical"),
        (&["security", "sec"], "Security"),
        (&["command", "bridge"], "Command"),
        (&["kitchen", "bar", "cafeteria", "diner"], "Service"),
        (&["hydroponics", "botany"], "Service"),
        (&["janitor", "custodial"], "Service"),
        (&["pharmacy", "chemistry", "chem"], "Medical"),
        (&["morgue", "virology"], "Medical"),
        (&["surgery", "operating", "cryo"], "Medical"),
        (&["patients", "exam"], "Medical"),
        (&["atmospherics", "atmos"], "Engineering"),
        (&["supermatter", "engine"], "Engineering"),
        (&["gravity", "telecomm", "tcomm"], "Engineering"),
        (&["xenobiology", "xenobio"], "Science"),
        (&["robotics", "genetics"], "Science"),
        (&["ordnance", "cytology"], "Science"),
        (&["brig", "holding", "armory", "checkpoint"], "Security"),
    ];
    table.iter().find(|(words, _)| has(words)).map(|&(_, d)| d)
}

/// The RETA departments of a `/datum/job_department` path (cargo is mining's too).
fn departments_of_path(path: &str) -> &'static [&'static str] {
    match path.rsplit('/').next().unwrap_or_default() {
        "medical" => &["Medical"],
        "security" => &["Security"],
        "engineering" => &["Engineering"],
        "science" => &["Science"],
        "service" => &["Service"],
        "command" => &["Command"],
        "cargo" => &["Cargo", "Mining"],
        _ => &[],
    }
}

/// The RETA departments of the job at `index` (`departments_bitflags`).
fn job_departments(world: &World, index: usize) -> Vec<&'static str> {
    let Some(jobs) = world.get_resource::<crate::jobs::Jobs>() else {
        return Vec::new();
    };
    jobs.data
        .departments
        .iter()
        .filter(|d| d.jobs.iter().any(|&j| usize::from(j) == index))
        .flat_map(|d| departments_of_path(&d.path).iter().copied())
        .collect()
}

/// `reta_get_user_department`: the caller's department by their job, in tg's order.
pub fn user_department(world: &World, mob: Entity) -> Option<&'static str> {
    let index = world.get::<crate::jobs::PlayerJob>(mob)?.0;
    let depts = job_departments(world, index);
    [
        "Engineering",
        "Science",
        "Cargo",
        "Service",
        "Medical",
        "Security",
        "Command",
    ]
    .into_iter()
    .find(|d| depts.contains(d))
}

/// Whether `card`'s trim job is one of `dept`'s (`is_type_in_list(trim, reta_job_trims[dept])`).
fn card_in_department(world: &World, card: Entity, dept: &str) -> bool {
    let Some(job) = world
        .get::<crate::outfit::IdRegistration>(card)
        .map(|r| r.job.clone())
    else {
        return false;
    };
    let Some(index) = world
        .get_resource::<crate::jobs::Jobs>()
        .and_then(|j| j.data.job(&job))
    else {
        return false;
    };
    job_departments(world, index).iter().any(|d| *d == dept)
}

/// `GLOB.reta_cooldown` and `GLOB.reta_active_grants`.
#[derive(Resource, Default, Debug)]
pub struct Reta {
    /// (origin, target) -> when calls are allowed again.
    cooldowns: HashMap<(String, String), f64>,
    /// (target, origin, expiry) of the grants in force.
    pub active: Vec<(String, String, f64)>,
    /// The airlocks' RETA lights need recomputing.
    doors_dirty: bool,
}

impl Reta {
    /// `reta_on_cooldown`.
    pub fn on_cooldown(&self, origin: &str, target: &str, now: f64) -> bool {
        self.cooldowns
            .get(&(origin.to_owned(), target.to_owned()))
            .is_some_and(|&until| now < until)
    }

    /// `reta_set_cooldown`.
    pub fn set_cooldown(&mut self, origin: &str, target: &str, until: f64) {
        self.cooldowns
            .insert((origin.to_owned(), target.to_owned()), until);
    }
}

/// On an ID card: department -> the temporary flags it gave and when they expire
/// (`reta_temp_access`, `reta_timers`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct RetaAccess(pub Vec<(String, Vec<String>, f64)>);

pub struct RetaPlugin;

impl Plugin for RetaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Reta>()
            .add_systems(Update, (expire, door_lights).chain());
    }
}

/// `grant_reta_access(dept, duration)`: the department's flags the card lacks, until `until`;
/// an earlier grant of the department is extended. True when it gave new access.
pub fn grant_card(world: &mut World, card: Entity, dept: &str, until: f64) -> bool {
    let flags = grants(dept);
    let Some(mut access) = world.get::<IdCard>(card).map(|c| c.access.clone()) else {
        return false;
    };
    if flags.is_empty() {
        return false;
    }
    let mut temp = world.get::<RetaAccess>(card).cloned().unwrap_or_default();
    let i = match temp.0.iter().position(|(d, ..)| d == dept) {
        Some(i) => i,
        None => {
            temp.0.push((dept.to_owned(), Vec::new(), until));
            temp.0.len() - 1
        }
    };
    let mut new = Vec::new();
    for &flag in flags {
        if !access.iter().any(|a| a == flag) {
            access.push(flag.to_owned());
            new.push(flag.to_owned());
        }
    }
    let entry = &mut temp.0[i];
    entry.1.extend(new.iter().cloned());
    // tg drops the old timer before finding nothing new, so a re-grant would never expire.
    if !entry.1.is_empty() {
        entry.2 = until;
    }
    temp.0.retain(|(_, f, _)| !f.is_empty());
    if temp.0.is_empty() {
        world.entity_mut(card).remove::<RetaAccess>();
    } else {
        world.entity_mut(card).insert(temp);
    }
    if new.is_empty() {
        return false;
    }
    if let Some(mut c) = world.get_mut::<IdCard>(card) {
        c.access = access;
    }
    if let Some(holder) = crate::cargo::economy::holder_of(world, card) {
        if let Some(&pos) = world.get::<ss13_shared::TilePos>(holder) {
            playsound(world, PlaySound::new(pos, WARNING_SOUND, 25.0, true));
        }
        crate::balloon::balloon_alert(world, holder, holder, &format!("emergency access: {dept}"));
    }
    info!("RETA: granted {dept} temporary access ({new:?}) to ID card {card:?}");
    true
}

/// `reta_find_and_grant_access(target, origin, duration)`: every living player's ID card of a
/// `target` job gets `origin`'s access. The number of cards given new access.
pub fn find_and_grant(world: &mut World, target: &str, origin: &str, duration: f64) -> u32 {
    let mut players = world.query::<(Entity, &crate::server::PlayerMob, &Inventory)>();
    let cards: Vec<Entity> = players
        .iter(world)
        .filter(|(mob, ..)| {
            world
                .get::<crate::health::Stat>(*mob)
                .is_some_and(|s| *s != crate::health::Stat::Dead)
        })
        .filter_map(|(_, _, inventory)| inventory.id_card(world))
        .collect();
    let until = now(world) + duration;
    let mut granted = 0;
    for card in cards {
        if card_in_department(world, card, target) && grant_card(world, card, origin, until) {
            granted += 1;
        }
    }
    if granted > 0 {
        let mut reta = world.resource_mut::<Reta>();
        reta.active
            .retain(|(t, o, _)| !(t == target && o == origin));
        reta.active
            .push((target.to_owned(), origin.to_owned(), until));
        reta.doors_dirty = true;
    }
    info!("RETA: {origin} called {target}: {granted} ID cards granted access");
    granted
}

/// `clear_reta_access_for_dept` of expired grants, and the registry's expired entries.
fn expire(world: &mut World) {
    let t = now(world);
    {
        let mut reta = world.resource_mut::<Reta>();
        let before = reta.active.len();
        reta.active.retain(|(.., until)| *until > t);
        if reta.active.len() != before {
            reta.doors_dirty = true;
        }
    }
    let mut cards = world.query::<(Entity, &RetaAccess)>();
    let due: Vec<(Entity, RetaAccess)> = cards
        .iter(world)
        .filter(|(_, r)| r.0.iter().any(|(.., until)| *until <= t))
        .map(|(e, r)| (e, r.clone()))
        .collect();
    for (card, temp) in due {
        let (expired, kept): (Vec<_>, Vec<_>) =
            temp.0.into_iter().partition(|(.., until)| *until <= t);
        let still: Vec<&String> = kept.iter().flat_map(|(_, f, _)| f).collect();
        if let Some(mut c) = world.get_mut::<IdCard>(card) {
            for (_, flags, _) in &expired {
                c.access
                    .retain(|a| !flags.contains(a) || still.contains(&a));
            }
        }
        if kept.is_empty() {
            world.entity_mut(card).remove::<RetaAccess>();
        } else {
            world.entity_mut(card).insert(RetaAccess(kept));
        }
        let Some(holder) = crate::cargo::economy::holder_of(world, card) else {
            continue;
        };
        for (dept, ..) in expired {
            crate::balloon::balloon_alert(world, holder, holder, &format!("{dept} access expired"));
            crate::tools::tell_mob(
                world,
                holder,
                Span::Warning,
                format!("Emergency access to {dept} has expired."),
            );
        }
    }
}

/// `has_active_reta_access` of every airlock, when the grants changed.
fn door_lights(world: &mut World) {
    if !world.resource::<Reta>().doors_dirty {
        return;
    }
    let active: Vec<&'static [&'static str]> = {
        let mut reta = world.resource_mut::<Reta>();
        reta.doors_dirty = false;
        reta.active
            .iter()
            .map(|(_, origin, _)| grants(origin))
            .collect()
    };
    let mut doors = world.query::<(&mut Door, &mut Appearance)>();
    for (mut door, mut appearance) in doors.iter_mut(world) {
        if door.kind != DoorKind::Airlock {
            continue;
        }
        let covered = active.iter().any(|flags| {
            door.req_access
                .iter()
                .chain(&door.req_one_access)
                .any(|a| flags.contains(&a.as_str()))
        });
        if door.reta != covered {
            door.reta = covered;
            door.apply_look(&mut appearance);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_names_map_to_departments() {
        assert_eq!(department_by_name("Engineering"), Some("Engineering"));
        assert_eq!(department_by_name("Medical Department"), Some("Medical"));
        assert_eq!(department_by_name("medbay"), Some("Medical"));
        assert_eq!(department_by_name("Mining Station"), Some("Mining"));
        assert_eq!(department_by_name("Bridge"), Some("Command"));
        assert_eq!(department_by_name("Kitchen"), Some("Service"));
        assert_eq!(department_by_name("Virology"), Some("Medical"));
        assert_eq!(department_by_name("Xenobiology"), Some("Science"));
        assert_eq!(department_by_name("Armory"), Some("Security"));
        assert_eq!(department_by_name("Unknown"), None);
    }

    #[test]
    fn cooldowns_hold_per_origin_and_target() {
        let mut reta = Reta::default();
        assert!(!reta.on_cooldown("Engineering", "Medical", 0.0));
        reta.set_cooldown("Engineering", "Medical", 15.0);
        assert!(reta.on_cooldown("Engineering", "Medical", 10.0));
        assert!(!reta.on_cooldown("Science", "Medical", 10.0));
        assert!(!reta.on_cooldown("Engineering", "Medical", 15.0));
    }

    #[test]
    fn grants_add_missing_flags_and_expire_cleanly() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.init_resource::<Reta>();
        let card = world
            .spawn(IdCard {
                access: vec!["medical".into(), "maint_tunnels".into()],
            })
            .id();
        assert!(grant_card(&mut world, card, "Security", 10.0));
        let access = &world.get::<IdCard>(card).unwrap().access;
        assert!(access.contains(&"brig".to_owned()) && access.contains(&"security".to_owned()));
        // Medical's call adds surgery only: medical was the card's own.
        assert!(grant_card(&mut world, card, "Medical", 20.0));
        let temp = world.get::<RetaAccess>(card).unwrap().clone();
        assert_eq!(temp.0[1].1, vec!["surgery".to_owned()]);
        // A repeated call extends the grant without new flags.
        assert!(!grant_card(&mut world, card, "Security", 30.0));
        assert_eq!(world.get::<RetaAccess>(card).unwrap().0[0].2, 30.0);

        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(25));
        expire(&mut world);
        let access = &world.get::<IdCard>(card).unwrap().access;
        assert!(
            !access.contains(&"surgery".to_owned()),
            "medical's grant ran out"
        );
        assert!(
            access.contains(&"medical".to_owned()),
            "the card's own access stays"
        );
        assert!(access.contains(&"brig".to_owned()), "security's still runs");
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(10));
        expire(&mut world);
        let access = &world.get::<IdCard>(card).unwrap().access;
        assert_eq!(
            access,
            &vec!["medical".to_owned(), "maint_tunnels".to_owned()]
        );
        assert!(world.get::<RetaAccess>(card).is_none());
    }
}
