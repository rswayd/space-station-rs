//! Transit tubes (server), after tg `code/game/objects/structures/transit_tubes/`
//! (`transit_tube.dm`, `station.dm`, `transit_tube_pod.dm`, `transit_tube_construction.dm`).
//!
//! - Tubes ([`TransitTube`]) carry tg's `tube_dirs` by shape and `dir` (straight, diagonal,
//!   curved, junction, crossings, stations) and the decorative end overlays.
//! - A pod ([`TubePod`]) launched into a tube follows it (`follow_tube` / `calibrate_engine`):
//!   one tile per `enter_delay + exit_delay` deciseconds, junctions picking an exit within 45°,
//!   until a station stops it; past the last tube it rolls one tile out and falls apart, its
//!   contents dropped; landing off a tube it ejects its insides and leaves its frame.
//! - Stations ([`TubeStation`]) open and close (0.6 s), take a mob walking into them from the
//!   boarding side into their pod, launch a pod at rest on their own every 2 s once 5 s passed
//!   since it arrived (closing first), push out the items and the mobs without a player when
//!   they open, and turn the pod around (terminus). A mob inside steps out of an open station,
//!   opens a closed one, or picks a tube direction to launch; mid-tube a step turns the pod.
//!   An empty hand opens or closes a station, or empties its pod's contents (1 s).
//! - Dispensers make a temporary pod around whoever walks in from the boarding side and launch
//!   it at once; a temporary pod vanishes at the next dispenser (ding) or off the tubes.
//! - A wrench detaches a tube without a pod into its unattached part (2 s) and attaches a part
//!   back (at most two tubes per tile); a crowbar empties a pod or takes it apart into its
//!   unattached pod (on a station: every pod there, contents out); the unattached pod dragged
//!   onto a station without one becomes a pod. Resisting in a pod empties it.
//!
//! Cost: per tick, one pass over the moving pods and the stations with a pending timer; a
//! station's 2 s process is one tile lookup.
// shortcut: no pod air (`air_contents`, the station's equalize), no climbing or elevation on
// tubes, no aggressive-grab stuffing at a station, no freight (non-mob) dispensing, no rotating
// unattached parts (`simple_rotation`, the RPD makes none), no singularity pulls or explosions
// of contents; add them with those mechanics.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, Overlay, TilePos};
use ss13_world::{Bumped, Density, Moved, SpatialIndex};

use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::closets::{ClosetRelayMove, InCloset};
use crate::construction::Job;
use crate::health::Stat;
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::storage::Stored;
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of};

pub const TUBE_ROOT: &str = "/obj/structure/transit_tube";
pub const CONSTRUCT_ROOT: &str = "/obj/structure/c_transit_tube";
pub const POD: &str = "/obj/structure/transit_tube_pod";
pub const DISPENSED_POD: &str = "/obj/structure/transit_tube_pod/dispensed";
pub const CONSTRUCT_POD: &str = "/obj/structure/c_transit_tube_pod";

/// Every tube shape by its type below [`TUBE_ROOT`] / [`CONSTRUCT_ROOT`], subtypes first (the
/// mapping helpers `horizontal` and `topleft` only preset `dir`). Spawned by path: listed in
/// `ss13_mapload::ENGMISC_RUNTIME_TYPES`.
pub const SHAPES: [&str; 16] = [
    "/station/dispenser/reverse/flipped",
    "/station/dispenser/reverse",
    "/station/dispenser/flipped",
    "/station/dispenser",
    "/station/reverse/flipped",
    "/station/reverse",
    "/station/flipped",
    "/station",
    "/junction/flipped",
    "/junction",
    "/curved/flipped",
    "/curved",
    "/diagonal/crossing",
    "/diagonal",
    "/crossing",
    "",
];

const N: u8 = 1;
const S: u8 = 2;
const E: u8 = 4;
const W: u8 = 8;
const NE: u8 = N | E;
const NW: u8 = N | W;
const SE: u8 = S | E;
const SW: u8 = S | W;

/// `OPEN_DURATION`, `CLOSE_DURATION`.
const ANIM_SECS: f64 = 0.6;
/// `sleep(CLOSE_DURATION + 2)` before a launch, `OPEN_DURATION + 2` before `finish_stopped`.
const SETTLE_SECS: f64 = 0.8;
/// `pod_stopped` -> `start_stopped`.
const STOPPED_SECS: f64 = 0.5;
/// `cooldown_delay`.
const LAUNCH_COOLDOWN_SECS: f64 = 5.0;
/// `SSobj` wait.
const PROCESS_SECS: f64 = 2.0;
/// `do_after(user, 1 SECONDS)` emptying a pod at an open station.
const EMPTY_DS: f32 = 10.0;
/// `use_tool(src, user, 2 SECONDS)` detaching and attaching.
const WRENCH_DS: f32 = 20.0;

/// The shape of `path` below `root`, if it is one.
pub fn shape_of(path: &str, root: &str) -> Option<&'static str> {
    let rest = path.strip_prefix(root)?;
    if !(rest.is_empty() || rest.starts_with('/')) {
        return None;
    }
    SHAPES
        .into_iter()
        .find(|s| is_under(path, &format!("{root}{s}")))
}

/// tg `init_tube_dirs` of a shape facing `dir`.
pub fn tube_dirs(shape: &str, dir: u8) -> Vec<u8> {
    let reverse = shape.contains("/reverse");
    let flipped = shape.ends_with("/flipped");
    if shape.starts_with("/station") {
        return if reverse {
            match dir {
                N => vec![E],
                S => vec![W],
                E => vec![S],
                W => vec![N],
                _ => vec![],
            }
        } else {
            match dir {
                N | S => vec![E, W],
                E | W => vec![N, S],
                _ => vec![],
            }
        };
    }
    let base = shape
        .trim_end_matches("/flipped")
        .trim_end_matches("/crossing");
    match (base, flipped, dir) {
        ("/diagonal", _, N | S) => vec![NE, SW],
        ("/diagonal", _, E | W) => vec![NW, SE],
        ("/curved", false, N) => vec![N, SW],
        ("/curved", false, S) => vec![S, NE],
        ("/curved", false, E) => vec![E, NW],
        ("/curved", false, W) => vec![SE, W],
        ("/curved", true, N) => vec![N, SE],
        ("/curved", true, S) => vec![S, NW],
        ("/curved", true, E) => vec![E, SW],
        ("/curved", true, W) => vec![NE, W],
        ("/junction", false, N) => vec![N, SE, SW],
        ("/junction", false, S) => vec![S, NW, NE],
        ("/junction", false, E) => vec![E, SW, NW],
        ("/junction", false, W) => vec![W, NE, SE],
        ("/junction", true, N) => vec![N, SW, SE],
        ("/junction", true, S) => vec![S, NE, NW],
        ("/junction", true, E) => vec![E, NW, SW],
        ("/junction", true, W) => vec![W, SE, NE],
        ("" | "/crossing", _, N | S) => vec![N, S],
        ("" | "/crossing", _, E | W) => vec![E, W],
        _ => vec![],
    }
}

fn reverse(dir: u8) -> u8 {
    Dir::from_bits(dir).map_or(0, |d| d.reverse() as u8)
}

/// One tube section.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TransitTube {
    pub shape: &'static str,
    pub dir: u8,
    pub dirs: Vec<u8>,
    /// Deciseconds (`exit_delay`, `enter_delay`).
    pub exit_delay: f32,
    pub enter_delay: f32,
}

impl TransitTube {
    pub fn new(shape: &'static str, dir: u8) -> Self {
        let (exit_delay, enter_delay) = if shape.starts_with("/station/dispenser") {
            (1.0, 1.0)
        } else if shape.starts_with("/station") {
            (1.0, 2.0)
        } else {
            (1.0, 0.0)
        };
        Self {
            shape,
            dir,
            dirs: tube_dirs(shape, dir),
            exit_delay,
            enter_delay,
        }
    }

    /// `has_entrance`.
    pub fn has_entrance(&self, from_dir: u8) -> bool {
        self.dirs.contains(&reverse(from_dir))
    }

    /// `has_exit`.
    pub fn has_exit(&self, in_dir: u8) -> bool {
        self.dirs.contains(&in_dir)
    }

    /// `get_exit`: `in_dir` itself, else the last direction within 45° of it, else 0.
    pub fn get_exit(&self, in_dir: u8) -> u8 {
        let cw = crate::disposals::pipes::turn(in_dir, -45);
        let ccw = crate::disposals::pipes::turn(in_dir, 45);
        let mut near = 0;
        for &d in &self.dirs {
            if d == in_dir {
                return d;
            }
            if d == cw || d == ccw {
                near = d;
            }
        }
        near
    }

    fn is_station(&self) -> bool {
        self.shape.starts_with("/station")
    }
}

/// `STATION_TUBE_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Open,
    Opening,
    Closed,
    Closing,
}

/// A station's state (`open_status`, `pod_moving`, `launch_cooldown`, its timers).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TubeStation {
    pub status: Status,
    pub pod_moving: bool,
    pub dispenser: bool,
    pub reverse_launch: bool,
    pub boarding_dir: u8,
    /// `base_icon_state`.
    pub base: &'static str,
    pub launch_cooldown: f64,
    anim_done: Option<f64>,
    /// `launch_pod`'s sleep: the pod launches at this time if the station closed meanwhile.
    launch: Option<(f64, Entity)>,
    /// `start_stopped`, then `finish_stopped`.
    stopped: Option<(f64, Entity, bool)>,
    next_process: f64,
}

impl TubeStation {
    fn new(shape: &str, dir: u8) -> Self {
        let dispenser = shape.starts_with("/station/dispenser");
        let reverse_launch = shape.contains("/reverse");
        let flipped = shape.ends_with("/flipped");
        let base = match (dispenser, reverse_launch, flipped) {
            (false, false, false) => "station0",
            (false, false, true) => "station1",
            (false, true, false) => "terminus0",
            (false, true, true) => "terminus1",
            (true, false, false) => "dispenser0",
            (true, false, true) => "dispenser1",
            (true, true, false) => "terminusdispenser0",
            (true, true, true) => "terminusdispenser1",
        };
        Self {
            status: if dispenser {
                Status::Open
            } else {
                Status::Closed
            },
            pod_moving: false,
            dispenser,
            reverse_launch,
            boarding_dir: if flipped { dir } else { reverse(dir) },
            base,
            launch_cooldown: 0.0,
            anim_done: None,
            launch: None,
            stopped: None,
            next_process: 0.0,
        }
    }

    fn icon_state(&self) -> String {
        let prefix = match self.status {
            Status::Open => "open",
            Status::Opening => "opening",
            Status::Closed => "closed",
            Status::Closing => "closing",
        };
        format!("{prefix}_{}", self.base)
    }
}

/// A pod (`moving`, `dir`, `current_tube` and its move loop).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TubePod {
    pub moving: bool,
    pub dir: u8,
    pub dispensed: bool,
    current: Option<Entity>,
    /// The engine's direction and its next step.
    next_dir: u8,
    next_at: f64,
}

/// A timed step with a tube or a station.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TubeJob {
    Detach,
    Attach,
    /// The pod's contents onto the user's tile at an open station.
    Empty {
        pod: Entity,
    },
}

pub struct TransitTubePlugin;

impl Plugin for TransitTubePlugin {
    fn build(&self, app: &mut App) {
        app.add_click_handler(ClickPhase::HeldOnTarget, 291, held_on_target)
            .add_click_handler(ClickPhase::EmptyHand, 3511, attack_hand)
            .add_systems(ss13_world::MapInit, init_mapped)
            .add_systems(
                Update,
                (
                    relay_moves,
                    bumps.after(ss13_world::WorldSystems::Movement),
                    move_pods.run_if(any_with_component::<TubePod>),
                    run_stations.run_if(any_with_component::<TubeStation>),
                )
                    .chain(),
            );
    }
}

fn path_of(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmObj>(e).map(|o| o.path.as_str())
}

fn dir_of(world: &World, e: Entity) -> u8 {
    world
        .get::<Facing>(e)
        .map(|f| f.0 as u8)
        .or_else(|| world.get::<DmAtom>(e).map(|a| a.dir))
        .unwrap_or(S)
}

fn now(world: &World) -> f64 {
    crate::tools::now(world)
}

fn init_mapped(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, TUBE_ROOT) || is_under(&o.path, POD))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        init(world, e);
    }
}

/// `Initialize` of a tube, station or pod.
pub fn init(world: &mut World, e: Entity) {
    let Some(path) = path_of(world, e).map(str::to_owned) else {
        return;
    };
    let dir = dir_of(world, e);
    if let Some(shape) = shape_of(&path, TUBE_ROOT) {
        let tube = TransitTube::new(shape, dir);
        if tube.is_station() {
            let station = TubeStation::new(shape, dir);
            world.entity_mut(e).insert(station);
            crate::obj_damage::set_icon_state(world, e, &station.icon_state());
        }
        world.entity_mut(e).insert(tube);
        tube_overlays(world, e);
    } else if is_under(&path, POD) {
        world.entity_mut(e).insert(TubePod {
            moving: false,
            dir,
            dispensed: is_under(&path, DISPENSED_POD),
            current: None,
            next_dir: 0,
            next_at: 0.0,
        });
    }
}

/// `update_overlays`: the decorative ends of every direction.
fn tube_overlays(world: &mut World, e: Entity) {
    let (Some(tube), Some(icon)) = (
        world.get::<TransitTube>(e),
        world.get::<DmAtom>(e).map(|a| a.icon.clone()),
    ) else {
        return;
    };
    let Some(index) = world.get_resource::<IconIndex>() else {
        return;
    };
    let mut overlays = Vec::new();
    let mut add = |state: &str, dir: u8, shift: [i16; 2]| {
        if let (Some(id), Some(d)) = (index.get(&icon, state), Dir::from_bits(dir)) {
            let mut o = Overlay::new(id);
            o.dir = Some(d);
            o.pixel_offset = shift;
            overlays.push(o);
        }
    };
    for &d in &tube.dirs {
        let diagonal = d & (N | S) != 0 && d & (E | W) != 0;
        if !diagonal {
            add("decorative", d, [0, 0]);
            continue;
        }
        if d & N == 0 {
            continue;
        }
        add("decorative_diag", d ^ 3, [0, 32]);
        if d & E != 0 {
            add("decorative_diag", d ^ 12, [32, 0]);
        } else {
            add("decorative_diag", d ^ 12, [-32, 0]);
        }
    }
    if let Some(mut appearance) = world.get_mut::<Appearance>(e) {
        appearance.overlays = overlays;
    }
}

fn tubes_on(world: &World, pos: TilePos) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<TransitTube>(e).is_some())
        .collect()
}

fn pods_on(world: &World, pos: TilePos) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<TubePod>(e).is_some())
        .collect()
}

fn station_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<TubeStation>(e).is_some())
}

/// The mobs inside `pod`.
fn mobs_in(world: &mut World, pod: Entity) -> Vec<Entity> {
    crate::disposals::mobs_in(world, pod)
}

fn items_in(world: &World, pod: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(pod)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

fn occupied(world: &mut World, pod: Entity) -> bool {
    !items_in(world, pod).is_empty() || !mobs_in(world, pod).is_empty()
}

/// `update_icon_state`: occupied or empty.
fn refresh_pod(world: &mut World, pod: Entity) {
    let Some(p) = world.get::<TubePod>(pod).copied() else {
        return;
    };
    let base = if p.dispensed { "temppod" } else { "pod" };
    let state = if occupied(world, pod) {
        format!("{base}_occupied")
    } else {
        base.to_owned()
    };
    crate::obj_damage::set_icon_state(world, pod, &state);
}

fn set_pod_dir(world: &mut World, pod: Entity, dir: u8) {
    if let Some(mut p) = world.get_mut::<TubePod>(pod) {
        p.dir = dir;
    }
    if let Some(d) = Dir::from_bits(dir) {
        world.entity_mut(pod).insert(Facing(d));
    }
}

fn set_station(world: &mut World, station: Entity, f: impl FnOnce(&mut TubeStation)) {
    let Some(mut s) = world.get::<TubeStation>(station).copied() else {
        return;
    };
    let before = s.icon_state();
    f(&mut s);
    let after = s.icon_state();
    world.entity_mut(station).insert(s);
    if before != after {
        crate::obj_damage::set_icon_state(world, station, &after);
    }
}

/// Puts `thing` (a mob or an item) into `pod`.
fn put_in(world: &mut World, thing: Entity, pod: Entity) {
    if world.get::<crate::health::Health>(thing).is_some() {
        if let Some(&pos) = world.get::<TilePos>(pod) {
            world.entity_mut(thing).insert(pos);
        }
        crate::closets::put_mob_inside(world, thing, pod);
    } else {
        crate::storage::put_inside(world, thing, pod);
    }
    refresh_pod(world, pod);
}

/// `empty_pod(location)`: everything inside onto `at`; what came out.
fn empty_pod(world: &mut World, pod: Entity, at: TilePos) -> Vec<Entity> {
    let mut out = Vec::new();
    for item in items_in(world, pod) {
        crate::storage::take_out(world, item, at);
        out.push(item);
    }
    for mob in mobs_in(world, pod) {
        crate::closets::let_out(world, mob);
        world.entity_mut(mob).insert(at);
        out.push(mob);
    }
    refresh_pod(world, pod);
    out
}

/// `qdel(pod)`: `Destroy` empties it where it is.
fn destroy_pod(world: &mut World, pod: Entity) {
    if let Some(&pos) = world.get::<TilePos>(pod) {
        empty_pod(world, pod, pos);
        world.despawn(pod);
        crate::construction::tile_changed(world, pos);
    }
}

/// `deconstruct`: the unattached pod in its place (`atom_deconstruct`), the contents out.
fn deconstruct_pod(world: &mut World, pod: Entity) -> Vec<Entity> {
    let Some(&pos) = world.get::<TilePos>(pod) else {
        return Vec::new();
    };
    let dir = world.get::<TubePod>(pod).map_or(S, |p| p.dir);
    if let Some(frame) = crate::construction::spawn_obj(world, CONSTRUCT_POD, pos)
        && let Some(d) = Dir::from_bits(dir)
    {
        world.entity_mut(frame).insert(Facing(d));
    }
    let out = empty_pod(world, pod, pos);
    world.despawn(pod);
    crate::construction::tile_changed(world, pos);
    out
}

/// `follow_tube`: the pod starts along `tube` if it has an exit its way.
fn follow_tube(world: &mut World, pod: Entity, tube: Entity) {
    let Some(p) = world.get::<TubePod>(pod).copied() else {
        return;
    };
    if p.moving
        || !world
            .get::<TransitTube>(tube)
            .is_some_and(|t| t.has_exit(p.dir))
    {
        return;
    }
    if let Some(mut p) = world.get_mut::<TubePod>(pod) {
        p.moving = true;
        p.current = Some(tube);
    }
    calibrate(world, pod, now(world));
}

/// `calibrate_engine`: the next direction and delay, or the end of the line.
fn calibrate(world: &mut World, pod: Entity, at: f64) {
    let (Some(p), Some(&pos)) = (
        world.get::<TubePod>(pod).copied(),
        world.get::<TilePos>(pod),
    ) else {
        return;
    };
    let Some(current) = p.current.and_then(|t| world.get::<TransitTube>(t).cloned()) else {
        engine_finish(world, pod);
        return;
    };
    let next_dir = current.get_exit(p.dir);
    if next_dir == 0 {
        engine_finish(world, pod);
        return;
    }
    let Some(d) = Dir::from_bits(next_dir) else {
        engine_finish(world, pod);
        return;
    };
    let next_loc = pos.step(d);
    let next = tubes_on(world, next_loc).into_iter().find(|&t| {
        world
            .get::<TransitTube>(t)
            .is_some_and(|t| t.has_entrance(next_dir))
    });
    let Some(next) = next else {
        // Out of the tubes: one ordinary step (it collides), then the pod is gone.
        set_pod_dir(world, pod, next_dir);
        let _ = crate::pull::step_once(world, pod, d, None);
        destroy_pod(world, pod);
        return;
    };
    let enter = world
        .get::<TransitTube>(next)
        .map_or(0.0, |t| t.enter_delay);
    if let Some(mut p) = world.get_mut::<TubePod>(pod) {
        p.current = Some(next);
        p.next_dir = next_dir;
        p.next_at = at + f64::from(enter + current.exit_delay) / 10.0;
    }
}

/// `engine_finish`: stopped; off a tube that runs its way, it ejects its insides.
fn engine_finish(world: &mut World, pod: Entity) {
    world.entity_mut(pod).insert(Density);
    let Some(p) = world.get::<TubePod>(pod).copied() else {
        return;
    };
    if let Some(mut p) = world.get_mut::<TubePod>(pod) {
        p.moving = false;
    }
    let Some(&pos) = world.get::<TilePos>(pod) else {
        return;
    };
    let on_tube = tubes_on(world, pos).into_iter().next().is_some_and(|t| {
        world
            .get::<TransitTube>(t)
            .is_some_and(|t| t.dirs.contains(&p.dir) || t.dirs.contains(&reverse(p.dir)))
    });
    if !on_tube {
        outside_tube(world, pod, pos, p);
    }
}

/// `outside_tube`: the frame stays, the contents fly 1-3 tiles on; a temporary pod is simply
/// gone.
fn outside_tube(world: &mut World, pod: Entity, pos: TilePos, p: TubePod) {
    if p.dispensed {
        destroy_pod(world, pod);
        return;
    }
    let name = crate::medical::name_of(world, pod);
    let clients = crate::obj_damage::clients_in_range(world, pos, 7, &[]);
    for client in clients {
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "{} выбрасывает своё содержимое!",
                ss13_shared::capitalize(&name)
            ),
        );
    }
    let out = deconstruct_pod(world, pod);
    let (dx, dy) = crate::disposals::pipes::offset(p.dir);
    let target = TilePos::new(pos.x + dx * 32, pos.y + dy * 32, pos.z);
    let now_ds = now(world) * 10.0;
    for thing in out {
        let range = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(2, |mut rng| rng.range(1, 3));
        if let Some(path) = crate::throw::Trajectory::new(pos, target, range, 5.0, now_ds) {
            world.entity_mut(thing).insert(crate::throw::Thrown {
                path,
                thrower: None,
                target: None,
                last_dir: None,
            });
        }
    }
}

/// The move loops: each due pod takes its step (`before_pipe_transfer`, the move,
/// `after_pipe_transfer`).
fn move_pods(world: &mut World) {
    let now = now(world);
    let mut query = world.query::<(Entity, &TubePod)>();
    let mut due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, p)| p.moving && p.next_at <= now)
        .map(|(e, _)| e)
        .collect();
    due.sort();
    for pod in due {
        // A delay shorter than a tick takes several steps.
        for _ in 0..8 {
            let Some(p) = world.get::<TubePod>(pod).copied() else {
                break;
            };
            if !p.moving || p.next_at > now {
                break;
            }
            step_pod(world, pod, p);
        }
    }
}

fn step_pod(world: &mut World, pod: Entity, p: TubePod) {
    let (Some(&from), Some(d)) = (world.get::<TilePos>(pod), Dir::from_bits(p.next_dir)) else {
        engine_finish(world, pod);
        return;
    };
    set_pod_dir(world, pod, p.next_dir);
    let to = from.step(d);
    world.entity_mut(pod).insert(to);
    world.write_message(Moved {
        entity: pod,
        from,
        to,
        dir: d,
    });
    for mob in mobs_in(world, pod) {
        world.entity_mut(mob).insert(to);
    }
    let Some(tube) = p.current.filter(|&t| world.get::<TransitTube>(t).is_some()) else {
        engine_finish(world, pod);
        return;
    };
    // `set_density(current_tube.density)`.
    if world.get::<Density>(tube).is_some() {
        world.entity_mut(pod).insert(Density);
    } else {
        world.entity_mut(pod).remove::<Density>();
    }
    if world.get::<TubeStation>(tube).is_some() {
        pod_stopped(world, tube, pod);
        if world.get::<TubePod>(pod).is_some() {
            engine_finish(world, pod);
        }
        return;
    }
    calibrate(world, pod, p.next_at);
}

/// `pod_stopped`: a dispenser dings the temporary pod away; a station opens after 0.5 s.
fn pod_stopped(world: &mut World, station: Entity, pod: Entity) {
    let Some(s) = world.get::<TubeStation>(station).copied() else {
        return;
    };
    if s.dispenser {
        if let Some(&pos) = world.get::<TilePos>(station) {
            playsound(
                world,
                PlaySound::new(pos, "sound/machines/ding.ogg", 50.0, true),
            );
        }
        destroy_pod(world, pod);
        return;
    }
    let at = now(world) + STOPPED_SECS;
    set_station(world, station, |s| {
        s.pod_moving = true;
        s.stopped = Some((at, pod, false));
    });
}

/// `open_animation`.
fn open_station(world: &mut World, station: Entity) {
    let at = now(world) + ANIM_SECS;
    set_station(world, station, |s| {
        if s.status == Status::Closed {
            s.status = Status::Opening;
            s.anim_done = Some(at);
        }
    });
}

/// `close_animation` (a dispenser never closes).
fn close_station(world: &mut World, station: Entity) {
    let at = now(world) + ANIM_SECS;
    set_station(world, station, |s| {
        if s.status == Status::Open && !s.dispenser {
            s.status = Status::Closing;
            s.anim_done = Some(at);
        }
    });
}

/// `launch_pod`: a pod at rest closes the station and leaves 0.8 s later (a dispenser sends
/// it at once).
fn launch_pod(world: &mut World, station: Entity) -> bool {
    let (Some(s), Some(&pos)) = (
        world.get::<TubeStation>(station).copied(),
        world.get::<TilePos>(station),
    ) else {
        return false;
    };
    let Some(pod) = pods_on(world, pos)
        .into_iter()
        .find(|&p| world.get::<TubePod>(p).is_some_and(|p| !p.moving))
    else {
        return false;
    };
    if s.dispenser {
        follow_tube(world, pod, station);
        return true;
    }
    if s.launch_cooldown >= now(world) {
        return false;
    }
    let at = now(world) + SETTLE_SECS;
    set_station(world, station, |s| {
        s.pod_moving = true;
        s.launch = Some((at, pod));
    });
    close_station(world, station);
    true
}

/// The stations' timers and `process`.
fn run_stations(world: &mut World) {
    let now = now(world);
    let mut query = world.query::<(Entity, &TubeStation)>();
    let stations: Vec<Entity> = query
        .iter(world)
        .filter(|(_, s)| {
            s.anim_done.is_some_and(|t| t <= now)
                || s.launch.is_some_and(|(t, _)| t <= now)
                || s.stopped.is_some_and(|(t, ..)| t <= now)
                || s.next_process <= now
        })
        .map(|(e, _)| e)
        .collect();
    for station in stations {
        let Some(s) = world.get::<TubeStation>(station).copied() else {
            continue;
        };
        if s.anim_done.is_some_and(|t| t <= now) {
            finish_animation(world, station);
        }
        if let Some((t, pod)) = s.launch
            && t <= now
        {
            let closed = world
                .get::<TubeStation>(station)
                .is_some_and(|s| s.status == Status::Closed);
            let here = world.get::<TilePos>(pod) == world.get::<TilePos>(station);
            set_station(world, station, |s| {
                s.launch = None;
                s.pod_moving = false;
            });
            if closed && here && world.get::<TubePod>(pod).is_some() {
                follow_tube(world, pod, station);
            }
        }
        if let Some((t, pod, opened)) = s.stopped
            && t <= now
        {
            stopped_stage(world, station, pod, opened, now);
        }
        if s.next_process <= now {
            set_station(world, station, |s| s.next_process = now + PROCESS_SECS);
            if !world
                .get::<TubeStation>(station)
                .is_some_and(|s| s.pod_moving)
            {
                launch_pod(world, station);
            }
        }
    }
}

/// `start_stopped` (turn a terminus pod around, 5 s cooldown, open) and `finish_stopped`.
fn stopped_stage(world: &mut World, station: Entity, pod: Entity, opened: bool, now: f64) {
    if world.get::<TubePod>(pod).is_none() {
        // tg leaves `pod_moving` set when the pod is gone: the station would never launch again.
        set_station(world, station, |s| {
            s.stopped = None;
            s.pod_moving = false;
        });
        return;
    }
    if opened {
        set_station(world, station, |s| {
            s.stopped = None;
            s.pod_moving = false;
        });
        return;
    }
    let s = world.get::<TubeStation>(station).copied();
    if let Some(s) = s
        && s.reverse_launch
        && let Some(&first) = world
            .get::<TransitTube>(station)
            .and_then(|t| t.dirs.first())
    {
        set_pod_dir(world, pod, first);
    }
    set_station(world, station, |s| {
        s.launch_cooldown = now + LAUNCH_COOLDOWN_SECS;
        s.stopped = Some((now + SETTLE_SECS, pod, true));
    });
    open_station(world, station);
}

/// `finish_animation`: opened (the items and the mobs without a player or awake out of its
/// pods) or closed.
fn finish_animation(world: &mut World, station: Entity) {
    let Some(s) = world.get::<TubeStation>(station).copied() else {
        return;
    };
    match s.status {
        Status::Opening => {
            set_station(world, station, |s| {
                s.status = Status::Open;
                s.anim_done = None;
            });
            let Some(&pos) = world.get::<TilePos>(station) else {
                return;
            };
            for pod in pods_on(world, pos) {
                for item in items_in(world, pod) {
                    crate::storage::take_out(world, item, pos);
                }
                for mob in mobs_in(world, pod) {
                    let player = world.get::<PlayerMob>(mob).is_some();
                    let awake = world.get::<Stat>(mob) == Some(&Stat::Conscious);
                    if player && awake {
                        continue;
                    }
                    crate::closets::let_out(world, mob);
                    world.entity_mut(mob).insert(pos);
                }
                refresh_pod(world, pod);
            }
        }
        Status::Closing => set_station(world, station, |s| {
            s.status = Status::Closed;
            s.anim_done = None;
        }),
        _ => set_station(world, station, |s| s.anim_done = None),
    }
}

/// `transit_tube_pod/relaymove`: a step of a player inside a pod at rest.
fn relay_moves(
    world: &mut World,
    mut moves: Local<bevy::ecs::message::MessageCursor<ClosetRelayMove>>,
) {
    let steps: Vec<(Entity, Dir)> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| (m.mob, m.dir))
        .collect();
    for (mob, dir) in steps {
        let Some(pod) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        let Some(p) = world.get::<TubePod>(pod).copied() else {
            continue;
        };
        if world.get::<PlayerMob>(mob).is_none() || p.moving {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(pod) else {
            continue;
        };
        let dir = dir as u8;
        if let Some(station) = station_on(world, pos) {
            let s = *world.get::<TubeStation>(station).expect("found");
            if s.pod_moving {
                continue;
            }
            if dir == reverse(s.boarding_dir) {
                if s.status == Status::Open {
                    crate::closets::let_out(world, mob);
                    world.entity_mut(mob).insert(pos);
                    refresh_pod(world, pod);
                } else {
                    open_station(world, station);
                }
            } else if world
                .get::<TransitTube>(station)
                .is_some_and(|t| t.dirs.contains(&dir))
            {
                set_pod_dir(world, pod, dir);
                launch_pod(world, station);
            }
            continue;
        }
        let turns = tubes_on(world, pos).into_iter().any(|t| {
            world
                .get::<TransitTube>(t)
                .is_some_and(|t| t.dirs.contains(&p.dir) && t.has_exit(dir))
        });
        if turns {
            set_pod_dir(world, pod, dir);
        }
    }
}

/// `station/Bumped` and `dispenser/Bumped`: a mob walking in from the boarding side.
fn bumps(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<Bumped>>) {
    let bumped: Vec<Bumped> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    for b in bumped {
        let Some(s) = world.get::<TubeStation>(b.obstacle).copied() else {
            continue;
        };
        let mob = b.entity;
        if world.get::<crate::health::Health>(mob).is_none()
            || world.get::<InCloset>(mob).is_some()
            || dir_of(world, mob) != s.boarding_dir
        {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(b.obstacle) else {
            continue;
        };
        if s.dispenser {
            let station_dir = dir_of(world, b.obstacle);
            let Some(pod) = crate::construction::spawn_obj(world, DISPENSED_POD, pos) else {
                continue;
            };
            init(world, pod);
            set_pod_dir(world, pod, crate::disposals::pipes::turn(station_dir, -90));
            crate::chem::visible(
                world,
                mob,
                "Вокруг вас материализуется временная капсула.",
                "Вокруг кого-то формируется временная капсула.",
                Span::Notice,
            );
            playsound(
                world,
                PlaySound::new(pos, "sound/items/weapons/emitter2.ogg", 50.0, true),
            );
            put_in(world, mob, pod);
            launch_pod(world, b.obstacle);
            continue;
        }
        if s.pod_moving || s.status != Status::Open {
            continue;
        }
        if let Some(pod) = pods_on(world, pos)
            .into_iter()
            .find(|&p| world.get::<TubePod>(p).is_some_and(|p| !p.moving))
        {
            put_in(world, mob, pod);
        }
    }
}

/// `station/attack_hand`: open, close, or empty the pod at rest.
fn attack_hand(world: &mut World, ctx: &ClickCtx) -> bool {
    let Some(s) = world.get::<TubeStation>(ctx.target).copied() else {
        return false;
    };
    if s.pod_moving {
        return true;
    }
    let dirs = world
        .get::<TransitTube>(ctx.target)
        .map(|t| t.dirs.clone())
        .unwrap_or_default();
    let Some(pod) = pods_on(world, ctx.pos).into_iter().next() else {
        return true;
    };
    let Some(p) = world.get::<TubePod>(pod).copied() else {
        return true;
    };
    if p.moving || !dirs.contains(&p.dir) {
        return true;
    }
    match s.status {
        Status::Closed => open_station(world, ctx.target),
        Status::Open => {
            let inside = world
                .get::<InCloset>(ctx.mob)
                .is_some_and(|c| c.closet == pod);
            if occupied(world, pod) && !inside {
                crate::chem::visible(
                    world,
                    ctx.mob,
                    "Вы начинаете вытряхивать содержимое капсулы на пол...",
                    "Кто-то начинает вытряхивать содержимое капсулы на пол.",
                    Span::Notice,
                );
                let actor = Actor {
                    client: ctx.client,
                    mob: ctx.mob,
                    item: None,
                };
                crate::tools::do_after(
                    world,
                    actor,
                    ctx.target,
                    EMPTY_DS,
                    0,
                    Job::TransitTube(TubeJob::Empty { pod }),
                );
            } else {
                close_station(world, ctx.target);
            }
        }
        _ => {}
    }
    true
}

/// Wrench and crowbar on tubes, stations, pods and unattached parts.
fn held_on_target(world: &mut World, ctx: &ClickCtx) -> bool {
    let Some(held) = ctx.held else {
        return false;
    };
    let Some((kind, _)) = tool_of(world, held) else {
        return false;
    };
    let target = ctx.target;
    let actor = Actor {
        client: ctx.client,
        mob: ctx.mob,
        item: Some(held),
    };
    let sound = |world: &mut World, file: &str, vol: f32| {
        playsound(world, PlaySound::new(ctx.pos, file, vol, true));
    };
    match kind {
        ToolKind::Crowbar if world.get::<TubePod>(target).is_some() => {
            crowbar_pod(world, ctx, target);
            sound(world, "sound/items/tools/crowbar.ogg", 50.0);
            true
        }
        ToolKind::Crowbar if world.get::<TubeStation>(target).is_some() => {
            let pods = pods_on(world, ctx.pos);
            let any = !pods.is_empty();
            for pod in pods {
                deconstruct_pod(world, pod);
            }
            let text = if any {
                "Вы опустошаете станцию."
            } else {
                "Станция уже пуста!"
            };
            tell(world, ctx.client, Span::Notice, text);
            true
        }
        ToolKind::Crowbar if world.get::<TransitTube>(target).is_some() => {
            for pod in pods_on(world, ctx.pos) {
                crowbar_pod(world, ctx, pod);
            }
            true
        }
        ToolKind::Wrench if world.get::<TransitTube>(target).is_some() => {
            if !pods_on(world, ctx.pos).is_empty() {
                tell(world, ctx.client, Span::Warning, "Сначала уберите капсулу!");
                return true;
            }
            crate::chem::visible(
                world,
                ctx.mob,
                "Вы начинаете отсоединять трубу...",
                "Кто-то начинает отсоединять трубу.",
                Span::Notice,
            );
            if crate::tools::use_tool(
                world,
                actor,
                target,
                WRENCH_DS,
                0,
                Job::TransitTube(TubeJob::Detach),
            ) {
                sound(world, "sound/items/tools/ratchet.ogg", 50.0);
            }
            true
        }
        ToolKind::Wrench
            if path_of(world, target).is_some_and(|p| shape_of(p, CONSTRUCT_ROOT).is_some()) =>
        {
            if !can_attach_on(world, ctx.client, ctx.pos) {
                return true;
            }
            tell(
                world,
                ctx.client,
                Span::Notice,
                "Вы начинаете присоединять трубу...",
            );
            if crate::tools::use_tool(
                world,
                actor,
                target,
                WRENCH_DS,
                0,
                Job::TransitTube(TubeJob::Attach),
            ) {
                sound(world, "sound/items/tools/ratchet.ogg", 50.0);
            }
            true
        }
        _ => false,
    }
}

/// `transit_tube_pod/crowbar_act`: a pod at rest is emptied, an empty one taken apart.
fn crowbar_pod(world: &mut World, ctx: &ClickCtx, pod: Entity) {
    if world.get::<TubePod>(pod).is_none_or(|p| p.moving) {
        return;
    }
    if occupied(world, pod) {
        crate::chem::visible(
            world,
            ctx.mob,
            "Вы опустошаете капсулу.",
            "Кто-то опустошает капсулу.",
            Span::Notice,
        );
        if let Some(&pos) = world.get::<TilePos>(pod) {
            empty_pod(world, pod, pos);
        }
    } else {
        deconstruct_pod(world, pod);
    }
}

/// `can_wrench_in_loc`: fewer than two tubes on the tile.
fn can_attach_on(world: &mut World, client: Entity, pos: TilePos) -> bool {
    if tubes_on(world, pos).len() >= 2 {
        tell(
            world,
            client,
            Span::Warning,
            "Сюда больше нельзя прикрутить трубы!",
        );
        return false;
    }
    true
}

/// A finished wrench or emptying step, re-checked now.
pub fn complete(world: &mut World, action: &DoAfter, job: TubeJob) {
    let target = action.target;
    let client = action.client;
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    match job {
        TubeJob::Detach => {
            let Some(shape) = world.get::<TransitTube>(target).map(|t| t.shape) else {
                return;
            };
            if !pods_on(world, pos).is_empty() {
                return;
            }
            let dir = dir_of(world, target);
            world.despawn(target);
            if let Some(part) =
                crate::construction::spawn_obj(world, &format!("{CONSTRUCT_ROOT}{shape}"), pos)
                && let Some(d) = Dir::from_bits(dir)
            {
                world.entity_mut(part).insert(Facing(d));
            }
            crate::construction::tile_changed(world, pos);
            tell(world, client, Span::Notice, "Вы отсоединяете трубу.");
        }
        TubeJob::Attach => {
            let Some(shape) = path_of(world, target).and_then(|p| shape_of(p, CONSTRUCT_ROOT))
            else {
                return;
            };
            if !can_attach_on(world, client, pos) {
                return;
            }
            let dir = dir_of(world, target);
            world.despawn(target);
            if let Some(tube) =
                crate::construction::spawn_obj(world, &format!("{TUBE_ROOT}{shape}"), pos)
            {
                if let Some(d) = Dir::from_bits(dir) {
                    world.entity_mut(tube).insert(Facing(d));
                }
                init(world, tube);
            }
            crate::construction::tile_changed(world, pos);
            tell(world, client, Span::Notice, "Вы присоединяете трубу.");
        }
        TubeJob::Empty { pod } => {
            let open = world
                .get::<TubeStation>(target)
                .is_some_and(|s| s.status == Status::Open);
            if open && world.get::<TilePos>(pod) == Some(&pos) {
                empty_pod(world, pod, action.user_pos);
            }
        }
    }
}

/// `station/mouse_drop_receive`: an unattached pod dragged onto a station without one.
pub fn mouse_drop_receive(world: &mut World, actor: Actor, over: Entity, dragged: Entity) -> bool {
    if world.get::<TubeStation>(over).is_none()
        || !path_of(world, dragged).is_some_and(|p| is_under(p, CONSTRUCT_POD))
    {
        return false;
    }
    let (Some(&pos), Some(&from)) = (world.get::<TilePos>(over), world.get::<TilePos>(dragged))
    else {
        return true;
    };
    if pos.chebyshev(from).is_none_or(|d| d > 1) || !pods_on(world, pos).is_empty() {
        return true;
    }
    let station_dir = dir_of(world, over);
    world.despawn(dragged);
    crate::construction::tile_changed(world, from);
    if let Some(pod) = crate::construction::spawn_obj(world, POD, pos) {
        init(world, pod);
        set_pod_dir(world, pod, crate::disposals::pipes::turn(station_dir, -90));
    }
    crate::chem::visible(
        world,
        actor.mob,
        "Вы вставляете капсулу.",
        "Кто-то вставляет капсулу.",
        Span::Notice,
    );
    true
}

/// `transit_tube_pod/container_resist_act`: an able mob empties the pod.
pub fn container_resist(world: &mut World, mob: Entity) {
    let Some(pod) = world.get::<InCloset>(mob).map(|c| c.closet) else {
        return;
    };
    if world.get::<TubePod>(pod).is_none() {
        return;
    }
    let able = world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated;
    // shortcut: an incapacitated mob does not struggle for a minute; it stays.
    if able && let Some(&pos) = world.get::<TilePos>(pod) {
        empty_pod(world, pod, pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_and_dirs() {
        assert_eq!(shape_of("/obj/structure/transit_tube", TUBE_ROOT), Some(""));
        assert_eq!(
            shape_of("/obj/structure/transit_tube/diagonal/topleft", TUBE_ROOT),
            Some("/diagonal")
        );
        assert_eq!(
            shape_of(
                "/obj/structure/transit_tube/station/reverse/flipped",
                TUBE_ROOT
            ),
            Some("/station/reverse/flipped")
        );
        assert_eq!(shape_of("/obj/structure/transit_tube_pod", TUBE_ROOT), None);
        assert_eq!(tube_dirs("", W), vec![E, W]);
        assert_eq!(tube_dirs("/crossing", N), vec![N, S]);
        assert_eq!(tube_dirs("/curved/flipped", W), vec![NE, W]);
        assert_eq!(tube_dirs("/junction", N), vec![N, SE, SW]);
        assert_eq!(tube_dirs("/station", S), vec![E, W]);
        assert_eq!(tube_dirs("/station/dispenser/reverse", E), vec![S]);
    }

    #[test]
    fn exits_within_45_degrees() {
        let junction = TransitTube::new("/junction", N);
        assert_eq!(junction.get_exit(N), N);
        // Coming south into a north junction: both diagonals are near, the last one wins.
        assert_eq!(junction.get_exit(S), SW);
        let curve = TransitTube::new("/curved", N);
        assert!(curve.has_entrance(S));
        assert_eq!(curve.get_exit(S), SW);
        assert_eq!(curve.get_exit(E), 0);
        let station = TubeStation::new("/station", S);
        assert_eq!(station.boarding_dir, N);
        assert_eq!(station.icon_state(), "closed_station0");
        let flipped = TubeStation::new("/station/flipped", S);
        assert_eq!(flipped.boarding_dir, S);
        let dispenser = TubeStation::new("/station/dispenser", S);
        assert_eq!(dispenser.icon_state(), "open_dispenser0");
    }
}
