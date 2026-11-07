//! Action speed modifiers (server), tg `code/modules/actionspeed/_actionspeed_modifier.dm`.
//!
//! A mob's [`ActionSpeedModifiers`] is the `actionspeed_modification` list: one
//! `multiplicative_slowdown` per id (a new one with the same id replaces the old). Every
//! `do_after` (`crate::tools::do_after`) scales its delay by [`action_multiplier`]: the base
//! modifier (1) plus every listed slowdown, the sanity tier (`crate::mood`) and the food haste
//! (`crate::buffs`) — `cached_multiplicative_actions_slowdown`.
// shortcut: no `conflicts_with` and no `actionspeed_mod_immunities` (no ported source uses
// them yet); add them with the first source that does.

use bevy::prelude::*;

/// `/datum/actionspeed_modifier/status_effect/midas_blight/*` (`ACTIONSPEED_ID_MIDAS_BLIGHT`).
pub const ID_MIDAS_BLIGHT: &str = "midas_blight";

/// `mob.actionspeed_modification`: `(id, multiplicative_slowdown)`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ActionSpeedModifiers(pub Vec<(&'static str, f32)>);

impl ActionSpeedModifiers {
    /// The sum of the listed slowdowns (without the base 1).
    pub fn total(&self) -> f32 {
        self.0.iter().map(|&(_, s)| s).sum()
    }

    /// `add_actionspeed_modifier`: replaces a modifier with the same id.
    pub fn add(&mut self, id: &'static str, slowdown: f32) {
        match self.0.iter_mut().find(|(k, _)| *k == id) {
            Some(entry) => entry.1 = slowdown,
            None => self.0.push((id, slowdown)),
        }
    }

    /// `remove_actionspeed_modifier`: false when it was not there.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.0.len();
        self.0.retain(|(k, _)| *k != id);
        self.0.len() != before
    }

    /// `has_actionspeed_modifier`.
    pub fn get(&self, id: &str) -> Option<f32> {
        self.0.iter().find(|(k, _)| *k == id).map(|&(_, s)| s)
    }
}

/// tg `add_actionspeed_modifier(mob, id)` / `add_or_update_variable_actionspeed_modifier`.
pub fn add_actionspeed_modifier(world: &mut World, mob: Entity, id: &'static str, slowdown: f32) {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    match entity.get_mut::<ActionSpeedModifiers>() {
        Some(mut mods) => mods.add(id, slowdown),
        None => {
            let mut mods = ActionSpeedModifiers::default();
            mods.add(id, slowdown);
            entity.insert(mods);
        }
    }
}

/// tg `remove_actionspeed_modifier(mob, id)`.
pub fn remove_actionspeed_modifier(world: &mut World, mob: Entity, id: &str) -> bool {
    world
        .get_mut::<ActionSpeedModifiers>(mob)
        .is_some_and(|mut mods| mods.remove(id))
}

/// `cached_multiplicative_actions_slowdown`: what a `do_after` delay is multiplied by (never
/// below 0).
pub fn action_multiplier(world: &World, mob: Entity) -> f32 {
    // The base modifier (1) is folded into the mood multiplier (1 without mood).
    let sanity = world
        .get::<crate::mood::Mood>(mob)
        .map_or(1.0, |m| m.action_multiplier());
    let listed = world
        .get::<ActionSpeedModifiers>(mob)
        .map_or(0.0, ActionSpeedModifiers::total);
    (sanity + crate::buffs::action_slowdown(world, mob) + listed).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_sum_replace_by_id_and_remove() {
        let mut world = World::new();
        let mob = world.spawn_empty().id();
        assert_eq!(action_multiplier(&world, mob), 1.0);
        add_actionspeed_modifier(&mut world, mob, ID_MIDAS_BLIGHT, 0.25);
        add_actionspeed_modifier(
            &mut world,
            mob,
            "/datum/actionspeed_modifier/status_effect/hazard_area",
            4.0,
        );
        assert_eq!(action_multiplier(&world, mob), 5.25);
        // Same id: the gold tier overwrites the soft one.
        add_actionspeed_modifier(&mut world, mob, ID_MIDAS_BLIGHT, 2.0);
        assert_eq!(action_multiplier(&world, mob), 7.0);
        assert!(remove_actionspeed_modifier(
            &mut world,
            mob,
            "/datum/actionspeed_modifier/status_effect/hazard_area"
        ));
        assert!(!remove_actionspeed_modifier(
            &mut world,
            mob,
            "/datum/actionspeed_modifier/status_effect/hazard_area"
        ));
        assert_eq!(action_multiplier(&world, mob), 3.0);
        // A speedup never makes the delay negative.
        add_actionspeed_modifier(&mut world, mob, ID_MIDAS_BLIGHT, -5.0);
        assert_eq!(action_multiplier(&world, mob), 0.0);
    }
}
