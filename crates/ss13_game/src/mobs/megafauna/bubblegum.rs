//! Bubblegum, after tg `megafauna/bubblegum.dm` and its abilities (`charge.dm`,
//! `blood_warp.dm`), the king of slaughter demons:
//!
//! - it leaves blood wherever it walks (`blood_walk`, with a boom) and bleeds thick blood when
//!   hurt (25%); every blood decal is a pool it can reach through (`can_bloodcrawl_in`);
//! - `OpenFire`: hands rise out of the pools under its targets (`try_bloodattack`): a smack
//!   (10 brute after 0.4 s) or, on a downed victim (10% anyway), a grab that drags the victim
//!   through the blood in front of it and devours them 0.2 s later; with no one on blood (or
//!   25% + anger) it sinks into a pool next to it and springs out of one 5 tiles from its target
//!   (`blood_warp`, 0.3 s) and enrages: immune to projectiles, a tile per 0.375 s, for 3.5-7 s;
//! - then above half health a triple charge (0.6, 0.4, 0.2 s wind-ups, 2 tiles past the target,
//!   a tile per 0.05 s: 30 brute to whoever it slams into, walls and rock in the way blown up);
//!   below half health hallucination charges (50% + anger): six copies of it in a ring of 6
//!   tiles charging at once with it (enraged: two thirds of the time three rings of four and a
//!   triple charge), or else five rounds of two copies 2 tiles out and a charge of its own. A
//!   copy's slam deals 15 brute; it bursts into blood when its charge ends.
//!
//! It keeps 5 tiles from its target unless enraged or the target is incapacitated. Light
//! explosions do nothing to it, harder ones count as light. Its sarcophagus holds the H.E.C.K.
//! suit and a bottle of mayhem or a soulscythe; a corpse it devours leaves bubblegum.
// shortcut: no retreat (it holds its distance instead of backing off: the AI movement has no
// flee), no armor on the hands' smack, dense objects and mechs stop a charge undamaged, no
// wall smashing beside the charge path (`DestroySurroundings`), no shuttle unlock on death. A
// slam hits each victim once per charge (tg's move loop re-bumps a blocking victim every tick
// until it times out).

use std::collections::VecDeque;

use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, MouseOpacity, TilePos};
use ss13_world::{Density, SpatialIndex};

use super::{
    BELOW_MOB_LAYER, GAME_PLANE, Kind, Megafauna, Step as MegaStep, TempVisual, Visual, kind_of,
    living_on, roll, schedule, shake, show, sound, spawn_visual, stop_moving, tell,
};
use crate::health::{DamageType, Health, Stat};
use crate::mobs::defense::hurt;
use crate::zone::BodyZone;

pub const CHEST: &str = "/obj/structure/closet/crate/necropolis/bubblegum";
const HECK_SUIT: &str = "/obj/item/clothing/suit/hooded/hostile_environment";
/// `necropolis/bubblegum/PopulateContents`: one of two with the suit.
const LOOT: [&str; 2] = ["/obj/item/mayhem", "/obj/item/soulscythe"];
/// What a devoured corpse leaves (`bubblegum/devour`).
pub const GUM: &str = "/obj/item/food/bubblegum/bubblegum";
const BLOOD_DECALS: &str = "/obj/effect/decal/cleanable/blood";
const ATTACK_SOUND: &str = "sound/effects/magic/demon_attack1.ogg";
const ENTER_BLOOD: &str = "sound/effects/magic/enter_blood.ogg";
const EXIT_BLOOD: &str = "sound/effects/magic/exit_blood.ogg";
const IMPACT_SOUND: &str = "sound/effects/meteorimpact.ogg";
/// `SFX_BULLET_MISS`.
const BULLET_MISS: &str = "bullet_miss";
const HANDS_DMI: &str = "icons/effects/bubblegum.dmi";
/// `COLOR_BUBBLEGUM_RED`.
const RED: [u8; 4] = [0x95, 0x0A, 0x0A, 0xFF];

/// `charge`: `charge_speed` (a tile), `charge_distance`, `charge_damage`; the copies'
/// `hallucination_damage`.
const CHARGE_STEP: f64 = 0.05;
const CHARGE_DISTANCE: u32 = 50;
const CHARGE_DAMAGE: f32 = 30.0;
const CLONE_DAMAGE: f32 = 15.0;
/// `cooldown_time` of the triple charge and the hallucination charges.
const TRIPLE_COOLDOWN: f64 = 1.5;
const HALLUCINATION_COOLDOWN: f64 = 2.0;
/// The copies' longest life: a safety net if Bubblegum dies mid-charge.
const CLONE_LIFE: f64 = 6.0;
/// `blood_warp`: `pick_range`, the sink.
const WARP_RANGE: u32 = 5;
const WARP_SINK: f64 = 0.3;
/// `enrage_time` at full anger; the enraged `move_to_delay` (seconds a tile).
const ENRAGE_SECS: f64 = 7.0;
const ENRAGED_MOVE_DELAY: f32 = 0.375;
/// `bloodsmack` and `bloodgrab`: the strike after, the whole hand; the smack's brute.
const SMACK_AT: f64 = 0.4;
const GRAB_AT: f64 = 0.6;
const HAND_SECS: f64 = 0.7;
const SMACK_DAMAGE: f32 = 10.0;
/// `temp_visual/bubblegum_hands` `duration`.
const HAND_VISUAL: f64 = 0.9;
/// `retreat_distance`/`minimum_distance`.
const KEEP_DISTANCE: u32 = 5;

#[derive(Debug, Default)]
pub struct State {
    /// `enrage_till`.
    pub enrage_till: f64,
    /// Its movement delay before the enrage.
    saved_delay: Option<f32>,
    /// The running ability: its acts in order (the sleeps of tg's synchronous `Activate`).
    program: VecDeque<Act>,
    program_target: Option<Entity>,
    /// Charging (`charging`): the hostile AI does not walk it.
    pub charging: bool,
    /// For the blood walk and the bleeding.
    last_pos: Option<TilePos>,
    last_health: Option<f32>,
}

/// One act of an ability (`charge_sequence`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    Warp,
    /// `do_charge(owner, target, delay, past)`.
    Charge {
        delay: f64,
        past: i32,
    },
    /// `hallucination_charge(target, clone_amount, delay, past, radius, use_self)`.
    Hallucination {
        clones: u32,
        delay: f64,
        past: i32,
        radius: f64,
        use_self: bool,
    },
    /// `StartCooldown`.
    Finish {
        cooldown: f64,
    },
}

#[derive(Debug)]
pub enum Step {
    /// The next act of the running ability.
    Next,
    WarpLand {
        target: Entity,
    },
    EnrageEnd,
    /// `do_charge` of Bubblegum or a copy: the warning, then the run.
    ChargeGo {
        charger: Entity,
        dest: TilePos,
    },
    ChargeMove {
        charger: Entity,
        dest: TilePos,
        until: f64,
        hit: Vec<Entity>,
    },
    ChargeEnd {
        charger: Entity,
    },
    /// A hand under `target` (`bloodattack`); `pool` rechecks the blood first.
    Hand {
        target: Entity,
        right: bool,
        pool: bool,
    },
    Smack {
        pos: TilePos,
    },
    Grab {
        pos: TilePos,
    },
    Devour {
        victim: Entity,
    },
}

/// One of Bubblegum's hallucinations (`bubblegum/hallucination`): an image that charges.
#[derive(Component, Clone, Copy, Debug)]
pub struct Hallucination {
    pub owner: Entity,
}

fn state(world: &World, mob: Entity) -> Option<&State> {
    world.get::<Megafauna>(mob).map(|m| &m.bubblegum)
}

/// `BUBBLEGUM_IS_ENRAGED`.
pub fn enraged(world: &World, mob: Entity, now: f64) -> bool {
    state(world, mob).is_some_and(|s| s.enrage_till > now)
}

/// Charging: the hostile AI neither walks nor strikes.
pub fn charging(world: &World, mob: Entity) -> bool {
    state(world, mob).is_some_and(|s| s.charging)
}

/// `anger_modifier`: a point per 60 health lost, at most 20; and whether it is below half
/// health (`BUBBLEGUM_SMASH`).
fn anger(world: &World, mob: Entity) -> (f32, bool) {
    world.get::<Health>(mob).map_or((0.0, false), |h| {
        let hp = h.health();
        (((h.max - hp) / 60.0).clamp(0.0, 20.0), hp <= h.max * 0.5)
    })
}

/// `enrage_time`: 7 s scaled by anger, at least half.
pub fn enrage_time(anger: f32) -> f64 {
    ENRAGE_SECS * f64::from((anger / 20.0).clamp(0.5, 1.0))
}

/// `get_minimum_distance`: next to its target when aggressive (enraged, or the target
/// incapacitated), else 5 tiles.
pub(super) fn minimum_distance(world: &World, mob: Entity, target: Entity, now: f64) -> u32 {
    let incapacitated = world
        .get::<crate::status_effects::Incapacitation>(target)
        .is_some_and(|i| i.incapacitated);
    if enraged(world, mob, now) || incapacitated {
        1
    } else {
        KEEP_DISTANCE
    }
}

/// The acts of an `OpenFire` (after the hands): the warp first when `warp`, then the charges.
pub fn plan(warp: bool, smash: bool, roll_halluc: bool, roll_enraged: bool) -> Vec<Act> {
    let mut acts = Vec::new();
    if warp {
        acts.push(Act::Warp);
    }
    let charge = |i: i32| Act::Charge {
        delay: f64::from(6 - 2 * i) / 10.0,
        past: 2,
    };
    if !smash {
        acts.extend((0..3).map(charge));
        acts.push(Act::Finish {
            cooldown: TRIPLE_COOLDOWN,
        });
    } else if roll_halluc {
        // `hallucination_charge.enraged` is `BUBBLEGUM_SMASH`: here always.
        if !roll_enraged {
            acts.push(Act::Hallucination {
                clones: 6,
                delay: 0.8,
                past: 0,
                radius: 6.0,
                use_self: true,
            });
        } else {
            acts.extend((0..3).map(|i| Act::Hallucination {
                clones: 4,
                delay: f64::from(9 - 2 * i) / 10.0,
                past: 0,
                radius: 4.0,
                use_self: true,
            }));
            acts.extend((0..3).map(charge));
        }
        acts.push(Act::Finish {
            cooldown: HALLUCINATION_COOLDOWN,
        });
    } else {
        for _ in 0..5 {
            acts.push(Act::Hallucination {
                clones: 2,
                delay: 0.8,
                past: 2,
                radius: 2.0,
                use_self: false,
            });
            acts.push(Act::Charge {
                delay: 0.6,
                past: 2,
            });
        }
        acts.push(Act::Finish {
            cooldown: HALLUCINATION_COOLDOWN,
        });
    }
    acts
}

/// `/mob/living/simple_animal/hostile/megafauna/bubblegum/OpenFire`.
pub(super) fn open_fire(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let on_blood = try_bloodattack(world, mob, now);
    let Some(m) = world.get::<Megafauna>(mob) else {
        return;
    };
    if !m.ability_available(now) {
        return;
    }
    let (anger, smash) = anger(world, mob);
    let (warp_roll, halluc, unenraged) = roll(world, |rng| {
        (
            rng.chance(25.0 + anger),
            rng.chance(50.0 + anger),
            rng.chance(33.0),
        )
    });
    let acts = plan(!on_blood || warp_roll, smash, halluc, !unenraged);
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        // `disable_cooldown_actions`, `next_melee_use_time` until the charges end.
        m.busy_until = now + 100.0;
        m.melee_ready = m.melee_ready.max(now + 100.0);
        m.bubblegum.program = acts.into();
        m.bubblegum.program_target = Some(target);
        m.at(now, MegaStep::Bubblegum(Step::Next));
    }
}

pub(super) fn run(world: &mut World, mob: Entity, at: f64, step: Step) {
    match step {
        Step::Next => next_act(world, mob, at),
        Step::WarpLand { target } => warp_land(world, mob, at, target),
        Step::EnrageEnd => enrage_end(world, mob),
        Step::ChargeGo { charger, dest } => charge_go(world, mob, at, charger, dest),
        Step::ChargeMove {
            charger,
            dest,
            until,
            hit,
        } => charge_move(world, mob, at, charger, dest, until, hit),
        Step::ChargeEnd { charger } => charge_end(world, mob, at, charger),
        Step::Hand {
            target,
            right,
            pool,
        } => hand(world, mob, at, target, right, pool),
        Step::Smack { pos } => smack(world, mob, pos),
        Step::Grab { pos } => grab(world, mob, at, pos),
        Step::Devour { victim } => {
            let gutted = world.get::<super::Gutted>(victim).is_some();
            if world.get::<Health>(victim).is_some() && !gutted {
                super::devour(world, mob, victim);
            }
        }
    }
}

fn next_act(world: &mut World, mob: Entity, at: f64) {
    let Some((act, target)) = world.get_mut::<Megafauna>(mob).and_then(|mut m| {
        let target = m.bubblegum.program_target?;
        Some((m.bubblegum.program.pop_front()?, target))
    }) else {
        return;
    };
    let target_alive = world.get::<TilePos>(target).is_some();
    match act {
        Act::Finish { cooldown } => {
            if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
                m.bubblegum.program_target = None;
                m.melee_ready = at;
                m.cooldown(at, cooldown);
            }
        }
        _ if !target_alive => schedule(world, mob, at, MegaStep::Bubblegum(Step::Next)),
        Act::Warp => warp(world, mob, at, target),
        Act::Charge { delay, past } => {
            if !charge_begin(world, mob, at, mob, target, delay, past) {
                schedule(world, mob, at, MegaStep::Bubblegum(Step::Next));
            }
        }
        Act::Hallucination {
            clones,
            delay,
            past,
            radius,
            use_self,
        } => {
            hallucination_charge(
                world, mob, at, target, clones, delay, past, radius, use_self,
            );
            if !use_self || !charge_begin(world, mob, at, mob, target, delay, past) {
                schedule(world, mob, at, MegaStep::Bubblegum(Step::Next));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Blood.

/// `can_bloodcrawl_in` of what lies on the map: any blood decal.
fn is_pool(world: &World, e: Entity) -> bool {
    world.get::<crate::janitor::BloodSplat>(e).is_some()
        || world
            .get::<ss13_mapload::DmObj>(e)
            .is_some_and(|o| ss13_mapload::fidelity::is_under(&o.path, BLOOD_DECALS))
}

/// `get_bloodcrawlable_pools(center, range)`: the turfs with blood in view of `center`.
pub fn pools(world: &World, center: TilePos, range: u32) -> Vec<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let mut out: Vec<TilePos> = index
        .in_radius(center, range)
        .filter(|&e| is_pool(world, e))
        .filter_map(|e| world.get::<TilePos>(e).copied())
        .filter(|&p| {
            p == center || crate::mobs::ai::behaviors::can_see(world, center, p, range.max(1))
        })
        .collect();
    out.sort_by_key(|p| (p.y, p.x));
    out.dedup();
    out
}

/// `blood_walk` and the bleeding of `on_damage_loss_changed`: every frame, blood on each new
/// turf and a 25% chance of thick blood per hurt.
// Cost: per frame, per Bubblegum (one per round) a position and health compare.
pub(super) fn bleed(world: &mut World) {
    let mobs: Vec<(Entity, TilePos, f32)> = world
        .query::<(Entity, &Megafauna, &TilePos, &Health)>()
        .iter(world)
        .filter(|(_, m, ..)| m.kind == Kind::Bubblegum)
        .map(|(e, _, p, h)| (e, *p, h.health()))
        .collect();
    for (mob, pos, health) in mobs {
        if world.get::<Stat>(mob) == Some(&Stat::Dead) {
            continue;
        }
        let Some((last_pos, last_health)) = world.get_mut::<Megafauna>(mob).map(|mut m| {
            let s = &mut m.bubblegum;
            (s.last_pos.replace(pos), s.last_health.replace(health))
        }) else {
            continue;
        };
        if last_pos.is_some_and(|p| p != pos)
            && crate::janitor::add_splatter_floor(world, pos, false).is_some()
        {
            sound(world, pos, IMPACT_SOUND, 200.0, 0.0);
        }
        if last_health.is_some_and(|h| health < h) && roll(world, |rng| rng.chance(25.0)) {
            // `gibs/bubblegum`: 40% a step off in a cardinal direction.
            let (stepped, i) = roll(world, |rng| (rng.chance(40.0), rng.range(0, 3) as usize));
            let dir = ss13_shared::Dir::CARDINALS[i];
            let spot = if stepped { pos.step(dir) } else { pos };
            crate::janitor::add_splatter_floor(world, spot, false);
        }
    }
}

/// `get_mobs_on_blood`: the targets in its vision standing on blood.
fn mobs_on_blood(world: &World, mob: Entity) -> Vec<Entity> {
    let (Some(&pos), Some(m)) = (world.get::<TilePos>(mob), world.get::<Megafauna>(mob)) else {
        return Vec::new();
    };
    let range = if m.aggro {
        super::AGGRO_VISION_RANGE
    } else {
        super::VISION_RANGE
    };
    let mut out: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, range)
        .filter(|&e| super::valid_target(world, mob, e))
        .filter(|&e| {
            world
                .get::<TilePos>(e)
                .is_some_and(|&p| !pools(world, p, 0).is_empty())
        })
        .collect();
    out.sort_by_key(|e| e.to_bits());
    out.dedup();
    out
}

/// `try_bloodattack` -> `bloodattack`: hands under one or two targets on blood, one after the
/// other (each hand takes 0.7 s); a lone target gets both. False with no one on blood.
fn try_bloodattack(world: &mut World, mob: Entity, now: f64) -> bool {
    let mut targets = mobs_on_blood(world, mob);
    if targets.is_empty() {
        return false;
    }
    let (handedness, first, second) = roll(world, |rng| {
        let right = rng.chance(50.0);
        let one = targets.swap_remove(rng.range(0, targets.len() as u32 - 1) as usize);
        let two = (!targets.is_empty())
            .then(|| targets.swap_remove(rng.range(0, targets.len() as u32 - 1) as usize));
        (right, one, two)
    });
    let mut t = now;
    let mut hand = |world: &mut World, target: Entity, right: bool, pool: bool| {
        schedule(
            world,
            mob,
            t,
            MegaStep::Bubblegum(Step::Hand {
                target,
                right,
                pool,
            }),
        );
        t += HAND_SECS;
    };
    if let Some(second) = second {
        hand(&mut *world, second, handedness, false);
    }
    hand(&mut *world, first, !handedness, true);
    if second.is_none() {
        hand(&mut *world, first, handedness, true);
    }
    true
}

fn hand(world: &mut World, mob: Entity, at: f64, target: Entity, right: bool, pool: bool) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    if pool && pools(world, pos, 0).is_empty() {
        return;
    }
    let downed = world
        .get::<Stat>(target)
        .is_some_and(|s| *s != Stat::Conscious);
    let grab = downed || roll(world, |rng| rng.chance(10.0));
    if grab {
        spawn_visual(
            world,
            pos,
            Visual {
                layer: BELOW_MOB_LAYER,
                plane: GAME_PLANE,
                ..Visual::new(
                    HANDS_DMI,
                    if right { "rightpawgrab" } else { "leftpawgrab" },
                    HAND_VISUAL,
                )
            },
        );
        spawn_visual(
            world,
            pos,
            Visual::new(
                HANDS_DMI,
                if right {
                    "rightthumbgrab"
                } else {
                    "leftthumbgrab"
                },
                HAND_VISUAL,
            ),
        );
        schedule(
            world,
            mob,
            at + GRAB_AT,
            MegaStep::Bubblegum(Step::Grab { pos }),
        );
    } else {
        let state = if right { "rightsmack" } else { "leftsmack" };
        spawn_visual(world, pos, Visual::new(HANDS_DMI, state, HAND_VISUAL));
        schedule(
            world,
            mob,
            at + SMACK_AT,
            MegaStep::Bubblegum(Step::Smack { pos }),
        );
    }
}

/// The victims a hand reaches on `pos`: living, not of its faction.
fn hand_victims(world: &World, mob: Entity, pos: TilePos) -> Vec<Entity> {
    living_on(world, pos)
        .into_iter()
        .filter(|&v| v != mob && !crate::mobs::same_faction(world, mob, v))
        .collect()
}

/// `bloodsmack`: 10 brute on a random limb.
fn smack(world: &mut World, mob: Entity, pos: TilePos) {
    let name = crate::security::name_of(world, mob);
    for victim in hand_victims(world, mob, pos) {
        tell(world, victim, format!("{name} rends you!"));
        sound(world, pos, ATTACK_SOUND, 100.0, -1.0);
        let zone = roll(world, |rng| {
            BodyZone::ALL[rng.range(0, BodyZone::ALL.len() as u32 - 1) as usize]
        });
        hurt(world, victim, DamageType::Brute, SMACK_DAMAGE, Some(zone));
    }
}

/// `bloodgrab`: a downed victim is dragged in front of Bubblegum and devoured 0.2 s later.
fn grab(world: &mut World, mob: Entity, at: f64, pos: TilePos) {
    let Some(&here) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = crate::security::name_of(world, mob);
    let front = crate::mobs::ai::movement::dir_towards(here, pos).map_or(here, |d| here.step(d));
    for victim in hand_victims(world, mob, pos) {
        if world.get::<Stat>(victim) == Some(&Stat::Conscious) {
            continue;
        }
        tell(
            world,
            victim,
            format!("{name} drags you through the blood!"),
        );
        sound(world, pos, ENTER_BLOOD, 100.0, -1.0);
        world.entity_mut(victim).insert(front);
        sound(world, front, EXIT_BLOOD, 100.0, -1.0);
        schedule(
            world,
            mob,
            at + 0.2,
            MegaStep::Bubblegum(Step::Devour { victim }),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Blood warp and enrage.

/// The pools 5 tiles from the target (`remove_inner_pools`).
fn warp_pools(world: &World, target: TilePos) -> Vec<TilePos> {
    let inner = pools(world, target, WARP_RANGE - 1);
    pools(world, target, WARP_RANGE)
        .into_iter()
        .filter(|p| !inner.contains(p))
        .collect()
}

/// `blood_warp`: not next to the target, on blood, with a pool 5 tiles from the target: it
/// sinks for 0.3 s.
fn warp(world: &mut World, mob: Entity, at: f64, target: Entity) {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return;
    };
    let adjacent = world
        .resource::<SpatialIndex>()
        .adjacent(from, to, |e| e == target);
    if adjacent
        || from.z != to.z
        || pools(world, from, 1).is_empty()
        || warp_pools(world, to).is_empty()
    {
        schedule(world, mob, at, MegaStep::Bubblegum(Step::Next));
        return;
    }
    decoy(world, mob, from, WARP_SINK, Some(RED));
    schedule(
        world,
        mob,
        at + WARP_SINK,
        MegaStep::Bubblegum(Step::WarpLand { target }),
    );
}

fn warp_land(world: &mut World, mob: Entity, at: f64, target: Entity) {
    schedule(world, mob, at, MegaStep::Bubblegum(Step::Next));
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return;
    };
    let pools = warp_pools(world, to);
    if pools.is_empty() {
        return;
    }
    let dest = roll(world, |rng| {
        pools[rng.range(0, pools.len() as u32 - 1) as usize]
    });
    let name = crate::security::name_of(world, mob);
    show(
        world,
        from,
        Span::Danger,
        format!("{name} sinks into the blood..."),
    );
    sound(world, from, ENTER_BLOOD, 100.0, -1.0);
    world.entity_mut(mob).insert(dest);
    sound(world, dest, EXIT_BLOOD, 100.0, -1.0);
    show(
        world,
        dest,
        Span::Danger,
        "And springs back out!".to_owned(),
    );
    blood_enrage(world, mob, at);
}

/// `blood_enrage`, at most once per three enrage times: faster, red, unhittable by projectiles.
fn blood_enrage(world: &mut World, mob: Entity, now: f64) {
    let (anger, _) = anger(world, mob);
    let time = enrage_time(anger);
    let Some(till) = state(world, mob).map(|s| s.enrage_till) else {
        return;
    };
    if till + time * 2.0 > now {
        return;
    }
    let delay = world.get::<ss13_world::Mover>(mob).map(|m| m.delay);
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.bubblegum.enrage_till = now + time;
        m.bubblegum.saved_delay = m.bubblegum.saved_delay.or(delay);
        m.at(now + time, MegaStep::Bubblegum(Step::EnrageEnd));
    }
    if let Some(mut mover) = world.get_mut::<ss13_world::Mover>(mob) {
        mover.delay = ENRAGED_MOVE_DELAY;
    }
    if let Some(mut look) = world.get_mut::<Appearance>(mob) {
        look.color = Some(RED);
    }
}

fn enrage_end(world: &mut World, mob: Entity) {
    let delay = world
        .get_mut::<Megafauna>(mob)
        .and_then(|mut m| m.bubblegum.saved_delay.take());
    if let (Some(delay), Some(mut mover)) = (delay, world.get_mut::<ss13_world::Mover>(mob)) {
        mover.delay = delay;
    }
    if let Some(mut look) = world.get_mut::<Appearance>(mob) {
        look.color = None;
    }
}

/// `projectile_hit` while enraged: deflected (`BULLET_ACT_BLOCK`).
pub fn deflects(world: &mut World, mob: Entity) -> bool {
    let now = super::now(world);
    if kind_of(world, mob) != Some(Kind::Bubblegum) || !enraged(world, mob, now) {
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let name = crate::security::name_of(world, mob);
        show(
            world,
            pos,
            Span::Danger,
            format!(
                "{name} deflects the projectile! It can't be hit with ranged weapons while enraged!"
            ),
        );
        sound(world, pos, BULLET_MISS, 300.0, 0.0);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Charges.

/// `temp_visual/decoy`: a still copy of the mob's look.
fn decoy(world: &mut World, mob: Entity, pos: TilePos, secs: f64, color: Option<[u8; 4]>) {
    let Some(mut look) = world.get::<Appearance>(mob).cloned() else {
        return;
    };
    look.mouse_opacity = MouseOpacity::Transparent;
    if color.is_some() {
        look.color = color;
    }
    let until = super::now(world) + secs;
    world.spawn((
        Name::new("decoy"),
        pos,
        look,
        ss13_net::server::Networked,
        TempVisual { until },
    ));
}

/// `get_ranged_target_turf(turf, dir, past)`: `past` steps on, stopping at the map's edge.
fn past_turf(world: &World, from: TilePos, to: TilePos, past: i32) -> TilePos {
    let Some(dir) = crate::mobs::ai::movement::dir_towards(from, to) else {
        return to;
    };
    let index = world.resource::<SpatialIndex>();
    let mut out = to;
    for _ in 0..past {
        let next = out.step(dir);
        if !index.in_bounds(next) {
            break;
        }
        out = next;
    }
    out
}

/// `do_charge`: the landing mark on the target's turf, the charger flashes red; it runs after
/// `delay`. False when there is nothing to charge at.
fn charge_begin(
    world: &mut World,
    mob: Entity,
    at: f64,
    charger: Entity,
    target: Entity,
    delay: f64,
    past: i32,
) -> bool {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(charger), world.get::<TilePos>(target))
    else {
        return false;
    };
    if charger == target || from.z != to.z {
        return false;
    }
    let dest = past_turf(world, from, to, past);
    if charger == mob {
        stop_moving(world, mob);
        if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
            m.bubblegum.charging = true;
        }
    }
    spawn_visual(
        world,
        to,
        Visual {
            layer: BELOW_MOB_LAYER,
            plane: GAME_PLANE,
            offset: [-32, -32],
            color: Some([255, 0, 0, 255]),
            ..Visual::new("icons/effects/96x96.dmi", "landing", 1.0)
        },
    );
    decoy(world, charger, from, 0.3, Some([255, 0, 0, 255]));
    schedule(
        world,
        mob,
        at + delay,
        MegaStep::Bubblegum(Step::ChargeGo { charger, dest }),
    );
    true
}

fn charge_go(world: &mut World, mob: Entity, at: f64, charger: Entity, dest: TilePos) {
    let Some(&from) = world.get::<TilePos>(charger) else {
        return;
    };
    let tiles = from.chebyshev(dest).unwrap_or(0).min(CHARGE_DISTANCE);
    let time_to_hit = f64::from(tiles) * CHARGE_STEP;
    schedule(
        world,
        mob,
        at + CHARGE_STEP,
        MegaStep::Bubblegum(Step::ChargeMove {
            charger,
            dest,
            until: at + time_to_hit,
            hit: Vec::new(),
        }),
    );
    schedule(
        world,
        mob,
        at + time_to_hit + CHARGE_STEP,
        MegaStep::Bubblegum(Step::ChargeEnd { charger }),
    );
}

/// One step of the run (`home_onto`): a wall or rock in the way is blown up (`medturf`), a
/// living mob in the way is slammed.
fn charge_move(
    world: &mut World,
    mob: Entity,
    at: f64,
    charger: Entity,
    dest: TilePos,
    until: f64,
    mut hit: Vec<Entity>,
) {
    let Some(&pos) = world.get::<TilePos>(charger) else {
        return;
    };
    if at > until + 1e-9 || pos == dest {
        return;
    }
    let Some(dir) = crate::mobs::ai::movement::dir_towards(pos, dest) else {
        return;
    };
    let next = pos.step(dir);
    let index = world.resource::<SpatialIndex>();
    if !index.in_bounds(next) {
        return;
    }
    let blockers: Vec<Entity> = index.dense_on_tile(next).to_vec();
    let mut blocked = false;
    for e in blockers {
        if e == mob || e == charger || world.get::<Density>(e).is_none() {
            continue;
        }
        if world.get::<Health>(e).is_some() {
            if world.get::<Stat>(e).is_some() && !hit.contains(&e) {
                hit.push(e);
                slam(world, mob, charger, e);
            }
            blocked = true;
        } else if world.get::<ss13_mapload::Turf>(e).is_some() {
            crate::explosions::queue_turfs(world, &[next], crate::explosions::Severity::Heavy);
            blocked = true;
        } else {
            blocked = true;
        }
    }
    if !blocked {
        decoy(world, charger, pos, 0.5, None);
        world.entity_mut(charger).insert(next);
        sound(world, next, IMPACT_SOUND, 200.0, 2.0);
    }
    schedule(
        world,
        mob,
        at + CHARGE_STEP,
        MegaStep::Bubblegum(Step::ChargeMove {
            charger,
            dest,
            until,
            hit,
        }),
    );
}

/// `hit_target`: 30 brute (a copy's 15), a boom, both screens shake.
fn slam(world: &mut World, mob: Entity, charger: Entity, victim: Entity) {
    let damage = if charger == mob {
        CHARGE_DAMAGE
    } else {
        CLONE_DAMAGE
    };
    let name = crate::security::name_of(world, mob);
    let victim_name = crate::security::name_of(world, victim);
    let (Some(&at), Some(&from)) = (world.get::<TilePos>(victim), world.get::<TilePos>(charger))
    else {
        return;
    };
    show(
        world,
        at,
        Span::Danger,
        format!("{name} slams into {victim_name}!"),
    );
    tell(
        world,
        victim,
        format!("{name} tramples you into the ground!"),
    );
    hurt(world, victim, DamageType::Brute, damage, None);
    sound(world, at, IMPACT_SOUND, 100.0, 0.0);
    shake(world, at, 0, 4.0, 3.0);
    shake(world, from, 0, 2.0, 3.0);
}

/// `charge_end` -> `after_charge`: the hands try again; a copy bursts into blood.
fn charge_end(world: &mut World, mob: Entity, at: f64, charger: Entity) {
    if charger == mob {
        if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
            m.bubblegum.charging = false;
        }
        schedule(world, mob, at, MegaStep::Bubblegum(Step::Next));
    } else if world.get::<Hallucination>(charger).is_some() {
        if let Some(&pos) = world.get::<TilePos>(charger) {
            crate::janitor::add_splatter_floor(world, pos, false);
        }
        world.despawn(charger);
    }
    try_bloodattack(world, mob, at);
}

/// `hallucination_charge`: `clones` points on a ring of `radius` around the target from a
/// random angle; with `use_self` Bubblegum takes the first, copies charge from the others.
#[allow(clippy::too_many_arguments)]
fn hallucination_charge(
    world: &mut World,
    mob: Entity,
    at: f64,
    target: Entity,
    clones: u32,
    delay: f64,
    past: i32,
    radius: f64,
    use_self: bool,
) {
    let Some(&center) = world.get::<TilePos>(target) else {
        return;
    };
    let start = f64::from(roll(world, |rng| rng.range(1, 360)));
    let step = 360.0 / f64::from(clones);
    let mut self_placed = false;
    for i in 1..=clones {
        let angle = (start + step * f64::from(i)).to_radians();
        let place = TilePos::new(
            (f64::from(center.x) + angle.cos() * radius) as i32,
            (f64::from(center.y) + angle.sin() * radius) as i32,
            center.z,
        );
        if !world.resource::<SpatialIndex>().in_bounds(place) {
            continue;
        }
        if use_self && !self_placed {
            world.entity_mut(mob).insert(place);
            self_placed = true;
            continue;
        }
        let Some(mut look) = world.get::<Appearance>(mob).cloned() else {
            return;
        };
        let mut color = look.color.unwrap_or([255, 255, 255, 255]);
        color[3] = 127;
        look.color = Some(color);
        look.mouse_opacity = MouseOpacity::Transparent;
        let name = format!("{}'s hallucination", crate::security::name_of(world, mob));
        let clone = world
            .spawn((
                Name::new(name),
                place,
                look,
                ss13_net::server::Networked,
                TempVisual {
                    until: at + CLONE_LIFE,
                },
                Hallucination { owner: mob },
            ))
            .id();
        charge_begin(world, mob, at, clone, target, delay, past);
    }
}

/// `necropolis/bubblegum/PopulateContents`.
pub(super) fn loot(world: &mut World) -> Vec<&'static str> {
    let i = roll(world, |rng| rng.range(0, LOOT.len() as u32 - 1)) as usize;
    vec![HECK_SUIT, LOOT[i]]
}

/// Its death ends the enrage and drops its copies (their charges die with it).
pub(super) fn on_death(world: &mut World, mob: Entity) {
    enrage_end(world, mob);
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.bubblegum.program.clear();
        m.bubblegum.charging = false;
    }
    let clones: Vec<Entity> = world
        .query::<(Entity, &Hallucination)>()
        .iter(world)
        .filter(|(_, h)| h.owner == mob)
        .map(|(e, _)| e)
        .collect();
    for clone in clones {
        world.despawn(clone);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triple_charge_above_half_health() {
        let acts = plan(false, false, true, true);
        let delays: Vec<f64> = acts
            .iter()
            .filter_map(|a| match a {
                Act::Charge { delay, past: 2 } => Some(*delay),
                _ => None,
            })
            .collect();
        assert_eq!(delays.len(), 3);
        assert!((delays[0] - 0.6).abs() < 1e-9 && (delays[2] - 0.2).abs() < 1e-9);
        assert_eq!(
            acts.last(),
            Some(&Act::Finish {
                cooldown: TRIPLE_COOLDOWN
            })
        );
        assert_eq!(plan(true, false, false, false)[0], Act::Warp);
    }

    #[test]
    fn hallucinations_below_half_health() {
        // One ring of six with itself.
        let ring = plan(false, true, true, false);
        assert!(matches!(
            ring[0],
            Act::Hallucination {
                clones: 6,
                use_self: true,
                ..
            }
        ));
        assert_eq!(ring.len(), 2);
        // Enraged: three rings of four, then a triple charge.
        let enraged = plan(false, true, true, true);
        assert_eq!(enraged.len(), 7);
        // Surround: five rounds of two copies and a charge.
        let surround = plan(false, true, false, false);
        let copies = surround
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    Act::Hallucination {
                        clones: 2,
                        use_self: false,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(copies, 5);
        assert_eq!(surround.len(), 11);
    }

    #[test]
    fn enrage_scales_with_anger() {
        assert_eq!(enrage_time(0.0), 3.5);
        assert_eq!(enrage_time(10.0), 3.5);
        assert_eq!(enrage_time(20.0), 7.0);
    }
}
