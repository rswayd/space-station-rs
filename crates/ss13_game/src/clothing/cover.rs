//! What worn clothing hides and covers: tg `flags_inv` and the carbon's `obscured_slots`
//! (`carbon/inventory.dm` `refresh_obscured`, `hidden_slots_to_inventory_slots`), its effect on
//! the worn sprites (`human_update_icons.dm`), hair and eyes (`head_hair_and_lips.dm`,
//! `_eyes.dm`) and the face (`get_visible_name`, `is_face_obscured`), and `flags_cover`
//! (`is_mouth_covered`, `is_eyes_covered`, `is_pepper_proof` in `human_defense.dm`).
//!
//! The flags are read live from each item's [`DmClothing`] (a visor or mask pushed up changes
//! them, `crate::clothing::toggles`), so nothing is cached on the mob: the look is rebuilt when
//! the inventory changes and the checks are cheap ORs over at most 13 slots.

use bevy::prelude::*;
use ss13_mapload::DmClothing;
use ss13_shared::protocol::InventorySlot;

use crate::human::HumanLook;
use crate::inventory::Inventory;

/// `flags_inv` bits (`code/__DEFINES/inventory.dm`).
pub const HIDEGLOVES: u32 = 1 << 0;
pub const HIDESUITSTORAGE: u32 = 1 << 1;
pub const HIDEJUMPSUIT: u32 = 1 << 2;
pub const HIDESHOES: u32 = 1 << 3;
pub const HIDEMASK: u32 = 1 << 4;
pub const HIDEEARS: u32 = 1 << 5;
pub const HIDEEYES: u32 = 1 << 6;
pub const HIDEFACE: u32 = 1 << 7;
pub const HIDEHAIR: u32 = 1 << 8;
pub const HIDEFACIALHAIR: u32 = 1 << 9;
pub const HIDENECK: u32 = 1 << 10;
pub const HIDEHEADGEAR: u32 = 1 << 11;
pub const HIDEBELT: u32 = 1 << 14;

/// `flags_cover` bits.
pub const GLASSESCOVERSEYES: u32 = 1 << 0;
pub const MASKCOVERSEYES: u32 = 1 << 1;
pub const HEADCOVERSEYES: u32 = 1 << 2;
pub const MASKCOVERSMOUTH: u32 = 1 << 3;
pub const HEADCOVERSMOUTH: u32 = 1 << 4;
pub const PEPPERPROOF: u32 = 1 << 5;

/// BandaStation's `get_face_name` default.
pub const UNKNOWN_NAME: &str = "Неизвестный";

/// The slots of tg `get_equipped_items()` (everything worn but the pockets).
const WORN: [InventorySlot; 13] = [
    InventorySlot::Uniform,
    InventorySlot::Suit,
    InventorySlot::SuitStorage,
    InventorySlot::Head,
    InventorySlot::Mask,
    InventorySlot::Neck,
    InventorySlot::Eyes,
    InventorySlot::Ears,
    InventorySlot::Gloves,
    InventorySlot::Shoes,
    InventorySlot::Back,
    InventorySlot::Belt,
    InventorySlot::Id,
];

/// tg `refresh_obscured` over `inventory` with each item's `flags_inv`.
pub fn obscured_by(inventory: &Inventory, flags_inv: impl Fn(Entity) -> u32) -> u32 {
    WORN.iter()
        .filter_map(|&s| inventory.slot(s))
        .fold(0, |acc, item| acc | flags_inv(item))
}

/// The mob's `obscured_slots`: its worn clothing's `flags_inv`, hair and facial hair for a husk.
pub fn obscured(world: &World, mob: Entity) -> u32 {
    let mut flags = world.get::<Inventory>(mob).map_or(0, |inventory| {
        obscured_by(inventory, |e| {
            world.get::<DmClothing>(e).map_or(0, |c| c.flags_inv)
        })
    });
    if world.get::<crate::human::Husk>(mob).is_some() {
        flags |= HIDEHAIR | HIDEFACIALHAIR;
    }
    flags
}

/// tg `hidden_slots_to_inventory_slots`: whether `hidden` hides `slot`'s item.
pub fn slot_hidden(hidden: u32, slot: InventorySlot) -> bool {
    let bit = match slot {
        InventorySlot::Neck => HIDENECK,
        InventorySlot::Mask => HIDEMASK,
        InventorySlot::Belt => HIDEBELT,
        InventorySlot::Eyes => HIDEEYES,
        InventorySlot::Ears => HIDEEARS,
        InventorySlot::Gloves => HIDEGLOVES,
        InventorySlot::Uniform => HIDEJUMPSUIT,
        InventorySlot::Shoes => HIDESHOES,
        InventorySlot::SuitStorage => HIDESUITSTORAGE,
        InventorySlot::Head => HIDEHEADGEAR,
        _ => 0,
    };
    hidden & bit != 0
}

/// `inventory` as it is drawn: the hidden slots empty (tg `update_worn_*` returns early when
/// `obscured_slots` hides the slot).
pub fn shown(inventory: &Inventory, hidden: u32) -> Inventory {
    let mut shown = inventory.clone();
    for slot in WORN {
        if slot_hidden(hidden, slot) {
            *shown.slot_mut(slot) = None;
        }
    }
    shown
}

/// The body as drawn under `hidden`: no hair (`HIDEHAIR`), no eyes (`HIDEEYES`), no beard or
/// lipstick (`HIDEFACIALHAIR`).
pub fn hide_body(mut look: HumanLook, hidden: u32) -> HumanLook {
    if hidden & HIDEHAIR != 0 {
        look.hair_style = None;
    }
    if hidden & HIDEEYES != 0 {
        look.eyeless = true;
    }
    // `get_hair_overlays` / `get_lips_overlays`: `HIDEFACIALHAIR` hides the beard and the lips.
    if hidden & HIDEFACIALHAIR != 0 {
        look.facial_hair_style = None;
        look.lip_style = None;
    }
    look
}

fn cover_of(world: &World, item: Option<Entity>) -> u32 {
    item.and_then(|i| world.get::<DmClothing>(i))
        .map_or(0, |c| c.flags_cover)
}

/// tg `is_mouth_covered()`: the head (`HEADCOVERSMOUTH`), else the mask (`MASKCOVERSMOUTH`).
pub fn mouth_covered(world: &World, mob: Entity) -> Option<Entity> {
    let inventory = world.get::<Inventory>(mob)?;
    [
        (inventory.head, HEADCOVERSMOUTH),
        (inventory.mask, MASKCOVERSMOUTH),
    ]
    .into_iter()
    .find(|&(item, bit)| cover_of(world, item) & bit != 0)
    .and_then(|(item, _)| item)
}

/// tg `is_eyes_covered()`: the head, the mask, then the glasses.
pub fn eyes_covered(world: &World, mob: Entity) -> Option<Entity> {
    let inventory = world.get::<Inventory>(mob)?;
    [
        (inventory.head, HEADCOVERSEYES),
        (inventory.mask, MASKCOVERSEYES),
        (inventory.eyes, GLASSESCOVERSEYES),
    ]
    .into_iter()
    .find(|&(item, bit)| cover_of(world, item) & bit != 0)
    .and_then(|(item, _)| item)
}

/// tg `is_pepper_proof()`: the head or the mask has `PEPPERPROOF`.
pub fn pepper_proof(world: &World, mob: Entity) -> Option<Entity> {
    let inventory = world.get::<Inventory>(mob)?;
    [inventory.head, inventory.mask]
        .into_iter()
        .find(|&item| cover_of(world, item) & PEPPERPROOF != 0)
        .flatten()
}

/// tg `is_face_obscured` of a human: `HIDEFACE` or no head.
pub fn face_obscured(world: &World, mob: Entity) -> bool {
    obscured(world, mob) & HIDEFACE != 0
        || world
            .get::<crate::health::Health>(mob)
            .is_some_and(|h| h.missing & (1 << crate::zone::BodyZone::Head.index()) != 0)
}

/// tg `get_id_name("")`: the registered name of the card the mob wears (`get_idcard(hand_first
/// = FALSE)`: the ID slot first).
pub fn id_name(world: &World, mob: Entity) -> Option<String> {
    let inventory = world.get::<Inventory>(mob)?;
    [
        inventory.id,
        inventory.hands[inventory.active],
        inventory.hands[1 - inventory.active],
    ]
    .into_iter()
    .flatten()
    .find_map(|item| crate::inventory::get_id(world, item))
    .and_then(|card| world.get::<crate::outfit::IdRegistration>(card))
    .map(|r| r.registered_name.clone())
    .filter(|n| !n.is_empty())
}

/// tg `get_visible_name` when the face is hidden: the worn ID's name, else "Неизвестный".
/// `None` while the face shows (the mob's own name).
// shortcut: a visible face never reads "Name (as ID name)" for a mismatching card.
pub fn hidden_face_name(world: &World, mob: Entity) -> Option<String> {
    if !face_obscured(world, mob) {
        return None;
    }
    Some(id_name(world, mob).unwrap_or_else(|| UNKNOWN_NAME.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clothing(world: &mut World, flags_inv: u32, flags_cover: u32) -> Entity {
        world
            .spawn(DmClothing {
                flags_inv,
                flags_cover,
                ..default()
            })
            .id()
    }

    #[test]
    fn a_gas_mask_and_helmet_hide_their_slots() {
        let mut world = World::new();
        // `/obj/item/clothing/mask/gas`: HIDEEARS|HIDEEYES|HIDEFACE|HIDEFACIALHAIR|HIDESNOUT.
        let mask = clothing(
            &mut world,
            HIDEEARS | HIDEEYES | HIDEFACE | HIDEFACIALHAIR,
            0,
        );
        let glasses = clothing(&mut world, 0, GLASSESCOVERSEYES);
        let headset = clothing(&mut world, 0, 0);
        // A mask in a pocket hides nothing.
        let pocket = clothing(&mut world, HIDEHAIR, 0);
        let inventory = Inventory {
            mask: Some(mask),
            eyes: Some(glasses),
            ears: Some(headset),
            pockets: [Some(pocket), None],
            ..default()
        };
        let mob = world.spawn(inventory.clone()).id();
        let hidden = obscured(&world, mob);
        assert_eq!(hidden, HIDEEARS | HIDEEYES | HIDEFACE | HIDEFACIALHAIR);
        assert!(slot_hidden(hidden, InventorySlot::Eyes));
        assert!(slot_hidden(hidden, InventorySlot::Ears));
        assert!(!slot_hidden(hidden, InventorySlot::Mask));
        let drawn = shown(&inventory, hidden);
        assert_eq!(
            (drawn.eyes, drawn.ears, drawn.mask),
            (None, None, Some(mask))
        );
        assert_eq!(drawn.pockets[0], Some(pocket));
        assert!(face_obscured(&world, mob));
        assert_eq!(hidden_face_name(&world, mob).as_deref(), Some(UNKNOWN_NAME));
        assert_eq!(eyes_covered(&world, mob), Some(glasses));
        assert_eq!(mouth_covered(&world, mob), None);
    }

    #[test]
    fn hair_and_eyes_leave_the_look() {
        let look = HumanLook::random_assistant(7);
        assert!(look.hair_style.is_some());
        let hooded = hide_body(look.clone(), HIDEHAIR);
        assert!(hooded.hair_style.is_none() && !hooded.eyeless);
        let masked = hide_body(look.clone(), HIDEEYES);
        assert!(masked.eyeless && masked.hair_style == look.hair_style);
    }

    #[test]
    fn a_breath_mask_covers_the_mouth_until_pushed_up() {
        let mut world = World::new();
        let mask = clothing(&mut world, 0, MASKCOVERSMOUTH);
        let mob = world
            .spawn(Inventory {
                mask: Some(mask),
                ..default()
            })
            .id();
        assert_eq!(mouth_covered(&world, mob), Some(mask));
        world.get_mut::<DmClothing>(mask).unwrap().flags_cover ^= MASKCOVERSMOUTH;
        assert_eq!(mouth_covered(&world, mob), None);
        assert!(pepper_proof(&world, mob).is_none());
    }
}
