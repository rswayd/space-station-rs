//! Smoke, after tg `fluid_spread/effects_smoke.dm` and `SSsmoke` (`spread_wait` 0.1 s,
//! `effect_wait` 2 s).
//!
//! - [`do_smoke`] starts a flood of `amount` nodes (`DIAMOND_AREA(range)` by default) on a
//!   tile: the first node spreads at once, each new node 0.1 s later, onto the atmos-adjacent
//!   tiles without smoke of its kind, until the group has its size. A new node inherits the
//!   lifetime its parent has left.
//! - Every 2 s a node loses 2 s of lifetime; at 0 it turns transparent and is gone 1 s later
//!   (`kill_smoke`, `fade_out`). Otherwise each carbon in it without internals or smoke
//!   protection (`BLOCK_GAS_SMOKE_EFFECT` clothing, `TRAIT_NOBREATH`) and not smoked in the
//!   last second (`smoke_delay`) is smoked: bad smoke drops what it holds, takes 1 oxygen
//!   damage and coughs; sleeping smoke puts it to sleep for 20 s and coughs.
//! - Chemical smoke ([`do_chem_smoke`], 20 s; quick 4 s, medium 8 s) carries a copy of up to
//!   1000 u of reagents in its colour: each process, every carbon in it without protection
//!   inhales `2 s / lifetime` of the whole amount into its blood (no `smoke_delay`), and the
//!   things and the turf under it are exposed to that fraction (`crate::janitor::expose`).
//! - The smoker is also exposed to it (`SMOKE_MACHINE`: `crate::janitor::expose_by`).
//! - Freezing smoke ([`do_freezing_smoke`], the nanofrost) first chills the tiles within its
//!   blast radius: the air goes to 2 K (the decontamination kind: 20 °C, at any distance),
//!   plasma turns into nitrogen, vents and scrubbers freeze shut (not the decon kind) and
//!   burning mobs go out.
// shortcut: no spreading again when a door opens (`react_to_atmos_adjacency_changes`), no
// halving of laser damage in bad smoke, no admin log (the server log has the floods), the
// node's random facing is not set; chemical smoke never reacts inside the cloud (tg nodes are
// `REAGENT_HOLDER_INSTANT_REACT`).

use bevy::prelude::*;
use ss13_mapload::Opaque;
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::{Appearance, Dir, TilePos};
use ss13_world::SpatialIndex;

use super::{FluidGroup, FluidGroups, on_tile};
use crate::chem::{Chem, MobReagents};
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;

pub const SMOKE: &str = "/obj/effect/particle_effect/fluid/smoke";
pub const SMOKE_TRANSPARENT: &str = "/obj/effect/particle_effect/fluid/smoke/transparent";
pub const SMOKE_QUICK: &str = "/obj/effect/particle_effect/fluid/smoke/quick";
pub const SMOKE_BAD: &str = ss13_mapload::runtime::SMOKE_BAD;
pub const SMOKE_BAD_GREEN: &str = "/obj/effect/particle_effect/fluid/smoke/bad/green";
pub const SMOKE_BAD_BLACK: &str = "/obj/effect/particle_effect/fluid/smoke/bad/black";
pub const SMOKE_FREEZING: &str = "/obj/effect/particle_effect/fluid/smoke/freezing";
pub const SMOKE_SLEEPING: &str = "/obj/effect/particle_effect/fluid/smoke/sleeping";
pub const SMOKE_CHEM: &str = "/obj/effect/particle_effect/fluid/smoke/chem";
pub const SMOKE_CHEM_QUICK: &str = "/obj/effect/particle_effect/fluid/smoke/chem/quick";
pub const SMOKE_CHEM_MEDIUM: &str = "/obj/effect/particle_effect/fluid/smoke/chem/medium";

/// `SSsmoke`: `spread_wait`, `effect_wait`.
const SPREAD_WAIT: f64 = 0.1;
const EFFECT_WAIT: f64 = 2.0;
/// `kill_smoke`: `QDEL_IN(src, 1 SECONDS)`.
const FADE_SECS: f64 = 1.0;
/// `smoke_delay`: one smoke effect per mob per second.
const SMOKE_DELAY: f64 = 1.0;
/// `/obj/effect/particle_effect/fluid/smoke/sleeping`: `Sleeping(20 SECONDS)`.
const SLEEP_SECS: f32 = 20.0;
/// `chemholder = new(1000, NO_REACT)`.
const CHEM_VOLUME: f32 = 1000.0;
/// `BLOCK_GAS_SMOKE_EFFECT` (`clothing_flags`).
pub const BLOCK_GAS_SMOKE_EFFECT: u32 = 1 << 2;
/// `/datum/effect_system/fluid_spread/smoke/freezing`: `temperature`; the decon kind's `T20C`.
const FREEZING_TEMP: f64 = 2.0;

/// The ported `/obj/effect/particle_effect/fluid/smoke` types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmokeKind {
    Basic,
    Transparent,
    Quick,
    Bad,
    BadGreen,
    BadBlack,
    Freezing,
    Sleeping,
    Chem,
    ChemQuick,
    ChemMedium,
}

impl SmokeKind {
    pub fn path(self) -> &'static str {
        match self {
            Self::Basic => SMOKE,
            Self::Transparent => SMOKE_TRANSPARENT,
            Self::Quick => SMOKE_QUICK,
            Self::Bad => SMOKE_BAD,
            Self::BadGreen => SMOKE_BAD_GREEN,
            Self::BadBlack => SMOKE_BAD_BLACK,
            Self::Freezing => SMOKE_FREEZING,
            Self::Sleeping => SMOKE_SLEEPING,
            Self::Chem => SMOKE_CHEM,
            Self::ChemQuick => SMOKE_CHEM_QUICK,
            Self::ChemMedium => SMOKE_CHEM_MEDIUM,
        }
    }

    /// `initial(lifetime)`, seconds.
    pub fn lifetime(self) -> f64 {
        match self {
            Self::Basic | Self::Transparent | Self::Freezing => 10.0,
            Self::Quick => 1.0,
            Self::Bad | Self::BadGreen | Self::BadBlack => 16.0,
            Self::Sleeping | Self::Chem => 20.0,
            Self::ChemQuick => 4.0,
            Self::ChemMedium => 8.0,
        }
    }

    pub fn chem(self) -> bool {
        matches!(self, Self::Chem | Self::ChemQuick | Self::ChemMedium)
    }

    fn bad(self) -> bool {
        matches!(self, Self::Bad | Self::BadGreen | Self::BadBlack)
    }

    /// `locate(type) in turf`: the kind and its subtypes.
    fn covers(self, other: Self) -> bool {
        self == other
            || (self == Self::Bad && other.bad())
            || (self == Self::Chem && other.chem())
            || self == Self::Basic
    }
}

/// A smoke node (`/obj/effect/particle_effect/fluid/smoke`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Smoke {
    pub kind: SmokeKind,
    /// Its [`FluidGroup`].
    pub group: u32,
    /// Seconds left.
    pub lifetime: f64,
    pub next_process: f64,
    /// Fading out (`kill_smoke`): gone at this time.
    pub gone_at: Option<f64>,
}

/// `SSsmoke.queue_spread`: the node spreads at this time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct SmokeSpread(f64);

/// `smoke_delay` of a mob: no smoke effect before this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SmokeDelay(pub f64);

/// `DIAMOND_AREA(range)`.
pub fn diamond_area(range: u32) -> u32 {
    1 + 2 * range * (range + 1)
}

/// tg `do_smoke(range, holder, location, amount, smoke_type)`. The group's id, `None` when the
/// smoke has no prototype.
pub fn do_smoke(
    world: &mut World,
    range: u32,
    pos: TilePos,
    amount: Option<u32>,
    kind: SmokeKind,
) -> Option<u32> {
    let amount = amount.unwrap_or_else(|| diamond_area(range));
    start(world, pos, amount, kind, FluidGroup::default())
}

/// tg `do_chem_smoke(range, holder, location, carry = reagents, amount)`: chemical smoke of
/// `kind` carrying a copy of `carry` (at most 1000 u).
pub fn do_chem_smoke(
    world: &mut World,
    range: u32,
    pos: TilePos,
    amount: Option<u32>,
    carry: &Holder,
    kind: SmokeKind,
) -> Option<u32> {
    let chem = world.get_resource::<Chem>()?.0.clone();
    let mut held = Holder::new(CHEM_VOLUME, flags::NO_REACT, Owner::default());
    let mut source = carry.clone();
    let volume = source.total_volume;
    source.trans_to(&chem, &mut held, volume, 1.0, None, true, true);
    info!(
        "chemical smoke at {pos:?}: {}",
        held.reagents
            .iter()
            .map(|r| format!("{}u {}", r.volume, chem.def(r.id).name))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let group = FluidGroup {
        color: ss13_reagents::mix_color(&chem, &held.reagents),
        reagents: Some(held),
        ..default()
    };
    let amount = amount.unwrap_or_else(|| diamond_area(range));
    start(world, pos, amount, kind, group)
}

/// `/datum/effect_system/fluid_spread/smoke/freezing` (`decon`: the decontamination kind):
/// `chill_turf` on every tile within `blast` of `pos`, then the transparent freezing smoke.
pub fn do_freezing_smoke(
    world: &mut World,
    range: u32,
    pos: TilePos,
    blast: u32,
    decon: bool,
) -> Option<u32> {
    if blast > 0 {
        let r = blast as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let t = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                // `get_dist(location, chilly) < blast`.
                let near = dx.abs().max(dy.abs()) < r;
                chill_turf(world, t, decon, near);
            }
        }
    }
    do_smoke(world, range, pos, None, SmokeKind::Freezing)
}

/// `chill_turf`.
fn chill_turf(world: &mut World, pos: TilePos, decon: bool, near: bool) {
    if crate::construction::turf_at(world, pos)
        .is_none_or(|t| world.get::<ss13_world::Density>(t).is_some())
    {
        return;
    }
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.modify_air(pos, |air| {
            if decon {
                air.temperature = ss13_atmos::gas::T20C;
            } else if near {
                air.temperature = FREEZING_TEMP;
            }
            let plasma = air.get(ss13_atmos::GasId::PLASMA);
            if plasma > 0.0 {
                air.add(ss13_atmos::GasId::N2, plasma);
                air.set(ss13_atmos::GasId::PLASMA, 0.0);
                air.garbage_collect();
            }
        });
    }
    for e in on_tile(world, pos) {
        if !decon {
            super::foam::weld_vent(world, e, "is frozen shut!");
        }
        if world.get::<Health>(e).is_some() {
            crate::fire::extinguish_mob(world, e);
        }
    }
}

/// `start()`: the first node, which spreads at once.
fn start(
    world: &mut World,
    pos: TilePos,
    amount: u32,
    kind: SmokeKind,
    mut group: FluidGroup,
) -> Option<u32> {
    group.target_size = amount;
    let id = world.resource_mut::<FluidGroups>().create(group);
    let Some(first) = spawn_node(world, kind, pos, id, kind.lifetime()) else {
        world.resource_mut::<FluidGroups>().groups.remove(&id);
        return None;
    };
    spread(world, first);
    Some(id)
}

fn spawn_node(
    world: &mut World,
    kind: SmokeKind,
    pos: TilePos,
    group: u32,
    lifetime: f64,
) -> Option<Entity> {
    let e = crate::construction::spawn_obj(world, kind.path(), pos)?;
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.resource_mut::<FluidGroups>().add_node(group);
    let color = world
        .resource::<FluidGroups>()
        .get(group)
        .and_then(|g| g.color);
    if let Some(color) = color
        && let Some(mut look) = world.get_mut::<Appearance>(e)
    {
        look.color = Some(color);
    }
    world.entity_mut(e).insert(Smoke {
        kind,
        group,
        lifetime,
        next_process: now,
        gone_at: None,
    });
    Some(e)
}

fn has_smoke(world: &World, pos: TilePos, kind: SmokeKind) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<Smoke>(e).is_some_and(|s| kind.covers(s.kind)))
}

/// `get_atmos_adjacent_turfs()`; without the air model, the cardinal tiles whose turf is open.
pub(super) fn spread_from(world: &World, pos: TilePos) -> Vec<TilePos> {
    if let Some(atmos) = world.get_resource::<ss13_atmos::Atmos>() {
        return atmos.adjacent_turfs(pos, false);
    }
    let index = world.resource::<SpatialIndex>();
    Dir::CARDINALS
        .into_iter()
        .map(|d| pos.step(d))
        .filter(|&p| {
            index.in_bounds(p)
                && crate::construction::turf_at(world, p)
                    .is_some_and(|t| world.get::<ss13_world::Density>(t).is_none())
        })
        .collect()
}

/// `/obj/effect/particle_effect/fluid/smoke/spread`.
fn spread(world: &mut World, node: Entity) {
    let (Some(&smoke), Some(&pos)) = (world.get::<Smoke>(node), world.get::<TilePos>(node)) else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    for to in spread_from(world, pos) {
        if !world.resource::<FluidGroups>().can_grow(smoke.group) {
            break;
        }
        if has_smoke(world, to, smoke.kind) {
            continue;
        }
        if let Some(child) = spawn_node(world, smoke.kind, to, smoke.group, smoke.lifetime) {
            world
                .entity_mut(child)
                .insert(SmokeSpread(now + SPREAD_WAIT));
        }
    }
}

/// `SSsmoke`: queued spreads, then `process` of the nodes whose effect bucket came.
// Cost: per tick, one pass over the smoke nodes (41 for a grenade) and, every 2 s per node,
// what stands on its tile.
pub(super) fn tick_smoke(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let nodes: Vec<(Entity, Smoke, Option<f64>)> = world
        .query::<(Entity, &Smoke, Option<&SmokeSpread>)>()
        .iter(world)
        .map(|(e, s, q)| (e, *s, q.map(|q| q.0)))
        .collect();
    for (e, _, spread_at) in &nodes {
        if spread_at.is_some_and(|at| at <= now) && world.get_entity(*e).is_ok() {
            world.entity_mut(*e).remove::<SmokeSpread>();
            spread(world, *e);
        }
    }
    for (e, smoke, _) in nodes {
        if let Some(at) = smoke.gone_at {
            if now >= at {
                despawn_node(world, e, smoke.group);
            }
            continue;
        }
        if now < smoke.next_process {
            continue;
        }
        process(world, e, smoke, now);
    }
}

fn despawn_node(world: &mut World, e: Entity, group: u32) {
    let pos = world.get::<TilePos>(e).copied();
    world.despawn(e);
    world.resource_mut::<FluidGroups>().remove_node(group);
    if let Some(pos) = pos {
        crate::construction::tile_changed(world, pos);
    }
}

/// `/obj/effect/particle_effect/fluid/smoke/process(seconds_per_tick = 2)`.
fn process(world: &mut World, e: Entity, mut smoke: Smoke, now: f64) {
    smoke.lifetime -= EFFECT_WAIT;
    smoke.next_process = now + EFFECT_WAIT;
    if smoke.lifetime <= 0.0 {
        // `kill_smoke`: no more processing or spreading, transparent while it fades.
        smoke.gone_at = Some(now + FADE_SECS);
        world
            .entity_mut(e)
            .insert(smoke)
            .remove::<(SmokeSpread, Opaque)>();
        return;
    }
    world.entity_mut(e).insert(smoke);
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let things = on_tile(world, pos);
    for &mob in &things {
        if world.get::<Health>(mob).is_some() && world.get::<crate::ghost::Ghost>(mob).is_none() {
            smoke_mob(world, mob, &smoke, now);
        }
    }
    if smoke.kind.chem() {
        chem_expose(world, &smoke, &things);
    }
}

/// `has_smoke_protection`: `BLOCK_GAS_SMOKE_EFFECT` on anything worn, or `TRAIT_NOBREATH`.
pub fn has_smoke_protection(world: &World, mob: Entity) -> bool {
    if crate::species::has_trait(world, mob, crate::species::Trait::NoBreath) {
        return true;
    }
    world.get::<Inventory>(mob).is_some_and(|inv| {
        inv.equipped().any(|item| {
            world
                .get::<ss13_mapload::DmClothing>(item)
                .is_some_and(|c| c.clothing_flags & BLOCK_GAS_SMOKE_EFFECT != 0)
        })
    })
}

/// `smoke_mob` on a carbon (a mob with an inventory) in the node.
fn smoke_mob(world: &mut World, mob: Entity, smoke: &Smoke, now: f64) {
    if world.get::<Inventory>(mob).is_none() || smoke.lifetime < 1.0 {
        return;
    }
    let internals = world
        .get::<crate::internals::Internals>(mob)
        .is_some_and(|i| i.0.is_some());
    if internals || has_smoke_protection(world, mob) {
        return;
    }
    if smoke.kind.chem() {
        inhale(world, mob, smoke);
        return;
    }
    if world.get::<SmokeDelay>(mob).is_some_and(|d| now < d.0) {
        return;
    }
    world.entity_mut(mob).insert(SmokeDelay(now + SMOKE_DELAY));
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    match smoke.kind {
        k if k.bad() => {
            drop_held(world, mob);
            if let Ok(mut entity) = world.get_entity_mut(mob)
                && let Ok((mut health, mut stat)) =
                    entity.get_components_mut::<(&mut Health, &mut Stat)>()
            {
                crate::health::apply_damage(&mut health, &mut stat, DamageType::Oxy, 1.0);
            }
            cough(world, mob);
        }
        SmokeKind::Sleeping => {
            crate::status_effects::apply(
                world,
                mob,
                crate::status_effects::Incap::Sleeping,
                SLEEP_SECS,
            );
            cough(world, mob);
        }
        _ => {}
    }
}

fn cough(world: &mut World, mob: Entity) {
    if let Some(emote) = ss13_chat::emote::find("cough") {
        crate::mobs::defense::emote(world, mob, emote.message);
    }
}

/// `drop_all_held_items`.
fn drop_held(world: &mut World, mob: Entity) {
    let (Some(hands), Some(&pos)) = (
        world.get::<Inventory>(mob).map(|i| i.hands),
        world.get::<TilePos>(mob),
    ) else {
        return;
    };
    for item in hands.into_iter().flatten() {
        if !crate::inventory::no_drop(world, item) {
            crate::inventory::put_down(world, item, pos);
        }
    }
}

/// The fraction of the cloud's reagents one process applies: `2 s / initial(lifetime)`.
fn chem_fraction(kind: SmokeKind) -> f32 {
    (EFFECT_WAIT / kind.lifetime()) as f32
}

/// `smoke/chem/smoke_mob`: `trans_to(smoker, total, fraction, copy_only)` into the blood.
fn inhale(world: &mut World, mob: Entity, smoke: &Smoke) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut held) = world
        .resource::<FluidGroups>()
        .get(smoke.group)
        .and_then(|g| g.reagents.clone())
    else {
        return;
    };
    let total = held.total_volume;
    let fraction = chem_fraction(smoke.kind);
    if let Some(mut m) = world.get_mut::<MobReagents>(mob) {
        held.trans_to(&chem, &mut m.blood, total, fraction, None, false, true);
    }
    // `trans_to(..., methods = SMOKE_MACHINE)`: its effect on the smoker.
    crate::janitor::expose_by(
        world,
        mob,
        &held,
        fraction,
        crate::janitor::SMOKE_MACHINE,
        crate::janitor::WATER_COOLING_TEMPERATURE,
    );
}

/// `smoke/chem/process`: the things on the tile (not mobs, not abstract) and the turf.
fn chem_expose(world: &mut World, smoke: &Smoke, things: &[Entity]) {
    let Some(held) = world
        .resource::<FluidGroups>()
        .get(smoke.group)
        .and_then(|g| g.reagents.clone())
    else {
        return;
    };
    let fraction = chem_fraction(smoke.kind);
    for &thing in things {
        if world.get_entity(thing).is_err()
            || world.get::<Smoke>(thing).is_some()
            || world.get::<Health>(thing).is_some()
        {
            continue;
        }
        crate::janitor::expose(world, thing, &held, fraction);
    }
}
