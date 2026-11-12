//! Melee weapon mechanics shared by many items (server): two-handed wielding
//! ([`two_handed`]), the double-bladed energy sword ([`energy`]), blocking and shields
//! ([`block`]), whetstones ([`sharpener`]), the fork's sledgehammer components
//! ([`stamina_cost`], [`rip_and_tear`]), the baseball bat's kneecapping ([`kneecap`]), the
//! fork's backstabs ([`backstab`]), the power-fist and the supermatter sword ([`special`]).

use bevy::prelude::*;

pub mod backstab;
pub mod block;
pub mod energy;
pub mod kneecap;
pub mod rip_and_tear;
pub mod sharpener;
pub mod special;
pub mod stamina_cost;
pub mod two_handed;

pub struct WeaponsPlugin;

impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A two-handed item wields or lets go.
        app.add_click_handler(ClickPhase::AttackSelf, 3221, |w, c| {
            two_handed::attack_self(w, c.client, c.mob, c.target)
        });
        // The held item on a whetstone or a shield in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 1261, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        // The held item on a whetstone or a shield on the floor or a table.
        app.add_click_handler(ClickPhase::HeldOnTarget, 251, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        // The supermatter sword consumes an object or a turf (`special`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 25, |w, c| {
            c.held
                .is_some_and(|h| special::sm_pre_attack(w, c.mob, h, c.target))
        });
        // A tank, wrench or screwdriver on a power-fist on the floor or a table.
        app.add_click_handler(ClickPhase::HeldOnTarget, 35, |w, c| {
            c.held
                .is_some_and(|h| special::powerfist_interaction(w, c.client, c.mob, h, c.target))
        });
        // ... or in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 1265, |w, c| {
            c.held
                .is_some_and(|h| special::powerfist_interaction(w, c.client, c.mob, h, c.target))
        });
        // A right click with a shield or a pillow shoves (`/datum/element/disarm_attack`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 247, |w, c| {
            c.modifiers.right && c.held.is_some_and(|h| disarm_attack(w, c.mob, h, c.target))
        });
        // A wielded sledgehammer on a wall (`rip_and_tear`, `COMSIG_ITEM_PRE_ATTACK`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 4491, |w, c| {
            c.held
                .is_some_and(|h| rip_and_tear::pre_attack(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(
            Update,
            two_handed::maintain.after(crate::click::ResolveClicks),
        );
    }
}

/// `/obj/item/fireaxe`.
pub const FIREAXE: &str = "/obj/item/fireaxe";

/// `/obj/item/fireaxe/afterattack`: a wielded fireaxe on a window or a grille.
pub fn fireaxe_destroys(world: &World, item: Entity, target_path: &str) -> bool {
    use ss13_mapload::fidelity::is_under;
    world
        .get::<ss13_mapload::DmItem>(item)
        .is_some_and(|d| is_under(&d.path, FIREAXE))
        && two_handed::is_wielded(world, item)
        && (is_under(target_path, "/obj/structure/window")
            || is_under(target_path, "/obj/structure/grille"))
}

/// `item_interaction` of whetstones and shields with `tool`.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    target: Entity,
) -> bool {
    tool != target
        && (sharpener::item_interaction(world, client, mob, tool, target)
            || block::bash(world, mob, tool, target))
}

/// `/datum/element/disarm_attack` (`secondary_attack`): a shield or a pillow shoves like an
/// empty hand's right click. True when the click is taken.
// shortcut: the energy and telescopic shields are always out here (they do not fold), so
// their "activate it first" refusal never happens.
pub fn disarm_attack(world: &mut World, user: Entity, item: Entity, victim: Entity) -> bool {
    use ss13_mapload::fidelity::is_under;
    let Some(dm) = world.get::<ss13_mapload::DmItem>(item).cloned() else {
        return false;
    };
    let pillow = is_under(&dm.path, "/obj/item/pillow");
    if !(pillow || is_under(&dm.path, "/obj/item/shield"))
        || user == victim
        || world.get::<crate::health::Health>(victim).is_none()
    {
        return false;
    }
    // `/obj/item/pillow/can_disarm_attack`: a pillow smothers whoever it could.
    if pillow {
        let lying = world
            .get::<crate::health::BodyPosition>(victim)
            .is_some_and(|b| b.lying());
        let grabbed = world.get::<crate::pull::Pulling>(user).map(|p| p.0) == Some(victim)
            && world
                .get::<crate::pull::GrabState>(user)
                .is_some_and(|g| *g >= crate::pull::GrabState::Aggressive);
        if lying || grabbed {
            return false;
        }
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(user).insert(crate::combat::NextMove(
        now + f64::from(dm.attack_speed) / 10.0,
    ));
    let name = crate::security::name_of(world, item);
    if !crate::martial::check_block(
        world,
        victim,
        Some(user),
        crate::martial::AttackKind::Melee,
        0.0,
        &name,
    ) {
        crate::security::shove(world, user, victim);
    }
    true
}
