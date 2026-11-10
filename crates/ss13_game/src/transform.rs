//! Mob transformations (server), after tg `code/modules/mob/transform_procs.dm`: the API other
//! modules call (`crate::transform::monkeyize`, `humanize`, `transform_into_mob`, `corgize`,
//! `crabize`, `gorillize`, `slimeize`, `alienize`).
//!
//! - [`monkeyize`], [`humanize`]: the same body changes species. Unless `instant`, it is stunned
//!   and hidden behind the `h2monkey`/`monkey2h` sprite for 2.2 s, then the species changes
//!   ([`Transforming`] is tg `transformation_timer` and the temporary `TRAIT_NO_TRANSFORM`).
//! - [`transform_into_mob`]: a new mob takes the mind, the old one is gone; its carried items
//!   drop to the floor (`delete_items`: go with it).
// shortcut: no `Robotize`, `AIize`, `become_overmind` or `Animalize` (admin) yet: add with the
// jobs and admin tools that need them; `slimeize` children are on the same tile, with default
// nutrition, and no `step_away`; the old body's name is not carried to a new mob, as in tg.

use bevy::prelude::*;
use ss13_shared::Appearance;
use ss13_shared::chat::Span;

use crate::combat::{CombatMode, CombatRng};
use crate::effects::temp_visual::{self, Visual};
use crate::explosions::turf_of;
use crate::species::{SpeciesId, species_of};
use crate::status_effects::{Incap, StatusEffects};
use crate::tools::tell_mob;

/// `TRANSFORMATION_DURATION`, seconds.
const TRANSFORMATION_SECS: f64 = 2.2;
const MOB_DMI: &str = "icons/mob/simple/mob.dmi";
const CORGI: &str = "/mob/living/basic/pet/dog/corgi";
const CRAB: &str = "/mob/living/basic/crab";
const GORILLA: &str = "/mob/living/basic/gorilla";
const GORILLA_GENETICS: &str = "/mob/living/basic/gorilla/genetics";
const SLIME: &str = "/mob/living/basic/slime";

/// tg `transformation_timer`: mid-transformation (hidden and stunned) until `at`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Transforming {
    at: f64,
    into: Change,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    Monkey,
    Human(SpeciesId),
}

pub struct TransformPlugin;

impl Plugin for TransformPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (finish_transformations, hide_transforming).run_if(any_with_component::<Transforming>),
        );
    }
}

/// `ismonkey`.
pub fn is_monkey(world: &World, mob: Entity) -> bool {
    species_of(world, mob) == SpeciesId::Monkey
}

/// Mid-transformation (`transformation_timer` or `TRAIT_NO_TRANSFORM`).
pub fn is_transforming(world: &World, mob: Entity) -> bool {
    world.get::<Transforming>(mob).is_some()
}

/// tg `monkeyize(instant)`: false when already a monkey or mid-transformation.
pub fn monkeyize(world: &mut World, mob: Entity, instant: bool) -> bool {
    if is_transforming(world, mob) || is_monkey(world, mob) {
        return false;
    }
    if instant {
        finish_monkeyize(world, mob);
    } else {
        begin(world, mob, Change::Monkey, "h2monkey");
    }
    true
}

/// tg `humanize(species, instant)`: false unless a monkey that is not mid-transformation.
pub fn humanize(world: &mut World, mob: Entity, species: SpeciesId, instant: bool) -> bool {
    if is_transforming(world, mob) || !is_monkey(world, mob) {
        return false;
    }
    if instant {
        finish_humanize(world, mob, species);
    } else {
        begin(world, mob, Change::Human(species), "monkey2h");
    }
    true
}

/// `ADD_TRAIT(NO_TRANSFORM)`, `Stun(ignore_canstun)`, the animation on the tile; the body is
/// hidden by [`hide_transforming`] until [`finish_transformations`].
fn begin(world: &mut World, mob: Entity, into: Change, state: &'static str) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(Transforming {
        at: now + TRANSFORMATION_SECS,
        into,
    });
    if let Some(mut effects) = world.get_mut::<StatusEffects>(mob) {
        effects.apply(Incap::Stun, TRANSFORMATION_SECS as f32);
    }
    if let Some(pos) = turf_of(world, mob) {
        temp_visual::spawn(world, pos, Visual::new(MOB_DMI, state, TRANSFORMATION_SECS));
    }
}

/// The body stays hidden whenever its appearance is rebuilt (`icon = null`, invisibility).
fn hide_transforming(mut mobs: Query<&mut Appearance, (With<Transforming>, Changed<Appearance>)>) {
    for mut appearance in &mut mobs {
        if appearance.color != Some([255, 255, 255, 0]) {
            appearance.color = Some([255, 255, 255, 0]);
        }
    }
}

fn finish_transformations(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(Entity, Change)> = world
        .query::<(Entity, &Transforming)>()
        .iter(world)
        .filter(|(_, t)| t.at <= now)
        .map(|(e, t)| (e, t.into))
        .collect();
    for (mob, into) in due {
        world.entity_mut(mob).remove::<Transforming>();
        if let Some(mut appearance) = world.get_mut::<Appearance>(mob) {
            appearance.color = None;
        }
        match into {
            Change::Monkey => finish_monkeyize(world, mob),
            Change::Human(species) => finish_humanize(world, mob, species),
        }
    }
}

/// tg `finish_monkeyize`: `crate::species::set_species` says "Вы теперь ...".
fn finish_monkeyize(world: &mut World, mob: Entity) {
    crate::species::set_species(world, mob, SpeciesId::Monkey);
    // `name = LOWER_TEXT(dna.species.name)`.
    let name = SpeciesId::Monkey.def().name.to_lowercase();
    world.entity_mut(mob).insert(Name::new(name));
    if let Some(pos) = turf_of(world, mob) {
        crate::restraints::uncuff(world, mob, pos);
    }
}

/// tg `finish_humanize`.
// shortcut: no underwear reset (`human/finish_humanize`): underwear is not ported.
fn finish_humanize(world: &mut World, mob: Entity, species: SpeciesId) {
    crate::species::set_species(world, mob, species);
    crate::inventory::refresh_look(world, mob);
}

/// tg `transform_into_mob(mob_path, delete_items)`: a `path` mob on the old one's turf takes its
/// mind (a mindless mob's body is simply replaced); the old one is gone. `None` when the path
/// has no prototype or the mob is mid-transformation.
pub fn transform_into_mob(
    world: &mut World,
    mob: Entity,
    path: &str,
    delete_items: bool,
) -> Option<Entity> {
    if is_transforming(world, mob) {
        return None;
    }
    let pos = turf_of(world, mob)?;
    let new = crate::mobs::spawn::spawn_mob(world, path, pos)?;
    if crate::medical::brainmob::mind_of(world, mob).is_some()
        && !crate::medical::brainmob::transfer_mind(world, mob, new)
    {
        world.despawn(new);
        return None;
    }
    let carried = crate::health::carried_items(world, mob);
    if delete_items {
        crate::health::despawn_all(world, carried);
    } else {
        for item in carried {
            crate::inventory::put_down(world, item, pos);
        }
    }
    world.despawn(mob);
    crate::construction::tile_changed(world, pos);
    Some(new)
}

fn into_mob_says(world: &mut World, mob: Entity, path: &str, text: &str) -> Option<Entity> {
    let new = transform_into_mob(world, mob, path, false)?;
    if let Some(mut combat) = world.get_mut::<CombatMode>(new) {
        combat.0 = true;
    }
    tell_mob(world, new, Span::BoldNotice, text);
    Some(new)
}

/// tg `corgize`.
pub fn corgize(world: &mut World, mob: Entity) -> Option<Entity> {
    into_mob_says(world, mob, CORGI, "Вы теперь корги. Йап-йап!")
}

/// tg `crabize`.
pub fn crabize(world: &mut World, mob: Entity) -> Option<Entity> {
    into_mob_says(world, mob, CRAB, "Вы эволюционировали в краба!")
}

/// tg `gorillize(genetics_gorilla)`.
pub fn gorillize(world: &mut World, mob: Entity, genetics_gorilla: bool) -> Option<Entity> {
    let path = if genetics_gorilla {
        GORILLA_GENETICS
    } else {
        GORILLA
    };
    into_mob_says(world, mob, path, "Вы теперь горилла. Уга-уга!")
}

/// tg `slimeize(reproduce)`: `reproduce` leaves `pick(1400;2, 3, 4) - 1` mindless slimes too.
pub fn slimeize(world: &mut World, mob: Entity, reproduce: bool) -> Option<Entity> {
    let new = into_mob_says(world, mob, SLIME, "Вы теперь слайм. Скриии!")?;
    if reproduce {
        let roll = world.resource_mut::<CombatRng>().range(0, 1401);
        let number = match roll {
            1400 => 3,
            1401 => 4,
            _ => 2,
        };
        let pos = turf_of(world, new)?;
        for _ in 1..number {
            crate::mobs::spawn::spawn_mob(world, SLIME, pos);
        }
    }
    Some(new)
}

/// tg `Alienize`: a hunter, sentinel or drone.
pub fn alienize(world: &mut World, mob: Entity) -> Option<Entity> {
    use crate::antag::xeno::Caste;
    let caste = [Caste::Hunter, Caste::Sentinel, Caste::Drone]
        [world.resource_mut::<CombatRng>().range(0, 2) as usize];
    let new = transform_into_mob(world, mob, caste.path(), false)?;
    crate::antag::xeno::make_xeno(world, new, false);
    tell_mob(world, new, Span::BoldNotice, "Вы теперь ксеноморф.");
    Some(new)
}
