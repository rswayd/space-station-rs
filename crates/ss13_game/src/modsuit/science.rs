//! Science MOD modules (server), after tg `code/modules/mod/modules/modules_science.dm` and
//! `module_kinesis.dm`; the anomaly core lock itself is [`super::anomalock`]:
//!
//! - reagent scanner: while on, the wearer sees what containers hold (`TRAIT_REAGENT_SCANNER`,
//!   `crate::chem::can_see_reagents`); the advanced one also reports every explosion within 21
//!   tiles on its level (`COMSIG_GLOB_EXPLOSION`).
//! - anti-gravity (gravitational core): while on, the wearer is weightless everywhere
//!   (`/datum/element/forced_gravity` of 0, `crate::spacemove`).
//! - teleporter (bluespace core): selected, a middle click on a free open tile within 9 tiles
//!   in line of sight teleports the wearer there after 1 s (`do_teleport`, bluespace channel).
//! - kinesis (gravitational core): selected, a middle click within 8 tiles grabs an unanchored
//!   item, object or dead mob (8 tiles, line of sight); the next middle click launches it away
//!   from the wearer (`throw_at` 10 tiles, range 8). The hold costs a tenth of `use_energy_cost`
//!   every 0.2 s and breaks out of range or when the wearer is incapacitated.
// shortcut: no research scanner (`TRAIT_RESEARCH_SCANNER`: no examine of research data), no
// mook dust or teleport animation, line of sight through walls only (no opacity); kinesis has
// no cursor catcher (the grabbed thing does not follow the mouse, items are not pulled into the
// hand), no beam, overlay or sound loop, no immobilizing of grabbed mobs and no impact damage of
// launched structures. Add them with mouse tracking and beams.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmObj, DmWeight};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use super::ModJob;
use super::modules::{SelectUse, def_of, drain_power};
use super::server::{ModSuit, tell_mob, worn_suit};
use super::supply::line_clear;
use crate::construction::{Job, turf_at};
use crate::health::{Health, Stat};
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, do_after};

const GRAVHIT: &str = "sound/effects/gravhit.ogg";
const PHASEIN: &str = "sound/effects/phasein.ogg";
const GRAB_SOUND: &str = "sound/items/weapons/contractor_baton/contractorbatonhit.ogg";
const RELEASE_SOUND: &str = "sound/effects/empulse.ogg";
const LAUNCH_SOUND: &str = "sound/effects/magic/repulse.ogg";
/// The advanced scanner's `explosion_detection_dist`.
const EXPLOSION_DETECTION_DIST: u32 = 21;
/// The teleporter's `max_range` and `teleport_time` (deciseconds).
const TELEPORT_RANGE: u32 = 9;
const TELEPORT_DS: f32 = 10.0;
/// Kinesis `grab_range`, the launch's distance and `SSfastprocess`'s period (seconds).
const GRAB_RANGE: u32 = 8;
const LAUNCH_DISTANCE: u32 = 10;
const FASTPROCESS_SECS: f64 = 0.2;
/// `WEIGHT_CLASS_GIGANTIC`.
const WEIGHT_CLASS_GIGANTIC: u8 = 6;

/// On a wearer: an active reagent scanner (`TRAIT_REAGENT_SCANNER`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReagentScanner {
    pub module: Entity,
    pub advanced: bool,
}

/// On a kinesis module: the `grabbed_atom`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct KinesisGrab(pub Entity);

pub(super) fn plugin(_app: &mut App) {}

fn sound(world: &mut World, e: Entity, file: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, vol, true));
    }
}

/// `TRAIT_REAGENT_SCANNER` from an active scanner module.
pub fn sees_reagents(world: &World, mob: Entity) -> bool {
    world.get::<ReagentScanner>(mob).is_some()
}

/// `reagent_scanner/advanced/sense_explosion` of every wearer with one on.
pub fn sense_explosion(world: &mut World, epicenter: TilePos, ranges: [f32; 3]) {
    let mut q = world.query::<(Entity, &ReagentScanner)>();
    let wearers: Vec<Entity> = q
        .iter(world)
        .filter(|(_, r)| r.advanced)
        .map(|(e, _)| e)
        .collect();
    for wearer in wearers {
        let near = crate::explosions::turf_of(world, wearer).is_some_and(|at| {
            at.chebyshev(epicenter)
                .is_some_and(|d| d <= EXPLOSION_DETECTION_DIST)
        });
        if near {
            let [dev, heavy, light] = ranges;
            tell_mob(
                world,
                wearer,
                Span::Notice,
                format!(
                    "Обнаружен взрыв! Эпицентр: {dev}, внешний радиус: {heavy}, ударная волна: \
                     {light}"
                ),
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Anti-gravity

/// `antigrav/on_activation`: `AddElement(/datum/element/forced_gravity, 0)`.
pub fn antigrav_on(world: &mut World, control: Entity, wearer: Entity) {
    world
        .entity_mut(wearer)
        .insert(crate::gravity::ForcedGravity(0.0));
    sound(world, control, GRAVHIT, 50.0);
}

/// `antigrav/on_deactivation`.
pub fn antigrav_off(world: &mut World, module: Entity, wearer: Entity) {
    if world
        .get::<crate::gravity::ForcedGravity>(wearer)
        .is_some_and(|g| g.0 == 0.0)
    {
        world
            .entity_mut(wearer)
            .remove::<crate::gravity::ForcedGravity>();
    }
    sound(world, module, GRAVHIT, 50.0);
}

// ---------------------------------------------------------------------------------------------
// Teleporter

/// `is_blocked_turf_ignore_climbable`: a dense thing on the tile that is not a table.
fn blocked(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| !crate::tables::is_table(world, e))
}

/// Why the teleporter refuses `dest` from `from`, if it does (the checks of `on_select_use`).
pub fn teleport_refusal(world: &World, from: TilePos, dest: TilePos) -> Option<&'static str> {
    if from.chebyshev(dest).is_none_or(|d| d > TELEPORT_RANGE) {
        return Some("слишком далеко!");
    }
    let open = turf_at(world, dest).is_some_and(|t| world.get::<ss13_world::Density>(t).is_none());
    if !open {
        return Some("неверная цель!");
    }
    if blocked(world, dest) || !line_clear(world, from, dest) {
        return Some("место назначения заблокировано!");
    }
    if !crate::teleport::check_teleport_valid(world, from, dest) {
        return Some("что-то удерживает вас!");
    }
    None
}

/// `teleporter/on_select_use`: the checks, then a 1 s `do_after`.
pub fn teleporter(world: &mut World, ctx: SelectUse) {
    let (Some(from), Some(dest)) = (
        world.get::<TilePos>(ctx.wearer).copied(),
        crate::chem::map_pos(world, ctx.target),
    ) else {
        return;
    };
    if let Some(refusal) = teleport_refusal(world, from, dest) {
        tell_mob(world, ctx.wearer, Span::Warning, refusal);
        return;
    }
    tell_mob(world, ctx.wearer, Span::Notice, "телепортация...");
    let actor = Actor {
        client: ctx.client,
        mob: ctx.wearer,
        item: world
            .get::<crate::inventory::Inventory>(ctx.wearer)
            .and_then(crate::inventory::Inventory::active_item),
    };
    let job = ModJob::Teleport {
        control: ctx.control,
        module: ctx.module,
        dest,
    };
    if !do_after(world, actor, ctx.wearer, TELEPORT_DS, 0, Job::ModSuit(job)) {
        tell_mob(world, ctx.wearer, Span::Warning, "прервано!");
    }
}

/// The teleporter's `do_teleport` after its `do_after`, if the suit is still worn.
pub fn complete(world: &mut World, mob: Entity, _action: &DoAfter, job: ModJob) {
    let ModJob::Teleport {
        control,
        module,
        dest,
    } = job
    else {
        return;
    };
    if worn_suit(world, mob) != Some(control)
        || !world
            .get::<ModSuit>(control)
            .is_some_and(|s| s.modules.contains(&module))
    {
        return;
    }
    let tele = crate::teleport::Tele {
        sound_in: Some(PHASEIN),
        ..Default::default()
    };
    if crate::teleport::do_teleport(world, mob, dest, tele) {
        let cost = def_of(world, module).map_or(0.0, |d| d.use_energy_cost);
        drain_power(world, control, cost);
    }
}

// ---------------------------------------------------------------------------------------------
// Kinesis

/// `kinesis/can_grab`.
pub fn can_grab(world: &World, wearer: Entity, target: Entity) -> bool {
    if target == wearer || world.get::<TilePos>(target).is_none() {
        return false;
    }
    if world.get::<ss13_mapload::Turf>(target).is_some()
        || world.get::<crate::throw::Thrown>(target).is_some()
    {
        return false;
    }
    let path = crate::construction::path_of(world, target).unwrap_or_default();
    if is_under(&path, "/obj/effect") {
        return false;
    }
    if world.get::<Health>(target).is_some() {
        // `stat_required = DEAD`, not buckled.
        return world.get::<Stat>(target) == Some(&Stat::Dead)
            && world
                .get::<crate::medical::surgery::Buckled>(target)
                .is_none();
    }
    if let Some(item) = world.get::<DmItem>(target) {
        let w_class = world.get::<DmWeight>(target).map_or(0, |w| w.w_class);
        return !item.anchored && w_class < WEIGHT_CLASS_GIGANTIC;
    }
    world.get::<DmObj>(target).is_some()
        && !world
            .get::<crate::obj_damage::Anchored>(target)
            .is_none_or(|a| a.0)
}

/// `kinesis/range_check`.
fn in_range(world: &World, wearer: Entity, target: Entity) -> bool {
    match (
        world.get::<TilePos>(wearer).copied(),
        world.get::<TilePos>(target).copied(),
    ) {
        (Some(a), Some(b)) => {
            a.chebyshev(b).is_some_and(|d| d <= GRAB_RANGE) && line_clear(world, a, b)
        }
        _ => false,
    }
}

/// `kinesis/on_select_use`: launch what is held, else grab the target.
pub fn kinesis(world: &mut World, ctx: SelectUse) {
    if world.get::<PlayerMob>(ctx.wearer).is_none() {
        return;
    }
    if let Some(grabbed) = world.get::<KinesisGrab>(ctx.module).map(|g| g.0) {
        clear_grab(world, ctx.module, false);
        launch(world, ctx.wearer, grabbed);
        return;
    }
    if !in_range(world, ctx.wearer, ctx.target) {
        tell_mob(world, ctx.wearer, Span::Warning, "слишком далеко!");
        return;
    }
    if !can_grab(world, ctx.wearer, ctx.target) {
        tell_mob(world, ctx.wearer, Span::Warning, "не удаётся схватить!");
        return;
    }
    let cost = def_of(world, ctx.module).map_or(0.0, |d| d.use_energy_cost);
    drain_power(world, ctx.control, cost);
    world.entity_mut(ctx.module).insert(KinesisGrab(ctx.target));
    sound(world, ctx.target, GRAB_SOUND, 75.0);
}

/// `kinesis/clear_grab`.
pub fn clear_grab(world: &mut World, module: Entity, playsound: bool) {
    let Some(grabbed) = world.get::<KinesisGrab>(module).map(|g| g.0) else {
        return;
    };
    world.entity_mut(module).remove::<KinesisGrab>();
    if playsound && world.get_entity(grabbed).is_ok() {
        sound(world, grabbed, RELEASE_SOUND, 75.0);
    }
}

/// `kinesis/launch`: away from the wearer, 10 tiles out.
fn launch(world: &mut World, wearer: Entity, thing: Entity) {
    let (Some(from), Some(at)) = (
        world.get::<TilePos>(wearer).copied(),
        world.get::<TilePos>(thing).copied(),
    ) else {
        return;
    };
    sound(world, thing, LAUNCH_SOUND, 100.0);
    let dest = super::supply::ranged_target(from, at, LAUNCH_DISTANCE);
    let speed = if world.get::<ss13_world::Density>(thing).is_some() {
        3.0
    } else {
        4.0
    };
    if crate::throw::throw_at(world, thing, dest, GRAB_RANGE, speed)
        && let Some(mut thrown) = world.get_mut::<crate::throw::Thrown>(thing)
    {
        thrown.thrower = Some(wearer);
    }
}

/// `kinesis/process`: the hold's upkeep while the module is on.
pub fn kinesis_process(world: &mut World, control: Entity, module: Entity, dt: f64) {
    let Some(grabbed) = world.get::<KinesisGrab>(module).map(|g| g.0) else {
        return;
    };
    let Some(wearer) = world.get::<ModSuit>(control).and_then(|s| s.wearer) else {
        clear_grab(world, module, true);
        return;
    };
    let gone = world.get_entity(grabbed).is_err()
        || world.get::<PlayerMob>(wearer).is_none()
        || crate::status_effects::incapacitation(world, wearer).incapacitated
        || world
            .get::<crate::obj_damage::Anchored>(grabbed)
            .is_some_and(|a| a.0);
    if gone {
        clear_grab(world, module, true);
        return;
    }
    if !in_range(world, wearer, grabbed) {
        tell_mob(world, wearer, Span::Warning, "вне досягаемости!");
        clear_grab(world, module, true);
        return;
    }
    let cost = def_of(world, module).map_or(0.0, |d| d.use_energy_cost);
    drain_power(world, control, cost / 10.0 * dt / FASTPROCESS_SECS);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinesis_grabs_loose_things_and_corpses_only() {
        let mut world = World::new();
        let wearer = world.spawn(TilePos::new(1, 1, 1)).id();
        let crate_ = world
            .spawn((
                TilePos::new(2, 1, 1),
                DmObj {
                    path: "/obj/structure/closet/crate".into(),
                    ..Default::default()
                },
                crate::obj_damage::Anchored(false),
            ))
            .id();
        let bolted = world
            .spawn((
                TilePos::new(2, 1, 1),
                DmObj {
                    path: "/obj/machinery/vending".into(),
                    ..Default::default()
                },
                crate::obj_damage::Anchored(true),
            ))
            .id();
        let alive = world
            .spawn((TilePos::new(2, 1, 1), Health::default(), Stat::Conscious))
            .id();
        let corpse = world
            .spawn((TilePos::new(2, 1, 1), Health::default(), Stat::Dead))
            .id();
        assert!(can_grab(&world, wearer, crate_));
        assert!(!can_grab(&world, wearer, bolted));
        assert!(!can_grab(&world, wearer, alive));
        assert!(can_grab(&world, wearer, corpse));
        assert!(!can_grab(&world, wearer, wearer));
    }
}
