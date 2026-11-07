//! The ablative trenchcoat and hood (server), after tg
//! `code/modules/clothing/suits/ablativecoat.dm` and the human `projectile_hit` /
//! `check_reflect` (`human_defense.dm`) with `/obj/projectile/proc/reflect`.
//!
//! - A `reflectable` projectile (lasers, disablers) aimed at a covered zone of a human wearing
//!   the coat (chest, arms, legs) or its hood (head) is reflected half the time
//!   ([`reflect_projectile`], `IsReflect`: `hit_reflect_chance` 50): it turns back by 120-240
//!   degrees, now the wearer's, and flies on (`BULLET_ACT_FORCE_PIERCE`). Not one's own shot at
//!   oneself.
//! - The hood carries a security HUD (`TRAIT_SECURITY_HUD`, `crate::records::hud`); the coat
//!   holds what a security vest holds (`allowed = GLOB.security_vest_allowed`, converted).
// shortcut: the zone is the firer's selected one (tg reflects on the zone the hit would take
// after its accuracy roll); held reflectors (energy swords, shields) are not ported, and the
// hood's "hud enabled" balloons are not shown. Upgrade with held reflectors.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use crate::combat::CombatRng;
use crate::inventory::Inventory;
use crate::projectile::Projectile;
use crate::zone::BodyZone;

pub const COAT: &str = "/obj/item/clothing/suit/hooded/ablative";
pub const HOOD: &str = "/obj/item/clothing/head/hooded/ablative";
/// `hit_reflect_chance`, percent.
pub const HIT_REFLECT_CHANCE: f32 = 50.0;

/// Whether a worn piece of `path` can reflect a hit on `zone` (`IsReflect` before its roll).
pub fn covers(path: &str, zone: BodyZone) -> bool {
    if is_under(path, COAT) {
        zone != BodyZone::Head
    } else if is_under(path, HOOD) {
        zone == BodyZone::Head
    } else {
        false
    }
}

/// tg human `check_reflect(def_zone)`: the suit, then the head, each with its own roll.
pub fn check_reflect(world: &mut World, mob: Entity, zone: BodyZone) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return false;
    };
    let worn: Vec<String> = [inventory.suit, inventory.head]
        .into_iter()
        .flatten()
        .filter_map(|e| world.get::<DmItem>(e).map(|i| i.path.clone()))
        .collect();
    worn.iter().any(|path| {
        covers(path, zone) && world.resource_mut::<CombatRng>().chance(HIT_REFLECT_CHANCE)
    })
}

/// Whether the security HUD of a hood is worn on `head`.
pub fn hood_hud(world: &World, head: Option<Entity>) -> bool {
    head.and_then(|h| world.get::<DmItem>(h))
        .is_some_and(|i| is_under(&i.path, HOOD))
}

/// tg human `projectile_hit` with a reflection: true when projectile `entity` (`p`) bounced off
/// `mob` and flies on.
pub fn reflect_projectile(
    world: &mut World,
    mob: Entity,
    entity: Entity,
    p: &mut Projectile,
) -> bool {
    if !p.data.reflectable || (p.firer == mob && p.original == Some(mob)) {
        return false;
    }
    if world.get::<Inventory>(mob).is_none() || !check_reflect(world, mob, p.zone) {
        return false;
    }
    let Some(&at) = world.get::<TilePos>(mob) else {
        return false;
    };
    let (what, who) = (
        capitalize(crate::obj_damage::strip(&crate::security::name_of(
            world, entity,
        ))),
        crate::security::name_of(world, mob),
    );
    let line = format!("{what} отражается от {who}!");
    crate::security::show(
        world,
        at,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, line.clone()),
        &[(mob, Span::BoldWarning, line)],
    );
    // `reflect`: a new course back by 120-240 degrees, from the wearer's tile.
    let turn = world.resource_mut::<CombatRng>().range(120, 240) as f32;
    p.angle = (p.angle + turn).rem_euclid(360.0);
    let (sin, cos) = p.angle.to_radians().sin_cos();
    p.dx = sin;
    p.dy = cos;
    p.firer = mob;
    p.original = None;
    p.start = at;
    true
}

use ss13_chat::capitalize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coat_covers_the_body_hood_the_head() {
        assert!(covers(COAT, BodyZone::Chest));
        assert!(covers(COAT, BodyZone::LLeg));
        assert!(!covers(COAT, BodyZone::Head));
        assert!(covers(HOOD, BodyZone::Head));
        assert!(!covers(HOOD, BodyZone::Chest));
        assert!(!covers(
            "/obj/item/clothing/suit/armor/vest",
            BodyZone::Chest
        ));
    }
}
