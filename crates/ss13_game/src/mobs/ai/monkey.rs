//! Monkey AI, after tg `/datum/ai_controller/monkey` (`code/datums/ai/monkey/`): a carbon
//! monkey nobody controls hates whoever hurts it (85%, 4 hatred), fights the most hated ones in
//! view weighted by closeness (picking up the strongest weapon it sees on the way), runs away
//! below 40 health, and otherwise wanders and grabs better weapons lying around.
//!
//! The monkey fights through the same melee as a player (a [`ClickResolved`] of its own in
//! combat mode, tg `ai_interact(combat_mode = TRUE)`), so punches, held weapons, blocks and
//! armor follow the human rules.
// shortcut: no pickpocketing, disarms, shoves, stuffing into disposals, recruiting other
// monkeys, battle screeches, food serving (Pun Pun), instruments, guns or escaping cuffs; the
// combat and weapon branches cover fleeing, grabbing items and fighting back. Port them with
// the mechanics they need.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

use super::behaviors::{DELAY_FAIL, DELAY_SUCCEED, FAIL, Perform, Strategy, can_see};
use super::{AiController, AiCtx, Key, Value};
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, Inventory};

/// The tg controller type (and `angry`, `BB_MONKEY_AGGRESSIVE`).
pub const MONKEY: &str = "/datum/ai_controller/monkey";
pub const MONKEY_ANGRY: &str = "/datum/ai_controller/monkey/angry";

/// `MONKEY_FLEE_HEALTH`, `MONKEY_ENEMY_VISION`, `MONKEY_RETALIATE_PROB`,
/// `MONKEY_HATRED_AMOUNT`, `MONKEY_HATRED_REDUCTION_PROB`.
pub const FLEE_HEALTH: f32 = 40.0;
const ENEMY_VISION: u32 = 9;
const RETALIATE_PROB: f32 = 85.0;
const HATRED_AMOUNT: f64 = 4.0;
const HATRED_REDUCTION_PROB: f32 = 40.0;
/// `vision_range` of `monkey_find_weapon`.
const WEAPON_VISION: u32 = 5;
/// `is_at_distance` of `monkey_combat` (`target_loss_distance`).
const LOSE_DISTANCE: u32 = 16;

/// The monkey leaves.
#[derive(Debug)]
pub enum MonkeyLeaf {
    /// `monkey_set_combat_target`: an enemy in view (anyone when aggressive), weighted by
    /// `100 / distance`; sets combat mode.
    SetCombatTarget,
    /// `update_interaction_target/monkey_find_weapon`: the strongest item in view stronger than
    /// what it holds.
    FindWeapon,
    /// `monkey_equip/ground`: takes the item next to it.
    EquipGround,
    /// `monkey_attack_mob`: a hit with the held weapon or a punch; a landed hit lowers the
    /// hatred (40%) and drops the target at none.
    Attack,
}

/// Monkey decorators.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MonkeyCond {
    /// `pawn_health_below` 40.
    HealthBelow(f32),
}

impl MonkeyCond {
    pub fn check(self, ctx: &mut AiCtx) -> bool {
        match self {
            Self::HealthBelow(limit) => ctx
                .world
                .get::<Health>(ctx.pawn)
                .is_some_and(|h| h.health() < limit),
        }
    }
}

impl MonkeyLeaf {
    pub fn setup(&mut self, ctx: &mut AiCtx) -> bool {
        match self {
            Self::EquipGround => ctx.truthy(Key::MonkeyPickup),
            Self::Attack => ctx.truthy(Key::CurrentTarget),
            _ => true,
        }
    }

    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        match self {
            Self::SetCombatTarget => set_combat_target(ctx),
            Self::FindWeapon => find_weapon(ctx),
            Self::EquipGround => equip_ground(ctx),
            Self::Attack => attack(ctx),
        }
    }

    pub fn finish(&mut self, ctx: &mut AiCtx, _succeeded: bool) {
        if let Self::EquipGround = self {
            ctx.bb.clear(Key::MonkeyPickup);
        }
    }
}

fn enemies(bb: &super::Blackboard) -> Vec<(Entity, f64)> {
    match bb.get(Key::MonkeyEnemies) {
        Some(Value::Attackers(list)) => list.clone(),
        _ => Vec::new(),
    }
}

/// `add_blackboard_key_assoc(BB_MONKEY_ENEMIES, mob, amount)`.
pub fn add_hatred(bb: &mut super::Blackboard, mob: Entity, amount: f64) {
    let mut list = enemies(bb);
    match list.iter_mut().find(|(e, _)| *e == mob) {
        Some((_, h)) => *h += amount,
        None => list.push((mob, amount)),
    }
    list.retain(|(_, h)| *h > 0.0);
    bb.set(Key::MonkeyEnemies, Value::Attackers(list));
}

/// tg `/datum/ai_controller/monkey/on_attacked` (`COMSIG_ATOM_WAS_ATTACKED`).
pub fn on_attacked(world: &mut World, victim: Entity, attacker: Entity) {
    if victim == attacker
        || !world
            .get::<AiController>(victim)
            .is_some_and(|c| is_monkey(&c.kind))
    {
        return;
    }
    if !world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(RETALIATE_PROB)
    {
        return;
    }
    if let Some(mut c) = world.get_mut::<AiController>(victim) {
        add_hatred(&mut c.bb, attacker, HATRED_AMOUNT);
    }
}

pub fn is_monkey(kind: &str) -> bool {
    ss13_mapload::fidelity::is_under(kind, MONKEY)
}

/// The controller tg gives `/mob/living/carbon/human/species/monkey` (and an admin hands out):
/// `BB_MONKEY_AGGRESSIVE` for `angry`.
pub fn controller(kind: &str) -> AiController {
    let mut bb = super::Blackboard::default();
    bb.set(
        Key::MonkeyAggressive,
        Value::Bool(ss13_mapload::fidelity::is_under(kind, MONKEY_ANGRY)),
    );
    AiController {
        kind: kind.to_owned(),
        tree: super::bt::Tree::new(vec![super::trees::monkey()]),
        bb,
        ..AiController::default()
    }
}

/// Puts the monkey controller on a carbon `mob` nobody controls (tg `ai_controller` of the
/// monkey species type, `InitializeAIController`).
pub fn attach(world: &mut World, mob: Entity, kind: &str) {
    world.entity_mut(mob).insert(controller(kind));
}

fn set_combat_target(ctx: &mut AiCtx) -> Perform {
    let aggressive = ctx.truthy(Key::MonkeyAggressive);
    let list = enemies(ctx.bb);
    let pacifist = ctx
        .world
        .get::<crate::medical::traumas::Pacifist>(ctx.pawn)
        .is_some();
    if pacifist || (list.is_empty() && !aggressive) {
        set_combat_mode(ctx, false);
        return FAIL;
    }
    let Some(from) = ctx.pawn_pos() else {
        return FAIL;
    };
    let mut valids: Vec<(Entity, u32)> = Vec::new();
    for mob in super::behaviors::candidates(ctx, ENEMY_VISION) {
        let world = &*ctx.world;
        let Some(&to) = world.get::<TilePos>(mob) else {
            continue;
        };
        if !can_see(world, from, to, ENEMY_VISION) {
            continue;
        }
        if !list.iter().any(|(e, _)| *e == mob) {
            // Aggressive monkeys spare monkeys and the incapacitated.
            let monkey_kin = world
                .get::<crate::mobs::Faction>(mob)
                .is_some_and(|f| f.0.iter().any(|f| f == "monkey" || f == "jungle"))
                || crate::species::species_of(world, mob) == crate::species::SpeciesId::Monkey;
            let down = world
                .get::<Stat>(mob)
                .is_some_and(|s| *s != Stat::Conscious);
            if !aggressive || monkey_kin || down {
                continue;
            }
        }
        let dist = from.chebyshev(to).unwrap_or(1).max(1);
        valids.push((mob, 100_u32.div_ceil(dist)));
    }
    let total: u32 = valids.iter().map(|(_, w)| w).sum();
    if total == 0 {
        set_combat_mode(ctx, false);
        return FAIL;
    }
    let mut roll = ctx.rng.range(1, total);
    let target = valids
        .iter()
        .find(|(_, w)| {
            if roll <= *w {
                return true;
            }
            roll -= w;
            false
        })
        .map_or(valids[0].0, |(e, _)| *e);
    set_combat_mode(ctx, true);
    ctx.bb.set(Key::CurrentTarget, Value::Entity(target));
    super::behaviors::SUCCEED
}

fn set_combat_mode(ctx: &mut AiCtx, on: bool) {
    if let Some(mut mode) = ctx.world.get_mut::<crate::combat::CombatMode>(ctx.pawn) {
        mode.set_if_neq(crate::combat::CombatMode(on));
    }
}

/// The force of the strongest held item (tg `top_force` of `pick_final_target`).
fn held_force(world: &World, mob: Entity) -> f32 {
    world.get::<Inventory>(mob).map_or(0.0, |inv| {
        inv.hands
            .iter()
            .flatten()
            .filter_map(|&i| world.get::<DmItem>(i))
            .map(|i| i.force)
            .fold(0.0, f32::max)
    })
}

fn find_weapon(ctx: &mut AiCtx) -> Perform {
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_FAIL;
    };
    let world = &*ctx.world;
    let mut top = held_force(world, ctx.pawn);
    let blacklist: Vec<Entity> = match ctx.bb.get(Key::MonkeyBlacklist) {
        Some(Value::Attackers(list)) => list.iter().map(|(e, _)| *e).collect(),
        _ => Vec::new(),
    };
    let mut items: Vec<(u32, Entity)> = world
        .resource::<SpatialIndex>()
        .in_radius(from, WEAPON_VISION)
        .filter(|e| !blacklist.contains(e))
        .filter_map(|e| {
            let item = world.get::<DmItem>(e)?;
            let pos = *world.get::<TilePos>(e)?;
            (item.pickable() && world.get::<CarriedBy>(e).is_none())
                .then_some((from.chebyshev(pos)?, e))
        })
        .filter(|&(_, e)| {
            world
                .get::<TilePos>(e)
                .is_some_and(|&p| can_see(world, from, p, WEAPON_VISION))
        })
        .collect();
    items.sort_by_key(|&(d, e)| (d, e.to_bits()));
    let mut best = None;
    for (_, e) in items {
        let force = world.get::<DmItem>(e).map_or(0.0, |i| i.force);
        if force > top {
            top = force;
            best = Some(e);
        }
    }
    match best {
        Some(e) => {
            ctx.bb.set(Key::MonkeyPickup, Value::Entity(e));
            DELAY_SUCCEED
        }
        None => {
            ctx.bb.clear(Key::MonkeyPickup);
            DELAY_FAIL
        }
    }
}

/// `monkey_equip/ground` -> `equip_item`: a better weapon replaces what it holds (dropped),
/// else a free hand takes it. A failed item is never tried again (`BB_MONKEY_BLACKLISTITEMS`).
fn equip_ground(ctx: &mut AiCtx) -> Perform {
    let Some(item) = ctx.bb.entity(Key::MonkeyPickup) else {
        return DELAY_FAIL;
    };
    let ok = equip(ctx, item);
    if !ok {
        let mut list = match ctx.bb.get(Key::MonkeyBlacklist) {
            Some(Value::Attackers(list)) => list.clone(),
            _ => Vec::new(),
        };
        list.push((item, ctx.now));
        ctx.bb.set(Key::MonkeyBlacklist, Value::Attackers(list));
        return DELAY_FAIL;
    }
    DELAY_SUCCEED
}

fn equip(ctx: &mut AiCtx, item: Entity) -> bool {
    let world = &mut *ctx.world;
    let (Some(&pos), Some(&at)) = (world.get::<TilePos>(ctx.pawn), world.get::<TilePos>(item))
    else {
        return false;
    };
    // Someone may have taken it while it walked over (on the floor only).
    if world.get::<CarriedBy>(item).is_some()
        || !world
            .resource::<SpatialIndex>()
            .adjacent(pos, at, |e| e == item)
    {
        return false;
    }
    let force = world.get::<DmItem>(item).map_or(0.0, |i| i.force);
    let Some(hands) = world.get::<Inventory>(ctx.pawn).map(|i| i.hands) else {
        return false;
    };
    let hand = if force > held_force(world, ctx.pawn) {
        // `drop_all_held_items`.
        for held in hands.iter().flatten() {
            world.entity_mut(*held).remove::<CarriedBy>().insert(pos);
        }
        if let Some(mut inv) = world.get_mut::<Inventory>(ctx.pawn) {
            inv.hands = [None, None];
        }
        0
    } else if let Some(free) = hands.iter().position(Option::is_none) {
        free
    } else {
        return false;
    };
    crate::inventory::pick_up(world, ctx.pawn, item, hand);
    let took = world
        .get::<Inventory>(ctx.pawn)
        .is_some_and(|i| i.holds(item));
    if took && let Some(mut inv) = world.get_mut::<Inventory>(ctx.pawn) {
        inv.active = hand;
    }
    took
}

/// `monkey_attack_mob` -> `monkey_attack`: one click on the target in combat mode when the
/// click cooldown allows.
fn attack(ctx: &mut AiCtx) -> Perform {
    let Some(target) = ctx.bb.entity(Key::CurrentTarget) else {
        return DELAY_FAIL;
    };
    let (Some(from), Some(&to)) = (ctx.pawn_pos(), ctx.world.get::<TilePos>(target)) else {
        ctx.bb.clear(Key::CurrentTarget);
        return DELAY_FAIL;
    };
    // `is_at_distance` (lost past 16 tiles) and `mob_stat_at_least` of `monkey_combat`: a
    // target that got away or went down is dropped.
    let down = ctx
        .world
        .get::<Stat>(target)
        .is_none_or(|s| *s == Stat::Dead);
    if down || from.chebyshev(to).is_none_or(|d| d > LOSE_DISTANCE) {
        ctx.bb.clear(Key::CurrentTarget);
        return DELAY_FAIL;
    }
    let ready = ctx
        .world
        .get::<crate::combat::NextMove>(ctx.pawn)
        .is_none_or(|n| ctx.now >= n.0);
    let reach = ctx
        .world
        .resource::<SpatialIndex>()
        .adjacent(from, to, |e| e == target);
    // `BB_TARGETING_STRATEGY` `basic/monkey`: enemies skip the faction check.
    if !ready || !reach || !Strategy::IgnoreFaction.valid(ctx, target, 0) {
        return DELAY_FAIL;
    }
    set_combat_mode(ctx, true);
    // No client: the melee consumers act on `mob`; nothing is sent to a client of an AI.
    ctx.world.write_message(crate::click::ClickResolved {
        client: Entity::PLACEHOLDER,
        mob: ctx.pawn,
        mob_pos: from,
        target,
        target_pos: to,
        modifiers: default(),
    });
    if ctx.truthy(Key::MonkeyAggressive) {
        return DELAY_FAIL;
    }
    // A landed attack calms it down a little (`MONKEY_HATRED_REDUCTION_PROB`).
    let known = enemies(ctx.bb).iter().any(|(e, _)| *e == target);
    if !known {
        add_hatred(ctx.bb, target, 1.0);
    }
    if ctx.prob(HATRED_REDUCTION_PROB) {
        add_hatred(ctx.bb, target, -1.0);
        if !enemies(ctx.bb).iter().any(|(e, _)| *e == target) {
            ctx.bb.clear(Key::CurrentTarget);
        }
    }
    DELAY_SUCCEED
}
