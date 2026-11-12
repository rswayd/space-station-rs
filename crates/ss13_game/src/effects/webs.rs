//! Sticky spider webs, after tg `/obj/structure/spider/stickyweb` (`spiderwebs.dm`): a living
//! mob walking into a web gets stuck with `stuck_chance` (50 %, very sticky 100 %): it loses
//! 10-15 stamina, or, already over 90 stamina loss, trips (`SetKnockdown(3 s)`). Spiders
//! (`TRAIT_WEB_SURFER`) and whoever they pull pass freely. Sealed webs are dense walls (from
//! their `density`). Webs take a quarter of brute and 125 % of burn melee damage
//! ([`crate::obj_damage::take_damage`]) and burn in air over 350 K
//! ([`crate::atmos_exposure`]).
// shortcut: no projectile catching (`projectile_stuck_chance`), no `Shake` animation, no web
// weaving into cloth (`TRAIT_WEB_WEAVER`), no genetic webs (`allowed_mob`), no `play_attack_sound`
// muting, no web passages or spikes (cocoons: `crate::antag::spiders`).
//
// Cost: per step, one look at the target tile's contents.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_shared::chat::Span;
use ss13_world::Moved;

use crate::combat::CombatRng;
use crate::health::{BodyPosition, Health};
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::status_effects::{Incap, StatusEffects};

/// `/obj/structure/spider`: every web kind.
pub const SPIDER_STRUCTURE: &str = "/obj/structure/spider";
pub const STICKYWEB: &str = "/obj/structure/spider/stickyweb";
const VERY_STICKY: &str = "/obj/structure/spider/stickyweb/very_sticky";
const SEALED: &str = "/obj/structure/spider/stickyweb/sealed";
/// Mobs with `TRAIT_WEB_SURFER` from their type.
const WEB_SURFERS: [&str; 3] = [
    "/mob/living/basic/spider",
    "/mob/living/basic/supermatter_spider",
    "/mob/living/basic/flesh_spider",
];
/// Over this stamina loss a stuck mob trips instead.
const EXHAUSTED: f32 = 90.0;
const TRIP_SECS: f32 = 3.0;

/// `stuck_chance` of the web type `path`; `None` for things that do not catch walkers.
pub fn stuck_chance(path: &str) -> Option<f32> {
    if !is_under(path, STICKYWEB) || is_under(path, SEALED) {
        return None;
    }
    Some(if is_under(path, VERY_STICKY) {
        100.0
    } else {
        50.0
    })
}

/// `run_atom_armor` of `/obj/structure/spider` for `MELEE` damage: burn ×1.25, brute ×0.25.
pub fn melee_damage_mult(burn: bool) -> f32 {
    if burn { 1.25 } else { 0.25 }
}

/// `is_whitelisted`: `TRAIT_WEB_SURFER`.
fn web_surfer(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::mobs::BasicMob>(mob)
        .is_some_and(|m| WEB_SURFERS.iter().any(|root| is_under(&m.def.path, root)))
}

/// `on_entered` of the webs on every step's target tile.
pub(super) fn walk_into_webs(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        if world.get::<Health>(m.entity).is_none() || web_surfer(world, m.entity) {
            continue;
        }
        let puller = world
            .get::<crate::pull::PulledBy>(m.entity)
            .map(|p| p.puller());
        if puller.is_some_and(|p| web_surfer(world, p)) {
            continue;
        }
        let chances: Vec<f32> = super::on_tile(world, m.to)
            .into_iter()
            .filter_map(|e| world.get::<DmObj>(e).and_then(|o| stuck_chance(&o.path)))
            .collect();
        for chance in chances {
            if world.resource_mut::<CombatRng>().chance(chance) {
                stuck_react(world, m.entity);
            }
        }
    }
}

/// tg `stuck_react(victim)`.
fn stuck_react(world: &mut World, victim: Entity) {
    let client = world.get::<PlayerMob>(victim).map(|p| p.client);
    let stamina = world
        .get::<StatusEffects>(victim)
        .map_or(0.0, |e| e.stamina);
    if stamina > EXHAUSTED {
        let lying = world.get::<BodyPosition>(victim).is_some_and(|b| b.lying());
        if !lying && let Some(client) = client {
            crate::tools::tell(
                world,
                client,
                Span::Warning,
                "Вы спотыкаетесь о паутину от изнеможения!",
            );
        }
        crate::status_effects::set(world, victim, Incap::Knockdown, TRIP_SECS);
        return;
    }
    if world.resource_mut::<CombatRng>().chance(25.0)
        && let Some(client) = client
    {
        crate::tools::tell(world, client, Span::Warning, "застрял в паутине!");
    }
    let amount = world.resource_mut::<CombatRng>().range(10, 15);
    crate::status_effects::adjust_stamina(world, victim, amount as f32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_kinds() {
        assert_eq!(stuck_chance("/obj/structure/spider/stickyweb"), Some(50.0));
        assert_eq!(
            stuck_chance("/obj/structure/spider/stickyweb/very_sticky"),
            Some(100.0)
        );
        assert_eq!(
            stuck_chance("/obj/structure/spider/stickyweb/sealed/tough"),
            None
        );
        assert_eq!(stuck_chance("/obj/structure/spider/cocoon"), None);
        assert_eq!(melee_damage_mult(false), 0.25);
    }
}
