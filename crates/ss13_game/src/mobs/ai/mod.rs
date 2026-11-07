//! Mob AI (server), after tg `/datum/ai_controller` (`code/datums/ai/_ai_controller.dm`,
//! `basic_mobs/base_basic_controller.dm`) and `SSai_controllers`.
//!
//! An [`AiController`] holds the mob's behavior tree ([`bt`], built per tg controller type in
//! [`trees`]), its blackboard and its movement order ([`movement`]). Every
//! [`AI_TICK_SECS`] the ticker plans the controllers that are on (tg `SelectBehaviors` with the
//! real time since the controller's last tick as `seconds_per_tick`); movement steps every
//! game tick at the mob's speed.
//!
//! Status (tg `get_expected_ai_status`, `get_active_ai_status`): off while dead (unless
//! `CAN_ACT_WHILE_DEAD`), off the map or player-controlled; on in a station or shuttle area,
//! whoever is around; on with a living player within [`INTERESTING_DIST`] (tg
//! `has_nearby_client` over the spatial grid cells of `interesting_dist`); otherwise on-low with
//! `RUN_WHILE_UNWATCHED` (on with `ALWAYS_HIGH_PRIORITY`), else off. On-low controllers plan at
//! the same rate (tg's low-priority ticker has the same `wait`, only a lower MC priority).

pub mod behaviors;
pub mod bt;
pub mod fauna;
pub mod movement;
#[cfg(test)]
mod tests;
pub mod trees;

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::mobs::MobDef;
use ss13_mapload::{Areas, DmPower};
use ss13_shared::TilePos;

use crate::combat::CombatRng;
use crate::ghost::Ghost;
use crate::health::Stat;
use crate::server::PlayerMob;

/// `SSai_controllers` `wait`: 0.25 s.
pub const AI_TICK_SECS: f64 = 0.25;
/// `AI_DEFAULT_INTERESTING_DIST`.
pub const INTERESTING_DIST: u32 = 10;

/// Blackboard keys (tg `BB_*`) the ported trees use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// `BB_CURRENT_TARGET`.
    CurrentTarget,
    /// `BB_BASIC_MOB_FLEE_TARGET`.
    FleeTarget,
    /// `BB_FLEE_LOCATION`.
    FleeLocation,
    /// `BB_BASIC_MOB_STOP_FLEEING`.
    StopFleeing,
    /// `BB_BASIC_MOB_MELEE_COOLDOWN_TIMER`.
    MeleeCooldown,
    /// `BB_MOUSE_TARGET` (a cat's prey).
    MouseTarget,
    /// `BB_LOW_PRIORITY_HUNTING_TARGET` (a mouse's cable).
    LowPriorityHunting,
    /// `BB_MOUSE_WANTS_TO_EAT_CABLE`.
    WantsToEatCable,
    /// `BB_MOUSE_CABLE_HUNT_COOLDOWN`.
    CableHuntCooldown,
    /// `BB_CARP_MIGRATION_TARGET`, `BB_CARP_MIGRATION_PATH`.
    MigrationTarget,
    MigrationPath,
    /// `BB_BASIC_MOB_RETALIATE_LIST`.
    RetaliateList,
    /// Bots (`crate::bots::ai`): `BB_BEACON_TARGET`, `BB_PREVIOUS_BEACON_TARGET`,
    /// `BB_BOT_SUMMON_TARGET`, `BB_BOT_BEACON_COOLDOWN`, `BB_SALUTE_COOLDOWN`,
    /// `BB_TEMPORARY_IGNORE_LIST` (target -> until), `BB_PATIENT_IN_CRIT`,
    /// `BB_POST_CLEAN_COOLDOWN`, and the medbot's and firebot's announcement cooldowns.
    BeaconTarget,
    PreviousBeacon,
    SummonTarget,
    BeaconCooldown,
    SaluteCooldown,
    IgnoreList,
    PatientInCrit,
    PostCleanCooldown,
    AnnounceCooldown,
    /// `BB_MULEBOT_TRAVEL_TARGET` (the beacon a MULEbot drives to).
    TravelTarget,
    /// The hygienebot's `BB_WASH_FRUSTRATION` (a count in [`Value::Time`]) and
    /// `BB_TRASH_TALK_COOLDOWN`; the cleanbot's `BB_ACID_SPRAY_COOLDOWN`.
    WashFrustration,
    TrashTalkCooldown,
    AcidSprayCooldown,
    /// The honkbot's `BB_CLOWN_FRIEND`, the vibebot's `BB_VIBEBOT_PARTY_TARGET`.
    FriendTarget,
    /// `BB_TEMPORARILY_IGNORE_FACTION`: the basic targeting strategy skips its faction check.
    IgnoreFaction,
    /// `BB_TARGET_FOOD`, `BB_NEXT_FOOD_EAT` (`find_food`, `move_to_and_eat`).
    TargetFood,
    NextFoodEat,
    /// Robot customers (`crate::restaurant`): `BB_CUSTOMER_MY_SEAT`, `BB_CUSTOMER_EXIT_PORTAL`.
    CustomerSeat,
    CustomerExit,
    /// `BB_ACTIVE_PET_COMMAND` ([`Value::Pet`]) and `BB_CURRENT_PET_TARGET`
    /// (`crate::mobs::pets`).
    ActivePetCommand,
    PetTarget,
    /// `BB_BASIC_MOB_TIP_REACTING`, `BB_BASIC_MOB_TIPPER` (`crate::mobs::farm`).
    TipReacting,
    Tipper,
    /// `BB_BREED_READY`, `BB_BABIES_TARGET`, `BB_PARTNER_SEARCH_TIMEOUT`, `BB_FUCKS`.
    BreedReady,
    BabiesTarget,
    PartnerSearchTimeout,
    WantsToBreed,
    /// The turtle's `BB_TURTLE_HEADBUTT_VICTIM`, `BB_TURTLE_HEADBUTT_COOLDOWN`,
    /// `BB_TURTLE_FLORA_TARGET`, `BB_TURTLE_FLORA_COOLDOWN`.
    HeadbuttVictim,
    HeadbuttCooldown,
    FloraTarget,
    FloraCooldown,
    /// `BB_HIVE_PARTNER`.
    HivePartner,
    /// `ai_target_timer`'s `BB_BASIC_MOB_HAS_TARGET_TIME`: since when ([`Value::Time`]) the
    /// target in `TargetTimerOf` has been the current one (`crate::mobs::lavaland::ai`).
    TargetTimer,
    TargetTimerOf,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Entity(Entity),
    Tile(TilePos),
    Bool(bool),
    /// Game time, seconds.
    Time(f64),
    Tiles(Vec<TilePos>),
    /// Attacker -> game time of its last attack.
    Attackers(Vec<(Entity, f64)>),
    /// A pet command (`BB_ACTIVE_PET_COMMAND`).
    Pet(crate::mobs::pets::PetCommand),
}

/// tg `blackboard`.
#[derive(Debug, Default, Clone)]
pub struct Blackboard(pub HashMap<Key, Value>);

impl Blackboard {
    pub fn get(&self, key: Key) -> Option<&Value> {
        self.0.get(&key)
    }

    pub fn set(&mut self, key: Key, value: Value) {
        self.0.insert(key, value);
    }

    pub fn clear(&mut self, key: Key) {
        self.0.remove(&key);
    }

    pub fn entity(&self, key: Key) -> Option<Entity> {
        match self.0.get(&key) {
            Some(Value::Entity(e)) => Some(*e),
            _ => None,
        }
    }

    pub fn time(&self, key: Key) -> Option<f64> {
        match self.0.get(&key) {
            Some(Value::Time(t)) => Some(*t),
            _ => None,
        }
    }
}

/// What one controller tick works with.
pub struct AiCtx<'w> {
    pub world: &'w mut World,
    pub pawn: Entity,
    pub bb: &'w mut Blackboard,
    pub movement: &'w mut movement::AiMovement,
    pub now: f64,
    /// tg `seconds_per_tick` of this tick.
    pub spt: f64,
    pub rng: CombatRng,
    /// tg `active_execution_index`.
    pub active: u32,
    /// tg `cancelled_during_tick`.
    pub cancelled: bool,
}

impl AiCtx<'_> {
    /// tg `blackboard_key_exists`: a living entity, a non-empty list, a true or set value.
    pub fn truthy(&self, key: Key) -> bool {
        match self.bb.get(key) {
            None => false,
            Some(Value::Entity(e)) => self.world.get_entity(*e).is_ok(),
            Some(Value::Bool(b)) => *b,
            Some(Value::Tiles(t)) => !t.is_empty(),
            Some(Value::Attackers(a)) => !a.is_empty(),
            Some(Value::Tile(_) | Value::Time(_) | Value::Pet(_)) => true,
        }
    }

    /// DM `prob(percent)`.
    pub fn prob(&mut self, percent: f32) -> bool {
        self.rng.chance(percent)
    }

    /// tg `SPT_PROB(percent_per_second, seconds_per_tick)`.
    pub fn spt_prob(&mut self, percent_per_second: f64) -> bool {
        let rate = 1.0 - (1.0 - percent_per_second / 100.0).powf(self.spt);
        self.rng.chance((rate * 100.0) as f32)
    }

    /// A target key's position: an entity's tile or a stored tile.
    pub fn target_pos(&self, key: Key) -> Option<TilePos> {
        match self.bb.get(key)? {
            Value::Entity(e) => self.world.get::<TilePos>(*e).copied(),
            Value::Tile(t) => Some(*t),
            _ => None,
        }
    }

    pub fn pawn_pos(&self) -> Option<TilePos> {
        self.world.get::<TilePos>(self.pawn).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AiStatus {
    On,
    /// tg `AI_STATUS_ON_LOW`.
    OnLow,
    #[default]
    Off,
}

/// tg `ai_traits`: `STOP_MOVING_WHEN_PULLED`, `RUN_WHILE_UNWATCHED` (on wherever it is).
pub const STOP_MOVING_WHEN_PULLED: u32 = 1 << 0;
pub const RUN_WHILE_UNWATCHED: u32 = 1 << 1;
/// tg `ALWAYS_HIGH_PRIORITY`, `CAN_ACT_WHILE_DEAD`.
pub const ALWAYS_HIGH_PRIORITY: u32 = 1 << 2;
pub const CAN_ACT_WHILE_DEAD: u32 = 1 << 3;

/// tg `/datum/ai_controller` on a mob.
#[derive(Component, Default)]
pub struct AiController {
    /// The tg controller type.
    pub kind: String,
    pub tree: bt::Tree,
    pub bb: Blackboard,
    pub movement: movement::AiMovement,
    pub status: AiStatus,
    /// Game time of the last planning tick (0: none since it turned on).
    pub last_tick: f64,
    pub traits: u32,
    /// tg `ai_retaliate` on the mob ([`Retaliates`]).
    pub retaliates: bool,
    /// tg `cancel_current_plan` asked from outside (a bot's reset): done before the next tick.
    pub cancel: bool,
    active: u32,
}

impl AiController {
    /// The controller of a mob's `ai_controller` type, when [`trees`] has it.
    pub fn for_mob(def: &MobDef) -> Option<Self> {
        let built = trees::build(&def.ai_controller, def)?;
        Some(Self {
            kind: def.ai_controller.clone(),
            tree: bt::Tree::new(built.roots),
            bb: Blackboard(built.blackboard.into_iter().collect()),
            movement: movement::AiMovement::default(),
            status: AiStatus::Off,
            last_tick: 0.0,
            traits: built.traits,
            retaliates: built.retaliates,
            cancel: false,
            active: 0,
        })
    }
}

/// Mobs that remember who hurt them (tg `/datum/element/ai_retaliate`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Retaliates;

/// tg `ai_retaliate` `on_attacked`: the attacker goes into `BB_BASIC_MOB_RETALIATE_LIST`.
/// Also tg `ai_flee_while_injured` (carp): `BB_BASIC_MOB_STOP_FLEEING` follows the health.
pub fn record_attacker(world: &mut World, victim: Entity, attacker: Entity) {
    crate::restaurant::on_attacked(world, victim, attacker);
    crate::mobs::pets::on_attacked(world, victim, attacker);
    crate::mobs::lavaland::on_attacked(world, victim, attacker);
    crate::mobs::illusion::on_attacked(world, victim);
    let now = world.resource::<Time>().elapsed_secs_f64();
    let health = world.get::<crate::health::Health>(victim).cloned();
    let retaliates = world.get::<Retaliates>(victim).is_some();
    let Some(mut controller) = world.get_mut::<AiController>(victim) else {
        return;
    };
    if retaliates && victim != attacker {
        let mut list = match controller.bb.get(Key::RetaliateList) {
            Some(Value::Attackers(list)) => list.clone(),
            _ => Vec::new(),
        };
        list.retain(|(e, _)| *e != attacker);
        list.push((attacker, now));
        controller
            .bb
            .set(Key::RetaliateList, Value::Attackers(list));
    }
    if let Some(health) = health {
        flee_while_injured(&mut controller, &health);
    }
}

/// tg `ai_flee_while_injured` (`stop_fleeing_at` 1, `start_fleeing_below` 0.5) on a controller
/// whose mob has the element: its tree starts with `BB_BASIC_MOB_STOP_FLEEING` set
/// ([`trees::Built::blackboard`]; carp, pigs, ponies, penguins).
pub fn flee_while_injured(controller: &mut AiController, health: &crate::health::Health) {
    let Some(Value::Bool(stop)) = controller.bb.get(Key::StopFleeing).cloned() else {
        return;
    };
    let fraction = health.health() / health.max;
    if stop && fraction <= 0.5 {
        controller.bb.set(Key::StopFleeing, Value::Bool(false));
    } else if !stop && fraction >= 1.0 {
        controller.bb.set(Key::StopFleeing, Value::Bool(true));
    }
}

#[derive(Resource, Default)]
struct AiClock {
    elapsed: f64,
}

/// Live count of the last planning pass (dev logs and tests).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct AiStats {
    pub controllers: usize,
    pub on: usize,
    /// Controllers ticked over the server's life.
    pub ticked: u64,
    /// Planning passes run and their total wall time (dev logs, the MetaStation measurement).
    pub passes: u64,
    pub pass_nanos: u64,
}

pub(super) fn build(app: &mut App) {
    app.init_resource::<AiClock>()
        .init_resource::<AiStats>()
        .add_systems(
            Update,
            // After the tick's damage and moves: steps are taken at the next `Movement`.
            (plan, movement::step_movers)
                .chain()
                .after(crate::health::HealthSystems),
        );
}

/// `SSai_controllers` `fire`: statuses, then a planning tick of every controller that is on.
// Cost: every 0.25 s, per controller one status check (a few component reads, a scan of the
// players for distance) and, when on, one tree walk (tens of nodes, a vision scan of ~19x19
// tiles at most once a second per targeting leaf). Off controllers cost the status check only.
fn plan(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    {
        let mut clock = world.resource_mut::<AiClock>();
        clock.elapsed += delta;
        if clock.elapsed < AI_TICK_SECS {
            return;
        }
        clock.elapsed %= AI_TICK_SECS;
    }
    let started = bevy::platform::time::Instant::now();
    let now = world.resource::<Time>().elapsed_secs_f64();
    // Living players by level, for the status rules.
    let players: Vec<TilePos> = world
        .query_filtered::<(&TilePos, Option<&Stat>), (With<PlayerMob>, Without<Ghost>)>()
        .iter(world)
        .filter(|(_, s)| s.is_none_or(|s| *s != Stat::Dead))
        .map(|(p, _)| *p)
        .collect();
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<AiController>>()
        .iter(world)
        .collect();
    let previous = *world.resource::<AiStats>();
    let mut stats = AiStats {
        controllers: mobs.len(),
        ticked: previous.ticked,
        passes: previous.passes + 1,
        ..default()
    };
    for mob in mobs {
        // Eaten or gone earlier in this pass.
        if world.get_entity(mob).is_err() {
            continue;
        }
        let status = expected_status(world, mob, &players);
        let Some(current) = world.get::<AiController>(mob).map(|c| c.status) else {
            continue;
        };
        if status == AiStatus::Off && current == AiStatus::Off {
            continue;
        }
        // The tree and blackboard move out of the component while it plans, so behaviors get
        // the world (no archetype move: the component stays).
        let mut controller = {
            let mut c = world.get_mut::<AiController>(mob).expect("checked above");
            AiController {
                kind: std::mem::take(&mut c.kind),
                tree: std::mem::take(&mut c.tree),
                bb: std::mem::take(&mut c.bb),
                movement: std::mem::take(&mut c.movement),
                ..*c
            }
        };
        if status != controller.status {
            controller.status = status;
            controller.last_tick = 0.0;
            if status == AiStatus::Off {
                // tg `set_ai_status(AI_STATUS_OFF)` cancels the plan.
                with_ctx(world, mob, &mut controller, now, 0.0, |tree, ctx| {
                    tree.reset(ctx)
                });
                controller.movement.stop();
            }
        }
        if std::mem::take(&mut controller.cancel) {
            with_ctx(world, mob, &mut controller, now, 0.0, |tree, ctx| {
                tree.reset(ctx)
            });
        }
        // tg pauses a controller while its pawn is in a `do_after` (a bot's task).
        // Lavaland mobs' abilities hold them still the same way (`PAUSE_DURING_DO_AFTER`).
        if controller.status != AiStatus::Off
            && !crate::bots::busy(world, mob)
            && !crate::mobs::lavaland::busy(world, mob)
        {
            stats.on += 1;
            stats.ticked += 1;
            let spt = if controller.last_tick > 0.0 {
                now - controller.last_tick
            } else {
                AI_TICK_SECS
            };
            controller.last_tick = now;
            with_ctx(world, mob, &mut controller, now, spt, |tree, ctx| {
                tree.tick(ctx)
            });
        }
        if let Some(mut c) = world.get_mut::<AiController>(mob) {
            // Traits may change while planning only through the blackboard owner (events set
            // them between passes), so the component's own are kept.
            controller.traits = c.traits;
            *c = controller;
        }
    }
    stats.pass_nanos = previous.pass_nanos + started.elapsed().as_nanos() as u64;
    world.insert_resource(stats);
}

fn with_ctx(
    world: &mut World,
    pawn: Entity,
    controller: &mut AiController,
    now: f64,
    spt: f64,
    f: impl FnOnce(&mut bt::Tree, &mut AiCtx),
) {
    let rng = CombatRng(world.resource::<CombatRng>().0);
    let AiController {
        tree,
        bb,
        movement,
        active,
        ..
    } = controller;
    let mut ctx = AiCtx {
        world,
        pawn,
        bb,
        movement,
        now,
        spt,
        rng,
        active: *active,
        cancelled: false,
    };
    f(tree, &mut ctx);
    *active = ctx.active;
    let rng = ctx.rng.0;
    world.resource_mut::<CombatRng>().0 = rng;
}

/// tg `get_expected_ai_status` (see the module docs).
fn expected_status(world: &World, mob: Entity, players: &[TilePos]) -> AiStatus {
    let e = world.entity(mob);
    let Some(&pos) = e.get::<TilePos>() else {
        return AiStatus::Off;
    };
    let traits = e.get::<AiController>().map_or(0, |c| c.traits);
    let dead = e.get::<Stat>().is_some_and(|s| *s != Stat::Conscious);
    // A closet is not a turf: tg `get_turf` still finds one, but the mob cannot act inside.
    // A bot that is off cannot run (`/datum/ai_controller/basic_controller/bot/get_able_to_run`).
    if e.contains::<PlayerMob>()
        || (dead && traits & CAN_ACT_WHILE_DEAD == 0)
        || e.contains::<crate::closets::InCloset>()
        || e.get::<crate::bots::Bot>().is_some_and(|b| !b.on())
    {
        return AiStatus::Off;
    }
    let station = world
        .get_resource::<Areas>()
        .and_then(|a| a.area_at(pos))
        .and_then(|area| world.get::<DmPower>(area))
        .is_some_and(|p| {
            ss13_mapload::fidelity::is_under(&p.path, "/area/station")
                || ss13_mapload::fidelity::is_under(&p.path, "/area/shuttle")
        });
    if station
        || players
            .iter()
            .any(|p| p.chebyshev(pos).is_some_and(|d| d <= INTERESTING_DIST))
    {
        return AiStatus::On;
    }
    if traits & RUN_WHILE_UNWATCHED != 0 {
        if traits & ALWAYS_HIGH_PRIORITY != 0 {
            return AiStatus::On;
        }
        return AiStatus::OnLow;
    }
    AiStatus::Off
}
