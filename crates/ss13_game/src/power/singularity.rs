//! Gravitational singularities (server): tg `code/modules/power/singularity/singularity.dm`,
//! `dark_matter_singularity.dm`, `reality_tear.dm` and `code/datums/components/singularity.dm`.
//!
//! - A [`Singularity`] (`/obj/singularity`, from the map, a singularity delamination of the
//!   supermatter, a dark matt-eor or a collapsing tear) moves and eats once a second: one step
//!   in a random direction but the last refused one (`move`), eating what is in the way first;
//!   then everything within `grav_pull` tiles is pulled one step closer (`singularity_pull`)
//!   and everything within `consume_range` is eaten (`singularity_act`): an object is deleted
//!   (+2 energy), a turf scraped one layer (+2), a mob gibbed (+20; engineers +100, the clown
//!   anywhere from -1000 to +1000), a supermatter crystal +100 and the singularity charged, a
//!   bag of holding collapses it. A step whose leading edge (by size) touches a containment
//!   field, an active field generator or a shield wall is refused (`check_turfs_in`), so a
//!   field ring holds it.
//! - Every 2 s (`process`): from stage two a 10% chance of an event (EMP 8/10, stunning the
//!   carbons looking at it without mesons, setting carbons within 20 tiles on fire at stage
//!   six), then it loses `dissipate_strength` per `dissipate_delay` seconds and grows or
//!   shrinks with its energy (`check_energy`, `expand`): 1/200/500/1000/2000/3000 energy for
//!   stages one to six, the sixth only after a supermatter; stages two to four need their
//!   cardinal edges clear of fields (it may step to find room). Out of energy it collapses.
//! - The dark matter singularity hunts the nearest living mob (`bloodthirsty`), stops at stage
//!   four and shrugs off explosions for 5 s. A tear in reality is a stage-six singularity that
//!   stands still and pulls from 21 tiles for 9 s, then collapses into a stage-two singularity
//!   of 400 energy (the temporary one: 2 s, then nothing).
//!
//! Cost: one or two singularities in a round; each pulls `(2 * grav_pull + 1)^2` tiles a second
//! (961 at stage six, 1849 for the 9 s of a tear).
// shortcut: no ghost alerts, admin logs, renames ("supermatter-charged ..."), vision hurting at
// stage six, telekinesis gibbing, singularity beacons, deadchat control, held items torn from
// hands, BSA or blob reactions, `notify_ghosts`. Projectiles other than emitter beams are not
// swallowed (`crate::projectile` does not know it).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmItem, DmObj, DmTurfBuild};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, Optics, TilePos};
use ss13_world::{Bumped, SpatialIndex};

use super::EngineRng;
use super::field_generator::{ContainmentField, FgActive, FieldGenerator};
use super::shields::{ShieldWall, ShieldWallGen};
use super::supermatter::{Delam, Delaminated, Supermatter};
use crate::construction::{path_of, turf_at};
use crate::explosions::Severity;
use crate::health::{Health, Stat};

pub const SINGULARITY: &str = "/obj/singularity";
pub const DARK_MATTER: &str = "/obj/singularity/dark_matter";
pub const REALITY_TEAR: &str = "/obj/reality_tear";
pub const REALITY_TEAR_TEMPORARY: &str = "/obj/reality_tear/temporary";
const BAG_OF_HOLDING: &str = "/obj/item/storage/backpack/holding";

/// `STAGE_ONE` .. `STAGE_SIX` (`code/__DEFINES/gravity.dm`).
pub const STAGE_ONE: u8 = 1;
pub const STAGE_TWO: u8 = 3;
pub const STAGE_THREE: u8 = 5;
pub const STAGE_FOUR: u8 = 7;
pub const STAGE_FIVE: u8 = 9;
pub const STAGE_SIX: u8 = 11;
/// `CHANCE_TO_MOVE_TO_TARGET`, `CHANCE_TO_MOVE_TO_TARGET_BLOODTHIRSTY`, percent.
const CHANCE_TO_MOVE_TO_TARGET: f32 = 60.0;
const CHANCE_TO_MOVE_TO_TARGET_BLOODTHIRSTY: f32 = 80.0;
/// `DARK_MATTER_SUPERMATTER_CHANCE_BONUS`.
const DARK_MATTER_SUPERMATTER_CHANCE_BONUS: f32 = 10.0;
/// The component moves and eats once a second; the object processes every 2 s.
const EAT_SECS: f64 = 1.0;
const ACT_SECS: f64 = 2.0;
/// `throw_at(singularity, 14, 3)` of pulled items and stage-six victims.
const PULL_THROW_RANGE: u32 = 14;
const PULL_THROW_SPEED: f32 = 3.0;
/// `apply_effect(60, EFFECT_STUN)` of `mezzer`, seconds.
const MEZZER_STUN_SECS: f32 = 6.0;
/// `GLOB.alldirs`.
const ALLDIRS: [Dir; 8] = [
    Dir::North,
    Dir::South,
    Dir::East,
    Dir::West,
    Dir::NorthEast,
    Dir::NorthWest,
    Dir::SouthEast,
    Dir::SouthWest,
];
/// Atoms whose `singularity_act` and `singularity_pull` do nothing.
const UNTOUCHABLE: [&str; 7] = [
    "/obj/effect/landmark",
    "/obj/effect/abstract",
    "/obj/effect/dummy",
    "/obj/effect/beam",
    "/obj/effect/bump_teleporter",
    "/obj/docking_port",
    "/obj/effect/immovablerod",
];
/// Turfs whose `singularity_act` does nothing.
const UNEATABLE_TURFS: [&str; 5] = [
    "/turf/open/space",
    "/turf/open/lava",
    "/turf/open/indestructible",
    "/turf/closed/indestructible",
    "/turf/cordon",
];
/// What falls apart under a stage-five pull (`deconstruct`, disassembled or not).
const PULLED_APART: [(&str, bool); 4] = [
    ("/obj/structure/cable", true),
    ("/obj/machinery/atmospherics", false),
    ("/obj/structure/disposalpipe", true),
    ("/obj/machinery/disposal", true),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SingKind {
    /// `/obj/singularity`.
    Gravity,
    /// `/obj/singularity/dark_matter`.
    DarkMatter,
    /// `/obj/reality_tear` after `start_disaster`.
    Tear { temporary: bool },
}

/// tg `/obj/singularity` with its `/datum/component/singularity` (a tear has only the latter).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Singularity {
    pub kind: SingKind,
    pub energy: f64,
    pub current_size: u8,
    pub allowed_size: u8,
    pub maximum_stage: u8,
    pub dissipate: bool,
    pub dissipate_delay: f64,
    pub dissipate_strength: f64,
    pub since_dissipation: f64,
    pub event_chance: f32,
    pub move_self: bool,
    pub consumed_supermatter: bool,
    /// Ate a bag of holding: gone at `ends_at`.
    pub collapsing: bool,
    pub consume_range: u32,
    pub grav_pull: u32,
    pub roaming: bool,
    pub disregard_failed_movements: bool,
    pub last_failed_movement: Option<Dir>,
    pub chance_to_move_to_target: f32,
    pub target: Option<Entity>,
    pub next_eat: f64,
    pub next_act: f64,
    pub last_act: f64,
    /// Dark matter: `initial_explosion_immunity` until then; `normalize_tracking` then.
    pub explosion_immune_until: f64,
    pub normalize_tracking_at: Option<f64>,
    /// A tear's collapse, a collapsing singularity's end.
    pub ends_at: Option<f64>,
    /// `consume_boh_sfx` of a collapsing singularity.
    pub boh_sound_at: Option<f64>,
}

impl Singularity {
    pub fn new(kind: SingKind, now: f64) -> Self {
        let base = Self {
            kind,
            energy: 50.0,
            current_size: STAGE_ONE,
            allowed_size: STAGE_ONE,
            maximum_stage: STAGE_SIX,
            dissipate: true,
            dissipate_delay: 20.0,
            dissipate_strength: 1.0,
            since_dissipation: 0.0,
            event_chance: 10.0,
            move_self: true,
            consumed_supermatter: false,
            collapsing: false,
            consume_range: 0,
            grav_pull: 4,
            roaming: false,
            disregard_failed_movements: false,
            last_failed_movement: None,
            chance_to_move_to_target: CHANCE_TO_MOVE_TO_TARGET,
            target: None,
            next_eat: now,
            next_act: now,
            last_act: now,
            explosion_immune_until: f64::NEG_INFINITY,
            normalize_tracking_at: None,
            ends_at: None,
            boh_sound_at: None,
        };
        match kind {
            SingKind::Gravity => base,
            SingKind::DarkMatter => Self {
                energy: 250.0,
                maximum_stage: STAGE_FOUR,
                chance_to_move_to_target: 100.0,
                explosion_immune_until: now + 5.0,
                normalize_tracking_at: Some(now + 20.0),
                ..base
            },
            SingKind::Tear { temporary } => Self {
                current_size: STAGE_SIX,
                allowed_size: STAGE_SIX,
                dissipate: false,
                move_self: false,
                consume_range: if temporary { 2 } else { 1 },
                grav_pull: if temporary { 3 } else { 21 },
                ends_at: Some(now + if temporary { 2.0 } else { 9.0 }),
                ..base
            },
        }
    }

    pub fn is_tear(&self) -> bool {
        matches!(self.kind, SingKind::Tear { .. })
    }

    /// `dissipate()` over `secs` of real time.
    // tg adds one subsystem tick (0.05 s) per 2 s act, against what `dissipate_delay` ("in
    // seconds") says; the port counts the real time.
    pub fn dissipate(&mut self, secs: f64) {
        if !self.dissipate {
            return;
        }
        self.since_dissipation += secs;
        while self.since_dissipation >= self.dissipate_delay {
            self.energy -= self.dissipate_strength;
            self.since_dissipation -= self.dissipate_delay;
        }
    }

    /// `ex_act`. True when the blast destroys it.
    pub fn ex_act(&mut self, severity: Severity, now: f64) -> bool {
        if self.is_tear() || now < self.explosion_immune_until {
            return false;
        }
        // DM `round(x, 1)`.
        let lose = |divisor: f64| ((self.energy + 1.0) / divisor + 0.5).floor();
        match severity {
            Severity::Devastate => {
                if self.current_size <= STAGE_TWO {
                    return true;
                }
                self.energy -= lose(2.0);
            }
            Severity::Heavy => self.energy -= lose(3.0),
            Severity::Light => self.energy -= lose(4.0),
        }
        false
    }
}

/// `check_energy`'s stage for `energy` (DM `switch` ranges, first match); `None` below 1.
pub fn allowed_size(energy: f64, consumed_supermatter: bool) -> Option<u8> {
    Some(if energy < 1.0 {
        return None;
    } else if energy <= 200.0 {
        STAGE_ONE
    } else if energy <= 500.0 {
        STAGE_TWO
    } else if energy <= 1000.0 {
        STAGE_THREE
    } else if energy <= 2000.0 {
        STAGE_FOUR
    } else if energy <= 3000.0 {
        STAGE_FIVE
    } else if consumed_supermatter {
        STAGE_SIX
    } else {
        STAGE_FIVE
    })
}

/// One stage of `expand`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StageParams {
    pub icon: &'static str,
    /// `[variant]_s1` .. `_s11`.
    pub suffix: &'static str,
    pub offset: i16,
    pub grav_pull: u32,
    pub consume_range: u32,
    /// `dissipate_delay`, `dissipate_strength`; `None`: `dissipate = FALSE`.
    pub dissipation: Option<(f64, f64)>,
    /// Must its cardinal edges this far out be free of fields (`check_cardinals_range`)?
    pub needs_room: Option<u32>,
}

pub fn stage_params(size: u8) -> Option<StageParams> {
    let p = |icon, suffix, offset, grav_pull, consume_range, dissipation, needs_room| StageParams {
        icon,
        suffix,
        offset,
        grav_pull,
        consume_range,
        dissipation,
        needs_room,
    };
    Some(match size {
        STAGE_ONE => p(
            "icons/obj/machines/engine/singularity.dmi",
            "s1",
            0,
            4,
            0,
            Some((10.0, 1.0)),
            None,
        ),
        STAGE_TWO => p(
            "icons/effects/96x96.dmi",
            "s3",
            -32,
            6,
            1,
            Some((5.0, 5.0)),
            Some(1),
        ),
        STAGE_THREE => p(
            "icons/effects/160x160.dmi",
            "s5",
            -64,
            8,
            2,
            Some((4.0, 20.0)),
            Some(2),
        ),
        STAGE_FOUR => p(
            "icons/effects/224x224.dmi",
            "s7",
            -96,
            10,
            3,
            Some((10.0, 10.0)),
            Some(3),
        ),
        STAGE_FIVE => p("icons/effects/288x288.dmi", "s9", -128, 10, 4, None, None),
        STAGE_SIX => p("icons/effects/352x352.dmi", "s11", -160, 15, 5, None, None),
        _ => return None,
    })
}

/// The component's `check_turfs_in` steps for a size (none past stage five).
fn edge_steps(size: u8) -> u32 {
    match size {
        STAGE_ONE => 1,
        STAGE_TWO => 2,
        STAGE_THREE => 3,
        STAGE_FOUR => 4,
        STAGE_FIVE => 5,
        _ => 0,
    }
}

pub struct SingularityPlugin;

impl Plugin for SingularityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (adopt_singularities, singularity_tick)
                .chain()
                .after(super::PowerSystems),
        );
    }
}

/// A new `/obj/singularity` or `/obj/reality_tear` (map, spawn) gets its state
/// (`Initialize`, `start_disaster`).
fn adopt_singularities(
    mut commands: Commands,
    time: Res<Time>,
    added: Query<(Entity, &DmObj), (Added<DmObj>, Without<Singularity>)>,
) {
    let now = time.elapsed_secs_f64();
    for (e, obj) in &added {
        let kind = if is_under(&obj.path, DARK_MATTER) {
            SingKind::DarkMatter
        } else if is_under(&obj.path, SINGULARITY) {
            SingKind::Gravity
        } else if is_under(&obj.path, REALITY_TEAR) {
            SingKind::Tear {
                temporary: is_under(&obj.path, REALITY_TEAR_TEMPORARY),
            }
        } else {
            continue;
        };
        commands.entity(e).insert(Singularity::new(kind, now));
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn singularity_tick(
    world: &mut World,
    mut bumps: Local<MessageCursor<Bumped>>,
    mut delams: Local<MessageCursor<Delaminated>>,
) {
    let delaminated: Vec<(TilePos, Delam)> = delams
        .read(world.resource::<Messages<Delaminated>>())
        .map(|d| (d.pos, d.kind))
        .collect();
    for (pos, kind) in delaminated {
        match kind {
            Delam::Singularity => {
                effect_singulo(world, pos);
            }
            Delam::Tesla => {
                super::tesla::effect_tesla(world, pos);
            }
            Delam::Cascade | Delam::Explosive => {}
        }
    }
    // `COMSIG_ATOM_BUMPED`: what walks into it is eaten.
    let bumped: Vec<(Entity, Entity)> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .map(|b| (b.entity, b.obstacle))
        .collect();
    for (mover, obstacle) in bumped {
        if world.get::<Singularity>(obstacle).is_some() {
            consume(world, obstacle, mover);
        }
    }
    let mut q = world.query_filtered::<Entity, With<Singularity>>();
    let all: Vec<Entity> = q.iter(world).collect();
    let t = now(world);
    for e in all {
        timers(world, e, t);
        let Some(s) = world.get::<Singularity>(e) else {
            continue;
        };
        if t >= s.next_eat {
            let roaming = s.roaming;
            world.get_mut::<Singularity>(e).expect("checked").next_eat = t + EAT_SECS;
            if roaming {
                roam(world, e);
            }
            eat(world, e);
        }
        let Some(s) = world.get::<Singularity>(e) else {
            continue;
        };
        if t >= s.next_act && !s.is_tear() {
            let secs = t - s.last_act;
            {
                let mut s = world.get_mut::<Singularity>(e).expect("checked");
                s.next_act = t + ACT_SECS;
                s.last_act = t;
            }
            process(world, e, secs);
        }
    }
}

/// A tear's collapse, a collapsing singularity's end, dark matter's `normalize_tracking`.
fn timers(world: &mut World, e: Entity, t: f64) {
    let (Some(s), Some(&pos)) = (
        world.get::<Singularity>(e).cloned(),
        world.get::<TilePos>(e),
    ) else {
        return;
    };
    if s.normalize_tracking_at.is_some_and(|at| t >= at) {
        let mut s = world.get_mut::<Singularity>(e).expect("checked");
        s.normalize_tracking_at = None;
        s.chance_to_move_to_target = CHANCE_TO_MOVE_TO_TARGET_BLOODTHIRSTY
            + if s.consumed_supermatter {
                DARK_MATTER_SUPERMATTER_CHANCE_BONUS
            } else {
                0.0
            };
    }
    if s.boh_sound_at.is_some_and(|at| t >= at) {
        world
            .get_mut::<Singularity>(e)
            .expect("checked")
            .boh_sound_at = None;
        play(
            world,
            pos,
            ss13_mapload::sound::files::SINGULARITY[0],
            200.0,
            Some(0.5),
        );
    }
    if !s.ends_at.is_some_and(|at| t >= at) {
        return;
    }
    // `reality_collapse`: the tear leaves a stage-two singularity of 400 energy.
    if s.kind == (SingKind::Tear { temporary: false }) {
        play(
            world,
            pos,
            ss13_mapload::sound::files::SINGULARITY[0],
            200.0,
            Some(0.5),
        );
        if let Some(bagulo) = spawn_singularity(world, SINGULARITY, pos) {
            expand(world, bagulo, Some(STAGE_TWO));
            if let Some(mut b) = world.get_mut::<Singularity>(bagulo) {
                b.energy = 400.0;
            }
        }
    }
    world.despawn(e);
}

/// `new path(pos)` with its state right away (not on the next frame).
pub fn spawn_singularity(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    let Some(e) = crate::construction::spawn_obj(world, path, pos) else {
        warn!("no prototype for {path}: no singularity at {pos:?}");
        return None;
    };
    let kind = if is_under(path, DARK_MATTER) {
        SingKind::DarkMatter
    } else if is_under(path, REALITY_TEAR) {
        SingKind::Tear {
            temporary: is_under(path, REALITY_TEAR_TEMPORARY),
        }
    } else {
        SingKind::Gravity
    };
    let t = now(world);
    world.entity_mut(e).insert(Singularity::new(kind, t));
    Some(e)
}

/// `/datum/sm_delam/proc/effect_singulo`: a singularity of 800 energy eats the crystal.
pub fn effect_singulo(world: &mut World, pos: TilePos) -> Option<Entity> {
    let e = spawn_singularity(world, SINGULARITY, pos)?;
    // `consume(sm)`: the crystal is already gone here (`Delaminated`), its gain and charge
    // still come.
    world.get_mut::<Singularity>(e).expect("just made").energy = 800.0 + SUPERMATTER_GAIN;
    supermatter_upgrade(world, e);
    Some(e)
}

/// `/obj/singularity/process`: an event, dissipation, then `check_energy`.
fn process(world: &mut World, e: Entity, secs: f64) {
    let Some(s) = world.get::<Singularity>(e) else {
        return;
    };
    let (size, chance) = (s.current_size, s.event_chance);
    if size >= STAGE_TWO && world.resource_mut::<EngineRng>().0.chance(chance) {
        event(world, e);
    }
    if let Some(mut s) = world.get_mut::<Singularity>(e) {
        s.dissipate(secs);
    }
    check_energy(world, e);
}

/// `check_energy`: collapses without energy, else grows or shrinks to its allowed size.
fn check_energy(world: &mut World, e: Entity) -> bool {
    let Some(s) = world.get::<Singularity>(e) else {
        return false;
    };
    if s.energy <= 0.0 {
        info!(target: "engine", "singularity {e} collapsed");
        world.despawn(e);
        return false;
    }
    let allowed = allowed_size(s.energy, s.consumed_supermatter).unwrap_or(s.allowed_size);
    let current = s.current_size;
    world
        .get_mut::<Singularity>(e)
        .expect("checked")
        .allowed_size = allowed;
    if current != allowed {
        expand(world, e, None);
    }
    true
}

/// `expand(force_size)`: tries the target stage, then each smaller one above the current
/// (tg `expand(--temp_allowed_size)`). True when it reached its allowed size.
// tg nulls the pull and consume ranges when no stage fits; the port keeps the current ones.
pub fn expand(world: &mut World, e: Entity, force: Option<u8>) -> bool {
    let Some(s) = world.get::<Singularity>(e) else {
        return false;
    };
    let mut temp = force.unwrap_or(s.allowed_size);
    if temp >= STAGE_SIX && !s.consumed_supermatter {
        temp = STAGE_FIVE;
    }
    temp = temp.min(s.maximum_stage);
    let current = s.current_size;
    let max_stage = s.maximum_stage;
    if temp == max_stage {
        world.get_mut::<Singularity>(e).expect("checked").dissipate = false;
    }
    let candidates: Vec<u8> = if temp > current {
        (current + 1..=temp).rev().filter(|s| s % 2 == 1).collect()
    } else {
        vec![temp]
    };
    for stage in candidates {
        let Some(params) = stage_params(stage) else {
            continue;
        };
        if let Some(steps) = params.needs_room
            && !check_cardinals_range(world, e, steps, true)
        {
            continue;
        }
        apply_stage(world, e, stage, params);
        break;
    }
    let Some(mut s) = world.get_mut::<Singularity>(e) else {
        return false;
    };
    s.roaming = s.move_self && s.current_size >= STAGE_TWO;
    s.disregard_failed_movements = s.current_size >= STAGE_FIVE;
    if s.current_size == s.allowed_size {
        info!(target: "engine", "singularity {e} grew to size {}", s.current_size);
        return true;
    }
    false
}

fn apply_stage(world: &mut World, e: Entity, stage: u8, params: StageParams) {
    let Some(mut s) = world.get_mut::<Singularity>(e) else {
        return;
    };
    s.current_size = stage;
    s.grav_pull = params.grav_pull;
    s.consume_range = params.consume_range;
    match params.dissipation {
        Some((delay, strength)) => {
            s.dissipate_delay = delay;
            s.dissipate_strength = strength;
            s.since_dissipation = 0.0;
        }
        None => s.dissipate = false,
    }
    let variant = if s.kind == SingKind::DarkMatter {
        "dark_matter"
    } else {
        "singularity"
    };
    let state = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(params.icon, &format!("{variant}_{}", params.suffix)));
    if let Some(mut appearance) = world.get_mut::<Appearance>(e) {
        if let Some(state) = state {
            appearance.icon_state = state;
        }
        appearance.pixel_offset = [params.offset, params.offset];
    }
}

/// `supermatter_upgrade`: stage six becomes possible (dark matter: it hunts harder), the light
/// grows to 10.
fn supermatter_upgrade(world: &mut World, e: Entity) {
    let Some(mut s) = world.get_mut::<Singularity>(e) else {
        return;
    };
    if s.consumed_supermatter || s.is_tear() {
        return;
    }
    s.consumed_supermatter = true;
    if s.kind == SingKind::DarkMatter {
        s.chance_to_move_to_target += DARK_MATTER_SUPERMATTER_CHANCE_BONUS;
        return;
    }
    if let Some(mut optics) = world.get_mut::<Optics>(e)
        && let Some(light) = optics.light.as_mut()
    {
        light.range = 10.0;
    }
}

/// `COMSIG_ATOM_SINGULARITY_TRY_MOVE` blockers and `TRAIT_CONTAINMENT_FIELD` givers on `pos`:
/// a containment field, a field generator that is on, a shield wall, a wall generator that is
/// on.
pub fn blocks_singularity(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world.get::<ContainmentField>(e).is_some()
                || world.get::<ShieldWall>(e).is_some()
                || world
                    .get::<FieldGenerator>(e)
                    .is_some_and(|g| g.active != FgActive::Offline)
                || world.get::<ShieldWallGen>(e).is_some_and(ShieldWallGen::on)
        })
}

/// `check_turfs_in(direction, steps)`: the edge `steps` tiles out towards `dir` (its far row,
/// `steps - 1` tiles to each side; a diagonal checks its far corner) is on the map and free of
/// fields.
pub fn edge_clear(world: &World, pos: TilePos, dir: Dir, steps: u32) -> bool {
    let (dx, dy) = dir.offset();
    let steps = steps as i32;
    let far = TilePos::new(pos.x + dx * steps, pos.y + dy * steps, pos.z);
    // `get_step(turf, 0)` is the turf itself: a diagonal's side rows collapse onto its corner.
    let (px, py) = match dir {
        Dir::North | Dir::South => (1, 0),
        Dir::East | Dir::West => (0, 1),
        _ => (0, 0),
    };
    let index = world.resource::<SpatialIndex>();
    let mut tiles = vec![far];
    for j in 1..steps {
        tiles.push(TilePos::new(far.x + px * j, far.y + py * j, far.z));
        tiles.push(TilePos::new(far.x - px * j, far.y - py * j, far.z));
    }
    tiles
        .into_iter()
        .all(|t| index.in_bounds(t) && !blocks_singularity(world, t))
}

/// `check_cardinals_range(steps, retry_with_move)`.
fn check_cardinals_range(world: &mut World, e: Entity, steps: u32, retry: bool) -> bool {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return false;
    };
    if Dir::CARDINALS
        .iter()
        .all(|&d| edge_clear(world, pos, d, steps))
    {
        return true;
    }
    if retry {
        for d in Dir::CARDINALS {
            if step(world, e, d) && check_cardinals_range(world, e, steps, false) {
                return true;
            }
        }
    }
    false
}

/// `step(parent, dir)` with the component's `moved()`: what is ahead is eaten, then the step
/// is refused when the leading edge meets a field (`last_failed_movement`).
fn step(world: &mut World, e: Entity, dir: Dir) -> bool {
    let (Some(s), Some(&pos)) = (world.get::<Singularity>(e), world.get::<TilePos>(e)) else {
        return false;
    };
    let (size, disregard) = (s.current_size, s.disregard_failed_movements);
    let mut ahead = pos;
    for c in Dir::CARDINALS {
        if dir as u8 & c as u8 == 0 {
            continue;
        }
        let (dx, dy) = c.offset();
        ahead = TilePos::new(ahead.x + dx, ahead.y + dy, ahead.z);
        if !world.resource::<SpatialIndex>().in_bounds(ahead) {
            break;
        }
        consume_tile(world, e, ahead);
    }
    let clear = disregard || edge_clear(world, pos, dir, edge_steps(size));
    let (dx, dy) = dir.offset();
    let next = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
    let on_map = world.resource::<SpatialIndex>().in_bounds(next);
    {
        let Some(mut s) = world.get_mut::<Singularity>(e) else {
            return false;
        };
        s.last_failed_movement = (!clear).then_some(dir);
    }
    if !clear || !on_map {
        return false;
    }
    world.entity_mut(e).insert(next);
    true
}

/// The component's `move()` (bloodthirsty: after picking its prey).
fn roam(world: &mut World, e: Entity) {
    let Some(s) = world.get::<Singularity>(e).cloned() else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let mut target = s.target.filter(|&t| world.get::<TilePos>(t).is_some());
    if s.kind == SingKind::DarkMatter {
        if let Some(t) = target {
            if world.get::<TilePos>(t).is_some_and(|p| p.z != pos.z) {
                target = None;
            }
            let closer = nearest_living(world, pos);
            if closer != Some(t) && world.resource_mut::<EngineRng>().0.chance(20.0) {
                target = closer;
            }
        }
        if target.is_none() {
            target = nearest_living(world, pos);
        }
        world.get_mut::<Singularity>(e).expect("checked").target = target;
    }
    let dirs: Vec<Dir> = ALLDIRS
        .into_iter()
        .filter(|&d| Some(d) != s.last_failed_movement)
        .collect();
    let (mut dir, chase) = {
        let mut rng = world.resource_mut::<EngineRng>();
        let dir = Some(dirs[rng.0.range(0, dirs.len() as u32 - 1) as usize]);
        (dir, rng.0.chance(s.chance_to_move_to_target))
    };
    if let Some(t) = target
        && chase
        && let Some(&tp) = world.get::<TilePos>(t)
    {
        dir = crate::mecha::core::dir_between(pos, tp);
    }
    if let Some(dir) = dir {
        step(world, e, dir);
    }
}

/// `find_new_target`: the closest living mob on its level.
fn nearest_living(world: &mut World, pos: TilePos) -> Option<Entity> {
    let mut q = world.query_filtered::<(Entity, &TilePos), With<Health>>();
    q.iter(world)
        .filter_map(|(e, &p)| p.chebyshev(pos).map(|d| (d, e)))
        .min()
        .map(|(_, e)| e)
}

/// `eat()` + `digest()`: every tile within `grav_pull`, nearest first; within
/// `consume_range` it is eaten, beyond it pulled.
fn eat(world: &mut World, e: Entity) {
    let (Some(s), Some(&center)) = (world.get::<Singularity>(e), world.get::<TilePos>(e)) else {
        return;
    };
    let (grav, consume_range) = (s.grav_pull as i32, s.consume_range as i32);
    for d in 0..=grav {
        for dy in -d..=d {
            for dx in -d..=d {
                if dx.abs().max(dy.abs()) != d {
                    continue;
                }
                let tile = TilePos::new(center.x + dx, center.y + dy, center.z);
                if !world.resource::<SpatialIndex>().in_bounds(tile) {
                    continue;
                }
                if world.get_entity(e).is_err() {
                    return;
                }
                if d <= consume_range {
                    consume_tile(world, e, tile);
                } else {
                    pull_tile(world, e, tile, center);
                }
            }
        }
    }
}

/// Everything on `tile`, then the turf.
fn consume_tile(world: &mut World, e: Entity, tile: TilePos) {
    for thing in crate::explosions::ex_act::tile_contents(world, tile) {
        if thing != e && world.get::<crate::inventory::CarriedBy>(thing).is_none() {
            consume(world, e, thing);
        }
    }
    let gain = turf_act(world, tile);
    if let Some(mut s) = world.get_mut::<Singularity>(e) {
        s.energy += gain;
    }
}

/// The supermatter's `singularity_act` gain.
const SUPERMATTER_GAIN: f64 = 100.0;

/// `consume(thing)`: a bag of holding collapses it, anything else gives its
/// `singularity_act` energy; a crystal charges it.
pub fn consume(world: &mut World, e: Entity, thing: Entity) {
    let Some(s) = world.get::<Singularity>(e) else {
        return;
    };
    if thing == e || world.get_entity(thing).is_err() {
        return;
    }
    let boh = path_of(world, thing).is_some_and(|p| is_under(&p, BAG_OF_HOLDING));
    if boh && !s.is_tear() && !s.consumed_supermatter && !s.collapsing {
        consume_boh(world, e, thing);
        return;
    }
    let crystal = world.get::<Supermatter>(thing).is_some();
    let gain = singularity_act(world, thing);
    if let Some(mut s) = world.get_mut::<Singularity>(e) {
        s.energy += gain;
    }
    if crystal {
        supermatter_upgrade(world, e);
    }
}

/// `consume_boh`: it collapses on itself in 4.1 s.
fn consume_boh(world: &mut World, e: Entity, boh: Entity) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let t = now(world);
    if let Some(mut s) = world.get_mut::<Singularity>(e) {
        s.collapsing = true;
        s.ends_at = Some(t + 4.1);
        s.boh_sound_at = Some(t + 4.0);
    }
    let (name, bag) = (
        crate::security::name_of(world, e),
        crate::security::name_of(world, boh),
    );
    crate::security::show(
        world,
        pos,
        15,
        ChatChannel::Local,
        (
            Span::Danger,
            format!("Поглотив {bag}, {name} начинает схлопываться!"),
        ),
        &[],
    );
    play(
        world,
        pos,
        ss13_mapload::sound::files::SINGULARITY[1],
        200.0,
        Some(-1.0),
    );
    qdel(world, boh);
}

/// `singularity_act()` of whatever `thing` is: the energy it gives.
fn singularity_act(world: &mut World, thing: Entity) -> f64 {
    if world.get::<crate::ghost::Ghost>(thing).is_some() {
        return 0.0;
    }
    if let Some(other) = world.get::<Singularity>(thing).cloned()
        && !other.is_tear()
    {
        // `/obj/singularity/singularity_act`: half its energy and a blast by its size.
        let dist = (i32::from(other.current_size) - 2).max(1);
        if let Some(&at) = world.get::<TilePos>(thing) {
            info!(target: "engine", "singularity {thing} destroyed by another singularity");
            qdel(world, thing);
            crate::explosions::explosion(
                world,
                at,
                crate::explosions::Explosion::new(dist, dist * 2, dist * 4),
            );
        }
        return other.energy / 2.0;
    }
    if world.get::<Supermatter>(thing).is_some() {
        return supermatter_consumed(world, thing);
    }
    if world.get::<Health>(thing).is_some() {
        return living_act(world, thing);
    }
    let Some(path) = path_of(world, thing) else {
        return 0.0;
    };
    if UNTOUCHABLE.iter().any(|r| is_under(&path, r))
        || world
            .get::<crate::events::immovable_rod::ImmovableRod>(thing)
            .is_some()
    {
        return 0.0;
    }
    qdel(world, thing);
    // `/obj/effect/singularity_act` returns nothing.
    if is_under(&path, "/obj/effect") {
        0.0
    } else {
        2.0
    }
}

/// `/mob/living/singularity_act` (gibbed, 20) and the human's role values.
fn living_act(world: &mut World, mob: Entity) -> f64 {
    let human = world.get::<crate::medical::wounds::Limbs>(mob).is_some()
        && world.get::<crate::silicon::Silicon>(mob).is_none();
    let title = world
        .get::<crate::jobs::PlayerJob>(mob)
        .and_then(|j| {
            let jobs = world.get_resource::<crate::jobs::Jobs>()?;
            Some(jobs.data.jobs.get(j.0)?.title.clone())
        })
        .filter(|_| human);
    let gain = match title.as_deref() {
        Some("Chief Engineer" | "Station Engineer") => 100.0,
        Some("Clown") => f64::from(world.resource_mut::<EngineRng>().0.range(0, 2000)) - 1000.0,
        _ => 20.0,
    };
    info!(target: "engine", "{mob} was consumed by a singularity");
    crate::health::gib(world, mob);
    gain
}

/// `/obj/machinery/power/supermatter_crystal/singularity_act`.
fn supermatter_consumed(world: &mut World, crystal: Entity) -> f64 {
    let Some(&pos) = world.get::<TilePos>(crystal) else {
        return 0.0;
    };
    let name = crate::security::name_of(world, crystal);
    warn!(target: "engine", "a singularity consumed {name} at {pos:?} and can reach stage six");
    crate::security::show(
        world,
        pos,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ChatChannel::Local,
        (
            Span::Danger,
            format!(
                "{} поглощается сингулярностью!",
                ss13_shared::capitalize(&name)
            ),
        ),
        &[],
    );
    let clients = crate::construction::clients_near(world, pos, 255, &[]);
    for client in clients {
        crate::tools::tell(
            world,
            client,
            Span::Danger,
            "Ужасающий визг наполняет ваши уши, и вас накрывает волна ужаса...",
        );
    }
    // shortcut: `SEND_SOUND` to the players of its level; here to every player.
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::to_playing_players(
            ss13_mapload::sound::files::SINGULARITY[0],
            100.0,
            false,
        ),
    );
    qdel(world, crystal);
    SUPERMATTER_GAIN
}

/// `/turf/singularity_act`: scraped one layer (`ScrapeAway(CHANGETURF_INHERIT_AIR)`), 2.
fn turf_act(world: &mut World, pos: TilePos) -> f64 {
    let Some(path) = turf_at(world, pos).and_then(|t| path_of(world, t)) else {
        return 0.0;
    };
    if UNEATABLE_TURFS.iter().any(|r| is_under(&path, r)) {
        return 0.0;
    }
    if crate::construction::scrape_away(world, pos, true).is_none() {
        return 0.0;
    }
    2.0
}

/// tg `qdel` of an eaten object: off the power grid, its shields and fields cleaned up, what
/// it holds gone with it.
pub(crate) fn qdel(world: &mut World, thing: Entity) {
    let Ok(entity) = world.get_entity(thing) else {
        return;
    };
    let pos = entity.get::<TilePos>().copied();
    if world.get::<FieldGenerator>(thing).is_some() {
        super::field_generator::turn_off(world, thing);
    }
    if let Some(field) = world.get::<ContainmentField>(thing).copied() {
        for g in field.gens {
            if let Some(mut fg) = world.get_mut::<FieldGenerator>(g) {
                fg.fields.retain(|&f| f != thing);
            }
        }
    }
    {
        let mut grid = world.resource_mut::<super::Grid>();
        grid.remove_node(thing);
        grid.remove_cable(thing);
    }
    super::shields::on_destroy(world, thing);
    crate::eng_machines::atmos_shield::on_destroy(world, thing);
    crate::eng_machines::modular_shield::on_destroy(world, thing);
    let mut held = crate::explosions::ex_act::contents_of(world, thing);
    while let Some(item) = held.pop() {
        if held.len() > 10_000 {
            break;
        }
        held.extend(crate::explosions::ex_act::contents_of(world, item));
        if let Ok(item) = world.get_entity_mut(item) {
            item.despawn();
        }
    }
    if let Ok(entity) = world.get_entity_mut(thing) {
        entity.despawn();
    }
    if let Some(pos) = pos {
        crate::construction::tile_changed(world, pos);
    }
}

/// The turf's `singularity_pull`, then every movable's on `tile`.
fn pull_tile(world: &mut World, e: Entity, tile: TilePos, center: TilePos) {
    let Some(size) = world.get::<Singularity>(e).map(|s| s.current_size) else {
        return;
    };
    if let Some(turf) = turf_at(world, tile)
        && let Some(path) = path_of(world, turf)
    {
        if is_under(&path, "/turf/open/floor") {
            // `/turf/open/floor/singularity_pull`: the tile sheers off.
            let chance = match size {
                STAGE_THREE => 30.0,
                STAGE_FOUR => 50.0,
                s if s >= STAGE_FIVE => 70.0,
                _ => 0.0,
            };
            let floor_tile = world
                .get::<DmTurfBuild>(turf)
                .map(|b| b.floor_tile.clone())
                .unwrap_or_default();
            if !floor_tile.is_empty()
                && chance > 0.0
                && world.resource_mut::<EngineRng>().0.chance(chance)
                && crate::construction::scrape_away(world, tile, true).is_some()
            {
                crate::construction::spawn_stack(world, &floor_tile, tile, 1);
            }
        } else if is_under(&path, "/turf/closed/wall") {
            // `wall_singularity_pull`.
            let chance = match size {
                s if s >= STAGE_FIVE => 50.0,
                STAGE_FOUR => 30.0,
                _ => 0.0,
            };
            if chance > 0.0 && world.resource_mut::<EngineRng>().0.chance(chance) {
                crate::construction::dismantle_wall(world, turf, tile);
            }
        }
    }
    for thing in crate::explosions::ex_act::tile_contents(world, tile) {
        if thing != e && world.get::<crate::inventory::CarriedBy>(thing).is_none() {
            pull(world, e, thing, center, size);
        }
    }
}

/// `singularity_pull` of a movable: a step closer (into the singularity: eaten, tg `Bump`),
/// items thrown from stage four, mobs from stage six; pipes and cables come apart at five.
fn pull(world: &mut World, e: Entity, thing: Entity, center: TilePos, size: u8) {
    if world.get_entity(thing).is_err()
        || world.get::<crate::ghost::Ghost>(thing).is_some()
        || world.get::<Singularity>(thing).is_some()
        || world.get::<super::tesla::TeslaBall>(thing).is_some()
    {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(thing) else {
        return;
    };
    let next = TilePos::new(
        pos.x + (center.x - pos.x).signum(),
        pos.y + (center.y - pos.y).signum(),
        pos.z,
    );
    if world.get::<Health>(thing).is_some() {
        if size >= STAGE_SIX {
            crate::throw::throw_at(world, thing, center, PULL_THROW_RANGE, PULL_THROW_SPEED);
        } else if world
            .get::<crate::modsuit::modules::NoSlip>(thing)
            .is_none()
        {
            step_closer(world, e, thing, next, center);
        }
        return;
    }
    let Some(path) = path_of(world, thing) else {
        return;
    };
    if UNTOUCHABLE.iter().any(|r| is_under(&path, r)) || is_under(&path, "/turf") {
        return;
    }
    if size >= STAGE_FIVE
        && let Some(&(_, disassembled)) = PULLED_APART.iter().find(|(r, _)| is_under(&path, r))
    {
        crate::obj_damage::deconstruct(world, thing, disassembled);
        return;
    }
    if !crate::events::anomalies::anchored(world, thing) || size >= STAGE_FIVE {
        step_closer(world, e, thing, next, center);
    }
    if size >= STAGE_FOUR
        && world.get::<DmItem>(thing).is_some()
        && world.get::<TilePos>(thing).is_some()
    {
        crate::throw::throw_at(world, thing, center, PULL_THROW_RANGE, PULL_THROW_SPEED);
    }
}

fn step_closer(world: &mut World, e: Entity, thing: Entity, next: TilePos, center: TilePos) {
    if next == center {
        consume(world, e, thing);
    } else {
        crate::events::anomalies::step_towards(world, thing, center);
    }
}

/// `event()`: `rand(1, 4)`: an EMP, `mezzer`, or (stage six) `combust_mobs`.
fn event(world: &mut World, e: Entity) {
    let (Some(s), Some(&pos)) = (world.get::<Singularity>(e), world.get::<TilePos>(e)) else {
        return;
    };
    let size = s.current_size;
    match world.resource_mut::<EngineRng>().0.range(1, 4) {
        1 => crate::emp::empulse(
            world,
            crate::emp::Empulse {
                epicenter: pos,
                heavy: 8,
                light: 10,
            },
        ),
        2 => mezzer(world, e, pos),
        _ if size >= STAGE_SIX => combust_mobs(world, pos),
        _ => {}
    }
}

/// tg `iscarbon`.
fn is_carbon(world: &World, mob: Entity) -> bool {
    world.get::<Health>(mob).is_some()
        && world.get::<crate::inventory::Inventory>(mob).is_some()
        && world.get::<crate::silicon::Silicon>(mob).is_none()
        && world.get::<crate::mobs::BasicMob>(mob).is_none()
}

fn carbons_near(world: &World, pos: TilePos, range: u32) -> Vec<(Entity, TilePos)> {
    world
        .resource::<SpatialIndex>()
        .in_radius(pos, range)
        .filter(|&m| is_carbon(world, m))
        .filter_map(|m| Some((m, *world.get::<TilePos>(m)?)))
        .collect()
}

/// `mezzer`: the carbons within 8 that see it, alive and not blind, are stunned 6 s unless
/// they wear mesons.
fn mezzer(world: &mut World, e: Entity, pos: TilePos) {
    let name = crate::security::name_of(world, e);
    for (mob, at) in carbons_near(world, pos, 8) {
        if world.get::<Stat>(mob) == Some(&Stat::Dead)
            || crate::vision::is_blind(world, mob)
            || !crate::mobs::ai::behaviors::can_see(world, at, pos, 8)
        {
            continue;
        }
        if crate::clothing::traits::has_trait(world, mob, crate::clothing::traits::MESON_VISION) {
            crate::tools::tell_mob(
                world,
                mob,
                Span::Notice,
                format!("Вы смотрите прямо в {name}. Хорошо, что ваше зрение защищено!"),
            );
            continue;
        }
        crate::status_effects::apply(
            world,
            mob,
            crate::status_effects::Incap::Stun,
            MEZZER_STUN_SECS,
        );
        let victim = crate::security::name_of(world, mob);
        crate::security::show(
            world,
            at,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (
                Span::Danger,
                format!("{victim} безучастно смотрит на {name}!"),
            ),
            &[(
                mob,
                Span::Danger,
                format!("Вы смотрите прямо в {name} и чувствуете слабость."),
            )],
        );
    }
}

/// `combust_mobs`: every carbon within 20 tiles catches fire (5 fire stacks).
fn combust_mobs(world: &mut World, pos: TilePos) {
    for (mob, at) in carbons_near(world, pos, 20) {
        let victim = crate::security::name_of(world, mob);
        crate::security::show(
            world,
            at,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (Span::Warning, format!("Кожа {victim} вспыхивает пламенем!")),
            &[(
                mob,
                Span::Danger,
                "Вы чувствуете внутренний огонь: ваша кожа вспыхивает пламенем!".to_owned(),
            )],
        );
        crate::fire::adjust_fire_stacks(world, mob, 5.0);
        crate::fire::ignite_mob(world, mob);
    }
}

/// `ex_act` of a singularity (true: handled, whatever the outcome).
pub fn ex_act(world: &mut World, thing: Entity, severity: Severity) -> bool {
    let t = now(world);
    let destroyed = {
        let Some(mut s) = world.get_mut::<Singularity>(thing) else {
            return false;
        };
        s.ex_act(severity, t)
    };
    if destroyed {
        info!(target: "engine", "singularity {thing} destroyed by a heavy explosion");
        world.despawn(thing);
        return true;
    }
    check_energy(world, thing);
    true
}

/// `playsound(loc, sound, vol, vary = TRUE, extrarange = 3, falloff_exponent = 1, frequency,
/// pressure_affected = FALSE, ignore_walls = TRUE, falloff_distance = 7)` of the collapses.
fn play(world: &mut World, pos: TilePos, sound: &str, vol: f32, frequency: Option<f32>) {
    let mut s = crate::sound::PlaySound::new(pos, sound, vol, true)
        .extrarange(3.0)
        .falloff_exponent(1.0)
        .falloff_distance(7.0);
    s.pressure_affected = false;
    if let Some(f) = frequency {
        s = s.frequency(f);
    }
    crate::sound::playsound(world, s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_follow_energy() {
        assert_eq!(allowed_size(0.5, false), None);
        assert_eq!(allowed_size(1.0, false), Some(STAGE_ONE));
        assert_eq!(allowed_size(200.0, false), Some(STAGE_ONE));
        assert_eq!(allowed_size(201.0, false), Some(STAGE_TWO));
        assert_eq!(allowed_size(500.0, false), Some(STAGE_TWO));
        assert_eq!(allowed_size(900.0, false), Some(STAGE_THREE));
        assert_eq!(allowed_size(1500.0, false), Some(STAGE_FOUR));
        assert_eq!(allowed_size(2500.0, false), Some(STAGE_FIVE));
        assert_eq!(allowed_size(4000.0, false), Some(STAGE_FIVE));
        assert_eq!(allowed_size(4000.0, true), Some(STAGE_SIX));
        let two = stage_params(STAGE_TWO).unwrap();
        assert_eq!(
            (two.grav_pull, two.consume_range, two.needs_room),
            (6, 1, Some(1))
        );
        assert_eq!(stage_params(STAGE_SIX).unwrap().grav_pull, 15);
        assert_eq!(stage_params(2), None);
    }

    #[test]
    fn dissipation_counts_real_seconds() {
        let mut s = Singularity::new(SingKind::Gravity, 0.0);
        s.dissipate_delay = 10.0;
        s.dissipate_strength = 1.0;
        s.dissipate(9.0);
        assert_eq!(s.energy, 50.0);
        s.dissipate(2.0);
        assert_eq!(s.energy, 49.0);
        // A long gap takes every step it covered.
        s.dissipate(30.0);
        assert_eq!(s.energy, 46.0);
        s.dissipate = false;
        s.dissipate(100.0);
        assert_eq!(s.energy, 46.0);
    }

    #[test]
    fn explosions_cut_energy_and_kill_small_ones() {
        let mut s = Singularity::new(SingKind::Gravity, 0.0);
        s.energy = 99.0;
        s.current_size = STAGE_THREE;
        assert!(!s.ex_act(Severity::Devastate, 0.0));
        assert_eq!(s.energy, 49.0);
        assert!(!s.ex_act(Severity::Heavy, 0.0));
        assert_eq!(s.energy, 32.0);
        assert!(!s.ex_act(Severity::Light, 0.0));
        assert_eq!(s.energy, 24.0);
        s.current_size = STAGE_TWO;
        assert!(s.ex_act(Severity::Devastate, 0.0));
        // Fresh dark matter shrugs it off for 5 s.
        let mut dark = Singularity::new(SingKind::DarkMatter, 10.0);
        assert!(!dark.ex_act(Severity::Devastate, 12.0));
        assert_eq!(dark.energy, 250.0);
        assert!(dark.ex_act(Severity::Devastate, 15.0));
    }

    #[test]
    fn kinds_start_like_tg() {
        let dark = Singularity::new(SingKind::DarkMatter, 0.0);
        assert_eq!(
            (dark.maximum_stage, dark.chance_to_move_to_target),
            (STAGE_FOUR, 100.0)
        );
        let tear = Singularity::new(SingKind::Tear { temporary: false }, 0.0);
        assert_eq!(
            (tear.grav_pull, tear.consume_range, tear.current_size),
            (21, 1, STAGE_SIX)
        );
        assert_eq!(tear.ends_at, Some(9.0));
        let small = Singularity::new(SingKind::Tear { temporary: true }, 0.0);
        assert_eq!((small.grav_pull, small.consume_range), (3, 2));
        assert!(!small.roaming && !tear.move_self);
    }
}
