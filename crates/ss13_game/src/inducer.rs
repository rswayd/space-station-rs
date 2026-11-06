//! Inducer (server): tg `/obj/item/inducer` (`code/game/objects/items/tools/inducer.dm`).
//!
//! Its battery (`powerdevice`, a cell item inside it) charges another atom's cell: out of
//! combat mode, with the hatch shut, clicked on an APC, a loose cell, a cyborg, an exosuit, a
//! space heater or a MULE, it moves charge once a second (`do_after(1 SECONDS)` in a loop) while
//! the target still misses some, at most the target cell's `rating_base * rating *
//! power_transfer_multiplier` per step, with sparks. A screwdriver opens the hatch; open, a cell
//! goes in, the cell comes out by `attack_self` and a plasma sheet adds 1.5 standard cells.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmStack};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::construction::Job;
use crate::inventory::{CarriedBy, Inventory};
use crate::mecha::core::{ItemCell, Mech};
use crate::obj_damage::is_under;
use crate::power::{Apc, Cell};
use crate::storage::{StoredIn, put_inside};
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of};

const INDUCER: &str = "/obj/item/inducer";
const BATTERY: &str = "/obj/item/stock_parts/power_store/battery";
const POWER_STORE: &str = "/obj/item/stock_parts/power_store";
const PLASMA_SHEET: &str = "/obj/item/stack/sheet/mineral/plasma";
/// `STANDARD_CELL_CHARGE`, joules.
pub const STANDARD_CELL_CHARGE: f64 = 10_000.0;
/// `STANDARD_BATTERY_CHARGE`, joules.
pub const STANDARD_BATTERY_CHARGE: f64 = 1_000_000.0;
/// `do_after(user, 1 SECONDS)`.
const STEP_DS: f32 = 10.0;

/// The inducer's live vars.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Inducer {
    pub opened: bool,
    /// `powerdevice`: the cell item inside.
    pub cell: Option<Entity>,
    /// `power_transfer_multiplier`.
    pub multiplier: f64,
    /// The cell type `Initialize` creates, until it has.
    pending: Option<&'static str>,
}

impl Inducer {
    /// The type's vars: the cell it starts with, the hatch, the speed.
    pub fn of(path: &str) -> Self {
        let (cell, opened, multiplier) = if is_under(path, "/obj/item/inducer/empty")
            || is_under(path, "/obj/item/inducer/sci")
        {
            (None, true, 1.0)
        } else if is_under(path, "/obj/item/inducer/orderable") {
            (
                Some("/obj/item/stock_parts/power_store/battery/upgraded"),
                false,
                1.0,
            )
        } else if is_under(path, "/obj/item/inducer/syndicate") {
            (
                Some("/obj/item/stock_parts/power_store/battery/super"),
                false,
                2.0,
            )
        } else {
            (
                Some("/obj/item/stock_parts/power_store/battery/high"),
                false,
                1.0,
            )
        };
        Self {
            opened,
            cell: None,
            multiplier,
            pending: cell,
        }
    }
}

pub struct InducerPlugin;

impl Plugin for InducerPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `attack_self`: the cell comes out of an open hatch.
        app.add_click_handler(ClickPhase::AttackSelf, 220, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        // The inducer in the other hand: screwdriver, a cell, plasma.
        app.add_click_handler(ClickPhase::OtherHand, 455, |w, c| {
            c.held
                .is_some_and(|held| item_interaction(w, c.client, c.mob, held, c.target))
        });
        // `interact_with_atom`: charging.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1555, |w, c| {
            c.held
                .is_some_and(|held| interact_with_atom(w, c.client, c.mob, held, c.target))
        });
        app.add_observer(add_inducer)
            .add_systems(Update, fill_inducers);
    }
}

pub fn is_inducer(path: &str) -> bool {
    is_under(path, INDUCER)
}

fn add_inducer(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Ok(item) = items.get(add.entity)
        && is_inducer(&item.path)
    {
        commands
            .entity(add.entity)
            .try_insert_if_new(Inducer::of(&item.path));
    }
}

/// `Initialize`: `powerdevice = new powerdevice(src)`. Cost: one pass over the inducers (a
/// handful) per tick.
fn fill_inducers(world: &mut World) {
    let mut new = world.query::<(Entity, &Inducer)>();
    let pending: Vec<(Entity, &'static str)> = new
        .iter(world)
        .filter_map(|(e, i)| Some((e, i.pending?)))
        .collect();
    for (inducer, path) in pending {
        let cell = crate::construction::spawn_detached(world, path);
        if let Some(cell) = cell {
            put_inside(world, cell, inducer);
        }
        if let Some(mut i) = world.get_mut::<Inducer>(inducer) {
            i.pending = None;
            if i.cell.is_none() {
                i.cell = cell;
            }
        }
    }
}

/// `get_cell()` of the inducer: its cell while it is still inside.
pub fn own_cell(world: &World, inducer: Entity) -> Option<Entity> {
    let cell = world.get::<Inducer>(inducer)?.cell?;
    world
        .get::<StoredIn>(cell)
        .is_some_and(|s| s.0 == inducer)
        .then_some(cell)
}

/// What holds the cell an atom's `get_cell()` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellOf {
    /// The APC's cell, kept on the APC (always a battery).
    Apc(Entity),
    /// A cell item.
    Item(Entity),
}

/// `interacting_with.get_cell(src, user)` for the atoms this server keeps a cell on.
fn target_cell(world: &World, target: Entity) -> Option<CellOf> {
    if let Some(apc) = world.get::<Apc>(target) {
        return apc.cell.is_some().then_some(CellOf::Apc(target));
    }
    if world.get::<ItemCell>(target).is_some() {
        return Some(CellOf::Item(target));
    }
    let item = world
        .get::<crate::silicon::chassis::Chassis>(target)
        .and_then(|c| c.cell)
        .or_else(|| world.get::<Mech>(target).and_then(|m| m.cell))
        .or_else(|| crate::atmos_machinery::space_heater::cell_of(world, target))
        .or_else(|| crate::bots::mule::cell_of(world, target))?;
    world
        .get::<ItemCell>(item)
        .is_some()
        .then_some(CellOf::Item(item))
}

fn read_cell(world: &World, cell: CellOf) -> Option<Cell> {
    match cell {
        CellOf::Apc(apc) => world.get::<Apc>(apc)?.cell,
        CellOf::Item(item) => world.get::<ItemCell>(item).map(|c| c.0),
    }
}

/// `give()` on the target cell; what it took.
fn give(world: &mut World, cell: CellOf, amount: f64) -> f64 {
    match cell {
        CellOf::Apc(apc) => world
            .get_mut::<Apc>(apc)
            .and_then(|mut a| a.cell.as_mut().map(|c| c.give(amount)))
            .unwrap_or(0.0),
        CellOf::Item(item) => world
            .get_mut::<ItemCell>(item)
            .map_or(0.0, |mut c| c.0.give(amount)),
    }
}

/// `rating_base * rating` of a cell: `rating = max(round(maxcharge / (rating_base * 10)), 1)`.
pub fn transfer_cap(cell: &Cell, battery: bool) -> f64 {
    let base = if battery {
        STANDARD_BATTERY_CHARGE
    } else {
        STANDARD_CELL_CHARGE
    };
    base * (cell.maxcharge / (base * 10.0)).round().max(1.0)
}

fn is_battery(world: &World, cell: CellOf) -> bool {
    match cell {
        CellOf::Apc(_) => true,
        CellOf::Item(item) => world
            .get::<DmItem>(item)
            .is_some_and(|i| is_under(&i.path, BATTERY)),
    }
}

fn combat_mode(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
}

fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(inducer) = world.get::<Inducer>(item) else {
        return false;
    };
    if !inducer.opened {
        return true;
    }
    let Some(cell) = own_cell(world, item) else {
        return true;
    };
    if let Some(mut i) = world.get_mut::<Inducer>(item) {
        i.cell = None;
    }
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, cell);
    }
    tell(world, client, Span::Notice, "Вы извлекаете батарею.");
    true
}

/// `screwdriver_act` and `item_interaction` of the inducer (`target`) with `held`.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(inducer) = world.get::<Inducer>(target).cloned() else {
        return false;
    };
    if tool_of(world, held).is_some_and(|(kind, _)| kind == ToolKind::Screwdriver) {
        let opened = !inducer.opened;
        if let Some(mut i) = world.get_mut::<Inducer>(target) {
            i.opened = opened;
        }
        let text = if opened {
            "Вы открываете батарейный отсек."
        } else {
            "Вы закрываете батарейный отсек."
        };
        tell(world, client, Span::Notice, text);
        return true;
    }
    if combat_mode(world, mob) {
        return false;
    }
    let path = world.get::<DmItem>(held).map(|i| i.path.clone());
    let path = path.as_deref().unwrap_or_default();
    if is_under(path, POWER_STORE) && world.get::<ItemCell>(held).is_some() {
        let text = if !inducer.opened {
            "Сначала откройте отсек!"
        } else if own_cell(world, target).is_some() {
            "Батарея уже установлена!"
        } else {
            put_inside(world, held, target);
            if let Some(mut i) = world.get_mut::<Inducer>(target) {
                i.cell = Some(held);
            }
            return true;
        };
        tell(world, client, Span::Warning, text);
        return true;
    }
    if is_under(path, PLASMA_SHEET)
        && world.get::<DmStack>(held).is_some()
        && let Some(cell) = own_cell(world, target)
    {
        let full = world
            .get::<ItemCell>(cell)
            .is_none_or(|c| c.0.used_charge() <= 0.0);
        if full {
            tell(world, client, Span::Warning, "Батарея полностью заряжена!");
            return true;
        }
        crate::tools::use_stack(world, held, 1);
        if let Some(mut c) = world.get_mut::<ItemCell>(cell) {
            c.0.give(1.5 * STANDARD_CELL_CHARGE);
        }
        tell(world, client, Span::Notice, "Батарея подзаряжена.");
        return true;
    }
    false
}

/// Whether `thing` is still within reach of `mob` (on a neighbouring tile or carried by it).
fn in_reach(world: &World, mob: Entity, thing: Entity) -> bool {
    if world.get::<CarriedBy>(thing).is_some_and(|c| c.0 == mob) {
        return true;
    }
    match (world.get::<TilePos>(mob), world.get::<TilePos>(thing)) {
        (Some(a), Some(b)) => a.z == b.z && (a.x - b.x).abs() <= 1 && (a.y - b.y).abs() <= 1,
        _ => false,
    }
}

/// The checks of `interact_with_atom` before each step; the message of the first that fails.
fn refusal(world: &World, inducer: Entity, thing: Entity) -> Result<CellOf, Option<&'static str>> {
    let opened = world.get::<Inducer>(inducer).is_none_or(|i| i.opened);
    if opened {
        return Err(Some("Сначала закройте отсек!"));
    }
    let own = own_cell(world, inducer)
        .and_then(|c| world.get::<ItemCell>(c))
        .map(|c| c.0);
    let Some(own) = own else {
        return Err(Some("Батарея не установлена!"));
    };
    if own.charge <= 0.0 {
        return Err(Some("Нет заряда!"));
    }
    let cell = target_cell(world, thing).ok_or(None)?;
    let Some(target) = read_cell(world, cell) else {
        return Err(None);
    };
    if target.used_charge() <= 0.0 {
        return Err(Some("Полностью заряжено!"));
    }
    Ok(cell)
}

fn interact_with_atom(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Inducer>(held).is_none() || combat_mode(world, mob) {
        return false;
    }
    // `recharging`: one charging loop per user.
    if world.get::<DoAfter>(mob).is_some() {
        return true;
    }
    match refusal(world, held, target) {
        Ok(_) => {}
        // No cell to charge: an ordinary click.
        Err(None) => return false,
        Err(Some(text)) => {
            tell(world, client, Span::Warning, text);
            return true;
        }
    }
    tell(world, client, Span::Notice, "Вы начинаете зарядку.");
    start_step(world, client, mob, held, target);
    true
}

fn start_step(world: &mut World, client: Entity, mob: Entity, inducer: Entity, thing: Entity) {
    let actor = Actor {
        client,
        mob,
        item: Some(inducer),
    };
    // `target = user`: the step needs the user and the inducer still; reach is checked at the
    // step's end.
    crate::tools::do_after(world, actor, mob, STEP_DS, 0, Job::Inducer(thing));
}

/// One `do_after` step done: the transfer, then the next step while there is charge to move.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, thing: Entity) {
    let Some(inducer) = action.item else {
        return;
    };
    if world.get_entity(thing).is_err() || !in_reach(world, mob, thing) {
        return;
    }
    let Ok(cell) = refusal(world, inducer, thing) else {
        return;
    };
    let (Some(target), Some(own_entity)) = (read_cell(world, cell), own_cell(world, inducer))
    else {
        return;
    };
    let own = world
        .get::<ItemCell>(own_entity)
        .map_or(0.0, |c| c.0.charge);
    let multiplier = world.get::<Inducer>(inducer).map_or(1.0, |i| i.multiplier);
    let cap = transfer_cap(&target, is_battery(world, cell)) * multiplier;
    let transferred = own.min(target.used_charge()).min(cap);
    if transferred <= 0.0 {
        return;
    }
    let given = give(world, cell, transferred);
    if let Some(mut c) = world.get_mut::<ItemCell>(own_entity) {
        c.0.take(given, true);
    }
    if let Some(&pos) = world.get::<TilePos>(thing).or(world.get::<TilePos>(mob)) {
        crate::effects::particles::do_sparks(world, 1, false, pos);
    }
    if refusal(world, inducer, thing).is_ok() {
        start_step(world, action.client, mob, inducer, thing);
    } else {
        tell(world, action.client, Span::Notice, "Зарядка завершена.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_is_the_target_rating() {
        // A 10 MJ upgraded battery: rating 1 of 1 MJ.
        let battery = Cell {
            charge: 0.0,
            maxcharge: 10_000_000.0,
            chargerate: 0.0,
        };
        assert_eq!(transfer_cap(&battery, true), 1_000_000.0);
        // A 50 MJ battery: rating 5.
        let big = Cell {
            maxcharge: 50_000_000.0,
            ..battery
        };
        assert_eq!(transfer_cap(&big, true), 5_000_000.0);
        // A small cell below one rating still moves its base.
        let small = Cell {
            maxcharge: 10_000.0,
            ..battery
        };
        assert_eq!(transfer_cap(&small, false), STANDARD_CELL_CHARGE);
    }

    #[test]
    fn type_vars() {
        let base = Inducer::of("/obj/item/inducer");
        assert!(!base.opened);
        assert_eq!(
            base.pending,
            Some("/obj/item/stock_parts/power_store/battery/high")
        );
        let sci = Inducer::of("/obj/item/inducer/sci");
        assert!(sci.opened && sci.pending.is_none());
        assert_eq!(Inducer::of("/obj/item/inducer/syndicate").multiplier, 2.0);
    }
}
