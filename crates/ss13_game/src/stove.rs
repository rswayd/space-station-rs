//! Stoves, soup pots and soups (server): tg `/datum/component/stove` (`stove_component.dm`) on
//! `/obj/machinery/stove` and `/obj/machinery/oven/range`, `/obj/item/reagent_containers/cup/
//! soup_pot` (`stove.dm`) and `/datum/chemical_reaction/food/soup` (`soup_mixtures.dm`).
//!
//! - A stove takes one open container on its burner (it lies on the stove's tile at the burner's
//!   offset and is picked up like any item; a mapped range starts with a soup pot). A right click
//!   with an empty hand switches the burner (a range's right click takes the tray out while it
//!   is open, `crate::cooking`). Lit and powered, every machine tick it heats the container
//!   (`expose_temperature(SOUP_BURN_TEMP + 80, heat_coefficient)`), uses `active_power_usage` and
//!   exposes the tile to 620 K; without power it goes out. `heat_coefficient = 0.033 ×
//!   max(round(micro-laser tiers × 0.5), 1)`.
//! - A soup pot takes up to 24 ingredients: any item smaller than bulky that is not an open
//!   container (a tray or bag on it with a right click: all of them); a right click with an
//!   empty hand takes the first one out. A closed range moves a tray's items into its pot.
//! - Soups are reactions the engine does not start by itself: every reagent tick (0.25 s) a pot
//!   whose ingredients cover a soup's `required_ingredients` (by type or a parent type; stacks
//!   count their amount) starts it if its reagents and temperature allow
//!   ([`ss13_reagents::Holder::start_reaction`]). A soup with a `resulting_food_path` makes its
//!   food from its ingredients first. While it reacts, every step moves `max(20% of each
//!   ingredient's starting volume, 2)` units of it into the pot (`percentage_of_nutriment_
//!   converted` of that much nutriment and vitamin removed, the rest at
//!   `ingredient_reagent_multiplier`); a pot 95% full boils over (its reagents halve). When the
//!   reaction ends having made soup, the ingredients that gave reagents or were required are
//!   used up, others without reagents get fried, and what is left is dumped next to the pot.
// shortcut: one soup reacts in a pot at a time; the multiplier is not clamped to the ingredient
// count, removing an ingredient mid-reaction stops every reaction of the pot; no foam when it
// boils over, no steam particles, sounds or pot examine; the custom soup (burnt mess of
// anything) and the soups with their own checks are not converted; `on_reaction`'s re-adding
// of required reagents beyond the food made (a duplicate for equilibrium reactions) is left out.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmPower, DmWeight, IconIndex};
use ss13_reagents::{Chemistry, Holder, is_under};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};

use crate::chem::{Chem, Reagents, name_of, visible};
use crate::inventory::{CarriedBy, Inventory};
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use crate::storage::{Stored, StoredIn, put_inside};
use crate::tools::tell;

pub const STOVE_ROOT: &str = "/obj/machinery/stove";
const RANGE: &str = "/obj/machinery/oven/range";
pub const SOUP_POT: &str = "/obj/item/reagent_containers/cup/soup_pot";
const TRAY: &str = "/obj/item/storage/bag/tray";
const NUTRIMENT: &str = "/datum/reagent/consumable/nutriment";
const VITAMIN: &str = "/datum/reagent/consumable/nutriment/vitamin";
/// `SOUP_BURN_TEMP + 80`, the burner's `heat_coefficient`.
const BURNER_TEMP: f32 = 540.0 + 80.0;
const HEAT_COEFFICIENT: f32 = 0.033;
/// `max_ingredients`, `WEIGHT_CLASS_BULKY`.
pub const MAX_INGREDIENTS: usize = 24;
const WEIGHT_CLASS_BULKY: u8 = 5;
/// `SSreagents` wait.
const STEP_SECS: f64 = 0.25;
/// `fried_item` time of a leftover without reagents: 30 seconds, in deciseconds.
const LEFTOVER_FRY_DS: f32 = 300.0;
/// The burner's flame and light colors.
const FLAME: [u8; 4] = [0x00, 0x6E, 0xFF, 0xFF];
const LIGHT: [u8; 4] = [0x00, 0xFF, 0x00, 0xFF];

/// A stove's burner.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Stove {
    pub on: bool,
    pub container: Option<Entity>,
    /// `container_x`, `container_y`.
    pub offset: [i16; 2],
}

/// A soup pot's ingredients and the soup it is making.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SoupPot {
    pub ingredients: Vec<Entity>,
    pub soup: Option<ActiveSoup>,
}

/// A running soup reaction: its index, each ingredient's starting volume (`reaction.data`), and
/// how much of its first result there was when it started.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveSoup {
    pub reaction: u32,
    pub cached: Vec<(Entity, f32)>,
    pub result_before: f32,
}

#[derive(Resource, Default)]
struct SoupClock(f64);

pub struct StovePlugin;

impl Plugin for StovePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoupClock>()
            .add_systems(
                Startup,
                setup_mapped.after(crate::power::init_power),
            )
            .add_systems(
                Update,
                (
                    stove_tick.after(crate::power::PowerSystems),
                    soup_steps.run_if(any_with_component::<SoupPot>),
                ),
            )
            .add_observer(add_pot_state);
    }
}

fn add_pot_state(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, SOUP_POT))
    {
        commands.entity(add.entity).insert_if_new(SoupPot::default());
    }
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmPower>(e)
        .map(|p| p.path.clone())
        .or_else(|| world.get::<DmItem>(e).map(|i| i.path.clone()))
        .unwrap_or_default()
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query::<(Entity, &DmPower)>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| is_under(&p.path, STOVE_ROOT) || is_under(&p.path, RANGE))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e, true);
    }
}

/// `AddComponent(/datum/component/stove, ...)`: a mapped range starts with a soup pot.
pub(crate) fn setup(world: &mut World, e: Entity, mapload: bool) {
    let range = is_under(&path_of(world, e), RANGE);
    let offset = if range { [-6, 14] } else { [-6, 16] };
    world.entity_mut(e).insert(Stove {
        on: false,
        container: None,
        offset,
    });
    if range
        && mapload
        && let Some(&pos) = world.get::<TilePos>(e)
        && let Some(pot) = crate::construction::spawn_obj(world, SOUP_POT, pos)
    {
        place(world, e, pot);
    }
    refresh_look(world, e);
}

/// `add_container`: on the stove's tile at the burner.
fn place(world: &mut World, stove: Entity, item: Entity) {
    let (Some(&pos), Some(offset)) = (
        world.get::<TilePos>(stove),
        world.get::<Stove>(stove).map(|s| s.offset),
    ) else {
        return;
    };
    world.entity_mut(item).remove::<StoredIn>();
    crate::inventory::put_down_at(world, item, pos, offset);
    crate::construction::tile_changed(world, pos);
    if let Some(mut s) = world.get_mut::<Stove>(stove) {
        s.container = Some(item);
    }
    refresh_look(world, stove);
}

/// `on_exited`: a container no longer on the stove's tile is off the burner.
fn prune(world: &mut World, stove: Entity) {
    let Some(container) = world.get::<Stove>(stove).and_then(|s| s.container) else {
        return;
    };
    let here = world.get::<TilePos>(stove).copied();
    let gone = world.get_entity(container).is_err()
        || world.get::<CarriedBy>(container).is_some()
        || world.get::<TilePos>(container).copied() != here;
    if gone {
        if let Some(mut s) = world.get_mut::<Stove>(stove) {
            s.container = None;
        }
        refresh_look(world, stove);
    }
}

fn take_from_hand(world: &mut World, mob: Entity, held: Entity) {
    world.entity_mut(held).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
    {
        *slot = None;
    }
}

/// tg `on_attackby` of the stove and `item_interaction` of the soup pot. True when the click was
/// theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<Stove>(target).is_some() {
        return stove_item(world, client, mob, held, target);
    }
    if world.get::<SoupPot>(target).is_some() {
        return pot_item(world, client, mob, held, target, right);
    }
    false
}

fn stove_item(world: &mut World, client: Entity, mob: Entity, held: Entity, e: Entity) -> bool {
    prune(world, e);
    let container = world.get::<Stove>(e).and_then(|s| s.container);
    // A closed range: a tray's items into its pot.
    if is_under(&path_of(world, e), RANGE)
        && is_under(&path_of(world, held), TRAY)
        && let Some(pot) = container.filter(|&c| world.get::<SoupPot>(c).is_some())
        && !world.get::<crate::cooking::Oven>(e).is_some_and(|o| o.open)
    {
        load_from(world, client, held, pot);
        return true;
    }
    if !world
        .get::<Reagents>(held)
        .is_some_and(|r| r.0.is_open_container())
    {
        return false;
    }
    if container.is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "Вы не посмеете приготовить два разных блюда в одной печи. Что если их содержимое \
             пересечется?",
        );
        return true;
    }
    let (what, name) = (name_of(world, held), name_of(world, e));
    take_from_hand(world, mob, held);
    place(world, e, held);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы перемещаете {what} на {name}."),
    );
    true
}

/// `can_add_ingredient`.
fn can_add(world: &World, item: Entity) -> bool {
    !world
        .get::<Reagents>(item)
        .is_some_and(|r| r.0.is_open_container())
        && world
            .get::<DmWeight>(item)
            .is_none_or(|w| w.w_class < WEIGHT_CLASS_BULKY)
        && world.get::<DmItem>(item).is_some()
}

fn pot_item(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    pot: Entity,
    right: bool,
) -> bool {
    if right {
        if world.get::<crate::storage::Storage>(held).is_none() {
            return false;
        }
        load_from(world, client, held, pot);
        return true;
    }
    if world
        .get::<Reagents>(held)
        .is_some_and(|r| r.0.is_open_container())
    {
        // Pouring: `crate::chem`.
        return false;
    }
    if !can_add(world, held) {
        return true;
    }
    let count = world.get::<SoupPot>(pot).map_or(0, |p| p.ingredients.len());
    if count >= MAX_INGREDIENTS {
        tell(world, client, Span::Warning, "слишком много ингредиентов!");
        return true;
    }
    take_from_hand(world, mob, held);
    put_inside(world, held, pot);
    if let Some(mut p) = world.get_mut::<SoupPot>(pot) {
        p.ingredients.push(held);
    }
    tell(world, client, Span::Notice, "ингредиент добавлен");
    refresh_pot(world, pot);
    true
}

/// `transfer_from_container_to_pot`: every item of the storage the pot can take, until full.
fn load_from(world: &mut World, client: Entity, storage: Entity, pot: Entity) {
    let items: Vec<Entity> = world
        .get::<Stored>(storage)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let mut loaded = 0;
    for item in items {
        if !can_add(world, item) {
            continue;
        }
        if world
            .get::<SoupPot>(pot)
            .is_some_and(|p| p.ingredients.len() >= MAX_INGREDIENTS)
        {
            tell(world, client, Span::Warning, "заполнено!");
            break;
        }
        put_inside(world, item, pot);
        if let Some(mut p) = world.get_mut::<SoupPot>(pot) {
            p.ingredients.push(item);
        }
        loaded += 1;
    }
    if loaded > 0 {
        let name = name_of(world, pot);
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "Вы перемещаете {loaded} {} в {name}.",
                crate::kitchen::items_word(loaded)
            ),
        );
        refresh_pot(world, pot);
    }
}

/// An empty-hand click: a stove's right click switches its burner; a pot's right click takes its
/// first ingredient out. True when the click was theirs.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity, right: bool) -> bool {
    if !right {
        return false;
    }
    if let Some(stove) = world.get::<Stove>(target).cloned() {
        toggle(world, client, target, stove.on);
        return true;
    }
    if let Some(first) = world
        .get::<SoupPot>(target)
        .and_then(|p| p.ingredients.first().copied())
    {
        if let Some(mut p) = world.get_mut::<SoupPot>(target) {
            p.ingredients.remove(0);
        }
        crate::cooking::give_to(world, mob, target, first);
        tell(world, client, Span::Notice, "ингредиент убран");
        refresh_pot(world, target);
        return true;
    }
    false
}

/// `toggle_mode`.
fn toggle(world: &mut World, client: Entity, e: Entity, on: bool) {
    if !on && world.get::<Unpowered>(e).is_some() {
        tell(world, client, Span::Warning, "нет питания!");
        return;
    }
    if let Some(mut s) = world.get_mut::<Stove>(e) {
        s.on = !on;
    }
    let state = if on { "отключен" } else { "включен" };
    tell(world, client, Span::Notice, format!("прожиг {state}"));
    refresh_look(world, e);
}

/// `on_refresh_parts`: `heat_coefficient`.
pub fn heat_coefficient(world: &World, e: Entity) -> f32 {
    let r = crate::stock_parts::ratings(world, e);
    let lasers: f32 = r
        .tiers(crate::stock_parts::PartKind::MicroLaser)
        .map(|t| t as f32 * 0.5)
        .sum();
    HEAT_COEFFICIENT * lasers.round().max(1.0)
}

/// Every `SSmachines` tick: lit stoves heat their container (`/datum/component/stove/process`).
// Cost: a scan of the stoves (one or two a map) every 2 s.
fn stove_tick(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    for _ in from..ticks {
        let mut q = world.query::<(Entity, &Stove)>();
        let lit: Vec<Entity> = q.iter(world).filter(|(_, s)| s.on).map(|(e, _)| e).collect();
        for e in lit {
            process_stove(world, e);
        }
    }
}

fn process_stove(world: &mut World, e: Entity) {
    if world.get::<Unpowered>(e).is_some() {
        if let Some(mut s) = world.get_mut::<Stove>(e) {
            s.on = false;
        }
        refresh_look(world, e);
        return;
    }
    prune(world, e);
    let coeff = heat_coefficient(world, e);
    if let Some(container) = world.get::<Stove>(e).and_then(|s| s.container)
        && let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
        && let Some(mut r) = world.get_mut::<Reagents>(container)
    {
        r.0.expose_temperature(&chem, BURNER_TEMP, coeff);
    }
    let watts = world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, watts * MACHINE_TICK_SECS);
    if let Some(&pos) = world.get::<TilePos>(e)
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        atmos.hotspot_expose(pos, f64::from(BURNER_TEMP), 100.0, false);
    }
}

/// The burner's overlays (`on_overlay_update`), for this module and the range's look.
pub fn burner_overlays(world: &World, e: Entity) -> Vec<Overlay> {
    let Some(stove) = world.get::<Stove>(e) else {
        return Vec::new();
    };
    if !stove.on {
        return Vec::new();
    }
    let (Some(atom), Some(icons)) = (world.get::<DmAtom>(e), world.get_resource::<IconIndex>())
    else {
        return Vec::new();
    };
    let base = if is_under(&path_of(world, e), RANGE) {
        "range"
    } else {
        "stove"
    };
    [("on_flame", FLAME), ("on_overlay", LIGHT)]
        .into_iter()
        .filter_map(|(suffix, color)| {
            let mut o = Overlay::new(icons.get(&atom.icon, &format!("{base}_{suffix}"))?);
            o.color = Some(color);
            Some(o)
        })
        .collect()
}

fn refresh_look(world: &mut World, e: Entity) {
    if world.get::<crate::cooking::Oven>(e).is_some() {
        crate::cooking::refresh_look(world, e);
        return;
    }
    let overlays = burner_overlays(world, e);
    if let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

/// `soup_pot/update_overlays`: the filling, colored by its reagents and its ingredients'.
fn refresh_pot(world: &mut World, pot: Entity) {
    let ingredients = world
        .get::<SoupPot>(pot)
        .map(|p| p.ingredients.clone())
        .unwrap_or_default();
    let mut all: Vec<ss13_reagents::Reagent> = world
        .get::<Reagents>(pot)
        .map(|r| r.0.reagents.clone())
        .unwrap_or_default();
    for i in &ingredients {
        if let Some(r) = world.get::<Reagents>(*i) {
            all.extend(r.0.reagents.iter().cloned());
        }
    }
    let overlay = if all.is_empty() && ingredients.is_empty() {
        None
    } else {
        let color = world
            .get_resource::<Chem>()
            .and_then(|c| ss13_reagents::mix_color(&c.0, &all));
        let id = world
            .get::<DmAtom>(pot)
            .zip(world.get_resource::<IconIndex>())
            .and_then(|(atom, icons)| icons.get(&atom.icon, "pot_filling_overlay"));
        id.map(|id| {
            let mut o = Overlay::new(id);
            o.color = color;
            o
        })
    };
    let overlays: Vec<Overlay> = overlay.into_iter().collect();
    if let Some(mut look) = world.get_mut::<Appearance>(pot)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

/// The soup reactions' indices (built once from the loaded chemistry).
#[derive(Resource)]
struct Soups(Vec<u32>);

fn soups(world: &mut World) -> Vec<u32> {
    if let Some(s) = world.get_resource::<Soups>() {
        return s.0.clone();
    }
    let list: Vec<u32> = world
        .get_resource::<Chem>()
        .map(|c| {
            c.0.reactions
                .iter()
                .enumerate()
                .filter(|(_, r)| r.def.soup.is_some())
                .map(|(i, _)| i as u32)
                .collect()
        })
        .unwrap_or_default();
    world.insert_resource(Soups(list.clone()));
    list
}

/// The parent of a DM type path (`/a/b/c` -> `/a/b`).
fn parent(path: &str) -> Option<&str> {
    path.rfind('/').filter(|&i| i > 0).map(|i| &path[..i])
}

/// `soup/pre_reaction_other_checks`: every required ingredient (by type, else the nearest
/// parent type it requires) is there in its count. `ingredients` are (type, count).
pub fn ingredients_met(required: &[(String, f32)], ingredients: &[(String, f32)]) -> bool {
    if required.is_empty() {
        return true;
    }
    let mut left: Vec<(String, f32)> = required.to_vec();
    let mut completed = 0;
    for (path, count) in ingredients {
        let mut ty: Option<&str> = Some(path);
        while let Some(t) = ty.filter(|t| *t != "/obj/item") {
            match left.iter_mut().find(|(p, _)| p == t) {
                Some(entry) if entry.1 > 0.0 => {
                    entry.1 -= count;
                    if entry.1 <= 0.0 {
                        completed += 1;
                        entry.1 = 0.0;
                    }
                    break;
                }
                // Already satisfied: this one is a spare.
                Some(_) => break,
                None => ty = parent(t),
            }
        }
    }
    completed == left.len()
}

fn ingredient_list(world: &World, items: &[Entity]) -> Vec<(String, f32)> {
    items
        .iter()
        .map(|&i| {
            let count = world
                .get::<crate::tools::Stack>(i)
                .map_or(1.0, |s| s.amount as f32);
            (path_of(world, i), count)
        })
        .collect()
}

/// Every [`STEP_SECS`]: soups start, step and finish.
fn soup_steps(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    {
        let mut clock = world.resource_mut::<SoupClock>();
        clock.0 += dt;
        if clock.0 < STEP_SECS {
            return;
        }
        clock.0 = 0.0;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let soups = soups(world);
    let mut q = world.query_filtered::<Entity, (With<SoupPot>, With<Reagents>)>();
    let pots: Vec<Entity> = q.iter(world).collect();
    for pot in pots {
        // Ingredients taken out some other way are no longer in it (`Exited`).
        let kept: Vec<Entity> = world
            .get::<SoupPot>(pot)
            .map(|p| p.ingredients.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|&i| world.get::<StoredIn>(i).is_some_and(|s| s.0 == pot))
            .collect();
        if let Some(mut p) = world.get_mut::<SoupPot>(pot)
            && p.ingredients != kept
        {
            p.ingredients = kept;
        }
        let active = world.get::<SoupPot>(pot).and_then(|p| p.soup.clone());
        match active {
            Some(soup) => step_soup(world, &chem, pot, soup),
            None => try_start(world, &chem, &soups, pot),
        }
    }
}

fn try_start(world: &mut World, chem: &Chemistry, soups: &[u32], pot: Entity) {
    let Some(state) = world.get::<SoupPot>(pot).cloned() else {
        return;
    };
    if state.ingredients.is_empty() {
        return;
    }
    let have = ingredient_list(world, &state.ingredients);
    for &index in soups {
        let reaction = &chem.reactions[index as usize];
        let Some(soup) = reaction.def.soup.as_ref() else {
            continue;
        };
        if !ingredients_met(&soup.required_ingredients, &have) {
            continue;
        }
        let Some(mut holder) = world.get::<Reagents>(pot).map(|r| r.0.clone()) else {
            return;
        };
        let multiplier = reaction
            .required
            .iter()
            .map(|&(id, needed)| holder.amount(id) / needed)
            .fold(f32::INFINITY, f32::min);
        let result_id = reaction.results.first().map(|r| r.0);
        let result_before = result_id.map_or(0.0, |id| holder.amount(id));
        if !holder.start_reaction(chem, index) {
            continue;
        }
        world.entity_mut(pot).insert(Reagents(holder));
        on_reaction(world, chem, pot, index, multiplier);
        // `reaction.data["ingredients"]`, temperatures equalized.
        let temp = world.get::<Reagents>(pot).map_or(0.0, |r| r.0.chem_temp);
        let items = world
            .get::<SoupPot>(pot)
            .map(|p| p.ingredients.clone())
            .unwrap_or_default();
        let cached = items
            .iter()
            .map(|&i| {
                let volume = world.get_mut::<Reagents>(i).map_or(1.0, |mut r| {
                    r.0.chem_temp = temp;
                    if r.0.total_volume > 0.0 {
                        r.0.total_volume
                    } else {
                        1.0
                    }
                });
                (i, volume)
            })
            .collect();
        if let Some(mut p) = world.get_mut::<SoupPot>(pot) {
            p.soup = Some(ActiveSoup {
                reaction: index,
                cached,
                result_before,
            });
        }
        return;
    }
}

/// `soup/on_reaction` with a `resulting_food_path`: one food per set of ingredients (at most
/// the reaction's multiplier), the ingredients used up.
fn on_reaction(world: &mut World, chem: &Chemistry, pot: Entity, index: u32, multiplier: f32) {
    let Some(soup) = chem.reactions[index as usize].def.soup.clone() else {
        return;
    };
    let Some(food) = soup.resulting_food_path else {
        return;
    };
    let items = world
        .get::<SoupPot>(pot)
        .map(|p| p.ingredients.clone())
        .unwrap_or_default();
    // `is_type_in_list`, grouped by exact type.
    let mut tracked: Vec<(String, Vec<Entity>)> = Vec::new();
    for i in items {
        let path = path_of(world, i);
        if !soup
            .required_ingredients
            .iter()
            .any(|(r, _)| is_under(&path, r))
        {
            continue;
        }
        match tracked.iter_mut().find(|(p, _)| *p == path) {
            Some((_, list)) => list.push(i),
            None => tracked.push((path, vec![i])),
        }
    }
    let max = tracked.iter().map(|(_, l)| l.len()).min().unwrap_or(0);
    let count = (multiplier.floor().max(0.0) as usize).min(max);
    let Some(pos) = crate::chem::map_pos(world, pot) else {
        return;
    };
    for n in 0..count {
        for (_, list) in &tracked {
            let used = list[n];
            if let Some(mut p) = world.get_mut::<SoupPot>(pot) {
                p.ingredients.retain(|&x| x != used);
            }
            world.despawn(used);
        }
        if let Some(made) = crate::construction::spawn_obj(world, &food, pos)
            && let Some(mut look) = world.get_mut::<Appearance>(made)
        {
            look.pixel_offset[1] += 8;
        }
    }
}

fn step_soup(world: &mut World, chem: &Chemistry, pot: Entity, mut soup: ActiveSoup) {
    let running = world
        .get::<Reagents>(pot)
        .is_some_and(|r| r.0.running().any(|i| i == soup.reaction));
    if !running {
        finish(world, chem, pot, &soup);
        return;
    }
    let Some(def) = chem.reactions[soup.reaction as usize].def.soup.clone() else {
        return;
    };
    let items = world
        .get::<SoupPot>(pot)
        .map(|p| p.ingredients.clone())
        .unwrap_or_default();
    // An ingredient was removed: stop (it restarts if it is still right).
    if items.len() < soup.cached.len() || soup.cached.iter().any(|(i, _)| !items.contains(i)) {
        if let Some(mut r) = world.get_mut::<Reagents>(pot) {
            r.0.force_stop_reacting(chem);
        }
        finish(world, chem, pot, &soup);
        return;
    }
    let temp = world.get::<Reagents>(pot).map_or(0.0, |r| r.0.chem_temp);
    for &i in &items {
        if soup.cached.iter().any(|(c, _)| *c == i) {
            continue;
        }
        let volume = world.get_mut::<Reagents>(i).map_or(1.0, |mut r| {
            r.0.chem_temp = temp;
            r.0.total_volume.max(1.0)
        });
        soup.cached.push((i, volume));
    }
    let Some(mut holder) = world.get::<Reagents>(pot).map(|r| r.0.clone()) else {
        return;
    };
    for &(i, initial) in &soup.cached {
        if !transfer(world, chem, &def, i, &mut holder, Some((initial * 0.2).max(2.0))) {
            continue;
        }
        if holder.total_volume >= holder.maximum_volume * 0.95 {
            let name = name_of(world, pot);
            visible(
                world,
                pot,
                "",
                &format!("{name} начинает выкипать!"),
                Span::Warning,
            );
            for r in &mut holder.reagents {
                r.volume *= 0.5;
            }
            holder.update_total();
        }
    }
    world.entity_mut(pot).insert(Reagents(holder));
    if let Some(mut p) = world.get_mut::<SoupPot>(pot) {
        p.soup = Some(soup);
    }
    refresh_pot(world, pot);
}

/// `transfer_ingredient_reagents`: `amount` (all when `None`) of the ingredient's reagents into
/// the pot, minus the converted nutriment. False when it had nothing to give.
fn transfer(
    world: &mut World,
    chem: &Chemistry,
    def: &ss13_reagents::SoupDef,
    ingredient: Entity,
    into: &mut Holder,
    amount: Option<f32>,
) -> bool {
    if def.ingredient_reagent_multiplier <= 0.0 {
        return false;
    }
    let Some(mut pool) = world.get::<Reagents>(ingredient).map(|r| r.0.clone()) else {
        return false;
    };
    if pool.total_volume <= 0.0 {
        return false;
    }
    let amount = amount.unwrap_or(pool.total_volume);
    let remove = amount * def.percentage_of_nutriment_converted;
    for path in [NUTRIMENT, VITAMIN] {
        if let Some(id) = chem.id(path) {
            pool.remove_reagent(id, remove);
        }
    }
    pool.trans_to(
        chem,
        into,
        amount,
        def.ingredient_reagent_multiplier,
        None,
        true,
        false,
    );
    world.entity_mut(ingredient).insert(Reagents(pool));
    true
}

/// `reaction_finish` -> `clean_up`.
fn finish(world: &mut World, chem: &Chemistry, pot: Entity, soup: &ActiveSoup) {
    if let Some(mut p) = world.get_mut::<SoupPot>(pot) {
        p.soup = None;
    }
    let reaction = &chem.reactions[soup.reaction as usize];
    let Some(def) = reaction.def.soup.clone() else {
        return;
    };
    let made = reaction.results.first().map_or(0.0, |&(id, _)| {
        world.get::<Reagents>(pot).map_or(0.0, |r| r.0.amount(id)) - soup.result_before
    });
    let items = world
        .get::<SoupPot>(pot)
        .map(|p| p.ingredients.clone())
        .unwrap_or_default();
    if made > 0.0 {
        let Some(mut holder) = world.get::<Reagents>(pot).map(|r| r.0.clone()) else {
            return;
        };
        for i in items {
            let indestructible = world
                .get::<DmItem>(i)
                .is_some_and(|d| d.resistance_flags & crate::obj_damage::INDESTRUCTIBLE != 0);
            if indestructible {
                continue;
            }
            let path = path_of(world, i);
            let required = def
                .required_ingredients
                .iter()
                .any(|(r, _)| is_under(&path, r));
            if world.get::<Reagents>(i).is_none() && !required {
                crate::cooking::fry(world, i, LEFTOVER_FRY_DS);
                continue;
            }
            if let Some(mut p) = world.get_mut::<SoupPot>(pot) {
                p.ingredients.retain(|&x| x != i);
            }
            transfer(world, chem, &def, i, &mut holder, None);
            world.despawn(i);
        }
        world.entity_mut(pot).insert(Reagents(holder));
    }
    dump(world, pot);
    refresh_pot(world, pot);
}

/// `dump_ingredients(get_turf(pot), y_offset = 8)`.
fn dump(world: &mut World, pot: Entity) {
    let Some(pos) = crate::chem::map_pos(world, pot) else {
        return;
    };
    let items = world
        .get_mut::<SoupPot>(pot)
        .map(|mut p| std::mem::take(&mut p.ingredients))
        .unwrap_or_default();
    for i in items {
        crate::storage::take_out(world, i, pos);
        if let Some(mut look) = world.get_mut::<Appearance>(i) {
            look.pixel_offset[1] += 8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(v: &[(&str, f32)]) -> Vec<(String, f32)> {
        v.iter().map(|(p, n)| ((*p).to_owned(), *n)).collect()
    }

    #[test]
    fn ingredients_count_by_type_and_parent() {
        let meatball_soup = list(&[
            ("/obj/item/food/meatball", 1.0),
            ("/obj/item/food/grown/carrotlike/carrot", 1.0),
            ("/obj/item/food/grown/potato", 1.0),
        ]);
        let pot = list(&[
            ("/obj/item/food/meatball/chicken", 1.0),
            ("/obj/item/food/grown/carrotlike/carrot", 1.0),
        ]);
        assert!(!ingredients_met(&meatball_soup, &pot));
        let mut pot = pot;
        pot.push(("/obj/item/food/grown/potato/sweet".into(), 1.0));
        // A subtype counts for its parent; extra ingredients do not hurt.
        pot.push(("/obj/item/food/grown/potato".into(), 1.0));
        assert!(ingredients_met(&meatball_soup, &pot));
        // Two of one kind are needed: one is not enough, a stack of two is.
        let two = list(&[("/obj/item/stack/sheet/mineral/plasma", 2.0)]);
        assert!(!ingredients_met(
            &two,
            &list(&[("/obj/item/stack/sheet/mineral/plasma", 1.0)])
        ));
        assert!(ingredients_met(
            &two,
            &list(&[("/obj/item/stack/sheet/mineral/plasma", 2.0)])
        ));
    }
}
