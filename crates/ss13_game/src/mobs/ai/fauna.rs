//! Leaves and decorators of the farm, pet, parrot, ranged and critter trees (tg
//! `tipped_reaction`, the pet command behaviors, `find_partner`/`make_babies`,
//! `parrot_repeat_speech`, `basic_ranged_attack`, `maintain_distance`, `use_mob_ability` and the
//! turtle's `hunt_target`s). The mechanics live in `crate::mobs::{farm, pets, parrot, ranged,
//! critters}`.

use bevy::prelude::*;
use ss13_shared::{Dir, TilePos};

use super::behaviors::{DELAY_FAIL, DELAY_RUNNING, DELAY_SUCCEED, FAIL, Perform, RUNNING, SUCCEED};
use super::movement::{Target, can_step, dir_towards};
use super::{AiCtx, Key, Value};
use crate::health::Stat;
use crate::mobs::pets::{PartnerSearch, PetCommand};

/// `maintain_distance` defaults (`BB_RANGED_SKIRMISH_MIN_DISTANCE`/`MAX_DISTANCE` unset).
const SKIRMISH_MIN: u32 = 4;
const SKIRMISH_MAX: u32 = 6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FaunaCond {
    /// The active pet command is this one.
    PetCommand(PetCommand),
    /// `ability_available` of a grown turtle tree's ability.
    TurtleAbilityReady,
    /// A lavaland mob decorator (`crate::mobs::lavaland::ai`).
    Lavaland(crate::mobs::lavaland::ai::LavaCond),
    /// A jungle mob decorator (`crate::mobs::jungle::ai`).
    Jungle(crate::mobs::jungle::ai::JungleCond),
}

impl FaunaCond {
    pub fn check(self, ctx: &mut AiCtx) -> bool {
        match self {
            Self::PetCommand(command) => {
                ctx.bb.get(Key::ActivePetCommand) == Some(&Value::Pet(command))
            }
            Self::TurtleAbilityReady => {
                crate::mobs::critters::turtle_ability_ready(ctx.world, ctx.pawn, ctx.now)
            }
            Self::Lavaland(cond) => cond.check(ctx),
            Self::Jungle(cond) => cond.check(ctx),
        }
    }
}

#[derive(Debug)]
pub enum FaunaLeaf {
    /// `tipped_reaction`: pleads with someone near, then gives up.
    TippedReaction,
    /// `wait` without a duration (the `stay` command): stands where it is.
    Wait,
    /// `play_dead`: looks dead until `SPT_PROB(10)` a second.
    PlayDead,
    /// `clear_pet_command`.
    ClearPetCommand,
    /// `find_partner`.
    FindPartner,
    /// `make_babies`: `ai_interact` with the partner next to it (`breed_with_partner`).
    MakeBabies {
        key: Key,
    },
    /// `parrot_repeat_speech` (the 7.5 s `cooldown` decorator over it is its perform cooldown).
    ParrotRepeat,
    /// `basic_ranged_attack` (`time_between_perform` as `cooldown`).
    RangedAttack {
        key: Key,
        cooldown: f64,
        max_range: u32,
        avoid_friendly_fire: bool,
    },
    /// `maintain_distance`.
    MaintainDistance {
        key: Key,
    },
    /// `use_mob_ability` of the turtle's tree.
    TurtleAbility,
    /// `update_interaction_target` of the turtle's headbutt victim or flora.
    AcquireTurtleTarget {
        key: Key,
        flora: bool,
    },
    /// `set_bb_cooldown`.
    SetCooldown {
        key: Key,
        secs: f64,
    },
    /// `hunt_target/headbutt_leg` and `hunt_target/sniff_flora` (`always_reset_target`).
    Headbutt {
        key: Key,
    },
    SniffFlora {
        key: Key,
    },
    /// A lavaland mob leaf (`crate::mobs::lavaland::ai`).
    Lavaland(crate::mobs::lavaland::ai::LavaLeaf),
    /// A jungle mob leaf (`crate::mobs::jungle::ai`).
    Jungle(crate::mobs::jungle::ai::JungleLeaf),
    /// A cult pet leaf (`crate::mobs::cult_pets`).
    CultPet(crate::mobs::cult_pets::CultLeaf),
    /// `run_emote` / `perform_emote` of this emote key.
    Emote(&'static str),
    /// `face_target_or_face_initial` (mannequins).
    FaceTarget {
        key: Key,
    },
    /// The statue's `update_interaction_target` over lit fixtures (`unbroken_light`, 7 tiles).
    AcquireLight {
        key: Key,
    },
    /// The statue's `hunt_target/interact_with_target` on a light: it smashes it.
    SmashLight {
        key: Key,
    },
    /// The regal rat's `targeted_mob_ability` of its `riot`.
    RatRiot,
    /// The regal rat's `use_mob_ability/domain`: on when it is off.
    RatDomain,
    /// The bee's `update_interaction_target` over pollinatable trays (10 tiles, 10 s).
    AcquireTray {
        key: Key,
    },
    /// `pollinate_hydro` (5 s).
    Pollinate {
        key: Key,
    },
    /// `find_hive`: a homeless bee's habitable apiary into the key.
    FindHive {
        key: Key,
    },
    /// `inhabit_hive` (10 s): next to the key's apiary, it becomes the bee's home.
    InhabitHive {
        key: Key,
    },
    /// `enter_exit_hive` outside the home (1 s): the home into the key 15%/s (queen 85%/s).
    HiveFlyback {
        key: Key,
    },
    /// `ai_interact` with the home next to it: the bee goes in.
    EnterHive {
        key: Key,
    },
}

impl FaunaLeaf {
    pub fn cooldown(&self) -> f64 {
        match self {
            Self::FindPartner => 5.0,
            Self::AcquireTray { .. } => 10.0,
            Self::Pollinate { .. } => 5.0,
            Self::InhabitHive { .. } => 10.0,
            Self::HiveFlyback { .. } => 1.0,
            Self::ParrotRepeat => crate::mobs::parrot::REPEAT_COOLDOWN,
            Self::RangedAttack { cooldown, .. } => *cooldown,
            Self::Lavaland(leaf) => leaf.cooldown(),
            Self::Jungle(leaf) => leaf.cooldown(),
            Self::CultPet(leaf) => leaf.cooldown(),
            _ => 0.0,
        }
    }

    pub fn setup(&mut self, ctx: &mut AiCtx) -> bool {
        if let Self::Lavaland(leaf) = self {
            return leaf.setup(ctx);
        }
        if let Self::Jungle(leaf) = self {
            return leaf.setup(ctx);
        }
        match *self {
            Self::PlayDead => {
                if ctx.world.get::<Stat>(ctx.pawn) != Some(&Stat::Conscious) {
                    return false;
                }
                // `emote("deathgasp")`, `TRAIT_FAKEDEATH`, `look_dead`.
                let text = ctx
                    .world
                    .get::<crate::mobs::BasicMob>(ctx.pawn)
                    .map(|b| b.def.death_message.clone())
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| "перестает двигаться...".to_owned());
                crate::mobs::defense::emote(ctx.world, ctx.pawn, &text);
                crate::mobs::set_look(ctx.world, ctx.pawn, true);
                true
            }
            Self::FindPartner => {
                if crate::mobs::pets::refresh_breed_ready(ctx.world, ctx.pawn, ctx.now) {
                    ctx.bb.set(Key::BreedReady, Value::Bool(true));
                }
                crate::mobs::pets::gender_of(ctx.world, ctx.pawn)
                    != crate::mobs::pets::Gender::Female
                    && ctx
                        .world
                        .get::<crate::mobs::pets::Breeds>(ctx.pawn)
                        .is_some()
            }
            Self::MakeBabies { key } => ctx
                .bb
                .entity(key)
                .is_some_and(|t| ctx.world.get::<Stat>(t) == Some(&Stat::Conscious)),
            Self::MaintainDistance { key } => {
                let (Some(from), Some(to)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
                    return false;
                };
                ctx.movement.failed = false;
                super::behaviors::can_see(ctx.world, from, to, 10)
            }
            _ => true,
        }
    }

    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        if let Self::Lavaland(leaf) = self {
            return leaf.perform(ctx);
        }
        if let Self::Jungle(leaf) = self {
            return leaf.perform(ctx);
        }
        if let Self::CultPet(leaf) = self {
            return leaf.perform(ctx);
        }
        match *self {
            Self::TippedReaction => tipped_reaction(ctx),
            Self::Wait => {
                ctx.movement.stop();
                DELAY_RUNNING
            }
            Self::PlayDead => {
                if ctx.spt_prob(10.0) {
                    DELAY_SUCCEED
                } else {
                    DELAY_RUNNING
                }
            }
            Self::ClearPetCommand => {
                ctx.bb.clear(Key::ActivePetCommand);
                ctx.bb.clear(Key::PetTarget);
                SUCCEED
            }
            Self::FindPartner => match crate::mobs::pets::find_partner(ctx.world, ctx.pawn) {
                PartnerSearch::Found(partner) => {
                    ctx.bb.set(Key::BabiesTarget, Value::Entity(partner));
                    DELAY_SUCCEED
                }
                PartnerSearch::Crowded => {
                    ctx.bb.set(
                        Key::PartnerSearchTimeout,
                        Value::Time(ctx.now + crate::mobs::pets::PARTNER_TIMEOUT),
                    );
                    DELAY_FAIL
                }
                PartnerSearch::Nobody | PartnerSearch::Watched => DELAY_FAIL,
            },
            Self::MakeBabies { key } => {
                let Some(partner) = ctx.bb.entity(key) else {
                    return DELAY_FAIL;
                };
                if ctx.world.get::<Stat>(partner) != Some(&Stat::Conscious) {
                    return DELAY_FAIL;
                }
                crate::mobs::pets::breed_with(ctx.world, ctx.pawn, partner);
                if !crate::mobs::pets::refresh_breed_ready(ctx.world, ctx.pawn, ctx.now) {
                    ctx.bb.set(Key::BreedReady, Value::Bool(false));
                }
                DELAY_SUCCEED
            }
            Self::ParrotRepeat => crate::mobs::parrot::repeat(ctx),
            Self::RangedAttack {
                key,
                max_range,
                avoid_friendly_fire,
                ..
            } => crate::mobs::ranged::attack(ctx, key, max_range, avoid_friendly_fire),
            Self::MaintainDistance { key } => {
                maintain_distance(ctx, key, SKIRMISH_MIN, SKIRMISH_MAX)
            }
            Self::TurtleAbility => crate::mobs::critters::use_tree_ability(ctx),
            Self::AcquireTurtleTarget { key, flora } => {
                crate::mobs::critters::acquire_turtle_target(ctx, key, flora)
            }
            Self::SetCooldown { key, secs } => {
                ctx.bb.set(key, Value::Time(ctx.now + secs));
                SUCCEED
            }
            Self::Headbutt { key } | Self::SniffFlora { key } => {
                let Some(target) = ctx.bb.entity(key) else {
                    return FAIL;
                };
                let name = crate::mobs::defense::name_of(ctx.world, target);
                let text = if matches!(self, Self::Headbutt { .. }) {
                    format!("игриво бодает ноги {name}!")
                } else {
                    format!("наслаждается сладким ароматом {name}!")
                };
                crate::mobs::defense::emote(ctx.world, ctx.pawn, &text);
                SUCCEED
            }
            Self::Emote(key) => {
                crate::mobs::space_fauna::run_emote(ctx.world, ctx.pawn, key);
                SUCCEED
            }
            Self::FaceTarget { key } => {
                if let (Some(from), Some(to)) = (ctx.pawn_pos(), ctx.target_pos(key))
                    && let Some(dir) = dir_towards(from, to)
                    && let Some(mut facing) = ctx.world.get_mut::<ss13_shared::Facing>(ctx.pawn)
                {
                    facing.set_if_neq(ss13_shared::Facing(dir));
                }
                DELAY_SUCCEED
            }
            Self::AcquireLight { key } => crate::mobs::space_fauna::acquire_light(ctx, key),
            Self::SmashLight { key } => crate::mobs::space_fauna::smash_light(ctx, key),
            Self::RatRiot => crate::mobs::space_fauna::rat_riot_leaf(ctx),
            Self::RatDomain => crate::mobs::space_fauna::rat_domain_leaf(ctx),
            Self::AcquireTray { key } => crate::mobs::bees::acquire_tray(ctx, key),
            Self::Pollinate { key } => crate::mobs::bees::pollinate_leaf(ctx, key),
            Self::FindHive { key } => crate::mobs::bees::find_hive(ctx, key),
            Self::InhabitHive { key } => crate::mobs::bees::inhabit_hive(ctx, key),
            Self::HiveFlyback { key } => crate::mobs::bees::hive_flyback(ctx, key),
            Self::EnterHive { key } => crate::mobs::bees::enter_hive(ctx, key),
            // Performed above.
            Self::Lavaland(_) | Self::Jungle(_) | Self::CultPet(_) => FAIL,
        }
    }

    pub fn finish(&mut self, ctx: &mut AiCtx, succeeded: bool) {
        if let Self::Lavaland(leaf) = self {
            leaf.finish(ctx, succeeded);
            return;
        }
        if let Self::Jungle(leaf) = self {
            leaf.finish(ctx);
            return;
        }
        if let Self::CultPet(leaf) = self {
            leaf.finish(ctx);
            return;
        }
        match *self {
            Self::TippedReaction => {
                // "I'VE SAID MY PEACE..."
                ctx.bb.set(Key::TipReacting, Value::Bool(false));
                ctx.bb.clear(Key::Tipper);
            }
            Self::PlayDead => {
                if ctx.world.get::<Stat>(ctx.pawn) != Some(&Stat::Conscious) {
                    return;
                }
                crate::mobs::defense::emote(ctx.world, ctx.pawn, "чудесным образом оживает!");
                crate::mobs::set_look(ctx.world, ctx.pawn, false);
            }
            Self::MakeBabies { key } | Self::Headbutt { key } | Self::SniffFlora { key } => {
                ctx.bb.clear(key)
            }
            // `always_reset_target`, `pollinate_hydro/finish_action`.
            Self::SmashLight { key } | Self::Pollinate { key } => ctx.bb.clear(key),
            // `inhabit_hive/finish_action`: a hive that failed is forgotten.
            Self::InhabitHive { key } if !succeeded => ctx.bb.clear(key),
            Self::MaintainDistance { .. } => {
                ctx.movement.failed = false;
                ctx.movement.stop();
            }
            _ => {}
        }
    }
}

/// `tipped_reaction` `perform`: a carbon within 3 tiles in view (the tipper first).
fn tipped_reaction(ctx: &mut AiCtx) -> Perform {
    let tipper = ctx.bb.entity(Key::Tipper);
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_SUCCEED;
    };
    let mut savior = None;
    for e in super::behaviors::candidates(ctx, 3) {
        let carbon = ctx.world.get::<crate::medical::wounds::Limbs>(e).is_some();
        let seen = ctx
            .world
            .get::<TilePos>(e)
            .is_some_and(|&p| super::behaviors::can_see(ctx.world, from, p, 3));
        if !carbon || !seen {
            continue;
        }
        savior = Some(e);
        if Some(e) == tipper {
            break;
        }
    }
    let text = match savior {
        Some(savior) if ctx.prob(75.0) => {
            let how = ["умоляюще", "с мольбой", "с видом смирения"][ctx.rng.range(0, 2) as usize];
            let name = crate::mobs::defense::name_of(ctx.world, savior);
            format!("{how} смотрит на {name}.")
        }
        _ => "кажется, смирилась со своей судьбой.".to_owned(),
    };
    crate::mobs::defense::emote(ctx.world, ctx.pawn, &text);
    DELAY_SUCCEED
}

/// `maintain_distance` `perform`: within `min`-`max` tiles (4-6 by default) it succeeds; closer
/// it backs off a step, farther it closes in.
pub(crate) fn maintain_distance(ctx: &mut AiCtx, key: Key, min: u32, max: u32) -> Perform {
    if ctx.movement.failed {
        return FAIL;
    }
    let (Some(from), Some(to)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
        return FAIL;
    };
    let range = from.chebyshev(to).unwrap_or(u32::MAX);
    if (min..=max).contains(&range) {
        return SUCCEED;
    }
    if range < min {
        // `retreat`: the step away, else any free way that is not towards the target.
        let away = dir_towards(to, from).unwrap_or(Dir::North);
        let index = ctx.world.resource::<ss13_world::SpatialIndex>();
        let step = if can_step(index, ctx.pawn, from, away) {
            Some(away)
        } else {
            let mut dirs: Vec<Dir> = Dir::DMI_ORDER
                .into_iter()
                .filter(|&d| d != away && Some(d) != dir_towards(from, to))
                .collect();
            for i in (1..dirs.len()).rev() {
                let j = ctx.rng.range(0, i as u32) as usize;
                dirs.swap(i, j);
            }
            let index = ctx.world.resource::<ss13_world::SpatialIndex>();
            dirs.into_iter()
                .find(|&d| can_step(index, ctx.pawn, from, d))
        };
        let Some(dir) = step else {
            return FAIL;
        };
        ctx.movement.start(Target::Tile(from.step(dir)), 0);
        return RUNNING;
    }
    if let Some(target) = super::behaviors::target_of(ctx, key) {
        ctx.movement.start(target, max);
    }
    RUNNING
}
