//! Moving between stacked z-levels (server): tg `zMove`/`can_z_move` (`atoms_movable.dm`),
//! `zFall`/`zImpact` (`turf.dm`), open space (`turf/open/openspace`), stairs
//! (`structures/stairs.dm`), ladders (`structures/ladders.dm`) and the up/down keys
//! (`/mob/proc/up`, `down`, `keybinding/movement.dm`).
//!
//! - Levels stack as the map loader says ([`ss13_mapload::MapLoadReport::stacked_levels`]).
//! - A tile lets things out of it up only from an open turf, down only from open space; into
//!   it down onto an open turf, up only into open space (`zPassOut`/`zPassIn`); objects with
//!   `BLOCK_Z_*` flags (lattices, catwalks, ladders) close those ways.
//! - Whatever lands on open space falls (unless it flies, is anchored, is thrown, or there is
//!   no gravity) and keeps falling while there is open space below; a living mob takes
//!   `(5 * levels) ^ 1.5` brute (split over its legs when standing) and is knocked down
//!   `levels * 5` s. The last stair of a flight catches a one-level fall.
//! - The last stair of a flight (`isTerminator`) does not let anyone off its far side
//!   ([`ss13_world::Border`] facing its `dir`, which the client predicts): the bump carries the
//!   climber up onto the tile beyond it on the level above, if it is not open space.
//! - Ladders link to the ladders straight above and below: a click climbs up, a right click
//!   down, in a second (ghosts at once); the pulled thing comes along.
//! - PgUp/PgDn: a ghost goes through the floors; a living mob uses the ladder it stands on, or
//!   flies up or down in a second when it can fly (no gravity, wings).
// shortcut: gravity is everywhere but in space areas (no gravity generators, no
// `NEGATIVE_GRAVITY`); no wings, free running or cat-like landings; a mob tumbling down stairs
// is not pushed along (`move_towards`); no ladder radial menu (click again), no stair
// indicators or minimap blips, no ladder holes; buckled mobs do not move with what they are
// buckled to.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_chat::Span;
use ss13_mapload::{DmAtom, DmItem, DmName, DmObj, DmTurfBuild, IconIndex, Turf};
use ss13_net::chat::server::SendChat;
use ss13_net::server::IntentReceived;
use ss13_shared::chat::ChatChannel;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_world::{Border, Bumped, SpatialIndex, WorldSystems};

use crate::health::{BodyPosition, DamageType, Health, Stat};
use crate::obj_damage::{Anchored, is_under};
use crate::server::Players;
use crate::tools::{Actor, tell};
use crate::zone::BodyZone;

pub const STAIRS: &str = "/obj/structure/stairs";
pub const LADDER: &str = "/obj/structure/ladder";
const OPENSPACE: [&str; 2] = ["/turf/open/openspace", "/turf/open/space/openspace"];
const OPEN_TURF: &str = "/turf/open";
/// Glass floors (`/turf/open/floor/glass`, reinforced and plasma ones): transparent.
const GLASS_FLOOR: &str = "/turf/open/floor/glass";

/// `obj_flags`: this object stops falls out of its tile, rises out of it, falls into it from
/// above and rises into it from below.
pub const BLOCK_Z_OUT_DOWN: u32 = 1 << 5;
pub const BLOCK_Z_OUT_UP: u32 = 1 << 6;
pub const BLOCK_Z_IN_DOWN: u32 = 1 << 7;
pub const BLOCK_Z_IN_UP: u32 = 1 << 8;

/// Ladder `travel_time`, and the climb of `/mob/proc/up`, deciseconds.
const CLIMB_DS: f32 = 10.0;

pub use ss13_shared::LevelStack as ZLevels;

/// A flight of stairs: travel in `dir` goes up.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stairs {
    pub dir: Dir,
    /// `isTerminator`: no stair of the same `dir` ahead; leaving it ahead climbs.
    pub terminator: bool,
}

/// A ladder and the ladders it links to.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ladder {
    pub up: Option<Entity>,
    pub down: Option<Entity>,
}

/// `z_move_flags`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZMoveFlags {
    /// `ZMOVE_FALL_CHECKS`: not while flying, thrown or weightless.
    pub fall_checks: bool,
    /// `ZMOVE_CAN_FLY_CHECKS`: only while flying or weightless.
    pub can_fly_checks: bool,
    /// `ZMOVE_IGNORE_OBSTACLES`.
    pub ignore_obstacles: bool,
    /// `ZMOVE_ALLOW_ANCHORED`.
    pub allow_anchored: bool,
    /// `ZMOVE_INCLUDE_PULLED`.
    pub include_pulled: bool,
    /// `ZMOVE_FEEDBACK`: the refusal is told to the mover's client.
    pub feedback: Option<Entity>,
}

impl ZMoveFlags {
    /// `ZMOVE_FALL_FLAGS`.
    pub const FALL: Self = Self {
        fall_checks: true,
        can_fly_checks: false,
        ignore_obstacles: false,
        allow_anchored: false,
        include_pulled: false,
        feedback: None,
    };
    /// `ZMOVE_STAIRS_FLAGS` (and the `ZMOVE_ALLOW_BUCKLED` checks of `stair_ascend`).
    pub const STAIRS: Self = Self {
        fall_checks: false,
        can_fly_checks: false,
        ignore_obstacles: false,
        allow_anchored: false,
        include_pulled: false,
        feedback: None,
    };
}

/// The turf entity on `pos`.
fn turf_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Turf>(e).is_some())
}

fn turf_path(world: &World, pos: TilePos) -> Option<&str> {
    turf_at(world, pos)
        .and_then(|t| world.get::<DmTurfBuild>(t))
        .map(|b| b.path.as_str())
}

/// tg `TRAIT_TURF_TRANSPARENT` of a turf type: open space and glass floors show the level below.
pub fn transparent_turf(path: &str) -> bool {
    OPENSPACE.iter().any(|root| is_under(path, root)) || is_under(path, GLASS_FLOOR)
}

/// Files whether the level below shows through `pos` ([`SpatialIndex::transparent`], which
/// replication reads), after its turf changed.
pub fn refresh_transparency(world: &mut World, pos: TilePos) {
    // Worlds without a map (unit tests) have nothing to file.
    if !world.contains_resource::<SpatialIndex>() || !world.contains_resource::<ZLevels>() {
        return;
    }
    let transparent = turf_path(world, pos).is_some_and(transparent_turf);
    if world.resource::<SpatialIndex>().transparent(pos) != transparent {
        world
            .resource_mut::<SpatialIndex>()
            .set_transparent(pos, transparent);
    }
    let above = world.resource::<ZLevels>().step(pos, true);
    air_link(world, pos);
    refresh_fall(world, pos);
    if let Some(above) = above {
        air_link(world, above);
        refresh_fall(world, above);
    }
}

/// Files [`ss13_world::FallsThrough`] on the turf of `pos`: open space a walker falls through
/// (`can_z_move(DOWN, ZMOVE_FALL_FLAGS)` of a plain walker), which the client predicts.
fn refresh_fall(world: &mut World, pos: TilePos) {
    let Some(turf) = turf_at(world, pos) else {
        return;
    };
    let falls = is_openspace(world, pos)
        && has_gravity(world, pos)
        && z_pass_out(world, pos, false)
        && world
            .get_resource::<ZLevels>()
            .and_then(|z| z.step(pos, false))
            .is_some_and(|below| z_pass_in(world, below, false));
    let has = world.get::<ss13_world::FallsThrough>(turf).is_some();
    if falls && !has {
        world.entity_mut(turf).insert(ss13_world::FallsThrough);
    } else if !falls && has {
        world.entity_mut(turf).remove::<ss13_world::FallsThrough>();
    }
}

/// tg `zAirOut`/`zAirIn`: air moves between open space and the open turf under it (a wall under
/// it closes the way). Files or drops the link of `upper` ([`ss13_atmos::Atmos::set_vertical`]).
fn air_link(world: &mut World, upper: TilePos) {
    let Some(lower) = world
        .get_resource::<ZLevels>()
        .and_then(|z| z.step(upper, false))
    else {
        return;
    };
    let open_space = is_openspace(world, upper);
    // Space under space open space: vacuum both ways, nothing ever moves.
    let space = |p: Option<&str>| p.is_some_and(|p| is_under(p, "/turf/open/space"));
    let lower_path = turf_path(world, lower);
    let lower_open = lower_path.is_some_and(|p| is_under(p, OPEN_TURF))
        && !(space(turf_path(world, upper)) && space(lower_path));
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.set_vertical(upper, lower, open_space && lower_open);
    }
}

/// `isopenspaceturf`.
pub fn is_openspace(world: &World, pos: TilePos) -> bool {
    turf_path(world, pos).is_some_and(|p| OPENSPACE.iter().any(|root| is_under(p, root)))
}

/// The `obj_flags` of the objects on `pos`, or'ed.
fn obj_flags(world: &World, pos: TilePos) -> u32 {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter_map(|&e| world.get::<DmObj>(e))
        .fold(0, |acc, o| acc | o.obj_flags)
}

/// `turf/zPassOut(direction)`.
pub fn z_pass_out(world: &World, pos: TilePos, up: bool) -> bool {
    let Some(path) = turf_path(world, pos) else {
        return false;
    };
    let flags = obj_flags(world, pos);
    if OPENSPACE.iter().any(|root| is_under(path, root)) {
        let block = if up { BLOCK_Z_OUT_UP } else { BLOCK_Z_OUT_DOWN };
        return flags & block == 0;
    }
    is_under(path, OPEN_TURF) && up && flags & BLOCK_Z_OUT_UP == 0
}

/// `turf/zPassIn(direction)`.
pub fn z_pass_in(world: &World, pos: TilePos, up: bool) -> bool {
    let Some(path) = turf_path(world, pos) else {
        return false;
    };
    let flags = obj_flags(world, pos);
    if OPENSPACE.iter().any(|root| is_under(path, root)) {
        let block = if up { BLOCK_Z_IN_UP } else { BLOCK_Z_IN_DOWN };
        return flags & block == 0;
    }
    is_under(path, OPEN_TURF) && !up && flags & BLOCK_Z_IN_DOWN == 0
}

/// `has_gravity(turf)`.
pub fn has_gravity(world: &World, pos: TilePos) -> bool {
    !crate::antag::nukeop::bomb::is_space_area(world, pos)
}

/// `movement_type & (FLYING | FLOATING)`: ghosts, the AI's eye, flying basic mobs.
pub fn flies(world: &World, e: Entity) -> bool {
    world.get::<crate::ghost::Ghost>(e).is_some()
        || world.get::<crate::silicon::ai::AiEye>(e).is_some()
        || world
            .get::<crate::mobs::BasicMob>(e)
            .is_some_and(|m| m.def.flying)
}

/// tg `can_z_move`: the destination, or `None` (told to `flags.feedback`).
pub fn can_z_move(
    world: &mut World,
    e: Entity,
    up: bool,
    start: TilePos,
    destination: Option<TilePos>,
    flags: ZMoveFlags,
) -> Option<TilePos> {
    let refuse = |world: &mut World, text: &str| {
        if let Some(client) = flags.feedback {
            tell(world, client, Span::Warning, text);
        }
        None
    };
    let Some(destination) = destination.or_else(|| world.resource::<ZLevels>().step(start, up))
    else {
        return refuse(world, "Туда не пройти!");
    };
    let flying = flies(world, e);
    let gravity = has_gravity(world, start);
    let thrown = world.get::<crate::throw::Thrown>(e).is_some();
    if flags.fall_checks && (thrown || flying || !gravity) {
        return None;
    }
    if flags.can_fly_checks && !flying && gravity {
        return refuse(world, "Вы не Супермен.");
    }
    // `observer/can_z_move`: ghosts pass through floors.
    let ignore = flags.ignore_obstacles || world.get::<crate::ghost::Ghost>(e).is_some();
    let passes = ignore || (z_pass_out(world, start, up) && z_pass_in(world, destination, up));
    let anchored = world.get::<Anchored>(e).is_some_and(|a| a.0);
    if !passes || (!flags.allow_anchored && anchored) {
        return refuse(world, "Туда не пройти!");
    }
    Some(destination)
}

/// tg `zMove(target)`: `e` (and with `include_pulled` what it pulls, and on) onto `target`.
/// A puller left behind lets go (`check_pulls`).
pub fn z_move(world: &mut World, e: Entity, target: TilePos, include_pulled: bool) {
    let mut moving = vec![e];
    if include_pulled {
        let mut at = e;
        while let Some(&crate::pull::Pulling(next)) = world.get::<crate::pull::Pulling>(at) {
            if moving.contains(&next) {
                break;
            }
            moving.push(next);
            at = next;
        }
    }
    for m in moving {
        world.entity_mut(m).insert(target);
    }
}

fn name_of(world: &World, e: Entity) -> String {
    if let Some(name) = world.get::<Name>(e) {
        return name.as_str().to_owned();
    }
    world
        .get::<DmName>(e)
        .map_or_else(|| "что-то".to_owned(), |n| n.name.clone())
}

fn send(world: &mut World, messages: Vec<SendChat>) {
    for m in messages {
        world.write_message(m);
    }
}

/// tg `zFall`: `e` on open space drops a level (and on, while there is open space below).
/// True when it fell.
pub fn z_fall(world: &mut World, e: Entity, levels: u32) -> bool {
    let Some(&start) = world.get::<TilePos>(e) else {
        return false;
    };
    if !is_openspace(world, start) {
        return false;
    }
    let Some(target) = can_z_move(world, e, false, start, None, ZMoveFlags::FALL) else {
        return false;
    };
    z_move(world, e, target, false);
    z_impact(world, e, target, levels, start);
    true
}

/// tg `zImpact`: the last stair of a flight catches a one-level fall; open space below goes
/// on falling; else the faller hits the floor (`onZImpact`).
fn z_impact(world: &mut World, e: Entity, at: TilePos, levels: u32, from: TilePos) {
    let stairs = world
        .resource::<SpatialIndex>()
        .on_tile(at)
        .iter()
        .copied()
        .find(|&s| world.get::<Stairs>(s).is_some_and(|s| s.terminator));
    let intercepted = levels == 1 && stairs.is_some();
    let name = name_of(world, e);
    if !intercepted {
        let turf = turf_at(world, from)
            .map(|t| name_of(world, t))
            .unwrap_or_default();
        let lines = crate::pull::visible_message(
            world,
            from,
            ChatChannel::Info,
            (Span::Danger, format!("{name} падает сквозь {turf}!")),
            &[],
        );
        send(world, lines);
    }
    if !intercepted && z_fall(world, e, levels + 1) {
        return;
    }
    if intercepted {
        // `stairs/intercept_zImpact`: the unconscious tumble down instead of walking.
        if world.get::<Stat>(e).is_some_and(|s| *s > Stat::SoftCrit) {
            crate::status_effects::apply(world, e, crate::status_effects::Incap::Paralyze, 2.0);
            crate::status_effects::knockdown(world, e, 5.0, 0.0);
            // shortcut: `rand(4, 8)` taken as its mean.
            hurt(world, e, None, 6.0);
        }
        return;
    }
    on_z_impact(world, e, at, levels);
}

/// `onZImpact` and, for the living, `ZImpactDamage`.
fn on_z_impact(world: &mut World, e: Entity, at: TilePos, levels: u32) {
    let turf = turf_at(world, at)
        .map(|t| name_of(world, t))
        .unwrap_or_default();
    let name = name_of(world, e);
    let living = world.get::<Health>(e).is_some();
    // `ZImpactDamage`: a freerunner (the quirk) who can brace takes a short fall unscathed.
    // shortcut: no cyberspine, wings, `TRAIT_CATLIKE_INSTINCT` landing or spin emote.
    let incap = crate::status_effects::incapacitation(world, e);
    if living
        && levels <= 1
        && !incap.incapacitated
        && !incap.knocked_out
        && crate::quirks::has(world, e, crate::quirks::Quirk::Freerunning)
    {
        crate::status_effects::knockdown(world, e, levels as f32 * 4.0, 0.0);
        let lines = crate::pull::visible_message(
            world,
            at,
            ChatChannel::Info,
            (
                Span::Notice,
                format!(
                    "{name} жестко падает на {turf} и не получает урона, группируясь и делая \
                     перекат."
                ),
            ),
            &[(
                e,
                Span::Notice,
                format!(
                    "Вы готовитесь к падению. Вы жестко падаете на {turf} и не получаете урона, \
                     группируясь и делая перекат."
                ),
            )],
        );
        send(world, lines);
        return;
    }
    let lines = crate::pull::visible_message(
        world,
        at,
        ChatChannel::Info,
        (Span::Danger, format!("{name} врезается в {turf}!")),
        &[(e, Span::BoldWarning, format!("Вы врезаетесь в {turf}!"))],
    );
    send(world, lines);
    if !living {
        return;
    }
    let damage = (levels as f32 * 5.0).powf(1.5);
    let standing = world.get::<BodyPosition>(e).is_none_or(|b| !b.lying());
    if standing {
        let leg = (damage / 2.0).round();
        hurt(world, e, Some(BodyZone::LLeg), leg);
        hurt(world, e, Some(BodyZone::RLeg), leg);
    } else {
        hurt(world, e, None, damage);
    }
    crate::status_effects::knockdown(world, e, levels as f32 * 5.0, 0.0);
}

fn hurt(world: &mut World, e: Entity, zone: Option<BodyZone>, amount: f32) {
    let Ok(mut entity) = world.get_entity_mut(e) else {
        return;
    };
    let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>() else {
        return;
    };
    match zone {
        Some(zone) => {
            crate::health::apply_damage_zone(
                &mut health,
                &mut stat,
                zone,
                DamageType::Brute,
                amount,
            );
        }
        None => {
            crate::health::apply_damage(&mut health, &mut stat, DamageType::Brute, amount);
        }
    }
}

/// Whatever comes to rest on open space falls: walked, dropped, spawned or mapped there
/// (`openspace/Entered`, `initialize_occupant`, `on_atom_created`).
pub fn fall_through_openspace(world: &mut World) {
    // A spawned mob gets its `Health` after its tile: that counts as arriving too.
    let mut q = world.query_filtered::<
        (Entity, Has<Health>, Has<DmItem>, Has<DmObj>),
        Or<(Changed<TilePos>, Added<Health>)>,
    >();
    let moved: Vec<Entity> = q
        .iter(world)
        .filter(|&(_, living, item, obj)| living || item || obj)
        .map(|(e, ..)| e)
        .collect();
    let fallers: Vec<Entity> = moved
        .into_iter()
        .filter(|&e| {
            world
                .get::<TilePos>(e)
                .is_some_and(|&p| is_openspace(world, p))
                && world.get::<Turf>(e).is_none()
                && world
                    .get::<DmObj>(e)
                    .is_none_or(|o| !is_under(&o.path, "/obj/effect"))
        })
        .collect();
    for e in fallers {
        z_fall(world, e, 1);
    }
}

/// Stairs and ladders as the map placed them: which stairs end a flight (they get their
/// [`Border`] and merged sprites), which ladders link.
pub fn init_multiz(world: &mut World) {
    let mut levels = ZLevels::default();
    if let Some(report) = world.get_resource::<ss13_mapload::MapLoadReport>() {
        for (below, above) in report.stacked_levels() {
            levels.link(below, above);
        }
    }
    world.insert_resource(levels);
    let mut turfs = world.query_filtered::<(&TilePos, &DmTurfBuild), With<Turf>>();
    let transparent: Vec<TilePos> = turfs
        .iter(world)
        .filter(|(_, t)| transparent_turf(&t.path))
        .map(|(&p, _)| p)
        .collect();
    let mut index = world.resource_mut::<SpatialIndex>();
    for &pos in &transparent {
        index.set_transparent(pos, true);
    }
    for &pos in &transparent {
        air_link(world, pos);
        refresh_fall(world, pos);
    }
    let air_links = world
        .get_resource::<ss13_atmos::Atmos>()
        .map_or(0, |a| a.vertical_links());
    let mut q = world.query::<(Entity, &DmObj, &TilePos, Option<&Facing>)>();
    let found: Vec<(Entity, String, TilePos, Dir)> = q
        .iter(world)
        .filter(|(_, o, ..)| is_under(&o.path, STAIRS) || is_under(&o.path, LADDER))
        .map(|(e, o, &p, f)| (e, o.path.clone(), p, f.map_or(Dir::South, |f| f.0)))
        .collect();
    let stairs: Vec<(Entity, TilePos, Dir)> = found
        .iter()
        .filter(|(_, path, ..)| is_under(path, STAIRS))
        .map(|&(e, _, p, d)| (e, p, d))
        .collect();
    for &(e, pos, dir) in &stairs {
        let ahead = pos.step(dir);
        let terminator = !stairs.iter().any(|&(_, p, d)| p == ahead && d == dir);
        let mut entity = world.entity_mut(e);
        entity.insert(Stairs { dir, terminator });
        if terminator {
            // `on_exit_stairs` and `Cross`: not off its far side, not onto it from there.
            entity.insert((
                ss13_world::NonDenseBorder,
                Border::facing(dir, false),
                ss13_world::ClimbsUp(dir),
            ));
        }
    }
    stair_sprites(world, &stairs);
    let ladders: Vec<(Entity, TilePos)> = found
        .iter()
        .filter(|(_, path, ..)| is_under(path, LADDER))
        .map(|&(e, _, p, _)| (e, p))
        .collect();
    let zl = world.resource::<ZLevels>().clone();
    for &(e, pos) in &ladders {
        let find = |up: bool| {
            let there = zl.step(pos, up)?;
            ladders.iter().find(|(_, p)| *p == there).map(|(l, _)| *l)
        };
        let ladder = Ladder {
            up: find(true),
            down: find(false),
        };
        world.entity_mut(e).insert(ladder);
        ladder_sprite(world, e);
    }
    info!(
        "multi-z: {} stacked levels, {} stairs, {} ladders, {} transparent tiles, {} air links",
        zl.links.len(),
        stairs.len(),
        ladders.len(),
        transparent.len(),
        air_links
    );
}

/// `stairs/update_icon_state`: plain stairs (`has_merged_sprites`) beside others of their `dir`
/// show the left, middle or right piece.
fn stair_sprites(world: &mut World, stairs: &[(Entity, TilePos, Dir)]) {
    for &(e, pos, dir) in stairs {
        let Some(atom) = world.get::<DmAtom>(e).cloned() else {
            continue;
        };
        if atom.icon_state != "stairs" {
            continue;
        }
        let side = |turn: Dir| {
            let p = pos.step(turn);
            stairs.iter().any(|&(_, q, d)| q == p && d == dir)
        };
        // DM `turn(dir, 90)` is counterclockwise.
        let (left, right) = (side(turn(dir, false)), side(turn(dir, true)));
        let state = match (left, right) {
            (true, true) => "stairs-m",
            (true, false) => "stairs-r",
            (false, true) => "stairs-l",
            (false, false) => continue,
        };
        set_state(world, e, &atom.icon, state);
    }
}

/// A cardinal turned a quarter, clockwise or not.
fn turn(dir: Dir, clockwise: bool) -> Dir {
    let order = [Dir::North, Dir::East, Dir::South, Dir::West];
    let i = order.iter().position(|&d| d == dir).unwrap_or(0);
    order[if clockwise { (i + 1) % 4 } else { (i + 3) % 4 }]
}

/// `ladder/update_icon_state`: `ladder[up][down]`.
fn ladder_sprite(world: &mut World, e: Entity) {
    let (Some(atom), Some(l)) = (world.get::<DmAtom>(e).cloned(), world.get::<Ladder>(e)) else {
        return;
    };
    let state = format!(
        "ladder{}{}",
        u8::from(l.up.is_some()),
        u8::from(l.down.is_some())
    );
    set_state(world, e, &atom.icon, &state);
}

fn set_state(world: &mut World, e: Entity, icon: &str, state: &str) {
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(icon, state))
    else {
        return;
    };
    if let Some(mut a) = world.get_mut::<Appearance>(e)
        && a.icon_state != id
    {
        a.icon_state = id;
    }
}

/// `stairs/stair_ascend`: a bump on the far side of the last stair from on it.
pub fn climb_stairs(world: &mut World, mut bumps: Local<MessageCursor<Bumped>>) {
    let pending: Vec<Bumped> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    for Bumped { entity, obstacle } in pending {
        let Some(stairs) = world.get::<Stairs>(obstacle).copied() else {
            continue;
        };
        let (Some(&at), Some(&pos)) =
            (world.get::<TilePos>(obstacle), world.get::<TilePos>(entity))
        else {
            continue;
        };
        if !stairs.terminator || pos != at || world.get::<crate::ghost::Ghost>(entity).is_some() {
            continue;
        }
        let Some(checking) = world.resource::<ZLevels>().step(at, true) else {
            continue;
        };
        if can_z_move(world, entity, true, at, Some(checking), ZMoveFlags::STAIRS).is_none() {
            continue;
        }
        let target = checking.step(stairs.dir);
        // Not onto open space: it would only drop them back.
        if can_z_move(world, entity, false, target, None, ZMoveFlags::FALL).is_some() {
            continue;
        }
        z_move(world, entity, target, false);
        // The pulled thing follows onto the stairs.
        if let Some(&crate::pull::Pulling(pulled)) = world.get::<crate::pull::Pulling>(entity) {
            world.entity_mut(pulled).insert(at);
        }
    }
}

/// Which way a ladder use goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Climb {
    pub up: bool,
}

/// `ladder/use`: from next to it, by a click (`up`) or a right click (down). True when the click
/// was a ladder's.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Option<Entity>,
    target: Entity,
    right: bool,
) -> bool {
    let Some(ladder) = world.get::<Ladder>(target).copied() else {
        return false;
    };
    let up = !right;
    if ladder.up.is_none() && ladder.down.is_none() {
        tell(world, client, Span::Warning, "никуда не ведет!");
        return true;
    }
    if (up && ladder.up.is_none()) || (!up && ladder.down.is_none()) {
        let way = if up {
            "подниматься наверх"
        } else {
            "спускаться вниз"
        };
        tell(world, client, Span::Warning, format!("больше некуда {way}"));
        return true;
    }
    let way = if up {
        "поднимается наверх..."
    } else {
        "спускается вниз..."
    };
    let lines = crate::pull::visible_message(
        world,
        world.get::<TilePos>(mob).copied().unwrap_or_default(),
        ChatChannel::Info,
        (Span::Notice, format!("{} {way}", name_of(world, mob))),
        &[],
    );
    send(world, lines);
    let actor = Actor {
        client,
        mob,
        item: held,
    };
    crate::tools::do_after(
        world,
        actor,
        target,
        CLIMB_DS,
        0,
        crate::construction::Job::Climb(Climb { up }),
    );
    true
}

/// `ladder/travel`: the climb is done.
pub fn complete_climb(world: &mut World, mob: Entity, ladder: Entity, climb: Climb) {
    travel(world, mob, ladder, climb.up);
}

fn travel(world: &mut World, mob: Entity, ladder: Entity, up: bool) -> bool {
    let Some(l) = world.get::<Ladder>(ladder).copied() else {
        return false;
    };
    let Some(to) = (if up { l.up } else { l.down }) else {
        return false;
    };
    let (Some(&from), Some(&target)) = (world.get::<TilePos>(ladder), world.get::<TilePos>(to))
    else {
        return false;
    };
    z_move(world, mob, target, true);
    let way = if up {
        "поднимается наверх"
    } else {
        "спускается вниз"
    };
    let ladder_name = name_of(world, ladder);
    let text = format!("{} {way} по {ladder_name}.", name_of(world, mob));
    let mut lines =
        crate::pull::visible_message(world, from, ChatChannel::Info, (Span::Notice, text), &[]);
    lines.extend(crate::pull::visible_message(
        world,
        target,
        ChatChannel::Info,
        (Span::Notice, format!("{} {way}", name_of(world, mob))),
        &[],
    ));
    send(world, lines);
    true
}

/// A ghost's click (`attack_ghost` -> `ghost_use`): a ladder takes it at once, up when it can.
#[derive(Message, Clone, Copy, Debug)]
pub struct GhostClick {
    pub client: Entity,
    pub ghost: Entity,
    pub target: Entity,
}

pub fn ghost_clicks(world: &mut World, mut clicks: Local<MessageCursor<GhostClick>>) {
    let pending: Vec<GhostClick> = clicks
        .read(world.resource::<Messages<GhostClick>>())
        .copied()
        .collect();
    for GhostClick {
        client,
        ghost,
        target,
    } in pending
    {
        let Some(l) = world.get::<Ladder>(target).copied() else {
            continue;
        };
        match (l.up, l.down) {
            (None, None) => tell(world, client, Span::Warning, "никуда не ведет!"),
            (Some(_), _) => {
                travel(world, ghost, target, true);
            }
            (None, Some(_)) => {
                travel(world, ghost, target, false);
            }
        }
    }
}

/// PgUp / PgDn (`/mob/proc/up`, `down`; `observer/up`, `down`).
pub fn move_vertical_intents(world: &mut World, mut intents: Local<MessageCursor<IntentReceived>>) {
    let pending: Vec<(Entity, bool)> = intents
        .read(world.resource::<Messages<IntentReceived>>())
        .filter_map(|i| match i.intent {
            ClientIntent::MoveUp => Some((i.client, true)),
            ClientIntent::MoveDown => Some((i.client, false)),
            _ => None,
        })
        .collect();
    for (client, up) in pending {
        let Some(mob) = world.resource::<Players>().mob(client) else {
            continue;
        };
        let Some(&pos) = world.get::<TilePos>(mob) else {
            continue;
        };
        let feedback = ZMoveFlags {
            feedback: Some(client),
            ..ZMoveFlags::default()
        };
        if world.get::<crate::ghost::Ghost>(mob).is_some() {
            if let Some(target) = can_z_move(world, mob, up, pos, None, feedback) {
                z_move(world, mob, target, false);
                let text = if up {
                    "Вы двигаетесь вверх."
                } else {
                    "Вы двигаетесь вниз."
                };
                tell(world, client, Span::Notice, text);
            }
            continue;
        }
        if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            continue;
        }
        // The ladder underfoot.
        let ladder = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Ladder>(e).is_some());
        if let Some(ladder) = ladder {
            interact(world, client, mob, None, ladder, !up);
            continue;
        }
        let flight = ZMoveFlags {
            can_fly_checks: true,
            ..feedback
        };
        if can_z_move(world, mob, up, pos, None, flight).is_none() {
            continue;
        }
        let text = if up {
            "двигаетесь вверх..."
        } else {
            "двигаетесь вниз..."
        };
        tell(world, client, Span::Notice, text);
        let actor = Actor {
            client,
            mob,
            item: None,
        };
        crate::tools::do_after(
            world,
            actor,
            mob,
            CLIMB_DS,
            0,
            crate::construction::Job::Fly(Climb { up }),
        );
    }
}

/// `/mob/proc/up` after its second: `zMove(ZMOVE_FLIGHT_FLAGS | ZMOVE_FEEDBACK)`.
pub fn complete_flight(world: &mut World, client: Entity, mob: Entity, climb: Climb) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let flags = ZMoveFlags {
        can_fly_checks: true,
        include_pulled: true,
        feedback: Some(client),
        ..ZMoveFlags::default()
    };
    if let Some(target) = can_z_move(world, mob, climb.up, pos, None, flags) {
        z_move(world, mob, target, true);
        let text = if climb.up {
            "Вы двигаетесь вверх."
        } else {
            "Вы двигаетесь вниз."
        };
        tell(world, client, Span::Notice, text);
    }
}

pub struct MultiZPlugin;

impl Plugin for MultiZPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` / `attack_hand_secondary` of a ladder: climb up or down.
        app.add_click_handler(ClickPhase::EmptyHand, 1700, |w, c| {
            crate::multiz::interact(w, c.client, c.mob, None, c.target, c.modifiers.right)
        });
        // tg `ladder/item_interaction(_secondary)`: out of combat mode, a climb.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3900, |w, c| {
            !w.get::<crate::combat::CombatMode>(c.mob)
                .is_some_and(|m| m.0)
                && crate::multiz::interact(w, c.client, c.mob, c.held, c.target, c.modifiers.right)
        });
        app.init_resource::<ZLevels>()
            .add_message::<GhostClick>()
            // Air links need the levels' air (`init_atmos`).
            .add_systems(PostStartup, init_multiz.after(ss13_atmos::init_atmos))
            .add_systems(
                Update,
                (
                    climb_stairs,
                    fall_through_openspace,
                    ghost_clicks,
                    move_vertical_intents,
                )
                    .chain()
                    .after(WorldSystems::Movement)
                    .after(crate::click::ResolveClicks)
                    // The climb or fall goes out with the step's ack, not a tick later.
                    .before(crate::server::ack_moves)
                    // The view follows the mob to its new level in the same tick: one tick
                    // off it, replication despawns the mob and spawns it anew on the client.
                    .before(crate::server::follow_mobs),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_link_both_ways() {
        let mut z = ZLevels::default();
        z.link(1, 2);
        let p = TilePos::new(5, 6, 1);
        assert_eq!(z.step(p, true), Some(TilePos::new(5, 6, 2)));
        assert_eq!(z.step(TilePos::new(5, 6, 2), false), Some(p));
        assert_eq!(z.step(p, false), None);
    }

    #[test]
    fn fall_damage_follows_tg() {
        // `(levels * 5) ** 1.5`.
        assert!(((1.0f32 * 5.0).powf(1.5) - 11.18).abs() < 0.01);
        assert!(((2.0f32 * 5.0).powf(1.5) - 31.62).abs() < 0.01);
    }
}
