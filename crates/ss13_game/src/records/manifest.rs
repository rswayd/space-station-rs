//! `GLOB.manifest` (`code/datums/records/manifest.dm`, `record.dm`, `crime.dm`): a crew record
//! and a locked record per crew member who joins (`inject`), edits from the ID console
//! (`modify`), the crew list by department (`get_manifest`).
// shortcut: no character preferences yet: age 30, species "Human", gender "Male" (names come
// from the male name lists), no quirks; no character appearance or photos (the consoles show
// no preview); `inject` runs for every human job (tg checks `JOB_CREW_MANIFEST`).

use bevy::prelude::*;
use ss13_mapload::jobs::JobsData;
use ss13_mapload::spawner::Rng;

use super::{MENTAL_STABLE, PHYSICAL_ACTIVE, WANTED_NONE};
use crate::modpc::ManifestDepartment;

/// `/datum/crime` and `/datum/crime/citation` (`fine` set).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crime {
    pub id: u32,
    pub name: String,
    pub details: String,
    /// The author's name (tg keeps `usr` and shows its name).
    pub author: String,
    /// The author's body: only it (or armory access) may edit or void the crime.
    pub author_mob: Option<Entity>,
    /// `round_timestamp()` when it was written.
    pub time: String,
    pub valid: bool,
    pub voider: Option<String>,
    pub fine: Option<u32>,
    pub paid: u32,
}

/// `/datum/medical_note`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MedicalNote {
    pub id: u32,
    pub author: String,
    pub content: String,
    pub time: String,
}

/// `/datum/record/crew`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewRecord {
    /// `REF(record)`.
    pub id: u32,
    pub age: u32,
    pub blood_type: String,
    pub dna_string: String,
    pub fingerprint: String,
    pub gender: String,
    pub initial_rank: String,
    pub name: String,
    /// The user-visible job (`assignment`).
    pub rank: String,
    pub species: String,
    /// The trim's assignment.
    pub trim: String,
    pub voice: String,
    /// The job the trim belongs to (index into the jobs), for `get_manifest`.
    pub job: Option<usize>,
    /// `REF` of the locked record.
    pub lock_ref: u32,
    pub citations: Vec<Crime>,
    pub crimes: Vec<Crime>,
    pub major_disabilities: String,
    pub major_disabilities_desc: String,
    pub minor_disabilities: String,
    pub minor_disabilities_desc: String,
    pub medical_notes: Vec<MedicalNote>,
    pub physical_status: String,
    pub cause_of_death: String,
    pub mental_status: String,
    pub quirk_notes: String,
    pub security_note: String,
    pub wanted_status: String,
}

/// `/datum/record/locked`: what admins and respawns read; never edited in game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockedRecord {
    pub id: u32,
    pub name: String,
    pub rank: String,
    pub dna_string: String,
    pub fingerprint: String,
    pub blood_type: String,
    pub species: String,
    /// `mind_ref`: the body it was made for.
    pub mob: Entity,
}

/// `GLOB.manifest`.
#[derive(Resource)]
pub struct Manifest {
    pub general: Vec<CrewRecord>,
    pub locked: Vec<LockedRecord>,
    next_id: u32,
    rng: Rng,
}

impl Default for Manifest {
    fn default() -> Self {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self {
            general: Vec::new(),
            locked: Vec::new(),
            next_id: 1,
            rng: Rng::new(seed),
        }
    }
}

/// What `inject` reads from the body and its ID.
pub struct Joiner<'a> {
    pub name: &'a str,
    /// `id_card?.get_trim_assignment() || assigned_role.title`.
    pub assignment: &'a str,
    pub job: Option<usize>,
    pub mob: Entity,
}

impl Manifest {
    /// Restarts the record rng (ids, fingerprints, DNA, blood types) from `seed`.
    pub fn reseed(&mut self, seed: u64) {
        self.rng = Rng::new(seed);
    }

    pub fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 32 hex digits: tg's `md5` fingerprints and unique enzymes.
    fn hash(&mut self) -> String {
        format!("{:016x}{:016x}", self.rng.next_u64(), self.rng.next_u64())
    }

    /// `inject(person)`: a locked record and a crew record.
    pub fn inject(&mut self, who: Joiner) -> u32 {
        let dna_string = self.hash();
        let fingerprint = self.hash();
        let blood_type =
            super::BLOOD_TYPES[self.rng.range(0, super::BLOOD_TYPES.len() as i64 - 1) as usize];
        let lock_ref = self.next_id();
        self.locked.push(LockedRecord {
            id: lock_ref,
            name: who.name.to_owned(),
            rank: who.assignment.to_owned(),
            dna_string: dna_string.clone(),
            fingerprint: fingerprint.clone(),
            blood_type: blood_type.to_owned(),
            species: "Human".to_owned(),
            mob: who.mob,
        });
        let id = self.next_id();
        self.general.push(CrewRecord {
            id,
            age: 30,
            blood_type: blood_type.to_owned(),
            dna_string,
            fingerprint,
            gender: "Male".to_owned(),
            initial_rank: who.assignment.to_owned(),
            name: who.name.to_owned(),
            rank: who.assignment.to_owned(),
            species: "Human".to_owned(),
            trim: who.assignment.to_owned(),
            voice: String::new(),
            job: who.job,
            lock_ref,
            citations: Vec::new(),
            crimes: Vec::new(),
            major_disabilities: "None".to_owned(),
            major_disabilities_desc: "No disabilities have been diagnosed at the moment."
                .to_owned(),
            minor_disabilities: "None".to_owned(),
            minor_disabilities_desc: "No disabilities have been diagnosed at the moment."
                .to_owned(),
            medical_notes: Vec::new(),
            physical_status: PHYSICAL_ACTIVE.to_owned(),
            cause_of_death: String::new(),
            mental_status: MENTAL_STABLE.to_owned(),
            quirk_notes: String::new(),
            security_note: String::new(),
            wanted_status: WANTED_NONE.to_owned(),
        });
        id
    }

    /// `find_record(name)`.
    pub fn find(&self, name: &str) -> Option<&CrewRecord> {
        self.general.iter().find(|r| r.name == name)
    }

    pub fn get(&self, id: u32) -> Option<&CrewRecord> {
        self.general.iter().find(|r| r.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut CrewRecord> {
        self.general.iter_mut().find(|r| r.id == id)
    }

    /// `modify(name, assignment, trim)`: the ID console changed a card.
    pub fn modify(&mut self, name: &str, assignment: &str, trim: &str, job: Option<usize>) {
        if let Some(r) = self.general.iter_mut().find(|r| r.name == name) {
            r.rank = assignment.to_owned();
            r.trim = trim.to_owned();
            r.job = job;
        }
    }

    /// `get_manifest()`: the records by department in the departments' order (a job of several
    /// departments in its first one, a command job in all of them), unknown jobs under
    /// "Unassigned"; empty departments left out.
    pub fn departments(&self, jobs: &JobsData) -> Vec<ManifestDepartment> {
        let mut out: Vec<ManifestDepartment> = jobs
            .departments
            .iter()
            .map(|d| ManifestDepartment {
                name: d.name.clone(),
                entries: Vec::new(),
            })
            .collect();
        let mut unassigned = Vec::new();
        for r in &self.general {
            let entry = (r.name.clone(), r.rank.clone());
            let mut placed = false;
            if let Some(job) = r.job {
                for (i, dept) in jobs.departments.iter().enumerate() {
                    if dept.jobs.iter().any(|&j| usize::from(j) == job) {
                        out[i].entries.push(entry.clone());
                        placed = true;
                    }
                }
            }
            if !placed {
                unassigned.push(entry);
            }
        }
        out.push(ManifestDepartment {
            name: "Unassigned".to_owned(),
            entries: unassigned,
        });
        out.retain(|d| !d.entries.is_empty());
        out
    }
}

/// `round_timestamp()`: `hh:mm:ss` of the round.
pub fn round_timestamp(world: &World) -> String {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let secs = world
        .get_resource::<crate::round::Ticker>()
        .map_or(0.0, |t| t.round_duration(now))
        .max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// tg `inject` for a body that just joined as `job` (`crate::jobs::join`): the rank is its ID's
/// trim assignment, else the job's.
pub fn inject_joiner(world: &mut World, mob: Entity, job: usize) {
    let Some(jobs) = world.get_resource::<crate::jobs::Jobs>() else {
        return;
    };
    let job_assignment = jobs.data.jobs[job].assignment.clone();
    let name = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let assignment = world
        .get::<crate::inventory::Inventory>(mob)
        .and_then(|i| i.id)
        .and_then(|id| crate::inventory::get_id(world, id))
        .and_then(|card| world.get::<crate::id_access::CardAccess>(card))
        .and_then(|c| c.trim.as_ref())
        .map_or(job_assignment, |t| t.assignment.clone());
    let mut manifest = world.resource_mut::<Manifest>();
    let id = manifest.inject(Joiner {
        name: &name,
        assignment: &assignment,
        job: Some(job),
        mob,
    });
    // The record's DNA is the body's unique enzymes (`dna.unique_enzymes`).
    let dna_string = manifest
        .general
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.dna_string.clone());
    crate::genetics::ensure_dna(world, mob);
    if let Some(dna_string) = dna_string
        && let Some(mut dna) = world.get_mut::<crate::genetics::Dna>(mob)
    {
        dna.unique_enzymes = dna_string;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::jobs::{Department, Job};

    #[test]
    fn inject_makes_a_crew_and_a_locked_record() {
        let mut m = Manifest::default();
        let mob = Entity::PLACEHOLDER;
        let id = m.inject(Joiner {
            name: "Алиса",
            assignment: "Офицер",
            job: Some(1),
            mob,
        });
        let r = m.get(id).unwrap();
        assert_eq!(r.rank, "Офицер");
        assert_eq!(r.wanted_status, WANTED_NONE);
        assert_eq!(r.fingerprint.len(), 32);
        assert_ne!(r.fingerprint, r.dna_string);
        assert_eq!(m.locked[0].id, r.lock_ref);
        assert_eq!(m.locked[0].mob, mob);
        assert_eq!(m.find("Алиса").map(|r| r.id), Some(id));
    }

    #[test]
    fn manifest_groups_by_department_and_follows_modify() {
        let jobs = JobsData {
            departments: vec![
                Department {
                    name: "Command".into(),
                    jobs: vec![1],
                    ..default()
                },
                Department {
                    name: "Security".into(),
                    jobs: vec![0, 1],
                    ..default()
                },
            ],
            jobs: vec![Job::default(), Job::default()],
            ..default()
        };
        let mut m = Manifest::default();
        let mob = Entity::PLACEHOLDER;
        m.inject(Joiner {
            name: "Боб",
            assignment: "Офицер",
            job: Some(0),
            mob,
        });
        m.inject(Joiner {
            name: "Ева",
            assignment: "Клоун",
            job: None,
            mob,
        });
        let d = m.departments(&jobs);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].name, "Security");
        assert_eq!(d[1].name, "Unassigned");
        m.modify("Боб", "Глава", "Глава", Some(1));
        let d = m.departments(&jobs);
        assert_eq!(d[0].name, "Command");
        assert_eq!(d[0].entries, [("Боб".to_owned(), "Глава".to_owned())]);
        assert_eq!(d[1].name, "Security");
    }
}
