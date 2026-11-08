//! Clothing traits after tg `/obj/item/clothing/equipped` and `attach_clothing_traits()`
//! (`code/modules/clothing/clothing.dm`): an item's `clothing_traits` belong to its wearer while
//! it sits in one of its `slot_flags` slots (`ADD_CLOTHING_TRAIT`). The port reads them live from
//! what is worn ([`has_trait`]) instead of keeping a trait list on the mob, so equipping,
//! unequipping and `attach_clothing_traits`/`detach_clothing_traits` (editing
//! `DmClothing::clothing_traits`) take effect at once.
//!
//! Consumers here: wet floor slips (`TRAIT_NO_SLIP_*`, `crate::janitor`), cuffing speed
//! (`TRAIT_FAST_CUFFING`, `crate::restraints`), gloves leaving prints
//! (`TRAIT_FINGERPRINT_PASSTHROUGH`, `crate::forensics`). Prescription and science glasses keep
//! their own flags (`DmClothing::vision_correction`, `reagent_scanner`).

use bevy::prelude::*;
use ss13_mapload::{DmClothing, DmItem};

use crate::inventory::{ALL_SLOTS, Inventory, slot_bit};

/// `TRAIT_NO_SLIP_WATER`: water does not slip (galoshes).
pub const NO_SLIP_WATER: &str = "noslip_water";
/// `TRAIT_NO_SLIP_ICE`: permafrost does not slip.
pub const NO_SLIP_ICE: &str = "noslip_ice";
/// `TRAIT_NO_SLIP_ALL`: nothing slips.
pub const NO_SLIP_ALL: &str = "noslip_all";
/// `TRAIT_NO_SLIP_SLIDE`.
pub const NO_SLIP_SLIDE: &str = "noslip_slide";
/// `TRAIT_NEGATES_GRAVITY`: magboots.
pub const NEGATES_GRAVITY: &str = "negates_gravity";
/// `TRAIT_THERMAL_VISION`.
pub const THERMAL_VISION: &str = "thermal_vision";
/// `TRAIT_XRAY_VISION`.
pub const XRAY_VISION: &str = "xray_vision";
/// `TRAIT_TRUE_NIGHT_VISION`.
pub const TRUE_NIGHT_VISION: &str = "true_night_vision";
/// `TRAIT_FAST_CUFFING`.
pub const FAST_CUFFING: &str = "fast_cuffing";
/// `TRAIT_FINGERPRINT_PASSTHROUGH`.
pub const FINGERPRINT_PASSTHROUGH: &str = "fingerprint_passthrough";
/// `TRAIT_MESON_VISION`: mesons (a singularity's stare does not stun).
pub const MESON_VISION: &str = "meson_vision";

/// The items whose `clothing_traits` `mob` has: worn in a slot their `slot_flags` allow.
pub fn trait_sources(world: &World, mob: Entity) -> impl Iterator<Item = Entity> + '_ {
    let inventory = world.get::<Inventory>(mob);
    ALL_SLOTS.into_iter().filter_map(move |slot| {
        let item = inventory?.slot(slot)?;
        let dm = world.get::<DmItem>(item)?;
        (dm.slot_flags & slot_bit(slot) != 0).then_some(item)
    })
}

/// `HAS_TRAIT(mob, trait)` from its clothing.
pub fn has_trait(world: &World, mob: Entity, name: &str) -> bool {
    trait_sources(world, mob).any(|item| {
        world
            .get::<DmClothing>(item)
            .is_some_and(|c| c.clothing_traits.iter().any(|t| t == name))
    })
}

/// `/mob/living/carbon/human/slip` on a wet floor of strength `wet` (`TURF_WET_*`, the
/// highest one): `TRAIT_NO_SLIP_ALL` resists everything, `TRAIT_NO_SLIP_WATER` what lacks
/// `GALOSHES_DONT_HELP` (water), `TRAIT_NO_SLIP_ICE` `SLIDE_ICE` (permafrost).
pub fn resists_wet_slip(world: &World, mob: Entity, wet: u8) -> bool {
    use crate::janitor::{TURF_WET_PERMAFROST, TURF_WET_WATER};
    has_trait(world, mob, NO_SLIP_ALL)
        || (wet == TURF_WET_WATER && has_trait(world, mob, NO_SLIP_WATER))
        || (wet == TURF_WET_PERMAFROST && has_trait(world, mob, NO_SLIP_ICE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::janitor::{TURF_WET_LUBE, TURF_WET_PERMAFROST, TURF_WET_WATER};

    #[test]
    fn traits_follow_the_worn_slot() {
        let mut world = World::new();
        // Galoshes: shoes (`ITEM_SLOT_FEET`).
        let galoshes = world
            .spawn((
                DmItem {
                    slot_flags: slot_bit(ss13_shared::protocol::InventorySlot::Shoes),
                    ..default()
                },
                DmClothing {
                    clothing_traits: vec![NO_SLIP_WATER.into()],
                    ..default()
                },
            ))
            .id();
        let mob = world.spawn(Inventory::default()).id();
        assert!(!has_trait(&world, mob, NO_SLIP_WATER), "naked");
        // Held: not in one of its slots.
        world.get_mut::<Inventory>(mob).unwrap().hands[0] = Some(galoshes);
        assert!(!has_trait(&world, mob, NO_SLIP_WATER), "held");
        {
            let mut inv = world.get_mut::<Inventory>(mob).unwrap();
            inv.hands[0] = None;
            inv.shoes = Some(galoshes);
        }
        assert!(has_trait(&world, mob, NO_SLIP_WATER), "worn");
        assert!(resists_wet_slip(&world, mob, TURF_WET_WATER));
        assert!(
            !resists_wet_slip(&world, mob, TURF_WET_LUBE),
            "galoshes don't help"
        );
        assert!(!resists_wet_slip(&world, mob, TURF_WET_PERMAFROST));
        // `attach_clothing_traits` while worn.
        world
            .get_mut::<DmClothing>(galoshes)
            .unwrap()
            .clothing_traits
            .push(NO_SLIP_ALL.into());
        assert!(resists_wet_slip(&world, mob, TURF_WET_LUBE));
    }
}
