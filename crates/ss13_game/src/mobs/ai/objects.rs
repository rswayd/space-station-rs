//! Object AI, after tg `/datum/ai_controller/vending_machine`
//! (`code/datums/ai/objects/vending_machines/`): when Brand Intelligence has infected every
//! station vendor they come alive, waddle up to the nearest standing person within 7 tiles,
//! warn with a telegraph box for 0.8 s and fall on them (15 squish damage), then rise again.
// shortcut: cursed and haunted items, the bane and robot customers' item behaviors
// (`_item_behaviors.dm`) are not here (robot customers live in `crate::restaurant`); risen
// vendors do not deny customers (`block_usage`), waddle or step loudly.

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_world::{Mover, SpatialIndex};

use super::behaviors::{DELAY_FAIL, DELAY_RUNNING, DELAY_SUCCEED, FAIL, Perform};
use super::bt::{Abort, Cond, Node, decorator, leaf, selector, sequence};
use super::{AiController, AiCtx, Blackboard, Key, Value};
use crate::health::{Health, Stat};

pub const VENDING: &str = "/datum/ai_controller/vending_machine/eventspawn";
/// `movement_delay`, `find_vendor_target` `vision_range` / `search_cooldown`, `vendor_crush`
/// `time_to_tilt` / `untilt_cooldown`, `vendor_rise_up` `success_tilt_cooldown`, the
/// controller's `squish_damage`.
const MOVE_DELAY: f64 = 0.4;
const VISION: u32 = 7;
const SEARCH_COOLDOWN: f64 = 2.0;
const TIME_TO_TILT: f64 = 0.8;
const UNTILT_COOLDOWN: f64 = 1.0;
const SUCCESS_TILT_COOLDOWN: f64 = 5.0;
pub const SQUISH_DAMAGE: f32 = 15.0;

const HIT_LINES: [&str; 5] = [
    "Как тебе размерчик?!",
    "Поцелуй мой блестящий металлический зад!",
    "Хочешь попробовать мои товары?",
    "В ЛЕПЕШКУ!",
    "Разве вам не нравятся эти сногсшибательные цены?!",
];
const MISS_LINES: [&str; 2] = [
    "А ну вернись!",
    "Разве тебе не хочется моей хорошо оцененной любви?",
];

#[derive(Debug)]
pub enum ObjectLeaf {
    /// `find_vendor_target`.
    FindVendorTarget,
    /// `vendor_crush`: the telegraph, then the tilt when `until` is reached.
    VendorCrush { until: Option<f64> },
    /// `vendor_rise_up`.
    VendorRiseUp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectCond {
    /// `vending_is_tilted`.
    VendingTilted,
}

impl ObjectCond {
    pub fn check(self, ctx: &mut AiCtx) -> bool {
        match self {
            Self::VendingTilted => crate::vending::tilt::is_tilted(ctx.world, ctx.pawn),
        }
    }
}

impl ObjectLeaf {
    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        match self {
            Self::FindVendorTarget => find_target(ctx),
            Self::VendorCrush { until } => crush(ctx, until),
            Self::VendorRiseUp => {
                // `visible_message("[vendor_pawn] untilts itself!")`.
                crate::mobs::defense::emote(ctx.world, ctx.pawn, "поднимается сама!");
                if ctx.bb.get(Key::VendingLastHit) == Some(&Value::Bool(true)) {
                    ctx.bb.set(
                        Key::VendingTiltCooldown,
                        Value::Time(ctx.now + SUCCESS_TILT_COOLDOWN),
                    );
                }
                crate::vending::tilt::untilt(ctx.world, ctx.pawn, None);
                DELAY_SUCCEED
            }
        }
    }

    pub fn finish(&mut self, ctx: &mut AiCtx, succeeded: bool) {
        if let Self::VendorCrush { until } = self {
            *until = None;
            if !succeeded {
                ctx.bb.clear(Key::CurrentTarget);
            }
        }
    }
}

fn find_target(ctx: &mut AiCtx) -> Perform {
    let Some(from) = ctx.pawn_pos() else {
        return FAIL;
    };
    let world = &*ctx.world;
    let found = super::behaviors::candidates(ctx, VISION)
        .into_iter()
        .find(|&e| {
            world.get::<Stat>(e) == Some(&Stat::Conscious)
                && world.get::<crate::ghost::Ghost>(e).is_none()
                && world
                    .get::<TilePos>(e)
                    .is_some_and(|&p| super::behaviors::can_see(world, from, p, VISION))
        });
    match found {
        Some(e) => {
            ctx.bb.set(Key::CurrentTarget, Value::Entity(e));
            DELAY_SUCCEED
        }
        None => {
            ctx.bb.clear(Key::CurrentTarget);
            ctx.bb.set(
                Key::VendingTiltCooldown,
                Value::Time(ctx.now + SEARCH_COOLDOWN),
            );
            DELAY_FAIL
        }
    }
}

fn crush(ctx: &mut AiCtx, until: &mut Option<f64>) -> Perform {
    if crate::vending::tilt::is_tilted(ctx.world, ctx.pawn) {
        return DELAY_SUCCEED;
    }
    let Some(target) = ctx.bb.entity(Key::CurrentTarget) else {
        return FAIL;
    };
    let (Some(from), Some(&to)) = (ctx.pawn_pos(), ctx.world.get::<TilePos>(target)) else {
        return FAIL;
    };
    match *until {
        None => {
            if !ctx
                .world
                .resource::<SpatialIndex>()
                .adjacent(from, to, |e| e == target)
            {
                return FAIL;
            }
            ctx.movement.stop();
            crate::effects::temp_visual::telegraph(
                ctx.world,
                to,
                crate::effects::temp_visual::Telegraph::VendingTilt,
            );
            *until = Some(ctx.now + TIME_TO_TILT);
            DELAY_RUNNING
        }
        Some(t) if ctx.now < t => DELAY_RUNNING,
        Some(_) => {
            // `tiltonmob`: onto whoever is still next to it (the target may have stepped away).
            let before = ctx.world.get::<Health>(target).map(Health::health);
            crate::vending::tilt::tilt(ctx.world, ctx.pawn, target, 0.0, None);
            let after = ctx.world.get::<Health>(target).map(Health::health);
            let hit = before.zip(after).is_some_and(|(b, a)| a < b);
            let lines: &[&str] = if hit { &HIT_LINES } else { &MISS_LINES };
            let i = ctx.rng.range(0, lines.len() as u32 - 1) as usize;
            crate::events::brand_intelligence::speak(ctx.world, ctx.pawn, lines[i]);
            ctx.bb.set(Key::VendingLastHit, Value::Bool(hit));
            ctx.bb.set(
                Key::VendingUntiltCooldown,
                Value::Time(ctx.now + UNTILT_COOLDOWN),
            );
            DELAY_SUCCEED
        }
    }
}

/// `vending_machine.bt.json`.
fn tree() -> Node {
    let tilted = Cond::Object(ObjectCond::VendingTilted);
    selector(vec![
        decorator(
            tilted.clone(),
            false,
            Abort::None,
            decorator(
                Cond::KeyOffCooldown(Key::VendingUntiltCooldown),
                false,
                Abort::None,
                leaf(super::behaviors::Behavior::Object(ObjectLeaf::VendorRiseUp)),
            ),
        ),
        decorator(
            tilted,
            true,
            Abort::None,
            decorator(
                Cond::KeyOffCooldown(Key::VendingTiltCooldown),
                false,
                Abort::None,
                sequence(vec![
                    leaf(super::behaviors::Behavior::Object(
                        ObjectLeaf::FindVendorTarget,
                    )),
                    leaf(super::behaviors::Behavior::MoveToTarget {
                        key: Key::CurrentTarget,
                        dist: 1,
                        finish_on_arrival: true,
                    }),
                    leaf(super::behaviors::Behavior::Object(
                        ObjectLeaf::VendorCrush { until: None },
                    )),
                ]),
            ),
        ),
    ])
}

/// `upriser.ai_controller = new /datum/ai_controller/vending_machine/eventspawn(upriser)`:
/// `tiltable = FALSE`, `squish_damage = 15`, and it moves.
pub fn possess_vendor(world: &mut World, vendor: Entity) {
    if let Some(mut v) = world.get_mut::<crate::vending::Vendor>(vendor) {
        v.tiltable = false;
        v.squish_damage = SQUISH_DAMAGE;
    }
    world.entity_mut(vendor).insert((
        Mover::new(MOVE_DELAY as f32),
        AiController {
            kind: VENDING.to_owned(),
            tree: super::bt::Tree::new(vec![tree()]),
            bb: Blackboard::default(),
            ..AiController::default()
        },
    ));
}
