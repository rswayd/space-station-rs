//! The jungle mobs' AI leaves and decorators (tg `targeted_mob_ability` and `use_mob_ability`
//! with their abilities, `ability_available`, `target_is_type`, `target_legcuffed`,
//! `target_health_below_fraction`, the arachnid's surveillance `ai_interact`, the seedling's
//! `beamable_hydro` search). They run through `crate::mobs::ai::fauna` (`FaunaLeaf::Jungle`,
//! `FaunaCond::Jungle`).

use bevy::prelude::*;
use ss13_shared::TilePos;

use crate::health::{Health, Stat};
use crate::mobs::ai::behaviors::{
    DELAY_FAIL, DELAY_SUCCEED, FAIL, Perform, SUCCEED, Strategy, can_see,
};
use crate::mobs::ai::{AiCtx, Key, Value};

/// The seedling's `update_interaction_target` `time_between_perform` for trays to beam.
const BEAMABLE_SEARCH: f64 = 4.0;
/// `oview` of the tray search (the controller's default vision range).
const SEARCH_RANGE: u32 = 9;
/// The arachnid tree's `target_health_below_fraction`.
const PREY_HEALTH: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JungleCond {
    /// `target_is_type(/mob/living/carbon/human)`.
    TargetIsHuman(Key),
    /// The arachnid's prey: `target_is_type(human)` inverted, `target_legcuffed`, or
    /// `target_health_below_fraction(0.5)`.
    TargetIsPrey(Key),
    /// `ability_available(BB_ARACHNID_SLIP)`.
    ArachnidSlipReady,
    /// `ability_available(BB_SOLARBEAM_ABILITY)`.
    SeedlingBeamReady,
}

impl JungleCond {
    pub fn check(self, ctx: &mut AiCtx) -> bool {
        match self {
            Self::TargetIsHuman(key) => ctx.bb.entity(key).is_some_and(|t| {
                ctx.world.get::<crate::medical::wounds::Limbs>(t).is_some()
                    && ctx.world.get::<crate::mobs::BasicMob>(t).is_none()
            }),
            Self::TargetIsPrey(key) => ctx.bb.entity(key).is_some_and(|t| {
                let world = &*ctx.world;
                let human = world.get::<crate::medical::wounds::Limbs>(t).is_some()
                    && world.get::<crate::mobs::BasicMob>(t).is_none();
                let cuffed = world.get::<crate::legcuffs::Legcuffed>(t).is_some();
                let weak = world
                    .get::<Health>(t)
                    .is_some_and(|h| h.max > 0.0 && h.health() / h.max < PREY_HEALTH);
                !human || cuffed || weak
            }),
            Self::ArachnidSlipReady => super::slip_ready(ctx.world, ctx.pawn, ctx.now),
            Self::SeedlingBeamReady => super::beam_ready(ctx.world, ctx.pawn, ctx.now),
        }
    }
}

#[derive(Debug)]
pub enum JungleLeaf {
    /// `targeted_mob_ability(BB_LEAPER_BUBBLE)`.
    LeaperBubble { key: Key },
    /// `targeted_mob_ability(BB_LEAPER_FLOP)`.
    LeaperFlop { key: Key },
    /// `targeted_mob_ability(BB_LEAPER_VOLLEY)` (untargeted: the target only has to be valid).
    LeaperVolley { key: Key },
    /// `targeted_mob_ability(BB_LEAPER_SUMMON)`.
    LeaperSummon { key: Key },
    /// `use_mob_ability(BB_ARACHNID_SLIP)`.
    ArachnidSlip,
    /// `targeted_mob_ability(BB_ARACHNID_RESTRAIN)`: a pointed spell within `cast_range`.
    ArachnidRestrain { key: Key },
    /// `targeted_mob_ability(BB_RAPIDSEEDS_ABILITY)`.
    SeedlingSeeds { key: Key },
    /// `targeted_mob_ability(BB_SOLARBEAM_ABILITY)` on a mob or a tray.
    SeedlingBeam { key: Key },
    /// `update_interaction_target` with `beamable_hydro` (4 s).
    AcquireBeamableTray { key: Key },
    /// `targeted_mob_ability(BB_TARGETED_ACTION)` of the venus trap: the tangle.
    VenusTangle { key: Key },
    /// `ai_interact` with a surveillance target next to it: a blow (`obj_damage`).
    SmashMachine { key: Key },
}

impl JungleLeaf {
    pub fn cooldown(&self) -> f64 {
        match self {
            Self::AcquireBeamableTray { .. } => BEAMABLE_SEARCH,
            _ => 0.0,
        }
    }

    pub fn setup(&mut self, _ctx: &mut AiCtx) -> bool {
        true
    }

    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        match *self {
            Self::LeaperBubble { key } => {
                let Some(target) = valid_target(ctx, key, 9) else {
                    return FAIL;
                };
                if super::fire_bubble(ctx.world, ctx.pawn, target) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::LeaperFlop { key } => {
                let Some(target) = valid_target(ctx, key, 9) else {
                    return FAIL;
                };
                if super::start_flop(ctx.world, ctx.pawn, target) {
                    ctx.movement.stop();
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::LeaperVolley { key } => {
                if valid_target(ctx, key, 9).is_none() {
                    return FAIL;
                }
                if super::start_volley(ctx.world, ctx.pawn) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::LeaperSummon { key } => {
                if valid_target(ctx, key, 9).is_none() {
                    return FAIL;
                }
                if super::summon_toads(ctx.world, ctx.pawn) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::ArachnidSlip => {
                if super::secrete_acid(ctx.world, ctx.pawn) {
                    SUCCEED
                } else {
                    FAIL
                }
            }
            Self::ArachnidRestrain { key } => {
                let Some(target) = valid_target(ctx, key, super::RESTRAIN_RANGE) else {
                    return FAIL;
                };
                if super::throw_snare(ctx.world, ctx.pawn, target) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::SeedlingSeeds { key } => {
                let Some(target) = valid_target(ctx, key, 9) else {
                    return FAIL;
                };
                if super::start_seeds(ctx.world, ctx.pawn, target) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::SeedlingBeam { key } => {
                // A tray is not a mob: only in sight.
                let Some(target) = ctx.bb.entity(key).filter(|&t| {
                    let tray = ctx.world.get::<crate::botany::tray::Tray>(t).is_some();
                    if tray {
                        sees(ctx, t, 9)
                    } else {
                        Strategy::Basic.valid(ctx, t, 9)
                    }
                }) else {
                    return FAIL;
                };
                if super::start_beam(ctx.world, ctx.pawn, target) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::AcquireBeamableTray { key } => {
                match super::beamable_tray(ctx.world, ctx.pawn, SEARCH_RANGE) {
                    Some(tray) => {
                        ctx.bb.set(key, Value::Entity(tray));
                        DELAY_SUCCEED
                    }
                    None => {
                        ctx.bb.clear(key);
                        DELAY_FAIL
                    }
                }
            }
            Self::VenusTangle { key } => {
                let Some(target) = valid_target(ctx, key, 9) else {
                    return FAIL;
                };
                if super::tangle(ctx.world, ctx.pawn, target) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::SmashMachine { key } => {
                let Some(light) = ctx.bb.entity(key) else {
                    return FAIL;
                };
                let (Some(from), Some(&to)) = (ctx.pawn_pos(), ctx.world.get::<TilePos>(light))
                else {
                    return FAIL;
                };
                if from.chebyshev(to).is_none_or(|d| d > 1) {
                    return FAIL;
                }
                let damage = ctx
                    .world
                    .get::<crate::mobs::BasicMob>(ctx.pawn)
                    .map_or(0.0, |b| b.def.obj_damage);
                crate::power::lighting::take_damage(ctx.world, light, damage);
                SUCCEED
            }
        }
    }

    pub fn finish(&mut self, _ctx: &mut AiCtx) {}
}

fn sees(ctx: &AiCtx, target: Entity, range: u32) -> bool {
    match (ctx.pawn_pos(), ctx.world.get::<TilePos>(target)) {
        (Some(from), Some(&to)) => can_see(ctx.world, from, to, range),
        _ => false,
    }
}

/// The key's target, living and valid for the basic targeting strategy within `range`.
fn valid_target(ctx: &AiCtx, key: Key, range: u32) -> Option<Entity> {
    ctx.bb.entity(key).filter(|&t| {
        ctx.world.get_entity(t).is_ok()
            && ctx.world.get::<Stat>(t).is_some_and(|s| *s != Stat::Dead)
            && Strategy::Basic.valid(ctx, t, range)
    })
}
