//! Cultist pets (server), after tg `/datum/element/cultist_pet` (`datums/elements/pet_cult.dm`)
//! and the `pet_cult` controller (`pets/pet_cult/pet_cult_ai.dm`, `pet_cult_bt.dm`,
//! `pet_cult.bt.json`).
//!
//! - A living dog or cat offered on the cult's offer rune (`COMSIG_LIVING_CULT_SACRIFICED` from
//!   `crate::antag::cult`) still counts as a sacrifice but is not gibbed: it turns cultist. It
//!   forgets its commands and obeys the cult's (stay, free, follow, attack), bites for 10-15,
//!   has at least 50 health and is healed, serves only the `cult` faction, takes its cult look
//!   (corgis `narsian`, pugs `pug_cult`, cats `cat_cult`, other dogs a rune red) and its new
//!   controller ([`CultLeaf`] through `crate::mobs::ai::fauna`). Dead, it bursts into gibs and
//!   leaves a soulstone (`death_drops`, `DEL_ON_DEATH`).
//! - The controller, by priority: pet commands; walking up to a cultist it is not yet friends
//!   with to befriend it ("eagerly awaits your command..."); while the cult can revive (three
//!   sacrifices unused, `SOULS_TO_REVIVE`), activating a revival rune within 9 tiles with a dead
//!   cultist on it (`raise_dead/invoke` with the pet as invoker, every 3 s), else dragging a
//!   dead cultist it sees (not buckled, not pulled, not on such a rune) onto the last revival
//!   rune it saw; else biting non-cultists (`targeting_strategy/basic/cultist`) or wandering.
//! - A pet already in the cult is no target of the offer rune again.
//!
//! Cost: the finders run every 1.5-5 s per cult pet over a 9-tile radius.
// shortcut: no "Create Revival Rune" ability and command (5 s conjure, 30 s cooldown), no cult
// indicator overlay on mindless pets, no eating organs and blood, no `move` command, no cult
// antag datum for a pet with a player; add with the pet ability actions.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use super::BasicMob;
use super::ai::behaviors::{DELAY_FAIL, DELAY_SUCCEED, Perform, can_see};
use super::ai::{AiController, AiCtx, Key, Value};
use super::pets::{ObeysCommands, PetCommand};
use crate::antag::cult::server::{CultTeam, Rune, RuneKind, can_revive_dead, is_cultist};
use crate::health::{Health, Stat};
use crate::medical::surgery::Buckled;

/// The controller cult pets get.
pub const CONTROLLER: &str = "/datum/ai_controller/basic_controller/pet_cult";
const DOG: &str = "/mob/living/basic/pet/dog";
const CORGI: &str = "/mob/living/basic/pet/dog/corgi";
const PUG: &str = "/mob/living/basic/pet/dog/pug";
const CAT: &str = "/mob/living/basic/pet/cat";
const SOULSTONE: &str = "/obj/item/soulstone";
/// `FACTION_CULT`.
pub const FACTION_CULT: &str = "cult";
/// `PET_CULT_ATTACK_UPPER`, `PET_CULT_HEALTH`.
const ATTACK_UPPER: f32 = 15.0;
const CULT_HEALTH: f32 = 50.0;
/// `RUNE_COLOR_MEDIUMRED`.
const MEDIUM_RED: [u8; 4] = [0xC8, 0x00, 0x00, 0xFF];
/// `BB_FRIENDLY_MESSAGE`.
const FRIENDLY_MESSAGE: &str = "с нетерпением ждёт ваших приказов...";
const VISION: u32 = 9;
/// The cult's commands (`/datum/pet_command/idle`, `free`, `follow`, `attack`).
const CULT_COMMANDS: &[PetCommand] = &[
    PetCommand::Stay,
    PetCommand::Free,
    PetCommand::Follow,
    PetCommand::Attack,
];

/// A pet that joined the cult.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CultPet;

/// `IS_CULTIST_OR_CULTIST_MOB`: a cultist, or a mob of the cult's faction.
pub fn is_cultist_or_cult_mob(world: &World, mob: Entity) -> bool {
    is_cultist(world, mob)
        || world
            .get::<super::Faction>(mob)
            .is_some_and(|f| f.0.iter().any(|x| x == FACTION_CULT))
}

/// The `cultist_pet` element's `pet_cult_icon_state` of mob type `path` (`None`: the red tint);
/// `None` outside dogs and cats.
fn cult_look(path: &str) -> Option<Option<&'static str>> {
    if is_under(path, CORGI) {
        Some(Some("narsian"))
    } else if is_under(path, PUG) {
        Some(Some("pug_cult"))
    } else if is_under(path, DOG) {
        Some(None)
    } else if is_under(path, CAT) {
        Some(Some("cat_cult"))
    } else {
        None
    }
}

/// A target the offer rune passes over: a pet already in the cult (offering it again would only
/// count another sacrifice).
pub fn spared_by_offer(world: &World, mob: Entity) -> bool {
    world.get::<CultPet>(mob).is_some()
}

/// `cultist_pet/become_cultist` on `COMSIG_LIVING_CULT_SACRIFICED`: true for `STOP_SACRIFICE`
/// (the pet turned, it is not gibbed).
pub fn become_cultist(world: &mut World, pet: Entity) -> bool {
    let Some(path) = world.get::<BasicMob>(pet).map(|b| b.def.path.clone()) else {
        return false;
    };
    let Some(look) = cult_look(&path) else {
        return false;
    };
    if world.get::<Stat>(pet) == Some(&Stat::Dead) {
        return false;
    }
    if world.get::<CultPet>(pet).is_some() {
        return true;
    }
    // Stats, the death drops and the new controller on the definition.
    let def = {
        let Some(mut basic) = world.get_mut::<BasicMob>(pet) else {
            return false;
        };
        let def = std::sync::Arc::make_mut(&mut basic.def);
        def.melee_damage_lower = def.melee_damage_lower.max(ATTACK_UPPER - 5.0);
        def.melee_damage_upper = def.melee_damage_upper.max(ATTACK_UPPER);
        def.max_health = def.max_health.max(CULT_HEALTH);
        // tg's `basic_mob_flags &= DEL_ON_DEATH` meant `|=`: the drops replace the body.
        def.basic_mob_flags |= super::DEL_ON_DEATH;
        def.ai_controller = CONTROLLER.to_owned();
        if let Some(state) = look {
            basic.icon_living = state.to_owned();
        }
        basic.def.clone()
    };
    // `fully_heal`: no damage, the basic body kept.
    if let Some(old) = world.get::<Health>(pet).cloned() {
        world.entity_mut(pet).insert(Health {
            max: def.max_health,
            basic: old.basic,
            mods: old.mods,
            ..Health::default()
        });
    }
    world.entity_mut(pet).insert((
        Stat::Conscious,
        super::Faction(vec![FACTION_CULT.to_owned()]),
        CultPet,
        ObeysCommands {
            commands: CULT_COMMANDS,
            follow_on_befriend: false,
        },
    ));
    if let Some(controller) = AiController::for_mob(&def) {
        world.entity_mut(pet).insert(controller);
    }
    if look.is_none()
        && let Some(mut appearance) = world.get_mut::<ss13_shared::Appearance>(pet)
    {
        appearance.color = Some(MEDIUM_RED);
    }
    super::set_look(world, pet, false);
    info!("{pet} joined the cult as a pet");
    true
}

/// `death_drops` of a cult pet (before `DEL_ON_DEATH` takes it): gibs and a soulstone.
pub fn on_death(world: &mut World, mob: Entity, pos: TilePos) {
    if world.get::<CultPet>(mob).is_none() {
        return;
    }
    crate::effects::gibs::spawn_gibs(world, crate::effects::gibs::Gibspawner::Generic, pos);
    crate::construction::spawn_obj(world, SOULSTONE, pos);
}

/// The `pet_cult` controller's own leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CultLeaf {
    /// `update_interaction_target` of `BB_FRIENDLY_CULTIST` (`befriendable_cultist`, 9 tiles,
    /// every 5 s).
    FindFriendlyCultist,
    /// `befriend_target` of `BB_FRIENDLY_CULTIST` (forgotten after).
    Befriend,
    /// `find_occupied_rune` (every 3 s).
    FindOccupiedRune,
    /// `activate_rune` (every 3 s).
    ActivateRune,
    /// `find_dead_cultist`.
    FindDeadCultist,
    /// `drag_target` of `BB_DEAD_CULTIST`.
    Drag,
}

impl CultLeaf {
    /// tg `time_between_perform`.
    pub fn cooldown(self) -> f64 {
        match self {
            Self::FindFriendlyCultist => 5.0,
            Self::FindOccupiedRune | Self::ActivateRune => 3.0,
            _ => 0.0,
        }
    }

    pub fn perform(self, ctx: &mut AiCtx) -> Perform {
        match self {
            Self::FindFriendlyCultist => find_friendly(ctx),
            Self::Befriend => befriend(ctx),
            Self::FindOccupiedRune => find_occupied_rune(ctx),
            Self::ActivateRune => activate_rune(ctx),
            Self::FindDeadCultist => find_dead_cultist(ctx),
            Self::Drag => drag(ctx),
        }
    }

    pub fn finish(self, ctx: &mut AiCtx) {
        match self {
            Self::Befriend => ctx.bb.clear(Key::FriendlyCultist),
            Self::ActivateRune => ctx.bb.clear(Key::OccupiedRune),
            _ => {}
        }
    }
}

/// Living things the pawn sees within [`VISION`], nearest first.
fn seen(ctx: &AiCtx) -> Vec<(Entity, TilePos)> {
    let Some(from) = ctx.pawn_pos() else {
        return Vec::new();
    };
    let world = &*ctx.world;
    let mut found: Vec<(u32, Entity, TilePos)> = world
        .resource::<SpatialIndex>()
        .in_radius(from, VISION)
        .filter(|&e| e != ctx.pawn)
        .filter_map(|e| {
            let pos = *world.get::<TilePos>(e)?;
            let d = from.chebyshev(pos)?;
            can_see(world, from, pos, VISION).then_some((d, e, pos))
        })
        .collect();
    found.sort_by_key(|&(d, e, _)| (d, e));
    found.into_iter().map(|(_, e, p)| (e, p)).collect()
}

fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::human::HumanLook>(mob).is_some()
}

/// The cult has souls for another revival (`LAZYLEN(GLOB.sacrificed) - SOULS_TO_REVIVE -
/// GLOB.sacrifices_used >= 0`).
fn can_revive(world: &World) -> bool {
    world
        .get_resource::<CultTeam>()
        .is_some_and(|t| can_revive_dead(t.sacrificed, t.sacrifices_used))
}

fn is_revive_rune(world: &World, e: Entity) -> bool {
    world
        .get::<Rune>(e)
        .is_some_and(|r| r.kind == RuneKind::Revive)
}

fn revive_rune_at(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| is_revive_rune(world, e))
}

fn dead_cultist(world: &World, mob: Entity) -> bool {
    is_human(world, mob) && world.get::<Stat>(mob) == Some(&Stat::Dead) && is_cultist(world, mob)
}

fn find_friendly(ctx: &mut AiCtx) -> Perform {
    let world = &*ctx.world;
    let found = seen(ctx).into_iter().find(|&(e, _)| {
        is_human(world, e)
            && world.get::<Stat>(e).is_some_and(|s| *s != Stat::Dead)
            && is_cultist(world, e)
            && !super::pets::allied(world, ctx.pawn, e)
    });
    match found {
        Some((e, _)) => {
            ctx.bb.set(Key::FriendlyCultist, Value::Entity(e));
            DELAY_SUCCEED
        }
        None => {
            ctx.bb.clear(Key::FriendlyCultist);
            DELAY_FAIL
        }
    }
}

fn befriend(ctx: &mut AiCtx) -> Perform {
    let Some(friend) = ctx.bb.entity(Key::FriendlyCultist) else {
        return DELAY_FAIL;
    };
    let near = ctx
        .pawn_pos()
        .zip(ctx.target_pos(Key::FriendlyCultist))
        .and_then(|(a, b)| a.chebyshev(b))
        .is_some_and(|d| d <= 1);
    if !near {
        return DELAY_FAIL;
    }
    super::pets::befriend(ctx.world, ctx.pawn, friend);
    let name = super::defense::name_of(ctx.world, ctx.pawn);
    crate::tools::tell_mob(
        ctx.world,
        friend,
        Span::Notice,
        format!("{name} {FRIENDLY_MESSAGE}"),
    );
    DELAY_SUCCEED
}

fn find_occupied_rune(ctx: &mut AiCtx) -> Perform {
    if !can_revive(ctx.world) {
        ctx.bb.clear(Key::OccupiedRune);
        return DELAY_FAIL;
    }
    for (rune, pos) in seen(ctx) {
        if !is_revive_rune(ctx.world, rune) {
            continue;
        }
        ctx.bb.set(Key::NearbyRune, Value::Entity(rune));
        let occupied = ctx
            .world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .any(|&e| dead_cultist(ctx.world, e));
        if occupied {
            ctx.bb.set(Key::OccupiedRune, Value::Entity(rune));
            return DELAY_SUCCEED;
        }
    }
    DELAY_FAIL
}

fn activate_rune(ctx: &mut AiCtx) -> Perform {
    let Some(rune) = ctx
        .bb
        .entity(Key::OccupiedRune)
        .filter(|&r| ctx.world.get_entity(r).is_ok())
    else {
        return DELAY_FAIL;
    };
    let (Some(from), Some(at)) = (ctx.pawn_pos(), ctx.target_pos(Key::OccupiedRune)) else {
        return DELAY_FAIL;
    };
    if from.chebyshev(at).is_none_or(|d| d > 1) {
        return DELAY_FAIL;
    }
    let occupied = ctx
        .world
        .resource::<SpatialIndex>()
        .on_tile(at)
        .iter()
        .any(|&e| dead_cultist(ctx.world, e));
    if !occupied {
        return DELAY_FAIL;
    }
    // `ai_interact` -> `cultist_pet/activate_rune` -> `attack_hand`: the pet invokes it alone.
    crate::antag::cult::server::invoke_revive(ctx.world, rune, &[ctx.pawn]);
    DELAY_SUCCEED
}

fn find_dead_cultist(ctx: &mut AiCtx) -> Perform {
    if !can_revive(ctx.world) {
        ctx.bb.clear(Key::DeadCultist);
        return DELAY_FAIL;
    }
    if ctx.world.get::<crate::pull::Pulling>(ctx.pawn).is_some() {
        return DELAY_FAIL;
    }
    let world = &*ctx.world;
    let found = seen(ctx).into_iter().find(|&(e, pos)| {
        dead_cultist(world, e)
            && world.get::<Buckled>(e).is_none()
            && world.get::<crate::pull::PulledBy>(e).is_none()
            && !revive_rune_at(world, pos)
    });
    match found {
        Some((e, _)) => {
            ctx.bb.set(Key::DeadCultist, Value::Entity(e));
            DELAY_SUCCEED
        }
        None => DELAY_FAIL,
    }
}

fn drag(ctx: &mut AiCtx) -> Perform {
    let Some(target) = ctx
        .bb
        .entity(Key::DeadCultist)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return DELAY_FAIL;
    };
    if ctx.world.get::<crate::pull::Pulling>(ctx.pawn) == Some(&crate::pull::Pulling(target)) {
        return DELAY_SUCCEED;
    }
    if ctx.world.get::<crate::pull::PulledBy>(target).is_some() {
        return DELAY_FAIL;
    }
    let adjacent = ctx
        .pawn_pos()
        .zip(ctx.target_pos(Key::DeadCultist))
        .and_then(|(a, b)| a.chebyshev(b))
        .is_some_and(|d| d <= 1);
    if !adjacent || !crate::pull::can_be_pulled(ctx.world, ctx.pawn, target) {
        return DELAY_FAIL;
    }
    ctx.world.entity_mut(ctx.pawn).insert((
        crate::pull::Pulling(target),
        crate::pull::GrabState::Passive,
        crate::pull::PullSlowdown(0.0),
    ));
    DELAY_SUCCEED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cult_looks_follow_the_pet_types() {
        assert_eq!(
            cult_look("/mob/living/basic/pet/dog/corgi/ian"),
            Some(Some("narsian"))
        );
        assert_eq!(
            cult_look("/mob/living/basic/pet/dog/pug"),
            Some(Some("pug_cult"))
        );
        assert_eq!(
            cult_look("/mob/living/basic/pet/dog/bullterrier"),
            Some(None)
        );
        assert_eq!(
            cult_look("/mob/living/basic/pet/cat/kitten"),
            Some(Some("cat_cult"))
        );
        assert_eq!(cult_look("/mob/living/basic/mouse"), None);
    }
}
