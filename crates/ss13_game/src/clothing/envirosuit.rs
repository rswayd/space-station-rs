//! Plasma envirosuits' built-in extinguisher (server), after tg
//! `code/modules/clothing/spacesuits/plasmamen.dm` (`/obj/item/clothing/suit/space/eva/
//! plasmaman`) and its jumpsuit counterpart (`/obj/item/clothing/under/plasmaman`,
//! `under/jobs/Plasmaman/civilian_service.dm`).
//!
//! - Worn in its slot (the EVA suit as the suit, the jumpsuit as the uniform) on a human that is
//!   on fire and sealed against plasma ignition (`is_atmos_sealed(PLASMAMAN_PREVENT_IGNITION,
//!   check_hands = TRUE)`), the suit puts the wearer out (`check_fire_state`): one charge (EVA:
//!   10, jumpsuit: 5) and a 10 s cooldown, after which it checks again.
//! - Examine shows the charges left; an envirosuit extinguisher cartridge used on the suit
//!   refills it (the cartridge is used up), unless it is full.
// shortcut: no water particle on the extinguish (`/obj/effect/particle_effect/water`); the
// check runs each tick over worn envirosuits instead of on `COMSIG_LIVING_IGNITED` and
// equips. The job jumpsuits' special `activate_fire_safety` (the clown's lube foam) is not
// ported: they extinguish like the plain one.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::InventorySlot;

use crate::inventory::{CarriedBy, Inventory};

pub const EVA_SUIT: &str = "/obj/item/clothing/suit/space/eva/plasmaman";
pub const JUMPSUIT: &str = "/obj/item/clothing/under/plasmaman";
pub const REFILL: &str = "/obj/item/extinguisher_refill";
/// `extinguish_cooldown` (100 ds), seconds.
pub const COOLDOWN: f64 = 10.0;
/// `PLASMAMAN_PREVENT_IGNITION` (`clothing_flags`).
const PLASMAMAN_PREVENT_IGNITION: u32 = 1 << 10;

/// The suit's extinguisher (`extinguishes_left`, `extinguish_timer`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Envirosuit {
    pub left: u32,
    pub max: u32,
    /// Game time the cooldown ends.
    pub ready_at: f64,
    /// The slot it works from.
    pub slot: InventorySlot,
}

/// The extinguisher of an envirosuit type: (charges, slot).
pub fn kind_of(path: &str) -> Option<(u32, InventorySlot)> {
    if is_under(path, EVA_SUIT) {
        Some((10, InventorySlot::Suit))
    } else if is_under(path, JUMPSUIT) {
        Some((5, InventorySlot::Uniform))
    } else {
        None
    }
}

pub(super) fn build(app: &mut App) {
    app.add_observer(add_envirosuit).add_systems(
        Update,
        check_fire_state.run_if(any_with_component::<Envirosuit>),
    );
}

fn add_envirosuit(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Some((max, slot)) = items.get(add.entity).ok().and_then(|i| kind_of(&i.path)) else {
        return;
    };
    commands.entity(add.entity).insert_if_new(Envirosuit {
        left: max,
        max,
        ready_at: 0.0,
        slot,
    });
}

/// `check_fire_state` of every worn envirosuit.
fn check_fire_state(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &Envirosuit, &CarriedBy)>();
    let worn: Vec<(Entity, Envirosuit, Entity)> = q
        .iter(world)
        .filter(|(_, s, _)| s.left > 0 && now >= s.ready_at)
        .map(|(e, s, c)| (e, *s, c.0))
        .collect();
    for (suit, state, owner) in worn {
        let Some(inventory) = world.get::<Inventory>(owner) else {
            continue;
        };
        if inventory.slot(state.slot) != Some(suit) || !crate::fire::on_fire(world, owner) {
            continue;
        }
        let sealed = crate::breath::Worn::read(world, inventory)
            .atmos_sealed(PLASMAMAN_PREVENT_IGNITION, true);
        if !sealed {
            continue;
        }
        if let Some(mut s) = world.get_mut::<Envirosuit>(suit) {
            s.left -= 1;
            s.ready_at = now + COOLDOWN;
        }
        let Some(&at) = world.get::<TilePos>(owner) else {
            continue;
        };
        let who = crate::security::name_of(world, owner);
        crate::security::show(
            world,
            at,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Warning,
                format!("Костюм {who} автоматически тушит его!"),
            ),
            &[(
                owner,
                Span::Warning,
                "Ваш костюм автоматически тушит вас.".to_owned(),
            )],
        );
        crate::fire::extinguish_mob(world, owner);
    }
}

/// `examine`: the charges left.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(s) = world.get::<Envirosuit>(item) {
        lines.push((
            Span::Notice,
            format!("Зарядов огнетушителя в костюме: {}.", s.left),
        ));
    }
}

/// `item_interaction` with an extinguisher cartridge: a refill.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(state) = world.get::<Envirosuit>(target).copied() else {
        return false;
    };
    if !world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, REFILL))
    {
        return false;
    }
    if state.left == state.max {
        crate::tools::tell(
            world,
            client,
            Span::Notice,
            "Встроенный огнетушитель полон.",
        );
        return true;
    }
    if let Some(mut s) = world.get_mut::<Envirosuit>(target) {
        s.left = s.max;
    }
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        "Вы перезаряжаете встроенный огнетушитель костюма, израсходовав картридж.",
    );
    world.despawn(held);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charges_and_slots() {
        assert_eq!(kind_of(EVA_SUIT), Some((10, InventorySlot::Suit)));
        assert_eq!(
            kind_of("/obj/item/clothing/under/plasmaman/chef"),
            Some((5, InventorySlot::Uniform))
        );
        assert_eq!(kind_of("/obj/item/clothing/suit/space/eva"), None);
    }
}
