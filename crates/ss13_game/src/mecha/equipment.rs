//! Exosuit equipment (server), after tg `/obj/item/mecha_parts/mecha_equipment`
//! (`equipment/mecha_equipment.dm`), the hydraulic clamp (`tools/work_tools.dm`), the drill
//! (`tools/mining_tools.dm`) and the Ripley's cargo compartment (`working/ripley.dm`
//! `/obj/item/mecha_parts/mecha_equipment/ejector`).
//!
//! - Attaching (`try_attach_part`): a click on the mech with the equipment puts it on the left
//!   arm, a right click on the right arm (`mecha_weapon` equipment), other slots take it up to
//!   `max_equip_by_category`; `mech_flags` must include the mech's `mech_type`.
//! - `action_checks`: active, the energy for it, not ejecting, the equipment's cooldown (kept on
//!   the mech, per type). `action` starts the cooldown and uses `energy_drain`.
//! - Hydraulic clamp: an unanchored object is lifted into the cargo compartment after its
//!   `equip_cooldown` (a `do_after` the mech must keep facing and stay next to; energy for the
//!   lift and again for the action, as tg's `do_after_cooldown` + `..()`), up to the
//!   compartment's `cargo_capacity`; a mob is pushed aside, or squeezed for `clamp_damage` in
//!   combat mode; anchored things are "firmly secured".
//! - Drill: walls fall after `60 / drill_level` deciseconds (reinforced walls need a hardened
//!   drill), objects with integrity take 15 brute and mobs 10 brute per `drill_delay`, repeated
//!   while the target stays.
// shortcut: weapons live in `super::weapons`; no other ranged equipment (the syringe gun, the
// medical beam), no sleeper, no detaching through the Mecha window, no clamp forcing doors open,
// no mining (no mineral turfs here), boulders, butchering or the killer clamp's dismemberment.

use bevy::prelude::*;
use ss13_mapload::mecha::EquipDef;
use ss13_mapload::{DmItem, Turf};
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::{Mover, SpatialIndex};

use super::core::{
    Mech, MechClick, MechJob, MechaDb, dir_between, has_charge, hurt_mob, name_of, path_of,
    push_step, tell_occupant, use_energy, visible,
};
use crate::combat::CombatMode;
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::obj_damage::{Anchored, INDESTRUCTIBLE, is_under};
use crate::storage::{put_inside, take_out};
use crate::tools::{Actor, DoAfter, do_after, tell};

/// `MECHA_WEAPON`, `MECHA_UTILITY` (`__DEFINES/mecha.dm`).
const MECHA_WEAPON: &str = "mecha_weapon";
const CLAMP: &str = "/obj/item/mecha_parts/mecha_equipment/hydraulic_clamp";
const DRILL: &str = "/obj/item/mecha_parts/mecha_equipment/drill";
const WALL: &str = "/turf/closed/wall";
const R_WALL: &str = "/turf/closed/wall/r_wall";
/// `DRILL_HARDENED`.
const DRILL_HARDENED: u32 = 2;
/// `UNACIDABLE` (`resistance_flags`).
const UNACIDABLE: u32 = 1 << 5;
/// Brute per drill turn on objects and mobs (`take_damage(15)`, `apply_damage(10)`).
const DRILL_OBJ_DAMAGE: f32 = 15.0;
const DRILL_MOB_DAMAGE: f32 = 10.0;

/// An equipment item (tg `/obj/item/mecha_parts/mecha_equipment`): its type and `active`.
#[derive(Component, Clone, Copy, Debug)]
pub struct MechEquip {
    /// Index into [`ss13_mapload::mecha::MechaData::equipment`].
    pub def: usize,
    pub active: bool,
}

/// The timed equipment steps (`MechJob::Equipment`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipJob {
    /// The clamp loads the target into the cargo compartment.
    ClampLoad { equip: Entity },
    /// The drill went through the wall.
    DrillWall { equip: Entity },
    /// One drill turn on an object or a mob.
    DrillTurn { equip: Entity },
}

pub(super) fn def(world: &World, equip: Entity) -> Option<EquipDef> {
    let e = world.get::<MechEquip>(equip)?;
    Some(world.resource::<MechaDb>().data.equipment[e.def].clone())
}

fn is_type(world: &World, equip: Entity, root: &str) -> bool {
    def(world, equip).is_some_and(|d| is_under(&d.path, root))
}

/// An equipment item gets its state when it is attached or used.
fn ensure_state(world: &mut World, item: Entity) -> Option<MechEquip> {
    if let Some(e) = world.get::<MechEquip>(item) {
        return Some(*e);
    }
    let path = world.get::<DmItem>(item)?.path.clone();
    let def = world.resource::<MechaDb>().equipment(&path)?;
    let state = MechEquip { def, active: true };
    world.entity_mut(item).insert(state);
    Some(state)
}

/// tg `attach`: into the mech, on an arm or in its category.
pub fn attach(world: &mut World, mech: Entity, item: Entity, right: bool) {
    let Some(state) = ensure_state(world, item) else {
        return;
    };
    let slot = world.resource::<MechaDb>().data.equipment[state.def]
        .slot
        .clone();
    put_inside(world, item, mech);
    let Some(mut m) = world.get_mut::<Mech>(mech) else {
        return;
    };
    match (slot.as_str(), right) {
        (MECHA_WEAPON, true) => m.r_arm = Some(item),
        (MECHA_WEAPON, false) => m.l_arm = Some(item),
        _ => m.modules.push(item),
    }
}

/// tg `detach(moveto)`: out of the mech onto `pos`; the cargo compartment drops its contents.
pub fn detach(world: &mut World, mech: Entity, equip: Entity, pos: TilePos) {
    let cargo_hold = def(world, equip).is_some_and(|d| d.cargo_capacity > 0);
    let Some(mut m) = world.get_mut::<Mech>(mech) else {
        return;
    };
    if m.l_arm == Some(equip) {
        m.l_arm = None;
    }
    if m.r_arm == Some(equip) {
        m.r_arm = None;
    }
    m.modules.retain(|&e| e != equip);
    let cargo = if cargo_hold {
        std::mem::take(&mut m.cargo)
    } else {
        Vec::new()
    };
    // `drop_contents`: onto the tile, then `step_rand`.
    for thing in cargo {
        take_out(world, thing, pos);
        let dir = {
            let mut rng = world.resource_mut::<crate::combat::CombatRng>();
            Dir::DMI_ORDER[rng.range(0, 7) as usize]
        };
        push_step(world, thing, dir);
    }
    take_out(world, equip, pos);
}

/// tg `try_attach_part` -> `can_attach` -> `attach` of a held equipment item. True when the
/// click was the equipment's.
pub fn try_attach_part(
    world: &mut World,
    actor: Actor,
    item: Entity,
    mech: Entity,
    right: bool,
) -> bool {
    let Some(state) = ensure_state(world, item) else {
        return false;
    };
    let Some(m) = world.get::<Mech>(mech).cloned() else {
        return false;
    };
    let db = world.resource::<MechaDb>();
    let (d, mech_def) = (
        db.data.equipment[state.def].clone(),
        db.data.mechs[m.def].clone(),
    );
    let (thing, mech_name) = (name_of(world, item), name_of(world, mech));
    let max = |category: &str| {
        mech_def
            .max_equip
            .iter()
            .find(|(c, _)| c == category)
            .map_or(0, |(_, n)| *n)
    };
    let refusal = if d.mech_flags & mech_def.mech_type == 0 {
        Some(format!("The {thing} is incompatible with {mech_name}!"))
    } else if d.slot == MECHA_WEAPON {
        let (taken, category) = if right {
            (m.r_arm.is_some(), "mecha_r_arm")
        } else {
            (m.l_arm.is_some(), "mecha_l_arm")
        };
        (taken || max(category) == 0).then(|| {
            let side = if right { "right" } else { "left" };
            format!("The {mech_name}'s {side} arm is full!")
        })
    } else {
        let same_slot: Vec<Entity> = m
            .modules
            .iter()
            .copied()
            .filter(|&e| {
                world
                    .get::<MechEquip>(e)
                    .is_some_and(|s| db.data.equipment[s.def].slot == d.slot)
            })
            .collect();
        if d.unstackable
            && same_slot.iter().any(|&e| {
                world
                    .get::<MechEquip>(e)
                    .is_some_and(|s| s.def == state.def)
            })
        {
            Some("You can't stack more of this item ontop itself!".to_owned())
        } else {
            (same_slot.len() as u32 >= max(&d.slot))
                .then(|| "This equipment slot is already full!".to_owned())
        }
    };
    if let Some(text) = refusal {
        tell(world, actor.client, Span::Warning, text);
        return true;
    }
    // `temporarilyRemoveItemFromInventory`.
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(actor.mob) {
        for hand in &mut inventory.hands {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
    world
        .entity_mut(item)
        .remove::<crate::inventory::CarriedBy>();
    attach(world, mech, item, right);
    let user = name_of(world, actor.mob);
    if let Some(&pos) = world.get::<TilePos>(mech) {
        visible(
            world,
            pos,
            Span::Notice,
            format!("{user} attaches {thing} to {mech_name}."),
            &[actor.mob],
        );
    }
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("You attach {thing} to {mech_name}."),
    );
    true
}

/// tg `action_checks`.
pub(super) fn action_checks(world: &World, mech: Entity, equip: Entity) -> bool {
    let (Some(m), Some(state)) = (world.get::<Mech>(mech), world.get::<MechEquip>(equip)) else {
        return false;
    };
    let d = &world.resource::<MechaDb>().data.equipment[state.def];
    let now = world.resource::<Time>().elapsed_secs_f64();
    state.active
        && !(d.energy_drain > 0.0 && !has_charge(world, mech, d.energy_drain))
        && !m.is_currently_ejecting
        && m.equipment_ready(state.def, now)
}

/// tg `/obj/item/mecha_parts/mecha_equipment/action`'s own part: the cooldown and the energy.
pub(super) fn spend(world: &mut World, mech: Entity, equip: Entity, cooldown_ds: f32) {
    let Some(state) = world.get::<MechEquip>(equip).copied() else {
        return;
    };
    let drain = world.resource::<MechaDb>().data.equipment[state.def].energy_drain;
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut m) = world.get_mut::<Mech>(mech) {
        m.start_equipment_cooldown(state.def, now + f64::from(cooldown_ds) / 10.0);
    }
    use_energy(world, mech, drain);
}

/// The selected arm's equipment on an adjacent target.
pub fn action(world: &mut World, click: MechClick, mech: Entity, equip: Entity) {
    if is_type(world, equip, CLAMP) {
        clamp_action(world, click, mech, equip);
    }
}

/// The cargo compartment's `cargo_capacity` (0 without one).
fn cargo_capacity(world: &World, mech: Entity) -> u32 {
    let Some(m) = world.get::<Mech>(mech) else {
        return 0;
    };
    m.modules
        .iter()
        .filter_map(|&e| def(world, e))
        .map(|d| d.cargo_capacity)
        .max()
        .unwrap_or(0)
}

/// `isobj(target)`.
fn is_obj(world: &World, target: Entity) -> bool {
    world.get::<Turf>(target).is_none() && world.get::<Health>(target).is_none()
}

fn anchored(world: &World, target: Entity) -> bool {
    world.get::<Anchored>(target).is_some_and(|a| a.0)
        || world.get::<DmItem>(target).is_some_and(|i| i.anchored)
}

/// `/obj/item/mecha_parts/mecha_equipment/hydraulic_clamp/action`.
fn clamp_action(world: &mut World, click: MechClick, mech: Entity, equip: Entity) {
    if !action_checks(world, mech, equip) {
        return;
    }
    let Some(d) = def(world, equip) else {
        return;
    };
    if cargo_capacity(world, mech) == 0 {
        return;
    }
    let target = click.target;
    let thing = name_of(world, target);
    if world.get::<Mech>(target).is_some() {
        tell(
            world,
            click.client,
            Span::Info,
            "No providable supplies found in cargo hold",
        );
        return;
    }
    if path_of(world, target).is_some_and(|p| is_under(&p, "/obj/machinery/door")) {
        // shortcut: tg forces the door open (`try_to_crowbar`); the cooldown and energy apply.
        spend(world, mech, equip, d.equip_cooldown);
        return;
    }
    if is_obj(world, target) {
        if anchored(world, target) {
            tell(
                world,
                click.client,
                Span::Warning,
                format!("{thing} is firmly secured!"),
            );
            return;
        }
        let load = world.get::<Mech>(mech).map_or(0, |m| m.cargo.len()) as u32;
        if load >= cargo_capacity(world, mech) {
            tell(
                world,
                click.client,
                Span::Warning,
                "Not enough room in cargo compartment!",
            );
            return;
        }
        let (mech_name, pos) = (name_of(world, mech), world.get::<TilePos>(mech).copied());
        if let Some(pos) = pos {
            visible(
                world,
                pos,
                Span::Notice,
                format!("{mech_name} lifts {thing} and starts to load it into cargo compartment."),
                &[],
            );
        }
        // `do_after_cooldown`: the energy first, then the wait.
        use_energy(world, mech, d.energy_drain);
        let actor = Actor {
            client: click.client,
            mob: click.pilot,
            item: None,
        };
        do_after(
            world,
            actor,
            target,
            d.equip_cooldown,
            0,
            Job::Mecha(MechJob::Equipment(EquipJob::ClampLoad { equip })),
        );
        return;
    }
    if let Some(&Stat::Conscious | &Stat::SoftCrit | &Stat::HardCrit) = world.get::<Stat>(target) {
        let combat = world.get::<CombatMode>(click.pilot).is_some_and(|c| c.0);
        let mech_name = name_of(world, mech);
        let pos = world.get::<TilePos>(mech).copied();
        if !combat {
            if let (Some(pos), Some(dir)) =
                (pos, pos.and_then(|p| dir_between(p, click.target_pos)))
            {
                push_step(world, target, dir);
                visible(
                    world,
                    pos,
                    Span::Notice,
                    format!("{mech_name} pushes {thing} out of the way."),
                    &[click.pilot],
                );
            }
            tell(
                world,
                click.client,
                Span::Notice,
                format!("You push {thing} out of the way."),
            );
        } else {
            // shortcut: the victim's armor (a third of it) is not checked.
            hurt_mob(world, target, d.clamp_damage, DamageType::Brute);
            if let Some(pos) = pos {
                visible(
                    world,
                    pos,
                    Span::Danger,
                    format!("{mech_name} squeezes {thing}!"),
                    &[click.pilot],
                );
            }
        }
    }
    spend(world, mech, equip, d.equip_cooldown);
}

/// The attached drill, if any.
fn drill_of(world: &World, mech: Entity) -> Option<Entity> {
    world
        .get::<Mech>(mech)?
        .modules
        .iter()
        .copied()
        .find(|&e| is_type(world, e, DRILL))
}

/// tg `on_mech_click` of a drill: the melee click drills instead. True when it took the click.
pub fn drill_click(world: &mut World, click: MechClick, mech: Entity) -> bool {
    let Some(drill) = drill_of(world, mech) else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<Mech>(mech).is_some_and(|m| now < m.next_melee) {
        return false;
    }
    drill_action(world, click, mech, drill);
    true
}

/// tg `bump_mine`: only mineral turfs are drilled by walking into them.
// shortcut: there are no mineral turfs here; bumping drills nothing.
pub fn bump_mine(_world: &mut World, _mech: Entity, _obstacle: Entity) {}

/// `/obj/item/mecha_parts/mecha_equipment/drill/action` of a click.
fn drill_action(world: &mut World, click: MechClick, mech: Entity, drill: Entity) {
    let target = click.target;
    let path = path_of(world, target).unwrap_or_default();
    let turf = world.get::<Turf>(target).is_some();
    if (turf && is_under(&path, "/turf/open/space"))
        || !(turf || world.get::<Health>(target).is_some() || is_obj(world, target))
    {
        return;
    }
    if let Some(obj) = world.get::<ss13_mapload::DmObj>(target)
        && obj.resistance_flags & (UNACIDABLE | INDESTRUCTIBLE) != 0
    {
        return;
    }
    if !action_checks(world, mech, drill) || world.get::<DoAfter>(click.pilot).is_some() {
        return;
    }
    let Some(d) = def(world, drill) else {
        return;
    };
    let (mech_name, thing) = (name_of(world, mech), name_of(world, target));
    if let Some(&pos) = world.get::<TilePos>(mech) {
        visible(
            world,
            pos,
            Span::Warning,
            format!("{mech_name} starts to drill {thing}."),
            &[],
        );
    }
    let actor = Actor {
        client: click.client,
        mob: click.pilot,
        item: None,
    };
    if turf {
        // `get_equip_cooldown(turf)`: a tenth.
        spend(world, mech, drill, d.equip_cooldown * 0.1);
        if is_under(&path, R_WALL) {
            if d.drill_level < DRILL_HARDENED {
                tell(
                    world,
                    click.client,
                    Span::Danger,
                    format!("{thing} is too durable to drill through."),
                );
                return;
            }
            let delay = 120.0 / d.drill_level as f32;
            do_after(
                world,
                actor,
                target,
                delay,
                0,
                drill_job(EquipJob::DrillWall { equip: drill }),
            );
        } else if is_under(&path, WALL) {
            let delay = 60.0 / d.drill_level.max(1) as f32;
            do_after(
                world,
                actor,
                target,
                delay,
                0,
                drill_job(EquipJob::DrillWall { equip: drill }),
            );
        }
        return;
    }
    do_after(
        world,
        actor,
        target,
        d.drill_delay,
        0,
        drill_job(EquipJob::DrillTurn { equip: drill }),
    );
}

fn drill_job(job: EquipJob) -> Job {
    Job::Mecha(MechJob::Equipment(job))
}

/// tg `do_after_checks` of mech equipment: still attached to the pilot's mech, facing the
/// target, next to it.
fn still_aimed(world: &World, mob: Entity, equip: Entity, target_pos: TilePos) -> Option<Entity> {
    let mech = world.get::<super::core::Pilot>(mob)?.mech;
    if !world.get::<Mech>(mech)?.equipment().any(|e| e == equip) {
        return None;
    }
    let pos = *world.get::<TilePos>(mech)?;
    let facing = world.get::<Facing>(mech).map_or(Dir::South, |f| f.0);
    let aimed = dir_between(pos, target_pos).is_none_or(|d| d as u8 & facing as u8 != 0);
    let adjacent = world
        .resource::<SpatialIndex>()
        .adjacent(pos, target_pos, |e| {
            e == mech || world.get::<Mover>(e).is_some()
        });
    (aimed && adjacent).then_some(mech)
}

/// End of an [`EquipJob`] `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: EquipJob) {
    let target = action.target;
    match job {
        EquipJob::ClampLoad { equip } => {
            let Some(mech) = still_aimed(world, mob, equip, action.target_pos) else {
                return;
            };
            let full = world.get::<Mech>(mech).map_or(0, |m| m.cargo.len()) as u32
                >= cargo_capacity(world, mech);
            if full || anchored(world, target) || !is_obj(world, target) {
                return;
            }
            release_pulls(world, target);
            put_inside(world, target, mech);
            if let Some(mut m) = world.get_mut::<Mech>(mech) {
                m.cargo.push(target);
            }
            crate::construction::tile_changed(world, action.target_pos);
            let thing = name_of(world, target);
            tell_occupant(
                world,
                mech,
                Span::Notice,
                format!("{thing} successfully loaded."),
            );
            let cooldown = def(world, equip).map_or(0.0, |d| d.equip_cooldown);
            spend(world, mech, equip, cooldown);
        }
        EquipJob::DrillWall { equip } => {
            if still_aimed(world, mob, equip, action.target_pos).is_none() {
                return;
            }
            crate::construction::devastate_wall(world, target, action.target_pos);
        }
        EquipJob::DrillTurn { equip } => {
            let Some(mech) = still_aimed(world, mob, equip, action.target_pos) else {
                return;
            };
            if !action_checks(world, mech, equip) {
                return;
            }
            let (mech_name, thing) = (name_of(world, mech), name_of(world, target));
            if world.get::<Health>(target).is_some() {
                visible(
                    world,
                    action.target_pos,
                    Span::Danger,
                    format!("{mech_name} is drilling {thing} with the drill!"),
                    &[],
                );
                hurt_mob(world, target, DRILL_MOB_DAMAGE, DamageType::Brute);
            } else if world.get::<crate::obj_damage::Integrity>(target).is_some() {
                crate::obj_damage::take_damage(world, target, DRILL_OBJ_DAMAGE, DamageType::Brute);
            }
            let cooldown = def(world, equip).map_or(0.0, |d| d.equip_cooldown);
            // `while(do_after_mecha(...))`: again while the target stays; the cooldown and the
            // energy when it stops.
            let delay = def(world, equip).map_or(0.0, |d| d.drill_delay);
            let again = world.get_entity(target).is_ok()
                && world.get::<TilePos>(target) == Some(&action.target_pos)
                && do_after(
                    world,
                    Actor {
                        client: action.client,
                        mob,
                        item: None,
                    },
                    target,
                    delay,
                    0,
                    drill_job(EquipJob::DrillTurn { equip }),
                );
            if !again {
                spend(world, mech, equip, cooldown);
            }
        }
    }
}

/// A thing loaded into the cargo stops being pulled.
fn release_pulls(world: &mut World, thing: Entity) {
    if let Some(puller) = world
        .get::<crate::pull::PulledBy>(thing)
        .map(|p| p.puller())
    {
        world.entity_mut(puller).remove::<crate::pull::Pulling>();
    }
}

/// tg `item_interaction`/`item_interaction_secondary` of a mech with a held mecha part.
pub fn is_equipment(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| world.resource::<MechaDb>().equipment(&i.path).is_some())
}

/// Click modifiers -> the arm (`item_interaction_secondary` is the right arm).
pub fn right_arm(modifiers: ClickModifiers) -> bool {
    modifiers.right
}
