//! Kneecapping (server), after tg `/datum/element/kneecapping`
//! (`code/datums/elements/kneecapping.dm`), which the baseball bat carries.
//!
//! A right click with the bat on a carbon whose leg the attacker aims at (`zone_selected` a
//! leg), while the target is floored, immobilized or buckled and has that leg, replaces the
//! attack: after a 3 s `do_after` the leg takes the bat's `force` as its `damtype`, unarmored,
//! with a `wound_bonus` rolled between the leg's severe and critical+10 blunt wound thresholds;
//! the target screams, the attacker lunges with the bat's image and the bat's `hitsound` plays.
//! A pacifist gets the ordinary (refused) attack. The checks are made again when the swing lands.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmWounding};
use ss13_shared::chat::Span;

use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::medical::wounds::{Hit, Limbs, WoundKind};
use crate::security::name_of;
use crate::tools::{Actor, DoAfter, do_after};
use crate::zone::BodyZone;

/// The items with the element.
pub const BASEBALL_BAT: &str = "/obj/item/melee/baseball_bat";
/// The aim, deciseconds.
pub const KNEECAP_DS: f32 = 30.0;

pub fn kneecaps(path: &str) -> bool {
    is_under(path, BASEBALL_BAT)
}

/// `try_kneecap_target`'s checks on the target: aimed at a leg it has, and it is buckled,
/// floored or immobilized.
pub fn can_kneecap(zone: BodyZone, limbs: Option<&Limbs>, helpless: bool) -> bool {
    matches!(zone, BodyZone::LLeg | BodyZone::RLeg)
        && helpless
        && limbs.is_some_and(|l| !l.limb(zone).missing)
}

/// `TRAIT_FLOORED`, `TRAIT_IMMOBILIZED` (a conscious mob's status effects, or a stat that
/// adds them) or `buckled`.
pub fn helpless(world: &World, mob: Entity) -> bool {
    let down = world
        .get::<Stat>(mob)
        .is_some_and(|&s| s != Stat::Conscious);
    let stunned = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(|e| e.floored() || e.immobilized());
    let buckled = world.get::<crate::medical::surgery::Buckled>(mob).is_some();
    down || stunned || buckled
}

/// `do_kneecap_target` up to its `do_after`. True when the special attack replaces the click
/// (the element cancels the attack chain whenever its checks pass).
pub fn try_start(
    world: &mut World,
    client: Entity,
    attacker: Entity,
    target: Entity,
    weapon: Entity,
    zone: BodyZone,
) -> bool {
    let carbon = world.get::<Inventory>(target).is_some();
    if !carbon || !can_kneecap(zone, world.get::<Limbs>(target), helpless(world, target)) {
        return false;
    }
    // `LAZYACCESS(attacker.do_afters, weapon)`: already swinging, the click is spent.
    if world.get::<DoAfter>(attacker).is_some() {
        return true;
    }
    let (user, victim, bat) = (
        name_of(world, attacker),
        name_of(world, target),
        name_of(world, weapon),
    );
    crate::chem::visible(
        world,
        attacker,
        &format!("Вы тщательно целитесь с помощью {bat} в колени {victim}!"),
        &format!("{user} тщательно целится с помощью {bat} в колени {victim}!"),
        Span::Warning,
    );
    let actor = Actor {
        client,
        mob: attacker,
        item: Some(weapon),
    };
    do_after(world, actor, target, KNEECAP_DS, 0, Job::Kneecap(zone));
    true
}

/// The swing at the end of the `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, zone: BodyZone) {
    let (target, Some(weapon)) = (action.target, action.item) else {
        return;
    };
    // The bat is still in the active hand, the leg still there, the target still helpless.
    let held = world.get::<Inventory>(mob).and_then(Inventory::active_item) == Some(weapon);
    if !held || !can_kneecap(zone, world.get::<Limbs>(target), helpless(world, target)) {
        return;
    }
    let Some(dm) = world.get::<DmItem>(weapon).cloned() else {
        return;
    };
    let (user, victim, bat) = (
        name_of(world, mob),
        name_of(world, target),
        name_of(world, weapon),
    );
    crate::chem::visible(
        world,
        mob,
        &format!("Вы бьёте с помощью {bat} по коленям {victim}!"),
        &format!("{user} бьёт с помощью {bat} по коленям {victim}!"),
        Span::Warning,
    );
    // `get_wound_threshold_of_wound_type(WOUND_BLUNT, SEVERE / CRITICAL)` of a flesh leg.
    let min_wound = WoundKind::Hairline.def().threshold_minimum as u32;
    let max_wound = WoundKind::Compound.def().threshold_minimum as u32;
    let wound_bonus = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(min_wound, max_wound + 10) as f32;
    let kind = DamageType::from_dm(&dm.damtype).unwrap_or(DamageType::Brute);
    let multiplier = world
        .get::<Limbs>(target)
        .map_or(1.0, |l| l.limb(zone).damage_multiplier());
    let damage = dm.force * multiplier;
    let mut before = 0.0;
    if let Ok(mut entity) = world.get_entity_mut(target)
        && let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        before = health.part(zone).damage();
        let mut new = *stat;
        crate::health::apply_damage_zone(&mut health, &mut new, zone, kind, damage);
        stat.set_if_neq(new);
    }
    let wounding = world.get::<DmWounding>(weapon).cloned().unwrap_or_default();
    crate::medical::wounds::after_hit(
        world,
        target,
        zone,
        Hit {
            kind,
            damage,
            sharpness: wounding.sharpness,
            wound_bonus,
            exposed_wound_bonus: wounding.exposed_wound_bonus,
            part_damage_before: before,
        },
    );
    crate::medical::diseases::emote(world, target, "scream");
    crate::animation::do_attack_animation_with(
        world,
        mob,
        target,
        crate::animation::AttackVisual::Item(weapon),
    );
    if let Some(&pos) = world.get::<ss13_shared::TilePos>(mob) {
        let w_class = world
            .get::<ss13_mapload::DmWeight>(weapon)
            .copied()
            .unwrap_or_default()
            .w_class;
        let volume = crate::sound::clamped_volume(dm.force, w_class);
        let file = crate::sound::hitsound(&dm).to_owned();
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, volume, true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_leg_of_a_helpless_target() {
        let limbs = Limbs::default();
        assert!(can_kneecap(BodyZone::LLeg, Some(&limbs), true));
        assert!(can_kneecap(BodyZone::RLeg, Some(&limbs), true));
        assert!(!can_kneecap(BodyZone::LLeg, Some(&limbs), false));
        assert!(!can_kneecap(BodyZone::Chest, Some(&limbs), true));
        assert!(!can_kneecap(BodyZone::LLeg, None, true));
        let mut legless = Limbs::default();
        legless.limb_mut(BodyZone::LLeg).missing = true;
        assert!(!can_kneecap(BodyZone::LLeg, Some(&legless), true));
        assert!(kneecaps("/obj/item/melee/baseball_bat/ablative"));
        assert!(!kneecaps("/obj/item/melee/baton"));
    }
}
