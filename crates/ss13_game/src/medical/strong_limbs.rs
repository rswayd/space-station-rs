//! BandaStation's augmented prostheses (`modular_bandastation/objects/code/items/bodyparts/
//! strongarm_bodypart.dm`, `strongleg_bodypart.dm`): `/obj/item/bodypart/arm/*/strongarm` and
//! `/obj/item/bodypart/leg/*/strongleg`, attached by surgery like any limb
//! (`crate::medical::bodyparts::attach_limb` keeps their type in the limb's `kind`).
//!
//! With both arms augmented (`/datum/component/strongarm_combat`):
//! - a punch that lands on a standing target throws it 3 tiles away (speed 1): "... отлетает от
//!   мощного удара!" ([`on_punched`]);
//! - a right-click shove with an empty hand ([`strong_shove`]) knocks the target down for 3 s
//!   (3 s of daze) and throws it 5 tiles (speed 3); a staggered or lying target drops what it
//!   holds.
//!
//! With both legs augmented (`/datum/component/strongleg_combat`) a kick on a standing target
//! throws it 2 tiles away.
// shortcut: none of the limbs' own vars (brute and burn modifiers, wound resistance, max damage,
// unarmed damage and effectiveness, grab bonuses, click and interaction speed, the legs' speed,
// `bodypart_traits`: throwing arm, shock/pierce/push immunity, no dismember, grab resistance)
// are applied; the body keeps one modifier and fixed unarmed numbers. Kicks only land on lying
// targets here (`crate::combat::attacking_limb`), so the strongleg knockback does not happen
// yet. The fork passes the armor block as `kicking` (`COMSIG_HUMAN_PUNCHED`'s arguments are
// shifted); the port tells kicks by the limb, as intended.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use super::wounds::Limbs;
use crate::health::{BodyPosition, COMBAT_MESSAGE_RANGE};
use crate::inventory::Inventory;
use crate::security::{name_of, show};
use crate::sound::{PlaySound, playsound};
use crate::zone::BodyZone;

const STRONGARM_LEFT: &str = "/obj/item/bodypart/arm/left/strongarm";
const STRONGARM_RIGHT: &str = "/obj/item/bodypart/arm/right/strongarm";
const STRONGLEG_LEFT: &str = "/obj/item/bodypart/leg/left/strongleg";
const STRONGLEG_RIGHT: &str = "/obj/item/bodypart/leg/right/strongleg";

/// `setup_strongarm`'s `AddComponent(..., 3, 5, 3, 3 SECONDS)`.
const PUNCH_KNOCKBACK: u32 = 3;
const SHOVE_THROW_RANGE: u32 = 5;
const SHOVE_THROW_SPEED: f32 = 3.0;
const SHOVE_KNOCKDOWN_SECS: f32 = 3.0;
const SHOVE_DAZE_SECS: f32 = 3.0;
/// `setup_strongleg`'s `AddComponent(..., 2)`.
const KICK_KNOCKBACK: u32 = 2;
const SHOVE_SOUND: &str = "sound/items/weapons/shove.ogg";

/// A strongarm or strongleg type: [`super::wounds::Limb::kind`] keeps it.
pub fn is_strong_limb(path: &str) -> bool {
    [
        STRONGARM_LEFT,
        STRONGARM_RIGHT,
        STRONGLEG_LEFT,
        STRONGLEG_RIGHT,
    ]
    .iter()
    .any(|root| is_under(path, root))
}

fn limb_is(world: &World, mob: Entity, zone: BodyZone, root: &str) -> bool {
    world.get::<Limbs>(mob).is_some_and(|l| {
        let limb = l.limb(zone);
        !limb.missing && limb.kind.as_deref().is_some_and(|k| is_under(k, root))
    })
}

/// `count_strongarm_limbs(owner) >= HUMAN_LIMBS_COUNT`.
pub fn strongarm(world: &World, mob: Entity) -> bool {
    limb_is(world, mob, BodyZone::LArm, STRONGARM_LEFT)
        && limb_is(world, mob, BodyZone::RArm, STRONGARM_RIGHT)
}

/// `count_strongleg_limbs(owner) >= HUMAN_LIMBS_COUNT`.
pub fn strongleg(world: &World, mob: Entity) -> bool {
    limb_is(world, mob, BodyZone::LLeg, STRONGLEG_LEFT)
        && limb_is(world, mob, BodyZone::RLeg, STRONGLEG_RIGHT)
}

fn lying(world: &World, mob: Entity) -> bool {
    world.get::<BodyPosition>(mob).is_some_and(|b| b.lying())
}

/// `get_edge_target_turf(target, get_dir(source, target))` for a throw of `range`.
fn throw_away(world: &mut World, source: Entity, target: Entity, range: u32, speed: f32) {
    let (Some(&from), Some(&at)) = (world.get::<TilePos>(source), world.get::<TilePos>(target))
    else {
        return;
    };
    let Some(dir) = crate::pull::dir_between(from, at) else {
        return;
    };
    let edge = (0..range).fold(at, |p, _| p.step(dir));
    crate::throw::throw_at(world, target, edge, range, speed);
}

fn seen(world: &mut World, target: Entity, others: String, victim: String) {
    if let Some(at) = world.get::<TilePos>(target).copied() {
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Danger, others),
            &[(target, Span::BoldWarning, victim)],
        );
    }
}

/// `COMSIG_HUMAN_PUNCHED` of the strongarm and strongleg components, after a landed blow;
/// `kicking`: the blow was a kick.
pub fn on_punched(world: &mut World, attacker: Entity, target: Entity, kicking: bool) {
    if attacker == target || lying(world, target) {
        return;
    }
    let (range, others, victim, own) = if !kicking && strongarm(world, attacker) {
        (
            PUNCH_KNOCKBACK,
            "отлетает от мощного удара!",
            "Мощный удар отбрасывает вас!",
            "Ваш удар отбрасывает",
        )
    } else if kicking && strongleg(world, attacker) {
        (
            KICK_KNOCKBACK,
            "отлетает от мощного пинка!",
            "Мощный пинок отбрасывает вас!",
            "Ваш пинок отбрасывает",
        )
    } else {
        return;
    };
    throw_away(world, attacker, target, range, 1.0);
    let name = name_of(world, target);
    seen(
        world,
        target,
        format!("{} {others}", ss13_shared::capitalize(&name)),
        victim.to_owned(),
    );
    crate::tools::tell_mob(world, attacker, Span::Danger, format!("{own} {name}!"));
}

/// The strongarm's `COMSIG_LIVING_UNARMED_ATTACK` with a right click: the shove replaced. False
/// when the attacker has no pair of strongarms (the normal shove runs).
pub fn strong_shove(world: &mut World, user: Entity, target: Entity) -> bool {
    if user == target
        || !strongarm(world, user)
        || world.get::<Inventory>(user).is_none()
        || world.get::<crate::health::Stat>(target).is_none()
    {
        return false;
    }
    if let Some(at) = world.get::<TilePos>(target).copied() {
        playsound(
            world,
            PlaySound::new(at, SHOVE_SOUND, 50.0, true).extrarange(-1.0),
        );
    }
    crate::status_effects::knockdown(world, target, SHOVE_KNOCKDOWN_SECS, SHOVE_DAZE_SECS);
    throw_away(world, user, target, SHOVE_THROW_RANGE, SHOVE_THROW_SPEED);
    let staggered = world
        .get::<crate::status_effects::StatusEffects>(target)
        .is_some_and(|s| s.staggered > 0.0);
    let held = world
        .get::<Inventory>(target)
        .and_then(Inventory::active_item);
    if let Some(item) = held
        && (staggered || lying(world, target))
        && !crate::inventory::no_drop(world, item)
        && let Some(at) = world.get::<TilePos>(target).copied()
    {
        let slot = world.get::<Inventory>(target).and_then(|i| i.slot_of(item));
        if let Some(slot) = slot {
            crate::inventory::unequip(world, target, at, slot);
            crate::inventory::put_down(world, item, at);
            let (who, what) = (name_of(world, target), name_of(world, item));
            show(
                world,
                at,
                COMBAT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Danger,
                    format!("{} роняет {what}!", ss13_shared::capitalize(&who)),
                ),
                &[(target, Span::Warning, format!("Вы роняете {what}!"))],
            );
        }
    }
    let (user_name, target_name) = (name_of(world, user), name_of(world, target));
    seen(
        world,
        target,
        format!(
            "{} с силой толкает {target_name}, сбивая с ног!",
            ss13_shared::capitalize(&user_name)
        ),
        format!(
            "{} с огромной силой толкает вас, сбивая с ног!",
            ss13_shared::capitalize(&user_name)
        ),
    );
    crate::tools::tell_mob(
        world,
        user,
        Span::Danger,
        format!("Вы с силой толкаете {target_name}, сбивая с ног!"),
    );
    info!("{user} strongarm shoved {target}");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_strongarms_are_needed() {
        let mut world = World::new();
        let mut limbs = Limbs::default();
        limbs.limb_mut(BodyZone::LArm).kind = Some(STRONGARM_LEFT.to_owned());
        let mob = world.spawn(limbs.clone()).id();
        assert!(!strongarm(&world, mob));
        limbs.limb_mut(BodyZone::RArm).kind = Some(STRONGARM_RIGHT.to_owned());
        world.entity_mut(mob).insert(limbs.clone());
        assert!(strongarm(&world, mob));
        assert!(!strongleg(&world, mob));
        // A stump does not count.
        limbs.limb_mut(BodyZone::RArm).missing = true;
        world.entity_mut(mob).insert(limbs);
        assert!(!strongarm(&world, mob));
    }

    #[test]
    fn only_the_fork_prostheses_are_kept() {
        assert!(is_strong_limb("/obj/item/bodypart/leg/right/strongleg"));
        assert!(!is_strong_limb("/obj/item/bodypart/arm/left/robot"));
        assert!(!is_strong_limb("/obj/item/bodypart/arm/left"));
    }
}
