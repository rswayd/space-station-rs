//! Illusions (server), after tg `/mob/living/basic/illusion` (`code/modules/mob/living/basic/
//! illusion/`): a copy of a living mob's look and name that fights (or flees) for a few seconds
//! and vanishes ("It was a fake!"). [`spawn_illusion`] is tg `full_setup`; its callers (the
//! reactive armour, the hallucination wand, cult and heretic magic, the MOD decoy) belong to
//! their own groups.
//!
//! - `illusion`: `simple_hostile` at its target; `shover`: shoves a carbon instead of hitting;
//!   `escape` (and `escape/retaliate`, whose tg tree is the same): runs from its target and
//!   anyone who hits it, never attacks its original; `mirage`: no AI, not dense, sparks on death.
//! - Each attack it makes and each hit it takes may `replicate` it (`multiply_chance`): a clone
//!   with half its health and half the chance, for 8 s.
// tg `mock_as` applies `hp`, `damage` and `replicate` only when they are null (`isnull` where
// `!isnull` is meant), so no illusion ever gets them; they are applied here. tg's escape
// illusions put their target into `BB_BASIC_MOB_FLEE_TARGET`, which their tree never reads
// (it flees `BB_CURRENT_TARGET`); the target goes there here.
// shortcut: examining an illusion shows its own description, not its original's (tg forwards
// `examine`); add it when examine can be forwarded to another entity.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::{Appearance, Facing, TilePos};

use super::ai::{AiController, Key, Value};
use super::{BasicMob, Faction};
use crate::combat::CombatRng;
use crate::health::{Health, Stat};

pub const ILLUSION: &str = "/mob/living/basic/illusion";
pub const SHOVER: &str = "/mob/living/basic/illusion/shover";
pub const ESCAPE: &str = "/mob/living/basic/illusion/escape";
pub const ESCAPE_RETALIATE: &str = "/mob/living/basic/illusion/escape/retaliate";
pub const MIRAGE: &str = "/mob/living/basic/illusion/mirage";
/// The illusion types spawned by path (`GAME_RUNTIME_TYPES`).
pub const TYPES: &[&str] = &[ILLUSION, SHOVER, ESCAPE, ESCAPE_RETALIATE, MIRAGE];

/// `replicate`: the clone's `life`.
const CLONE_LIFE: f64 = 8.0;

/// `full_setup` arguments (tg defaults: 5 s, 100 health, no damage, no replication).
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub target: Option<Entity>,
    pub faction: Option<Vec<String>>,
    /// Seconds until it vanishes.
    pub life: f64,
    pub hp: f32,
    pub damage: f32,
    /// `multiply_chance`, percent.
    pub replicate: f32,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            target: None,
            faction: None,
            life: 5.0,
            hp: 100.0,
            damage: 0.0,
            replicate: 0.0,
        }
    }
}

/// An illusion's state: whom it copies, when it vanishes, its replication chance.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Illusion {
    pub original: Entity,
    pub path: String,
    pub dies_at: f64,
    pub multiply_chance: f32,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(Update, expire.before(crate::health::HealthSystems));
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `new path(pos)` + `full_setup(original, ...)`. `None` without the prototype or the original.
pub fn spawn_illusion(
    world: &mut World,
    path: &str,
    original: Entity,
    pos: TilePos,
    setup: Setup,
) -> Option<Entity> {
    if world.get_entity(original).is_err() {
        return None;
    }
    let mob = super::spawn::spawn_mob(world, path, pos)?;
    let basic = world.get::<BasicMob>(mob).cloned()?;
    // `mock_as`: the look, the facing, health and damage.
    let look = world.get::<Appearance>(original).cloned();
    let facing = world.get::<Facing>(original).copied();
    let name = world.get::<Name>(original).cloned();
    let mut def = (*basic.def).clone();
    def.max_health = setup.hp;
    def.health = setup.hp;
    def.melee_damage_lower = setup.damage;
    def.melee_damage_upper = setup.damage;
    // `SET_FACTION_AND_ALLIES_FROM(original)` without `FACTION_NEUTRAL`, or the override.
    let faction = setup.faction.clone().unwrap_or_else(|| {
        world
            .get::<Faction>(original)
            .map_or_else(Vec::new, |f| f.0.clone())
            .into_iter()
            .filter(|f| f != Faction::NEUTRAL)
            .collect()
    });
    let physiology = def.physiology;
    let dies_at = now(world) + setup.life;
    let mut e = world.entity_mut(mob);
    e.insert((
        BasicMob {
            def: Arc::new(def),
            ..basic
        },
        Health::basic(setup.hp, physiology),
        Faction(faction),
        Illusion {
            original,
            path: path.to_owned(),
            dies_at,
            multiply_chance: setup.replicate,
        },
    ));
    if let Some(look) = look {
        e.insert(look);
    }
    if let Some(facing) = facing {
        e.insert(facing);
    }
    if let Some(name) = name {
        e.insert(name);
    }
    if let Some(target) = setup.target
        && let Some(mut ai) = world.get_mut::<AiController>(mob)
    {
        ai.bb.set(Key::CurrentTarget, Value::Entity(target));
    }
    Some(mob)
}

/// `addtimer(death, life)`.
fn expire(world: &mut World) {
    let now = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &Illusion, &Stat)>()
        .iter(world)
        .filter(|(_, i, s)| i.dies_at <= now && **s != Stat::Dead)
        .map(|(e, _, _)| e)
        .collect();
    for mob in due {
        if world.get::<Illusion>(mob).is_some_and(|i| i.path == MIRAGE)
            && let Some(&pos) = world.get::<TilePos>(mob)
        {
            // `mirage/death`: `do_sparks(rand(3, 6))`.
            let mut rng = CombatRng(world.resource::<CombatRng>().0);
            let n = rng.range(3, 6);
            world.resource_mut::<CombatRng>().0 = rng.0;
            crate::effects::particles::do_sparks(world, n, false, pos);
        }
        world.entity_mut(mob).insert(Stat::Dead);
    }
}

/// `try_replicate`: `prob(multiply_chance)`, a clone of the same original at the same target,
/// with half the health and half the chance, for 8 s.
fn try_replicate(world: &mut World, mob: Entity) {
    let Some(ill) = world.get::<Illusion>(mob).cloned() else {
        return;
    };
    if ill.multiply_chance <= 0.0 || world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let roll = rng.chance(ill.multiply_chance);
    world.resource_mut::<CombatRng>().0 = rng.0;
    if !roll {
        return;
    }
    let (Some(&pos), Some(health), Some(basic)) = (
        world.get::<TilePos>(mob),
        world.get::<Health>(mob).map(Health::health),
        world.get::<BasicMob>(mob).cloned(),
    ) else {
        return;
    };
    let target = world
        .get::<AiController>(mob)
        .and_then(|ai| ai.bb.entity(Key::CurrentTarget));
    let faction = world.get::<Faction>(mob).map(|f| f.0.clone());
    // tg `new(loc)`: always the base illusion type.
    spawn_illusion(
        world,
        ILLUSION,
        ill.original,
        pos,
        Setup {
            target,
            faction,
            life: CLONE_LIFE,
            hp: health / 2.0,
            damage: basic.def.melee_damage_upper,
            replicate: ill.multiply_chance / 2.0,
        },
    );
}

/// `COMSIG_MOB_AFTER_APPLY_DAMAGE` (`on_damage_taken`).
pub fn on_attacked(world: &mut World, victim: Entity) {
    if world.get::<Illusion>(victim).is_some() {
        try_replicate(world, victim);
    }
}

/// `COMSIG_HOSTILE_PRE_ATTACKINGTARGET` (`on_preattack`): may replicate; true cancels the hit
/// (a shover's shove, an escape illusion's own original).
pub fn on_preattack(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(ill) = world.get::<Illusion>(mob).cloned() else {
        return false;
    };
    try_replicate(world, mob);
    if is_under(&ill.path, ESCAPE) && target == ill.original {
        return true;
    }
    // `shover`: `disarm(target)` succeeds on a carbon.
    if ill.path == SHOVER && world.get::<crate::inventory::Inventory>(target).is_some() {
        crate::security::shove(world, mob, target);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn illusions_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        assert!(TYPES.iter().all(|p| registry.contains(p)));
    }

    #[test]
    fn escape_illusions_spare_their_original_and_others_hit() {
        let mut world = World::new();
        world.insert_resource(CombatRng(3));
        let original = world.spawn_empty().id();
        let stranger = world.spawn_empty().id();
        let illusion = |world: &mut World, path: &str| {
            world
                .spawn((
                    Illusion {
                        original,
                        path: path.to_owned(),
                        dies_at: 5.0,
                        multiply_chance: 0.0,
                    },
                    Stat::Conscious,
                ))
                .id()
        };
        let escape = illusion(&mut world, ESCAPE_RETALIATE);
        assert!(on_preattack(&mut world, escape, original));
        assert!(!on_preattack(&mut world, escape, stranger));
        let plain = illusion(&mut world, ILLUSION);
        assert!(!on_preattack(&mut world, plain, original));
        // Not a carbon: a shover hits as usual.
        let shover = illusion(&mut world, SHOVER);
        assert!(!on_preattack(&mut world, shover, stranger));
        // `multiply_chance` 0: never a clone.
        let before = world.entities().len();
        on_attacked(&mut world, plain);
        assert_eq!(world.entities().len(), before);
    }

    #[test]
    fn full_setup_defaults_follow_tg() {
        let s = Setup::default();
        assert_eq!(
            (s.life, s.hp, s.damage, s.replicate),
            (5.0, 100.0, 0.0, 0.0)
        );
    }
}
