//! Emitters and reflectors (server): tg `code/modules/power/singularity/emitter.dm`,
//! `code/game/objects/structures/reflector.dm` and the emitter's hitscan beam
//! (`/obj/projectile/beam/emitter/hitscan`, `code/modules/projectiles/projectile/beams.dm`).
//!
//! - An [`Emitter`] runs in the machine tick's early phase (tg `processing_early`, before the
//!   APCs): while on and welded it draws 300 W from its powernet and fires when its delay is
//!   up: three shots 2 s apart, then a random 2-10 s pause. With too little surplus it idles
//!   ("low power").
//! - Shots go into [`PendingBeams`]; [`fire_beams`] traces them after the tick. The beam is a
//!   hitscan: it flies tile by tile through glass, grilles, tables and glass doors, turns at
//!   reflectors and stops at the first other dense thing. A supermatter it hits gains
//!   `30 damage * 2` MeV (`eat_bullets`), a mob takes 30 burn.
//! - Empty-hand click toggles it (welded and wired only); wrench anchors, welder welds
//!   (`welder_act`, 2 s, 1 fuel), as tg.
//!
//! Cost: a beam is at most 50 tiles per straight run, 10 runs; MetaStation fires about one
//! beam per second with all three engine emitters on.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmObj, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, EMISSIVE_PLANE, IconStateId, Overlay, TilePos};
use ss13_world::{Density, SpatialIndex};

use super::{Grid, NodeKind, Powernet};
use crate::click::ClickResolved;
use crate::combat::CombatRng;
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat, apply_damage_zone};
use crate::obj_damage::{Anchored, set_anchored};
use crate::tools::{Actor, ToolKind, tell, use_tool};
use crate::zone::BodyZone;

pub const EMITTER_ROOT: &str = "/obj/machinery/power/emitter";
pub const EMITTER_WELDED: &str = "/obj/machinery/power/emitter/welded";
pub const REFLECTOR_ROOT: &str = "/obj/structure/reflector";

/// `RefreshParts` with the board's tier 1 micro-laser and servo: 350 - 50 W.
pub const EMITTER_POWER_USAGE: f64 = 300.0;
/// `maximum_fire_delay`, `minimum_fire_delay` with a tier 1 micro-laser, seconds.
pub const MAX_FIRE_DELAY: f64 = 10.0;
pub const MIN_FIRE_DELAY: f64 = 2.0;
/// The delay between the first shots of a burst (`fire_delay = 20`).
pub const BURST_DELAY: f64 = 2.0;
/// `/obj/projectile/beam/emitter` `damage`.
pub const BEAM_DAMAGE: f32 = 30.0;
/// `/obj/projectile` `range`.
pub const BEAM_RANGE: u32 = 50;
/// Straight runs a beam may make (reflections + 1); tg decrements `maximum_range` per reflection.
const MAX_RUNS: u32 = 10;
/// `COLOR_VIBRANT_LIME`, `COLOR_ORANGE` (low power) of the `emitter_overlay`.
const LASER_COLOR: [u8; 4] = [0x00, 0xFF, 0x00, 0xFF];
const LOW_POWER_COLOR: [u8; 4] = [0xFF, 0x80, 0x00, 0xFF];

/// tg `/obj/machinery/power/emitter`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Emitter {
    pub active: bool,
    /// The last tick had the power to run.
    pub powered: bool,
    pub welded: bool,
    pub locked: bool,
    /// Seconds.
    pub fire_delay: f64,
    pub last_shot: f64,
    pub shot_number: u8,
    pub dir: Dir,
    pub layer: u8,
    /// `emitter_overlay`.
    overlay: Option<IconStateId>,
}

impl Emitter {
    pub fn new(dir: Dir, welded: bool, layer: u8) -> Self {
        Self {
            active: false,
            powered: false,
            welded,
            locked: false,
            fire_delay: MAX_FIRE_DELAY,
            last_shot: f64::NEG_INFINITY,
            shot_number: 0,
            dir,
            layer,
            overlay: None,
        }
    }

    /// `process_early`: draw power, and whether it fires now.
    pub fn process_early(
        &mut self,
        net: Option<&mut Powernet>,
        now: f64,
        dt: f64,
        rng: &mut CombatRng,
    ) -> bool {
        let usage = EMITTER_POWER_USAGE * dt;
        let Some(net) = net.filter(|_| self.welded) else {
            self.active = false;
            return false;
        };
        if !self.active {
            return false;
        }
        if net.surplus() < usage {
            self.powered = false;
            return false;
        }
        net.load += usage;
        self.powered = true;
        if self.last_shot + self.fire_delay > now {
            return false;
        }
        // `fire_beam`.
        self.last_shot = now;
        if self.shot_number < 3 {
            self.fire_delay = BURST_DELAY;
            self.shot_number += 1;
        } else {
            // DM `rand(20, 100)` deciseconds.
            let ds = rng.range(
                (MIN_FIRE_DELAY * 10.0) as u32,
                (MAX_FIRE_DELAY * 10.0) as u32,
            );
            self.fire_delay = f64::from(ds) / 10.0;
            self.shot_number = 0;
        }
        true
    }

    /// `interact`: on/off. Returns the message for the user.
    pub fn toggle(&mut self, wired: bool) -> (Span, &'static str) {
        if !self.welded {
            return (
                Span::Warning,
                "The emitter needs to be firmly secured to the floor first!",
            );
        }
        if !wired {
            return (Span::Warning, "The emitter isn't connected to a wire!");
        }
        if self.locked {
            return (Span::Warning, "The controls are locked!");
        }
        self.active = !self.active;
        if self.active {
            self.shot_number = 0;
            self.fire_delay = MAX_FIRE_DELAY;
            (Span::Notice, "You turn on the emitter.")
        } else {
            (Span::Notice, "You turn off the emitter.")
        }
    }
}

/// tg reflector kinds (`auto_reflect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReflectorKind {
    /// One mirror face: reflects beams hitting its front, stops the rest.
    Single,
    Double,
    /// Sends every beam out at its angle.
    Box,
}

/// tg `/obj/structure/reflector` (finished ones).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Reflector {
    pub kind: ReflectorKind,
    /// `rotation_angle`, degrees clockwise from north.
    pub angle: f64,
}

impl Reflector {
    /// The beam's new angle, `None` when it is absorbed (`auto_reflect` returns FALSE).
    pub fn reflect(self, beam_angle: f64) -> Option<f64> {
        let incidence = modulus(self.angle - (beam_angle + 180.0), 360.0);
        match self.kind {
            ReflectorKind::Box => Some(self.angle),
            ReflectorKind::Single if incidence.abs() > 90.0 && incidence.abs() < 270.0 => None,
            ReflectorKind::Single | ReflectorKind::Double => {
                Some(modulus(self.angle + incidence, 360.0))
            }
        }
    }
}

/// DM `MODULUS(x, y)`: `x - floor(x / y) * y`.
fn modulus(x: f64, y: f64) -> f64 {
    x - (x / y).floor() * y
}

/// `dir2angle`.
pub fn dir2angle(dir: Dir) -> f64 {
    match dir {
        Dir::North => 0.0,
        Dir::NorthEast => 45.0,
        Dir::East => 90.0,
        Dir::SouthEast => 135.0,
        Dir::South => 180.0,
        Dir::SouthWest => 225.0,
        Dir::West => 270.0,
        Dir::NorthWest => 315.0,
    }
}

/// The tile step of a beam angle; `None` off the eight directions.
fn step_of(angle: f64) -> Option<(i32, i32)> {
    let a = modulus(angle.round(), 360.0) as i32;
    Some(match a {
        0 => (0, 1),
        45 => (1, 1),
        90 => (1, 0),
        135 => (1, -1),
        180 => (0, -1),
        225 => (-1, -1),
        270 => (-1, 0),
        315 => (-1, 1),
        _ => return None,
    })
}

/// Shots fired this machine tick: emitter, its tile, beam angle.
#[derive(Resource, Default, Debug)]
pub struct PendingBeams(pub Vec<(Entity, TilePos, f64)>);

/// Where a beam ended.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct BeamHit {
    pub emitter: Entity,
    pub target: Option<Entity>,
    pub pos: TilePos,
}

pub struct EmitterServerPlugin;

impl Plugin for EmitterServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingBeams>()
            .add_message::<BeamHit>()
            .add_systems(Startup, init_emitters.after(super::init_power))
            .add_systems(
                Update,
                (
                    fire_beams.after(super::PowerSystems),
                    toggle_on_click
                        .after(ss13_world::WorldSystems::Movement)
                        .after(crate::click::ResolveClicks),
                    emitter_looks.after(super::PowerSystems),
                ),
            );
    }
}

fn init_emitters(world: &mut World) {
    let index = world.remove_resource::<IconIndex>();
    let mut query = world.query::<(Entity, &super::DmPower, &TilePos, Option<&DmAtom>)>();
    let found: Vec<(Entity, String, TilePos, u8, Option<String>)> = query
        .iter(world)
        .filter(|(_, dm, ..)| is_under(&dm.path, EMITTER_ROOT))
        .map(|(e, dm, &pos, atom)| {
            (
                e,
                dm.path.clone(),
                pos,
                atom.map_or(2, |a| a.dir),
                atom.map(|a| a.icon.clone()),
            )
        })
        .collect();
    for (e, path, pos, dir, icon) in found {
        let welded = is_under(&path, EMITTER_WELDED);
        let layer = world.get::<super::DmPower>(e).map_or(2, super::layer);
        let mut emitter = Emitter::new(Dir::from_bits(dir).unwrap_or_default(), welded, layer);
        emitter.overlay = icon.and_then(|icon| index.as_ref()?.get(&icon, "emitter_overlay"));
        if welded {
            // `/obj/machinery/power/emitter/welded/Initialize`: anchored, `connect_to_network`.
            set_anchored(world, e, true);
            world
                .resource_mut::<Grid>()
                .add_node(e, pos, layer, NodeKind::Machine);
        }
        world.entity_mut(e).insert(emitter);
    }
    let mut reflectors = world.query::<(Entity, &DmObj, Option<&DmAtom>)>();
    let found: Vec<(Entity, Reflector)> = reflectors
        .iter(world)
        .filter_map(|(e, obj, atom)| {
            let kind = if is_under(&obj.path, "/obj/structure/reflector/single") {
                ReflectorKind::Single
            } else if is_under(&obj.path, "/obj/structure/reflector/double") {
                ReflectorKind::Double
            } else if is_under(&obj.path, "/obj/structure/reflector/box") {
                ReflectorKind::Box
            } else {
                return None;
            };
            let dir = Dir::from_bits(atom.map_or(1, |a| a.dir)).unwrap_or(Dir::North);
            Some((
                e,
                Reflector {
                    kind,
                    angle: dir2angle(dir),
                },
            ))
        })
        .collect();
    info!("emitters: {} reflectors", found.len());
    world.insert_batch(found);
    if let Some(index) = index {
        world.insert_resource(index);
    }
}

/// What a beam does on entering `pos`.
enum Contact {
    Pass,
    Stop(Option<Entity>),
    Reflect(f64),
}

/// The dense thing on `pos` the beam meets, tg projectile `Impact` with the beam's
/// `PASSTABLE | PASSGLASS | PASSGRILLE`.
fn contact(world: &World, pos: TilePos, angle: f64, firer: Entity) -> Contact {
    let index = world.resource::<SpatialIndex>();
    for &e in index.on_tile(pos) {
        if e == firer {
            continue;
        }
        if let Some(reflector) = world.get::<Reflector>(e) {
            return match reflector.reflect(angle) {
                Some(new_angle) => Contact::Reflect(new_angle),
                None => Contact::Stop(Some(e)),
            };
        }
    }
    for &e in index.dense_on_tile(pos) {
        if e == firer || world.get::<Density>(e).is_none() {
            continue;
        }
        if world.get::<Turf>(e).is_some() {
            return Contact::Stop(None);
        }
        use crate::projectile::{PASSGLASS, PASSGRILLE, PASSTABLE, passes};
        let passes = passes(world, e, PASSTABLE | PASSGLASS | PASSGRILLE);
        if !passes {
            return Contact::Stop(Some(e));
        }
    }
    Contact::Pass
}

/// The hitscan path from `start` at `angle`: where it stopped and on what.
pub fn trace(
    world: &World,
    firer: Entity,
    start: TilePos,
    angle: f64,
) -> (TilePos, Option<Entity>) {
    let mut pos = start;
    let mut angle = angle;
    for _ in 0..MAX_RUNS {
        let Some((dx, dy)) = step_of(angle) else {
            // shortcut: beams fly the eight directions only (tg moves them by pixel).
            return (pos, None);
        };
        let mut turned = false;
        for _ in 0..BEAM_RANGE {
            pos = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            if !world.resource::<SpatialIndex>().in_bounds(pos) {
                return (pos, None);
            }
            match contact(world, pos, angle, firer) {
                Contact::Pass => {}
                Contact::Stop(target) => return (pos, target),
                Contact::Reflect(new_angle) => {
                    angle = new_angle;
                    turned = true;
                    break;
                }
            }
        }
        if !turned {
            return (pos, None);
        }
    }
    (pos, None)
}

/// Traces the shots of the last machine tick and applies their hits.
fn fire_beams(world: &mut World) {
    let shots = std::mem::take(&mut world.resource_mut::<PendingBeams>().0);
    for (emitter, start, angle) in shots {
        let (pos, target) = trace(world, emitter, start, angle);
        if let Some(target) = target {
            hit(world, emitter, target);
        }
        world.write_message(BeamHit {
            emitter,
            target,
            pos,
        });
    }
}

/// `bullet_act` of what the beam hit.
fn hit(world: &mut World, emitter: Entity, target: Entity) {
    if let Some(mut sm) = world.get_mut::<super::supermatter::Supermatter>(target) {
        sm.eat_beam(BEAM_DAMAGE);
        return;
    }
    // `/obj/projectile/beam/emitter` (`LASER`) on a mech (`crate::mecha`).
    let from = world
        .get::<TilePos>(target)
        .zip(world.get::<TilePos>(emitter))
        .and_then(|(&t, &e)| crate::mecha::core::dir_between(t, e));
    if crate::mecha::core::bullet_act(world, target, BEAM_DAMAGE, DamageType::Burn, "laser", from) {
        return;
    }
    let world_armor = crate::armor::getarmor(world, target, BodyZone::Chest, "laser");
    if let Some(mut entity) = world.get_entity_mut(target).ok()
        && let Some((mut health, mut stat)) =
            entity.get_components_mut::<(&mut Health, &mut Stat)>().ok()
    {
        // shortcut: the hitscan hits the chest (default `def_zone`), its armor unannounced.
        let armor = world_armor;
        apply_damage_zone(
            &mut health,
            &mut stat,
            BodyZone::Chest,
            DamageType::Burn,
            crate::armor::blocked_damage(BEAM_DAMAGE, armor),
        );
    }
}

/// `interact` from an empty-hand click.
fn toggle_on_click(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let pending: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in pending {
        if world.get::<Emitter>(click.target).is_none() {
            continue;
        }
        let empty = world
            .get::<crate::inventory::Inventory>(click.mob)
            .is_some_and(|i| i.active_item().is_none());
        if !empty {
            continue;
        }
        let wired = world.resource::<Grid>().node_net(click.target).is_some();
        let mut emitter = world.get_mut::<Emitter>(click.target).expect("checked");
        let (span, text) = emitter.toggle(wired);
        let on = emitter.active;
        tell(world, click.client, span, text);
        info!(target: "engine", "emitter {:?} turned {} by {:?}", click.target, if on { "ON" } else { "OFF" }, click.mob);
    }
}

/// `update_overlays`: the laser overlay while on, lime or orange (low power), with its
/// emissive.
fn emitter_looks(mut emitters: Query<(&Emitter, &mut Appearance), Changed<Emitter>>) {
    for (emitter, mut appearance) in &mut emitters {
        let overlays = match (emitter.active, emitter.overlay) {
            (true, Some(state)) => {
                let color = if emitter.powered {
                    LASER_COLOR
                } else {
                    LOW_POWER_COLOR
                };
                vec![
                    Overlay {
                        color: Some(color),
                        ..Overlay::new(state)
                    },
                    Overlay {
                        plane: Some(EMISSIVE_PLANE),
                        ..Overlay::new(state)
                    },
                ]
            }
            _ => Vec::new(),
        };
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
}

/// A timed emitter tool action ([`Job::Emitter`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitterJob {
    /// `default_unfasten_wrench` from this anchored state.
    Wrench { anchored: bool },
    /// `welder_act` from this welded state.
    Weld { welded: bool },
}

/// `wrench_act` / `welder_act`. True when handled.
pub fn tool_act(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    let Some(emitter) = world.get::<Emitter>(target).cloned() else {
        return false;
    };
    let anchored = world.get::<Anchored>(target).is_some_and(|a| a.0);
    let client = actor.client;
    match kind {
        ToolKind::Wrench => {
            // `can_be_unfasten_wrench`.
            if emitter.active {
                tell(world, client, Span::Warning, "Turn the emitter off first!");
            } else if emitter.welded {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "The emitter is welded to the floor!",
                );
            } else {
                use_tool(
                    world,
                    actor,
                    target,
                    20.0,
                    0,
                    Job::Emitter(EmitterJob::Wrench { anchored }),
                );
            }
        }
        ToolKind::Welder => {
            if emitter.active {
                tell(world, client, Span::Warning, "Turn the emitter off first!");
            } else if emitter.welded {
                if use_tool(
                    world,
                    actor,
                    target,
                    20.0,
                    1,
                    Job::Emitter(EmitterJob::Weld { welded: true }),
                ) {
                    tell(
                        world,
                        client,
                        Span::Notice,
                        "You start to cut the emitter free from the floor...",
                    );
                }
            } else if !anchored {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "The emitter needs to be wrenched to the floor!",
                );
            } else if use_tool(
                world,
                actor,
                target,
                20.0,
                1,
                Job::Emitter(EmitterJob::Weld { welded: false }),
            ) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "You start to weld the emitter to the floor...",
                );
            }
        }
        _ => return false,
    }
    true
}

/// The end of an [`EmitterJob`], re-checked against the state it started from.
pub fn complete(
    world: &mut World,
    client: Entity,
    target: Entity,
    job: EmitterJob,
    consume: impl FnOnce(&mut World) -> bool,
) {
    let Some(emitter) = world.get::<Emitter>(target).cloned() else {
        return;
    };
    let anchored = world.get::<Anchored>(target).is_some_and(|a| a.0);
    if emitter.active {
        return;
    }
    match job {
        EmitterJob::Wrench { anchored: from } => {
            if anchored != from || emitter.welded || !consume(world) {
                return;
            }
            set_anchored(world, target, !from);
            let text = if from {
                "You unsecure the emitter from the floor."
            } else {
                "You secure the emitter to the floor."
            };
            tell(world, client, Span::Notice, text);
        }
        EmitterJob::Weld { welded: from } => {
            if emitter.welded != from || (!from && !anchored) || !consume(world) {
                return;
            }
            let Some(&pos) = world.get::<TilePos>(target) else {
                return;
            };
            world.get_mut::<Emitter>(target).expect("checked").welded = !from;
            let mut grid = world.resource_mut::<Grid>();
            if from {
                grid.remove_node(target);
            } else {
                grid.add_node(target, pos, emitter.layer, NodeKind::Machine);
            }
            let text = if from {
                "You cut the emitter free from the floor."
            } else {
                "You weld the emitter to the floor."
            };
            tell(world, client, Span::Notice, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflectors_turn_beams_like_tg() {
        // MetaStation's engine: a south-firing emitter into a west-facing box.
        let boxed = Reflector {
            kind: ReflectorKind::Box,
            angle: 270.0,
        };
        assert_eq!(boxed.reflect(180.0), Some(270.0));
        // Single at SW (225) turns a north beam west; a beam from behind is absorbed.
        let single = Reflector {
            kind: ReflectorKind::Single,
            angle: 225.0,
        };
        assert_eq!(single.reflect(0.0), Some(270.0));
        assert_eq!(single.reflect(180.0), None);
        // Double at NE (45) turns a west beam north.
        let double = Reflector {
            kind: ReflectorKind::Double,
            angle: 45.0,
        };
        assert_eq!(double.reflect(270.0), Some(0.0));
    }

    #[test]
    fn burst_of_three_then_a_pause() {
        let mut rng = CombatRng(7);
        let mut net = Powernet {
            avail: 1e6,
            ..default()
        };
        let mut e = Emitter::new(Dir::North, true, 2);
        assert_eq!(e.toggle(true).0, Span::Notice);
        let mut shots = Vec::new();
        let mut t = 100.0;
        while t < 140.0 {
            net.load = 0.0;
            if e.process_early(Some(&mut net), t, 2.0, &mut rng) {
                shots.push(t);
            }
            assert_eq!(net.load, 600.0);
            t += 2.0;
        }
        // Four shots 2 s apart, then a pause of 2-10 s (rounded up to the 2 s tick).
        assert_eq!(&shots[..4], &[100.0, 102.0, 104.0, 106.0]);
        assert!(shots[4] - shots[3] >= 2.0 && shots[4] - shots[3] <= 10.0);
        // No surplus: no shot, no load, "low power".
        net.avail = 100.0;
        net.load = 0.0;
        assert!(!e.process_early(Some(&mut net), 1000.0, 2.0, &mut rng));
        assert!(!e.powered && net.load == 0.0);
        // Unwired: it switches off.
        assert!(!e.process_early(None, 1002.0, 2.0, &mut rng));
        assert!(!e.active);
    }
}
