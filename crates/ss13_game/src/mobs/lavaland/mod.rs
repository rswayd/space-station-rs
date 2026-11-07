//! Lavaland fauna (server), after tg `/mob/living/basic/mining` (`code/modules/mob/living/basic/
//! lavaland/`): the shared mining mob, the abilities its kinds fight with ([`abilities`]), their
//! AI leaves ([`ai`], the trees are `crate::mobs::ai::trees::lavaland`) and the cave population
//! of the mining level ([`spawn`]).
//!
//! - Every `/mob/living/basic/mining` (`mining.dm`): `ranged_armour` (a projectile under 30
//!   damage that is not brute does 30%: "seems resistant to"), fights back (`ai_retaliate`), and
//!   an ash walker that hurts one loses the `ashwalker` faction of it (`on_attacked`).
//! - Goliaths dig tentacles under their target (`goliath_tentacles`), legions launch skulls
//!   that bite and turn downed humans into new legions (`skull_launcher`, `legion_brood`),
//!   watchers and basilisks shoot freezing blasts (`ranged_attacks`), brimdemons line up and
//!   fire a beam (`brimbeam`), lobstrosities rush (`basic_charge/lobster`), bileworms lob bile
//!   (`bileworm_spew`), hivelords throw their brood (`hivelord_spawn`), goldgrubs run away and
//!   drop the ore they carry.
//! - Death drops: the legion's and the hivelord's regenerative cores (`death_drops`), the
//!   goldgrub's ore (`generate_loot` + `barf_contents`), the legion's corpse (`content_barfer`);
//!   a dead brimdemon bursts (`brim_burst`).
//! - Icemoon's polar bears, wolves, whelps and demonic watchers ([`icemoon`]).
// shortcut: no crusher trophies, kill tally, lava/ash storm immunity traits (the weather and
// lava are not dangerous to mobs here yet), butchering (`butcher_results`: goliath hide plates
// and the rest come with tg's butchering component), taming or riding; thrown items are not
// blocked (`minimum_thrown_force`).

pub mod abilities;
pub mod ai;
pub mod icemoon;
pub mod spawn;

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType};

/// `/mob/living/basic/mining`.
pub const MINING_ROOT: &str = "/mob/living/basic/mining";
pub const GOLIATH: &str = "/mob/living/basic/mining/goliath";
pub const IMMORTAL_GOLIATH: &str = "/mob/living/basic/mining/goliath/ancient/immortal";
pub const LEGION: &str = "/mob/living/basic/mining/legion";
pub const LEGION_BROOD: &str = "/mob/living/basic/mining/legion_brood";
pub const BRIMDEMON: &str = "/mob/living/basic/mining/brimdemon";
pub const LOBSTROSITY: &str = "/mob/living/basic/mining/lobstrosity";
pub const BILEWORM: &str = "/mob/living/basic/mining/bileworm";
pub const GOLDGRUB: &str = "/mob/living/basic/mining/goldgrub";
pub const HIVELORD: &str = "/mob/living/basic/mining/hivelord";
pub const HIVELORD_BROOD: &str = "/mob/living/basic/hivelord_brood";
const LEGION_CORE: &str = "/obj/item/organ/monster_core/regenerative_core/legion";
const REGEN_CORE: &str = "/obj/item/organ/monster_core/regenerative_core";
/// `FACTION_ASHWALKER`.
const ASHWALKER: &str = "ashwalker";
/// `ranged_armour` of mining mobs: `minimum_projectile_force`, `below_projectile_multiplier`
/// (`MINING_MOB_PROJECTILE_VULNERABILITY` is `list(BRUTE)`).
const MIN_PROJECTILE_FORCE: f32 = 30.0;
const BELOW_PROJECTILE_MULTIPLIER: f32 = 0.3;
/// The goldgrub's `generate_loot` weights.
const GRUB_LOOT: [(&str, u32); 4] = [
    ("/obj/item/stack/ore/silver", 4),
    ("/obj/item/stack/ore/gold", 3),
    ("/obj/item/stack/ore/uranium", 3),
    ("/obj/item/stack/ore/diamond", 1),
];

/// A `/mob/living/basic/mining` mob (its `ranged_armour`); the brood skulls have none.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct MiningMob;

/// The ore a goldgrub swallowed (`generate_loot`): it drops them when it dies.
#[derive(Component, Clone, Debug, Default)]
pub struct Swallowed(pub Vec<&'static str>);

pub(super) fn build(app: &mut App) {
    abilities::build(app);
    icemoon::build(app);
    app.add_systems(PostStartup, spawn::populate);
}

/// tg `/mob/living/basic/mining/Initialize` and the kinds' own: their components.
pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let path = def.path.as_str();
    if !is_under(path, MINING_ROOT) && !is_under(path, HIVELORD_BROOD) {
        return;
    }
    if is_under(path, MINING_ROOT) && !is_under(path, LEGION_BROOD) {
        world.entity_mut(mob).insert(MiningMob);
    }
    if is_under(path, GOLDGRUB) && !is_under(path, &format!("{GOLDGRUB}/baby")) {
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let n = rng.range(1, 3);
        let total: u32 = GRUB_LOOT.iter().map(|l| l.1).sum();
        let loot = (0..n)
            .map(|_| {
                let mut roll = rng.range(1, total);
                GRUB_LOOT
                    .iter()
                    .find(|l| {
                        if roll <= l.1 {
                            return true;
                        }
                        roll -= l.1;
                        false
                    })
                    .map_or(GRUB_LOOT[0].0, |l| l.0)
            })
            .collect();
        world.resource_mut::<CombatRng>().0 = rng.0;
        world.entity_mut(mob).insert(Swallowed(loot));
    }
    abilities::attach(world, mob, path);
    icemoon::attach(world, mob, path);
}

/// What a dying lavaland mob leaves (before a `DEL_ON_DEATH` mob is gone): `death_drops`, the
/// goldgrub's ore, the brimdemon's burst.
pub(super) fn on_death(world: &mut World, mob: Entity, def: &MobDef, pos: TilePos) {
    let path = def.path.as_str();
    if is_under(path, LEGION) {
        crate::construction::spawn_obj(world, LEGION_CORE, pos);
    } else if is_under(path, HIVELORD) {
        crate::construction::spawn_obj(world, REGEN_CORE, pos);
    }
    if let Some(Swallowed(ores)) = world.entity_mut(mob).take::<Swallowed>() {
        for ore in ores {
            crate::construction::spawn_stack(world, ore, pos, 1);
        }
    }
    abilities::on_death(world, mob, path);
    icemoon::on_death(world, path, pos);
    crate::mining::necropolis::on_death(world, mob, pos);
}

/// tg `ranged_armour` `pre_bullet_impact` of a mining mob: the damage a projectile does to it.
pub fn ranged_armour(world: &mut World, target: Entity, damage: f32, damage_type: &str) -> f32 {
    if world.get::<MiningMob>(target).is_none() {
        return damage;
    }
    let through = armoured(damage, damage_type);
    if through < damage
        && let Some(&at) = world.get::<TilePos>(target)
    {
        let name = crate::security::name_of(world, target);
        crate::security::show(
            world,
            at,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{name} кажется устойчивым к снаряду!"),
            ),
            &[],
        );
    }
    through
}

/// `pre_bullet_impact`'s multiplier: under 30 and not brute, 30% gets through.
pub fn armoured(damage: f32, damage_type: &str) -> f32 {
    if damage >= MIN_PROJECTILE_FORCE || damage_type == "brute" {
        damage
    } else {
        damage * BELOW_PROJECTILE_MULTIPLIER
    }
}

/// `/mob/living/basic/mining/on_attacked`: an ash walker mob hurt by an ash walker stops being
/// one.
pub fn on_attacked(world: &mut World, victim: Entity, attacker: Entity) {
    if world.get::<MiningMob>(victim).is_none() {
        return;
    }
    let ashwalker = |world: &World, e: Entity| {
        world
            .get::<crate::mobs::Faction>(e)
            .is_some_and(|f| f.0.iter().any(|f| f == ASHWALKER))
    };
    if !ashwalker(world, victim) || !ashwalker(world, attacker) {
        return;
    }
    if let Some(mut faction) = world.get_mut::<crate::mobs::Faction>(victim) {
        faction.0.retain(|f| f != ASHWALKER);
    }
}

/// An ability holds the mob still (tg `do_after` on itself with `PAUSE_DURING_DO_AFTER`): its
/// AI waits.
pub fn busy(world: &World, mob: Entity) -> bool {
    abilities::channeling(world, mob) || icemoon::channeling(world, mob)
}

/// Applies `amount` of `kind` to a living mob on `zone` (a carbon's) through
/// `crate::mobs::defense`.
pub(crate) fn hurt(
    world: &mut World,
    mob: Entity,
    kind: DamageType,
    amount: f32,
    zone: Option<crate::zone::BodyZone>,
) {
    super::defense::hurt(world, mob, kind, amount, zone);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranged_armour_softens_weak_non_brute_projectiles() {
        // A 10 burn watcher blast or laser: 30%.
        assert!((armoured(10.0, "burn") - 3.0).abs() < 1e-6);
        // Kinetic accelerators and bullets are brute: all of it.
        assert_eq!(armoured(10.0, "brute"), 10.0);
        // Strong enough to go through.
        assert_eq!(armoured(30.0, "burn"), 30.0);
    }
}
