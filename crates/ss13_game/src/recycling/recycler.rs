//! The recycler (`/obj/machinery/recycler`, `/obj/machinery/recycler/deathtrap`): what enters its
//! tile is eaten: items become sheets (`amount_produced`% of their `custom_materials`, the rest
//! is kept for the next item), other objects are taken apart, a living mob trips the safety
//! (10 s, emagged or a deathtrap: it is crushed instead). Its sides are dense: things enter only
//! from where it faces (`CanAllowThrough`).
// shortcut: no energy use, no butchering (a dead mob with butcher results is treated like any
// mob), no sounds or blood overlay, a mob's own items are not eaten (only what lies in a locker
// or a bag), no log planks, no deconstruction by parts (`default_deconstruction_*`), no examine
// text.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::engmisc::RECYCLER_ROOT;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmMaterials, DmObj, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Facing, TilePos};
use ss13_world::{Density, EnterPolicy, SpatialIndex};

use super::RecyclingJob;
use crate::closets::Closet;
use crate::construction::{Job, path_of};
use crate::health::{DamageType, Health};
use crate::obj_damage::{Anchored, INDESTRUCTIBLE, anchored};
use crate::power::Unpowered;
use crate::status_effects::Incap;
use crate::storage::Stored;
use crate::tools::{Actor, Stack, ToolKind, do_after, tell};

/// `SAFETY_COOLDOWN`, seconds.
const SAFETY_SECS: f64 = 10.0;
/// `Unconscious(100)`, seconds.
const CRUSH_UNCONSCIOUS_SECS: f32 = 10.0;
const CRUSH_DAMAGE: f32 = 1000.0;
const DEATHTRAP_CRUSH_DAMAGE: f32 = 120.0;
const DEATHTRAP: &str = "/obj/machinery/recycler/deathtrap";
/// `/datum/stock_part/servo` tier 1 gives `12.5 * 1 + 50`.
const AMOUNT_PRODUCED: f32 = 62.5;
/// `SHEET_MATERIAL_AMOUNT`.
const SHEET: f32 = 100.0;
/// `default_unfasten_wrench`: `2 SECONDS`.
const UNFASTEN_DS: f32 = 20.0;
/// How often its tile is looked at, seconds.
const SCAN_SECS: f64 = 0.2;

/// `/obj/machinery/recycler`.
#[derive(Component, Clone, Debug, Default)]
pub struct Recycler {
    pub emagged: bool,
    pub safety_mode: bool,
    /// When the safety resets (`reboot`).
    safety_until: Option<f64>,
    pub crush_damage: f32,
    /// What did not fill a sheet yet, by material.
    leftovers: HashMap<String, f32>,
    /// What was on the tile at the last look (what entered since is new).
    seen: Vec<Entity>,
    last_look: f64,
}

/// Recyclers get their state as they appear (map load or a later spawn); their sides block
/// through [`EnterPolicy`] instead of `Density`.
pub fn init_new(mut commands: Commands, added: Query<(Entity, &DmObj), Added<DmObj>>) {
    for (e, obj) in &added {
        if is_under(&obj.path, RECYCLER_ROOT) {
            let deathtrap = is_under(&obj.path, DEATHTRAP);
            commands.entity(e).remove::<Density>().insert((
                Recycler {
                    emagged: deathtrap,
                    crush_damage: if deathtrap {
                        DEATHTRAP_CRUSH_DAMAGE
                    } else {
                        CRUSH_DAMAGE
                    },
                    ..default()
                },
                EnterPolicy(allows),
            ));
        }
    }
}

/// `CanAllowThrough`: a border-free dense machine that lets in what comes from where it faces.
fn allows(world: &World, mover: Entity, machine: Entity) -> bool {
    if !anchored(world, machine) {
        return false;
    }
    let (Some(&at), Some(&from)) = (world.get::<TilePos>(machine), world.get::<TilePos>(mover))
    else {
        return false;
    };
    let facing = world
        .get::<Facing>(machine)
        .map(|f| f.0)
        .unwrap_or_default();
    crate::pull::dir_between(at, from) == Some(facing)
}

/// Whether `thing` may be eaten at all (`eat`'s early exits, `triggers_safety_shutdown`).
fn eatable(world: &World, thing: Entity) -> bool {
    let Ok(e) = world.get_entity(thing) else {
        return false;
    };
    if e.contains::<Turf>() || e.contains::<Recycler>() {
        return false;
    }
    if e.contains::<Health>() {
        return true;
    }
    let Some(path) = path_of(world, thing) else {
        return false;
    };
    if is_under(&path, "/obj/effect") {
        return false;
    }
    !e.get::<DmObj>()
        .is_some_and(|o| o.resistance_flags & INDESTRUCTIBLE != 0)
}

/// `eat`: what it finds in `morsel` (a closet's and a bag's contents too).
fn collect(world: &World, morsel: Entity, out: &mut Vec<Entity>) {
    if world.get::<Health>(morsel).is_some() {
        out.push(morsel);
        return;
    }
    // Locked secure closets stay shut (`closet_thing.secure && closet_thing.locked`).
    if world
        .get::<Closet>(morsel)
        .is_some_and(|c| c.secure && c.locked)
    {
        return;
    }
    out.push(morsel);
    if let Some(stored) = world.get::<Stored>(morsel) {
        for &item in stored.items() {
            collect(world, item, out);
        }
    }
}

/// `update_icon_state`.
fn icon_state(world: &World, machine: Entity, r: &Recycler) -> String {
    if crate::construction::panel_open(world, machine) {
        return "grinder-oOpen".to_owned();
    }
    let running = world.get::<Unpowered>(machine).is_none() && !r.safety_mode;
    format!("grinder-o{}", u8::from(running))
}

fn refresh(world: &mut World, machine: Entity) {
    let Some(state) = world
        .get::<Recycler>(machine)
        .map(|r| icon_state(world, machine, r))
    else {
        return;
    };
    let id = world.get::<DmAtom>(machine).and_then(|a| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&a.icon, &state))
    });
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<Appearance>(machine)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

/// `emergency_stop`.
fn emergency_stop(world: &mut World, machine: Entity, now: f64) {
    if let Some(mut r) = world.get_mut::<Recycler>(machine) {
        r.safety_mode = true;
        r.safety_until = Some(now + SAFETY_SECS);
    }
    refresh(world, machine);
}

/// Every look: safety resets, power, what entered since the last look.
pub fn tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query_filtered::<(Entity, &TilePos), With<Recycler>>();
    let list: Vec<(Entity, TilePos)> = query.iter(world).map(|(e, &p)| (e, p)).collect();
    for (machine, pos) in list {
        let due = {
            let mut r = world.get_mut::<Recycler>(machine).expect("queried");
            if now - r.last_look < SCAN_SECS {
                false
            } else {
                r.last_look = now;
                true
            }
        };
        if !due {
            continue;
        }
        let reboot = world
            .get::<Recycler>(machine)
            .and_then(|r| r.safety_until)
            .is_some_and(|t| now >= t);
        if reboot {
            let mut r = world.get_mut::<Recycler>(machine).expect("queried");
            r.safety_mode = false;
            r.safety_until = None;
            drop(r);
            refresh(world, machine);
        }
        let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
        let mut r = world.get_mut::<Recycler>(machine).expect("queried");
        let new: Vec<Entity> = on_tile
            .iter()
            .copied()
            .filter(|e| !r.seen.contains(e))
            .collect();
        r.seen = on_tile;
        let safety = r.safety_mode;
        drop(r);
        if safety || world.get::<Unpowered>(machine).is_some() {
            continue;
        }
        for thing in new {
            if !eat(world, machine, pos, thing, now) {
                break;
            }
        }
        refresh(world, machine);
    }
}

/// `eat`: false when the machine stopped.
fn eat(world: &mut World, machine: Entity, pos: TilePos, morsel: Entity, now: f64) -> bool {
    if !eatable(world, morsel) {
        return true;
    }
    let mut nom = Vec::new();
    collect(world, morsel, &mut nom);
    let Some(r) = world.get::<Recycler>(machine) else {
        return false;
    };
    let (emagged, crush) = (r.emagged, r.crush_damage);
    let living: Vec<Entity> = nom
        .iter()
        .copied()
        .filter(|&e| world.get::<Health>(e).is_some())
        .collect();
    if !emagged && !living.is_empty() {
        emergency_stop(world, machine, now);
        return false;
    }
    for mob in living {
        crush_living(world, mob, crush);
    }
    // Innermost first (`for i = nom_length; i >= 1; i--`), so a container is empty when it goes.
    for &e in nom.iter().rev() {
        if world.get::<Health>(e).is_some() {
            continue;
        }
        let item = world.get::<DmItem>(e).is_some();
        if item {
            recycle_item(world, machine, pos, e);
        } else {
            crate::obj_damage::deconstruct(world, e, false);
        }
    }
    true
}

/// `recycle_item`: `materials.insert_item(target, multiplier = amount_produced / 100)` and
/// `retrieve_all` (whole sheets out, the rest stays).
fn recycle_item(world: &mut World, machine: Entity, pos: TilePos, item: Entity) {
    let mult = AMOUNT_PRODUCED / 100.0;
    let amount = world.get::<Stack>(item).map_or(1, |s| s.amount) as f32;
    let materials = world.get::<DmMaterials>(item).cloned();
    world.despawn(item);
    let Some(materials) = materials else {
        return;
    };
    let mut sheets = Vec::new();
    {
        let mut r = world.get_mut::<Recycler>(machine).expect("alive");
        for (material, &per) in materials.materials.iter().zip(&materials.amounts) {
            let stored = r.leftovers.entry(material.clone()).or_default();
            *stored += per * amount * mult;
            let whole = (*stored / SHEET).floor();
            if whole >= 1.0 {
                *stored -= whole * SHEET;
                sheets.push((material.clone(), whole as u32));
            }
        }
    }
    for (material, count) in sheets {
        if let Some(path) = sheet_of(world, &material) {
            crate::construction::spawn_manufactured(world, &path, pos, count);
        }
    }
}

/// `/datum/material::sheet_type`.
fn sheet_of(world: &World, material: &str) -> Option<String> {
    world
        .get_resource::<crate::science::techweb::ResearchDb>()?
        .materials
        .iter()
        .find(|m| m.id == material)?
        .sheet_type
        .clone()
}

/// `crush_living`: instantly floored and unconscious, then the damage.
fn crush_living(world: &mut World, mob: Entity, damage: f32) {
    crate::status_effects::adjust(world, mob, Incap::Unconscious, CRUSH_UNCONSCIOUS_SECS);
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.adjust(DamageType::Brute, damage);
    }
}

/// `emag_act`.
pub fn emag(world: &mut World, client: Entity, target: Entity) -> bool {
    let Some(mut r) = world.get_mut::<Recycler>(target) else {
        return false;
    };
    if r.emagged {
        return false;
    }
    r.emagged = true;
    r.safety_mode = false;
    r.safety_until = None;
    refresh(world, target);
    tell(world, client, Span::Warning, "Защита отключена.");
    true
}

pub(super) fn tool_act(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    if world.get::<Recycler>(target).is_none() || kind != ToolKind::Wrench {
        return false;
    }
    let anchored = anchored(world, target);
    do_after(
        world,
        actor,
        target,
        UNFASTEN_DS,
        0,
        Job::Recycling(RecyclingJob::RecyclerUnfasten { anchored }),
    );
    true
}

/// `default_unfasten_wrench` after its `do_after`.
pub(super) fn complete_unfasten(world: &mut World, target: Entity, was: bool) {
    if world.get::<Recycler>(target).is_some() && anchored(world, target) == was {
        world.entity_mut(target).insert(Anchored(!was));
    }
}

/// `/datum/element/simple_rotation`: a loose recycler turns.
pub fn rotate(world: &mut World, target: Entity) -> bool {
    if world.get::<Recycler>(target).is_none() {
        return false;
    }
    if !anchored(world, target) {
        let current = world.get::<Facing>(target).map(|f| f.0).unwrap_or_default();
        let next = crate::disposals::pipes::turn(current as u8, -90);
        if let Some(dir) = ss13_shared::Dir::from_bits(next) {
            world.entity_mut(target).insert(Facing(dir));
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::Dir;

    #[test]
    fn entered_only_from_where_it_faces() {
        let mut app = App::new();
        app.add_plugins(ss13_world::SpatialIndexPlugin);
        let world = app.world_mut();
        let m = world
            .spawn((TilePos::new(5, 5, 1), Facing(Dir::East), Anchored(true)))
            .id();
        let east = world.spawn(TilePos::new(6, 5, 1)).id();
        let west = world.spawn(TilePos::new(4, 5, 1)).id();
        assert!(allows(world, east, m));
        assert!(!allows(world, west, m));
        world.entity_mut(m).insert(Anchored(false));
        assert!(!allows(world, east, m), "a loose one is just dense");
    }
}
