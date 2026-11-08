//! Blob structures after tg `/obj/structure/blob` (`structures/*.dm`): the normal blob, strong
//! and reflective shields, the core, nodes, factories and resource blobs; growth
//! (`expand`), pulses from the core and nodes (`pulse_area`, `Be_Pulsed`, `reinforce_area`),
//! eating the tile (`ConsumeTile`, `blob_act`) and damage (`run_atom_armor`, `take_damage`).
//!
//! A structure is a converted `/obj/structure/blob/...` object with [`Blob`] (its kind, its
//! overmind, cooldowns) and `crate::obj_damage::Integrity`. Every hit on one (items through
//! `obj_damage::attackby`, projectiles, explosions, fire) comes through [`damage`]: brute
//! times `brute_resist`, burn times `fire_resist`, then the kind's armor for the flag, then the
//! strain's `damage_reaction`.
// shortcut: reflective blobs do not reflect projectiles; a door hit by the blob is destroyed
// outright (no wreckage); other dense objects without ported integrity (closets, tables, most
// machines) block the blob; `blob_act` on items only destroys them on the blob's own tile;
// blob mobs pass blobs (`PASSBLOB`) only when they step, the AI's paths ignore it; the
// attack animation is not drawn.

use bevy::prelude::*;
use ss13_atmos::{AtmosBlocker, AtmosPass};
use ss13_mapload::{DmItem, DmObj, DmTurfBuild, HiddenUnderTile, IconIndex};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, Facing, Overlay, TilePos};
use ss13_world::{Density, PassFlagsSelf, SpatialIndex};

use super::strains::{self, Flag, Hurt, Strain};
use super::{BlobOvermind, PASSBLOB, is_ally, rand_u, roll, strain_of};
use crate::health::{DamageType, Health, Stat};
use crate::obj_damage::{Hit, INDESTRUCTIBLE, Integrity, is_under};

pub const BLOB_DMI: &str = "icons/mob/nonhuman-player/blob.dmi";
pub const BLOB_ROOT: &str = "/obj/structure/blob";
pub const NORMAL: &str = "/obj/structure/blob/normal";
pub const SHIELD: &str = "/obj/structure/blob/shield";
pub const SHIELD_CORE: &str = "/obj/structure/blob/shield/core";
pub const REFLECTIVE: &str = "/obj/structure/blob/shield/reflective";
pub const REFLECTIVE_CORE: &str = "/obj/structure/blob/shield/reflective/core";
pub const CORE: &str = "/obj/structure/blob/special/core";
pub const NODE: &str = "/obj/structure/blob/special/node";
pub const FACTORY: &str = "/obj/structure/blob/special/factory";
pub const RESOURCE: &str = "/obj/structure/blob/special/resource";
/// Every blob structure type the game spawns.
pub const BLOB_RUNTIME_TYPES: [&str; 9] = [
    NORMAL,
    SHIELD,
    SHIELD_CORE,
    REFLECTIVE,
    REFLECTIVE_CORE,
    CORE,
    NODE,
    FACTORY,
    RESOURCE,
];
const LATTICE: &str = "/obj/structure/lattice";
const DOOR: &str = "/obj/machinery/door";
const WALL: &str = "/turf/closed/wall";
const SPLAT: &str = "sound/effects/splat.ogg";
const ATTACK_SOUND: &str = "sound/effects/blob/attackblob.ogg";

/// `BLOB_REGULAR_HP_INIT`; a normal blob at or under 15 is "fragile".
pub const REGULAR_HP_INIT: f32 = 21.0;
const FRAGILE_HP: f32 = 15.0;
/// `Be_Pulsed`: pulse and heal cooldowns, seconds.
const PULSE_COOLDOWN: f64 = 1.0;
const HEAL_COOLDOWN: f64 = 2.0;
/// `BLOB_REINFORCE_CHANCE` (per second).
const REINFORCE_CHANCE: f32 = 2.5;
/// `BLOB_FACTORY_MAX_SPORES`, `BLOBMOB_SPORE_SPAWN_COOLDOWN`.
pub const FACTORY_MAX_SPORES: usize = 3;
pub const SPORE_COOLDOWN: f64 = 8.0;
/// `BLOB_RESOURCE_GATHER_DELAY`, `..._ADDED_DELAY`, `..._AMOUNT`.
const GATHER_DELAY: f64 = 4.0;
const GATHER_ADDED_DELAY: f64 = 0.25;
const GATHER_AMOUNT: f32 = 1.0;
/// `/obj/blob_act`: `take_damage(400, BRUTE, MELEE)`.
const OBJ_BLOB_ACT_DAMAGE: f32 = 400.0;

/// The structure types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Normal,
    /// `/obj/structure/blob/shield` ("strong blob"); `StrongCore` is the core's free one.
    Strong,
    StrongCore,
    Reflective,
    ReflectiveCore,
    Core,
    Node,
    Factory,
    Resource,
}

impl Kind {
    pub fn path(self) -> &'static str {
        match self {
            Kind::Normal => NORMAL,
            Kind::Strong => SHIELD,
            Kind::StrongCore => SHIELD_CORE,
            Kind::Reflective => REFLECTIVE,
            Kind::ReflectiveCore => REFLECTIVE_CORE,
            Kind::Core => CORE,
            Kind::Node => NODE,
            Kind::Factory => FACTORY,
            Kind::Resource => RESOURCE,
        }
    }

    /// `max_integrity`.
    pub fn max_hp(self) -> f32 {
        match self {
            Kind::Normal => 25.0,
            Kind::Strong | Kind::StrongCore | Kind::Reflective | Kind::ReflectiveCore => 150.0,
            Kind::Core => 400.0,
            Kind::Node | Kind::Factory => 200.0,
            Kind::Resource => 60.0,
        }
    }

    /// `health_regen` when pulsed (the core heals in its own process instead).
    pub fn regen(self) -> f32 {
        match self {
            Kind::Strong | Kind::StrongCore | Kind::Reflective | Kind::ReflectiveCore => 2.0,
            Kind::Core => 0.0,
            Kind::Node => 3.0,
            Kind::Normal | Kind::Factory | Kind::Resource => 1.0,
        }
    }

    /// `point_return`: what removing it gives back; negative: it cannot be removed.
    pub fn point_return(self) -> i32 {
        match self {
            Kind::Normal | Kind::StrongCore | Kind::ReflectiveCore => 0,
            Kind::Strong => 4,
            Kind::Reflective => 8,
            Kind::Core => -1,
            Kind::Node | Kind::Factory => 25,
            Kind::Resource => 15,
        }
    }

    /// The `armor_type` rating for `flag`, percent.
    pub fn armor(self, flag: Flag) -> f32 {
        // (fire, acid, laser)
        let (fire, acid, laser) = match self {
            Kind::Normal => (80.0, 70.0, 50.0),
            Kind::Strong | Kind::StrongCore | Kind::Reflective | Kind::ReflectiveCore => {
                (90.0, 90.0, 25.0)
            }
            Kind::Core => (75.0, 90.0, 0.0),
            Kind::Node => (65.0, 90.0, 25.0),
            Kind::Factory | Kind::Resource => (80.0, 70.0, 25.0),
        };
        match flag {
            Flag::Fire => fire,
            Flag::Acid => acid,
            Flag::Laser => laser,
            _ => 0.0,
        }
    }

    /// `brute_resist` at `integrity` (a fragile normal blob loses its bonus).
    pub fn brute_resist(self, integrity: f32) -> f32 {
        match self {
            Kind::Normal if integrity <= FRAGILE_HP => 0.5,
            Kind::Normal | Kind::Strong | Kind::StrongCore => 0.25,
            _ => 0.5,
        }
    }

    pub fn shield(self) -> bool {
        matches!(
            self,
            Kind::Strong | Kind::StrongCore | Kind::Reflective | Kind::ReflectiveCore
        )
    }

    pub fn reflective(self) -> bool {
        matches!(self, Kind::Reflective | Kind::ReflectiveCore)
    }

    /// (`claim_range`, `pulse_range`, `expand_range`, `strong_reinforce_range`) of the core and
    /// nodes.
    pub fn ranges(self) -> Option<(u32, u32, u32, u32)> {
        match self {
            Kind::Core => Some((12, 4, 3, 1)),
            Kind::Node => Some((10, 3, 2, 0)),
            _ => None,
        }
    }
}

/// On a blob structure.
#[derive(Component, Clone, Debug)]
pub struct Blob {
    pub kind: Kind,
    /// `overmind`; none: a dead blob, which a pulsing core may claim.
    pub overmind: Option<Entity>,
    pub max_integrity: f32,
    /// In a `BLOBS_ALLOWED` area: counts towards critical mass.
    pub legit: bool,
    pub pulse_at: f64,
    pub heal_at: f64,
    /// A factory's next spore, a resource blob's next gathering.
    pub next_at: f64,
    /// A factory's blobbernaut (`blobbernaut`, `is_creating_blobbernaut` is instant here).
    pub blobbernaut: Option<Entity>,
}

/// The blob structure on `pos`, if any.
pub fn blob_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Blob>(e).is_some())
}

/// Blob structures within `range` of `pos` (BYOND `range`), with their kinds.
pub fn blobs_in_range(world: &World, pos: TilePos, range: u32) -> Vec<(Entity, TilePos, Kind)> {
    let index = world.resource::<SpatialIndex>();
    let r = range as i32;
    let mut out = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            if !index.in_bounds(p) {
                continue;
            }
            for &e in index.on_tile(p) {
                if let Some(b) = world.get::<Blob>(e) {
                    out.push((e, p, b.kind));
                }
            }
        }
    }
    out
}

/// tg `is_valid_turf` and `BLOBS_ALLOWED` of the area at `pos`.
pub fn blobs_allowed(world: &World, pos: TilePos) -> bool {
    crate::teleport::area_flags(world, pos).1 & super::BLOBS_ALLOWED != 0
}

pub(super) fn now(world: &World) -> f64 {
    crate::tools::now(world)
}

/// A new structure of `kind` on `pos` for `overmind` (tg `new type(loc, overmind)`).
pub fn spawn_blob(
    world: &mut World,
    kind: Kind,
    pos: TilePos,
    overmind: Option<Entity>,
) -> Option<Entity> {
    let blob = crate::construction::spawn_obj(world, kind.path(), pos)?;
    let max = kind.max_hp();
    let current = if kind == Kind::Normal {
        REGULAR_HP_INIT
    } else {
        max
    };
    let dir = Dir::CARDINALS[rand_u(world, 0, 3) as usize];
    let legit = blobs_allowed(world, pos);
    world.entity_mut(blob).insert((
        Blob {
            kind,
            overmind,
            max_integrity: max,
            legit,
            pulse_at: 0.0,
            heal_at: 0.0,
            next_at: 0.0,
            blobbernaut: None,
        },
        Integrity {
            current,
            broken: false,
        },
        PassFlagsSelf(PASSBLOB),
        Facing(dir),
    ));
    if kind.shield() {
        world
            .entity_mut(blob)
            .insert(AtmosBlocker(AtmosPass::Density));
    }
    refresh_look(world, blob);
    crate::construction::tile_changed(world, pos);
    Some(blob)
}

/// tg `change_to`: the structure becomes `kind` (a new one on its tile, same overmind and
/// facing).
pub fn change_to(world: &mut World, blob: Entity, kind: Kind) -> Option<Entity> {
    let (Some(b), Some(&pos)) = (world.get::<Blob>(blob).cloned(), world.get::<TilePos>(blob))
    else {
        return None;
    };
    let facing = world.get::<Facing>(blob).copied();
    world.despawn(blob);
    let new = spawn_blob(world, kind, pos, b.overmind)?;
    if let Some(f) = facing {
        world.entity_mut(new).insert(f);
    }
    Some(new)
}

/// BYOND `BlendRGB(color, white, 0.5)`: an off-station blob is paler.
pub fn paler(color: [u8; 4]) -> [u8; 4] {
    let mix = |c: u8| ((u16::from(c) + 255) / 2) as u8;
    [mix(color[0]), mix(color[1]), mix(color[2]), color[3]]
}

/// The icon state of a structure at `integrity` (`update_icon_state`).
pub fn icon_state(kind: Kind, integrity: f32, max: f32) -> &'static str {
    let weak = integrity < max * 0.5;
    match kind {
        Kind::Normal if integrity <= FRAGILE_HP => "blob_damaged",
        Kind::Normal => "blob",
        Kind::Strong | Kind::StrongCore if weak => "blob_shield_damaged",
        Kind::Strong | Kind::StrongCore => "blob_shield",
        Kind::Reflective | Kind::ReflectiveCore if weak => "blob_glow_damaged",
        Kind::Reflective | Kind::ReflectiveCore => "blob_glow",
        Kind::Core | Kind::Node => "blank_blob",
        Kind::Factory => "blob_factory",
        Kind::Resource => "blob_resource",
    }
}

/// `update_icon`: the strain's colour (paler off station), the damage state, the core's and
/// nodes' overlays.
pub fn refresh_look(world: &mut World, blob: Entity) {
    let Some(b) = world.get::<Blob>(blob).cloned() else {
        return;
    };
    let integrity = world.get::<Integrity>(blob).map_or(0.0, |i| i.current);
    let color = b
        .overmind
        .and_then(|o| strain_of(world, o))
        .map(|s| if b.legit { s.color() } else { paler(s.color()) });
    let icon = |world: &World, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(BLOB_DMI, state))
    };
    let state = icon(world, icon_state(b.kind, integrity, b.max_integrity));
    let overlays = match b.kind {
        Kind::Core | Kind::Node => {
            let top = if b.kind == Kind::Core {
                "blob_core_overlay"
            } else {
                "blob_node_overlay"
            };
            let mut out = Vec::new();
            if let Some(id) = icon(world, "blob") {
                let mut o = Overlay::new(id);
                o.color = color;
                out.push(o);
            }
            if let Some(id) = icon(world, top) {
                out.push(Overlay::new(id));
            }
            Some(out)
        }
        _ => None,
    };
    let Some(mut look) = world.get_mut::<Appearance>(blob) else {
        return;
    };
    if let Some(id) = state {
        look.icon_state = id;
    }
    match overlays {
        Some(overlays) => {
            look.color = None;
            look.overlays = overlays;
        }
        None => look.color = color,
    }
}

// ---------------------------------------------------------------------------------------------
// Damage

fn no_hit() -> Hit {
    Hit {
        damage: 0.0,
        broke: false,
        destroyed: false,
    }
}

/// `run_atom_armor` + `take_damage` of a blob structure: `None` when `target` is no blob (the
/// caller damages it its own way).
pub fn damage(
    world: &mut World,
    target: Entity,
    amount: f32,
    kind: DamageType,
    flag: Flag,
) -> Option<Hit> {
    let b = world.get::<Blob>(target)?.clone();
    let Some(integrity) = world.get::<Integrity>(target).copied() else {
        return Some(no_hit());
    };
    if integrity.current <= 0.0 {
        return Some(no_hit());
    }
    // `/obj/structure/blob/special/core/ex_act`: `10 * (severity + 1)` whatever the blast.
    let amount = if b.kind == Kind::Core && flag == Flag::Bomb {
        if amount.is_infinite() || amount >= 250.0 {
            40.0
        } else if amount >= 100.0 {
            30.0
        } else {
            20.0
        }
    } else {
        amount
    };
    let hurt = match kind {
        DamageType::Brute => Hurt::Brute,
        DamageType::Burn => Hurt::Burn,
        _ => return Some(no_hit()),
    };
    let resist = match hurt {
        Hurt::Brute => b.kind.brute_resist(integrity.current),
        Hurt::Burn => 1.0,
    };
    let armor = b.kind.armor(flag).clamp(0.0, 100.0);
    let mut dealt = (amount * resist * (100.0 - armor) * 0.01 * 10.0).round() / 10.0;
    let strain = b.overmind.and_then(|o| strain_of(world, o));
    if let (Some(strain), Some(overmind)) = (strain, b.overmind) {
        damage_reaction(
            world,
            target,
            overmind,
            strain,
            dealt,
            integrity.current,
            hurt,
            flag,
        );
        dealt = strains::damage_taken(strain, dealt, hurt, flag);
    }
    if dealt < crate::obj_damage::DAMAGE_PRECISION || world.get_entity(target).is_err() {
        return Some(no_hit());
    }
    // The reaction may have moved it (shifting fragments) but not hurt it.
    let current = world
        .get::<Integrity>(target)
        .map_or(integrity.current, |i| i.current);
    let left = (current - dealt).max(0.0);
    if left <= 0.0 {
        destroy(world, target);
        return Some(Hit {
            damage: dealt,
            broke: false,
            destroyed: true,
        });
    }
    set_integrity(world, target, left);
    Some(Hit {
        damage: dealt,
        broke: false,
        destroyed: false,
    })
}

/// A new integrity: the look, and a strong blob under half lets air through.
pub fn set_integrity(world: &mut World, blob: Entity, value: f32) {
    let Some(b) = world.get::<Blob>(blob).cloned() else {
        return;
    };
    let value = value.clamp(0.0, b.max_integrity);
    world.entity_mut(blob).insert(Integrity {
        current: value,
        broken: false,
    });
    if b.kind.shield() {
        // `atmosblock = atom_integrity < max_integrity * 0.5` (inverted in tg's code; the
        // intent, "a weakened shield stops blocking", is ported).
        let blocks = value >= b.max_integrity * 0.5;
        let has = world.get::<AtmosBlocker>(blob).is_some();
        if blocks && !has {
            world
                .entity_mut(blob)
                .insert(AtmosBlocker(AtmosPass::Density));
        } else if !blocks && has {
            world.entity_mut(blob).remove::<AtmosBlocker>();
        }
    }
    refresh_look(world, blob);
}

/// `Destroy` of a structure: gone with a splat (tg `atom_destruction` -> `qdel`).
pub fn destroy(world: &mut World, blob: Entity) {
    let Some(&pos) = world.get::<TilePos>(blob) else {
        return;
    };
    let factory = world
        .get::<Blob>(blob)
        .is_some_and(|b| b.kind == Kind::Factory);
    world.despawn(blob);
    play(world, pos, SPLAT);
    crate::construction::tile_changed(world, pos);
    if factory {
        super::minions::factory_destroyed(world, blob);
    }
}

fn play(world: &mut World, pos: TilePos, sound: &str) {
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 50.0, true));
}

/// The world half of `damage_reaction` (the numbers are in [`strains::damage_taken`]).
#[allow(clippy::too_many_arguments)]
fn damage_reaction(
    world: &mut World,
    blob: Entity,
    overmind: Entity,
    strain: Strain,
    damage: f32,
    integrity: f32,
    hurt: Hurt,
    flag: Flag,
) {
    let survives = integrity - damage > 0.0;
    match strain {
        Strain::ReactiveSpines => {
            let t = now(world);
            let ready = world
                .get::<BlobOvermind>(overmind)
                .is_some_and(|o| t >= o.retaliate_at);
            if damage > 0.0 && survives && ready {
                if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
                    o.retaliate_at = t + strains::RETALIATE_COOLDOWN;
                }
                retaliate(world, blob, overmind);
            }
        }
        Strain::ReplicatingFoam => {
            if hurt == Hurt::Burn
                && damage > 0.0
                && survives
                && roll(world, 60.0)
                && let Some(new) = expand(world, blob, None, None, false)
            {
                set_integrity(world, new, integrity - damage);
            }
        }
        Strain::ShiftingFragments => {
            let shot = matches!(flag, Flag::Melee | Flag::Bullet | Flag::Laser);
            if shot && damage > 0.0 && survives && roll(world, 60.0 - damage) {
                shift_away(world, blob, strain);
            }
        }
        _ => {}
    }
}

/// Reactive spines: "The blob retaliates, lashing out!" at everything living next to it.
fn retaliate(world: &mut World, blob: Entity, overmind: Entity) {
    let Some(&pos) = world.get::<TilePos>(blob) else {
        return;
    };
    for m in crate::pull::visible_message(
        world,
        pos,
        ChatChannel::Warning,
        (
            Span::BoldWarning,
            "Блоб наносит ответный удар, хлеща всё вокруг!".to_owned(),
        ),
        &[],
    ) {
        world.write_message(m);
    }
    for victim in living_in_range(world, pos, 1) {
        if !is_ally(world, victim) {
            super::attack_living(world, overmind, victim);
        }
    }
}

/// Living mobs within `range` of `pos`.
fn living_in_range(world: &World, pos: TilePos, range: i32) -> Vec<Entity> {
    let index = world.resource::<SpatialIndex>();
    let mut out = Vec::new();
    for dy in -range..=range {
        for dx in -range..=range {
            let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            if !index.in_bounds(p) {
                continue;
            }
            out.extend(
                index
                    .on_tile(p)
                    .iter()
                    .copied()
                    .filter(|&e| world.get::<Health>(e).is_some()),
            );
        }
    }
    out
}

/// Shifting fragments: swap places with a normal (or, a quarter of the time, a strong) blob of
/// the same strain next to it.
fn shift_away(world: &mut World, blob: Entity, strain: Strain) {
    let Some(&pos) = world.get::<TilePos>(blob) else {
        return;
    };
    let mut picks = Vec::new();
    for (e, p, kind) in blobs_in_range(world, pos, 1) {
        if e == blob {
            continue;
        }
        let same = world
            .get::<Blob>(e)
            .and_then(|b| b.overmind)
            .and_then(|o| strain_of(world, o))
            == Some(strain);
        if !same {
            continue;
        }
        if kind == Kind::Normal || (kind.shield() && roll(world, 25.0)) {
            picks.push((e, p));
        }
    }
    if picks.is_empty() {
        return;
    }
    let (other, at) = picks[rand_u(world, 0, picks.len() as u32 - 1) as usize];
    swap(world, blob, pos, other, at);
}

fn swap(world: &mut World, a: Entity, a_pos: TilePos, b: Entity, b_pos: TilePos) {
    world.entity_mut(a).insert(b_pos);
    world.entity_mut(b).insert(a_pos);
    crate::construction::tile_changed(world, a_pos);
    crate::construction::tile_changed(world, b_pos);
}

// ---------------------------------------------------------------------------------------------
// Growth

/// A cardinal tile next to `pos` without a blob (tg `expand` without a target).
fn free_side(world: &mut World, pos: TilePos) -> Option<TilePos> {
    let mut dirs = Dir::CARDINALS.to_vec();
    while !dirs.is_empty() {
        let i = rand_u(world, 0, dirs.len() as u32 - 1) as usize;
        let to = pos.step(dirs.remove(i));
        if world.resource::<SpatialIndex>().in_bounds(to) && blob_at(world, to).is_none() {
            return Some(to);
        }
    }
    None
}

/// tg `expand(T, controller, expand_reaction)` of `from`: hits what is on `target` (a random free
/// side without one) and grows a normal blob there if nothing dense is left. `controller`: the
/// overmind expanding by hand. The new blob, if any.
pub fn expand(
    world: &mut World,
    from: Entity,
    target: Option<TilePos>,
    controller: Option<Entity>,
    reaction: bool,
) -> Option<Entity> {
    let (Some(b), Some(&from_pos)) = (world.get::<Blob>(from).cloned(), world.get::<TilePos>(from))
    else {
        return None;
    };
    let to = match target {
        Some(t) => t,
        None => free_side(world, from_pos)?,
    };
    let mut make = world.resource::<SpatialIndex>().in_bounds(to);
    let lattice = world
        .resource::<SpatialIndex>()
        .on_tile(to)
        .iter()
        .any(|&e| crate::construction::path_of(world, e).is_some_and(|p| is_under(&p, LATTICE)));
    if make && crate::construction::is_space(world, to) && !lattice && roll(world, 80.0) {
        make = false;
        play(world, from_pos, SPLAT);
        if let Some(c) = controller {
            crate::tools::tell_mob(world, c, Span::Warning, "Не удалось расшириться!");
        }
    }
    consume_tile(world, from);
    if !make {
        return None;
    }
    // `T.CanPass` of the turf, then of everything on it.
    let things: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(to).to_vec();
    for thing in things {
        if world.get_entity(thing).is_err() {
            continue;
        }
        if world.get::<Density>(thing).is_some() {
            make = false;
        }
        let living = world.get::<Health>(thing).is_some();
        if living
            && controller.is_none()
            && let Some(overmind) = b.overmind
            && !is_ally(world, thing)
        {
            super::attack_living(world, overmind, thing);
            continue;
        }
        blob_act(world, from_pos, thing);
    }
    // Something dense was there (even if the hit broke it): no blob this time.
    if !make {
        return None;
    }
    let owner = controller.or(b.overmind);
    let new = spawn_blob(world, Kind::Normal, to, owner)?;
    let legit = world.get::<Blob>(new).is_some_and(|n| n.legit);
    if !legit && let Some(c) = controller {
        crate::tools::tell_mob(
            world,
            c,
            Span::Warning,
            "Вне станции — этот тайл не засчитывается!",
        );
    }
    if reaction && let Some(strain) = owner.and_then(|o| strain_of(world, o)) {
        expand_reaction(world, from, new, to, controller, strain);
    }
    world.get_entity(new).is_ok().then_some(new)
}

/// `expand_reaction` of the strains that react to growth.
fn expand_reaction(
    world: &mut World,
    from: Entity,
    new: Entity,
    to: TilePos,
    controller: Option<Entity>,
    strain: Strain,
) {
    match strain {
        Strain::ReplicatingFoam => {
            if roll(world, 30.0) {
                expand(world, new, None, None, false);
            }
        }
        Strain::ShiftingFragments => {
            let kind = world.get::<Blob>(from).map(|b| b.kind);
            if let (Some(kind), Some(&from_pos)) = (kind, world.get::<TilePos>(from))
                && (kind == Kind::Normal || kind.shield())
            {
                swap(world, new, to, from, from_pos);
            }
        }
        Strain::NetworkedFibers => {
            let owner = world.get::<Blob>(new).and_then(|b| b.overmind);
            let Some(owner) = owner else {
                return;
            };
            // Grown by a pulse: it is taken back and pays a point.
            let Some(controller) = controller else {
                super::add_points(world, owner, 1.0);
                destroy_quiet(world, new);
                return;
            };
            if crate::construction::is_space(world, to) {
                return;
            }
            let movers: Vec<(Entity, TilePos)> = blobs_in_range(world, to, 1)
                .into_iter()
                .filter(|(e, _, k)| {
                    matches!(k, Kind::Core | Kind::Node)
                        && world.get::<Blob>(*e).and_then(|b| b.overmind) == Some(controller)
                })
                .map(|(e, p, _)| (e, p))
                .collect();
            if let Some(&(special, at)) = movers.first() {
                swap(world, new, to, special, at);
                return;
            }
            super::add_points(world, controller, 4.0);
            destroy_quiet(world, new);
        }
        _ => {}
    }
}

/// `qdel` of a structure without its splat.
fn destroy_quiet(world: &mut World, blob: Entity) {
    if let Some(&pos) = world.get::<TilePos>(blob) {
        world.despawn(blob);
        crate::construction::tile_changed(world, pos);
    }
}

/// tg `ConsumeTile`: the blob hits everything on its own tile.
pub fn consume_tile(world: &mut World, blob: Entity) {
    let (Some(b), Some(&pos)) = (world.get::<Blob>(blob).cloned(), world.get::<TilePos>(blob))
    else {
        return;
    };
    let things: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
    for thing in things {
        if thing == blob || world.get_entity(thing).is_err() {
            continue;
        }
        if world.get::<Health>(thing).is_some()
            && let Some(overmind) = b.overmind
            && !is_ally(world, thing)
        {
            super::attack_living(world, overmind, thing);
            continue;
        }
        blob_act(world, pos, thing);
    }
}

/// tg `blob_act` of `thing` hit by a blob standing on `blob_pos`.
pub fn blob_act(world: &mut World, blob_pos: TilePos, thing: Entity) {
    if world.get::<Blob>(thing).is_some() || world.get::<HiddenUnderTile>(thing).is_some() {
        return;
    }
    if world.get::<Health>(thing).is_some() {
        mob_blob_act(world, thing);
        return;
    }
    let pos = world.get::<TilePos>(thing).copied();
    if let Some(item) = world.get::<DmItem>(thing) {
        // `/obj/item/blob_act`: only what lies on the blob's own tile.
        if item.resistance_flags & INDESTRUCTIBLE == 0 && pos == Some(blob_pos) {
            crate::explosions::ex_act::destroy_item(world, thing);
        }
        return;
    }
    if let Some(obj) = world.get::<DmObj>(thing).cloned() {
        if obj.resistance_flags & INDESTRUCTIBLE != 0 {
            return;
        }
        if is_under(&obj.path, DOOR) {
            crate::obj_damage::deconstruct(world, thing, false);
            return;
        }
        if world.get::<Integrity>(thing).is_none() {
            if crate::machine_frame::buildable_machine(world, thing).is_none()
                || obj.max_integrity <= 0.0
            {
                return;
            }
            world.entity_mut(thing).insert(Integrity {
                current: obj.max_integrity,
                broken: false,
            });
        }
        crate::obj_damage::take_damage(world, thing, OBJ_BLOB_ACT_DAMAGE, DamageType::Brute);
        return;
    }
    // `/turf/closed/wall/blob_act`: half the time the wall comes down.
    if let (Some(build), Some(pos)) = (world.get::<DmTurfBuild>(thing), pos)
        && is_under(&build.path, WALL)
        && roll(world, 50.0)
    {
        crate::construction::dismantle_wall(world, thing, pos);
    }
}

/// `blob_act` of a living mob: blob mobs are soothed, the dead ignored, a human takes 5 brute,
/// a basic mob 20, anything else 10.
fn mob_blob_act(world: &mut World, mob: Entity) {
    if is_ally(world, mob) {
        super::minions::blob_touched(world, mob);
        return;
    }
    if world.get::<Stat>(mob).is_none_or(|s| *s == Stat::Dead) {
        return;
    }
    let amount = if world.get::<crate::inventory::Inventory>(mob).is_some() {
        crate::tools::tell_mob(world, mob, Span::Danger, "Блоб атакует вас!");
        5.0
    } else if world.get::<crate::mobs::BasicMob>(mob).is_some() {
        20.0
    } else {
        10.0
    };
    super::hurt(world, mob, DamageType::Brute, amount);
    if let Some(&pos) = world.get::<TilePos>(mob) {
        play(world, pos, ATTACK_SOUND);
    }
}

// ---------------------------------------------------------------------------------------------
// Pulses

/// tg `Be_Pulsed`: off its 1 s cooldown it eats its tile and, off its 2 s one, heals; a factory
/// makes a spore and a resource blob gathers a point either way.
pub fn be_pulsed(world: &mut World, blob: Entity) -> bool {
    let Some(b) = world.get::<Blob>(blob).cloned() else {
        return false;
    };
    let t = now(world);
    let mut pulsed = false;
    if t >= b.pulse_at {
        consume_tile(world, blob);
        if world.get_entity(blob).is_err() {
            return false;
        }
        if t >= b.heal_at {
            let current = world.get::<Integrity>(blob).map_or(0.0, |i| i.current);
            set_integrity(world, blob, current + b.kind.regen());
            if let Some(mut b) = world.get_mut::<Blob>(blob) {
                b.heal_at = t + HEAL_COOLDOWN;
            }
        }
        refresh_look(world, blob);
        if let Some(mut b) = world.get_mut::<Blob>(blob) {
            b.pulse_at = t + PULSE_COOLDOWN;
        }
        pulsed = true;
    }
    match b.kind {
        Kind::Factory => factory_pulse(world, blob, t),
        Kind::Resource => resource_pulse(world, blob, t),
        _ => {}
    }
    pulsed
}

fn factory_pulse(world: &mut World, factory: Entity, t: f64) {
    let Some(b) = world.get::<Blob>(factory).cloned() else {
        return;
    };
    let naut_alive = b
        .blobbernaut
        .is_some_and(|n| world.get::<Stat>(n).is_some_and(|s| *s != Stat::Dead));
    // `on_blobbernaut_death`: the factory's health comes back.
    if b.blobbernaut.is_some() && !naut_alive {
        if let Some(mut b) = world.get_mut::<Blob>(factory) {
            b.blobbernaut = None;
            b.max_integrity = Kind::Factory.max_hp();
        }
        refresh_look(world, factory);
    }
    if naut_alive || t < b.next_at {
        return;
    }
    if super::minions::spores_of(world, factory) >= FACTORY_MAX_SPORES {
        return;
    }
    if let Some(mut b) = world.get_mut::<Blob>(factory) {
        b.next_at = t + SPORE_COOLDOWN;
    }
    super::minions::spawn_spore(world, factory, b.overmind);
}

fn resource_pulse(world: &mut World, blob: Entity, t: f64) {
    let Some(b) = world.get::<Blob>(blob).cloned() else {
        return;
    };
    if t < b.next_at {
        return;
    }
    let delay = match b.overmind {
        Some(overmind) => {
            super::add_points(world, overmind, GATHER_AMOUNT);
            let count = super::count_kind(world, overmind, Kind::Resource);
            GATHER_DELAY + count as f64 * GATHER_ADDED_DELAY
        }
        None => GATHER_DELAY,
    };
    if let Some(mut b) = world.get_mut::<Blob>(blob) {
        b.next_at = t + delay;
    }
}

/// tg `pulse_area` of a core or node: pulses itself, grows once (70%, else one blob near it),
/// reclaims dead blobs and pulses those within `pulse_range`.
pub fn pulse_area(world: &mut World, special: Entity, overmind: Entity) {
    let (Some(b), Some(&pos)) = (
        world.get::<Blob>(special).cloned(),
        world.get::<TilePos>(special),
    ) else {
        return;
    };
    let Some((claim, pulse, expand_range, _)) = b.kind.ranges() else {
        return;
    };
    be_pulsed(world, special);
    let mut expanded = roll(world, 70.0) && expand(world, special, None, None, true).is_some();
    let mut near: Vec<(Entity, TilePos, Kind)> = blobs_in_range(world, pos, claim)
        .into_iter()
        .filter(|(e, ..)| *e != special)
        .collect();
    // `shuffle_inplace`.
    for i in (1..near.len()).rev() {
        let j = rand_u(world, 0, i as u32) as usize;
        near.swap(i, j);
    }
    let crowded = near.len() >= 120;
    let t = now(world);
    for (blob, at, _) in near {
        let Some(nb) = world.get::<Blob>(blob).cloned() else {
            continue;
        };
        if nb.overmind.is_none() && roll(world, 30.0) {
            if let Some(mut nb) = world.get_mut::<Blob>(blob) {
                nb.overmind = Some(overmind);
            }
            refresh_look(world, blob);
        }
        let distance = pos.chebyshev(at).unwrap_or(u32::MAX);
        let mut chance = (20.0 - distance as f32 * 8.0).max(1.0);
        if distance <= 1 {
            chance = 20.0;
        }
        if distance <= expand_range && !expanded {
            let can_expand = !(crowded && t < nb.heal_at);
            if can_expand && t >= nb.pulse_at && roll(world, chance) {
                expanded = expand(world, blob, None, None, true).is_some();
            }
        }
        if distance <= pulse && world.get_entity(blob).is_ok() {
            be_pulsed(world, blob);
        }
    }
}

/// tg `reinforce_area(seconds_per_tick)`: normal blobs within `strong_reinforce_range` may
/// harden into the core's free strong blobs.
pub fn reinforce_area(world: &mut World, special: Entity, secs: f64) {
    let (Some(b), Some(&pos)) = (
        world.get::<Blob>(special).cloned(),
        world.get::<TilePos>(special),
    ) else {
        return;
    };
    let Some((_, _, _, strong)) = b.kind.ranges() else {
        return;
    };
    if strong == 0 {
        return;
    }
    let chance = spt_prob(REINFORCE_CHANCE, secs);
    for (blob, _, kind) in blobs_in_range(world, pos, strong) {
        if kind == Kind::Normal && roll(world, chance) {
            change_to(world, blob, Kind::StrongCore);
        }
    }
}

/// tg `SPT_PROB`: the chance per tick of a per-second `percent`.
pub fn spt_prob(percent: f32, secs: f64) -> f32 {
    let p = f64::from(percent / 100.0);
    ((1.0 - (1.0 - p).powf(secs)) * 100.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_carry_the_defines() {
        assert_eq!(Kind::Normal.max_hp(), 25.0);
        assert_eq!(Kind::Core.point_return(), -1);
        assert_eq!(Kind::Factory.point_return(), 25);
        assert_eq!(Kind::Strong.point_return(), 4);
        assert_eq!(Kind::StrongCore.point_return(), 0);
        assert_eq!(Kind::Normal.armor(Flag::Laser), 50.0);
        assert_eq!(Kind::Core.armor(Flag::Laser), 0.0);
        assert_eq!(Kind::Node.armor(Flag::Fire), 65.0);
        assert_eq!(Kind::Normal.armor(Flag::Melee), 0.0);
        assert_eq!(Kind::Normal.brute_resist(21.0), 0.25);
        assert_eq!(Kind::Normal.brute_resist(15.0), 0.5);
        assert_eq!(Kind::Core.brute_resist(400.0), 0.5);
        assert_eq!(Kind::Core.ranges(), Some((12, 4, 3, 1)));
        assert_eq!(Kind::Node.ranges(), Some((10, 3, 2, 0)));
        assert_eq!(Kind::Factory.ranges(), None);
        assert_eq!(BLOB_RUNTIME_TYPES.len(), 9);
    }

    /// Spawned by a computed path (`Kind::path`), so the runtime-types scan cannot see them.
    #[test]
    fn spawned_types_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        for path in BLOB_RUNTIME_TYPES
            .iter()
            .chain(super::super::minions::MINION_RUNTIME_TYPES.iter())
        {
            assert!(registry.contains(path), "{path}");
        }
    }

    #[test]
    fn looks_follow_damage_and_station() {
        assert_eq!(icon_state(Kind::Normal, 21.0, 25.0), "blob");
        assert_eq!(icon_state(Kind::Normal, 15.0, 25.0), "blob_damaged");
        assert_eq!(icon_state(Kind::Strong, 74.0, 150.0), "blob_shield_damaged");
        assert_eq!(icon_state(Kind::Core, 1.0, 400.0), "blank_blob");
        assert_eq!(paler([0, 100, 255, 255]), [127, 177, 255, 255]);
    }

    #[test]
    fn reinforcing_is_per_second() {
        let p = spt_prob(2.5, 2.0);
        assert!((p - 4.9375).abs() < 1e-3, "{p}");
        assert!((spt_prob(2.5, 1.0) - 2.5).abs() < 1e-4);
    }
}
