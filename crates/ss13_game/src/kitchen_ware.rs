//! Kitchenware (server): plates (`code/modules/food_and_drinks/equipment/plate.dm`), tongs
//! (`kitchen_utensils.dm`), pizza boxes (`food_and_drinks/pizzabox.dm`) and the taps of
//! reagent tanks (`/obj/structure/reagent_dispensers` `openable`, the vat of cooking oil among
//! them; `code/modules/reagents/reagent_dispenser.dm`).
//!
//! - A plate ([`Plate`]) takes food up to its `biggest_w_class` and `max_items`, drawn where it
//!   was clicked (`max_x_offset`, `max_height_offset`); the empty other hand takes the last one
//!   back. Used on a mob it feeds the first food (`pre_attack`). Thrown and not caught, a
//!   fragile plate shatters (`/datum/element/can_shatter`): the food falls out, five ceramic
//!   shards, the plate is gone. Iron plates and oven trays do not break.
//! - Tongs ([`Tongs`]) pick up food of `WEIGHT_CLASS_NORMAL` or smaller (drawn on them) from up
//!   to 2 tiles (`reach = 2`) and feed it to a mob like the food itself; used in hand they drop
//!   it, or clack (2 s cooldown); dropped, the food falls.
//! - A pizza box ([`PizzaBox`]) opens and closes in hand (`pizzabox`, `_open`, `_messy`). Open:
//!   a pizza goes in, the empty other hand takes it out. Closed boxes stack (five or more: `10%`
//!   per box that the pile falls apart), the empty other hand takes the top box; a pen writes
//!   on the top box's tag (30 characters). A pile falls apart when thrown (2+ boxes, `20%` per
//!   box) or swung at someone (3+ boxes, `25%` per box): every box steps aside, half of them
//!   drop their pizza. The mapped boxes come with their pizza; the infinite box makes the
//!   opener's favourite pizza every time it is used.
//! - An `openable` tank (water, foam, fuel, keg, kvass, cooking oil) has a tap a wrench opens
//!   and closes; open, it spills 10 u onto its turf (`tank_leak`) each time it is opened or
//!   moved.
// shortcut: plates do not grow a weight class from what they hold; tongs reach over anything
// within 2 tiles (no `CanReach` path) and draw the food at full size; pizza boxes: no
// folding into cardboard (right click in hand), no pre-sliced pizza or slices from an open
// box, no pizza bomb (with the syndicate bombs: no `bombcore` yet); the infinite box's
// favourite is per mob, not per ckey, and ignores pineapple quirks and botanists; tanks do not
// turn into plumbing tanks with iron.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmFood, DmItem, DmObj, DmReagents, DmTool, DmWeight};
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, Dir, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::UiSystems;

use crate::chem::{Chem, Reagents, map_pos, name_of, tell_mob, visible};
use crate::click::{RangedClick, ResolveClicks};
use crate::combat::{CombatMode, CombatRng};
use crate::inventory::{CarriedBy, Inventory};
use crate::sound::{PlaySound, playsound};
use crate::storage::{Stored, StoredIn, put_inside, take_out};
use crate::tools::Actor;

pub const PLATE_ROOT: &str = "/obj/item/plate";
pub const PLATE_SHARD: &str = "/obj/item/plate_shard";
pub const TONGS: &str = "/obj/item/kitchen/tongs";
pub const PIZZABOX_ROOT: &str = "/obj/item/pizzabox";
const PIZZABOX_INFINITE: &str = "/obj/item/pizzabox/infinite";
const PIZZA_ROOT: &str = "/obj/item/food/pizza";
const DISPENSER_ROOT: &str = "/obj/structure/reagent_dispensers";
/// `can_shatter` defaults: five shards, `ceramic_break.ogg`.
const SHARDS: u32 = 5;
const SHATTER_SOUND: &str = "sound/items/ceramic_break.ogg";
/// `WEIGHT_CLASS_*`.
const WEIGHT_CLASS_SMALL: u8 = 2;
const WEIGHT_CLASS_NORMAL: u8 = 3;
const WEIGHT_CLASS_BULKY: u8 = 4;
/// Tongs: `reach`, `clack_delay`, the closed look `min(0.5 s, clack_delay / 2)`.
const TONGS_REACH: u32 = 2;
const CLACK_SECS: f64 = 2.0;
const CLACK_CLOSED_SECS: f64 = 0.5;
const CLACK_SOUND: &str = "sound/items/handling/component_drop.ogg";
/// Tongs' `held_food` overlay offset (`pixel_w`, `pixel_z`).
const TONGS_FOOD_OFFSET: [i16; 2] = [6, 6];
/// `boxtag` length (`tgui_input_text(max_length = 30)`).
const BOXTAG_MAX: u32 = 30;
/// `SILENCED_SOUND_EXTRARANGE`.
const SILENCED_SOUND_EXTRARANGE: f32 = -11.0;
/// `amount_to_leak`.
const LEAK_AMOUNT: f32 = 10.0;

/// A plate type's `max_items`, `max_x_offset`, `max_height_offset`, `fragile`,
/// `biggest_w_class`. Most specific type first.
struct PlateSpec {
    root: &'static str,
    max_items: usize,
    max_x: i16,
    max_height: i16,
    fragile: bool,
    biggest: u8,
}

const PLATES: [PlateSpec; 5] = [
    PlateSpec {
        root: "/obj/item/plate/oven_tray",
        max_items: 6,
        max_x: 4,
        max_height: 5,
        fragile: false,
        biggest: WEIGHT_CLASS_BULKY,
    },
    PlateSpec {
        root: "/obj/item/plate/large",
        max_items: 12,
        max_x: 8,
        max_height: 12,
        fragile: true,
        biggest: WEIGHT_CLASS_BULKY,
    },
    PlateSpec {
        root: "/obj/item/plate/small",
        max_items: 4,
        max_x: 4,
        max_height: 5,
        fragile: true,
        biggest: WEIGHT_CLASS_SMALL,
    },
    PlateSpec {
        root: "/obj/item/plate/iron",
        max_items: 8,
        max_x: 4,
        max_height: 5,
        fragile: false,
        biggest: WEIGHT_CLASS_NORMAL,
    },
    PlateSpec {
        root: PLATE_ROOT,
        max_items: 8,
        max_x: 4,
        max_height: 5,
        fragile: true,
        biggest: WEIGHT_CLASS_NORMAL,
    },
];

fn plate_spec(path: &str) -> Option<&'static PlateSpec> {
    PLATES.iter().find(|s| is_under(path, s.root))
}

/// The `pizza` of the boxes that come with one (`/obj/item/pizzabox/bomb/armed`'s meat pizza
/// goes with the bomb).
const BOXED_PIZZAS: [(&str, &str); 6] = [
    (
        "/obj/item/pizzabox/margherita/robo",
        "/obj/item/food/pizza/margherita/robo",
    ),
    (
        "/obj/item/pizzabox/margherita",
        "/obj/item/food/pizza/margherita",
    ),
    (
        "/obj/item/pizzabox/vegetable",
        "/obj/item/food/pizza/vegetable",
    ),
    (
        "/obj/item/pizzabox/mushroom",
        "/obj/item/food/pizza/mushroom",
    ),
    ("/obj/item/pizzabox/meat", "/obj/item/food/pizza/meat"),
    (
        "/obj/item/pizzabox/pineapple",
        "/obj/item/food/pizza/pineapple",
    ),
];

/// `/obj/item/pizzabox/infinite::pizza_types`, weighted.
const INFINITE_PIZZAS: [(&str, u32); 8] = [
    ("/obj/item/food/pizza/meat", 10),
    ("/obj/item/food/pizza/mushroom", 10),
    ("/obj/item/food/pizza/margherita", 10),
    ("/obj/item/food/pizza/sassysage", 8),
    ("/obj/item/food/pizza/vegetable", 8),
    ("/obj/item/food/pizza/pineapple", 5),
    ("/obj/item/food/pizza/donkpocket", 3),
    ("/obj/item/food/pizza/dank", 1),
];

/// `openable` tanks.
const OPENABLE_TANKS: [&str; 6] = [
    "/obj/structure/reagent_dispensers/watertank",
    "/obj/structure/reagent_dispensers/foamtank",
    "/obj/structure/reagent_dispensers/fueltank",
    "/obj/structure/reagent_dispensers/keg",
    "/obj/structure/reagent_dispensers/kvasstank",
    "/obj/structure/reagent_dispensers/cooking_oil",
];

/// A plate's live state.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plate;

/// On food on a plate: where it is drawn (`pixel_w`, `pixel_z`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnPlate(pub [i16; 2]);

/// `/obj/item/kitchen/tongs`: `tonged`, the clack cooldown and the closed look's end.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Tongs {
    pub tonged: Option<Entity>,
    clack_ready: f64,
    reopen_at: Option<f64>,
}

/// `/obj/item/pizzabox`: `open`, `pizza`, `boxes` (stacked on it, bottom first), `boxtag`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PizzaBox {
    pub open: bool,
    pub pizza: Option<Entity>,
    pub boxes: Vec<Entity>,
    pub boxtag: String,
}

/// An open tap (`leaking`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapOpen;

/// A boxtag question waiting for its answer: the box and the pen.
#[derive(Component, Clone, Copy, Debug)]
struct BoxtagAsk {
    pizzabox: Entity,
    pen: Entity,
}

/// The infinite box's `pizza_preferences`, per mob.
#[derive(Resource, Default)]
struct PizzaPreferences(std::collections::HashMap<Entity, &'static str>);

pub struct KitchenWarePlugin;

impl Plugin for KitchenWarePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 3309, |w, c| {
            attack_self(w, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::OtherHand, 499, |w, c| {
            other_hand(w, c.client, c.mob, c.held, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 1809, |w, c| {
            c.held
                .is_some_and(|h| held_on_target(w, c.client, c.mob, h, c.target, c.modifiers))
        });
        app.init_resource::<PizzaPreferences>()
            .add_observer(add_state)
            .add_systems(
                Update,
                (
                    ranged_clicks.after(ResolveClicks),
                    tongs_tick,
                    refresh_plates,
                    leak_on_move.after(ss13_world::WorldSystems::Movement),
                    boxtag_answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                ),
            );
    }
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .or_else(|| world.get::<DmObj>(e).map(|o| o.path.clone()))
        .unwrap_or_default()
}

fn w_class(world: &World, item: Entity) -> u8 {
    world
        .get::<DmWeight>(item)
        .copied()
        .unwrap_or_default()
        .w_class
}

fn contents(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(e)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

fn sound_at(world: &mut World, e: Entity, file: &str, vol: f32, vary: bool) {
    if let Some(pos) = map_pos(world, e) {
        playsound(world, PlaySound::new(pos, file, vol, vary));
    }
}

/// `new path(container)`: built on demand (`construction::spawn_detached`), replicated once
/// it comes out.
fn spawn_into(world: &mut World, path: &str, container: Entity) -> Option<Entity> {
    let item = crate::construction::spawn_detached(world, path)?;
    world.entity_mut(item).insert(ss13_net::server::Networked);
    put_inside(world, item, container);
    Some(item)
}

fn give(world: &mut World, mob: Entity, item: Entity) {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    crate::storage::put_in_hands(world, mob, &inventory, item);
}

fn add_state(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if plate_spec(&item.path).is_some() {
        commands.entity(e).try_insert_if_new(Plate);
    } else if is_under(&item.path, TONGS) {
        commands.entity(e).try_insert_if_new(Tongs::default());
    } else if is_under(&item.path, PIZZABOX_ROOT) {
        commands.queue(move |world: &mut World| init_pizzabox(world, e));
    }
}

/// What a held item feeds with: the food in tongs, or the first food on a plate.
pub fn held_food(world: &World, item: Entity) -> Option<Entity> {
    if let Some(tongs) = world.get::<Tongs>(item) {
        return tongs.tonged;
    }
    if world.get::<Plate>(item).is_some() {
        return contents(world, item).first().copied();
    }
    None
}

// --- Plates -----------------------------------------------------------------------------------

/// `/obj/item/plate/item_interaction`: food onto the plate at the clicked point.
fn add_to_plate(
    world: &mut World,
    mob: Entity,
    food: Entity,
    plate: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let Some(spec) = plate_spec(&path_of(world, plate)) else {
        return false;
    };
    if world.get::<DmFood>(food).is_none() {
        return false;
    }
    if w_class(world, food) > spec.biggest {
        crate::balloon::balloon_alert(world, mob, plate, "слишком большое!");
        return true;
    }
    if contents(world, plate).len() >= spec.max_items {
        crate::balloon::balloon_alert(world, mob, plate, "не помещается!");
        return true;
    }
    let [x, y] = modifiers.pixel.map_or([0, 0], |p| p.map(i16::from));
    // `ICON_X - 16`, `ICON_Y + placement_offset (-15)`.
    let offset = [
        x.clamp(-spec.max_x, spec.max_x),
        (y + 1).min(spec.max_height),
    ];
    let (what, dish) = (name_of(world, food), name_of(world, plate));
    put_inside(world, food, plate);
    world.entity_mut(food).insert(OnPlate(offset));
    tell_mob(
        world,
        mob,
        Span::Notice,
        &format!("Вы кладёте {what} на {dish}."),
    );
    refresh_plate(world, plate);
    true
}

/// The plate's `vis_contents`: its food at their offsets.
fn refresh_plate(world: &mut World, plate: Entity) {
    let overlays: Vec<Overlay> = contents(world, plate)
        .into_iter()
        .filter_map(|food| {
            let state = world.get::<Appearance>(food)?.icon_state;
            let mut overlay = Overlay::new(state);
            overlay.pixel_offset = world.get::<OnPlate>(food).map_or([0, 0], |o| o.0);
            Some(overlay)
        })
        .collect();
    if let Some(mut look) = world.get_mut::<Appearance>(plate)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

/// Plates whose contents changed by any path (eaten, baked, taken).
// Cost: per tick, the plates whose contents changed and the empty ones still drawn with food.
fn refresh_plates(world: &mut World) {
    let mut changed = world.query_filtered::<Entity, (With<Plate>, Changed<Stored>)>();
    let mut plates: Vec<Entity> = changed.iter(world).collect();
    let mut empty = world.query_filtered::<(Entity, &Appearance), (With<Plate>, Without<Stored>)>();
    plates.extend(
        empty
            .iter(world)
            .filter(|(_, a)| !a.overlays.is_empty())
            .map(|(e, _)| e),
    );
    for plate in plates {
        refresh_plate(world, plate);
    }
}

/// `can_shatter/shatter` of a fragile plate that was not caught.
pub fn plate_thrown(world: &mut World, plate: Entity) -> bool {
    if world.get::<Plate>(plate).is_none()
        || !plate_spec(&path_of(world, plate)).is_some_and(|s| s.fragile)
        || world.get::<CarriedBy>(plate).is_some()
        || world.get::<StoredIn>(plate).is_some()
    {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(plate) else {
        return false;
    };
    for food in contents(world, plate) {
        world.entity_mut(food).remove::<OnPlate>();
        take_out(world, food, pos);
        let (x, y) = {
            let mut rng = world.resource_mut::<CombatRng>();
            (rng.range(0, 32) as i16 - 16, rng.range(0, 32) as i16 - 16)
        };
        if let Some(mut look) = world.get_mut::<Appearance>(food) {
            look.pixel_offset = [x, y];
        }
    }
    for _ in 0..SHARDS {
        if let Some(shard) = crate::construction::spawn_obj(world, PLATE_SHARD, pos) {
            let (x, y) = {
                let mut rng = world.resource_mut::<CombatRng>();
                (rng.range(0, 12) as i16 - 6, rng.range(0, 12) as i16 - 6)
            };
            if let Some(mut look) = world.get_mut::<Appearance>(shard) {
                look.pixel_offset = [x, y];
            }
        }
    }
    playsound(world, PlaySound::new(pos, SHATTER_SOUND, 60.0, true));
    world.despawn(plate);
    true
}

// --- Tongs ------------------------------------------------------------------------------------

fn refresh_tongs(world: &mut World, tongs: Entity) {
    let state = world
        .get::<Tongs>(tongs)
        .and_then(|t| t.tonged)
        .and_then(|f| world.get::<Appearance>(f))
        .map(|a| a.icon_state);
    let overlays: Vec<Overlay> = state
        .map(|s| {
            let mut overlay = Overlay::new(s);
            overlay.pixel_offset = TONGS_FOOD_OFFSET;
            overlay
        })
        .into_iter()
        .collect();
    if let Some(mut look) = world.get_mut::<Appearance>(tongs)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

/// `drop_tonged`: the food falls where the tongs are.
fn drop_tonged(world: &mut World, tongs: Entity) {
    let Some(food) = world
        .get_mut::<Tongs>(tongs)
        .and_then(|mut t| t.tonged.take())
    else {
        return;
    };
    if world.get_entity(food).is_ok()
        && world.get::<StoredIn>(food).is_some_and(|s| s.0 == tongs)
        && let Some(pos) = map_pos(world, tongs)
    {
        take_out(world, food, pos);
        let what = name_of(world, food);
        let clients = crate::chem::clients_near(world, pos, None);
        if !clients.is_empty() {
            world.write_message(ss13_net::chat::server::SendChat {
                clients,
                message: crate::chem::line(Span::Notice, &format!("{what} падает на пол!")),
            });
        }
    }
    refresh_tongs(world, tongs);
}

/// `click_clack`: the sound, the closed tongs for half a second.
fn clack(world: &mut World, tongs: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    match world.get_mut::<Tongs>(tongs) {
        Some(mut t) if now >= t.clack_ready => {
            t.clack_ready = now + CLACK_SECS;
            t.reopen_at = Some(now + CLACK_CLOSED_SECS.min(CLACK_SECS / 2.0));
        }
        _ => return,
    }
    sound_at(world, tongs, CLACK_SOUND, 100.0, false);
    crate::obj_damage::set_icon_state(world, tongs, "tongs_closed");
}

/// Tongs used on `target` (adjacent or within reach). True when taken.
fn tongs_on(world: &mut World, client: Entity, mob: Entity, tongs: Entity, target: Entity) -> bool {
    let Some(state) = world.get::<Tongs>(tongs).copied() else {
        return false;
    };
    // `tonged.force <= 0`: the food is used on the target, as if held.
    if let Some(food) = state.tonged
        && world.get::<DmItem>(food).is_some_and(|i| i.force <= 0.0)
    {
        if world.get::<crate::chem::MobReagents>(target).is_some() {
            if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
                return false;
            }
            let actor = Actor {
                client,
                mob,
                item: Some(tongs),
            };
            crate::food::try_to_eat(world, actor, target);
            return true;
        }
        if world.get::<Plate>(target).is_some() {
            add_to_plate(world, mob, food, target, ClickModifiers::default());
            if world.get::<StoredIn>(food).is_some_and(|s| s.0 == target) {
                if let Some(mut t) = world.get_mut::<Tongs>(tongs) {
                    t.tonged = None;
                }
                refresh_tongs(world, tongs);
            }
            return true;
        }
        return false;
    }
    if world.get::<crate::health::Health>(target).is_some() {
        clack(world, tongs);
        return false;
    }
    if state.tonged.is_some()
        || world.get::<DmFood>(target).is_none()
        || w_class(world, target) > WEIGHT_CLASS_NORMAL
        || world.get::<TilePos>(target).is_none()
    {
        return false;
    }
    put_inside(world, target, tongs);
    if let Some(mut t) = world.get_mut::<Tongs>(tongs) {
        t.tonged = Some(target);
    }
    refresh_tongs(world, tongs);
    true
}

/// `attack_self`: drop the food, or clack.
fn tongs_attack_self(world: &mut World, mob: Entity, tongs: Entity) -> bool {
    let Some(state) = world.get::<Tongs>(tongs).copied() else {
        return false;
    };
    if state.tonged.is_some() {
        drop_tonged(world, tongs);
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < state.clack_ready {
        return true;
    }
    let (user, name) = (name_of(world, mob), name_of(world, tongs));
    visible(
        world,
        mob,
        &format!("Вы щёлкаете {name}, как краб. Клац-клац!"),
        &format!("{user} щёлкает {name}, как краб. Клац-клац!"),
        Span::Notice,
    );
    clack(world, tongs);
    true
}

/// Tongs out of the hands drop their food (`dropped`), the clack reopens them (`clack`), food
/// that left them otherwise is forgotten (`Exited`).
fn tongs_tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let all: Vec<(Entity, Tongs)> = world
        .query::<(Entity, &Tongs)>()
        .iter(world)
        .filter(|(_, t)| t.tonged.is_some() || t.reopen_at.is_some())
        .map(|(e, t)| (e, *t))
        .collect();
    for (tongs, state) in all {
        if let Some(at) = state.reopen_at
            && now >= at
        {
            if let Some(mut t) = world.get_mut::<Tongs>(tongs) {
                t.reopen_at = None;
            }
            sound_at(world, tongs, CLACK_SOUND, 100.0, false);
            crate::obj_damage::set_icon_state(world, tongs, "tongs");
        }
        let Some(food) = state.tonged else {
            continue;
        };
        let inside = world.get_entity(food).is_ok()
            && world.get::<StoredIn>(food).is_some_and(|s| s.0 == tongs);
        if !inside {
            if let Some(mut t) = world.get_mut::<Tongs>(tongs) {
                t.tonged = None;
            }
            refresh_tongs(world, tongs);
            continue;
        }
        let in_hand = world.get::<CarriedBy>(tongs).is_some_and(|c| {
            world
                .get::<Inventory>(c.0)
                .is_some_and(|i| i.hands.contains(&Some(tongs)))
        });
        if !in_hand {
            drop_tonged(world, tongs);
        }
    }
}

/// `reach = 2`: tongs clicks up to two tiles away.
fn ranged_clicks(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(tongs) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&h| world.get::<Tongs>(h).is_some())
        else {
            continue;
        };
        if click
            .mob_pos
            .chebyshev(click.target_pos)
            .is_some_and(|d| d <= TONGS_REACH)
            && world.get::<TilePos>(click.target) == Some(&click.target_pos)
        {
            tongs_on(world, click.client, click.mob, tongs, click.target);
        }
    }
}

// --- Pizza boxes ------------------------------------------------------------------------------

fn init_pizzabox(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() || world.get::<PizzaBox>(e).is_some() {
        return;
    }
    let path = path_of(world, e);
    world.entity_mut(e).insert(PizzaBox::default());
    let pizza = BOXED_PIZZAS
        .iter()
        .find(|(root, _)| is_under(&path, root))
        .and_then(|&(_, pizza)| spawn_into(world, pizza, e));
    if let Some(mut b) = world.get_mut::<PizzaBox>(e) {
        b.pizza = pizza;
    }
    refresh_pizzabox(world, e);
}

/// `base_icon_state` (the type's `icon_state`).
fn base_state(world: &World, e: Entity) -> String {
    world
        .get::<DmAtom>(e)
        .map(|a| a.icon_state.clone())
        .unwrap_or_default()
}

fn icon_id(world: &World, e: Entity, state: &str) -> Option<ss13_shared::IconStateId> {
    let icon = world.get::<DmAtom>(e)?.icon.clone();
    world
        .get_resource::<ss13_mapload::IconIndex>()?
        .get(&icon, state)
}

/// `update_icon_state`, `update_overlays`: the open box's pizza, the closed pile and the top
/// box's tag.
fn refresh_pizzabox(world: &mut World, e: Entity) {
    let Some(b) = world.get::<PizzaBox>(e).cloned() else {
        return;
    };
    let base = base_state(world, e);
    let state = if !b.open {
        base
    } else if b.pizza.is_some() {
        format!("{base}_messy")
    } else {
        format!("{base}_open")
    };
    crate::obj_damage::set_icon_state(world, e, &state);
    let mut overlays = Vec::new();
    if b.open {
        if let Some(look) = b.pizza.and_then(|p| world.get::<Appearance>(p)) {
            let mut overlay = Overlay::new(look.icon_state);
            overlay.pixel_offset = [0, -2];
            overlays.push(overlay);
        }
    } else {
        let mut offset = 0;
        for &stacked in &b.boxes {
            offset += 3;
            let Some(state) = icon_id(world, stacked, &base_state(world, stacked)) else {
                continue;
            };
            let mut overlay = Overlay::new(state);
            overlay.pixel_offset = [0, offset];
            overlays.push(overlay);
        }
        let top = b.boxes.last().copied().unwrap_or(e);
        let tagged = world
            .get::<PizzaBox>(top)
            .is_some_and(|t| !t.boxtag.is_empty())
            || (top == e && is_under(&path_of(world, e), PIZZABOX_INFINITE));
        if tagged && let Some(tag) = icon_id(world, e, "pizzabox_tag") {
            let mut overlay = Overlay::new(tag);
            overlay.pixel_offset = [0, offset];
            overlays.push(overlay);
        }
    }
    if let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
    if let Some(holder) = world.get::<CarriedBy>(e).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        inventory.set_changed();
    }
}

/// `attack_self`: open or close a single box; the infinite box first makes the user's pizza.
fn pizzabox_attack_self(world: &mut World, mob: Entity, e: Entity) -> bool {
    let Some(b) = world.get::<PizzaBox>(e).cloned() else {
        return false;
    };
    if is_under(&path_of(world, e), PIZZABOX_INFINITE) && world.get::<Inventory>(mob).is_some() {
        attune_pizza(world, mob, e);
        tell_mob(
            world,
            mob,
            Span::Notice,
            "В коробке тут же появляется ещё одна пицца, что за чёрт?",
        );
    }
    if !b.boxes.is_empty() {
        return true;
    }
    if let Some(mut b) = world.get_mut::<PizzaBox>(e) {
        b.open = !b.open;
    }
    refresh_pizzabox(world, e);
    true
}

/// `/obj/item/pizzabox/infinite/attune_pizza`: the mob's favourite pizza, a new one unless it is
/// already in the box.
fn attune_pizza(world: &mut World, mob: Entity, e: Entity) {
    let favourite = match world.resource::<PizzaPreferences>().0.get(&mob) {
        Some(&p) => p,
        None => {
            let total: u32 = INFINITE_PIZZAS.iter().map(|&(_, w)| w).sum();
            let mut roll = world.resource_mut::<CombatRng>().range(1, total);
            let mut pick = INFINITE_PIZZAS[0].0;
            for &(path, weight) in &INFINITE_PIZZAS {
                if roll <= weight {
                    pick = path;
                    break;
                }
                roll -= weight;
            }
            world.resource_mut::<PizzaPreferences>().0.insert(mob, pick);
            pick
        }
    };
    let current = world.get::<PizzaBox>(e).and_then(|b| b.pizza);
    if let Some(pizza) = current {
        if path_of(world, pizza) == favourite {
            return;
        }
        world.despawn(pizza);
    }
    let pizza = spawn_into(world, favourite, e);
    if let Some(mut b) = world.get_mut::<PizzaBox>(e) {
        b.pizza = pizza;
    }
    refresh_pizzabox(world, e);
}

/// `attack_hand` with the box in the other hand: the pizza or the top box out.
fn pizzabox_take(world: &mut World, mob: Entity, e: Entity) -> bool {
    let Some(b) = world.get::<PizzaBox>(e).cloned() else {
        return false;
    };
    if b.open {
        if let Some(pizza) = b.pizza {
            if let Some(mut b) = world.get_mut::<PizzaBox>(e) {
                b.pizza = None;
            }
            give(world, mob, pizza);
            refresh_pizzabox(world, e);
        }
        return true;
    }
    let Some(&top) = b.boxes.last() else {
        return false;
    };
    if let Some(mut b) = world.get_mut::<PizzaBox>(e) {
        b.boxes.pop();
    }
    give(world, mob, top);
    refresh_pizzabox(world, top);
    refresh_pizzabox(world, e);
    true
}

/// `/obj/item/pizzabox/item_interaction`: stacking, a pizza in, a pen on the tag.
fn pizzabox_item(world: &mut World, client: Entity, mob: Entity, held: Entity, e: Entity) -> bool {
    let Some(b) = world.get::<PizzaBox>(e).cloned() else {
        return false;
    };
    if let Some(newbox) = world.get::<PizzaBox>(held).cloned() {
        if b.open || newbox.open {
            crate::balloon::balloon_alert(world, mob, e, "сначала закройте!");
            return true;
        }
        put_inside(world, held, e);
        let mut add = vec![held];
        add.extend(newbox.boxes.iter().copied());
        for &moved in &newbox.boxes {
            put_inside(world, moved, e);
        }
        if let Some(mut nb) = world.get_mut::<PizzaBox>(held) {
            nb.boxes.clear();
        }
        let count = {
            let mut b = world.get_mut::<PizzaBox>(e).expect("checked above");
            b.boxes.extend(add);
            b.boxes.len()
        };
        refresh_pizzabox(world, held);
        refresh_pizzabox(world, e);
        if count >= 5 {
            if world
                .resource_mut::<CombatRng>()
                .chance(10.0 * count as f32)
            {
                crate::balloon::balloon_alert_to_viewers(world, mob, "упс!", None, &[]);
                disperse_pizzas(world, e);
            } else {
                crate::balloon::balloon_alert(world, mob, e, "выглядит неустойчиво...");
            }
        }
        return true;
    }
    let held_path = path_of(world, held);
    if is_under(&held_path, PIZZA_ROOT) {
        if !b.open {
            return false;
        }
        if b.pizza.is_some() {
            crate::balloon::balloon_alert(world, mob, e, "уже занято!");
            return true;
        }
        put_inside(world, held, e);
        if let Some(mut b) = world.get_mut::<PizzaBox>(e) {
            b.pizza = Some(held);
        }
        refresh_pizzabox(world, e);
        return true;
    }
    if is_under(&held_path, "/obj/item/pen") || is_under(&held_path, "/obj/item/toy/crayon") {
        if b.open {
            return true;
        }
        let top = b.boxes.last().copied().unwrap_or(e);
        let name = name_of(world, top);
        let prompt = ask_world(
            world,
            Ask::new(
                client,
                mob,
                &name,
                format!("Напишите на бирке {name}:"),
                PromptKind::Text {
                    default: String::new(),
                    max_length: BOXTAG_MAX,
                    multiline: false,
                },
            ),
        );
        world.entity_mut(prompt).insert(BoxtagAsk {
            pizzabox: e,
            pen: held,
        });
        return true;
    }
    false
}

/// The boxtag text, at the moment of effect: the pen still in hand, the box closed and in
/// reach.
fn boxtag_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, mob, answer) in answers {
        let Some(&BoxtagAsk { pizzabox, pen }) = world.get::<BoxtagAsk>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Text(text)) = answer else {
            continue;
        };
        let holds_pen = world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.hands.contains(&Some(pen)));
        let reach = world.get::<CarriedBy>(pizzabox).is_some_and(|c| c.0 == mob)
            || crate::storage::in_reach(world, mob, pizzabox);
        let Some(b) = world.get::<PizzaBox>(pizzabox).cloned() else {
            continue;
        };
        if !holds_pen || !reach || b.open {
            continue;
        }
        let top = b.boxes.last().copied().unwrap_or(pizzabox);
        let text: String = text.trim().chars().take(BOXTAG_MAX as usize).collect();
        if let Some(mut t) = world.get_mut::<PizzaBox>(top) {
            t.boxtag.push_str(&text);
        }
        crate::balloon::balloon_alert(world, mob, pizzabox, "пишем на бирке...");
        refresh_pizzabox(world, pizzabox);
    }
}

/// `disperse_pizzas`: every stacked box steps aside, half of them losing their pizza.
pub fn disperse_pizzas(world: &mut World, e: Entity) {
    let Some(boxes) = world
        .get_mut::<PizzaBox>(e)
        .map(|mut b| std::mem::take(&mut b.boxes))
    else {
        return;
    };
    let Some(pos) = map_pos(world, e) else {
        return;
    };
    let clients = crate::chem::clients_near(world, pos, None);
    if !clients.is_empty() {
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: crate::chem::line(Span::Warning, "Пиццы падают повсюду!"),
        });
    }
    for stacked in boxes {
        take_out(world, stacked, pos);
        let dir = random_dir(world);
        let _ = crate::pull::step_once(world, stacked, dir, None);
        let pizza = world.get::<PizzaBox>(stacked).and_then(|b| b.pizza);
        let infinite = is_under(&path_of(world, stacked), PIZZABOX_INFINITE);
        if let Some(pizza) = pizza
            && !infinite
            && world.resource_mut::<CombatRng>().chance(50.0)
        {
            let at = world.get::<TilePos>(stacked).copied().unwrap_or(pos);
            if let Some(mut b) = world.get_mut::<PizzaBox>(stacked) {
                b.open = true;
                b.pizza = None;
            }
            take_out(world, pizza, at);
            let dir = random_dir(world);
            let _ = crate::pull::step_once(world, pizza, dir, None);
        }
        refresh_pizzabox(world, stacked);
    }
    refresh_pizzabox(world, e);
}

fn random_dir(world: &mut World) -> Dir {
    let i = world.resource_mut::<CombatRng>().range(0, 3) as usize;
    Dir::CARDINALS[i]
}

/// `throw_impact` of a pile: 2+ boxes, `20%` per box.
pub fn pizzabox_thrown(world: &mut World, e: Entity) {
    let n = world.get::<PizzaBox>(e).map_or(0, |b| b.boxes.len());
    if n >= 2 && world.resource_mut::<CombatRng>().chance(20.0 * n as f32) {
        disperse_pizzas(world, e);
    }
}

// --- Tanks ------------------------------------------------------------------------------------

/// `wrench_act` of an `openable` tank: the tap opens or closes, then `tank_leak`.
fn tap(world: &mut World, mob: Entity, held: Entity, tank: Entity) -> bool {
    let path = path_of(world, tank);
    if !OPENABLE_TANKS.iter().any(|root| is_under(&path, root))
        || world
            .get::<DmTool>(held)
            .is_none_or(|t| t.behaviour != "wrench")
    {
        return false;
    }
    let open = world.get::<TapOpen>(tank).is_none();
    if open {
        world.entity_mut(tank).insert(TapOpen);
    } else {
        world.entity_mut(tank).remove::<TapOpen>();
    }
    let text = if open {
        "кран открыт"
    } else {
        "кран закрыт"
    };
    crate::balloon::balloon_alert(world, mob, tank, text);
    tank_leak(world, tank);
    true
}

/// `tank_leak`: an open tap spills `amount_to_leak` of its reagent onto the turf.
fn tank_leak(world: &mut World, tank: Entity) {
    if world.get::<TapOpen>(tank).is_none() {
        return;
    }
    let Some(holder) = world.get::<Reagents>(tank).map(|r| r.0.clone()) else {
        return;
    };
    if holder.total_volume < LEAK_AMOUNT {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(tank) else {
        return;
    };
    if let Some(turf) = crate::construction::turf_at(world, pos) {
        let fraction = LEAK_AMOUNT / holder.total_volume.max(LEAK_AMOUNT);
        crate::janitor::expose(world, turf, &holder, fraction);
    }
    // `remove_reagent(reagent_id, amount_to_leak)`.
    let reagent = world
        .get::<DmReagents>(tank)
        .and_then(|d| d.list_reagents.first().map(|r| r.path.clone()));
    let id = reagent.and_then(|p| world.get_resource::<Chem>().and_then(|c| c.0.id(&p)));
    if let (Some(id), Some(mut r)) = (id, world.get_mut::<Reagents>(tank)) {
        r.0.remove_reagent(id, LEAK_AMOUNT);
    }
    playsound(
        world,
        PlaySound::new(pos, "sound/effects/glug.ogg", 33.0, true)
            .extrarange(SILENCED_SOUND_EXTRARANGE),
    );
}

/// `Moved` of a tank with its tap open.
fn leak_on_move(world: &mut World, mut cursor: Local<MessageCursor<ss13_world::Moved>>) {
    let moved: Vec<Entity> = cursor
        .read(world.resource::<Messages<ss13_world::Moved>>())
        .map(|m| m.entity)
        .collect();
    for e in moved {
        if world.get_entity(e).is_ok() && world.get::<TapOpen>(e).is_some() {
            tank_leak(world, e);
        }
    }
}

// --- Clicks -----------------------------------------------------------------------------------

fn attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    pizzabox_attack_self(world, mob, item) || tongs_attack_self(world, mob, item)
}

/// The other hand's plate or pizza box with the active hand's item (or the empty hand).
fn other_hand(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Option<Entity>,
    target: Entity,
) -> bool {
    let Some(held) = held else {
        if world.get::<PizzaBox>(target).is_some() {
            return pizzabox_take(world, mob, target);
        }
        if world.get::<Plate>(target).is_some() {
            let Some(food) = contents(world, target).last().copied() else {
                return false;
            };
            world.entity_mut(food).remove::<OnPlate>();
            give(world, mob, food);
            refresh_plate(world, target);
            return true;
        }
        return false;
    };
    if world.get::<PizzaBox>(target).is_some() {
        return pizzabox_item(world, client, mob, held, target);
    }
    if world.get::<Plate>(target).is_some() {
        return add_to_plate(world, mob, held, target, ClickModifiers::default());
    }
    if world.get::<Tongs>(held).is_some() {
        return tongs_on(world, client, mob, held, target);
    }
    false
}

/// A world click with the active hand's item.
fn held_on_target(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if is_under(&path_of(world, target), DISPENSER_ROOT) && tap(world, mob, held, target) {
        return true;
    }
    if world.get::<Tongs>(held).is_some() {
        return tongs_on(world, client, mob, held, target);
    }
    // `plate/pre_attack` on a mob: the first food is used on it, never a hit.
    if world.get::<Plate>(held).is_some()
        && world.get::<crate::chem::MobReagents>(target).is_some()
        && !contents(world, held).is_empty()
    {
        if world.get::<CombatMode>(mob).is_none_or(|c| !c.0) {
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            crate::food::try_to_eat(world, actor, target);
        }
        return true;
    }
    // `pizzabox/attack`: a pile swung at someone may fall apart (3+ boxes, `25%` per box).
    if world.get::<PizzaBox>(held).is_some() && world.get::<crate::health::Health>(target).is_some()
    {
        let n = world.get::<PizzaBox>(held).map_or(0, |b| b.boxes.len());
        if n >= 3 && world.resource_mut::<CombatRng>().chance(25.0 * n as f32) {
            disperse_pizzas(world, held);
        }
        return false;
    }
    if world.get::<PizzaBox>(target).is_some() && world.get::<TilePos>(target).is_some() {
        return pizzabox_item(world, client, mob, held, target);
    }
    if world.get::<Plate>(target).is_some() && world.get::<TilePos>(target).is_some() {
        return add_to_plate(world, mob, held, target, modifiers);
    }
    false
}

pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<(Span, String)>) {
    if let Some(b) = world.get::<PizzaBox>(item) {
        if b.open {
            if let Some(pizza) = b.pizza {
                lines.push((
                    Span::Notice,
                    format!(
                        "Внутри {}. Возьмите её другой рукой.",
                        name_of(world, pizza)
                    ),
                ));
            }
        } else {
            if !b.boxes.is_empty() {
                lines.push((
                    Span::Notice,
                    format!("Стопка коробок из-под пиццы: {} шт.", b.boxes.len() + 1),
                ));
            }
            let top = b.boxes.last().copied().unwrap_or(item);
            if let Some(tag) = world
                .get::<PizzaBox>(top)
                .map(|t| t.boxtag.clone())
                .filter(|t| !t.is_empty())
            {
                lines.push((Span::Notice, format!("На бирке написано: {tag}.")));
            }
        }
    }
    if let Some(food) = world.get::<Tongs>(item).and_then(|t| t.tonged) {
        lines.push((Span::Notice, format!("В них {}.", name_of(world, food))));
    }
    if world.get::<TapOpen>(item).is_some() {
        lines.push((Span::Warning, "Его кран открыт <b>ключом</b>!".to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plates_by_type() {
        let plate = plate_spec("/obj/item/plate").unwrap();
        assert_eq!((plate.max_items, plate.fragile), (8, true));
        assert!(!plate_spec("/obj/item/plate/iron").unwrap().fragile);
        assert!(!plate_spec("/obj/item/plate/oven_tray").unwrap().fragile);
        assert_eq!(
            plate_spec("/obj/item/plate/small").unwrap().biggest,
            WEIGHT_CLASS_SMALL
        );
        assert_eq!(plate_spec("/obj/item/plate/large").unwrap().max_items, 12);
        assert!(plate_spec("/obj/item/plate_shard").is_none());
    }

    #[test]
    fn boxed_pizzas_most_specific_first() {
        let pizza = |box_path: &str| {
            BOXED_PIZZAS
                .iter()
                .find(|(root, _)| is_under(box_path, root))
                .map(|&(_, p)| p)
        };
        assert_eq!(
            pizza("/obj/item/pizzabox/margherita/robo"),
            Some("/obj/item/food/pizza/margherita/robo")
        );
        assert_eq!(pizza("/obj/item/pizzabox"), None);
        assert_eq!(INFINITE_PIZZAS.iter().map(|&(_, w)| w).sum::<u32>(), 55);
    }
}
