//! The shared teleport (server), after tg `code/datums/helper_datums/teleport.dm`:
//! [`do_teleport`], [`check_teleport_valid`] and [`valid_teleport_turf`].
//!
//! - Most effects (`/obj/effect`, but not chameleon dummies, wisps, mob spawners, immovable rods
//!   or meteors) are deleted instead of teleported.
//! - With a `precision` the landing is a random tile within that range of the destination that
//!   is not in a `NOTELEPORT` area or across a `LOCAL_TELEPORT` boundary (any tile when
//!   `forced`); without one, the destination itself.
//! - Unless `forced`: neither the origin's nor the landing's area may be `NOTELEPORT`, a
//!   `LOCAL_TELEPORT` area on either side keeps the teleport inside that area, and the mover may
//!   not have [`NoTeleport`] (`TRAIT_NO_TELEPORT`, the anti-teleportation implant). A refused mob
//!   is told "something holds you back!".
//! - Then `COMSIG_MOVABLE_TELEPORTING`, forced or not: a bluespace grounding implant
//!   (`crate::implants::teleport_blocked`) flings its host back.
//! - Ghosts just move. Others get the channel's sparks (bluespace: sparks, quantum: rainbow
//!   sparks, unless `no_effects`) and the optional sounds at both ends; a mob buckled to the
//!   teleported thing goes along (to the same landing when it can, else it is off its seat).
// shortcut: no `TRAIT_NO_TELEPORT` on turfs, no bag-of-holding bluespace interference (neither
// the trait nor its items are ported), no reservation check (no reserved levels), no map-edge
// transition turfs, no `COMSIG_ATOM_INTERCEPT_TELEPORTING`; things in a hand or a container (no
// tile) are not teleported. Upgrade with the first item or area that needs them.

use bevy::prelude::*;
use ss13_mapload::{Areas, DmPower};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use crate::effects::particles::{Kind, start};
use crate::obj_damage::is_under;

/// `TRAIT_NO_TELEPORT` on a mob (the anti-teleportation implant, `crate::implants`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoTeleport;

/// `area_flags`: `NOTELEPORT`, `LOCAL_TELEPORT`.
pub const NOTELEPORT: u32 = 1 << 2;
pub const LOCAL_TELEPORT: u32 = 1 << 15;
/// `do_teleport`'s `spark_spread(teleatom, 5, TRUE)`.
const SPARKS: u32 = 5;
/// `tele_play_specials`: `playsound(location, sound, 60, TRUE)`.
const SOUND_VOLUME: f32 = 60.0;

/// `TELEPORT_CHANNEL_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    #[default]
    Bluespace,
    Quantum,
    Wormhole,
    Magic,
    Cult,
    Free,
}

/// `do_teleport`'s optional arguments.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tele {
    /// 0: exactly onto the destination.
    pub precision: i32,
    pub channel: Channel,
    /// `no_effects`: no default sparks.
    pub no_effects: bool,
    /// Ignores `NOTELEPORT` and `LOCAL_TELEPORT`.
    pub forced: bool,
    /// `asoundin`, `asoundout`.
    pub sound_in: Option<&'static str>,
    pub sound_out: Option<&'static str>,
}

/// `delete_atoms` of `do_teleport`: effects other than these are deleted.
const KEPT_EFFECTS: [&str; 5] = [
    "/obj/effect/dummy/chameleon",
    "/obj/effect/wisp",
    "/obj/effect/mob_spawn",
    "/obj/effect/immovablerod",
    "/obj/effect/meteor",
];

/// The area at `pos` and its `area_flags` (none outside every area).
pub fn area_flags(world: &World, pos: TilePos) -> (Option<Entity>, u32) {
    let area = world.get_resource::<Areas>().and_then(|a| a.area_at(pos));
    let flags = area
        .and_then(|a| world.get::<DmPower>(a))
        .map_or(0, |p| p.area_flags as u32);
    (area, flags)
}

/// tg `check_teleport_valid`: from `from` to `dest` (on the map, no `NOTELEPORT` area at either
/// end, `LOCAL_TELEPORT` areas only within themselves).
pub fn check_teleport_valid(world: &World, from: TilePos, dest: TilePos) -> bool {
    if !world.resource::<SpatialIndex>().in_bounds(dest) {
        return false;
    }
    let (from_area, from_flags) = area_flags(world, from);
    let (dest_area, dest_flags) = area_flags(world, dest);
    if (from_flags | dest_flags) & NOTELEPORT != 0 {
        return false;
    }
    !((from_flags | dest_flags) & LOCAL_TELEPORT != 0 && from_area != dest_area)
}

/// tg `get_valid_teleport_turf(origin, center, range, skip_restrictions)`: a random tile within
/// `range` of `center`.
pub fn valid_teleport_turf(
    world: &mut World,
    origin: TilePos,
    center: TilePos,
    range: i32,
    skip_restrictions: bool,
) -> Option<TilePos> {
    let (origin_area, origin_flags) = area_flags(world, origin);
    let mut tiles = Vec::new();
    for dy in -range..=range {
        for dx in -range..=range {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            if !world.resource::<SpatialIndex>().in_bounds(pos) {
                continue;
            }
            if !skip_restrictions {
                let (area, flags) = area_flags(world, pos);
                if flags & NOTELEPORT != 0
                    || ((origin_flags | flags) & LOCAL_TELEPORT != 0 && origin_area != area)
                {
                    continue;
                }
            }
            tiles.push(pos);
        }
    }
    if tiles.is_empty() {
        return None;
    }
    let i = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, tiles.len() as u32 - 1) as usize;
    tiles.get(i).copied()
}

/// tg `do_teleport(teleatom, destination, ...)`: true when `mover` arrived.
pub fn do_teleport(world: &mut World, mover: Entity, dest: TilePos, tele: Tele) -> bool {
    if crate::construction::path_of(world, mover).is_some_and(|p| {
        is_under(&p, "/obj/effect") && !KEPT_EFFECTS.iter().any(|k| is_under(&p, k))
    }) {
        world.despawn(mover);
        return false;
    }
    let Some(&from) = world.get::<TilePos>(mover) else {
        return false;
    };
    let landing = if tele.precision > 0 {
        valid_teleport_turf(world, from, dest, tele.precision, tele.forced)
    } else if world.resource::<SpatialIndex>().in_bounds(dest) {
        Some(dest)
    } else {
        None
    };
    let valid = landing.is_some_and(|l| {
        tele.forced
            || (world.get::<NoTeleport>(mover).is_none() && check_teleport_valid(&*world, from, l))
    });
    let (Some(landing), true) = (landing, valid) else {
        if let Some(client) = world
            .get::<crate::server::PlayerMob>(mover)
            .map(|p| p.client)
        {
            crate::tools::tell(world, client, Span::Warning, "something holds you back!");
        }
        return false;
    };
    // `COMSIG_MOVABLE_TELEPORTING`.
    if crate::implants::teleport_blocked(world, mover) {
        return false;
    }
    if world.get::<crate::ghost::Ghost>(mover).is_some() {
        world.entity_mut(mover).insert(landing);
        return true;
    }
    let sparks = match tele.channel {
        _ if tele.no_effects => None,
        Channel::Bluespace => Some(Kind::Sparks),
        Channel::Quantum => Some(Kind::QuantumSparks),
        _ => None,
    };
    specials(world, from, sparks, tele.sound_in);
    // A mob buckled to a teleported object (checked before the object moves).
    let rider = if world.get::<crate::health::Health>(mover).is_none() {
        crate::medical::surgery::buckled_to(world, mover)
    } else {
        None
    };
    world.entity_mut(mover).insert(landing);
    specials(world, landing, sparks, tele.sound_out);
    if let Some(rider) = rider {
        let ride = Tele {
            no_effects: true,
            sound_in: None,
            sound_out: None,
            ..tele
        };
        // Landing elsewhere (or not at all) leaves the rider off its seat
        // (`surgery::check_buckles` frees a mob off its furniture's tile).
        if do_teleport(world, rider, landing, ride)
            && world.get::<TilePos>(rider) != Some(&landing)
            && let Some(client) = world
                .get::<crate::server::PlayerMob>(rider)
                .map(|p| p.client)
        {
            let name = crate::examine::name_of(world, mover);
            crate::tools::tell(
                world,
                client,
                Span::Warning,
                format!(
                    "As you reorient your senses, you realize you aren't riding {name} anymore!"
                ),
            );
        }
    }
    true
}

/// tg `tele_play_specials`.
fn specials(world: &mut World, at: TilePos, sparks: Option<Kind>, sound: Option<&'static str>) {
    if let Some(sound) = sound {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(at, sound, SOUND_VOLUME, true),
        );
    }
    if let Some(kind) = sparks {
        start(world, kind, SPARKS, true, at);
    }
}
