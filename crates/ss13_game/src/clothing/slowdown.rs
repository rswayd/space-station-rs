//! Equipment slowdown after tg `/mob/proc/update_equipment_speed_mods` (`code/modules/mob/mob.dm`):
//! the `slowdown` of every worn item (pockets and suit storage excluded, held items only with
//! `SLOWS_WHILE_IN_HAND`) is summed into `/datum/movespeed_modifier/equipment_speedmod`, which
//! a floating (weightless) mob ignores; `IMMUTABLE_SLOW` items go into the `immutable` modifier
//! that applies always. The sum lands in [`EquipmentSlowdown`], which `crate::health` adds to
//! the step delay.
//!
//! Not ported: `TRAIT_STURDY_FRAME` (settler quirk, no quirk traits in the port),
//! `TRAIT_IGNORESLOWDOWN` (no such trait source yet). MOD parts have their own modifier
//! (`crate::modsuit::server::SuitSlowdown`), their static `slowdown` is 0.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::protocol::InventorySlot;

use crate::inventory::{ALL_SLOTS, Inventory};

/// `item_flags` `SLOWS_WHILE_IN_HAND` (`code/__DEFINES/obj_flags.dm`).
pub const SLOWS_WHILE_IN_HAND: u32 = 1 << 3;
/// `item_flags` `IMMUTABLE_SLOW`.
pub const IMMUTABLE_SLOW: u32 = 1 << 8;

/// On a mob: its equipment's slowdown, deciseconds per tile (negative: faster).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct EquipmentSlowdown(pub f32);

/// `get_equipped_speed_mod_items`: worn slots without the pockets and suit storage, plus held
/// items that slow while held.
fn counts(slot: InventorySlot, item_flags: u32) -> bool {
    match slot {
        InventorySlot::LeftPocket | InventorySlot::RightPocket | InventorySlot::SuitStorage => {
            false
        }
        InventorySlot::LeftHand | InventorySlot::RightHand => item_flags & SLOWS_WHILE_IN_HAND != 0,
        _ => true,
    }
}

/// The equipment slowdown of `inventory`: (`speedies`, `immutable_speedies`) summed, the first
/// dropped while `floating` (`blacklisted_movetypes = FLOATING`).
pub fn equipment_slowdown<'a>(
    inventory: &Inventory,
    item: impl Fn(Entity) -> Option<&'a DmItem>,
    floating: bool,
) -> f32 {
    let (mut speedies, mut immutable) = (0.0, 0.0);
    for slot in ALL_SLOTS {
        let Some(dm) = inventory.slot(slot).and_then(&item) else {
            continue;
        };
        if !counts(slot, dm.item_flags) {
            continue;
        }
        if dm.item_flags & IMMUTABLE_SLOW != 0 {
            immutable += dm.slowdown;
        } else {
            speedies += dm.slowdown;
        }
    }
    if floating {
        immutable
    } else {
        speedies + immutable
    }
}

/// Keeps [`EquipmentSlowdown`] in step with what every mob wears.
// shortcut: recomputed for every mob with an inventory each tick (17 lookups per mob); switch
// to change detection on `Inventory`/`DmItem` if mob counts grow into the thousands.
fn refresh(
    mobs: Query<(
        Entity,
        &Inventory,
        Has<crate::spacemove::Weightless>,
        Option<&EquipmentSlowdown>,
        Option<&crate::jetpack::GasJetpack>,
        Option<&crate::legcuffs::Legcuffed>,
    )>,
    items: Query<&DmItem>,
    mut commands: Commands,
) {
    for (mob, inventory, floating, old, jetpack, legcuffed) in &mobs {
        let mut new = equipment_slowdown(inventory, |e| items.get(e).ok(), floating);
        // `legcuffed` counts as worn (`get_equipped_speed_mod_items`); floating drops it like the
        // rest of the non-immutable sum.
        if !floating && let Some(cuffs) = legcuffed.and_then(|l| items.get(l.0).ok()) {
            new += cuffs.slowdown;
        }
        if floating {
            new += crate::jetpack::speed_modifier(jetpack);
        }
        match old {
            Some(old) if old.0 == new => {}
            None if new == 0.0 => {}
            _ => {
                commands.entity(mob).insert(EquipmentSlowdown(new));
            }
        }
    }
}

pub fn build(app: &mut App) {
    app.add_systems(Update, refresh.before(crate::health::HealthSystems));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worn_items_slow_pockets_and_hands_do_not() {
        let mut world = World::new();
        let item = |slowdown: f32, item_flags: u32| DmItem {
            slowdown,
            item_flags,
            ..default()
        };
        let suit = world.spawn(item(1.0, 0)).id();
        let boots = world.spawn(item(-0.5, 0)).id();
        let pocket = world.spawn(item(3.0, 0)).id();
        let held = world.spawn(item(2.0, 0)).id();
        let bola = world.spawn(item(4.0, SLOWS_WHILE_IN_HAND)).id();
        let cursed = world.spawn(item(0.25, IMMUTABLE_SLOW)).id();
        let inventory = Inventory {
            suit: Some(suit),
            shoes: Some(boots),
            pockets: [Some(pocket), None],
            hands: [Some(held), Some(bola)],
            belt: Some(cursed),
            ..default()
        };
        let get = |e| world.get::<DmItem>(e);
        assert_eq!(equipment_slowdown(&inventory, get, false), 4.75);
        // Floating: only the immutable modifier stays.
        assert_eq!(equipment_slowdown(&inventory, get, true), 0.25);
        assert_eq!(equipment_slowdown(&Inventory::default(), get, false), 0.0);
    }
}
