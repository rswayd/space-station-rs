//! Melee weapon mechanics shared by many items (server): two-handed wielding
//! ([`two_handed`]), the double-bladed energy sword ([`energy`]), blocking and shields
//! ([`block`]) and whetstones ([`sharpener`]).

use bevy::prelude::*;

pub mod block;
pub mod energy;
pub mod sharpener;
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
