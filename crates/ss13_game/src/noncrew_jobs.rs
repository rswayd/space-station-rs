//! Non-crew jobs (server), after tg's `/datum/job` types outside the station crew
//! (`code/modules/jobs/job_types/antagonists`, `spawner`, `event`, `ert`): the
//! `mind.assigned_role` of antagonists with their own body (operatives, wizards, nightmares,
//! ...), ghost role spawns, event roles and response teams.
//!
//! A mob holds either a station job ([`crate::jobs::PlayerJob`], with its slot) or a
//! [`NonCrewJob`]; [`set_assigned_role`] swaps the first for the second, which frees the slot
//! and keeps the mob off everything that counts the crew. The job's title is what the mind
//! panel and the job lookups show.
// shortcut: the roles' own `after_spawn`/`get_*_spawn_point` overrides are their antagonists'
// and spawners' (their groups); every ghost role spawner gives `/datum/job/ghost_role`, as
// `spawner_job_path` is not converted. The station trait jobs (bridge assistant, veteran
// advisor, big brother, cargo gorilla, Pun Pun) are joinable crew jobs behind their station
// traits (`docs/parity/round/station-traits.md`).

use bevy::prelude::*;

/// One non-crew `/datum/job` type: its path and `title`.
#[derive(Debug, PartialEq, Eq)]
pub struct NonCrewDef {
    pub path: &'static str,
    pub title: &'static str,
}

/// On a mob: its non-crew job (tg `mind.assigned_role` of a job without `JOB_CREW_MEMBER`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonCrewJob(pub &'static NonCrewDef);

pub const NUCLEAR_OPERATIVE: &str = "/datum/job/nuclear_operative";
pub const NUCLEAR_OPERATIVE_LEADER: &str = "/datum/job/nuclear_operative/leader";
pub const SPACE_WIZARD: &str = "/datum/job/space_wizard";
pub const WIZARD_APPRENTICE: &str = "/datum/job/wizard_apprentice";
pub const NIGHTMARE: &str = "/datum/job/nightmare";
pub const PARADOX_CLONE: &str = "/datum/job/paradox_clone";
pub const MORPH: &str = "/datum/job/morph";
pub const GHOST_ROLE: &str = "/datum/job/ghost_role";

const fn job(path: &'static str, title: &'static str) -> NonCrewDef {
    NonCrewDef { path, title }
}

/// The non-crew job types and their titles (`ROLE_*`, `JOB_ERT_*`), in tg's file order.
pub static NON_CREW_JOBS: [NonCrewDef; 63] = [
    // antagonists
    job("/datum/job/abductor_agent", "Abductor Agent"),
    job("/datum/job/abductor_scientist", "Abductor Scientist"),
    job("/datum/job/abductor_solo", "Abductor Solo"),
    job("/datum/job/lone_operative", "Lone Operative"),
    job(MORPH, "Morph"),
    job(NIGHTMARE, "Nightmare"),
    job(NUCLEAR_OPERATIVE, "Operative"),
    job(NUCLEAR_OPERATIVE_LEADER, "Operative"),
    job(
        "/datum/job/nuclear_operative/clown_operative",
        "Clown Operative",
    ),
    job(
        "/datum/job/nuclear_operative/clown_operative/leader",
        "Clown Operative",
    ),
    job(PARADOX_CLONE, "Paradox Clone"),
    job("/datum/job/revenant", "Revenant"),
    job("/datum/job/slaughter_demon", "Slaughter Demon"),
    job("/datum/job/space_dragon", "Space Dragon"),
    job("/datum/job/space_ninja", "Space Ninja"),
    job(SPACE_WIZARD, "Wizard"),
    job("/datum/job/voidwalker", "Voidwalker"),
    job(WIZARD_APPRENTICE, "Apprentice"),
    job("/datum/job/xenomorph", "Xenomorph"),
    // spawner
    job("/datum/job/ancient_crew", "Ancient Crew"),
    job("/datum/job/ash_walker", "Ash Walker"),
    job("/datum/job/battlecruiser_crew", "Battlecruiser Crew"),
    job("/datum/job/battlecruiser_captain", "Battlecruiser Captain"),
    job("/datum/job/beach_bum", "Beach Bum"),
    job("/datum/job/escaped_prisoner", "Escaped Prisoner"),
    job("/datum/job/exile", "Exile"),
    job("/datum/job/free_golem", "Free Golem"),
    job("/datum/job/fugitive_hunter", "Fugitive Hunter"),
    job(GHOST_ROLE, "Ghost Role"),
    job("/datum/job/hermit", "Hermit"),
    job("/datum/job/hotel_staff", "Hotel Staff"),
    job("/datum/job/lavaland_syndicate", "Lavaland Syndicate"),
    job("/datum/job/lifebringer", "Lifebringer"),
    job("/datum/job/maintenance_drone", "Maintenance Drone"),
    job("/datum/job/personal_ai", "pAI"),
    job("/datum/job/positronic_brain", "Positronic Brain"),
    job("/datum/job/servant_golem", "Servant Golem"),
    job("/datum/job/skeleton", "Skeleton"),
    job("/datum/job/space_bar_patron", "Space Bar Patron"),
    job("/datum/job/space_bartender", "Space Bartender"),
    job("/datum/job/space_doctor", "Space Doctor"),
    job("/datum/job/space_pirate", "Space Pirate"),
    job("/datum/job/space_syndicate", "Space Syndicate"),
    job("/datum/job/spider", "Spider"),
    job("/datum/job/syndicate_cybersun", "Cybersun Space Syndicate"),
    job(
        "/datum/job/syndicate_cybersun_captain",
        "Cybersun Space Syndicate Captain",
    ),
    job("/datum/job/venus_human_trap", "Venus Human Trap"),
    job("/datum/job/zombie", "Zombie"),
    // event
    job("/datum/job/admin", "Debugger"),
    job("/datum/job/fugitive", "Fugitive"),
    job("/datum/job/santa", "Santa"),
    // ert (BandaStation's Russian titles where it sets them)
    job("/datum/job/ert", "Сотрудник ОБР"),
    job("/datum/job/ert/deatsquad", "Death Commando"),
    job(
        "/datum/job/ert/commander",
        "Emergency Response Team Commander",
    ),
    job("/datum/job/ert/officer", "Security Response Officer"),
    job("/datum/job/ert/engineer", "Engineering Response Officer"),
    job("/datum/job/ert/medical_doctor", "Medical Response Officer"),
    job("/datum/job/ert/chaplain", "Religious Response Officer"),
    job("/datum/job/ert/janitor", "Janitorial Response Officer"),
    job("/datum/job/ert/clown", "Entertainment Response Officer"),
    job("/datum/job/ert/official", "Представитель ЦК"),
    job("/datum/job/ert/intern", "Интерн ЦК"),
    job("/datum/job/ert/intern/leader", "Главный интерн ЦК"),
];

/// The non-crew job type `path`.
pub fn def(path: &str) -> Option<&'static NonCrewDef> {
    NON_CREW_JOBS.iter().find(|j| j.path == path)
}

/// The first non-crew job titled `title` (any case), for the admin's job pick.
pub fn by_title(title: &str) -> Option<&'static NonCrewDef> {
    NON_CREW_JOBS
        .iter()
        .find(|j| j.title.eq_ignore_ascii_case(title))
}

/// tg `mind.set_assigned_role(SSjob.get_job_type(path))` for a non-crew job: the station job
/// (and its slot) goes. An unknown `path` only drops the station job (a bug in the caller:
/// logged).
pub fn set_assigned_role(world: &mut World, mob: Entity, path: &str) {
    let job = def(path);
    if job.is_none() {
        error!("{path}: not a non-crew job type");
    }
    let Ok(mut e) = world.get_entity_mut(mob) else {
        return;
    };
    e.remove::<crate::jobs::PlayerJob>();
    match job {
        Some(job) => {
            e.insert(NonCrewJob(job));
        }
        None => {
            e.remove::<NonCrewJob>();
        }
    }
}

/// The title of `mob`'s non-crew job, if it has one.
pub fn title_of(world: &World, mob: Entity) -> Option<&'static str> {
    world.get::<NonCrewJob>(mob).map(|j| j.0.title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_unique_and_known() {
        for j in &NON_CREW_JOBS {
            assert_eq!(NON_CREW_JOBS.iter().filter(|o| o.path == j.path).count(), 1);
        }
        for path in [
            NUCLEAR_OPERATIVE,
            NUCLEAR_OPERATIVE_LEADER,
            SPACE_WIZARD,
            WIZARD_APPRENTICE,
            NIGHTMARE,
            PARADOX_CLONE,
            MORPH,
            GHOST_ROLE,
        ] {
            assert!(def(path).is_some(), "{path}");
        }
        assert_eq!(by_title("wizard").map(|j| j.path), Some(SPACE_WIZARD));
    }

    #[test]
    fn assigned_role_replaces_the_station_job() {
        let mut world = World::new();
        let mob = world.spawn(crate::jobs::PlayerJob(3)).id();
        set_assigned_role(&mut world, mob, SPACE_WIZARD);
        assert!(world.get::<crate::jobs::PlayerJob>(mob).is_none());
        assert_eq!(title_of(&world, mob), Some("Wizard"));
        set_assigned_role(&mut world, mob, "/datum/job/nope");
        assert_eq!(title_of(&world, mob), None);
    }
}
