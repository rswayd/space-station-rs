//! Bioware (server), after tg `code/datums/status_effects/buffs/bioware` (`/datum/status_effect/
//! bioware`: `circulation.dm`, `nerves.dm`, `cortex.dm`) given by the bioware operations of
//! `code/modules/surgery/operations/operation_bioware.dm` ([`super::surgery`]).
//!
//! A human keeps its bioware ([`Biowares`]) for good, at most one of each status effect `id`
//! ([`Bioware::category`]). Threaded veins bleed a quarter (`PHYS_COEFF_BLEED`, read by
//! [`super::blood`]); muscled veins pump blood without a heart (`TRAIT_STABLEHEART`,
//! [`super::organs::cardiac_arrest`]); spliced nerves halve stuns and take 80% stamina damage
//! (`PHYS_COEFF_STUN`, `STAMINA`, read by `crate::status_effects`); grounded nerves are immune
//! to shocks (`TRAIT_SHOCKIMMUNE`, `crate::wires`); an imprinted cortex cures and blocks basic
//! brain traumas ([`super::traumas`]).
// shortcut: no ligament hook or reinforcement (dismemberment reads only the species'
// `TRAIT_NODISMEMBER`), no cortex folding (no
// special trauma boost), no mechanical variants (robotic bodyparts); the stun multiplier
// covers `crate::status_effects::apply`/`knockdown`, not effects set on the component directly.

use bevy::prelude::*;

use super::traumas::Resilience;
use crate::zone::TargetZone;

/// The ported bioware status effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bioware {
    /// `/datum/status_effect/bioware/heart/threaded_veins`.
    VeinThreading,
    /// `/datum/status_effect/bioware/heart/muscled_veins`.
    MuscledVeins,
    /// `/datum/status_effect/bioware/nerves/spliced`.
    NerveSplicing,
    /// `/datum/status_effect/bioware/nerves/grounded`.
    NerveGrounding,
    /// `/datum/status_effect/bioware/cortex/imprinted`.
    CortexImprint,
}

impl Bioware {
    pub const ALL: [Self; 5] = [
        Self::VeinThreading,
        Self::MuscledVeins,
        Self::NerveSplicing,
        Self::NerveGrounding,
        Self::CortexImprint,
    ];

    /// The status effect `id`: a mob takes one bioware of each.
    pub fn category(self) -> &'static str {
        match self {
            Self::VeinThreading | Self::MuscledVeins => "circulation",
            Self::NerveSplicing | Self::NerveGrounding => "nerves",
            Self::CortexImprint => "cortex",
        }
    }

    /// The operation's `required_zone`.
    pub fn zone(self) -> TargetZone {
        match self {
            Self::CortexImprint => TargetZone::Head,
            _ => TargetZone::Chest,
        }
    }
}

/// A human's bioware.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Biowares(pub Vec<Bioware>);

pub fn has(world: &World, mob: Entity, kind: Bioware) -> bool {
    world
        .get::<Biowares>(mob)
        .is_some_and(|b| b.0.contains(&kind))
}

/// The bioware operation's `state_check`: no bioware of the same `id` yet.
pub fn can_gain(world: &World, mob: Entity, kind: Bioware) -> bool {
    !world
        .get::<Biowares>(mob)
        .is_some_and(|b| b.0.iter().any(|k| k.category() == kind.category()))
}

/// `apply_status_effect(status_effect_gained)` and its `bioware_gained`.
pub fn gain(world: &mut World, mob: Entity, kind: Bioware) {
    if !can_gain(world, mob, kind) {
        return;
    }
    let mut all = world.get::<Biowares>(mob).cloned().unwrap_or_default();
    all.0.push(kind);
    world.entity_mut(mob).insert(all);
    if kind == Bioware::CortexImprint
        && let Some(brain) = super::organs::organ(world, mob, super::organs::Slot::Brain)
    {
        super::traumas::cure_all(world, brain, Resilience::Basic);
    }
}

/// `GET_PHYSIOLOGY(mob, PHYS_COEFF_BLEED)`.
pub fn bleed_mod(world: &World, mob: Entity) -> f32 {
    let veins = if has(world, mob, Bioware::VeinThreading) {
        0.25
    } else {
        1.0
    };
    veins * crate::debuffs::bleed_mod(world, mob)
}

/// `GET_PHYSIOLOGY(mob, PHYS_COEFF_STUN)`.
pub fn stun_mod(world: &World, mob: Entity) -> f32 {
    if has(world, mob, Bioware::NerveSplicing) {
        0.5
    } else {
        1.0
    }
}

/// `GET_PHYSIOLOGY(mob, STAMINA)` (stamina damage taken).
pub fn stamina_mod(world: &World, mob: Entity) -> f32 {
    if has(world, mob, Bioware::NerveSplicing) {
        0.8
    } else {
        1.0
    }
}

/// `TRAIT_STABLEHEART`: no heart needed.
pub fn stable_heart(world: &World, mob: Entity) -> bool {
    has(world, mob, Bioware::MuscledVeins)
}

/// `TRAIT_SHOCKIMMUNE`.
pub fn shock_immune(world: &World, mob: Entity) -> bool {
    has(world, mob, Bioware::NerveGrounding)
}

/// The imprinted cortex's `COMSIG_CARBON_BLOCK_TRAUMA`: basic traumas do not take.
pub fn blocks_trauma(world: &World, mob: Entity, resilience: Resilience) -> bool {
    resilience <= Resilience::Basic && has(world, mob, Bioware::CortexImprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_bioware_of_each_kind() {
        let mut world = World::new();
        let mob = world.spawn_empty().id();
        assert!(can_gain(&world, mob, Bioware::VeinThreading));
        gain(&mut world, mob, Bioware::VeinThreading);
        assert_eq!(bleed_mod(&world, mob), 0.25);
        // Muscled veins are circulation bioware too: refused.
        gain(&mut world, mob, Bioware::MuscledVeins);
        assert!(!stable_heart(&world, mob));
        gain(&mut world, mob, Bioware::NerveGrounding);
        assert!(shock_immune(&world, mob) && stun_mod(&world, mob) == 1.0);
        assert!(!can_gain(&world, mob, Bioware::NerveSplicing));
        assert!(!blocks_trauma(&world, mob, Resilience::Basic));
    }
}
