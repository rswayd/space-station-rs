//! Disposals (server), after tg `code/modules/recycling/disposal/` and `sortingmachinery.dm`.
//!
//! - At startup the map's pipes become [`DisposalPipe`]s ([`pipes::Pipe`]: kind, `dir`,
//!   `dpdir`), mail sorting helpers add their tags to the junction on their tile, trunks link the
//!   bin, chute or outlet above them (`getlinked`, `trunk_check`).
//! - Bins ([`DisposalBin`], `bin.rs`): items go in by click, throw (75%) and storage dumps, mobs
//!   by drag (2 s) or by climbing in; tg `process()` every 2 s pressurizes the bin's own air from
//!   the room (`SEND_PRESSURE`, needs area equipment power), auto-flushes full bins with contents
//!   every 30 ticks, and flushes when the handle is engaged; the tgui `DisposalUnit` window.
//!   Delivery chutes flush whatever enters them, from the front.
//! - A flush (`flush()`: 1.5 s, then a holder) moves the bin's contents and air into a
//!   [`DisposalHolder`], an off-map entity that moves one pipe per tg tick (`world.tick_lag`,
//!   0.05 s; at most 1000 steps) through the pipes' `transfer`/`nextdir`. Junctions split at
//!   random, sorting junctions divert by `destinationTag` (mail) or `tomail` (wrap). It leaves
//!   at an outlet (2 s later, thrown `eject_range` tiles out of its front), at a bin (thrown
//!   around it), where the pipe ends (thrown 10 tiles on) or where its 1000 steps run out
//!   (thrown around; tg stops it there until a mob struggles out).
//! - Mobs inside a bin or a holder are [`InCloset`] (as in closets: kept on the map, hidden, the
//!   view follows): a step climbs out of a bin that is not flushing, and "clongs" in a pipe.
//! - Mail (`mail.rs`): the destination tagger (tgui `DestinationTagger`) sets parcels'
//!   `sort_tag`, which a flushed parcel gives its holder; the package wrapper wraps items.
//!
//! Cost: the network is the map's pipe entities found through the spatial index (no graph
//! build; startup only links trunks and sort tags). Per tick, each active holder takes
//! one step per 0.05 s: a tile lookup and a `nextdir`; a holder with a mob also scans the
//! hidden mobs ([`InCloset`], a handful) to move its own. Bins process every
//! 2 s (one tile air read and removal each while charging).
// shortcut: no RPD building (constructs come from the dispenser), no multi-z trunks, no pipe damage, stopped holders or struggling out of them (a holder out of
// steps expels at once), broken pipe ends do not pop the floor tile; add with pipe
// deconstruction.

mod bin;
pub mod build;
mod mail;
pub mod pipes;

use crate::sound::PlaySound;
use ss13_mapload::sound::files;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use ss13_atmos::GasMixture;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmMailSort, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, UiSystems};
use ss13_world::{SpatialIndex, WorldSystems};

pub use bin::{DisposalBin, DisposalUnitAct, DisposalUnitWindow, SEND_PRESSURE};
pub use mail::{Delivery, DestTagger, DestTaggerWindow, TAGGER_LOCATIONS};
pub use pipes::{Cargo, Pipe, PipeKind, SortFilter};

use crate::closets::InCloset;
use crate::storage::{Stored, take_out};

pub const PIPE_ROOT: &str = "/obj/structure/disposalpipe";
pub const MACHINE_ROOT: &str = "/obj/machinery/disposal";
pub const CHUTE_ROOT: &str = "/obj/machinery/disposal/delivery_chute";
pub const OUTLET_ROOT: &str = "/obj/structure/disposaloutlet";

/// `world.tick_lag`: `move_disposals` moves a holder one pipe per tick.
pub const MOVE_DELAY_SECS: f64 = 0.05;
/// `/obj/structure/disposalholder::count`.
pub const HOLDER_STEPS: u32 = 1000;
/// `SORT_TYPE_DISPOSALS`: what `newHolderDestination` tags every holder with.
pub const SORT_TYPE_DISPOSALS: u8 = 1;
/// Holder steps one frame may catch up after a stall (`SSmovement` skips late fires).
const MAX_CATCH_UP: u32 = 3;
/// `/obj/proc/pipe_eject` defaults, and `disposalpipe/expel` with a direction.
const PIPE_EJECT_RANGE: u32 = 5;
const BREAK_EJECT_RANGE: u32 = 10;
/// `get_ranged_target_turf(src, dir, 10)` of outlets and broken pipes.
const EJECT_TARGET_DISTANCE: i32 = 10;
/// Outlets wait `2 SECONDS` before throwing (`addtimer(expel_holder)`).
const OUTLET_DELAY_SECS: f64 = 2.0;
/// `EJECT_SPEED_SLOW`, `EJECT_RANGE_SLOW`.
const EJECT_SPEED_SLOW: f32 = 1.0;
const EJECT_RANGE_SLOW: u32 = 2;

/// A pipe on the map.
#[derive(Component, Clone, Debug)]
pub struct DisposalPipe {
    pub pipe: Pipe,
    /// A trunk's bin, chute or outlet (`linked`).
    pub linked: Option<Entity>,
}

/// `/obj/structure/disposaloutlet`.
#[derive(Component, Clone, Debug)]
pub struct DisposalOutlet {
    /// BYOND direction bits it throws towards.
    pub dir: u8,
    pub eject_range: u32,
    pub eject_speed: f32,
    pub trunk: Option<Entity>,
    /// The `outlet-open` flick ends then.
    open_until: Option<f64>,
    /// tg `start_eject`: when it last buzzed, seconds.
    start_eject: Option<f64>,
    sprites: [Option<ss13_shared::IconStateId>; 2],
}

/// `/obj/structure/disposalholder`: a flush in transit. Off the map; its items are
/// [`crate::storage::StoredIn`] it, its mobs [`InCloset`] it and kept on its tile.
#[derive(Component, Clone, Debug)]
pub struct DisposalHolder {
    /// The pipe it is in (`loc`).
    pub pipe: Entity,
    pub pos: TilePos,
    /// BYOND direction bits of its movement ([`pipes::DOWN`] when it just left a bin).
    pub dir: u8,
    pub gas: GasMixture,
    /// Moving; false when waiting at an outlet.
    pub active: bool,
    /// Steps left.
    pub count: u32,
    pub destination_tag: u8,
    pub tomail: bool,
    pub hasmob: bool,
    /// Waiting at this outlet until the time, seconds.
    pub ejecting: Option<(Entity, f64)>,
    /// The outlet buzzed for it, so it leaves with a hiss (`expel_holder(H, TRUE)`).
    pub eject_hiss: bool,
}

impl DisposalHolder {
    fn cargo(&self) -> Cargo {
        Cargo {
            destination_tag: self.destination_tag,
            tomail: self.tomail,
        }
    }
}

/// What startup found.
#[derive(Resource, Clone, Debug, Default)]
pub struct DisposalReport {
    pub pipes: usize,
    pub sorting: usize,
    pub bins: usize,
    pub chutes: usize,
    pub outlets: usize,
    /// Trunks with a bin, chute or outlet above them.
    pub linked_trunks: usize,
    /// Bins and chutes without a trunk (they never pressurize).
    pub unconnected: usize,
    pub build_secs: f64,
}

/// Holders delivered, for tests and logs.
#[derive(Resource, Clone, Debug, Default)]
pub struct DisposalStats {
    pub flushes: u64,
    pub steps: u64,
    pub delivered: u64,
    /// Wall time spent moving holders.
    pub move_secs: f64,
}

/// Time owed to the 2 s machine tick and to holder steps.
#[derive(Resource, Debug, Default)]
struct DisposalClock {
    machines: f64,
    moves: f64,
}

/// Timed steps (`crate::construction::Job::Disposal`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisposalJob {
    /// The target mob into `bin` (`stuff_mob_in`).
    StuffMob { bin: Entity },
    /// Unwrapping a held parcel (`attempt_pre_unwrap_contents`).
    Unwrap { parcel: Entity },
}

pub struct DisposalsServerPlugin;

impl Plugin for DisposalsServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A destination tagger opens its window, a parcel is unwrapped.
        app.add_click_handler(ClickPhase::AttackSelf, 1300, |w, c| {
            crate::disposals::attack_self(w, c.client, c.mob, c.target)
        });
        // A tagger on a parcel, the package wrapper on an item, in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 1300, |w, c| {
            c.held.is_some_and(|h| {
                crate::disposals::mail_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // tg `attack_hand` of disposal bins (the `DisposalUnit` window).
        app.add_click_handler(ClickPhase::EmptyHand, 2500, |w, c| {
            crate::disposals::interact(w, c.client, c.mob, c.target, c.modifiers)
        });
        // tg `item_interaction` of disposal bins (items go in), parcels (tagging) and
        // `interact_with_atom` of the package wrapper.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4200, |w, c| {
            c.held.is_some_and(|h| {
                crate::disposals::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.init_resource::<DisposalReport>()
            .init_resource::<DisposalStats>()
            .init_resource::<DisposalClock>()
            .register_ui::<DisposalUnitWindow>()
            .register_ui::<DestTaggerWindow>()
            .register_ui::<build::DisposalDispenserWindow>()
            .add_systems(ss13_world::MapInit, init_disposals)
            .add_systems(
                Update,
                (
                    (bin::relay_moves, bin::chute_bumps, tick)
                        .chain()
                        .after(WorldSystems::Movement)
                        .before(UiSystems::Open),
                    bin::apply_acts.in_set(UiSystems::Handle),
                    mail::apply_acts.in_set(UiSystems::Handle),
                    build::dispenser_acts.in_set(UiSystems::Handle),
                    build::init_dispensers,
                    (bin::window_status, mail::window_status).in_set(UiSystems::StatusHook),
                ),
            )
            .add_observer(mail::attach)
            .add_observer(bin::release_on_despawn);
    }
}

/// Pipes, bins, chutes and outlets from the map; sort helpers' tags; trunk links.
fn init_disposals(world: &mut World) {
    let start = Instant::now();
    let mut report = DisposalReport::default();
    let mut helpers: Vec<(Entity, TilePos, Vec<u8>)> = Vec::new();
    let mut q = world.query_filtered::<(Entity, &TilePos, &DmMailSort), (Without<DmObj>, With<ss13_world::Mapped>)>();
    for (e, &pos, sort) in q.iter(world) {
        helpers.push((e, pos, sort.sort_types.clone()));
    }
    let mut found = Vec::new();
    let mut q = world.query_filtered::<(Entity, &DmObj, &DmAtom, &TilePos, Option<&DmMailSort>), With<ss13_world::Mapped>>();
    for (e, obj, atom, &pos, sort) in q.iter(world) {
        found.push((
            e,
            obj.path.clone(),
            atom.clone(),
            pos,
            sort.map(|s| s.sort_types.clone()).unwrap_or_default(),
        ));
    }
    found.sort_by_key(|f| f.0);
    let icons = world.get_resource::<IconIndex>();
    let mut pipes = Vec::new();
    let mut bins = Vec::new();
    let mut outlets = Vec::new();
    for (e, path, atom, pos, tags) in found {
        if let Some(pipe) = Pipe::from_type(&path, atom.dir, &tags) {
            pipes.push((e, pos, pipe));
        } else if is_under(&path, MACHINE_ROOT) {
            let chute = is_under(&path, CHUTE_ROOT);
            bins.push((e, pos, bin::DisposalBin::new(chute, &path, &atom, icons)));
        } else if is_under(&path, OUTLET_ROOT) {
            outlets.push((e, pos, outlet_state(&atom, icons)));
        }
    }
    // `mail_sorting/LateInitialize`: the first mail junction on the helper's tile gets the tag.
    for (helper, pos, tags) in helpers {
        if let Some((_, _, pipe)) = pipes
            .iter_mut()
            .find(|(_, p, pipe)| *p == pos && matches!(pipe.kind, PipeKind::Sorting(_)))
            && let PipeKind::Sorting(SortFilter::Mail(list)) = &mut pipe.kind
        {
            for t in tags {
                if !list.contains(&t) {
                    list.push(t);
                }
            }
        }
        world.despawn(helper);
    }
    // `trunk/getlinked`: a disposal machine on the tile, then an outlet (which wins).
    for (e, pos, pipe) in pipes {
        let mut linked = None;
        if pipe.kind == PipeKind::Trunk {
            if let Some((b, _, state)) = bins.iter_mut().find(|(_, p, _)| *p == pos) {
                linked = Some(*b);
                state.trunk = Some(e);
            }
            if let Some((o, _, state)) = outlets.iter_mut().find(|(_, p, _)| *p == pos) {
                linked = Some(*o);
                state.trunk = Some(e);
            }
            report.linked_trunks += usize::from(linked.is_some());
        }
        report.pipes += 1;
        report.sorting += usize::from(matches!(pipe.kind, PipeKind::Sorting(_)));
        world.entity_mut(e).insert(DisposalPipe { pipe, linked });
    }
    for (e, _, mut state) in bins {
        report.bins += usize::from(!state.chute);
        report.chutes += usize::from(state.chute);
        // `trunk_check`: no trunk, no pump and no handle.
        if state.trunk.is_none() {
            report.unconnected += 1;
            state.pressure_charging = false;
            state.flush = false;
        }
        world.entity_mut(e).insert(state);
        bin::refresh_look(world, e);
    }
    report.outlets = outlets.len();
    for (e, _, state) in outlets {
        world.entity_mut(e).insert(state);
    }
    report.build_secs = start.elapsed().as_secs_f64();
    info!(
        "disposals: {} pipes ({} sorting), {} bins, {} chutes, {} outlets, {} linked trunks, {} \
         machines without a trunk, built in {:.2} ms",
        report.pipes,
        report.sorting,
        report.bins,
        report.chutes,
        report.outlets,
        report.linked_trunks,
        report.unconnected,
        report.build_secs * 1000.0
    );
    world.insert_resource(report);
}

/// A new outlet (`disposaloutlet/Initialize`).
pub(crate) fn outlet_state(atom: &DmAtom, icons: Option<&IconIndex>) -> DisposalOutlet {
    let sprite = |s: &str| icons.and_then(|i| i.get(&atom.icon, s));
    DisposalOutlet {
        dir: if atom.dir == 0 {
            pipes::SOUTH
        } else {
            atom.dir
        },
        eject_range: EJECT_RANGE_SLOW,
        eject_speed: EJECT_SPEED_SLOW,
        trunk: None,
        open_until: None,
        start_eject: None,
        sprites: [sprite(&atom.icon_state), sprite("outlet-open")],
    }
}

/// Bins' 2 s process, finished flushes, holder steps, outlet throws.
fn tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let dt = world.resource::<Time>().delta_secs_f64();
    bin::init_air(world);
    let machine_ticks = {
        let mut clock = world.resource_mut::<DisposalClock>();
        clock.machines += dt;
        let mut n = 0;
        while clock.machines >= crate::power::MACHINE_TICK_SECS && n < MAX_CATCH_UP {
            clock.machines -= crate::power::MACHINE_TICK_SECS;
            n += 1;
        }
        clock.machines = clock.machines.min(crate::power::MACHINE_TICK_SECS);
        n
    };
    for _ in 0..machine_ticks {
        bin::process(world, now, crate::power::MACHINE_TICK_SECS);
    }
    bin::finish_flushes(world, now);
    let steps = {
        let mut clock = world.resource_mut::<DisposalClock>();
        clock.moves += dt;
        let n = ((clock.moves / MOVE_DELAY_SECS) as u32).min(MAX_CATCH_UP);
        clock.moves = (clock.moves - f64::from(n) * MOVE_DELAY_SECS).min(MOVE_DELAY_SECS);
        n
    };
    if steps > 0 {
        move_holders(world, steps, now);
    }
    outlet_throws(world, now);
}

fn holders(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<Entity, With<DisposalHolder>>();
    let mut list: Vec<Entity> = q.iter(world).collect();
    list.sort();
    list
}

fn move_holders(world: &mut World, steps: u32, now: f64) {
    let start = Instant::now();
    for holder in holders(world) {
        for _ in 0..steps {
            if !world
                .get::<DisposalHolder>(holder)
                .is_some_and(|h| h.active)
            {
                break;
            }
            step(world, holder, now);
        }
    }
    world.resource_mut::<DisposalStats>().move_secs += start.elapsed().as_secs_f64();
}

/// One `move_loop/disposal_holder/move` (`transfer` of the current pipe) and `try_expel`.
fn step(world: &mut World, holder: Entity, now: f64) {
    let Some(h) = world.get::<DisposalHolder>(holder).cloned() else {
        return;
    };
    world.resource_mut::<DisposalStats>().steps += 1;
    if h.count == 0 {
        // The move loop timed out (`movement_stop`).
        // shortcut: tg stops the holder there and a mob inside struggles out (`struggle_free`:
        // 20 s, 5-15 brute, the pipe breaks and `spew_forth` expels everything); without pipe
        // damage nothing could free a mob that cannot struggle, and later flushes would pile
        // into the stopped holder, so it expels at once, as `spew_forth` does.
        for mob in mobs_in(world, holder) {
            crate::chem::tell_mob(
                world,
                mob,
                Span::Notice,
                "Вы постепенно останавливаетесь и вываливаетесь из трубы.",
            );
        }
        expel(world, holder, h.pos, 0, now);
        return;
    }
    let Some(pipe) = world.get::<DisposalPipe>(h.pipe).cloned() else {
        // The pipe is gone: everything falls out where it was.
        expel(world, holder, h.pos, 0, now);
        return;
    };
    if pipe.pipe.kind == PipeKind::Trunk && h.dir != pipes::DOWN {
        match pipe.linked {
            Some(o) if world.get::<DisposalOutlet>(o).is_some() => {
                outlet_expel(world, holder, o, now)
            }
            Some(b) if world.get::<bin::DisposalBin>(b).is_some() => bin::expel(world, b, holder),
            _ => expel(world, holder, h.pos, h.dir, now),
        }
        return;
    }
    let coin = world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(50.0);
    let dir = pipe.pipe.next_dir(h.dir, h.cargo(), || coin);
    let (dx, dy) = pipes::offset(dir);
    let next = TilePos::new(h.pos.x + dx, h.pos.y + dy, h.pos.z);
    {
        let mut h = world.get_mut::<DisposalHolder>(holder).expect("checked");
        h.dir = dir;
        h.count -= 1;
    }
    let Some(next_pipe) = find_pipe(world, next, dir) else {
        // `try_expel`: no pipe on: out of the last pipe, onwards.
        expel(world, holder, h.pos, dir, now);
        return;
    };
    {
        let mut h = world.get_mut::<DisposalHolder>(holder).expect("checked");
        h.pipe = next_pipe;
        h.pos = next;
    }
    if h.hasmob {
        for mob in mobs_in(world, holder) {
            world.entity_mut(mob).insert(next);
        }
    }
}

/// `findpipe`: a pipe on `pos` with an opening towards where the holder comes from.
fn find_pipe(world: &World, pos: TilePos, dir: u8) -> Option<Entity> {
    let index = world.resource::<SpatialIndex>();
    if !index.in_bounds(pos) {
        return None;
    }
    let from = pipes::reverse(dir);
    index.on_tile(pos).iter().copied().find(|&e| {
        world
            .get::<DisposalPipe>(e)
            .is_some_and(|p| p.pipe.dpdir & from != 0)
    })
}

/// Living mobs inside `container` (a bin or a holder).
pub(crate) fn mobs_in(world: &mut World, container: Entity) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &InCloset)>();
    let mut mobs: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.closet == container)
        .map(|(e, _)| e)
        .collect();
    mobs.sort();
    mobs
}

fn items_in(world: &World, container: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(container)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

/// The tile `distance` steps from `pos` in `dir` (`get_ranged_target_turf`, not clamped: a
/// throw stops at the map edge anyway).
fn ranged_target(pos: TilePos, dir: u8, distance: i32) -> TilePos {
    let (dx, dy) = pipes::offset(dir);
    TilePos::new(pos.x + dx * distance, pos.y + dy * distance, pos.z)
}

/// `get_offset_target_turf(loc, rand(5) - rand(5), rand(5) - rand(5))`.
fn random_target(world: &mut World, pos: TilePos) -> TilePos {
    let mut rng = world.resource_mut::<crate::combat::CombatRng>();
    let dx = rng.range(0, 5) as i32 - rng.range(0, 5) as i32;
    let dy = rng.range(0, 5) as i32 - rng.range(0, 5) as i32;
    TilePos::new(pos.x + dx, pos.y + dy, pos.z)
}

/// `/obj/proc/pipe_eject`: `holder`'s items and mobs onto `origin`, then (with `throw`) thrown
/// at `target`, or each at a random tile around when there is none.
fn pipe_eject(
    world: &mut World,
    holder: Entity,
    origin: TilePos,
    throw: Option<(Option<TilePos>, u32, f32)>,
    now: f64,
) -> Vec<Entity> {
    let mut out = Vec::new();
    for item in items_in(world, holder) {
        take_out(world, item, origin);
        out.push(item);
    }
    for mob in mobs_in(world, holder) {
        bin::let_out(world, mob);
        world.entity_mut(mob).insert(origin);
        out.push(mob);
    }
    let Some((target, range, speed)) = throw else {
        return out;
    };
    for &thing in &out {
        let target = target.unwrap_or_else(|| random_target(world, origin));
        if let Some(path) = crate::throw::Trajectory::new(origin, target, range, speed, now * 10.0)
        {
            world.entity_mut(thing).insert(crate::throw::Thrown {
                path,
                thrower: None,
                target: None,
                last_dir: None,
            });
        }
    }
    out
}

/// `disposalholder/vent_gas` and `qdel`.
fn vent_and_delete(world: &mut World, holder: Entity, at: TilePos) {
    if let Some(h) = world.get::<DisposalHolder>(holder).cloned()
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        atmos.merge_air(at, &h.gas);
    }
    world.despawn(holder);
}

/// `disposalpipe/expel`: out of a pipe at `at` (a break or a trunk with nothing above it),
/// thrown 10 tiles on in `dir`, or around when there is no direction.
fn expel(world: &mut World, holder: Entity, at: TilePos, dir: u8, now: f64) {
    let throw = if dir & (pipes::NORTH | pipes::SOUTH | pipes::EAST | pipes::WEST) != 0 {
        (
            Some(ranged_target(at, dir, EJECT_TARGET_DISTANCE)),
            BREAK_EJECT_RANGE,
            1.0,
        )
    } else {
        (None, PIPE_EJECT_RANGE, 1.0)
    };
    pipe_eject(world, holder, at, Some(throw), now);
    vent_and_delete(world, holder, at);
    world.resource_mut::<DisposalStats>().delivered += 1;
}

/// `disposaloutlet/expel`: the holder waits 2 s at the outlet, then is thrown out; the warning
/// buzzer sounds unless it did in the last 3 s, and then the holder leaves with a hiss.
// shortcut: bins play no flush or hiss sounds; add with the bin's `flush()` timing.
fn outlet_expel(world: &mut World, holder: Entity, outlet: Entity, now: f64) {
    let Some(mut o) = world.get_mut::<DisposalOutlet>(outlet) else {
        return;
    };
    o.open_until = Some(now + OUTLET_DELAY_SECS);
    let buzz = o.start_eject.is_none_or(|t| t + 3.0 < now);
    if buzz {
        o.start_eject = Some(now);
    }
    if let Some(mut h) = world.get_mut::<DisposalHolder>(holder) {
        h.active = false;
        h.ejecting = Some((outlet, now + OUTLET_DELAY_SECS));
        h.eject_hiss = buzz;
    }
    if buzz && let Some(&pos) = world.get::<TilePos>(outlet) {
        let sound = PlaySound::new(pos, files::WARNING_BUZZER, 50.0, false);
        crate::sound::playsound(world, sound);
    }
    refresh_outlet_look(world, outlet, true);
}

fn refresh_outlet_look(world: &mut World, outlet: Entity, open: bool) {
    let Some(o) = world.get::<DisposalOutlet>(outlet) else {
        return;
    };
    let Some(state) = o.sprites[usize::from(open)].or(o.sprites[0]) else {
        return;
    };
    if let Some(mut appearance) = world.get_mut::<ss13_shared::Appearance>(outlet)
        && appearance.icon_state != state
    {
        appearance.icon_state = state;
    }
}

/// `expel_holder`: holders whose 2 s at an outlet are over are thrown out of its front.
fn outlet_throws(world: &mut World, now: f64) {
    let mut q = world.query::<(Entity, &DisposalHolder)>();
    let mut due: Vec<(Entity, Entity)> = q
        .iter(world)
        .filter_map(|(e, h)| h.ejecting.filter(|&(_, at)| now >= at).map(|(o, _)| (e, o)))
        .collect();
    due.sort();
    for (holder, outlet) in due {
        let Some(o) = world.get::<DisposalOutlet>(outlet).cloned() else {
            // The outlet is gone: out where the holder is.
            let pos = world.get::<DisposalHolder>(holder).map(|h| h.pos);
            if let Some(pos) = pos {
                expel(world, holder, pos, 0, now);
            }
            continue;
        };
        let Some(&pos) = world.get::<TilePos>(outlet) else {
            continue;
        };
        if world
            .get::<DisposalHolder>(holder)
            .is_some_and(|h| h.eject_hiss)
        {
            crate::sound::playsound(world, PlaySound::new(pos, files::HISS, 50.0, false));
        }
        let target = ranged_target(pos, o.dir, EJECT_TARGET_DISTANCE);
        pipe_eject(
            world,
            holder,
            pos,
            Some((Some(target), o.eject_range, o.eject_speed)),
            now,
        );
        vent_and_delete(world, holder, pos);
        world.resource_mut::<DisposalStats>().delivered += 1;
    }
    // The `outlet-open` flicks that are over.
    let mut q = world.query::<(Entity, &DisposalOutlet)>();
    let closing: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| o.open_until.is_some_and(|t| now >= t))
        .map(|(e, _)| e)
        .collect();
    for outlet in closing {
        world
            .get_mut::<DisposalOutlet>(outlet)
            .expect("queried")
            .open_until = None;
        refresh_outlet_look(world, outlet, false);
    }
}

pub use bin::{hitby, mouse_drop_receive};
pub use mail::attack_self;

/// tg `attack_hand` of bins (the `DisposalUnit` window) and the disposal pipe dispenser.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    modifiers: ss13_shared::protocol::ClickModifiers,
) -> bool {
    build::dispenser_interact(world, client, mob, target)
        || bin::interact(world, client, mob, target, modifiers)
}

/// Disposal bins first (any item goes in), then the mail interactions.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ss13_shared::protocol::ClickModifiers,
) -> bool {
    bin::item_interaction(world, client, mob, held, target, modifiers)
        || mail::item_interaction(world, client, mob, held, target)
}

/// A tagger or the package wrapper used on an item in the other hand.
pub fn mail_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    mail::item_interaction(world, client, mob, held, target)
}

/// End of a [`DisposalJob`] `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: DisposalJob) {
    match job {
        DisposalJob::StuffMob { bin } => bin::complete_stuff(world, mob, action.target, bin),
        DisposalJob::Unwrap { parcel } => mail::complete_unwrap(world, mob, parcel),
    }
}

/// `flush()` of a bin or chute now (what a chute does when something enters it).
pub fn start_flush(world: &mut World, bin: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    bin::start_flush(world, bin, now);
}
