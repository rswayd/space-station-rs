//! Foam, after tg `fluid_spread/effects_foam.dm` and `SSfoam` (spread and effects every
//! 0.2 s).
//!
//! - [`do_foam`] starts a flood of `amount` nodes on a tile carrying a copy of its reagents
//!   amplified 3.2 times (`FOAM_REAGENT_SCALE`; the long-lived kind ×15). Each node spreads
//!   0.2 s after it appears to the cardinal tiles it can reach (`LinkBlockedWithAccess` with
//!   `PASSTABLE | PASSGRILLE | PASSMACHINE | PASSSTRUCTURE`: walls, doors, windows, racks and
//!   resin stop it) that have no foam, foaming the mobs there, until the group has its size.
//! - Every 0.2 s a node loses 0.2 s of lifetime and exposes the objects on its tile and the tile
//!   to `0.2 × 25 / (initial lifetime × max(25, flood size))` of its reagents (VAPOR,
//!   `crate::janitor::expose`); each mob in it uses up 0.2 s more instead. Heat over 475 K
//!   dissolves it (`prob(T - 475)` a process). Slippery foam slips who walks in (10 s).
//! - At the end of its lifetime a node leaves its result (`make_result`): foamed metal walls of
//!   aluminium (20 integrity) or iron (50), smart foam only next to an area border, ATMOS resin
//!   (resets the tile's air to 20 °C with only nitrogen, oxygen, pluoxium and halon left, seals
//!   vents and scrubbers, puts out mobs), the firefighting foam's plasma residue (holding the
//!   plasma it absorbed), the dirty foam's dirt.
//! - Firefighting foam puts out the fire under it: absorbs up to 30 mol of plasma, halves the
//!   air's temperature down to 20 °C, wets mobs (2 wet stacks: fire stacks off).
// shortcut: no foam on elevated turfs or mobs (`TRAIT_ELEVATED_TURF`, `TRAIT_MOB_ELEVATED`), no
// dissolve animation, the smart foam does not plate space (`/turf/open/floor/plating/foam`),
// foamed metal is not a wall base (`uses_girder_wall_recipes`) and does not bounce hands off
// (`attack_hand`); the foam's hotspot is put out by its cooling, not deleted; no admin log; the
// carried reagents never react in the foam (tg nodes are `REAGENT_HOLDER_INSTANT_REACT`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_atmos::GasId;
use ss13_mapload::fidelity::is_under;
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, TilePos};
use ss13_world::{Moved, SpatialIndex};

use super::{FluidGroup, FluidGroups, Pass, on_tile, step_blocked};
use crate::chem::Chem;
use crate::combat::CombatRng;
use crate::health::Health;

pub const FOAM: &str = "/obj/effect/particle_effect/fluid/foam";
pub const FOAM_SHORT: &str = "/obj/effect/particle_effect/fluid/foam/short_life";
pub const FOAM_LONG: &str = "/obj/effect/particle_effect/fluid/foam/long_life";
pub const FOAM_FIREFIGHTING: &str = "/obj/effect/particle_effect/fluid/foam/firefighting";
pub const FOAM_METAL: &str = "/obj/effect/particle_effect/fluid/foam/metal";
pub const FOAM_IRON: &str = "/obj/effect/particle_effect/fluid/foam/metal/iron";
pub const FOAM_SMART: &str = "/obj/effect/particle_effect/fluid/foam/metal/smart";
pub const FOAM_RESIN: &str = "/obj/effect/particle_effect/fluid/foam/metal/resin";
pub const FOAM_RESIN_HALON: &str = "/obj/effect/particle_effect/fluid/foam/metal/resin/halon";
pub const FOAM_DIRTY: &str = "/obj/effect/particle_effect/fluid/foam/dirty";
pub const FOAMED_METAL: &str = "/obj/structure/foamedmetal";
pub const FOAMED_IRON: &str = "/obj/structure/foamedmetal/iron";
pub const RESIN: &str = "/obj/structure/foamedmetal/resin";
pub const PLASMA_RESIDUE: &str = "/obj/effect/decal/cleanable/plasma";
pub const DIRT: &str = "/obj/effect/decal/cleanable/dirt";

/// `SSfoam`: `spread_wait`, `effect_wait`.
const FOAM_WAIT: f64 = 0.2;
/// `kill_foam`: `QDEL_IN(src, 0.5 SECONDS)`.
const DISSOLVE_SECS: f64 = 0.5;
/// `MINIMUM_FOAM_DILUTION` (`DIAMOND_AREA(3)`), `FOAM_REAGENT_SCALE`.
const MINIMUM_DILUTION: f64 = 25.0;
pub const FOAM_REAGENT_SCALE: f32 = 3.2;
/// `chemholder = new(1000, NO_REACT)`.
const CHEM_VOLUME: f32 = 1000.0;
/// `should_atmos_process`: `exposed_temperature > 475`.
const DISSOLVE_TEMP: f64 = 475.0;
/// `/datum/component/slippery, 100`: 10 s knockdown, walking does not help.
const SLIP_KNOCKDOWN: f32 = 10.0;
/// Firefighting foam: `min(30, plasma)` absorbed per process, `T20C` floor of the cooling, 2
/// wet stacks.
const ABSORB_PLASMA: f64 = 30.0;
const WET_STACKS: f32 = 2.0;
/// Resin keeps these gases (`ignored_gases`).
const RESIN_KEEPS: [GasId; 4] = [GasId::N2, GasId::O2, GasId::PLUOXIUM, GasId::HALON];

/// The ported `/obj/effect/particle_effect/fluid/foam` types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoamKind {
    Plain,
    ShortLife,
    LongLife,
    Firefighting,
    Metal,
    Iron,
    Smart,
    Resin,
    ResinHalon,
    Dirty,
}

impl FoamKind {
    pub fn path(self) -> &'static str {
        match self {
            Self::Plain => FOAM,
            Self::ShortLife => FOAM_SHORT,
            Self::LongLife => FOAM_LONG,
            Self::Firefighting => FOAM_FIREFIGHTING,
            Self::Metal => FOAM_METAL,
            Self::Iron => FOAM_IRON,
            Self::Smart => FOAM_SMART,
            Self::Resin => FOAM_RESIN,
            Self::ResinHalon => FOAM_RESIN_HALON,
            Self::Dirty => FOAM_DIRTY,
        }
    }

    /// `initial(lifetime)`, seconds (the firefighting foam's `20` is deciseconds).
    pub fn lifetime(self) -> f64 {
        match self {
            Self::ShortLife => 1.0,
            Self::LongLife => 30.0,
            Self::Firefighting => 2.0,
            _ => 8.0,
        }
    }

    /// `result_type`.
    pub fn result(self) -> Option<&'static str> {
        match self {
            Self::Firefighting => Some(PLASMA_RESIDUE),
            Self::Metal | Self::Smart => Some(FOAMED_METAL),
            Self::Iron => Some(FOAMED_IRON),
            Self::Resin | Self::ResinHalon => Some(RESIN),
            Self::Dirty => Some(DIRT),
            _ => None,
        }
    }

    /// `slippery_foam`.
    fn slippery(self) -> bool {
        matches!(
            self,
            Self::Plain | Self::ShortLife | Self::LongLife | Self::Dirty
        )
    }

    /// `allow_duplicate_results`.
    fn duplicates(self) -> bool {
        !matches!(self, Self::Firefighting | Self::Dirty)
    }

    /// `atmos_sensitive`: heat dissolves it.
    fn dissolves(self) -> bool {
        !matches!(self, Self::Firefighting | Self::ResinHalon)
    }

    /// The system's `reagent_scale`.
    fn reagent_scale(self) -> f32 {
        if self == Self::LongLife {
            FOAM_REAGENT_SCALE * (30.0 / 8.0)
        } else {
            FOAM_REAGENT_SCALE
        }
    }
}

/// A foam node.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Foam {
    pub kind: FoamKind,
    pub group: u32,
    pub lifetime: f64,
    /// `original_lifetime`: what its spread inherits.
    pub original_lifetime: f64,
    pub slippery: bool,
    pub result: Option<&'static str>,
    pub next_process: f64,
    /// Dissolving (`kill_foam`): gone at this time.
    pub gone_at: Option<f64>,
    /// Firefighting foam: plasma it took from the air, moles.
    pub absorbed_plasma: f64,
}

/// `SSfoam.queue_spread`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct FoamSpread(f64);

/// Options of `do_foam` / `create_foam` beyond the kind.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FoamOptions {
    /// `lifetime` (default: the kind's).
    pub lifetime: Option<f64>,
    /// `slippery` (default: the kind's).
    pub slippery: Option<bool>,
    /// `result_type` (default: the kind's).
    pub result: Option<&'static str>,
}

/// tg `do_foam(range, holder, location, carry, amount, foam_type, result_type)`: a flood of
/// `amount` nodes (`DIAMOND_AREA(range)` by default) carrying a copy of `carry`.
pub fn do_foam(
    world: &mut World,
    range: u32,
    pos: TilePos,
    amount: Option<u32>,
    carry: Option<&Holder>,
    kind: FoamKind,
    options: FoamOptions,
) -> Option<u32> {
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let mut reagents = None;
    let mut color = None;
    if let (Some(chem), Some(carry)) = (chem, carry) {
        let mut held = Holder::new(CHEM_VOLUME, flags::NO_REACT, Owner::default());
        let mut source = carry.clone();
        let volume = source.total_volume;
        source.trans_to(&chem, &mut held, volume, 1.0, None, true, true);
        color = ss13_reagents::mix_color(&chem, &held.reagents);
        // `foam.reagents.maximum_volume *= reagent_scale`, then the amplified copy.
        let scale = kind.reagent_scale();
        let mut node = Holder::new(CHEM_VOLUME * scale.max(1.0), 0, Owner::default());
        let total = held.total_volume;
        held.trans_to(&chem, &mut node, total, scale, None, true, true);
        reagents = Some(node);
    }
    let group = world.resource_mut::<FluidGroups>().create(FluidGroup {
        target_size: amount.unwrap_or_else(|| super::smoke::diamond_area(range)),
        reagents,
        color,
        ..default()
    });
    let lifetime = options.lifetime.unwrap_or_else(|| kind.lifetime());
    let slippery = options.slippery.unwrap_or_else(|| kind.slippery());
    let result = options.result.or_else(|| kind.result());
    info!("{kind:?} foam flood started at {pos:?}");
    if spawn_node(world, kind, pos, group, lifetime, slippery, result).is_none() {
        world.resource_mut::<FluidGroups>().groups.remove(&group);
        return None;
    }
    Some(group)
}

fn spawn_node(
    world: &mut World,
    kind: FoamKind,
    pos: TilePos,
    group: u32,
    lifetime: f64,
    slippery: bool,
    result: Option<&'static str>,
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
    world.entity_mut(e).insert((
        Foam {
            kind,
            group,
            lifetime,
            original_lifetime: lifetime,
            slippery,
            result,
            next_process: now + FOAM_WAIT,
            gone_at: None,
            absorbed_plasma: 0.0,
        },
        FoamSpread(now + FOAM_WAIT),
    ));
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/bubbles/bubbles2.ogg", 80.0, true)
            .extrarange(-3.0),
    );
    Some(e)
}

fn has_foam(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<Foam>(e).is_some())
}

/// `/obj/effect/particle_effect/fluid/foam/spread`.
fn spread(world: &mut World, node: Entity) {
    let (Some(mut foam), Some(pos)) = (
        world.get::<Foam>(node).copied(),
        world.get::<TilePos>(node).copied(),
    ) else {
        return;
    };
    for dir in Dir::CARDINALS {
        if !world.resource::<FluidGroups>().can_grow(foam.group) {
            return;
        }
        if step_blocked(world, pos, dir, Pass::Foam) {
            continue;
        }
        let to = pos.step(dir);
        if has_foam(world, to) || blacklisted(world, to) {
            continue;
        }
        let mobs: Vec<Entity> = on_tile(world, to)
            .into_iter()
            .filter(|&m| world.get::<Health>(m).is_some())
            .collect();
        for mob in mobs {
            foam_mob(world, mob, &mut foam, FOAM_WAIT);
        }
        world.entity_mut(node).insert(foam);
        spawn_node(
            world,
            foam.kind,
            to,
            foam.group,
            foam.original_lifetime,
            foam.slippery,
            foam.result,
        );
    }
}

/// `blacklisted_turfs`: transit space, chasms, lava.
fn blacklisted(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| {
            is_under(&p, "/turf/open/space/transit")
                || is_under(&p, "/turf/open/chasm")
                || is_under(&p, "/turf/open/lava")
        })
}

/// `foam_mob`: a mob in the foam uses up its lifetime (the firefighting foam wets it instead).
/// True when it counted as a hit.
fn foam_mob(world: &mut World, mob: Entity, foam: &mut Foam, secs: f64) -> bool {
    if foam.kind == FoamKind::Firefighting {
        // Wet stacks cancel fire stacks (`fire_handler` `enemy_types`).
        if world.get::<crate::fire::FireStacks>(mob).is_some() {
            crate::fire::adjust_fire_stacks(world, mob, -WET_STACKS);
        }
        return false;
    }
    if foam.lifetime <= 0.0 {
        return false;
    }
    foam.lifetime -= secs.min(foam.lifetime);
    true
}

/// `SSfoam`: queued spreads, then `process` of every node due.
// Cost: per tick, one pass over the foam nodes; each due node looks at its own tile.
pub(super) fn tick_foam(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let nodes: Vec<(Entity, Option<f64>)> = world
        .query_filtered::<(Entity, Option<&FoamSpread>), With<Foam>>()
        .iter(world)
        .map(|(e, q)| (e, q.map(|q| q.0)))
        .collect();
    for &(e, spread_at) in &nodes {
        if spread_at.is_some_and(|at| at <= now) && world.get_entity(e).is_ok() {
            world.entity_mut(e).remove::<FoamSpread>();
            spread(world, e);
        }
    }
    for (e, _) in nodes {
        let Some(&foam) = world.get::<Foam>(e) else {
            continue;
        };
        if let Some(at) = foam.gone_at {
            if now >= at {
                world.despawn(e);
                world.resource_mut::<FluidGroups>().remove_node(foam.group);
            }
            continue;
        }
        if now >= foam.next_process {
            process(world, e, foam, now);
        }
    }
    slip_in_foam(world);
}

/// `/obj/effect/particle_effect/fluid/foam/process(seconds_per_tick = 0.2)`.
fn process(world: &mut World, e: Entity, mut foam: Foam, now: f64) {
    foam.next_process = now + FOAM_WAIT;
    foam.lifetime -= FOAM_WAIT;
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    if foam.lifetime <= 0.0 || dissolved_by_heat(world, &foam, pos) {
        kill_foam(world, e, foam, pos, now);
        return;
    }
    let total = world
        .resource::<FluidGroups>()
        .get(foam.group)
        .map_or(1, |g| g.total_size);
    let fraction = (FOAM_WAIT * MINIMUM_DILUTION
        / (foam.kind.lifetime() * MINIMUM_DILUTION.max(f64::from(total))))
        as f32;
    let held = world
        .resource::<FluidGroups>()
        .get(foam.group)
        .and_then(|g| g.reagents.clone());
    let things = on_tile(world, pos);
    let mut hit = false;
    for &thing in &things {
        if thing == e || world.get_entity(thing).is_err() {
            continue;
        }
        if world.get::<Health>(thing).is_some() {
            hit |= foam_mob(world, thing, &mut foam, FOAM_WAIT);
        } else if let Some(held) = &held
            && world.get::<Foam>(thing).is_none()
        {
            // The turf too (`reagents.expose(loc, VAPOR, fraction)`).
            crate::janitor::expose(world, thing, held, fraction);
        }
    }
    if hit {
        // "So the decrease from mobs hit and the natural decrease don't cumulate."
        foam.lifetime += FOAM_WAIT;
    }
    if foam.kind == FoamKind::Firefighting {
        put_out_fire(world, &mut foam, pos);
    }
    if world.get_entity(e).is_ok() {
        world.entity_mut(e).insert(foam);
    }
}

/// `atmos_expose`: `prob(T - 475)` above 475 K.
fn dissolved_by_heat(world: &mut World, foam: &Foam, pos: TilePos) -> bool {
    if !foam.kind.dissolves() {
        return false;
    }
    let t = world
        .get_resource::<ss13_atmos::Atmos>()
        .and_then(|a| a.temperature(pos))
        .unwrap_or(0.0);
    t > DISSOLVE_TEMP
        && world
            .resource_mut::<CombatRng>()
            .chance((t - DISSOLVE_TEMP) as f32)
}

/// `firefighting/process`: a fire on the tile is smothered.
fn put_out_fire(world: &mut World, foam: &mut Foam, pos: TilePos) {
    let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() else {
        return;
    };
    if atmos.hotspot(pos).is_none() {
        return;
    }
    let absorbed = atmos.modify_air(pos, |air| {
        let scrub = air.get(GasId::PLASMA).min(ABSORB_PLASMA);
        air.add(GasId::PLASMA, -scrub);
        if air.temperature > ss13_atmos::gas::T20C {
            air.temperature = (air.temperature / 2.0).max(ss13_atmos::gas::T20C);
        }
        air.garbage_collect();
        scrub
    });
    foam.absorbed_plasma += absorbed.unwrap_or(0.0);
}

/// `kill_foam`: the result, then gone in 0.5 s.
fn kill_foam(world: &mut World, e: Entity, mut foam: Foam, pos: TilePos, now: f64) {
    foam.gone_at = Some(now + DISSOLVE_SECS);
    world.entity_mut(e).insert(foam).remove::<FoamSpread>();
    make_result(world, &foam, pos);
}

/// `make_result`.
fn make_result(world: &mut World, foam: &Foam, pos: TilePos) {
    let Some(result) = foam.result else {
        return;
    };
    // `metal/smart/make_result`: only where the foam touches another area.
    if foam.kind == FoamKind::Smart {
        let Some(areas) = world.get_resource::<ss13_mapload::Areas>() else {
            return;
        };
        let here = areas.area_at(pos);
        if Dir::CARDINALS
            .into_iter()
            .all(|d| areas.area_at(pos.step(d)) == here)
        {
            return;
        }
    }
    let existing = on_tile(world, pos)
        .into_iter()
        .find(|&t| crate::construction::path_of(world, t).is_some_and(|p| is_under(&p, result)));
    let deposit = match existing {
        Some(found) if !foam.kind.duplicates() => found,
        _ => {
            let Some(made) = crate::construction::spawn_obj(world, result, pos) else {
                return;
            };
            made
        }
    };
    if is_under(result, RESIN) && existing.is_none_or(|f| f != deposit) {
        resin_init(world, pos);
    }
    // `firefighting/make_result`: the residue holds the plasma.
    if foam.absorbed_plasma > 0.0
        && let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
        && let Some(id) = chem.id("/datum/reagent/stable_plasma")
        && let Some(mut r) = world.get_mut::<crate::chem::Reagents>(deposit)
    {
        r.0.add_reagent(
            &chem,
            id,
            foam.absorbed_plasma as f32,
            ss13_atmos::gas::T20C as f32,
            None,
            None,
            true,
            false,
        );
    }
}

/// `/obj/structure/foamedmetal/resin/Initialize`.
fn resin_init(world: &mut World, pos: TilePos) {
    if let Some(turf) = crate::construction::turf_at(world, pos) {
        world.entity_mut(turf).remove::<crate::janitor::WetFloor>();
    }
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.modify_air(pos, |air| {
            air.temperature = ss13_atmos::gas::T20C;
            for i in 0..ss13_atmos::GAS_COUNT {
                let gas = GasId(i as u8);
                if !RESIN_KEEPS.contains(&gas) {
                    air.set(gas, 0.0);
                }
            }
            air.garbage_collect();
        });
    }
    for e in on_tile(world, pos) {
        weld_vent(world, e, "sealed shut!");
        if world.get::<Health>(e).is_some() {
            crate::fire::extinguish_mob(world, e);
        }
    }
}

/// An unwelded vent pump or scrubber on the tile is welded (`comp.welded = TRUE`), with
/// "[comp] `verb`".
pub(super) fn weld_vent(world: &mut World, e: Entity, verb: &str) {
    use crate::atmos_machinery::devices::{VentPump, VentScrubber};
    let mut welded = false;
    if let Some(mut v) = world.get_mut::<VentPump>(e)
        && !v.welded
    {
        v.welded = true;
        welded = true;
    }
    if let Some(mut v) = world.get_mut::<VentScrubber>(e)
        && !v.welded
    {
        v.welded = true;
        welded = true;
    }
    if welded && let Some(&pos) = world.get::<TilePos>(e) {
        let name = crate::chem::name_of(world, e);
        super::tell_near(world, pos, 7, Span::Danger, &format!("{name} {verb}"));
    }
}

/// The slippery foam's `/datum/component/slippery`: who walks into it slips.
fn slip_in_foam(world: &mut World) {
    let moved: Vec<Moved> = {
        let mut cursor = world
            .remove_resource::<FoamSlipCursor>()
            .unwrap_or_default();
        let read: Vec<Moved> = cursor
            .0
            .read(world.resource::<Messages<Moved>>())
            .copied()
            .collect();
        world.insert_resource(cursor);
        read
    };
    for m in moved {
        if world.get::<Health>(m.entity).is_none() {
            continue;
        }
        let slippery = world
            .resource::<SpatialIndex>()
            .on_tile(m.to)
            .iter()
            .any(|&e| world.get::<Foam>(e).is_some_and(|f| f.slippery));
        if slippery {
            crate::janitor::try_slip(world, m.entity, (SLIP_KNOCKDOWN, false));
        }
    }
}

#[derive(Resource, Default)]
struct FoamSlipCursor(MessageCursor<Moved>);

/// `/datum/reagents/create_foam(foamtype, foam_volume, result_type, notification)`: a flood of
/// `volume` nodes carrying the container's reagents, which are gone.
pub(crate) fn create_foam(
    world: &mut World,
    container: Entity,
    kind: FoamKind,
    volume: u32,
    options: FoamOptions,
    notification: Option<&str>,
) {
    let Some(pos) = crate::explosions::turf_of(world, container) else {
        return;
    };
    let carry = world
        .get::<crate::chem::Reagents>(container)
        .map(|r| r.0.clone());
    do_foam(world, 0, pos, Some(volume), carry.as_ref(), kind, options);
    if let Some(mut r) = world.get_mut::<crate::chem::Reagents>(container) {
        r.0.clear();
    }
    if let Some(text) = notification {
        super::tell_near(world, pos, 5, Span::Danger, text);
    }
}

/// The halon combustion's resin (`halon_o2removal/react`): `do_foam(amount = 1, resin/halon)`
/// on each turf it asked for that has neither foam nor resin.
// Cost: per tick, the turfs halon burned on this atmos fire (usually none).
pub(super) fn halon_resin(world: &mut World) {
    let Some(tiles) = world
        .get_resource_mut::<ss13_atmos::Atmos>()
        .map(|mut a| a.drain_resin())
    else {
        return;
    };
    for pos in tiles {
        let covered = on_tile(world, pos).into_iter().any(|e| {
            world.get::<Foam>(e).is_some()
                || crate::construction::path_of(world, e).is_some_and(|p| is_under(&p, RESIN))
        });
        if !covered {
            do_foam(
                world,
                0,
                pos,
                Some(1),
                None,
                FoamKind::ResinHalon,
                FoamOptions::default(),
            );
        }
    }
}
