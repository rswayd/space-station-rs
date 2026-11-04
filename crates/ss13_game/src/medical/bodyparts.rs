//! Limbs as items (server), after tg `code/modules/surgery/bodyparts` (`dismemberment.dm`
//! `drop_limb`, `try_attach_limb`; `stumps.dm`; `/obj/item/bodypart/arm|leg`).
//!
//! An amputated arm or leg leaves the body as its `/obj/item/bodypart` item, carrying its brute
//! and burn ([`Limb`]); a stump stays ([`super::wounds::Limb::missing`], with no skin to cut:
//! `SURGERY_SKIN_OPEN` is innate). Damage aimed at a missing part hits the chest, a missing
//! hand drops what it held and holds nothing, a missing leg counts as a disabled one
//! (`crate::medical::LimbLimits`), and the body is drawn without it. A limb item attached to
//! its stump by surgery brings its damage back.
// shortcut: arms and legs only (no decapitation, the head keeps its organs), only human flesh
// limbs (no species, robotic or wooden limbs, no arbitrary items as prosthetics); a missing
// hand still holds what other code puts there directly (only pickups check it), gloves and
// shoes stay on.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::TilePos;

use super::wounds::{Limbs, state};
use crate::health::Health;
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::zone::BodyZone;

/// The damage a removed limb keeps (tg `brute_dam`, `burn_dam` of the item).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Limb {
    pub brute: f32,
    pub burn: f32,
}

/// The human limb type of a zone (tg `/datum/species/human` `bodypart_overrides`).
pub fn limb_path(zone: BodyZone) -> Option<&'static str> {
    Some(match zone {
        BodyZone::LArm => "/obj/item/bodypart/arm/left",
        BodyZone::RArm => "/obj/item/bodypart/arm/right",
        BodyZone::LLeg => "/obj/item/bodypart/leg/left",
        BodyZone::RLeg => "/obj/item/bodypart/leg/right",
        BodyZone::Head | BodyZone::Chest => return None,
    })
}

/// The `body_zone` of a limb item type (any species' variant).
pub fn zone_of_path(path: &str) -> Option<BodyZone> {
    [
        BodyZone::LArm,
        BodyZone::RArm,
        BodyZone::LLeg,
        BodyZone::RLeg,
    ]
    .into_iter()
    .find(|&z| limb_path(z).is_some_and(|root| is_under(path, root)))
}

/// The hand a missing arm takes away.
fn hand_of(zone: BodyZone) -> Option<usize> {
    match zone {
        BodyZone::LArm => Some(0),
        BodyZone::RArm => Some(1),
        _ => None,
    }
}

/// The hand at `hand` is there to hold things (not amputated or paralyzed).
pub fn has_hand(world: &World, mob: Entity, hand: usize) -> bool {
    let zone = if hand == 0 {
        BodyZone::LArm
    } else {
        BodyZone::RArm
    };
    world
        .get::<Limbs>(mob)
        .is_none_or(|l| !l.limb(zone).missing && !l.limb(zone).paralyzed)
}

/// tg `drop_limb()`: `zone` leaves `mob` as an item on its tile with the part's damage. `None`
/// for the head and chest, a missing part or a mob without limbs.
pub fn drop_limb(world: &mut World, mob: Entity, zone: BodyZone) -> Option<Entity> {
    let path = limb_path(zone)?;
    let pos = *world.get::<TilePos>(mob)?;
    let mut limbs = world.get::<Limbs>(mob)?.clone();
    if limbs.limb(zone).missing {
        return None;
    }
    let limb = limbs.limb_mut(zone);
    while !limb.wounds.is_empty() {
        super::wounds::remove_wound(limb, 0);
    }
    limb.gauze = None;
    limb.missing = true;
    limb.surgery = state::SKIN_OPEN;
    world.entity_mut(mob).insert(limbs);
    let mut damage = Limb::default();
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        let part = health.part(zone);
        damage = Limb {
            brute: part.brute,
            burn: part.burn,
        };
        health.heal_zone(zone, part.brute, part.burn);
        health.missing |= 1 << zone.index();
        world.entity_mut(mob).insert(health);
    }
    if let Some(hand) = hand_of(zone)
        && let Some(held) = world.get::<Inventory>(mob).and_then(|i| i.hands[hand])
    {
        crate::inventory::put_down(world, held, pos);
    }
    set_drawn(world, mob, zone, false);
    let item = crate::construction::spawn_obj(world, path, pos)?;
    world.entity_mut(item).insert(damage);
    Some(item)
}

/// tg `try_attach_limb()`: the limb item onto `mob`'s stump of its zone, its damage with it.
/// False if the zone is not a stump.
pub fn attach_limb(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(zone) = world
        .get::<DmItem>(item)
        .and_then(|i| zone_of_path(&i.path))
    else {
        return false;
    };
    let Some(mut limbs) = world.get::<Limbs>(mob).cloned() else {
        return false;
    };
    if !limbs.limb(zone).missing {
        return false;
    }
    let limb = limbs.limb_mut(zone);
    limb.missing = false;
    limb.surgery = 0;
    world.entity_mut(mob).insert(limbs);
    let damage = world.get::<Limb>(item).copied().unwrap_or_default();
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.missing &= !(1 << zone.index());
        health.damage_zone(zone, crate::health::DamageType::Brute, damage.brute);
        health.damage_zone(zone, crate::health::DamageType::Burn, damage.burn);
        world.entity_mut(mob).insert(health);
    }
    set_drawn(world, mob, zone, true);
    world.entity_mut(item).despawn();
    true
}

/// tg `regenerate_limbs()` of `fully_heal(HEAL_LIMBS)`: every stump grows a fresh limb.
pub fn regenerate_limbs(world: &mut World, mob: Entity) {
    let Some(mut limbs) = world.get::<Limbs>(mob).cloned() else {
        return;
    };
    let grown: Vec<BodyZone> = BodyZone::ALL
        .into_iter()
        .filter(|&z| limbs.limb(z).missing)
        .collect();
    if grown.is_empty() {
        return;
    }
    for &zone in &grown {
        let limb = limbs.limb_mut(zone);
        limb.missing = false;
        limb.surgery = 0;
    }
    world.entity_mut(mob).insert(limbs);
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.missing = 0;
    }
    for zone in grown {
        set_drawn(world, mob, zone, true);
    }
}

fn set_drawn(world: &mut World, mob: Entity, zone: BodyZone, drawn: bool) {
    if let Some(mut look) = world.get_mut::<crate::human::HumanLook>(mob) {
        let missing = if drawn {
            look.missing & !(1 << zone.index())
        } else {
            look.missing | 1 << zone.index()
        };
        if look.missing != missing {
            look.missing = missing;
        }
    }
    crate::inventory::refresh_look(world, mob);
}
