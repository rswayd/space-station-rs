//! Mob armor (server): tg `/mob/living/proc/run_armor_check`, `getarmor` and the human
//! `get_worn_bodypart_armor_value` (`living_defense.dm`, `human_defense.dm`).
//!
//! Every worn piece of clothing that covers the hit bodypart (`body_parts_covered`) multiplies
//! the damage that gets through by `(100 - rating)%` for the attack's flag (`melee`, `bullet`,
//! `laser`, `energy`, ...); pockets and hands do not count. Armour penetration lowers the result
//! (`PENETRATE_ARMOUR`); `weak_against_armour` doubles it. Damage is then reduced by at most
//! `ARMOR_MAX_BLOCK` (90%); effects (stuns, stamina) by the full rating.
// shortcut: no inner armor (`inner_armor`: species and implants), no wound armor; add with them.

use bevy::prelude::*;
use ss13_mapload::{DmArmor, DmClothing};
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;

use crate::inventory::Inventory;
use crate::zone::BodyZone;

/// `ARMOR_MAX_BLOCK`: damage is reduced by at most this much, percent.
pub const ARMOR_MAX_BLOCK: f32 = 90.0;
/// `ARMOR_WEAKENED_MULTIPLIER`.
pub const ARMOR_WEAKENED_MULTIPLIER: f32 = 2.0;

/// The slots whose clothing protects (tg `covering_clothing`: everything but pockets).
const COVERING: [InventorySlot; 13] = [
    InventorySlot::Head,
    InventorySlot::Mask,
    InventorySlot::Suit,
    InventorySlot::Uniform,
    InventorySlot::Back,
    InventorySlot::Gloves,
    InventorySlot::Shoes,
    InventorySlot::Belt,
    InventorySlot::SuitStorage,
    InventorySlot::Eyes,
    InventorySlot::Ears,
    InventorySlot::Id,
    InventorySlot::Neck,
];

/// `PENETRATE_ARMOUR(armour, penetration)`.
pub fn penetrate(armour: f32, penetration: f32) -> f32 {
    if penetration >= 100.0 {
        0.0
    } else {
        100.0 * (armour - penetration) / (100.0 - penetration)
    }
}

/// tg `get_worn_bodypart_armor_value`: `pieces` are the worn clothing's
/// (`body_parts_covered`, rating); `bit` is the bodypart's `body_part`.
pub fn worn_value(pieces: impl IntoIterator<Item = (u32, f32)>, bit: u32) -> f32 {
    let mut unblocked = 100.0;
    for (covered, rating) in pieces {
        if covered & bit != 0 {
            unblocked *= (100.0 - rating.min(100.0)) * 0.01;
        }
    }
    100.0 - unblocked
}

/// The covered bits and `flag` rating of the clothing `inventory` wears, for [`worn_value`].
pub fn worn_pieces<'a>(
    inventory: &'a Inventory,
    clothing: impl Fn(Entity) -> Option<(&'a DmClothing, &'a DmArmor)> + 'a,
    flag: &'a str,
) -> impl Iterator<Item = (u32, f32)> + 'a {
    COVERING
        .iter()
        .filter_map(move |&s| inventory.slot(s))
        .filter_map(move |item| clothing(item).map(|(c, a)| (c.body_parts_covered, a.rating(flag))))
}

/// tg `getarmor(def_zone, flag)` of a mob in `world`.
pub fn getarmor(world: &World, mob: Entity, zone: BodyZone, flag: &str) -> f32 {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return 0.0;
    };
    let clothing =
        |item: Entity| Some((world.get::<DmClothing>(item)?, world.get::<DmArmor>(item)?));
    worn_value(
        worn_pieces(inventory, clothing, flag),
        crate::chem::body_part_bit(zone),
    )
}

/// tg `run_armor_check` from the armor value: the effective armor and the line the victim reads
/// (none without armor). `absorb`/`soften` replace the default lines.
pub fn run_armor_check(
    armor: f32,
    armour_penetration: f32,
    weak_against_armour: bool,
    absorb: Option<String>,
    soften: Option<String>,
) -> (f32, Option<(Span, String)>) {
    let mut armor = armor;
    if armor <= 0.0 {
        return (armor, None);
    }
    if weak_against_armour {
        armor *= ARMOR_WEAKENED_MULTIPLIER;
    }
    if armour_penetration > 0.0 {
        let armor = penetrate(armor, armour_penetration).max(0.0);
        return (
            armor,
            Some((Span::BoldWarning, "Ваша броня была пробита!".to_owned())),
        );
    }
    let line = if armor >= 100.0 {
        (
            Span::Notice,
            absorb.unwrap_or_else(|| "Ваша броня поглощает удар!".to_owned()),
        )
    } else {
        (
            Span::Warning,
            soften.unwrap_or_else(|| "Ваша броня смягчает удар!".to_owned()),
        )
    };
    (armor, Some(line))
}

/// Damage that gets through `armor` (tg `apply_damage(blocked = min(ARMOR_MAX_BLOCK, armor))`).
pub fn blocked_damage(amount: f32, armor: f32) -> f32 {
    amount * (100.0 - armor.min(ARMOR_MAX_BLOCK)) / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worn_armor_multiplies_per_covering_piece() {
        // An armor vest (bullet 30) over a jumpsuit (0) on the chest; a helmet elsewhere.
        let chest = crate::chem::body_part_bit(BodyZone::Chest);
        let pieces = [(1 | 2, 30.0), (1 | 2 | 8 | 16 | 128 | 256, 0.0), (4, 30.0)];
        assert!((worn_value(pieces, chest) - 30.0).abs() < 1e-4);
        assert_eq!(
            worn_value(pieces, crate::chem::body_part_bit(BodyZone::LArm)),
            0.0
        );
        // Two 50% pieces stop 75%.
        assert!((worn_value([(1, 50.0), (1, 50.0)], 1) - 75.0).abs() < 1e-4);
    }

    #[test]
    fn armor_check_penetration_messages_and_cap() {
        assert_eq!(run_armor_check(0.0, 20.0, false, None, None), (0.0, None));
        let (armor, line) = run_armor_check(30.0, 0.0, false, None, None);
        assert_eq!(armor, 30.0);
        assert_eq!(line.unwrap().1, "Ваша броня смягчает удар!");
        let (armor, line) = run_armor_check(50.0, 20.0, false, None, None);
        assert!((armor - 37.5).abs() < 1e-4);
        assert_eq!(line.unwrap().1, "Ваша броня была пробита!");
        assert_eq!(run_armor_check(60.0, 0.0, true, None, None).0, 120.0);
        assert_eq!(
            run_armor_check(100.0, 0.0, false, None, None).1.unwrap().0,
            Span::Notice
        );
        assert_eq!(penetrate(50.0, 100.0), 0.0);
        // A laser (25) through a vest (laser 30); armor over 90 still lets 10% through.
        assert!((blocked_damage(25.0, 30.0) - 17.5).abs() < 1e-4);
        assert!((blocked_damage(25.0, 120.0) - 2.5).abs() < 1e-4);
    }
}
