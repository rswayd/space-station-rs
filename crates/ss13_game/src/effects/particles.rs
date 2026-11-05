//! Basic effect systems, after tg `/datum/effect_system/basic` (`effect_system.dm`),
//! `effects_sparks.dm` and the steam of `effects_water.dm`.
//!
//! - [`start`] spawns `amount` particles (at most 21, `PER_SYSTEM_PARTICLE_CAP`) on a tile,
//!   each flying off in its own direction (cardinals only or all eight, not reused until all
//!   are taken) one tile every `step_delay` for its step count. A particle that cannot move
//!   stops; lightning and steam are deleted then (`delete_on_stop`).
//! - Sparks (`do_sparks`) live as long as their flight (`decay_in(last_loop_length)`), 0.1 s
//!   after a failed step; they light (their prototype's light), crackle, expose their tile to
//!   1000 K (`hotspot_expose(1000, 100)`) where they appear, on each step and where they die,
//!   and heat the unsealed reagents of what they touch on the way, held items included
//!   (`expose_temperature(1000)`). Quantum sparks are the blue kind.
//! - Steam lives 2 s at most; lightning and steam are gone when their flight ends.
// shortcut: no light fade-out animation (`light_middleman`), no `COMSIG_ATOM_TOUCHED_SPARKS`
// (plasma floors, hazardous sparks), no flick of the spark animation.

use bevy::prelude::*;
use ss13_reagents::flags;
use ss13_shared::{Dir, TilePos};

use super::{Pass, on_tile, step_blocked};
use crate::chem::{Chem, Reagents};
use crate::combat::CombatRng;

pub const SPARKS: &str = "/obj/effect/particle_effect/sparks";
pub const QUANTUM_SPARKS: &str = "/obj/effect/particle_effect/sparks/quantum";
pub const LIGHTNING: &str = "/obj/effect/particle_effect/sparks/electricity";
pub const STEAM: &str = "/obj/effect/particle_effect/steam";

/// `PER_SYSTEM_PARTICLE_CAP`: `start` stops once more than this many are out.
const PARTICLE_CAP: u32 = 20;
/// `affect_location`: `hotspot_expose(1000, 100)`, `expose_temperature(1000)` (default
/// `coeff` 0.02).
const SPARK_HEAT: f64 = 1000.0;
const SPARK_VOLUME: f64 = 100.0;
const SPARK_REAGENT_HEAT: f32 = 1000.0;
const EXPOSE_COEFF: f32 = 0.02;
/// `move_failed`: `decay_in(0.1 SECONDS)`.
const FAILED_DECAY: f64 = 0.1;
/// `/obj/effect/particle_effect/steam`: `QDEL_IN(src, 2 SECONDS)`.
const STEAM_LIFETIME: f64 = 2.0;

const ALL_DIRS: [Dir; 8] = [
    Dir::North,
    Dir::South,
    Dir::East,
    Dir::West,
    Dir::NorthEast,
    Dir::NorthWest,
    Dir::SouthEast,
    Dir::SouthWest,
];

/// The ported `/datum/effect_system/basic` subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `spark_spread`.
    Sparks,
    /// `spark_spread/quantum`.
    QuantumSparks,
    /// `lightning_spread`.
    Lightning,
    /// `steam_spread`.
    Steam,
}

impl Kind {
    pub fn path(self) -> &'static str {
        match self {
            Self::Sparks => SPARKS,
            Self::QuantumSparks => QUANTUM_SPARKS,
            Self::Lightning => LIGHTNING,
            Self::Steam => STEAM,
        }
    }

    /// `step_delay`.
    fn step_delay(self) -> f64 {
        match self {
            Self::Sparks | Self::QuantumSparks => 0.35,
            Self::Lightning | Self::Steam => 0.5,
        }
    }

    /// `get_step_count`: sparks never take 1 step.
    fn steps(self, rng: &mut CombatRng) -> u32 {
        match self {
            Self::Sparks | Self::QuantumSparks => rng.range(2, 3),
            Self::Lightning | Self::Steam => rng.range(1, 3),
        }
    }

    fn sparks(self) -> bool {
        matches!(self, Self::Sparks | Self::QuantumSparks | Self::Lightning)
    }

    /// `delete_on_stop`.
    fn delete_on_stop(self) -> bool {
        matches!(self, Self::Lightning | Self::Steam)
    }
}

/// A flying particle: `steps` more steps in `dir`, the next at `next_step`; gone at `until`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub kind: Kind,
    pub dir: Dir,
    pub steps: u32,
    pub next_step: f64,
    pub until: f64,
}

/// tg `do_sparks(number, cardinal_only, source)`.
pub fn do_sparks(world: &mut World, number: u32, cardinal_only: bool, pos: TilePos) {
    start(world, Kind::Sparks, number, cardinal_only, pos);
}

/// `new spark_type(...)` / `steam_spread` `set_up` and `start()`: `amount` particles of `kind`
/// from `pos`.
pub fn start(world: &mut World, kind: Kind, amount: u32, cardinal_only: bool, pos: TilePos) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut pickable: Vec<Dir> = Vec::new();
    for spawned in 0..amount {
        if spawned > PARTICLE_CAP {
            return;
        }
        if pickable.is_empty() {
            pickable = if cardinal_only {
                Dir::CARDINALS.to_vec()
            } else {
                ALL_DIRS.to_vec()
            };
        }
        let mut rng = world.resource_mut::<CombatRng>();
        let dir = pickable.swap_remove(rng.range(0, pickable.len() as u32 - 1) as usize);
        let steps = kind.steps(&mut rng);
        let Some(e) = crate::construction::spawn_obj(world, kind.path(), pos) else {
            return;
        };
        // `MOVEMENT_LOOP_START_FAST`: the first step at once; the loop lasts its steps.
        let mut until = now + kind.step_delay() * f64::from(steps);
        if kind == Kind::Steam {
            until = until.min(now + STEAM_LIFETIME);
        }
        world.entity_mut(e).insert(Particle {
            kind,
            dir,
            steps,
            next_step: now,
            until,
        });
        if kind.sparks() {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, "sparks", 100.0, true)
                    .extrarange(crate::sound::SHORT_RANGE_SOUND_EXTRARANGE),
            );
            affect_location(world, pos, true);
        }
    }
}

/// `affect_location`: the turf's hotspot exposure, and on `touch` the things on it
/// (`sparks_touched`).
fn affect_location(world: &mut World, pos: TilePos, touch: bool) {
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.hotspot_expose(pos, SPARK_HEAT, SPARK_VOLUME, false);
    }
    if !touch {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    for e in on_tile(world, pos) {
        let mut touched = vec![e];
        // `ishuman`: the visible items (worn and held).
        if let Some(inv) = world.get::<crate::inventory::Inventory>(e) {
            touched.extend(inv.hands.iter().flatten().copied());
            touched.extend(inv.equipped());
        }
        for thing in touched {
            if let Some(mut r) = world.get_mut::<Reagents>(thing)
                && r.0.flags & flags::SEALED_CONTAINER == 0
                && r.0.total_volume > 0.0
            {
                r.0.expose_temperature(&chem, SPARK_REAGENT_HEAT, EXPOSE_COEFF);
            }
        }
    }
}

/// The move loops and lifetimes of every particle.
// Cost: per tick, one pass over the live particles (a handful per burst, 2 s at most).
pub(super) fn tick_particles(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let live: Vec<(Entity, Particle, TilePos)> = world
        .query::<(Entity, &Particle, &TilePos)>()
        .iter(world)
        .map(|(e, p, pos)| (e, *p, *pos))
        .collect();
    for (e, mut p, pos) in live {
        if now >= p.until {
            if p.kind.sparks() {
                affect_location(world, pos, false);
            }
            world.despawn(e);
            continue;
        }
        if p.steps == 0 || now < p.next_step {
            continue;
        }
        p.steps -= 1;
        p.next_step = now + p.kind.step_delay();
        if step_blocked(world, pos, p.dir, Pass::Particle) {
            p.steps = 0;
            if p.kind.delete_on_stop() {
                world.despawn(e);
                continue;
            }
            if p.kind.sparks() {
                p.until = p.until.min(now + FAILED_DECAY);
            }
            world.entity_mut(e).insert(p);
            continue;
        }
        let to = pos.step(p.dir);
        world.entity_mut(e).insert((p, to));
        if p.kind.sparks() {
            affect_location(world, to, true);
        }
    }
}
