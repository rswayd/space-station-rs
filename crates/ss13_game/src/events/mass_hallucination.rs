//! Mass Hallucination (`code/modules/events/mass_hallucination.dm`): every living human gets
//! the same hallucination type (`crate::hallucination`) at once: a normal or weird fake sound, a
//! fake station announcement or one of the generic hallucinations.
// shortcut: no delusion categories (delusions are not ported: those two rolls of ten take a
// generic hallucination instead), the generic list holds only the ported types (chat, death,
// fire, message; no bolts, plasma flood, Bubblegum or xeno attack), no CentCom level to skip
// and no admin setup (`/datum/event_admin_setup/mass_hallucination`).

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::hallucination::{self, Kind};
use crate::health::Stat;

pub const NAME: &str = "Mass Hallucination";

/// The generic hallucinations of category 6-10 that are ported.
const GENERIC: [&str; 4] = [
    "/datum/hallucination/chat",
    "/datum/hallucination/death",
    "/datum/hallucination/fire",
    "/datum/hallucination/message",
];

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(MassHallucination { forced: None })
}

/// `/datum/round_event/mass_hallucination`; `forced` is `admin_forced_hallucination`.
pub struct MassHallucination {
    pub forced: Option<Kind>,
}

impl RoundEvent for MassHallucination {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let Some(kind) = self.forced.or_else(|| pick(world)) else {
            error!("{NAME} found no hallucination to play");
            return;
        };
        for mob in targets(world) {
            hallucination::cause_hallucination(world, mob, kind, "mass hallucination");
        }
    }
}

/// `start()`'s `rand(1, 10)` category.
pub fn pick(world: &mut World) -> Option<Kind> {
    let category = world
        .get_resource_mut::<EventRng>()
        .map_or(6, |mut rng| rng.0.range(1, 10));
    let mut rng = hallucination::rng(world);
    let parent = match category {
        1 => "/datum/hallucination/fake_sound/normal",
        2 => "/datum/hallucination/fake_sound/weird",
        3 => "/datum/hallucination/station_message",
        _ => GENERIC[rng.range(0, GENERIC.len() as u32 - 1) as usize],
    };
    Kind::by_path(parent).or_else(|| Kind::random_subtype(&mut rng, parent))
}

/// The living humans; clientless ones only on the station levels (`GLOB.carbon_list` filters).
pub fn targets(world: &mut World) -> Vec<Entity> {
    let levels = station_levels(world);
    let mut query = world.query_filtered::<(
        Entity,
        &Stat,
        Option<&ss13_shared::TilePos>,
        Has<crate::server::PlayerMob>,
    ), With<crate::human::HumanLook>>();
    let mut out: Vec<Entity> = query
        .iter(world)
        .filter(|(_, stat, pos, client)| {
            **stat != Stat::Dead
                && (*client || pos.is_some_and(|p| p.z == 0 || levels.contains(&p.z)))
        })
        .map(|(e, ..)| e)
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_paths_are_ported_types() {
        for path in GENERIC {
            assert!(Kind::by_path(path).is_some(), "{path}");
        }
        let mut rng = crate::combat::CombatRng(5);
        for parent in [
            "/datum/hallucination/fake_sound/normal",
            "/datum/hallucination/fake_sound/weird",
            "/datum/hallucination/station_message",
        ] {
            assert!(Kind::random_subtype(&mut rng, parent).is_some(), "{parent}");
        }
    }
}
