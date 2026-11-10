//! Supply MOD modules (server), after tg `code/modules/mod/modules/modules_supply.dm`:
//!
//! - internal GPS: the module is a GPS (`crate::devices::gps`, tag `MOD0`); used, it opens the
//!   GPS window for the wearer.
//! - hydraulic clamp (and the loader's): selected, a middle click on an adjacent crate or large
//!   parcel loads it after `load_time` (3 s; loader 1 s), up to `max_crates` (3; loader 5); a
//!   click elsewhere with crates loaded sets the last one down on that free tile after the same
//!   delay. The crates fall out when its parts unseal.
//! - drill: selected, a middle click on an adjacent mineral wall digs it (`manual_mine` with
//!   `toolspeed` 0.25); while active, walking into a wall digs it too (twice as fast with ore)
//!   and warns of unstruck gibtonite. An active sphere transform turns the drill on with
//!   `toolspeed` 0.075, half the energy and a fifth of the mining experience.
//! - ore bag: every step of the wearer scoops the ore on the new tile into the module, merged
//!   into its stacks; used, it drops all of it.
//! - loader hydraulic arms: selected, a middle click charges for 2 s (a moved or downed wearer
//!   launches early, weaker), then throws the wearer towards the click as far as the charge
//!   (deciseconds) at `round(0.2 × power)` speed.
//! - disposal selector: while its parts are sealed, a disposal flush carrying the wearer goes to
//!   the module's configured destination tag.
//! - loader hydraulic magnet: selected, a middle click on a closet in view pulls it at the wearer
//!   (thrown, range 7) and the wearer starts pulling it on landing; with a closet pulled, the
//!   click throws it at the target instead. Turning it off lets go of a pulled closet.
//! - ash accretion: while sealed, each step on asteroid, ash or dirt adds a layer of armor to
//!   every part (up to 10 tiles: 30 melee, 10 bullet, 20 laser, 20 energy, 30 bomb) and at the
//!   full layer the suit is 0.5 faster; each step elsewhere (but grass, snow, ice, sand, lava,
//!   water) loses one. The wearer is ash storm immune.
//! - sphere transform: on (with gravity), the wearer is 0.5 faster and the parts get 20 melee and
//!   bomb armor; it turns off without gravity or a conscious wearer; only below 50 kPa may it be
//!   used.
// shortcut: no micromissiles of the sphere (no mining missile projectile), no ball filters,
// spin, footsteps, forced standing or blocked hands, no bileworm hide plating (lava immunity);
// the drill does not dig asteroid sand; a crate with any mob inside is too heavy for the clamp
// (tg: only mobs above `MOB_SIZE_SMALL`); the ore bag does not catch ore landing on the wearer's
// tile; the arms have no screen zoom or lightning overlay; the magnet's pull has no
// `TRAIT_STRONGPULL`; ash leaves no trail and gives no snow storm immunity; no candela spool
// (no candela beacons yet). Add them with those mechanics.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmArmor, DmMineral, DmOre, DmTurfBuild};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_world::{Bumped, Moved, SpatialIndex, WorldSystems};

use super::ModJob;
use super::modules::{Kind, ModModule, SelectUse, def_of, drain_power};
use super::server::{ModSuit, name, tell_mob, worn_suit};
use crate::construction::{Job, path_of, turf_at};
use crate::health::Stat;
use crate::inventory::Inventory;
use crate::server::PlayerMob;
use crate::storage::{Stored, StoredIn, put_inside, take_out};
use crate::tools::{Actor, DoAfter, do_after, now};

pub const DRY_FIRE: &str = "sound/items/weapons/gun/general/dry_fire.ogg";
const HYDRAULIC_SOUND: &str = "sound/vehicles/mecha/hydraulic.ogg";
const LOADER_CHARGE: &str = "sound/items/modsuit/loader_charge.ogg";
const LOADER_LAUNCH: &str = "sound/items/modsuit/loader_launch.ogg";
const GRAVHIT: &str = "sound/effects/gravhit.ogg";
const BALLIN: &str = "sound/items/modsuit/ballin.ogg";
const SPARKS: &str = "sound/effects/sparks/sparks1.ogg";
const SCANBUZZ: &str = "sound/machines/scanner/scanbuzz.ogg";
/// `/obj/item/mod/module/clamp` `accepted_items`.
const CRATE_TYPES: [&str; 2] = ["/obj/structure/closet/crate", "/obj/item/delivery/big"];
/// The drill's `toolspeed`, in sphere mode, and the sphere's experience multiplier.
const DRILL_TOOLSPEED: f32 = 0.25;
const SPHERE_DRILL_TOOLSPEED: f32 = 0.075;
pub const SPHERE_EXP_MULT: f32 = 0.2;
/// `gibtonite_warning_cd`.
const GIBTONITE_WARNING_SECS: f64 = 3.0;
/// The hydraulic arms' `launch_time`, seconds.
const LAUNCH_SECS: f64 = 2.0;
/// The magnet's throw range and speeds.
const MAGNET_RANGE: u32 = 7;
/// Ash accretion: `max_traveled_tiles`, `speed_added`, `armor_mod` per tile.
const ASH_MAX_TILES: u32 = 10;
const ASH_SPEED: f32 = -0.5;
const ASH_ARMOR: [(&str, f32); 5] = [
    ("melee", 3.0),
    ("bullet", 1.0),
    ("laser", 2.0),
    ("energy", 2.0),
    ("bomb", 3.0),
];
const ACCRETION_TURFS: [&str; 3] = [
    "/turf/open/misc/asteroid",
    "/turf/open/misc/ashplanet",
    "/turf/open/misc/dirt",
];
const KEEP_TURFS: [&str; 10] = [
    "/turf/open/misc/grass",
    "/turf/open/floor/plating/snowed",
    "/turf/open/misc/sandy_dirt",
    "/turf/open/misc/ironsand",
    "/turf/open/misc/ice",
    "/turf/open/indestructible/hierophant",
    "/turf/open/indestructible/boss",
    "/turf/open/indestructible/necropolis",
    "/turf/open/lava",
    "/turf/open/water",
];
/// `/datum/movespeed_modifier/sphere`, deciseconds per tile.
pub const SPHERE_SPEED: f32 = -0.5;
const SPHERE_ARMOR: [(&str, f32); 2] = [("melee", 20.0), ("bomb", 20.0)];

/// The clamp's `stored_crates`.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct ClampCargo(pub Vec<Entity>);

/// The disposal selector's `disposal_tag` (1-based into `TAGGER_LOCATIONS`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisposalTag(pub u8);

/// Ash accretion's `traveled_tiles`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AshCoat(pub u32);

/// On a wearer: `TRAIT_ASHSTORM_IMMUNE` from sealed ash accretion.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AshImmune(pub Entity);

/// The drill's `gibtonite_warning_cd`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct GibtoniteWarned(pub f64);

/// On a wearer: the hydraulic arms charging (tg's `do_after` of `launch_time`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct HydraulicCharge {
    pub control: Entity,
    pub module: Entity,
    pub target: TilePos,
    pub from: TilePos,
    pub started: f64,
}

/// On a closet the magnet pulls: the wearer starts pulling it when it lands (`check_locker`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MagnetPull {
    pub wearer: Entity,
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            (scoop_ore, accrete_ash, bump_mine)
                .after(WorldSystems::Movement)
                .run_if(resource_exists::<Messages<Moved>>),
            hydraulic_charges.run_if(any_with_component::<HydraulicCharge>),
            magnet_landings.run_if(any_with_component::<MagnetPull>),
        ),
    );
}

fn sound(world: &mut World, e: Entity, file: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, vol, true));
    }
}

fn use_cost(world: &World, module: Entity) -> f64 {
    def_of(world, module).map_or(0.0, |d| d.use_energy_cost)
}

fn adjacent(world: &World, a: Entity, b: Entity) -> bool {
    match (
        world.get::<TilePos>(a).copied(),
        crate::chem::map_pos(world, b),
    ) {
        (Some(a), Some(b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    }
}

fn actor(world: &World, ctx: &SelectUse) -> Actor {
    Actor {
        client: ctx.client,
        mob: ctx.wearer,
        item: world
            .get::<Inventory>(ctx.wearer)
            .and_then(Inventory::active_item),
    }
}

/// `Initialize` of a supply module.
pub fn init(world: &mut World, module: Entity, kind: Kind) {
    match kind {
        // `AddComponent(/datum/component/gps/item, "MOD0", overlay_state = FALSE)`.
        Kind::Gps => {
            let mut gps = crate::devices::gps::Gps::of("");
            gps.tag = "MOD0".to_owned();
            gps.overlay = false;
            world
                .entity_mut(module)
                .insert((gps, ss13_tgui::server::AlwaysState));
        }
        // tg picks a location name (`pick(GLOB.TAGGERLOCATIONS)`) where a tag number is meant.
        Kind::DisposalConnector => {
            let count = crate::disposals::mail::TAGGER_LOCATIONS.len() as u32;
            let tag = world
                .resource_mut::<crate::combat::CombatRng>()
                .range(1, count) as u8;
            world.entity_mut(module).insert(DisposalTag(tag));
        }
        _ => {}
    }
}

/// `has_gravity()` of a mob (`mob_negates_gravity` counts).
pub fn has_gravity(world: &World, mob: Entity) -> bool {
    world
        .get::<TilePos>(mob)
        .is_none_or(|&pos| !crate::spacemove::weightless_at(world, mob, pos))
}

/// `lavaland_equipment_pressure_check` of the wearer's tile.
pub fn low_pressure(world: &World, mob: Entity) -> bool {
    crate::explosions::turf_of(world, mob)
        .is_none_or(|pos| !crate::mining::rocks::pressure_over(world, pos))
}

/// Adds `mult` times `armor` to every part of the suit (`set_armor(add_other_armor)`).
fn adjust_armor(world: &mut World, control: Entity, armor: &[(&str, f32)], mult: f32) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    for part in suit.parts.iter().map(|p| p.item).chain([control]) {
        let Some(mut a) = world.get_mut::<DmArmor>(part) else {
            continue;
        };
        for &(flag, value) in armor {
            let slot = match flag {
                "melee" => &mut a.melee,
                "bullet" => &mut a.bullet,
                "laser" => &mut a.laser,
                "energy" => &mut a.energy,
                "bomb" => &mut a.bomb,
                _ => continue,
            };
            *slot += value * mult;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// GPS

/// `gps/on_use`: `attack_self(mod.wearer)`.
pub fn gps_use(world: &mut World, control: Entity, module: Entity) {
    let Some(wearer) = world.get::<ModSuit>(control).and_then(|s| s.wearer) else {
        return;
    };
    if let Some(client) = world.get::<PlayerMob>(wearer).map(|p| p.client) {
        crate::devices::gps::interact(world, client, wearer, module);
    }
}

// ---------------------------------------------------------------------------------------------
// Hydraulic clamp

fn is_crate(world: &World, e: Entity) -> bool {
    path_of(world, e).is_some_and(|p| CRATE_TYPES.iter().any(|root| is_under(&p, root)))
}

fn max_crates(loader: bool) -> usize {
    if loader { 5 } else { 3 }
}

/// `check_crate_pickup`.
fn check_pickup(world: &mut World, module: Entity, wearer: Entity, target: Entity) -> bool {
    let loader = world
        .get::<ModModule>(module)
        .is_some_and(|m| m.kind == Kind::Clamp { loader: true });
    let held = world.get::<ClampCargo>(module).map_or(0, |c| c.0.len());
    if held >= max_crates(loader) {
        tell_mob(world, wearer, Span::Warning, "слишком много ящиков!");
        return false;
    }
    let mut inside = world.query::<&crate::closets::InCloset>();
    if inside.iter(world).any(|c| c.closet == target) {
        tell_mob(world, wearer, Span::Warning, "ящик слишком тяжёлый!");
        return false;
    }
    true
}

/// A tile a crate can be set down on (`!is_blocked_turf()`).
fn free_tile(world: &World, pos: TilePos) -> bool {
    world.resource::<SpatialIndex>().in_bounds(pos)
        && world
            .resource::<SpatialIndex>()
            .dense_on_tile(pos)
            .is_empty()
}

/// `clamp/on_select_use`.
pub fn clamp(world: &mut World, ctx: SelectUse, loader: bool) {
    if !adjacent(world, ctx.wearer, ctx.target) {
        return;
    }
    let delay_ds = if loader { 10.0 } else { 30.0 };
    if is_crate(world, ctx.target) {
        if !check_pickup(world, ctx.module, ctx.wearer, ctx.target) {
            return;
        }
        sound(world, ctx.control, HYDRAULIC_SOUND, 25.0);
        let job = ModJob::ClampLoad {
            control: ctx.control,
            module: ctx.module,
        };
        let actor = actor(world, &ctx);
        do_after(world, actor, ctx.target, delay_ds, 0, Job::ModSuit(job));
        return;
    }
    let loaded = world
        .get::<ClampCargo>(ctx.module)
        .is_some_and(|c| !c.0.is_empty());
    if loaded {
        let Some(pos) = crate::chem::map_pos(world, ctx.target) else {
            return;
        };
        let Some(turf) = turf_at(world, pos) else {
            return;
        };
        if !free_tile(world, pos) {
            return;
        }
        sound(world, ctx.control, HYDRAULIC_SOUND, 25.0);
        let job = ModJob::ClampDrop {
            control: ctx.control,
            module: ctx.module,
        };
        let actor = actor(world, &ctx);
        do_after(world, actor, turf, delay_ds, 0, Job::ModSuit(job));
        return;
    }
    tell_mob(world, ctx.wearer, Span::Warning, "неверная цель!");
}

/// `clamp/on_part_deactivation`: every crate falls out.
pub fn drop_crates(world: &mut World, control: Entity, module: Entity) {
    let crates = world
        .get_mut::<ClampCargo>(module)
        .map(|mut c| std::mem::take(&mut c.0))
        .unwrap_or_default();
    let Some(pos) = crate::explosions::turf_of(world, control) else {
        return;
    };
    for c in crates {
        if world.get_entity(c).is_ok() {
            take_out(world, c, pos);
        }
    }
}

/// The end of a supply module's `do_after`, checked again at the moment of effect.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: ModJob) {
    match job {
        ModJob::ClampLoad { control, module } => {
            let target = action.target;
            if worn_suit(world, mob) != Some(control)
                || !world
                    .get::<ModSuit>(control)
                    .is_some_and(|s| s.modules.contains(&module))
                || !is_crate(world, target)
                || world.get::<TilePos>(target).is_none()
                || !check_pickup(world, module, mob, target)
            {
                return;
            }
            put_inside(world, target, module);
            match world.get_mut::<ClampCargo>(module) {
                Some(mut c) => c.0.push(target),
                None => {
                    world.entity_mut(module).insert(ClampCargo(vec![target]));
                }
            }
            tell_mob(world, mob, Span::Notice, "ящик поднят");
            drain_power(world, control, use_cost(world, module));
        }
        ModJob::ClampDrop { control, module } => {
            if worn_suit(world, mob) != Some(control) || !free_tile(world, action.target_pos) {
                return;
            }
            let Some(dropped) = world
                .get_mut::<ClampCargo>(module)
                .and_then(|mut c| c.0.pop())
            else {
                return;
            };
            if world.get_entity(dropped).is_err() {
                return;
            }
            take_out(world, dropped, action.target_pos);
            let text = format!("{} опущен", name(world, dropped));
            tell_mob(world, mob, Span::Notice, text);
            drain_power(world, control, use_cost(world, module));
        }
        ModJob::Drill { sphere } => {
            // `if(!ismineralturf(src))`: someone else got it first.
            if world.get::<DmMineral>(action.target).is_none() {
                return;
            }
            let mult = if sphere { SPHERE_EXP_MULT } else { 1.0 };
            crate::skills::mining_experience(world, mob, action.target, mult);
            crate::mining::rocks::gets_drilled(world, action.target_pos, Some(mob));
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Drill

/// The wearer's drill and whether a sphere transform is on (the drill's `ballin`).
fn drill_of(world: &World, wearer: Entity) -> Option<(Entity, Entity, bool, bool)> {
    let control = worn_suit(world, wearer)?;
    let suit = world.get::<ModSuit>(control)?;
    let state = |k: Kind| {
        suit.modules
            .iter()
            .find(|&&m| world.get::<ModModule>(m).is_some_and(|s| s.kind == k))
            .map(|&m| (m, world.get::<ModModule>(m).is_some_and(|s| s.active)))
    };
    let (drill, active) = state(Kind::Drill)?;
    let sphere = state(Kind::Sphere).is_some_and(|(_, on)| on);
    Some((control, drill, active, sphere))
}

/// `manual_mine` with the drill as the tool: a `do_after` of the wall's `tool_mine_speed`
/// times the drill's `toolspeed` and the miner's speed.
fn mine(world: &mut World, actor: Actor, wall: Entity, toolspeed: f32, sphere: bool) {
    let Some(speed) = world.get::<DmMineral>(wall).map(|m| m.tool_mine_speed) else {
        return;
    };
    if world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    let skill = crate::skills::modifier(
        world,
        actor.mob,
        crate::skills::Skill::Mining,
        crate::skills::Modifier::Speed,
    );
    do_after(
        world,
        actor,
        wall,
        speed * toolspeed * skill,
        0,
        Job::ModSuit(ModJob::Drill { sphere }),
    );
}

/// `drill/on_select_use`: an adjacent mineral wall.
pub fn drill(world: &mut World, ctx: SelectUse) {
    if !adjacent(world, ctx.wearer, ctx.target) || world.get::<DmMineral>(ctx.target).is_none() {
        return;
    }
    let sphere = drill_of(world, ctx.wearer).is_some_and(|d| d.3);
    let (toolspeed, cost) = if sphere {
        (SPHERE_DRILL_TOOLSPEED, use_cost(world, ctx.module) * 0.5)
    } else {
        (DRILL_TOOLSPEED, use_cost(world, ctx.module))
    };
    if drain_power(world, ctx.control, cost) {
        let actor = actor(world, &ctx);
        mine(world, actor, ctx.target, toolspeed, sphere);
    }
}

/// `drill/bump_mine`: a wearer with an active drill (or a sphere) walking into a mineral wall.
fn bump_mine(world: &mut World, mut cursor: Local<MessageCursor<Bumped>>) {
    let bumps: Vec<Bumped> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    for bump in bumps {
        let Some(m) = world.get::<DmMineral>(bump.obstacle).cloned() else {
            continue;
        };
        let Some((control, module, active, sphere)) = drill_of(world, bump.entity) else {
            continue;
        };
        if !(active || sphere) || world.get::<DoAfter>(bump.entity).is_some() {
            continue;
        }
        let Some(client) = world.get::<PlayerMob>(bump.entity).map(|p| p.client) else {
            continue;
        };
        let cost = use_cost(world, module) * if sphere { 0.5 } else { 1.0 };
        if !drain_power(world, control, cost) {
            continue;
        }
        let unstruck = world
            .get::<crate::mining::rocks::Gibtonite>(bump.obstacle)
            .is_some_and(|g| g.stage == crate::mining::rocks::GibStage::Unstruck);
        if unstruck {
            let now = now(world);
            if world
                .get::<GibtoniteWarned>(module)
                .is_some_and(|w| now < w.0)
            {
                continue;
            }
            world
                .entity_mut(module)
                .insert(GibtoniteWarned(now + GIBTONITE_WARNING_SECS));
            sound(world, bump.entity, SCANBUZZ, 25.0);
            tell_mob(
                world,
                bump.entity,
                Span::Warning,
                "Обнаружено нестабильное месторождение гибтонита!",
            );
            continue;
        }
        let base = if sphere {
            SPHERE_DRILL_TOOLSPEED
        } else {
            DRILL_TOOLSPEED
        };
        // `mine_rock`: even faster with ore.
        let toolspeed = if m.mineral_type.is_empty() {
            base
        } else {
            base / 2.0
        };
        let actor = Actor {
            client,
            mob: bump.entity,
            item: world
                .get::<Inventory>(bump.entity)
                .and_then(Inventory::active_item),
        };
        mine(world, actor, bump.obstacle, toolspeed, sphere);
    }
}

// ---------------------------------------------------------------------------------------------
// Ore bag

fn suit_module(world: &World, wearer: Entity, kind: Kind) -> Option<(Entity, Entity)> {
    let control = worn_suit(world, wearer)?;
    let module = world
        .get::<ModSuit>(control)?
        .modules
        .iter()
        .copied()
        .find(|&m| world.get::<ModModule>(m).is_some_and(|s| s.kind == kind))?;
    Some((control, module))
}

/// `orebag/move_ore`: merged into the stacks inside, else in.
fn move_ore(world: &mut World, module: Entity, ore: Entity) {
    let stored: Vec<Entity> = world
        .get::<Stored>(module)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for into in stored {
        if world.get_entity(ore).is_err() {
            return;
        }
        if crate::construction::can_merge(world, ore, into) {
            crate::construction::merge_into(world, ore, into);
        }
    }
    if world.get_entity(ore).is_ok() {
        put_inside(world, ore, module);
    }
}

/// `orebag/on_wearer_moved`: the ore on the wearer's new tile.
fn scoop_ore(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Some((_, module)) = suit_module(world, m.entity, Kind::OreBag) else {
            continue;
        };
        let ores: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(m.to)
            .iter()
            .copied()
            .filter(|&e| world.get::<DmOre>(e).is_some() && world.get::<StoredIn>(e).is_none())
            .collect();
        if ores.is_empty() {
            continue;
        }
        for ore in ores {
            move_ore(world, module, ore);
        }
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(m.to, "rustle", 50.0, true),
        );
    }
}

/// `orebag/on_use`: everything out at the unit's feet.
pub fn orebag_use(world: &mut World, control: Entity, module: Entity) {
    let Some(pos) = crate::explosions::turf_of(world, control) else {
        return;
    };
    let ores: Vec<Entity> = world
        .get::<Stored>(module)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for ore in ores {
        take_out(world, ore, pos);
    }
    drain_power(world, control, use_cost(world, module));
}

// ---------------------------------------------------------------------------------------------
// Hydraulic arms

/// `hydraulic/on_select_use`: the charge starts.
pub fn hydraulic(world: &mut World, ctx: SelectUse) {
    let (Some(from), Some(target)) = (
        world.get::<TilePos>(ctx.wearer).copied(),
        crate::chem::map_pos(world, ctx.target),
    ) else {
        return;
    };
    if world.get::<HydraulicCharge>(ctx.wearer).is_some() {
        return;
    }
    let who = name(world, ctx.wearer);
    crate::security::show(
        world,
        from,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ChatChannel::Local,
        (Span::Warning, format!("{who} начинает жужжать!")),
        &[],
    );
    sound(world, ctx.control, LOADER_CHARGE, 75.0);
    tell_mob(
        world,
        ctx.wearer,
        Span::Notice,
        "вы начинаете заряжаться...",
    );
    let started = now(world);
    world.entity_mut(ctx.wearer).insert(HydraulicCharge {
        control: ctx.control,
        module: ctx.module,
        target,
        from,
        started,
    });
}

/// `get_ranged_target_turf_direct(from, target, range)`.
pub fn ranged_target(from: TilePos, target: TilePos, range: u32) -> TilePos {
    let angle = crate::projectile::angle_between(from, target).to_radians();
    let range = range as f32;
    TilePos::new(
        from.x + (angle.sin() * range).round() as i32,
        from.y + (angle.cos() * range).round() as i32,
        from.z,
    )
}

/// The arms' launch: the charge in deciseconds is the range, a fifth of it the speed.
pub fn launch_params(charged_secs: f64) -> (u32, f32) {
    let power = (charged_secs.clamp(0.0, LAUNCH_SECS) * 10.0).round() as u32;
    (power, ((0.2 * power as f32).round()).max(1.0))
}

/// The charge ends when it is full or the `do_after` breaks (moved, downed): the wearer flies.
fn hydraulic_charges(world: &mut World) {
    let now = now(world);
    let mut q = world.query::<(Entity, &HydraulicCharge)>();
    let charges: Vec<(Entity, HydraulicCharge)> = q.iter(world).map(|(e, c)| (e, *c)).collect();
    for (wearer, charge) in charges {
        let elapsed = now - charge.started;
        let broken = world.get::<TilePos>(wearer) != Some(&charge.from)
            || world.get::<Stat>(wearer) != Some(&Stat::Conscious)
            || crate::status_effects::incapacitation(world, wearer).incapacitated;
        if elapsed < LAUNCH_SECS && !broken {
            continue;
        }
        world.entity_mut(wearer).remove::<HydraulicCharge>();
        if worn_suit(world, wearer) != Some(charge.control) {
            continue;
        }
        drain_power(world, charge.control, use_cost(world, charge.module));
        sound(world, charge.control, LOADER_LAUNCH, 75.0);
        let Some(&from) = world.get::<TilePos>(wearer) else {
            continue;
        };
        let (power, speed) = launch_params(elapsed);
        if power == 0 {
            continue;
        }
        let dest = ranged_target(from, charge.target, power);
        if crate::throw::throw_at(world, wearer, dest, power, speed)
            && let Some(mut thrown) = world.get_mut::<crate::throw::Thrown>(wearer)
        {
            thrown.thrower = Some(wearer);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Disposal selector

/// `disposal_handling` (`COMSIG_MOVABLE_DISPOSING`): the tag of a flushed wearer's sealed
/// disposal selector.
pub fn disposal_tag(world: &World, mob: Entity) -> Option<u8> {
    let (_, module) = suit_module(world, mob, Kind::DisposalConnector)?;
    if !world.get::<ModModule>(module)?.part_activated {
        return None;
    }
    world.get::<DisposalTag>(module).map(|t| t.0)
}

/// `disposal_connector/get_configuration`.
pub fn disposal_configuration(world: &World, module: Entity) -> Vec<super::ModuleConfig> {
    use crate::disposals::mail::TAGGER_LOCATIONS;
    let tag = world.get::<DisposalTag>(module).map_or(1, |t| t.0);
    let current = TAGGER_LOCATIONS
        .get(usize::from(tag).saturating_sub(1))
        .copied()
        .unwrap_or_default();
    vec![super::ModuleConfig {
        key: "disposal_tag".into(),
        display_name: "Метка утилизации".into(),
        kind: "list".into(),
        value: current.into(),
        values: TAGGER_LOCATIONS.iter().map(|l| (*l).to_owned()).collect(),
    }]
}

/// `disposal_connector/configure_edit`: only a known location.
pub fn disposal_configure(world: &mut World, module: Entity, value: &str) {
    if let Some(i) = crate::disposals::mail::TAGGER_LOCATIONS
        .iter()
        .position(|l| *l == value)
    {
        world.entity_mut(module).insert(DisposalTag(i as u8 + 1));
    }
}

// ---------------------------------------------------------------------------------------------
// Magnet

fn is_closet(world: &World, e: Entity) -> bool {
    world.get::<crate::closets::Closet>(e).is_some()
}

/// No dense turf between `from` and `to` (tg's view and line-of-sight checks).
// shortcut: opacity is not modeled here; dense turfs (walls) block the line.
pub fn line_clear(world: &World, from: TilePos, to: TilePos) -> bool {
    if from.z != to.z {
        return false;
    }
    let index = world.resource::<SpatialIndex>();
    crate::radiation::get_line(from, to)
        .into_iter()
        .filter(|&p| p != from && p != to)
        .all(|p| {
            !index
                .dense_on_tile(p)
                .iter()
                .any(|&e| world.get::<ss13_mapload::Turf>(e).is_some())
        })
}

/// `magnet/on_select_use`.
pub fn magnet(world: &mut World, ctx: SelectUse) {
    let Some(from) = world.get::<TilePos>(ctx.wearer).copied() else {
        return;
    };
    if let Some(locker) = world
        .get::<crate::pull::Pulling>(ctx.wearer)
        .map(|p| p.0)
        .filter(|&l| is_closet(world, l))
    {
        let Some(target) = crate::chem::map_pos(world, ctx.target) else {
            return;
        };
        world
            .entity_mut(ctx.wearer)
            .remove::<crate::pull::Pulling>();
        sound(world, locker, GRAVHIT, 75.0);
        world.entity_mut(locker).insert(from);
        if crate::throw::throw_at(world, locker, target, MAGNET_RANGE, 4.0)
            && let Some(mut thrown) = world.get_mut::<crate::throw::Thrown>(locker)
        {
            thrown.thrower = Some(ctx.wearer);
        }
        return;
    }
    let target_pos = world.get::<TilePos>(ctx.target).copied();
    let in_view = target_pos.is_some_and(|p| {
        from.chebyshev(p).is_some_and(|d| d <= MAGNET_RANGE) && line_clear(world, from, p)
    });
    if !is_closet(world, ctx.target) || !in_view {
        tell_mob(world, ctx.wearer, Span::Warning, "неверная цель!");
        return;
    }
    if world
        .get::<crate::obj_damage::Anchored>(ctx.target)
        .is_some_and(|a| a.0)
    {
        tell_mob(world, ctx.wearer, Span::Warning, "цель закреплена!");
        return;
    }
    sound(world, ctx.target, GRAVHIT, 75.0);
    if crate::throw::throw_at(world, ctx.target, from, MAGNET_RANGE, 3.0) {
        world
            .entity_mut(ctx.target)
            .insert(MagnetPull { wearer: ctx.wearer });
    }
}

/// `magnet/check_locker` once the closet stopped flying.
fn magnet_landings(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &MagnetPull), Without<crate::throw::Thrown>>();
    let landed: Vec<(Entity, MagnetPull)> = q.iter(world).map(|(e, m)| (e, *m)).collect();
    for (locker, pull) in landed {
        world.entity_mut(locker).remove::<MagnetPull>();
        let Some((_, module)) = suit_module(world, pull.wearer, Kind::Magnet) else {
            continue;
        };
        if !world.get::<ModModule>(module).is_some_and(|m| m.active)
            || !adjacent(world, pull.wearer, locker)
            || world.get::<TilePos>(locker).is_none()
            || world.get::<crate::pull::Pulling>(pull.wearer).is_some()
        {
            continue;
        }
        world.entity_mut(pull.wearer).insert((
            crate::pull::Pulling(locker),
            crate::pull::GrabState::Passive,
            crate::pull::PullSlowdown(0.0),
        ));
    }
}

/// `magnet/on_deactivation`: a pulled closet is let go.
pub fn magnet_off(world: &mut World, wearer: Entity) {
    if world
        .get::<crate::pull::Pulling>(wearer)
        .is_some_and(|p| is_closet(world, p.0))
    {
        world.entity_mut(wearer).remove::<crate::pull::Pulling>();
    }
}

// ---------------------------------------------------------------------------------------------
// Ash accretion

/// What a turf does to the ash: `Some(true)` accretes, `Some(false)` keeps, `None` sheds.
pub fn ash_turf(path: &str) -> Option<bool> {
    if ACCRETION_TURFS.iter().any(|r| is_under(path, r)) {
        Some(true)
    } else if KEEP_TURFS.iter().any(|r| is_under(path, r)) {
        Some(false)
    } else {
        None
    }
}

/// `ash_accretion/on_move`: one step changes `tiles` by this much (0, 1 or -1).
pub fn ash_step(tiles: u32, turf: Option<bool>) -> i32 {
    match turf {
        Some(true) if tiles < ASH_MAX_TILES => 1,
        Some(_) => 0,
        None if tiles > 0 => -1,
        None => 0,
    }
}

fn accrete_ash(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Some((control, module)) = suit_module(world, m.entity, Kind::AshAccretion) else {
            continue;
        };
        if !world
            .get::<ModModule>(module)
            .is_some_and(|s| s.part_activated)
            || world.get::<crate::closets::InCloset>(m.entity).is_some()
        {
            continue;
        }
        let turf = turf_at(world, m.to)
            .and_then(|t| world.get::<DmTurfBuild>(t))
            .map(|b| ash_turf(&b.path))
            .unwrap_or(None);
        let tiles = world.get::<AshCoat>(module).map_or(0, |c| c.0);
        let step = ash_step(tiles, turf);
        if step == 0 {
            continue;
        }
        let new = tiles.saturating_add_signed(step);
        world.entity_mut(module).insert(AshCoat(new));
        adjust_armor(world, control, &ASH_ARMOR, step as f32);
        if new == ASH_MAX_TILES {
            tell_mob(world, m.entity, Span::Notice, "полностью покрыт пеплом");
            sound(world, control, SPARKS, 100.0);
        } else if new == 0 {
            tell_mob(world, m.entity, Span::Warning, "пепел закончился!");
        }
    }
}

/// `ash_accretion/on_part_deactivation`: the armor goes and so does the ash.
pub fn shed_ash(world: &mut World, control: Entity, module: Entity) {
    let tiles = world.get::<AshCoat>(module).map_or(0, |c| c.0);
    world.entity_mut(module).insert(AshCoat(0));
    if tiles > 0 {
        adjust_armor(world, control, &ASH_ARMOR, -(tiles as f32));
    }
    if let Some(wearer) = world.get::<ModSuit>(control).and_then(|s| s.wearer)
        && world
            .get::<AshImmune>(wearer)
            .is_some_and(|a| a.0 == module)
    {
        world.entity_mut(wearer).remove::<AshImmune>();
    }
}

/// `ash_accretion/on_part_activation`: `TRAIT_ASHSTORM_IMMUNE`.
pub fn ash_sealed(world: &mut World, control: Entity, module: Entity) {
    if let Some(wearer) = world.get::<ModSuit>(control).and_then(|s| s.wearer) {
        world.entity_mut(wearer).insert(AshImmune(module));
    }
}

/// `ash_accretion/on_update_speed`.
pub fn ash_slowdown(world: &World, module: Entity) -> f32 {
    let full = world
        .get::<AshCoat>(module)
        .is_some_and(|c| c.0 == ASH_MAX_TILES)
        && world
            .get::<ModModule>(module)
            .is_some_and(|m| m.part_activated);
    if full { ASH_SPEED } else { 0.0 }
}

// ---------------------------------------------------------------------------------------------
// Sphere transform

/// `sphere_transform/on_activation`.
pub fn sphere_on(world: &mut World, control: Entity, _module: Entity, _wearer: Entity) {
    sound(world, control, BALLIN, 100.0);
    adjust_armor(world, control, &SPHERE_ARMOR, 1.0);
}

/// `sphere_transform/on_deactivation`.
pub fn sphere_off(world: &mut World, module: Entity, _wearer: Entity) {
    let Some(control) = world.get::<super::server::InSuit>(module).map(|s| s.0) else {
        return;
    };
    sound(world, control, BALLIN, 100.0);
    adjust_armor(world, control, &SPHERE_ARMOR, -1.0);
}

/// `sphere_transform/on_select_use`: the energy of a micromissile salvo.
// shortcut: no mining micromissiles (see the module docs).
pub fn sphere_fire(world: &mut World, ctx: SelectUse) {
    drain_power(world, ctx.control, use_cost(world, ctx.module));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ash_builds_on_ash_and_wears_off_elsewhere() {
        assert_eq!(ash_turf("/turf/open/misc/asteroid/basalt"), Some(true));
        assert_eq!(ash_turf("/turf/open/misc/grass"), Some(false));
        assert_eq!(ash_turf("/turf/open/floor/iron"), None);
        assert_eq!(ash_step(0, Some(true)), 1);
        assert_eq!(ash_step(ASH_MAX_TILES, Some(true)), 0);
        assert_eq!(ash_step(5, Some(false)), 0);
        assert_eq!(ash_step(5, None), -1);
        assert_eq!(ash_step(0, None), 0);
    }

    #[test]
    fn arms_launch_by_their_charge() {
        assert_eq!(launch_params(2.0), (20, 4.0));
        assert_eq!(launch_params(5.0), (20, 4.0));
        assert_eq!(launch_params(0.5), (5, 1.0));
        assert_eq!(launch_params(0.0), (0, 1.0));
        let from = TilePos::new(10, 10, 1);
        assert_eq!(
            ranged_target(from, TilePos::new(10, 12, 1), 20),
            TilePos::new(10, 30, 1)
        );
        assert_eq!(
            ranged_target(from, TilePos::new(7, 10, 1), 5),
            TilePos::new(5, 10, 1)
        );
    }

    #[test]
    fn clamp_holds_more_on_the_loader() {
        assert_eq!(max_crates(false), 3);
        assert_eq!(max_crates(true), 5);
    }
}
