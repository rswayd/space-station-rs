//! BandaStation `/datum/component/stamina_cost_per_hit`
//! (`modular_bandastation/~components/code/stamina_cost_per_hit.dm`): an attack with the item
//! costs stamina, by target (living or not) and wield; an attack that would put the attacker
//! into stamina crit is refused ("нет сил!"). Its only carriers are the fork's sledgehammers
//! (`modular_bandastation/weapon/code/melee/sledgehammer.dm`).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;

use crate::health::{HEALTH_THRESHOLD_CRIT, MAX_HEALTH};

/// `stamina_cost`, `stamina_cost_wielded`, `stamina_cost_on_atom`,
/// `stamina_cost_wielded_on_atom` (nulls resolved as `Initialize` does).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Costs {
    pub root: &'static str,
    pub living: f32,
    pub living_wielded: f32,
    pub atom: f32,
    pub atom_wielded: f32,
}

/// The longest matching root wins (the D-6 replaces its parent's component).
pub const COSTS: [Costs; 2] = [
    Costs {
        root: "/obj/item/sledgehammer",
        living: 12.0,
        living_wielded: 10.0,
        atom: 10.0,
        atom_wielded: 8.0,
    },
    Costs {
        root: "/obj/item/sledgehammer/syndie",
        living: 0.0,
        living_wielded: 0.0,
        atom: 0.0,
        atom_wielded: 0.0,
    },
];

/// The stamina one attack with an item of `path` costs, `None` without the component.
pub fn cost_per_hit(path: &str, wielded: bool, living: bool) -> Option<f32> {
    let c = COSTS
        .iter()
        .filter(|c| is_under(path, c.root))
        .max_by_key(|c| c.root.len())?;
    Some(match (living, wielded) {
        (true, false) => c.living,
        (true, true) => c.living_wielded,
        (false, false) => c.atom,
        (false, true) => c.atom_wielded,
    })
}

/// `(maxHealth - (get_stamina_loss() + cost)) <= crit_threshold`.
pub fn exhausted(stamina_loss: f32, cost: f32) -> bool {
    MAX_HEALTH - (stamina_loss + cost) <= HEALTH_THRESHOLD_CRIT
}

/// The stamina an attack with `mob`'s held `item` takes at the moment of the attack (0
/// without the component); `None` when too tired: the attack is refused.
pub fn charge(world: &World, mob: Entity, item: Entity, living: bool) -> Option<f32> {
    let Some(path) = world.get::<DmItem>(item).map(|d| d.path.as_str()) else {
        return Some(0.0);
    };
    let wielded = crate::weapons::two_handed::is_wielded(world, item);
    let Some(cost) = cost_per_hit(path, wielded, living) else {
        return Some(0.0);
    };
    let loss = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .map_or(0.0, |s| s.stamina);
    (!exhausted(loss, cost)).then_some(cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sledgehammers_cost_stamina_by_target_and_wield() {
        let hammer = "/obj/item/sledgehammer";
        assert_eq!(cost_per_hit(hammer, false, true), Some(12.0));
        assert_eq!(cost_per_hit(hammer, true, true), Some(10.0));
        assert_eq!(cost_per_hit(hammer, false, false), Some(10.0));
        assert_eq!(cost_per_hit(hammer, true, false), Some(8.0));
        assert_eq!(
            cost_per_hit("/obj/item/sledgehammer/tactical", true, false),
            Some(8.0)
        );
        assert_eq!(
            cost_per_hit("/obj/item/sledgehammer/syndie", false, true),
            Some(0.0)
        );
        assert_eq!(cost_per_hit("/obj/item/fireaxe", true, true), None);
        assert!(!exhausted(80.0, 12.0));
        assert!(exhausted(90.0, 10.0));
        assert!(exhausted(95.0, 10.0));
    }
}
