//! The hierophant, after tg `megafauna/hierophant.dm`: a club that never strikes in melee but
//! fills its square arena with vortex blasts.
//!
//! - Vortex blasts (`blast/damaging`): a blast appears on a tile and goes off 0.6 s later, 10
//!   burn (30 for a blink's) to every living thing there but its caster; megafauna and lavaland
//!   elites take as much brute again (`monster_damage_boost`). A blast on a rock wall drills it.
//! - Anger: a point per 42 health lost (at most 50); the burst grows by `anger * 0.08` tiles, the
//!   cross blasts by `anger * 0.12`, the shared cooldown shrinks by 0.075 s a point.
//! - `OpenFire` (every 4 s at most): an arena (below), then at a dead target more than 2 tiles
//!   away a blink; with `anger * 0.75`% a major attack (several cross blasts, blink spam, a
//!   swarm of four chasers); else four chasers' worth of one (and a faster second one when angry
//!   or next to the target) when its 10.1 s chaser cooldown is over, a blink, a cross blast
//!   (cardinal, diagonal, below half health sometimes all eight ways) or a burst around itself.
//! - Blink: telegraphs at both ends, 0.2 s later 3x3 blasts of 30 at both, it fades out (not
//!   dense), arrives 0.5 s in and is back 0.8 s in; it neither walks nor strikes meanwhile.
//! - Chasers seek their target turf, four tiles a leg, leaving a blast on every tile, for 9.8 s.
//! - The arena (every 20 s, not inside its own ruin): a square ring of temporary vortex walls
//!   11 tiles around the target (only the hierophant passes) with blasts on it; a target 11 or
//!   more tiles away is blinked to.
//! - In melee (it has no strike of its own): a burst around itself (fast when on cooldown), then
//!   `OpenFire`; a dead target is devoured.
//! - It speaks ROT4 ("Xevkix psgexih." on a new target). Left without a target for 30 s, it
//!   blinks home to its beacon and heals half its missing health (at least 250).
//! - Death: a burst of range 10 from where it was (hitting everyone), its club, the beacon gone.
// shortcut: no hostile mobs struck by a blast turning on the hierophant, no mech damage, no
// armor against blasts (random limb, 50 penetration), no blast catching mobs that step in during
// its 0.13 s burst, no arena squares or step squares visuals, no alpha fade or purple tint
// animation (the tint is instant), no projectiles stopped by the arena walls, no wall smoothing,
// no player control, no boss music, no crusher trophy; arena traps are checked on its AI ticks
// rather than on each of its steps.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::{Density, EnterPolicy, SpatialIndex};

use super::{
    BELOW_MOB_LAYER, GAME_PLANE, Megafauna, Step as MegaStep, Visual, living_on, roll, schedule,
    show, sound, spawn_visual,
};
use crate::health::{DamageType, Health, Stat};

pub const HIEROPHANT: &str = "/mob/living/simple_animal/hostile/megafauna/hierophant";
pub const BEACON: &str = "/obj/effect/hierophant";
pub const CLUB: &str = "/obj/item/hierophant_club";
const ARENA_AREA: &str = "/area/ruin/unpowered/hierophant";
const ARENA_WALL_TURF: &str = "/turf/closed/indestructible/riveted/hierophant";
const ELITE_ROOT: &str = "/mob/living/simple_animal/hostile/asteroid";
const EFFECTS: &str = "icons/effects/effects.dmi";
const BIG: &str = "icons/effects/96x96.dmi";
const WALL_DMI: &str = "icons/turf/walls/hierophant_wall_temp.dmi";
const BLIND: &str = "sound/effects/magic/blind.ogg";
const SEAR: &str = "sound/items/weapons/sear.ogg";
const BIN_CLOSE: &str = "sound/effects/bin/bin_close.ogg";
const AIRLOCK_OPEN: &str = "sound/machines/airlock/airlockopen.ogg";
const WAND_TELEPORT: &str = "sound/effects/magic/wand_teleport.ogg";
/// `ranged_cooldown_time`, `major_attack_cooldown`, `chaser_cooldown_time`,
/// `arena_cooldown_time`, seconds.
const RANGED_COOLDOWN: f64 = 4.0;
const MAJOR_COOLDOWN: f64 = 6.0;
const CHASER_COOLDOWN: f64 = 10.1;
const ARENA_COOLDOWN: f64 = 20.0;
/// `initial(burst_range)`, `initial(beam_range)`.
const BURST_RANGE: u32 = 3;
const BEAM_RANGE: u32 = 5;
/// `blast/damaging` `blast()`: the blast goes off this long after it appears.
const FUSE: f64 = 0.6;
/// `blast` `damage`, a blink's blasts.
const BLAST_DAMAGE: f32 = 10.0;
const BLINK_DAMAGE: f32 = 30.0;
/// The chaser's `duration` and `standard_moving_before_recalc`.
const CHASER_LIFE: f64 = 9.8;
const CHASER_LEG: u32 = 4;
/// `send_me_home` timer.
const HOME_DELAY: f64 = 30.0;
/// The arena's radius.
const ARENA_RADIUS: i32 = 11;
/// The tint of its specials (`#660099`).
const PURPLE: [u8; 4] = [0x66, 0x00, 0x99, 0xFF];
const TARGET_PHRASES: [&str; 3] = ["Xevkix psgexih.", "Iriqc jsyrh.", "Eguymvih xevkix."];
const KILL_PHRASES: [&str; 4] = [
    "Wsyvgi sj irivkc xettih. Vitemvmrk...",
    "Irivkc wsyvgi jsyrh. Vitemvmrk...",
    "Jyip jsyrh. Egxmzexmrk vitemv gcgpiw...",
    "Kix fiex. Liepmrk...",
];

#[derive(Debug, Default)]
pub struct State {
    /// A blink is under way.
    pub blink: bool,
    /// A major attack is under way (`blinking` through its sleeps).
    pub special: bool,
    pub chaser_ready: f64,
    pub arena_ready: f64,
    /// `anger_modifier`.
    pub anger: f32,
    /// Where its beacon is (`spawned_beacon_ref`).
    pub home: Option<TilePos>,
    pub beacon: Option<Entity>,
    /// `sitting_at_center`.
    pub at_center: bool,
    /// When it goes home (`respawn_timer_id`).
    pub home_at: Option<f64>,
}

/// The directions of a cross blast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cross {
    Cardinal,
    Diagonal,
    All,
}

impl Cross {
    fn dirs(self) -> &'static [Dir] {
        const DIAGONALS: [Dir; 4] = [
            Dir::NorthEast,
            Dir::NorthWest,
            Dir::SouthEast,
            Dir::SouthWest,
        ];
        const ALL: [Dir; 8] = [
            Dir::North,
            Dir::South,
            Dir::East,
            Dir::West,
            Dir::NorthEast,
            Dir::NorthWest,
            Dir::SouthEast,
            Dir::SouthWest,
        ];
        match self {
            Self::Cardinal => &Dir::CARDINALS,
            Self::Diagonal => &DIAGONALS,
            Self::All => &ALL,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Step {
    /// 0.2 s into a blink: the 3x3 blasts at both ends.
    BlinkBlasts {
        from: TilePos,
        to: TilePos,
    },
    FadeOut,
    Arrive(TilePos),
    FadeIn,
    BlinkDone,
    /// A cross blast goes off on `at`.
    CrossFire {
        at: TilePos,
        cross: Cross,
    },
    /// The next round of a major attack; `pace` is the target's slowness, deciseconds.
    CrossSpam {
        target: Entity,
        left: u32,
        pace: f32,
    },
    BlinkSpam {
        target: Entity,
        left: u32,
        pace: f32,
    },
    Swarm {
        target: Entity,
        targets: Vec<Entity>,
        dirs: Vec<Dir>,
        pace: f32,
    },
    EndSpecial,
}

/// A vortex blast: who made it (it never hurts its caster) and its damage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub caster: Option<Entity>,
    pub damage: f32,
}

/// Blasts waiting to appear, then to go off; and the chasers.
#[derive(Resource, Default)]
pub struct Vortex {
    queued: Vec<(f64, TilePos, Blast)>,
    armed: Vec<(f64, TilePos, Blast)>,
    chasers: Vec<Chaser>,
}

#[derive(Clone, Copy, Debug)]
struct Chaser {
    caster: Entity,
    target: Entity,
    pos: TilePos,
    dir: Option<Dir>,
    prev: Option<Dir>,
    prev2: Option<Dir>,
    moving: u32,
    /// Seconds between steps.
    speed: f64,
    next_at: f64,
    until: f64,
}

/// A temporary arena wall: only its caster walks through.
#[derive(Component, Clone, Copy, Debug)]
pub struct VortexWall {
    pub caster: Entity,
}

pub(super) fn build(app: &mut App) {
    app.init_resource::<Vortex>();
}

fn now(world: &World) -> f64 {
    crate::tools::now(world)
}

fn state(world: &World, mob: Entity) -> Option<&State> {
    world.get::<Megafauna>(mob).map(|m| &m.hierophant)
}

fn state_mut(world: &mut World, mob: Entity) -> Option<Mut<'_, Megafauna>> {
    world.get_mut::<Megafauna>(mob)
}

/// It neither walks nor strikes (`blinking`).
pub fn blinking(world: &World, mob: Entity) -> bool {
    state(world, mob).is_some_and(|s| s.blink || s.special)
}

/// `Initialize`: its beacon where it stands.
pub(super) fn attach(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let beacon = crate::construction::spawn_obj(world, BEACON, pos);
    if let Some(mut m) = state_mut(world, mob) {
        m.hierophant.home = Some(pos);
        m.hierophant.beacon = beacon;
        m.hierophant.at_center = true;
    }
}

/// `calculate_rage`: the anger, and the burst and beam ranges with it.
pub fn anger_of(health: f32, max: f32) -> f32 {
    ((max - health) / 42.0).clamp(0.0, 50.0)
}

pub fn burst_range(anger: f32) -> u32 {
    BURST_RANGE + (anger * 0.08).floor() as u32
}

pub fn beam_range(anger: f32) -> u32 {
    BEAM_RANGE + (anger * 0.12).floor() as u32
}

fn calculate_rage(world: &mut World, mob: Entity) -> f32 {
    let anger = world
        .get::<Health>(mob)
        .map_or(0.0, |h| anger_of(h.health(), h.max));
    if let Some(mut m) = state_mut(world, mob) {
        m.hierophant.anger = anger;
        m.hierophant.at_center = false;
    }
    anger
}

fn anger(world: &World, mob: Entity) -> f32 {
    state(world, mob).map_or(0.0, |s| s.anger)
}

/// `update_cooldowns(COOLDOWN_UPDATE_SET_RANGED = max(0.5 s, base - anger * 0.75 ds))`.
fn set_ranged(world: &mut World, mob: Entity, base: f64, now: f64) {
    let a = f64::from(anger(world, mob));
    if let Some(mut m) = state_mut(world, mob) {
        m.ranged_ready = now + (base - a * 0.075).max(0.5);
    }
}

fn tint(world: &mut World, mob: Entity, on: bool) {
    if let Some(mut appearance) = world.get_mut::<ss13_shared::Appearance>(mob) {
        let color = on.then_some(PURPLE);
        if appearance.color != color {
            appearance.color = color;
        }
    }
}

fn pos_of(world: &World, e: Entity) -> Option<TilePos> {
    world.get::<TilePos>(e).copied()
}

fn alive(world: &World, e: Entity) -> bool {
    world.get::<Health>(e).is_some() && world.get::<Stat>(e).is_some_and(|s| *s != Stat::Dead)
}

/// The target's `cached_multiplicative_slowdown` in deciseconds, at least 1.
fn slowness(world: &World, target: Entity) -> f32 {
    world
        .get::<ss13_world::Mover>(target)
        .map_or(0.0, |m| m.delay * 10.0)
        .max(1.0)
}

/// `/mob/living/simple_animal/hostile/megafauna/hierophant/OpenFire`.
pub(super) fn open_fire(world: &mut World, mob: Entity, target: Entity, now: f64) {
    if blinking(world, mob) {
        return;
    }
    let anger = calculate_rage(world, mob);
    let blink_counter = 1 + (anger * 0.08).floor() as u32;
    let cross_counter = 1 + (anger * 0.12).floor() as u32;
    arena_trap(world, mob, target, now);
    set_ranged(world, mob, RANGED_COOLDOWN, now);
    let pace = slowness(world, target);
    let chaser_speed = (3.0 - anger * 0.04 + (pace - 1.0) * 0.5).max(1.0);
    let (Some(from), Some(to)) = (pos_of(world, mob), pos_of(world, target)) else {
        return;
    };
    let far = from.chebyshev(to).is_none_or(|d| d > 2);
    if world.get::<Stat>(target) == Some(&Stat::Dead) && far {
        blink(world, mob, to, now);
        return;
    }
    let chaser_ready = state(world, mob).is_some_and(|s| now >= s.chaser_ready);
    if roll(world, |rng| rng.chance(anger * 0.75)) {
        let mut options: Vec<u8> = Vec::new();
        if cross_counter > 1 {
            options.push(0);
        }
        if far {
            options.push(1);
        }
        if chaser_ready {
            if roll(world, |rng| rng.chance(anger * 2.0)) {
                options = vec![2];
            } else {
                options.push(2);
            }
        }
        if !options.is_empty() {
            let i = roll(world, |rng| rng.range(0, options.len() as u32 - 1)) as usize;
            match options[i] {
                0 => cross_blast_spam(world, mob, target, cross_counter, pace, now),
                1 => blink_spam(world, mob, target, blink_counter, pace, now),
                _ => chaser_swarm(world, mob, target, pace, now),
            }
            return;
        }
    }
    let (health, max) = world
        .get::<Health>(mob)
        .map_or((0.0, 1.0), |h| (h.health(), h.max));
    if chaser_ready {
        spawn_chaser(world, mob, target, from, chaser_speed, None, now);
        if let Some(mut m) = state_mut(world, mob) {
            m.hierophant.chaser_ready = now + CHASER_COOLDOWN;
        }
        let adjacent = from.chebyshev(to).is_some_and(|d| d <= 1);
        if roll(world, |rng| rng.chance(anger)) || adjacent {
            let dir = Dir::CARDINALS[roll(world, |rng| rng.range(0, 3)) as usize];
            spawn_chaser(
                world,
                mob,
                target,
                from,
                chaser_speed * 1.5,
                Some((dir, 4)),
                now,
            );
        }
    } else if far && roll(world, |rng| rng.chance(10.0 + anger * 0.5)) {
        blink(world, mob, to, now);
    } else if roll(world, |rng| rng.chance(70.0 - anger)) {
        let cross = roll(world, |rng| {
            if rng.chance(anger * (2.0 / pace)) && health < max * 0.5 {
                Cross::All
            } else if rng.chance(60.0) {
                Cross::Cardinal
            } else {
                Cross::Diagonal
            }
        });
        blasts(world, mob, target, cross, now);
    } else {
        burst(world, mob, from, burst_range(anger), 0.5, now);
    }
}

/// `AttackingTarget` on a living target: a burst around itself, then `OpenFire`.
pub(super) fn attacking_target(world: &mut World, mob: Entity, target: Entity, now: f64) {
    if blinking(world, mob) {
        return;
    }
    let Some(from) = pos_of(world, mob) else {
        return;
    };
    let ready = world
        .get::<Megafauna>(mob)
        .is_some_and(|m| now >= m.ranged_ready);
    if ready {
        let anger = calculate_rage(world, mob);
        set_ranged(world, mob, RANGED_COOLDOWN, now);
        burst(world, mob, from, burst_range(anger), 0.5, now);
    } else {
        burst(world, mob, from, BURST_RANGE, 0.25, now);
    }
    open_fire(world, mob, target, now);
}

/// `celebrate_kill`.
pub(super) fn celebrate_kill(world: &mut World, mob: Entity, victim: Entity) {
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    let i = roll(world, |rng| rng.range(0, KILL_PHRASES.len() as u32 - 1)) as usize;
    show(world, pos, Span::Danger, format!("\"{}\"", KILL_PHRASES[i]));
    let (name, victim) = (
        crate::security::name_of(world, mob),
        crate::security::name_of(world, victim),
    );
    show(
        world,
        pos,
        Span::Danger,
        format!("{name} поглощает жизненную силу {victim}!"),
    );
}

/// `GiveTarget` of a new target: a phrase, and the arena when it sits at home.
pub(super) fn on_new_target(world: &mut World, mob: Entity, target: Entity, now: f64) {
    if let Some(mut m) = state_mut(world, mob) {
        m.hierophant.home_at = None;
    }
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    let i = roll(world, |rng| rng.range(0, TARGET_PHRASES.len() as u32 - 1)) as usize;
    show(
        world,
        pos,
        Span::Danger,
        format!("\"{}\"", TARGET_PHRASES[i]),
    );
    let home = state(world, mob).is_some_and(|s| s.at_center && s.home == Some(pos));
    if home {
        arena_trap(world, mob, mob, now);
    }
}

/// Each AI tick with a target: `Moved` -> `arena_trap(target)`.
pub(super) fn on_target(world: &mut World, mob: Entity, target: Entity, now: f64) {
    if let Some(mut m) = state_mut(world, mob) {
        m.hierophant.home_at = None;
    }
    if !blinking(world, mob) {
        arena_trap(world, mob, target, now);
    }
}

/// `GiveTarget(null)` -> the 30 s `send_me_home` timer.
pub(super) fn on_no_target(world: &mut World, mob: Entity, now: f64) {
    let Some(s) = state(world, mob) else {
        return;
    };
    let Some(home) = s.home else {
        return;
    };
    match s.home_at {
        None => {
            if let Some(mut m) = state_mut(world, mob) {
                m.hierophant.home_at = Some(now + HOME_DELAY);
            }
        }
        Some(at) if now >= at && !blinking(world, mob) => {
            if let Some(mut m) = state_mut(world, mob) {
                m.hierophant.home_at = None;
            }
            send_me_home(world, mob, home, now);
        }
        Some(_) => {}
    }
}

/// `send_me_home`: back to the beacon, healed half its missing health (at least 250).
fn send_me_home(world: &mut World, mob: Entity, home: TilePos, now: f64) {
    let beacon_there = state(world, mob)
        .and_then(|s| s.beacon)
        .is_some_and(|b| world.get_entity(b).is_ok());
    if !beacon_there {
        return;
    }
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    if let Some(mut m) = state_mut(world, mob) {
        m.hierophant.at_center = true;
    }
    show(
        world,
        pos,
        Span::Danger,
        "\"Vixyvrmrk xs fewi...\"".to_owned(),
    );
    blink(world, mob, home, now);
    let Some(health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    let heal = ((health.max - health.health()) * 0.5).max(250.0);
    crate::mobs::defense::hurt(world, mob, DamageType::Brute, -heal, None);
    let healthy = world
        .get::<Health>(mob)
        .is_some_and(|h| h.health() > h.max * 0.9);
    let text = if healthy {
        "\"Vitemvw gsqtpixi. Stivexmrk ex qebmqyq ijjmgmirgc.\""
    } else {
        "\"Vitemvw gsqtpixi. Stivexmsrep ijjmgmirgc gsqtvsqmwih.\""
    };
    show(world, pos, Span::Bold, text.to_owned());
}

/// `blink(victim)`: to `to` in 0.5 s, blasting both ends.
fn blink(world: &mut World, mob: Entity, to: TilePos, now: f64) {
    let Some(from) = pos_of(world, mob) else {
        return;
    };
    if state(world, mob).is_none_or(|s| s.blink) {
        return;
    }
    let telegraph = Visual {
        layer: BELOW_MOB_LAYER,
        plane: GAME_PLANE,
        offset: [-32, -32],
        ..Visual::new(BIG, "hierophant_telegraph", 0.3)
    };
    spawn_visual(world, to, telegraph);
    spawn_visual(world, from, telegraph);
    sound(world, to, WAND_TELEPORT, 80.0, 0.0);
    sound(world, from, AIRLOCK_OPEN, 80.0, 0.0);
    if let Some(mut m) = state_mut(world, mob) {
        m.hierophant.blink = true;
        m.at(
            now + 0.2,
            MegaStep::Hierophant(Step::BlinkBlasts { from, to }),
        );
        m.at(now + 0.3, MegaStep::Hierophant(Step::FadeOut));
        m.at(now + 0.5, MegaStep::Hierophant(Step::Arrive(to)));
        m.at(now + 0.7, MegaStep::Hierophant(Step::FadeIn));
        m.at(now + 0.8, MegaStep::Hierophant(Step::BlinkDone));
    }
}

fn blink_spam(world: &mut World, mob: Entity, target: Entity, counter: u32, pace: f32, now: f64) {
    set_ranged(world, mob, MAJOR_COOLDOWN, now);
    let half = world
        .get::<Health>(mob)
        .is_some_and(|h| h.health() < h.max * 0.5);
    if !(half && counter > 1) {
        if let Some(to) = pos_of(world, target) {
            blink(world, mob, to, now);
        }
        return;
    }
    start_special(world, mob, "\"Mx ampp rsx iwgeti.\"");
    schedule(
        world,
        mob,
        now + 0.6,
        MegaStep::Hierophant(Step::BlinkSpam {
            target,
            left: counter,
            pace,
        }),
    );
}

fn cross_blast_spam(
    world: &mut World,
    mob: Entity,
    target: Entity,
    counter: u32,
    pace: f32,
    now: f64,
) {
    set_ranged(world, mob, MAJOR_COOLDOWN, now);
    start_special(world, mob, "\"Piezi mx rsalivi xs vyr.\"");
    schedule(
        world,
        mob,
        now + 0.6,
        MegaStep::Hierophant(Step::CrossSpam {
            target,
            left: counter,
            pace,
        }),
    );
}

fn chaser_swarm(world: &mut World, mob: Entity, target: Entity, pace: f32, now: f64) {
    set_ranged(world, mob, MAJOR_COOLDOWN, now);
    start_special(world, mob, "\"Mx gerrsx lmhi.\"");
    // `ListTargets`: what it hears within its aggro vision range.
    let targets = match pos_of(world, mob) {
        Some(pos) => world
            .resource::<SpatialIndex>()
            .in_radius(pos, 21)
            .filter(|&e| e != mob && world.get::<Health>(e).is_some())
            .filter(|&e| world.get::<crate::ghost::Ghost>(e).is_none())
            .collect(),
        None => Vec::new(),
    };
    schedule(
        world,
        mob,
        now + 0.6,
        MegaStep::Hierophant(Step::Swarm {
            target,
            targets,
            dirs: Dir::CARDINALS.to_vec(),
            pace,
        }),
    );
}

fn start_special(world: &mut World, mob: Entity, text: &str) {
    if let Some(pos) = pos_of(world, mob) {
        show(world, pos, Span::Bold, text.to_owned());
    }
    if let Some(mut m) = state_mut(world, mob) {
        m.hierophant.special = true;
    }
    tint(world, mob, true);
}

/// The end of a major attack: the tint off, `blinking` over 0.8 s later.
fn end_special(world: &mut World, mob: Entity, now: f64) {
    tint(world, mob, false);
    schedule(
        world,
        mob,
        now + 0.8,
        MegaStep::Hierophant(Step::EndSpecial),
    );
}

/// `blasts(victim, directions)`: telegraphed, then a blast on the target's tile and a line of
/// `beam_range` blasts each way 0.2 s later.
fn blasts(world: &mut World, mob: Entity, target: Entity, cross: Cross, now: f64) {
    let Some(at) = pos_of(world, target) else {
        return;
    };
    let state = match cross {
        Cross::Cardinal => "hierophant_telegraph_cardinal",
        Cross::Diagonal => "hierophant_telegraph_diagonal",
        Cross::All => "hierophant_telegraph",
    };
    spawn_visual(
        world,
        at,
        Visual {
            layer: BELOW_MOB_LAYER,
            offset: [-32, -32],
            ..Visual::new(BIG, state, 0.3)
        },
    );
    sound(world, at, BIN_CLOSE, 75.0, 0.0);
    schedule(
        world,
        mob,
        now + 0.2,
        MegaStep::Hierophant(Step::CrossFire { at, cross }),
    );
}

/// The tiles of a cross blast from `at`: the centre, then `range` tiles each way.
pub fn cross_tiles(at: TilePos, cross: Cross, range: u32) -> Vec<TilePos> {
    let mut out = vec![at];
    for &dir in cross.dirs() {
        let mut t = at;
        for _ in 0..range {
            t = t.step(dir);
            out.push(t);
        }
    }
    out
}

/// `hierophant_burst`: rings of blasts outwards from `center`; ring `d` waits `1 + min(range -
/// d, 12) * spread` deciseconds after the previous one. Returns (offset seconds, tile).
pub fn burst_tiles(center: TilePos, range: u32, spread: f32) -> Vec<(f64, TilePos)> {
    let mut out = vec![(0.0, center)];
    let mut t = 0.0;
    for d in 1..=range as i32 {
        t += f64::from(1.0 + (range as i32 - d).min(12) as f32 * spread) / 10.0;
        for dy in -d..=d {
            for dx in -d..=d {
                if dx.abs() == d || dy.abs() == d {
                    out.push((t, TilePos::new(center.x + dx, center.y + dy, center.z)));
                }
            }
        }
    }
    out
}

fn burst(world: &mut World, caster: Entity, center: TilePos, range: u32, spread: f32, now: f64) {
    sound(world, center, AIRLOCK_OPEN, 100.0, 0.0);
    queue_burst(world, Some(caster), center, range, spread, now);
}

fn queue_burst(
    world: &mut World,
    caster: Option<Entity>,
    center: TilePos,
    range: u32,
    spread: f32,
    now: f64,
) {
    let blast = Blast {
        caster,
        damage: BLAST_DAMAGE,
    };
    let tiles = burst_tiles(center, range, spread);
    let mut vortex = world.resource_mut::<Vortex>();
    for (t, pos) in tiles {
        vortex.queued.push((now + t, pos, blast));
    }
}

/// A blast appearing on `pos` now.
fn blast_now(world: &mut World, pos: TilePos, blast: Blast) {
    let now = now(world);
    world
        .resource_mut::<Vortex>()
        .queued
        .push((now, pos, blast));
}

/// `arena_trap(victim)`.
fn arena_trap(world: &mut World, mob: Entity, victim: Entity, now: f64) {
    let Some(at) = pos_of(world, victim) else {
        return;
    };
    let ready = state(world, mob).is_some_and(|s| now >= s.arena_ready);
    if !alive(world, victim) || !ready {
        return;
    }
    let in_arena = |p: Option<TilePos>| {
        p.and_then(|p| crate::events::area_type_at(world, p))
            .is_some_and(|a| is_under(a, ARENA_AREA))
    };
    if victim != mob && (in_arena(Some(at)) || in_arena(pos_of(world, mob))) {
        return;
    }
    if let Some(mut m) = state_mut(world, mob) {
        m.hierophant.arena_ready = now + ARENA_COOLDOWN;
    }
    let blast = Blast {
        caster: Some(mob),
        damage: BLAST_DAMAGE,
    };
    let r = ARENA_RADIUS;
    let ring: Vec<TilePos> = (-r..=r)
        .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
        .filter(|(dx, dy)| dx.abs() == r || dy.abs() == r)
        .map(|(dx, dy)| TilePos::new(at.x + dx, at.y + dy, at.z))
        .filter(|&p| world.resource::<SpatialIndex>().in_bounds(p))
        .collect();
    for pos in ring {
        let riveted = crate::construction::turf_at(world, pos)
            .and_then(|t| crate::construction::path_of(world, t))
            .is_some_and(|p| is_under(&p, ARENA_WALL_TURF));
        if riveted {
            continue;
        }
        spawn_wall(world, mob, pos);
        blast_now(world, pos, blast);
    }
    let far = pos_of(world, mob)
        .and_then(|p| p.chebyshev(at))
        .is_none_or(|d| d >= ARENA_RADIUS as u32);
    if far {
        blink(world, mob, at, now);
    }
}

fn wall_lets_through(world: &World, mover: Entity, wall: Entity) -> bool {
    let Some(caster) = world.get::<VortexWall>(wall).map(|w| w.caster) else {
        return true;
    };
    // `QDELETED(caster)`: nobody; the caster and whoever pulls it pass.
    world.get_entity(caster).is_ok()
        && (mover == caster
            || world
                .get::<crate::pull::PulledBy>(caster)
                .is_some_and(|p| p.puller() == mover))
}

/// `/obj/effect/temp_visual/hierophant/wall` (10 s).
fn spawn_wall(world: &mut World, caster: Entity, pos: TilePos) {
    let visual = Visual {
        layer: BELOW_MOB_LAYER,
        ..Visual::new(WALL_DMI, "hierophant_wall_temp-0", 10.0)
    };
    let wall = match spawn_visual(world, pos, visual) {
        Some(e) => e,
        None => world
            .spawn((
                pos,
                super::TempVisual {
                    until: now(world) + 10.0,
                },
            ))
            .id(),
    };
    world.entity_mut(wall).insert((
        VortexWall { caster },
        Density,
        EnterPolicy(wall_lets_through),
    ));
}

/// A chaser from `from` after `target` (`chaser/Initialize`; `preset`: its first direction and
/// legs, as the swarm and the second chaser set them).
fn spawn_chaser(
    world: &mut World,
    caster: Entity,
    target: Entity,
    from: TilePos,
    speed_ds: f32,
    preset: Option<(Dir, u32)>,
    now: f64,
) {
    let (dir, moving) = match preset {
        Some((d, m)) => (Some(d), m),
        None => (None, 0),
    };
    world.resource_mut::<Vortex>().chasers.push(Chaser {
        caster,
        target,
        pos: from,
        dir,
        prev: None,
        prev2: None,
        moving,
        speed: f64::from(speed_ds) / 10.0,
        // `addtimer(seek_target, 0.1 SECONDS)`.
        next_at: now + 0.1,
        until: now + CHASER_LIFE,
    });
}

/// `get_cardinal_dir(start, end)`: the vertical or horizontal part of the way, at random weighed
/// by the distances; `None` on the same tile.
fn cardinal_dir(from: TilePos, to: TilePos, roll01: f32) -> Option<Dir> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    if dx == 0 && dy == 0 {
        return None;
    }
    let (ax, ay) = (dx.abs() as f32, dy.abs() as f32);
    if roll01 * (ax + ay) < ay {
        Some(if dy > 0 { Dir::North } else { Dir::South })
    } else {
        Some(if dx > 0 { Dir::East } else { Dir::West })
    }
}

/// One step of a chaser (`seek_target`'s loop body). False when it is done.
fn chaser_step(world: &mut World, c: &mut Chaser) -> bool {
    let Some(target_at) = pos_of(world, c.target) else {
        return false;
    };
    if c.moving == 0 {
        c.prev2 = c.prev;
        c.prev = c.dir;
        let (r1, r2, pick) = roll(world, |rng| (rng.unit(), rng.unit(), rng.range(0, 2)));
        // `get_target_dir`: alternating back and forth picks another way.
        let mut d = cardinal_dir(c.pos, target_at, r1);
        if (d != c.prev && d == c.prev2) || d.is_none() {
            let options: Vec<Dir> = Dir::CARDINALS
                .into_iter()
                .filter(|&x| Some(x) != c.prev2)
                .collect();
            d = options.get(pick as usize % options.len()).copied();
        }
        c.dir = d;
        let standard = cardinal_dir(c.pos, target_at, r2);
        c.moving = if (standard != c.prev && standard == c.prev2) || standard.is_none() {
            1
        } else {
            CHASER_LEG
        };
    }
    let Some(dir) = c.dir else {
        return true;
    };
    let next = c.pos.step(dir);
    if world.resource::<SpatialIndex>().in_bounds(next) {
        c.pos = next;
    }
    blast_now(
        world,
        c.pos,
        Blast {
            caster: Some(c.caster),
            damage: BLAST_DAMAGE,
        },
    );
    c.moving -= 1;
    true
}

/// Chasers step, blasts appear and go off.
// Cost: per tick, the live blasts and chasers (a few hundred at most during a burst).
pub(super) fn tick(world: &mut World) {
    let now = now(world);
    // Chasers.
    let mut chasers = std::mem::take(&mut world.resource_mut::<Vortex>().chasers);
    chasers.retain_mut(|c| {
        while c.next_at <= now {
            if now >= c.until || !chaser_step(world, c) {
                return false;
            }
            c.next_at += c.speed.max(0.05);
        }
        now < c.until
    });
    world.resource_mut::<Vortex>().chasers.extend(chasers);
    // Blasts appearing.
    let appearing: Vec<(f64, TilePos, Blast)> = {
        let mut v = world.resource_mut::<Vortex>();
        let (due, rest): (Vec<_>, Vec<_>) = v.queued.drain(..).partition(|(t, ..)| *t <= now);
        v.queued = rest;
        due
    };
    for (t, pos, blast) in appearing {
        if !world.resource::<SpatialIndex>().in_bounds(pos) {
            continue;
        }
        spawn_visual(
            world,
            pos,
            Visual {
                layer: BELOW_MOB_LAYER,
                ..Visual::new(EFFECTS, "hierophant_blast", 0.9)
            },
        );
        sound(world, pos, BLIND, 65.0, -5.0);
        crate::mining::rocks::gets_drilled(world, pos, blast.caster);
        world
            .resource_mut::<Vortex>()
            .armed
            .push((t + FUSE, pos, blast));
    }
    // Blasts going off.
    let going: Vec<(f64, TilePos, Blast)> = {
        let mut v = world.resource_mut::<Vortex>();
        let (due, rest): (Vec<_>, Vec<_>) = v.armed.drain(..).partition(|(t, ..)| *t <= now);
        v.armed = rest;
        due
    };
    for (_, pos, blast) in going {
        do_damage(world, pos, blast);
    }
}

/// `blast/damaging/do_damage`.
fn do_damage(world: &mut World, pos: TilePos, blast: Blast) {
    for victim in living_on(world, pos) {
        if Some(victim) == blast.caster || world.get::<Stat>(victim) == Some(&Stat::Dead) {
            continue;
        }
        sound(world, pos, SEAR, 50.0, -4.0);
        crate::tools::tell_mob(
            world,
            victim,
            Span::BoldWarning,
            "Вас поражает вихревой взрыв!",
        );
        crate::mobs::defense::hurt(world, victim, DamageType::Burn, blast.damage, None);
        let monster = world.get::<crate::mobs::BasicMob>(victim).is_some_and(|b| {
            is_under(&b.def.path, ss13_mapload::mobs::MEGAFAUNA_ROOT)
                || is_under(&b.def.path, ELITE_ROOT)
        });
        if monster {
            crate::mobs::defense::hurt(world, victim, DamageType::Brute, blast.damage, None);
        }
    }
}

pub(super) fn run(world: &mut World, mob: Entity, at: f64, step: Step) {
    match step {
        Step::BlinkBlasts { from, to } => {
            let teleport = Visual {
                layer: BELOW_MOB_LAYER,
                offset: [-32, -32],
                ..Visual::new(BIG, "hierophant_telegraph_teleport", 0.9)
            };
            spawn_visual(world, to, teleport);
            spawn_visual(world, from, teleport);
            let blast = Blast {
                caster: Some(mob),
                damage: BLINK_DAMAGE,
            };
            for center in [to, from] {
                for (_, pos) in burst_tiles(center, 1, 0.0) {
                    blast_now(world, pos, blast);
                }
            }
        }
        Step::FadeOut => {
            if let Some(pos) = pos_of(world, mob) {
                let name = crate::security::name_of(world, mob);
                show(world, pos, Span::Danger, format!("{name} растворяется!"));
            }
            world.entity_mut(mob).remove::<Density>();
        }
        Step::Arrive(to) => {
            super::stop_moving(world, mob);
            world.entity_mut(mob).insert(to);
        }
        Step::FadeIn => {
            world.entity_mut(mob).insert(Density);
            if let Some(pos) = pos_of(world, mob) {
                let name = crate::security::name_of(world, mob);
                show(world, pos, Span::Danger, format!("{name} проявляется!"));
            }
        }
        Step::BlinkDone => {
            if let Some(mut m) = state_mut(world, mob) {
                m.hierophant.blink = false;
            }
        }
        Step::CrossFire { at: pos, cross } => {
            let range = beam_range(anger(world, mob));
            let blast = Blast {
                caster: Some(mob),
                damage: BLAST_DAMAGE,
            };
            for t in cross_tiles(pos, cross, range) {
                blast_now(world, t, blast);
            }
        }
        Step::CrossSpam { target, left, pace } => {
            if left == 0 || world.get_entity(target).is_err() {
                end_special(world, mob, at);
                return;
            }
            let cross = if roll(world, |rng| rng.chance(60.0)) {
                Cross::Cardinal
            } else {
                Cross::Diagonal
            };
            blasts(world, mob, target, cross, at);
            schedule(
                world,
                mob,
                at + 0.6 + f64::from(pace) / 10.0,
                MegaStep::Hierophant(Step::CrossSpam {
                    target,
                    left: left - 1,
                    pace,
                }),
            );
        }
        Step::BlinkSpam { target, left, pace } => {
            let to = pos_of(world, target);
            if left == 0 || to.is_none() || to == pos_of(world, mob) {
                end_special(world, mob, at);
                return;
            }
            if let Some(to) = to {
                blink(world, mob, to, at);
            }
            schedule(
                world,
                mob,
                at + 0.8 + 0.4 + f64::from(pace) / 10.0,
                MegaStep::Hierophant(Step::BlinkSpam {
                    target,
                    left: left - 1,
                    pace,
                }),
            );
        }
        Step::Swarm {
            target,
            mut targets,
            mut dirs,
            pace,
        } => {
            if targets.is_empty() || dirs.is_empty() {
                finish_swarm(world, mob, at);
                return;
            }
            let i = roll(world, |rng| rng.range(0, targets.len() as u32 - 1)) as usize;
            let mut picked = targets[i];
            if targets.len() >= dirs.len() {
                targets.swap_remove(i);
            }
            if !alive(world, picked) {
                picked = target;
                if !alive(world, picked) {
                    finish_swarm(world, mob, at);
                    return;
                }
            }
            let j = roll(world, |rng| rng.range(0, dirs.len() as u32 - 1)) as usize;
            let dir = dirs.swap_remove(j);
            let speed = state(world, mob).map_or(3.0, |s| (3.0 - s.anger * 0.04).max(1.0))
                + (pace - 1.0) * 0.5;
            if let Some(from) = pos_of(world, mob) {
                spawn_chaser(world, mob, picked, from, speed.max(1.0), Some((dir, 3)), at);
            }
            schedule(
                world,
                mob,
                at + 0.8 + f64::from(pace) / 10.0,
                MegaStep::Hierophant(Step::Swarm {
                    target,
                    targets,
                    dirs,
                    pace,
                }),
            );
        }
        Step::EndSpecial => {
            if let Some(mut m) = state_mut(world, mob) {
                m.hierophant.special = false;
            }
        }
    }
}

fn finish_swarm(world: &mut World, mob: Entity, at: f64) {
    if let Some(mut m) = state_mut(world, mob) {
        m.hierophant.chaser_ready = at + CHASER_COOLDOWN;
    }
    end_special(world, mob, at);
}

/// `hierophant/death`: the self-destruct burst of range 10 (it hurts everyone), its club, its
/// beacon gone (`Destroy`).
pub(super) fn on_death(world: &mut World, mob: Entity, pos: TilePos) {
    show(
        world,
        pos,
        Span::Bold,
        "\"Mrmxmexmrk wipj-hiwxvygx wiuyirgi...\"".to_owned(),
    );
    let name = crate::security::name_of(world, mob);
    show(
        world,
        pos,
        Span::Danger,
        format!("{name} сжимается, высвобождая мощный выброс энергии!"),
    );
    let now = now(world);
    sound(world, pos, AIRLOCK_OPEN, 100.0, 0.0);
    queue_burst(world, None, pos, 10, 0.5, now);
    crate::construction::spawn_obj(world, CLUB, pos);
    if let Some(beacon) = state(world, mob).and_then(|s| s.beacon)
        && world.get_entity(beacon).is_ok()
    {
        world.despawn(beacon);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anger_grows_the_ranges() {
        assert_eq!(anger_of(2500.0, 2500.0), 0.0);
        assert_eq!(anger_of(0.0, 2500.0), 50.0);
        assert_eq!(burst_range(0.0), 3);
        assert_eq!(beam_range(0.0), 5);
        // 1260 health lost: anger 30, +2 burst, +3 beam.
        let a = anger_of(1240.0, 2500.0);
        assert_eq!(a, 30.0);
        assert_eq!((burst_range(a), beam_range(a)), (5, 8));
    }

    #[test]
    fn burst_rings_speed_up_outwards() {
        let c = TilePos::new(10, 10, 1);
        let tiles = burst_tiles(c, 3, 0.5);
        assert_eq!(tiles.len(), 49, "a 7x7 square");
        assert_eq!(tiles[0], (0.0, c));
        let ring = |d: i32| {
            tiles
                .iter()
                .find(|(_, p)| p.chebyshev(c) == Some(d as u32))
                .map(|(t, _)| *t)
                .unwrap()
        };
        // 1 + min(3 - d, 12) * 0.5 deciseconds before ring d.
        assert!((ring(1) - 0.2).abs() < 1e-9);
        assert!((ring(2) - 0.35).abs() < 1e-9);
        assert!((ring(3) - 0.45).abs() < 1e-9);
    }

    #[test]
    fn cross_blasts_reach_beam_range() {
        let c = TilePos::new(10, 10, 1);
        let cardinal = cross_tiles(c, Cross::Cardinal, 5);
        assert_eq!(cardinal.len(), 21);
        assert!(cardinal.contains(&TilePos::new(15, 10, 1)));
        let diagonal = cross_tiles(c, Cross::Diagonal, 5);
        assert!(diagonal.contains(&TilePos::new(5, 5, 1)));
        assert_eq!(cross_tiles(c, Cross::All, 2).len(), 17);
    }

    #[test]
    fn chasers_head_the_long_way_mostly() {
        let a = TilePos::new(0, 0, 1);
        assert_eq!(cardinal_dir(a, a, 0.5), None);
        // dx 4, dy 1: north only when the roll falls in the first fifth.
        assert_eq!(
            cardinal_dir(a, TilePos::new(4, 1, 1), 0.1),
            Some(Dir::North)
        );
        assert_eq!(cardinal_dir(a, TilePos::new(4, 1, 1), 0.5), Some(Dir::East));
        assert_eq!(
            cardinal_dir(a, TilePos::new(-4, 0, 1), 0.0),
            Some(Dir::West)
        );
    }
}
