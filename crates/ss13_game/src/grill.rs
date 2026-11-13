//! The charcoal grill (server): tg `/obj/machinery/grill` (`food_and_drinks/machinery/grill.dm`)
//! with `/datum/component/sizzle` and `/datum/element/grilled_item`.
//!
//! - Fuel: coal or wood sheets go into its firebox (a stored stack of the type takes them up to
//!   50, else the stack goes in); one sheet burns into `5 × (0.5 + 5)` fuel, coal twice that. An
//!   open reagent container pours `amount_per_transfer_from_this` (30 at most): monkey energy
//!   ×4, oil ×3, welding fuel ×2, ethanol ×1 per unit times 5.5, anything else takes fuel away.
//!   Both need it anchored and nothing on it.
//! - Food (anything edible) goes onto a fuelled, anchored grill; an empty hand takes it off.
//!   Every machine tick it burns 0.5 fuel/s (another sheet when short; nothing left stops it),
//!   puffs bad smoke now and then (0.5%/s) and, with food on it, 5 fuel/s more: the food grills
//!   (`grill_time`), gets 0.5 u/s of char and grill marks ([`Sizzle`]); past its grilling time
//!   (its `grill_time` roll, else 20 s) it is renamed by how long it grilled ([`grill_name`]).
//!   Food put back keeps the time it already grilled.
//! - A wrench (out of combat mode) fastens it; unanchored, a crowbar takes it apart into an
//!   igniter, 5 iron and 5 rods, dropping its food and fuel (a puff of bad smoke when it had
//!   fuel).
// shortcut: the grill marks are the whole `grillmarks` state over the food (tg cuts them to the
// food's shape with icon blending); no screentips, no `quality_food_ingredient` buff for grilled
// ingredients.

use bevy::prelude::*;
use ss13_mapload::{DmFood, DmItem, DmName, DmObj, IconIndex};
use ss13_reagents::{Holder, Owner, flags, is_under};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};

use crate::chem::{Chem, Reagents, TransferAmount, name_of};
use crate::combat::{CombatMode, CombatRng};
use crate::power::{MACHINE_TICK_SECS, MachineTicks};
use crate::sound::{PlaySound, playsound};
use crate::storage::{Stored, StoredIn, take_out};
use crate::tools::{Actor, Stack, ToolKind, tell, tool_of, use_tool};

pub const GRILL: &str = "/obj/machinery/grill";
const COAL: &str = "/obj/item/stack/sheet/mineral/coal";
const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";
const REAGENT_CONTAINER: &str = "/obj/item/reagent_containers";
const CHAR: &str = "/datum/reagent/consumable/char";
const IGNITER: &str = "/obj/item/assembly/igniter";
const IRON: &str = "/obj/item/stack/sheet/iron";
const RODS: &str = "/obj/item/stack/rods";
const KITCHEN_DMI: &str = "icons/obj/machines/kitchen.dmi";
const GRILL_MARKS: &str = "grillmarks";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";
const CROWBAR: &str = "sound/items/tools/crowbar.ogg";
/// `GRILL_FUELUSAGE_IDLE`, `GRILL_FUELUSAGE_ACTIVE`: fuel per second.
const FUEL_IDLE: f32 = 0.5;
const FUEL_ACTIVE: f32 = 5.0;
/// `create_reagents(30, NO_REACT)`: what one pour can hold.
const POUR_VOLUME: f32 = 30.0;
/// `MAX_STACK_SIZE`.
const MAX_STACK: u32 = 50;
/// `time_limit`: 20 seconds, in deciseconds.
const DEFAULT_GRILL_DS: f32 = 200.0;
/// `default_unfasten_wrench`, deciseconds.
const WRENCH_DS: f32 = 20.0;
/// `fuel_map`: reagent type -> fuel per unit (before `× (idle + active)`).
const FUEL_MAP: [(&str, f32); 4] = [
    ("/datum/reagent/consumable/monkey_energy", 4.0),
    ("/datum/reagent/fuel/oil", 3.0),
    ("/datum/reagent/fuel", 2.0),
    ("/datum/reagent/consumable/ethanol", 1.0),
];

/// A grill's live state (`grill_fuel`, `grilled_item`, `grill_time` in deciseconds); fuel
/// stacks and the food are stored in it.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Grill {
    pub fuel: f32,
    pub item: Option<Entity>,
    pub time: f32,
    /// `begin_processing` .. `PROCESS_KILL`.
    pub processing: bool,
}

/// `/datum/component/sizzle`: how long the food has grilled (deciseconds).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Sizzle(pub f32);

/// `/datum/element/grilled_item` (`TRAIT_FOOD_BBQ_GRILLED`): the name and description it had
/// before (`initial(name)`), as `Detach` restores them.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct BbqGrilled {
    pub name: String,
    pub desc: String,
}

pub struct GrillPlugin;

impl Plugin for GrillPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `attack_hand`: the food off the grill.
        app.add_click_handler(ClickPhase::EmptyHand, 591, |w, c| {
            crate::grill::attack_hand(w, c.mob, c.target)
        });
        // `item_interaction`, `wrench_act`, `crowbar_act`.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1891, |w, c| {
            c.held
                .is_some_and(|h| crate::grill::item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(Update, grill_tick.after(crate::power::PowerSystems))
            // `grill_loop`: food put on .. taken off (`Exited`).
            .add_systems(
                Update,
                crate::looping_sound::sync::<Grill>(&crate::looping_sound::defs::GRILL, |g| {
                    g.item.is_some()
                }),
            );
    }
}

fn is_grill(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, GRILL))
}

fn grill_of(world: &World, e: Entity) -> Grill {
    world.get::<Grill>(e).cloned().unwrap_or_default()
}

fn is_anchored(world: &World, e: Entity) -> bool {
    world.get::<crate::obj_damage::Anchored>(e).map_or_else(
        || world.get::<DmObj>(e).is_some_and(|o| o.anchored),
        |a| a.0,
    )
}

fn item_path(world: &World, item: Entity) -> String {
    world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn contents(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(e)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

/// `update_icon_state`.
fn refresh_look(world: &mut World, e: Entity) {
    let grill = grill_of(world, e);
    let state = if grill.item.is_some() {
        "grill"
    } else if grill.fuel > 0.0 {
        "grill_on"
    } else {
        "grill_open"
    };
    crate::obj_damage::set_icon_state(world, e, state);
}

/// `attack_hand`: the food goes onto the grill's tile (`drop_location`).
pub fn attack_hand(world: &mut World, mob: Entity, target: Entity) -> bool {
    if !is_grill(world, target) {
        return false;
    }
    let Some(item) = grill_of(world, target).item else {
        return false;
    };
    crate::balloon::balloon_alert(world, mob, target, "предмет убран");
    if let Some(&pos) = world.get::<TilePos>(target) {
        take_out(world, item, pos);
    }
    exited(world, target);
    true
}

/// `Exited(grilled_item)`.
fn exited(world: &mut World, e: Entity) {
    if let Some(mut g) = world.get_mut::<Grill>(e) {
        g.item = None;
        g.time = 0.0;
    }
    refresh_look(world, e);
}

/// `item_interaction`, `wrench_act`, `crowbar_act`. True when the click was the grill's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_grill(world, target) || world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    if world.get::<Grill>(target).is_none() {
        world.entity_mut(target).insert(Grill::default());
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    match tool_of(world, held).map(|(kind, _)| kind) {
        Some(ToolKind::Wrench) => {
            let anchored = is_anchored(world, target);
            if let Some(&pos) = world.get::<TilePos>(target) {
                playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
            }
            use_tool(
                world,
                actor,
                target,
                WRENCH_DS,
                0,
                crate::construction::Job::Unfasten { anchored },
            );
            return true;
        }
        Some(ToolKind::Crowbar) => {
            if is_anchored(world, target) {
                crate::balloon::balloon_alert(world, mob, target, "нужно открутить от пола!");
            } else {
                deconstruct(world, target);
            }
            return true;
        }
        _ => {}
    }
    let path = item_path(world, held);
    let busy = grill_of(world, target).item.is_some();
    if is_under(&path, COAL) || is_under(&path, WOOD) {
        if busy {
            return false;
        }
        add_stack(world, client, mob, held, target, &path);
        return true;
    }
    if is_under(&path, REAGENT_CONTAINER)
        && world
            .get::<Reagents>(held)
            .is_some_and(|r| r.0.is_open_container())
    {
        if busy {
            return false;
        }
        pour(world, client, mob, held, target);
        return true;
    }
    if world.get::<DmFood>(held).is_none() {
        return false;
    }
    if !is_anchored(world, target) {
        crate::balloon::balloon_alert(world, mob, target, "нужно прикрутить к полу!");
        return true;
    }
    if crate::inventory::no_drop(world, held) {
        return false;
    }
    if busy {
        crate::balloon::balloon_alert(world, mob, target, "уберите предмет!");
        return true;
    }
    if grill_of(world, target).fuel <= 0.0 {
        crate::balloon::balloon_alert(world, mob, target, "нет топлива!");
        return true;
    }
    crate::cooking::from_hand(world, held, target);
    // A sizzling food keeps the time it already grilled.
    let time = world.get::<Sizzle>(held).map_or(0.0, |s| s.0);
    if let Some(mut g) = world.get_mut::<Grill>(target) {
        g.item = Some(held);
        g.time = time;
    }
    let (what, name) = (name_of(world, held), name_of(world, target));
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы положили {what} на {name}."),
    );
    refresh_look(world, target);
    true
}

/// Coal or wood into the firebox: merged into a stored stack of its kind, else moved in.
fn add_stack(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    path: &str,
) {
    if !is_anchored(world, target) {
        crate::balloon::balloon_alert(world, mob, target, "нужно прикрутить к полу!");
        return;
    }
    let kind = if is_under(path, COAL) { COAL } else { WOOD };
    let stored = contents(world, target)
        .into_iter()
        .find(|&s| world.get::<Stack>(s).is_some() && is_under(&item_path(world, s), kind));
    let what = name_of(world, held);
    match stored {
        Some(stored) => {
            if crate::tools::stack_amount(world, stored) >= MAX_STACK {
                crate::balloon::balloon_alert(world, mob, target, "заполнено!");
                return;
            }
            crate::construction::merge_into(world, held, stored);
        }
        None => crate::cooking::from_hand(world, held, target),
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы добавляете {what} в топку."),
    );
    if grill_of(world, target).fuel <= 0.0 {
        burn_stack(world, target);
        if let Some(mut g) = world.get_mut::<Grill>(target) {
            g.processing = true;
        }
    }
}

/// An open container poured into the firebox and turned into fuel at once.
fn pour(world: &mut World, client: Entity, mob: Entity, held: Entity, target: Entity) {
    if !is_anchored(world, target) {
        crate::balloon::balloon_alert(world, mob, target, "нужно прикрутить к полу!");
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let amount = world.get::<TransferAmount>(held).map_or(5.0, |t| t.0);
    let mut firebox = Holder::new(
        POUR_VOLUME,
        flags::NO_REACT,
        Owner {
            path: GRILL.to_owned(),
            living: false,
        },
    );
    let Some(mut source) = world.get::<Reagents>(held).map(|r| r.0.clone()) else {
        return;
    };
    let moved = source.trans_to(&chem, &mut firebox, amount, 1.0, None, true, false);
    world.entity_mut(held).insert(Reagents(source));
    if moved <= 0.0 {
        crate::balloon::balloon_alert(world, mob, target, "топливо не добавлено!");
        return;
    }
    let reagents: Vec<(String, f32)> = firebox
        .reagents
        .iter()
        .map(|r| (chem.def(r.id).path.clone(), r.volume))
        .collect();
    let added: f32 = reagents
        .iter()
        .map(|(path, volume)| fuel_boost(path) * volume * (FUEL_IDLE + FUEL_ACTIVE))
        .sum();
    let mut g = grill_of(world, target);
    g.fuel = (g.fuel + added).max(0.0);
    if g.fuel > 0.0 {
        g.processing = true;
    }
    world.entity_mut(target).insert(g);
    refresh_look(world, target);
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "Вы наливаете {} {} в топку.",
            crate::chem::dm_num(moved),
            units_word(moved)
        ),
    );
}

/// `fuel_map[stored.type]` (the exact type), `-1` for anything else.
pub fn fuel_boost(path: &str) -> f32 {
    FUEL_MAP
        .iter()
        .find(|(p, _)| *p == path)
        .map_or(-1.0, |&(_, b)| b)
}

/// `declension_ru(amount, "юнит", "юнита", "юнитов")`.
fn units_word(amount: f32) -> &'static str {
    if amount.fract() != 0.0 {
        return "юнита";
    }
    let n = amount as u64;
    match (n % 10, n % 100) {
        (1, r) if r != 11 => "юнит",
        (2..=4, r) if !(12..=14).contains(&r) => "юнита",
        _ => "юнитов",
    }
}

/// `burn_stack`: every stored stack burns one sheet into fuel.
fn burn_stack(world: &mut World, e: Entity) {
    let food = grill_of(world, e).item;
    let mut added = 0.0;
    for stack in contents(world, e) {
        if Some(stack) == food || world.get::<Stack>(stack).is_none() {
            continue;
        }
        let mut boost = 5.0 * (FUEL_IDLE + FUEL_ACTIVE);
        if is_under(&item_path(world, stack), COAL) {
            boost *= 2.0;
        }
        if crate::tools::use_stack(world, stack, 1) {
            added += boost;
        }
    }
    if let Some(mut g) = world.get_mut::<Grill>(e) {
        g.fuel += added;
    }
    refresh_look(world, e);
}

/// `default_deconstruction_crowbar` -> `on_deconstruction(TRUE)` and `dump_inventory_contents`.
fn deconstruct(world: &mut World, e: Entity) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    playsound(world, PlaySound::new(pos, CROWBAR, 50.0, true));
    for thing in contents(world, e) {
        take_out(world, thing, pos);
    }
    crate::construction::spawn_obj(world, IGNITER, pos);
    crate::construction::spawn_stack(world, IRON, pos, 5);
    crate::construction::spawn_stack(world, RODS, pos, 5);
    if grill_of(world, e).fuel > 0.0 {
        crate::effects::smoke::do_smoke(world, 1, pos, None, crate::effects::smoke::SmokeKind::Bad);
    }
    world.despawn(e);
    crate::construction::tile_changed(world, pos);
}

fn grill_tick(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    for _ in from..ticks {
        let grills: Vec<Entity> = world
            .query_filtered::<Entity, With<Grill>>()
            .iter(world)
            .collect();
        for e in grills {
            process_grill(world, e, MACHINE_TICK_SECS as f32);
        }
    }
}

/// `grill/process(seconds_per_tick)`.
pub fn process_grill(world: &mut World, e: Entity, dt: f32) {
    // The food left some other way (the grill was emptied by something else).
    if let Some(item) = grill_of(world, e).item
        && world.get::<StoredIn>(item).is_none_or(|s| s.0 != e)
    {
        exited(world, e);
    }
    let pos = world.get::<TilePos>(e).copied();
    if !grill_of(world, e).processing {
        return;
    }
    if !is_anchored(world, e) {
        stop(world, e);
        return;
    }
    let idle = FUEL_IDLE * dt;
    if grill_of(world, e).fuel < idle {
        if let Some(mut g) = world.get_mut::<Grill>(e) {
            g.fuel = 0.0;
        }
        burn_stack(world, e);
        if grill_of(world, e).fuel < idle {
            stop(world, e);
            return;
        }
    }
    if let Some(mut g) = world.get_mut::<Grill>(e) {
        g.fuel -= idle;
    }
    let smoke = world
        .resource_mut::<CombatRng>()
        .chance(crate::cooking::spt_percent(0.5, dt));
    if smoke && let Some(pos) = pos {
        crate::effects::smoke::do_smoke(world, 1, pos, None, crate::effects::smoke::SmokeKind::Bad);
    }
    let active = FUEL_ACTIVE * dt;
    if let Some(item) = grill_of(world, e).item {
        if grill_of(world, e).fuel < active {
            burn_stack(world, e);
            if grill_of(world, e).fuel < active {
                refresh_look(world, e);
                return;
            }
        }
        let last = grill_of(world, e).time;
        let time = last + dt * 10.0;
        if let Some(mut g) = world.get_mut::<Grill>(e) {
            g.time = time;
            g.fuel -= active;
        }
        add_char(world, item, 0.5 * dt);
        sizzle(world, item, time);
        let limit = grill_limit(world, item);
        if time >= limit {
            grilled_item(world, item, time);
        }
    }
    refresh_look(world, e);
}

fn stop(world: &mut World, e: Entity) {
    if let Some(mut g) = world.get_mut::<Grill>(e) {
        g.processing = false;
    }
    refresh_look(world, e);
}

/// `custom_grilling.required_cook_time` (the food's grillable roll), else 20 s.
fn grill_limit(world: &mut World, item: Entity) -> f32 {
    let Some(range) = world
        .get::<DmFood>(item)
        .filter(|f| !f.grill_result.is_empty())
        .map(|f| f.grill_time)
    else {
        return DEFAULT_GRILL_DS;
    };
    let mut times = crate::cooking::cook_times(world, item);
    match times.grill_required {
        Some(r) => r,
        None => {
            let r = crate::cooking::roll(world, range);
            times.grill_required = Some(r);
            world.entity_mut(item).insert(times);
            r
        }
    }
}

/// `grilled_item.reagents.add_reagent(/datum/reagent/consumable/char, amount)`.
fn add_char(world: &mut World, item: Entity, amount: f32) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(id) = chem.id(CHAR) else {
        return;
    };
    if let Some(mut r) = world.get_mut::<Reagents>(item) {
        r.0.add_reagent_list(&chem, &[(id, amount)], None, false);
    }
}

/// `AddComponent(/datum/component/sizzle, grill_time)`: the grill marks once, the time always.
pub fn sizzle(world: &mut World, item: Entity, grilled_time: f32) {
    let first = world.get::<Sizzle>(item).is_none();
    world.entity_mut(item).insert(Sizzle(grilled_time));
    if !first {
        return;
    }
    let Some(marks) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(KITCHEN_DMI, GRILL_MARKS))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<Appearance>(item) {
        look.overlays.push(Overlay {
            icon_state: marks,
            dir: None,
            color: None,
            pixel_offset: [0, 0],
            plane: None,
            only_dirs: 0,
        });
    }
}

/// `/datum/element/grilled_item/Attach(grill_time)`: the prefix (or the whole name) for the
/// time grilled, deciseconds; `None` below 20 s.
pub fn grill_name(name: &str, desc: &str, grill_time: f32) -> Option<(String, String)> {
    let secs = grill_time / 10.0;
    if secs < 20.0 {
        return None;
    }
    let (prefix, note) = if secs <= 30.0 {
        ("lightly-grilled", "It's been lightly grilled.")
    } else if secs <= 80.0 {
        ("grilled", "It's been grilled.")
    } else if secs <= 100.0 {
        ("heavily grilled", "It's been heavily grilled.")
    } else {
        let name = format!("Powerfully Grilled {name}");
        let desc = format!("A {name}. Reminds you of your wife, wait, no, it's prettier!");
        return Some((name, desc));
    };
    Some((format!("{prefix} {name}"), format!("{desc} {note}")))
}

/// `RemoveElement(grilled_item, last)` + `AddElement(grilled_item, grill_time)`: the name from
/// the one before the first grilling.
fn grilled_item(world: &mut World, item: Entity, grill_time: f32) {
    let base = match world.get::<BbqGrilled>(item).cloned() {
        Some(base) => base,
        None => {
            let Some(dm) = world.get::<DmName>(item) else {
                return;
            };
            let base = BbqGrilled {
                name: dm.name.clone(),
                desc: dm.desc.clone(),
            };
            world.entity_mut(item).insert(base.clone());
            base
        }
    };
    let (name, desc) =
        grill_name(&base.name, &base.desc, grill_time).unwrap_or((base.name, base.desc));
    if let Some(mut dm) = world.get_mut::<DmName>(item) {
        dm.name = name;
        dm.desc = desc;
    }
}

/// `grill/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if !is_grill(world, target) {
        return;
    }
    lines.push((
        Span::Notice,
        "Добавляйте топливо, такое как дрова или уголь, или открытый контейнер с подходящим \
         видом топлива."
            .to_owned(),
    ));
    lines.push((
        Span::Notice,
        "Monkey energy > Oil > Welding fuel > Ethanol. Другие виды топлива вызывают плохие \
         последствия"
            .to_owned(),
    ));
    lines.push((
        Span::Notice,
        "Поместите любую еду на верхнюю часть с помощью руки, чтобы начать жарить.".to_owned(),
    ));
    if is_anchored(world, target) {
        lines.push((Span::Notice, "Машина прикручена к полу.".to_owned()));
    } else {
        lines.push((
            Span::Notice,
            "Машина может быть разобрана ломом.".to_owned(),
        ));
        lines.push((
            Span::Warning,
            "Машина должна быть прикручена к полу для работы.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grill_names_follow_the_time() {
        assert_eq!(grill_name("steak", "Meat.", 150.0), None);
        assert_eq!(
            grill_name("steak", "Meat.", 250.0).unwrap().0,
            "lightly-grilled steak"
        );
        assert_eq!(
            grill_name("steak", "Meat.", 500.0).unwrap().0,
            "grilled steak"
        );
        assert_eq!(
            grill_name("steak", "Meat.", 900.0).unwrap().0,
            "heavily grilled steak"
        );
        assert_eq!(
            grill_name("steak", "Meat.", 1200.0).unwrap().0,
            "Powerfully Grilled steak"
        );
    }

    #[test]
    fn coal_burns_and_the_food_grills() {
        let mut world = World::new();
        world.insert_resource(CombatRng(7));
        let grill = world
            .spawn((
                DmObj {
                    path: GRILL.to_owned(),
                    anchored: true,
                    ..default()
                },
                Grill {
                    processing: true,
                    ..default()
                },
            ))
            .id();
        world.spawn((
            DmItem {
                path: COAL.to_owned(),
                ..default()
            },
            Stack { amount: 2 },
            StoredIn(grill),
        ));
        let steak = world
            .spawn((
                DmFood::default(),
                DmName {
                    name: "steak".to_owned(),
                    desc: "Meat.".to_owned(),
                },
                StoredIn(grill),
            ))
            .id();
        world.get_mut::<Grill>(grill).unwrap().item = Some(steak);
        // No fuel: the first tick burns a sheet of coal (55 fuel) and spends 1 + 10.
        process_grill(&mut world, grill, 2.0);
        assert_eq!(world.get::<Grill>(grill).unwrap().fuel, 44.0);
        assert_eq!(world.get::<Sizzle>(steak), Some(&Sizzle(20.0)));
        // 11 fuel a tick: the second sheet at the 6th tick, nothing left at the 11th.
        for _ in 0..10 {
            process_grill(&mut world, grill, 2.0);
        }
        let g = world.get::<Grill>(grill).unwrap();
        assert_eq!((g.time, g.fuel, g.processing), (200.0, 0.0, false));
        // 20 s on the grill: the default limit, lightly grilled.
        assert_eq!(
            world.get::<DmName>(steak).unwrap().name,
            "lightly-grilled steak"
        );
    }

    #[test]
    fn fuel_map_is_exact() {
        assert_eq!(fuel_boost("/datum/reagent/fuel"), 2.0);
        assert_eq!(fuel_boost("/datum/reagent/fuel/oil"), 3.0);
        assert_eq!(fuel_boost("/datum/reagent/water"), -1.0);
        assert_eq!(units_word(1.0), "юнит");
        assert_eq!(units_word(3.0), "юнита");
        assert_eq!(units_word(11.0), "юнитов");
    }
}
