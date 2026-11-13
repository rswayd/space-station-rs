//! Footsteps (server): tg `/datum/element/footstep` as humans have it (`FOOTSTEP_MOB_HUMAN`,
//! volume 1, `e_range` -6, `human.dm`).
//!
//! Every second step a running human plays a sound of the turf it entered: from `GLOB.footstep`
//! by the turf's `footstep` key when its suit, uniform or shoes cover the feet, else from
//! `GLOB.barefootstep` by `barefootstep` (human legs are `FOOTSTEP_MOB_BAREFOOT`), at the table's
//! volume, `extrarange` table + `e_range`, `falloff_distance` 1. The tables come from the
//! converter (`ss13_mapload::sound`, dumped globals), the turf keys from the turf types
//! ([`DmTurf`]). A lying, unbuckled mob crawls instead (`crawl1.ogg` at 15, every step). Walking,
//! buckled or immobile mobs are silent, and so are turfs without a `footstep` (open space).
//!
//! BandaStation's MOD boots (`modular_bandastation/sounds/code/mod_shoes.dm`) carry tg's
//! `/datum/component/shoe_footstep`: they silence the wearer's own steps and every other step
//! play a rig step by their slowdown ([`mod_step_sound`]) at 30, `vary`, 3 tiles less range.
//!
//! Cost: one turf lookup per mob step (a few per player per second).
// shortcut: humans only (basic mobs' claw/heavy/shoe elements are added in `Initialize` procs
// the dump does not lower); no `footstep_override` (catwalks, tables), species or robotic legs,
// `TRAIT_SILENT_FOOTSTEPS`, FOV effect, and gravity is everywhere (the
// every-sixth-step hop rule never applies). Add with those mechanics.

use std::collections::HashSet;

use bevy::prelude::*;
use ss13_mapload::{DmClothing, DmTurf, Turf};
use ss13_world::{Moved, SpatialIndex};

use crate::health::{BodyPosition, Health, Stat};
use crate::human::HumanLook;
use crate::inventory::Inventory;
use crate::janitor::Walking;
use crate::medical::surgery::Buckled;
use crate::sound::{PlaySound, Sfx, SoundRng};

/// `AddElement(/datum/element/footstep, FOOTSTEP_MOB_HUMAN, 1, -6)`.
pub const HUMAN_VOLUME: f32 = 1.0;
pub const HUMAN_E_RANGE: f32 = -6.0;
/// `FEET` (`FOOT_LEFT | FOOT_RIGHT`).
const FEET: u32 = (1 << 5) | (1 << 6);

/// BandaStation `/obj/item/clothing/shoes/mod` `step_volume`.
pub const MOD_STEP_VOLUME: f32 = 30.0;

/// MOD boots: their `slowdown` (their share of the suit's, `update_speed`) and their
/// `shoe_footstep` step counter.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ModBoots {
    pub slowdown: f32,
    pub steps: u8,
}

/// BandaStation `update_footstep_sounds`: heavier boots, heavier steps.
pub fn mod_step_sound(slowdown: f32) -> &'static str {
    use ss13_mapload::sound::files::MOD_STEPS;
    if slowdown >= 0.3 {
        MOD_STEPS[0]
    } else if slowdown >= 0.2 {
        MOD_STEPS[1]
    } else if slowdown >= 0.1 {
        MOD_STEPS[2]
    } else {
        MOD_STEPS[3]
    }
}

/// `update_speed` sets the boots' `slowdown` (and with it their step sound).
pub fn set_mod_boots_slowdown(world: &mut World, boots: Entity, slowdown: f32) {
    if let Some(mut b) = world.get_mut::<ModBoots>(boots) {
        if b.slowdown != slowdown {
            b.slowdown = slowdown;
        }
    } else if let Ok(mut e) = world.get_entity_mut(boots) {
        e.insert(ModBoots { slowdown, steps: 0 });
    }
}

/// `shoe_footstep/stepped` with `steps_per_play = 1`: every other step sounds.
pub fn count_shoe_step(steps: &mut u8) -> bool {
    if *steps < 1 {
        *steps += 1;
        return false;
    }
    *steps = 0;
    true
}

/// tg `steps_for_living[mob]`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepCount(pub u8);

/// `prepare_step`'s counter: 24 steps around, every other one sounds.
pub fn count_step(steps: &mut u8) -> bool {
    *steps += 1;
    if *steps >= 24 {
        *steps = 0;
    }
    steps.is_multiple_of(2)
}

/// `FOOTSTEP_MOB_SHOE` / `FOOTSTEP_MOB_BAREFOOT`: the table and the turf's key into it.
pub fn step_table(turf: &DmTurf, shod: bool) -> (&'static str, &str) {
    if shod {
        ("footstep", &turf.footstep)
    } else {
        ("barefootstep", &turf.barefootstep)
    }
}

type Stepper<'a> = (
    Option<&'a mut StepCount>,
    Option<&'a Walking>,
    Option<&'a BodyPosition>,
    Option<&'a Health>,
    Option<&'a Stat>,
    Has<Buckled>,
    Option<&'a Inventory>,
    Option<&'a crate::quirks::Quirks>,
);

#[allow(clippy::too_many_arguments)]
pub(crate) fn play_footsteps(
    mut moved: MessageReader<Moved>,
    // `TRAIT_SILENT_FOOTSTEPS` (the ninja's cloak) makes no sound.
    mut mobs: Query<
        Stepper,
        (
            With<HumanLook>,
            Without<crate::modsuit::ninja::SilentFootsteps>,
        ),
    >,
    clothing: Query<&DmClothing>,
    mut mod_boots: Query<&mut ModBoots>,
    turfs: Query<&DmTurf, With<Turf>>,
    index: Res<SpatialIndex>,
    sfx: Res<Sfx>,
    mut rng: ResMut<SoundRng>,
    mut commands: Commands,
    mut out: MessageWriter<PlaySound>,
) {
    // A diagonal move is two steps in one tick; tg skips the second (`SECOND_DIAG_STEP`).
    let mut stepped = HashSet::new();
    for m in moved.read() {
        let Ok((count, walking, body, health, stat, buckled, inventory, quirks)) =
            mobs.get_mut(m.entity)
        else {
            continue;
        };
        if !stepped.insert(m.entity) {
            continue;
        }
        let Some(turf) = index
            .on_tile(m.to)
            .iter()
            .find_map(|&e| turfs.get(e).ok())
            .filter(|t| !t.closed)
        else {
            continue;
        };
        if body.is_some_and(|b| b.lying()) && !buckled {
            // `play_craw_step`.
            if !turf.footstep.is_empty() {
                let file = ss13_mapload::sound::files::CRAWL;
                let sound = PlaySound::new(m.to, file, 15.0 * HUMAN_VOLUME, false);
                out.write(sound.falloff_distance(1.0));
            }
            continue;
        }
        let stat = stat.copied().unwrap_or_default();
        if walking.is_some_and(|w| w.0) || buckled || health.is_some_and(|h| !h.can_move(stat)) {
            continue;
        }
        if let Some(mut boots) = inventory
            .and_then(|i| i.shoes)
            .and_then(|s| mod_boots.get_mut(s).ok())
        {
            if count_shoe_step(&mut boots.steps) {
                let file = mod_step_sound(boots.slowdown);
                let sound = PlaySound::new(m.to, file, MOD_STEP_VOLUME, true);
                out.write(sound.extrarange(crate::sound::MEDIUM_RANGE_SOUND_EXTRARANGE));
            }
            continue;
        }
        let sounds = match count {
            Some(mut c) => count_step(&mut c.0),
            None => {
                let mut steps = 0;
                let sounds = count_step(&mut steps);
                commands.entity(m.entity).insert(StepCount(steps));
                sounds
            }
        };
        if !sounds || turf.footstep.is_empty() {
            continue;
        }
        let covered = inventory.map_or(0, |i| {
            [i.suit, i.uniform, i.shoes]
                .into_iter()
                .flatten()
                .filter_map(|e| clothing.get(e).ok())
                .fold(0, |acc, c| acc | c.body_parts_covered)
        });
        let (table, key) = step_table(turf, covered & FEET != 0);
        let Some(step) = sfx.0.footsteps.get(table).and_then(|t| t.get(key)) else {
            continue;
        };
        if step.files.is_empty() {
            continue;
        }
        let file = &step.files[rng.0.range(1, step.files.len() as u32) as usize - 1];
        // `TRAIT_LIGHT_STEP` (the Light Step quirk): ×0.6 volume, 2 tiles less range.
        let (volume, range) = if quirks.is_some_and(|q| q.has(crate::quirks::Quirk::LightStep)) {
            (0.6, -2.0)
        } else {
            (1.0, 0.0)
        };
        let sound = PlaySound::new(
            m.to,
            file.as_str(),
            step.volume * HUMAN_VOLUME * volume,
            false,
        )
        .extrarange(step.range + HUMAN_E_RANGE + range)
        .falloff_distance(1.0);
        out.write(sound);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_boots_step_by_slowdown_every_other_step() {
        use ss13_mapload::sound::files::MOD_STEPS;
        assert_eq!(mod_step_sound(0.5), MOD_STEPS[0]);
        assert_eq!(mod_step_sound(0.3), MOD_STEPS[0]);
        assert_eq!(mod_step_sound(0.25), MOD_STEPS[1]);
        assert_eq!(mod_step_sound(0.1), MOD_STEPS[2]);
        assert_eq!(mod_step_sound(0.0), MOD_STEPS[3]);
        assert_eq!(mod_step_sound(-0.2), MOD_STEPS[3]);
        let mut steps = 0;
        let sounded: Vec<bool> = (0..4).map(|_| count_shoe_step(&mut steps)).collect();
        assert_eq!(sounded, [false, true, false, true]);
    }

    #[test]
    fn every_other_step_sounds_and_the_count_wraps_at_24() {
        let mut steps = 0;
        let played: Vec<bool> = (0..26).map(|_| count_step(&mut steps)).collect();
        assert_eq!(&played[..4], &[false, true, false, true]);
        // The 24th step wraps to 0 (sounds), the 25th is 1 again.
        assert!(played[23]);
        assert!(!played[24]);
        assert_eq!(steps, 2);
    }

    #[test]
    fn shoes_pick_the_shoe_table_bare_feet_the_barefoot_one() {
        let floor = DmTurf {
            footstep: "floor".into(),
            barefootstep: "hardbarefoot".into(),
            ..default()
        };
        assert_eq!(step_table(&floor, true), ("footstep", "floor"));
        assert_eq!(step_table(&floor, false), ("barefootstep", "hardbarefoot"));
    }
}
