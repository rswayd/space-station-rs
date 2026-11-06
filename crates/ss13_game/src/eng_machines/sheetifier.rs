//! The Sheet-meister 2000 (server): tg `/obj/machinery/sheetifier`
//! (`code/game/machinery/sheetifier.dm`), built from its board (`crate::machine_frame`).
//!
//! - Its `/datum/material_container` takes meat (`/obj/item/food/meat` with
//!   `/datum/material/meat` in `custom_materials`, [`ss13_mapload::DmFood::meat_sheets`]) on a
//!   right click out of combat mode (`on_secondary_insert`: not a stack), powered and with the
//!   panel shut; up to `SHEET_MATERIAL_AMOUNT * MAX_STACK_SIZE * 2` (100 sheets).
//! - Each inserted item starts 6.4 s of processing (`AfterInsertMaterials`), during which
//!   inserting is blocked; then every whole sheet held comes out as meat sheets on its tile
//!   (`retrieve_all`, the fraction stays) and it uses its `active_power_usage` once.
//! - A wrench (2 s) fastens it, a screwdriver and crowbar take it apart (`crate::machine_frame`).
// shortcut: no hauntium (only photos of ghosts carry it, and photos are not ported), no
// per-mob meat material (sheets are plain `/obj/item/stack/sheet/meat`), no coloured
// `processing` flick; add them with photos and mob meat materials.

use bevy::prelude::*;
use ss13_mapload::{DmFood, DmItem, DmObj, DmPower};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::construction::PanelOpen;
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::{Anchored, is_under, set_icon_state};
use crate::power::Unpowered;
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

pub const SHEETIFIER: &str = "/obj/machinery/sheetifier";
const MEAT: &str = "/obj/item/food/meat";
pub const MEAT_SHEET: &str = "/obj/item/stack/sheet/meat";
/// `SHEET_MATERIAL_AMOUNT * MAX_STACK_SIZE * 2`, in sheets.
pub const CAPACITY_SHEETS: f32 = 100.0;
/// `addtimer(..., 6.4 SECONDS)`.
pub const PROCESS_SECS: f64 = 6.4;
/// `default_unfasten_wrench`, deciseconds.
const WRENCH_DS: f32 = 20.0;
const RATCHET: &str = "sound/items/tools/ratchet.ogg";

/// A sheetifier's live state.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Sheetifier {
    /// Meat held, in sheets.
    pub meat: f32,
    /// `busy_processing`: when it finishes (game seconds).
    pub busy_until: Option<f64>,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The material container's `on_secondary_insert` and `wrench_act` (before
    // `crate::machine_frame` takes it apart).
    app.add_click_handler(ClickPhase::HeldOnTarget, 1599, |w, c| {
        c.held
            .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target, c.modifiers.right))
    });
    app.add_systems(
        ss13_world::MapInit,
        setup_mapped.after(crate::power::init_power),
    )
    .add_systems(
        Update,
        finish_processing
            .after(crate::power::PowerSystems)
            .run_if(any_with_component::<Sheetifier>),
    );
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, SHEETIFIER))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// `Initialize`; also for one built from a frame (`crate::machine_frame::setup_built`).
pub fn setup(world: &mut World, e: Entity) {
    world.entity_mut(e).insert(Sheetifier::default());
    update_look(world, e);
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `update_icon_state`.
fn update_look(world: &mut World, e: Entity) {
    let busy = world
        .get::<Sheetifier>(e)
        .is_some_and(|s| s.busy_until.is_some());
    let state = if busy {
        "base_machine_processing"
    } else {
        "base_machine"
    };
    set_icon_state(world, e, state);
}

/// The meat an item carries, in sheets (`allowed_item_typecache` and `allowed_materials`).
pub fn meat_of(world: &World, item: Entity) -> Option<f32> {
    let path = &world.get::<DmItem>(item)?.path;
    is_under(path, MEAT).then(|| world.get::<DmFood>(item).map_or(0.0, |f| f.meat_sheets))
}

fn name_of(world: &World, e: Entity) -> String {
    crate::obj_damage::strip(&crate::medical::name_of(world, e)).to_owned()
}

/// The held item on a sheetifier: meat in (right click), a wrench. True when it was ours.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
    right: bool,
) -> bool {
    let Some(state) = world.get::<Sheetifier>(e).copied() else {
        return false;
    };
    // `if(!(mat_container_flags & MATCONTAINER_ANY_INTENT) && user.combat_mode) return`.
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    if let Some((ToolKind::Wrench, _)) = tool_of(world, held) {
        let anchored = world.get::<Anchored>(e).is_some_and(|a| a.0);
        if let Some(&pos) = world.get::<TilePos>(e) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, RATCHET, 50.0, true),
            );
        }
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        use_tool(
            world,
            actor,
            e,
            WRENCH_DS,
            0,
            crate::construction::Job::Unfasten { anchored },
        );
        return true;
    }
    // Stacks with either click (`on_item_insert`), the rest with the right one
    // (`on_secondary_insert`).
    if !right && world.get::<crate::tools::Stack>(held).is_none() {
        return false;
    }
    // `if(machine.machine_stat || machine.panel_open) return`.
    if world.get::<Unpowered>(e).is_some() || world.get::<PanelOpen>(e).is_some_and(|p| p.0) {
        return false;
    }
    insert(world, client, mob, held, e, state);
    true
}

/// `user_insert` of one held item.
fn insert(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
    state: Sheetifier,
) {
    // `CanInsertMaterials`: `MATCONTAINER_BLOCK_INSERT` while busy (silently).
    if state.busy_until.is_some() {
        return;
    }
    let (item, machine) = (name_of(world, held), name_of(world, e));
    let Some(sheets) = meat_of(world, held) else {
        tell(
            world,
            client,
            Span::Warning,
            format!("The {item} was rejected by {machine}!"),
        );
        return;
    };
    if sheets <= 0.0 {
        tell(
            world,
            client,
            Span::Warning,
            format!("The {item} has no materials that can be accepted by {machine}!"),
        );
        return;
    }
    if state.meat + sheets > CAPACITY_SHEETS {
        tell(
            world,
            client,
            Span::Warning,
            format!("{machine} does not have enough space for {item}!"),
        );
        return;
    }
    if crate::inventory::no_drop(world, held) {
        return;
    }
    world.entity_mut(held).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
    {
        *slot = None;
    }
    world.despawn(held);
    let until = now(world) + PROCESS_SECS;
    if let Some(mut s) = world.get_mut::<Sheetifier>(e) {
        s.meat += sheets;
        s.busy_until = Some(until);
    }
    update_look(world, e);
    let shown = (sheets * 100.0).round() / 100.0;
    tell(
        world,
        client,
        Span::Notice,
        format!("The {item}, worth {shown} sheets, was added to {machine}."),
    );
}

/// `finish_processing`: whole sheets out, the power used.
fn finish_processing(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &Sheetifier)>()
        .iter(world)
        .filter(|(_, s)| s.busy_until.is_some_and(|at| at <= t))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        let Some(mut s) = world.get_mut::<Sheetifier>(e) else {
            continue;
        };
        s.busy_until = None;
        // Float sums of thirds: a hair under a whole sheet counts as one.
        let whole = (s.meat + 1e-3).floor().max(0.0);
        s.meat = (s.meat - whole).max(0.0);
        update_look(world, e);
        if let Some(&pos) = world.get::<TilePos>(e) {
            crate::construction::spawn_stack(world, MEAT_SHEET, pos, whole as u32);
        }
        let watts = world
            .get::<DmPower>(e)
            .map_or(0.0, |p| p.active_power_usage);
        crate::power::machine_use_energy(world, e, watts, false);
    }
}
