//! Immovable Rod (`code/modules/events/immovable_rod/`): a rod comes in from a random side of
//! the station's level, 13 tiles in, flying at the edge of the map behind a random station
//! turf, one tile every 0.1 s (`move_towards`, as the meteors: [`super::meteors`]). It phases
//! through everything: a wall it enters is devastated (`SSexplosions.highturf`), every dense
//! object on its tile is destroyed (`take_damage(INFINITY)`), every living mob there is
//! penetrated (`penetrate()`: 100 brute over the body, 60 to the chest, a heavy `ex_act` when
//! dense or 10% of the time). It is gone at its destination or off the map.
// shortcut: no admin aim or forced looping, no ghost orbit or kill counter in its name, no
// suplex; two rods meeting both vanish (no singularity is ported).

use bevy::prelude::*;
use ss13_mapload::{DmName, DmObj, IconIndex};
use ss13_net::server::Networked;
use ss13_shared::appearance::{Layer, MouseOpacity};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_world::{Density, SpatialIndex};

use super::controller::{EventRng, EventState, RoundEvent};
use super::meteors::{Towards, debris_start};
use crate::construction::turf_at;
use crate::explosions::Severity;
use crate::explosions::ex_act::{ex_act_mob, ex_act_movable, ex_act_turf, tile_contents};
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::obj_damage::{INDESTRUCTIBLE, Integrity};
use crate::zone::BodyZone;

pub const NAME: &str = "Immovable Rod";
const ICON: &str = "icons/obj/anomaly.dmi";
const ICON_STATE: &str = "immrod";
/// `move_towards` with the default `delay = 1` decisecond.
const MOVE_SECS: f64 = 0.1;
/// `/obj/effect` on `GAME_PLANE`, `OBJ_LAYER`.
const GAME_PLANE: i16 = -4;
const OBJ_LAYER: f32 = 3.0;

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(RodEvent)
}

struct RodEvent;

impl RoundEvent for RodEvent {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 5;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        super::priority_announce(world, "Общая тревога", "Что это за хуйня?!");
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        launch(world);
    }
}

/// `/obj/effect/immovablerod`.
#[derive(Component, Debug)]
pub struct ImmovableRod {
    pub dest: TilePos,
    towards: Towards,
    next_move: f64,
    /// `num_mobs_hit`: living mobs penetrated.
    pub mobs_hit: u32,
}

/// `/datum/round_event/immovable_rod/start()`: from a random side (`GLOB.cardinals`: north,
/// south, east, west) at the map edge opposite to it, behind a random station turf.
pub fn launch(world: &mut World) -> Option<Entity> {
    let side = world.resource_mut::<EventRng>().0.range(0, 3) as usize;
    let target = super::electrical_storm::random_station_turf(world)?;
    let (w, h) = world.resource::<SpatialIndex>().level_size(target.z)?;
    // `get_edge_target_turf(target, REVERSE_DIR(startside))`.
    let (x, y) = match side {
        0 => (target.x, 1),
        1 => (target.x, h as i32),
        2 => (1, target.y),
        _ => (w as i32, target.y),
    };
    let end = TilePos::new(x, y, target.z);
    let start = debris_start(
        side,
        target.z,
        (w, h),
        &mut world.resource_mut::<EventRng>().0,
    )?;
    Some(spawn_rod(world, start, end))
}

/// `new /obj/effect/immovablerod(start, end)`.
pub fn spawn_rod(world: &mut World, start: TilePos, end: TilePos) -> Entity {
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ICON, ICON_STATE))
        .unwrap_or(IconStateId(u32::MAX));
    let now = world.resource::<Time>().elapsed_secs_f64();
    world
        .spawn((
            Name::new("/obj/effect/immovablerod"),
            DmName {
                name: "immovable rod".to_owned(),
                desc: "What the fuck is that?".to_owned(),
            },
            start,
            Appearance {
                icon_state: icon,
                layer: Layer(OBJ_LAYER),
                plane: GAME_PLANE,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                overlays: vec![],
                mouse_opacity: MouseOpacity::Icon,
            },
            Density,
            Networked,
            ImmovableRod {
                dest: end,
                towards: Towards::new(start, end),
                next_move: now + MOVE_SECS,
                mobs_hit: 0,
            },
        ))
        .id()
}

/// Moves every rod its due steps.
pub(super) fn move_rods(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query_filtered::<Entity, With<ImmovableRod>>();
    let rods: Vec<Entity> = q.iter(world).collect();
    for rod in rods {
        while let Some(r) = world.get::<ImmovableRod>(rod) {
            if r.next_move > now {
                break;
            }
            world.get_mut::<ImmovableRod>(rod).expect("rod").next_move += MOVE_SECS;
            step(world, rod);
        }
    }
}

/// One step: through whatever is there (`PHASING`), then `Moved()`.
fn step(world: &mut World, rod: Entity) {
    let (Some(&pos), Some(r)) = (world.get::<TilePos>(rod), world.get::<ImmovableRod>(rod)) else {
        return;
    };
    let next = r.towards.next(pos);
    if next == pos || !world.resource::<SpatialIndex>().in_bounds(next) {
        world.despawn(rod);
        return;
    }
    let dest = r.dest;
    world
        .get_mut::<ImmovableRod>(rod)
        .expect("rod")
        .towards
        .advance();
    world.entity_mut(rod).insert(next);
    // `on_entering_atom`: a dense turf is bumped.
    if turf_at(world, next).is_some_and(|t| world.get::<Density>(t).is_some()) {
        bump_sound(world, next);
        ex_act_turf(world, next, Severity::Devastate);
    }
    // `Moved()`: everything dense or living on the tile.
    let things: Vec<Entity> = tile_contents(world, next)
        .into_iter()
        .filter(|&e| {
            e != rod && (world.get::<Density>(e).is_some() || world.get::<Health>(e).is_some())
        })
        .collect();
    for thing in things {
        if world.get_entity(rod).is_err() {
            return;
        }
        bump(world, rod, thing, next);
    }
    if world.get_entity(rod).is_ok() && next == dest {
        world.despawn(rod);
    }
}

/// `/obj/effect/immovablerod/Bump(clong)` of a movable.
fn bump(world: &mut World, rod: Entity, clong: Entity, pos: TilePos) {
    if world.get_entity(clong).is_err() {
        return;
    }
    bump_sound(world, pos);
    if world.get::<ImmovableRod>(clong).is_some() {
        world.despawn(clong);
        world.despawn(rod);
        return;
    }
    if world.get::<Health>(clong).is_some() {
        penetrate(world, rod, clong, pos);
        return;
    }
    // `take_damage(INFINITY)`: an object without integrity here (machines, doors) goes
    // straight to `atom_destruction` -> `deconstruct(FALSE)`; an item meets a devastating
    // `ex_act` (its own integrity).
    let indestructible = world
        .get::<DmObj>(clong)
        .map(|o| o.resistance_flags & INDESTRUCTIBLE != 0);
    match indestructible {
        Some(true) => {}
        Some(false) if world.get::<Integrity>(clong).is_some() => {
            crate::obj_damage::take_damage_flagless(world, clong, f32::INFINITY);
        }
        Some(false) => crate::obj_damage::deconstruct(world, clong, false),
        None => ex_act_movable(world, clong, Severity::Devastate),
    }
}

/// 10%: `bang.ogg` and "You hear a CLANG!".
fn bump_sound(world: &mut World, pos: TilePos) {
    if world.resource_mut::<EventRng>().0.prob(10.0) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/bang.ogg", 50.0, true),
        );
    }
}

/// `penetrate(smeared_mob)`.
fn penetrate(world: &mut World, rod: Entity, mob: Entity, pos: TilePos) {
    let name = crate::security::name_of(world, mob);
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (
            Span::Danger,
            format!("{name} is penetrated by an immovable rod!"),
        ),
        &[(mob, Span::Danger, "The rod penetrates you!".to_owned())],
    );
    if world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead)
        && let Some(mut r) = world.get_mut::<ImmovableRod>(rod)
    {
        r.mobs_hit += 1;
    }
    brute(world, mob, None, 100.0);
    brute(world, mob, Some(BodyZone::Chest), 60.0);
    let dense = world.get::<Density>(mob).is_some();
    if dense || world.resource_mut::<EventRng>().0.prob(10.0) {
        ex_act_mob(world, mob, Severity::Heavy);
    }
}

/// `apply_damage(amount, BRUTE, zone)`, `None`: spread over the body.
fn brute(world: &mut World, mob: Entity, zone: Option<BodyZone>, amount: f32) {
    let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    else {
        return;
    };
    let mut new = stat;
    match zone {
        Some(zone) => {
            crate::health::apply_damage_zone(
                &mut health,
                &mut new,
                zone,
                DamageType::Brute,
                amount,
            );
        }
        None => {
            crate::health::apply_damage(&mut health, &mut new, DamageType::Brute, amount);
        }
    }
    world.entity_mut(mob).insert(health);
    if new != stat {
        world.entity_mut(mob).insert(new);
    }
}
