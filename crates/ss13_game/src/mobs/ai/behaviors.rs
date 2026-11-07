//! Behavior leaves (tg `/datum/bt_node/ai_behavior/*`) and targeting strategies (tg
//! `/datum/targeting_strategy/*`) of the ported trees.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_mapload::mobs::{MobDef, MobSpeech};
use ss13_mapload::{DmItem, DmPower, HiddenUnderTile};
use ss13_shared::{Dir, TilePos};
use ss13_world::{MoveRequest, SpatialIndex};

use super::movement::{Target, can_step, dir_towards};
use super::{AiCtx, Key, Value};
use crate::health::{Health, Stat};
use crate::mobs::BasicMob;
use crate::server::PlayerMob;

/// tg `IDLE_BEHAVIOR_RATE`, `BASIC_MOB_FIND_TARGET_RATE`, seconds.
pub const IDLE_BEHAVIOR_RATE: f64 = 1.5;
pub const FIND_TARGET_RATE: f64 = 1.0;
/// `DEFAULT_ATTACK_DELAY`: the first swing waits this long.
const DEFAULT_ATTACK_DELAY: f64 = 0.4;
/// `DEFAULT_BASIC_FLEE_DISTANCE`.
const DEFAULT_FLEE_DISTANCE: u32 = 9;
const CABLE_ROOT: &str = "/obj/structure/cable";
const MOUSE_ROOT: &str = "/mob/living/basic/mouse";
const TOM: &str = "/mob/living/basic/mouse/brown/tom";
const TONGS: &str = "/obj/item/kitchen/tongs";

/// tg `/datum/targeting_strategy/*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// `/datum/targeting_strategy/basic`: living, not of a shared faction, not dead, seen.
    Basic,
    /// `basic` with `ignore_faction` (whom a fox flees: anyone who hurt it).
    IgnoreFaction,
    /// `basic/holding_object` with `BB_TARGET_HELD_ITEM` tongs (corgis).
    HoldingTongs,
    /// `huntable_mouse`: a living mouse without a mind.
    HuntableMouse,
    /// `basic/not_friends`: `basic` without the faction check, never one of the mob's friends
    /// (`BB_FRIENDS_LIST`, `crate::mobs::pets`).
    NotFriends,
    /// `basic/of_size/smaller` with `BB_ALWAYS_IGNORE_FACTION` (crabs): a smaller living mob.
    Smaller,
    /// `basic/same_faction/construct` with `BB_TARGET_WOUNDED_ONLY` (artificers): a hurt
    /// construct or shade of a shared faction.
    Construct,
}

impl Strategy {
    /// tg `is_valid_target` with `vision_range` 0 = unlimited.
    pub fn valid(self, ctx: &AiCtx, target: Entity, vision: u32) -> bool {
        let world = &*ctx.world;
        if target == ctx.pawn {
            return false;
        }
        let (Some(from), Some(&to)) = (ctx.pawn_pos(), world.get::<TilePos>(target)) else {
            return false;
        };
        if from.z != to.z || world.get::<crate::ghost::Ghost>(target).is_some() {
            return false;
        }
        if vision > 0 && from.chebyshev(to).is_none_or(|d| d > vision) {
            return false;
        }
        let Some(&stat) = world.get::<Stat>(target) else {
            return false;
        };
        if world.get::<Health>(target).is_none() {
            return false;
        }
        let ignore_faction = ctx.truthy(super::Key::IgnoreFaction);
        let ok = match self {
            Self::NotFriends => {
                stat != Stat::Dead && !crate::mobs::pets::is_friend(world, ctx.pawn, target)
            }
            Self::Smaller => {
                stat != Stat::Dead
                    && crate::mobs::pets::mob_size(world, target)
                        < crate::mobs::pets::mob_size(world, ctx.pawn)
            }
            Self::Basic => {
                // `BB_TARGET_MINIMUM_STAT` defaults to `HARD_CRIT`: not dead.
                // `TARGETING_FACTION_CHECK` with `BB_TEMPORARILY_IGNORE_FACTION`.
                (ignore_faction || !crate::mobs::same_faction(world, ctx.pawn, target))
                    && stat != Stat::Dead
            }
            Self::IgnoreFaction => stat != Stat::Dead,
            Self::Construct => {
                stat != Stat::Dead
                    && crate::mobs::same_faction(world, ctx.pawn, target)
                    && crate::mobs::constructs::is_construct_or_shade(world, target)
                    && world
                        .get::<Health>(target)
                        .is_some_and(|h| h.health() < h.max)
            }
            Self::HoldingTongs => {
                (ignore_faction || !crate::mobs::same_faction(world, ctx.pawn, target))
                    && stat != Stat::Dead
                    && world
                        .get::<crate::inventory::Inventory>(target)
                        .and_then(crate::inventory::Inventory::active_item)
                        .and_then(|i| world.get::<DmItem>(i))
                        .is_some_and(|i| ss13_mapload::fidelity::is_under(&i.path, TONGS))
            }
            Self::HuntableMouse => {
                stat != Stat::Dead
                    && world.get::<PlayerMob>(target).is_none()
                    && world
                        .get::<BasicMob>(target)
                        .is_some_and(|b| ss13_mapload::fidelity::is_under(&b.def.path, MOUSE_ROOT))
            }
        };
        // Vision 0 is unlimited: only the line of sight counts (a range of 1 here made every
        // ranged mob hold fire until its target stood next to it).
        let range = if vision == 0 { u32::MAX } else { vision };
        ok && can_see(world, from, to, range)
    }
}

/// BYOND `can_see`: within range and no opaque tile on the line between.
pub fn can_see(world: &World, from: TilePos, to: TilePos, range: u32) -> bool {
    if from.z != to.z || from.chebyshev(to).is_none_or(|d| d > range.max(1)) {
        return false;
    }
    let index = world.resource::<SpatialIndex>();
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let steps = dx.abs().max(dy.abs());
    (1..steps).all(|i| {
        let t = f64::from(i) / f64::from(steps);
        let p = TilePos::new(
            from.x + (f64::from(dx) * t).round() as i32,
            from.y + (f64::from(dy) * t).round() as i32,
            from.z,
        );
        !index
            .on_tile(p)
            .iter()
            .any(|&e| world.get::<ss13_mapload::Opaque>(e).is_some())
    })
}

/// A player's living mob within `DEFAULT_MESSAGE_RANGE` and sight (tg `no_humans_watching`).
pub fn player_in_view(ctx: &AiCtx) -> bool {
    let Some(from) = ctx.pawn_pos() else {
        return false;
    };
    let world = &*ctx.world;
    world
        .resource::<SpatialIndex>()
        .in_radius(from, 7)
        .filter(|&e| world.get::<PlayerMob>(e).is_some())
        .filter_map(|e| world.get::<TilePos>(e).copied())
        .any(|p| can_see(world, from, p, 7))
}

/// tg `perform` result flags: `AI_BEHAVIOR_DELAY`, and success / failure / still running.
pub struct Perform {
    pub delay: bool,
    pub outcome: Option<bool>,
}

pub(crate) const SUCCEED: Perform = Perform {
    delay: false,
    outcome: Some(true),
};
pub(crate) const FAIL: Perform = Perform {
    delay: false,
    outcome: Some(false),
};
pub(crate) const DELAY_SUCCEED: Perform = Perform {
    delay: true,
    outcome: Some(true),
};
pub(crate) const DELAY_FAIL: Perform = Perform {
    delay: true,
    outcome: Some(false),
};
pub(crate) const RUNNING: Perform = Perform {
    delay: false,
    outcome: None,
};
pub(crate) const DELAY_RUNNING: Perform = Perform {
    delay: true,
    outcome: None,
};

/// tg random speech datums (`speech.dm`, the dog's `BB_BASIC_MOB_SPEAK_LINES`), with
/// BandaStation's translations where it has them (`ru_emotes.toml`).
#[derive(Debug, PartialEq)]
pub struct Speech {
    pub chance: f32,
    pub emote_hear: &'static [&'static str],
    pub emote_see: &'static [&'static str],
    pub speak: &'static [&'static str],
}

pub static MOUSE_SPEECH: Speech = Speech {
    chance: 1.0,
    emote_hear: &["squeaks."],
    emote_see: &["runs in a circle.", "shakes."],
    speak: &["Squeak!", "SQUEAK!", "Squeak?"],
};
pub static CAT_SPEECH: Speech = Speech {
    chance: 10.0,
    emote_hear: &["meows."],
    emote_see: &["meows."],
    speak: &[],
};
pub static FOX_SPEECH: Speech = Speech {
    chance: 1.0,
    emote_hear: &["howls.", "barks.", "screams."],
    emote_see: &["качает своей головой.", "дрожит."],
    speak: &[
        "Ack-Ack",
        "Ack-Ack-Ack-Ackawoooo",
        "Geckers",
        "Awoo",
        "Tchoff",
    ],
};
/// `/mob/living/basic/pet/dog` `update_dog_speak_blackboard` without a hat.
pub static DOG_SPEECH: Speech = Speech {
    chance: 1.0,
    emote_hear: &["barks!", "woofs!", "yaps.", "pants."],
    emote_see: &["shakes their head.", "chases their tail.", "shivers."],
    speak: &["YAP", "Woof!", "Bark!", "AUUUUUU"],
};
/// `BB_BASIC_MOB_SPEAK_LINES` of the cockroach controller.
pub static COCKROACH_SPEECH: Speech = Speech {
    chance: 5.0,
    emote_hear: &["щёлкает жвалами."],
    emote_see: &[],
    speak: &[],
};

/// The leaves of the ported trees.
#[derive(Debug)]
pub enum Behavior {
    /// `idle_random_walk`: a random step with `walk_chance` percent per second.
    IdleRandomWalk {
        chance: f64,
    },
    /// `idle_dog`.
    IdleDog,
    /// `random_speech` (1 s between performs).
    RandomSpeech(&'static Speech),
    /// `acquire_target/update_combat_targets` (and its `nearest`): the closest valid mob within
    /// 9 tiles (tg's base takes the first hearer, in no set order), kept up to 16 tiles while no
    /// other is found.
    AcquireCombatTarget {
        key: Key,
        strategy: Strategy,
    },
    /// `update_combat_targets/most_wounded`: the valid one in view with the lowest health.
    AcquireMostWounded {
        key: Key,
        strategy: Strategy,
    },
    /// `acquire_target/update_interaction_target` of a cat's mouse (`oview_single_type/mouse`).
    AcquireMouse {
        key: Key,
        vision: u32,
    },
    /// `update_interaction_target` of a cable on the mouse's own tile (`accessible_cable`).
    AcquireCable {
        key: Key,
    },
    /// `acquire_target/target_from_retaliate_list`.
    TargetFromRetaliateList {
        key: Key,
        strategy: Strategy,
    },
    /// `basic_melee_attack` (`dog`: the dog variant, which bites only with `BB_DOG_HARASS_HARM`).
    MeleeAttack {
        key: Key,
        strategy: Strategy,
    },
    /// `move_to_target`.
    MoveToTarget {
        key: Key,
        dist: u32,
        finish_on_arrival: bool,
    },
    /// `find_flee_location` (3 steps away from the threat).
    FindFleeLocation {
        key: Key,
    },
    /// `play_with_mouse` (10 s between tries, 70% to splat it, 5% for Tom).
    PlayWithMouse {
        key: Key,
    },
    /// `hunt_target/interact_with_target` on a cable: the mouse bites it (20 s cooldown).
    BiteCable {
        key: Key,
    },
    /// `find_next_carp_migration_step`.
    NextMigrationStep,
    /// `clear_arrived_migration_target`.
    ClearArrivedMigration,
    ClearKey(Key),
    SetBool(Key, bool),
    /// `random_speech` of a speech type (`Some(path)`, from `MobsData::speech`) or
    /// `random_speech_blackboard` (`None`: the mob's `speech_lines`), with their sounds.
    Speak(Option<&'static str>),
    /// `capricious_retaliate` with `ignore_faction` (`BB_RANDOM_AGGRO_CHANCE` 0.5,
    /// `BB_RANDOM_DEAGGRO_CHANCE` 10, `BB_AGGRO_RANGE` 9: tg's defaults, no ported mob sets them).
    CapriciousRetaliate,
    /// `find_food`: `acquire_target` (`TARGET_REVALIDATE`, `pickup_item`) of the closest thing of
    /// the mob's `BB_BASIC_FOODS` in view.
    AcquireFood,
    /// `ai_interact` on `BB_TARGET_FOOD` (`move_to_and_eat`): `basic_eating`.
    EatFood,
    /// A bot leaf (`crate::bots::ai`).
    Bot(crate::bots::ai::BotLeaf),
    /// A robot customer leaf (`crate::restaurant`).
    Customer(crate::restaurant::CustomerLeaf),
    /// Farm, pet, parrot, ranged and critter leaves ([`super::fauna`]).
    Fauna(super::fauna::FaunaLeaf),
    /// Monkey leaves ([`super::monkey`]).
    Monkey(super::monkey::MonkeyLeaf),
}

impl Behavior {
    /// tg `time_between_perform`.
    pub fn cooldown(&self) -> f64 {
        match self {
            Self::RandomSpeech(_) | Self::Speak(_) | Self::CapriciousRetaliate => 1.0,
            Self::PlayWithMouse { .. } => 10.0,
            Self::Bot(leaf) => leaf.cooldown(),
            Self::Fauna(leaf) => leaf.cooldown(),
            _ => 0.0,
        }
    }

    pub fn setup(&mut self, ctx: &mut AiCtx) -> bool {
        match *self {
            Self::MoveToTarget { key, dist, .. } => {
                let Some(target) = target_of(ctx, key) else {
                    return false;
                };
                ctx.movement.failed = false;
                ctx.movement.start(target, dist);
                true
            }
            Self::MeleeAttack { key, .. } | Self::PlayWithMouse { key } => ctx.truthy(key),
            Self::Fauna(ref mut leaf) => leaf.setup(ctx),
            Self::Monkey(ref mut leaf) => leaf.setup(ctx),
            _ => true,
        }
    }

    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        match *self {
            Self::IdleRandomWalk { chance } => {
                try_random_step(ctx, chance, true);
                SUCCEED
            }
            Self::IdleDog => {
                if ctx.spt_prob(5.0) {
                    try_random_step(ctx, 100.0, false);
                } else if ctx.spt_prob(2.0) {
                    let text = if ctx.prob(50.0) {
                        "dances around."
                    } else {
                        "chases their tail!"
                    };
                    crate::mobs::defense::emote(ctx.world, ctx.pawn, text);
                }
                DELAY_RUNNING
            }
            Self::RandomSpeech(speech) => random_speech(ctx, speech),
            Self::AcquireCombatTarget { key, strategy } => {
                acquire_combat_target(ctx, key, strategy)
            }
            Self::AcquireMostWounded { key, strategy } => most_wounded(ctx, key, strategy),
            Self::AcquireMouse { key, vision } => {
                if ctx
                    .bb
                    .entity(key)
                    .is_some_and(|t| Strategy::HuntableMouse.valid(ctx, t, vision))
                {
                    return DELAY_SUCCEED;
                }
                match candidates(ctx, vision)
                    .into_iter()
                    .find(|&t| Strategy::HuntableMouse.valid(ctx, t, vision))
                {
                    Some(t) => {
                        ctx.bb.set(key, Value::Entity(t));
                        DELAY_SUCCEED
                    }
                    None => {
                        ctx.bb.clear(key);
                        DELAY_FAIL
                    }
                }
            }
            Self::AcquireCable { key } => match exposed_cable(ctx.world, ctx.pawn) {
                Some(cable) => {
                    ctx.bb.set(key, Value::Entity(cable));
                    DELAY_SUCCEED
                }
                None => {
                    ctx.bb.clear(key);
                    DELAY_FAIL
                }
            },
            Self::TargetFromRetaliateList { key, strategy } => {
                // `check_faction` FALSE (every ported tree): factions are ignored while picking.
                ctx.bb.set(Key::IgnoreFaction, Value::Bool(true));
                let result = target_from_retaliate_list(ctx, key, strategy);
                // `BB_ALWAYS_IGNORE_FACTION` is not used by the ported trees.
                ctx.bb.set(Key::IgnoreFaction, Value::Bool(false));
                result
            }
            Self::Speak(datum) => speak(ctx, datum),
            Self::CapriciousRetaliate => capricious_retaliate(ctx),
            Self::AcquireFood => acquire_food(ctx),
            Self::EatFood => eat_food(ctx),
            Self::MeleeAttack { key, strategy } => melee(ctx, key, strategy),
            Self::MoveToTarget {
                key,
                dist,
                finish_on_arrival,
            } => {
                if ctx.movement.failed {
                    return FAIL;
                }
                let Some(target) = target_of(ctx, key) else {
                    return Perform {
                        delay: false,
                        outcome: Some(false),
                    };
                };
                ctx.movement.start(target, dist);
                let arrived = ctx
                    .pawn_pos()
                    .zip(ctx.target_pos(key))
                    .and_then(|(a, b)| a.chebyshev(b))
                    .is_some_and(|d| d <= dist);
                if finish_on_arrival && arrived {
                    SUCCEED
                } else {
                    RUNNING
                }
            }
            Self::FindFleeLocation { key } => find_flee_location(ctx, key),
            Self::PlayWithMouse { key } => {
                let Some(mouse) = ctx
                    .bb
                    .entity(key)
                    .filter(|&m| ctx.world.get_entity(m).is_ok())
                else {
                    return DELAY_SUCCEED;
                };
                let tom = ctx
                    .world
                    .get::<BasicMob>(mouse)
                    .is_some_and(|b| b.def.path == TOM);
                if ctx.prob(if tom { 5.0 } else { 70.0 }) {
                    splat(ctx.world, mouse);
                    DELAY_SUCCEED
                } else {
                    DELAY_FAIL
                }
            }
            Self::BiteCable { key } => {
                let Some(cable) = ctx.bb.entity(key) else {
                    return FAIL;
                };
                let here = ctx.pawn_pos();
                if ctx.world.get::<TilePos>(cable).copied() != here {
                    return FAIL;
                }
                ctx.bb
                    .set(Key::CableHuntCooldown, Value::Time(ctx.now + 20.0));
                ctx.bb.clear(key);
                bite_cable(ctx, cable);
                SUCCEED
            }
            Self::NextMigrationStep => {
                let path = match ctx.bb.get(Key::MigrationPath) {
                    Some(Value::Tiles(path)) if !path.is_empty() => path.clone(),
                    _ => return FAIL,
                };
                ctx.bb.set(Key::MigrationTarget, Value::Tile(path[0]));
                ctx.bb
                    .set(Key::MigrationPath, Value::Tiles(path[1..].to_vec()));
                SUCCEED
            }
            Self::ClearArrivedMigration => {
                ctx.bb.clear(Key::MigrationTarget);
                SUCCEED
            }
            Self::ClearKey(key) => {
                ctx.bb.clear(key);
                SUCCEED
            }
            Self::SetBool(key, value) => {
                ctx.bb.set(key, Value::Bool(value));
                SUCCEED
            }
            Self::Bot(ref mut leaf) => leaf.perform(ctx),
            Self::Customer(ref mut leaf) => leaf.perform(ctx),
            Self::Fauna(ref mut leaf) => leaf.perform(ctx),
            Self::Monkey(ref mut leaf) => leaf.perform(ctx),
        }
    }

    pub fn finish(&mut self, ctx: &mut AiCtx, succeeded: bool) {
        if let Self::Bot(leaf) = self {
            leaf.finish(ctx, succeeded);
            return;
        }
        if let Self::Customer(leaf) = self {
            leaf.finish(ctx, succeeded);
            return;
        }
        if let Self::Fauna(leaf) = self {
            leaf.finish(ctx, succeeded);
            return;
        }
        if let Self::Monkey(leaf) = self {
            leaf.finish(ctx, succeeded);
            return;
        }
        match *self {
            Self::MoveToTarget { .. } => {
                ctx.movement.failed = false;
                ctx.movement.stop();
            }
            // tg resets `BB_TEMPORARILY_IGNORE_FACTION` only when nobody was picked.
            Self::CapriciousRetaliate if !succeeded => {
                ctx.bb.set(Key::IgnoreFaction, Value::Bool(false));
            }
            Self::PlayWithMouse { key } => {
                let target = ctx.bb.entity(key);
                ctx.bb.clear(key);
                let Some(target) = target else {
                    return;
                };
                let name = ctx
                    .world
                    .get::<Name>(target)
                    .map_or_else(String::new, |n| n.as_str().to_owned());
                let result = if succeeded {
                    "and succeeds!"
                } else {
                    "but fails!"
                };
                let text = format!("attempts to hunt {name}...{result}");
                crate::mobs::defense::emote(ctx.world, ctx.pawn, &text);
            }
            _ => {}
        }
    }
}

/// tg `acquire_target/target_from_retaliate_list` (the nearest valid attacker).
fn target_from_retaliate_list(ctx: &mut AiCtx, key: Key, strategy: Strategy) -> Perform {
    let list = match ctx.bb.get(Key::RetaliateList) {
        Some(Value::Attackers(list)) => list.clone(),
        _ => return DELAY_FAIL,
    };
    let alive: Vec<(Entity, f64)> = list
        .into_iter()
        .filter(|&(e, _)| ctx.world.get_entity(e).is_ok())
        .collect();
    ctx.bb
        .set(Key::RetaliateList, Value::Attackers(alive.clone()));
    // tg picks the closest valid one.
    let from = ctx.pawn_pos();
    let best = alive
        .iter()
        .map(|&(e, _)| e)
        .filter(|&e| strategy.valid(ctx, e, 9))
        .min_by_key(|&e| {
            let p = ctx.world.get::<TilePos>(e).copied();
            from.zip(p)
                .and_then(|(a, b)| a.chebyshev(b))
                .unwrap_or(u32::MAX)
        });
    match best {
        Some(t) => {
            ctx.bb.set(key, Value::Entity(t));
            DELAY_SUCCEED
        }
        None => {
            ctx.bb.clear(key);
            DELAY_FAIL
        }
    }
}

/// What a [`Behavior::Speak`] reads: a speech type or the mob's definition.
enum SpeechRef {
    Def(Arc<MobDef>),
    Table(Arc<MobSpeech>),
}

impl SpeechRef {
    fn get(&self) -> Option<&MobSpeech> {
        match self {
            Self::Def(def) => def.speech_lines.as_ref(),
            Self::Table(speech) => Some(speech),
        }
    }
}

/// tg `random_speech` / `random_speech_blackboard` `perform`.
fn speak(ctx: &mut AiCtx, datum: Option<&'static str>) -> Perform {
    let source = match datum {
        Some(path) => ctx
            .world
            .get_resource::<crate::mobs::MobSpeeches>()
            .and_then(|s| s.0.get(path).cloned())
            .map(SpeechRef::Table),
        None => ctx
            .world
            .get::<BasicMob>(ctx.pawn)
            .map(|b| SpeechRef::Def(b.def.clone())),
    };
    let Some(speech) = source.as_ref().and_then(SpeechRef::get) else {
        return DELAY_FAIL;
    };
    if !ctx.prob(speech.chance) {
        return DELAY_FAIL;
    }
    let (hear, see, spoken) = (
        speech.emote_hear.len(),
        speech.emote_see.len(),
        speech.speak.len(),
    );
    let total = hear + see + spoken;
    if total == 0 {
        return DELAY_FAIL;
    }
    fn pick(rng: &mut crate::combat::CombatRng, lines: &[String]) -> String {
        lines[rng.range(0, lines.len() as u32 - 1) as usize].clone()
    }
    let roll = ctx.rng.range(1, total as u32) as usize;
    let sound = (!speech.sound.is_empty()).then(|| pick(&mut ctx.rng, &speech.sound));
    let audible = if roll <= hear {
        let text = pick(&mut ctx.rng, &speech.emote_hear);
        crate::mobs::defense::emote(ctx.world, ctx.pawn, &text);
        // `pressure_affected = TRUE, ignore_walls = FALSE`.
        Some(false)
    } else if roll <= hear + see {
        let text = pick(&mut ctx.rng, &speech.emote_see);
        crate::mobs::defense::emote(ctx.world, ctx.pawn, &text);
        None
    } else {
        let line = pick(&mut ctx.rng, &speech.speak);
        say(ctx, &line);
        Some(true)
    };
    if let (Some(ignore_walls), Some(sound), Some(pos)) = (audible, sound, ctx.pawn_pos()) {
        crate::sound::playsound(
            ctx.world,
            crate::sound::PlaySound::new(pos, sound, 80.0, true).ignore_walls(ignore_walls),
        );
    }
    DELAY_SUCCEED
}

/// tg `capricious_retaliate` `perform`: now and then calms down (forgets its grudges), else
/// sometimes takes a random dislike to someone it hears.
fn capricious_retaliate(ctx: &mut AiCtx) -> Perform {
    const DEAGGRO_CHANCE: f32 = 10.0;
    const AGGRO_CHANCE: f32 = 0.5;
    const AGGRO_RANGE: u32 = 9;
    if ctx.truthy(Key::RetaliateList) && ctx.prob(DEAGGRO_CHANCE) {
        crate::mobs::defense::emote(ctx.world, ctx.pawn, "успокаивается.");
        ctx.bb.clear(Key::RetaliateList);
        ctx.bb.clear(Key::CurrentTarget);
        return DELAY_FAIL;
    }
    if !ctx.prob(AGGRO_CHANCE) {
        return DELAY_FAIL;
    }
    // `hearers(aggro_range)`: living mobs around, picked at random until one is valid.
    let mut pool = candidates(ctx, AGGRO_RANGE);
    if pool.is_empty() {
        crate::mobs::defense::emote(ctx.world, ctx.pawn, "ворчит.");
        return DELAY_FAIL;
    }
    ctx.bb.set(Key::IgnoreFaction, Value::Bool(true));
    let mut picked = None;
    while !pool.is_empty() {
        let i = ctx.rng.range(0, pool.len() as u32 - 1) as usize;
        let test = pool.swap_remove(i);
        if Strategy::Basic.valid(ctx, test, AGGRO_RANGE) {
            picked = Some(test);
            break;
        }
    }
    let Some(target) = picked else {
        crate::mobs::defense::emote(ctx.world, ctx.pawn, "ворчит.");
        return DELAY_FAIL;
    };
    let mut list = match ctx.bb.get(Key::RetaliateList) {
        Some(Value::Attackers(list)) => list.clone(),
        _ => Vec::new(),
    };
    list.retain(|(e, _)| *e != target);
    list.push((target, ctx.now));
    ctx.bb.set(Key::RetaliateList, Value::Attackers(list));
    let name = ctx
        .world
        .get::<Name>(target)
        .map_or_else(String::new, |n| n.as_str().to_owned());
    crate::mobs::defense::emote(ctx.world, ctx.pawn, &format!("сердито смотрит на {name}!"));
    DELAY_SUCCEED
}

/// `acquire_target` `vision_range`.
const FOOD_VISION: u32 = 9;

/// tg `find_food`'s `acquire_target`: keeps a still valid food, else the closest one in view.
fn acquire_food(ctx: &mut AiCtx) -> Perform {
    let Some(eating) = crate::mobs::eating::eating(ctx.world, ctx.pawn) else {
        ctx.bb.clear(Key::TargetFood);
        return DELAY_FAIL;
    };
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_FAIL;
    };
    let (world, pawn) = (&*ctx.world, ctx.pawn);
    if ctx
        .bb
        .entity(Key::TargetFood)
        .is_some_and(|t| food_in_view(world, pawn, from, t, eating))
    {
        return DELAY_SUCCEED;
    }
    // Cheapest checks first: the type, then the line of sight of the closest.
    let mut found: Vec<(u32, Entity)> = world
        .resource::<SpatialIndex>()
        .in_radius(from, FOOD_VISION)
        .filter(|&e| crate::mobs::eating::is_food(world, e, eating))
        .filter_map(|e| Some((from.chebyshev(*world.get::<TilePos>(e)?)?, e)))
        .collect();
    found.sort_by_key(|&(d, e)| (d, e.to_bits()));
    let best = found
        .into_iter()
        .map(|(_, e)| e)
        .find(|&e| food_in_view(world, pawn, from, e, eating));
    match best {
        Some(food) => {
            ctx.bb.set(Key::TargetFood, Value::Entity(food));
            DELAY_SUCCEED
        }
        None => {
            ctx.bb.clear(Key::TargetFood);
            DELAY_FAIL
        }
    }
}

/// `pickup_item` on a food the mob sees.
fn food_in_view(
    world: &World,
    eater: Entity,
    from: TilePos,
    food: Entity,
    eating: &crate::mobs::eating::Eating,
) -> bool {
    crate::mobs::eating::edible_now(world, eater, food, eating)
        && world
            .get::<TilePos>(food)
            .is_some_and(|&p| can_see(world, from, p, FOOD_VISION))
}

/// tg `ai_interact` on the food next to the mob -> `basic_eating/on_unarm_attack`.
fn eat_food(ctx: &mut AiCtx) -> Perform {
    let Some(food) = ctx
        .bb
        .entity(Key::TargetFood)
        .filter(|&f| ctx.world.get_entity(f).is_ok())
    else {
        return FAIL;
    };
    let reachable = ctx
        .pawn_pos()
        .zip(ctx.world.get::<TilePos>(food).copied())
        .is_some_and(|(a, b)| {
            a == b
                || ctx
                    .world
                    .resource::<SpatialIndex>()
                    .adjacent(a, b, |e| e == food)
        });
    let edible = crate::mobs::eating::eating(ctx.world, ctx.pawn)
        .is_some_and(|e| crate::mobs::eating::edible_now(ctx.world, ctx.pawn, food, e));
    if !reachable || !edible {
        ctx.bb.clear(Key::TargetFood);
        return FAIL;
    }
    match crate::mobs::eating::eat(ctx.world, ctx.pawn, food) {
        Some(next) => {
            ctx.bb.clear(Key::TargetFood);
            ctx.bb.set(Key::NextFoodEat, Value::Time(next));
            SUCCEED
        }
        None => FAIL,
    }
}

pub(crate) fn target_of(ctx: &AiCtx, key: Key) -> Option<Target> {
    match ctx.bb.get(key)? {
        Value::Entity(e) if ctx.world.get_entity(*e).is_ok() => Some(Target::Entity(*e)),
        Value::Tile(t) => Some(Target::Tile(*t)),
        _ => None,
    }
}

/// tg `can_cross_safely`: space only for space walkers (carp).
fn safe_to_cross(world: &World, mob: Entity, pos: TilePos) -> bool {
    if world
        .get::<BasicMob>(mob)
        .is_some_and(|b| ss13_mapload::fidelity::is_under(&b.def.path, super::trees::CARP))
    {
        return true;
    }
    !is_space(world, pos)
}

pub fn is_space(world: &World, pos: TilePos) -> bool {
    crate::construction::is_space(world, pos)
}

/// `idle_random_walk/try_random_step` (`idle_dog` skips the safety check, as tg does).
fn try_random_step(ctx: &mut AiCtx, chance: f64, safe: bool) {
    let pulled = ctx.world.get::<crate::pull::PulledBy>(ctx.pawn).is_some();
    if pulled
        || ctx.world.get::<Stat>(ctx.pawn) != Some(&Stat::Conscious)
        || crate::mobs::farm::immobilized(ctx.world, ctx.pawn)
    {
        return;
    }
    if chance < 100.0 && !ctx.spt_prob(chance) {
        return;
    }
    let dir = Dir::DMI_ORDER[ctx.rng.range(0, 7) as usize];
    let Some(from) = ctx.pawn_pos() else {
        return;
    };
    if safe && !safe_to_cross(ctx.world, ctx.pawn, from.step(dir)) {
        return;
    }
    ctx.world.write_message(MoveRequest {
        entity: ctx.pawn,
        dir,
    });
}

fn random_speech(ctx: &mut AiCtx, speech: &Speech) -> Perform {
    if !ctx.prob(speech.chance) {
        return DELAY_FAIL;
    }
    let (hear, see, spoken) = (
        speech.emote_hear.len(),
        speech.emote_see.len(),
        speech.speak.len(),
    );
    let total = hear + see + spoken;
    if total == 0 {
        return DELAY_FAIL;
    }
    let roll = ctx.rng.range(1, total as u32) as usize;
    if roll <= hear + see {
        let text = if roll <= hear {
            speech.emote_hear[roll - 1]
        } else {
            speech.emote_see[roll - hear - 1]
        };
        crate::mobs::defense::emote(ctx.world, ctx.pawn, text);
    } else {
        let line = speech.speak[roll - hear - see - 1];
        say(ctx, line);
    }
    DELAY_SUCCEED
}

/// tg `say` of a basic mob: its `speak_emote` verb (`get_default_say_verb`).
pub(crate) fn say(ctx: &mut AiCtx, text: &str) {
    let Some(&pos) = ctx.world.get::<TilePos>(ctx.pawn) else {
        return;
    };
    let name = ctx
        .world
        .get::<Name>(ctx.pawn)
        .map_or_else(String::new, |n| n.as_str().to_owned());
    let verbs = ctx
        .world
        .get::<BasicMob>(ctx.pawn)
        .map(|b| b.def.speak_emote.clone())
        .unwrap_or_default();
    let message = if verbs.is_empty() {
        ss13_chat::server::say_message(&name, text)
    } else {
        let verb = &verbs[ctx.rng.range(0, verbs.len() as u32 - 1) as usize];
        ss13_shared::chat::ChatMessage {
            channel: ss13_shared::chat::ChatChannel::Local,
            segments: vec![
                ss13_shared::chat::ChatSegment::new(ss13_shared::chat::Span::Name, name),
                ss13_shared::chat::ChatSegment::new(
                    ss13_shared::chat::Span::Say,
                    format!(" {verb}, \"{text}\""),
                ),
            ],
        }
    };
    let clients = crate::obj_damage::clients_in_range(
        ctx.world,
        pos,
        crate::health::DEFAULT_MESSAGE_RANGE,
        &[],
    );
    ctx.world
        .write_message(ss13_net::chat::server::SendChat { clients, message });
}

/// Living mobs within `range` of the pawn, nearest first (tg `hearers`/`oview` sources).
pub(crate) fn candidates(ctx: &AiCtx, range: u32) -> Vec<Entity> {
    let Some(from) = ctx.pawn_pos() else {
        return Vec::new();
    };
    let world = &*ctx.world;
    let mut found: Vec<(u32, Entity)> = world
        .resource::<SpatialIndex>()
        .in_radius(from, range)
        .filter(|&e| e != ctx.pawn && world.get::<Health>(e).is_some())
        .filter_map(|e| {
            let d = from.chebyshev(*world.get::<TilePos>(e)?)?;
            Some((d, e))
        })
        .collect();
    found.sort_by_key(|&(d, e)| (d, e.to_bits()));
    found.into_iter().map(|(_, e)| e).collect()
}

/// tg `update_combat_targets` (`vision_range` 9, `target_loss_distance` 16,
/// `TARGET_RESELECT_WITH_SELECTION` without a priority strategy: a valid target is kept).
fn acquire_combat_target(ctx: &mut AiCtx, key: Key, strategy: Strategy) -> Perform {
    const VISION: u32 = 9;
    const LOSS: u32 = 16;
    let current = ctx.bb.entity(key);
    if current.is_some_and(|t| strategy.valid(ctx, t, VISION)) {
        return DELAY_SUCCEED;
    }
    let found = candidates(ctx, VISION);
    match found.into_iter().find(|&t| strategy.valid(ctx, t, VISION)) {
        Some(t) => {
            ctx.bb.set(key, Value::Entity(t));
            DELAY_SUCCEED
        }
        None => {
            if let Some(t) = current
                && strategy.valid(ctx, t, LOSS)
            {
                return DELAY_SUCCEED;
            }
            ctx.bb.clear(key);
            DELAY_FAIL
        }
    }
}

/// `most_wounded` `select_target` over the valid mobs in `vision_range` 9 (the nearest of
/// equals).
fn most_wounded(ctx: &mut AiCtx, key: Key, strategy: Strategy) -> Perform {
    const VISION: u32 = 9;
    let found = candidates(ctx, VISION)
        .into_iter()
        .filter(|&t| strategy.valid(ctx, t, VISION))
        .filter_map(|t| Some((ctx.world.get::<Health>(t)?.health(), t)))
        .fold(None::<(f32, Entity)>, |best, (h, t)| match best {
            Some((b, _)) if b <= h => best,
            _ => Some((h, t)),
        });
    match found {
        Some((_, t)) => {
            ctx.bb.set(key, Value::Entity(t));
            DELAY_SUCCEED
        }
        None => {
            ctx.bb.clear(key);
            DELAY_FAIL
        }
    }
}

/// tg `basic_melee_attack` `perform`.
pub(crate) fn melee(ctx: &mut AiCtx, key: Key, strategy: Strategy) -> Perform {
    let Some(target) = ctx
        .bb
        .entity(key)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return FAIL;
    };
    let reachable = ctx
        .pawn_pos()
        .zip(ctx.world.get::<TilePos>(target).copied())
        .is_some_and(|(a, b)| {
            ctx.world
                .resource::<SpatialIndex>()
                .adjacent(a, b, |e| e == target)
        });
    if !reachable {
        ctx.bb.clear(Key::MeleeCooldown);
        return FAIL;
    }
    match ctx.bb.time(Key::MeleeCooldown) {
        None => {
            ctx.bb.set(
                Key::MeleeCooldown,
                Value::Time(ctx.now + DEFAULT_ATTACK_DELAY),
            );
            return FAIL;
        }
        Some(t) if t > ctx.now => return FAIL,
        Some(_) => {}
    }
    if ctx
        .world
        .get::<crate::combat::NextMove>(ctx.pawn)
        .is_some_and(|n| ctx.now < n.0)
    {
        return FAIL;
    }
    if !strategy.valid(ctx, target, 0) {
        ctx.bb.clear(key);
        return FAIL;
    }
    crate::mobs::defense::melee_attack(ctx.world, ctx.pawn, target);
    DELAY_SUCCEED
}

/// tg `find_flee_location`: up to 3 steps, each away from the threat or any other free way.
fn find_flee_location(ctx: &mut AiCtx, key: Key) -> Perform {
    // `BB_BASIC_MOB_FLEE_DISTANCE`: 3 for mice, else `DEFAULT_BASIC_FLEE_DISTANCE`.
    let mouse = ctx
        .world
        .get::<BasicMob>(ctx.pawn)
        .is_some_and(|b| ss13_mapload::fidelity::is_under(&b.def.path, MOUSE_ROOT));
    let run = if mouse { 3 } else { DEFAULT_FLEE_DISTANCE };
    let (Some(from), Some(threat)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
        return FAIL;
    };
    if !can_see(ctx.world, from, threat, run) {
        return FAIL;
    }
    let mut at = from;
    for _ in 0..3 {
        let away = dir_towards(threat, at).unwrap_or(Dir::North);
        let index = ctx.world.resource::<SpatialIndex>();
        let next = if can_step(index, ctx.pawn, at, away) {
            Some(at.step(away))
        } else {
            let mut dirs: Vec<Dir> = Dir::DMI_ORDER
                .into_iter()
                .filter(|&d| d != away && Some(d) != dir_towards(at, threat))
                .collect();
            for i in (1..dirs.len()).rev() {
                let j = ctx.rng.range(0, i as u32) as usize;
                dirs.swap(i, j);
            }
            let index = ctx.world.resource::<SpatialIndex>();
            dirs.into_iter()
                .find(|&d| can_step(index, ctx.pawn, at, d))
                .map(|d| at.step(d))
        };
        match next {
            Some(n) => at = n,
            None => break,
        }
    }
    if at == from {
        return FAIL;
    }
    ctx.bb.set(Key::FleeLocation, Value::Tile(at));
    SUCCEED
}

/// A cable on the mob's tile not covered by a floor tile (tg `accessible_cable`).
pub fn exposed_cable(world: &World, mob: Entity) -> Option<Entity> {
    let pos = *world.get::<TilePos>(mob)?;
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| {
            world.get::<HiddenUnderTile>(e).is_none()
                && world
                    .get::<DmPower>(e)
                    .is_some_and(|p| ss13_mapload::fidelity::is_under(&p.path, CABLE_ROOT))
        })
}

/// tg mouse `splat`: `icon_dead` becomes the bloody one, and it dies.
pub fn splat(world: &mut World, mouse: Entity) {
    if let Some(mut basic) = world.get_mut::<BasicMob>(mouse) {
        basic.icon_dead = format!("{}_splat", basic.icon_living);
    }
    let max = world.get::<Health>(mouse).map_or(0.0, |h| h.max);
    if let Some(mut health) = world.get_mut::<Health>(mouse) {
        health.adjust(crate::health::DamageType::Brute, max);
    }
    let dead = world
        .get::<Health>(mouse)
        .is_some_and(|h| h.health() <= 0.0);
    if dead && let Some(mut stat) = world.get_mut::<Stat>(mouse) {
        stat.set_if_neq(Stat::Dead);
    }
}

/// tg mouse `try_bite_cable`: a powered cable shocks it to death `cable_zap_prob` of the time;
/// the cable is cut either way (`deconstruct`: one length of cable coil).
fn bite_cable(ctx: &mut AiCtx, cable: Entity) {
    let world = &mut *ctx.world;
    let Some(&pos) = world.get::<TilePos>(cable) else {
        return;
    };
    let zap = world
        .get::<BasicMob>(ctx.pawn)
        .map_or(0.0, |b| b.def.cable_zap_prob);
    let powered = world
        .get_resource::<crate::power::grid::Grid>()
        .is_some_and(|g| {
            g.cable_net(cable)
                .and_then(|n| g.net(n))
                .is_some_and(|n| n.avail > 0.0)
        });
    let name = world
        .get::<Name>(ctx.pawn)
        .map_or_else(String::new, |n| n.as_str().to_owned());
    let mut name_cap = name.chars();
    let name_cap: String = name_cap
        .next()
        .map(|c| c.to_uppercase().chain(name_cap).collect())
        .unwrap_or_default();
    let shocked = powered && ctx.rng.chance(zap);
    let text = if shocked {
        format!("{name_cap} прогрызает провод и поджаривается!")
    } else {
        format!("{name_cap} прогрызает провод.")
    };
    let clients =
        crate::obj_damage::clients_in_range(world, pos, crate::health::DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(ss13_net::chat::server::SendChat {
        clients,
        message: ss13_shared::chat::ChatMessage::simple(
            ss13_shared::chat::ChatChannel::Info,
            ss13_shared::chat::Span::Warning,
            text,
        ),
    });
    if let Some(mut grid) = world.get_resource_mut::<crate::power::grid::Grid>() {
        grid.remove_cable(cable);
    }
    world.despawn(cable);
    crate::construction::spawn_stack(world, ss13_mapload::circuit::CABLE_COIL, pos, 1);
    if shocked {
        // `electrocute_act(maxHealth * 2)`: burn.
        let max = world.get::<Health>(ctx.pawn).map_or(0.0, |h| h.max);
        if let Some(mut health) = world.get_mut::<Health>(ctx.pawn) {
            health.adjust(crate::health::DamageType::Burn, max * 2.0);
        }
        if let Some(mut stat) = world.get_mut::<Stat>(ctx.pawn) {
            stat.set_if_neq(Stat::Dead);
        }
    }
}
