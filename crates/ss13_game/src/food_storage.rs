//! Hiding things in food (server): tg `/datum/component/food_storage` (`food_storage.dm`) on
//! bread loaves, cakes, cheese wedges and wheels, firm cheese slices and cutlets, with the base
//! `/obj/item/on_accidental_consumption` (`items.dm`).
//!
//! - Out of combat mode a right click with an item no bigger than small (not a storage, not
//!   food) on such a food slips it inside after 1.5 s; one item per food.
//! - A ctrl-click on the food (adjacent) pulls the item out after 10 s, into the hands or onto
//!   the floor (it never pulls the food).
//! - Every bite rolls `bad = bitecount / (initial volume / bite size) × 100`: `bad - 50`% it is
//!   noticed before biting ("something in it") and comes out, else `bad`% it is bitten
//!   ([`accidental_consumption`]): a sharp item with force 5+ hurts the head (at least 15 brute),
//!   takes 2 breaths and bleeds 2 u of blood into the food; a reagent container gives the eater
//!   `min(15, volume / rand(5, 10))` of its reagents; anything else is "something strange".
//!   A bitten item comes out.
//! - Eaten up or cut into slices, the food drops what it hid.
// shortcut: no embedding of a bitten sharp item, no materials (glass shattering, disgust), tiny
// items are not swallowed into the stomach, no scream emote, no screentips.
// tg-fork deviation: tg deletes the hidden item with a sliced food (`qdel` takes its contents);
// the component's intent is that cutting the food reveals it, so slicing drops it.

use bevy::prelude::*;
use ss13_mapload::{DmFood, DmItem, DmWeight, DmWounding};
use ss13_reagents::{Holder, Owner, is_under};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::chem::{Chem, MobReagents, Reagents, name_of, tell_mob, visible};
use crate::combat::{CombatMode, CombatRng};
use crate::construction::Job;
use crate::food::FoodJob;
use crate::health::DamageType;
use crate::inventory::{CarriedBy, Inventory};
use crate::storage::{StoredIn, put_inside, take_out};
use crate::tools::{Actor, DoAfter, do_after, tell};

/// Types whose `Initialize` adds `/datum/component/food_storage`.
pub const STORAGE_FOODS: [&str; 7] = [
    "/obj/item/food/bread",
    "/obj/item/food/cake",
    "/obj/item/food/cheese/wedge",
    "/obj/item/food/cheese/wheel",
    "/obj/item/food/cheese/firm_cheese_slice",
    "/obj/item/food/meat/rawcutlet",
    "/obj/item/food/meat/cutlet",
];
/// `minimum_weight_class` (`WEIGHT_CLASS_SMALL`).
const MAX_W_CLASS: u8 = 2;
/// `insert_item`, `begin_remove_item`: deciseconds.
const INSERT_DS: f32 = 15.0;
const REMOVE_DS: f32 = 100.0;
const REAGENT_CONTAINER: &str = "/obj/item/reagent_containers";
const BLOOD: &str = "/datum/reagent/blood";

/// The hidden item (`stored_item`) and the food's volume when it got the component
/// (`initial_volume`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FoodStorage {
    pub stored: Option<Entity>,
    pub initial_volume: f32,
}

pub struct FoodStoragePlugin;

impl Plugin for FoodStoragePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `COMSIG_ATOM_ITEM_INTERACTION_SECONDARY`: a food in the world.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3291, |w, c| {
            c.modifiers.right
                && c.held.is_some_and(|h| {
                    crate::food_storage::try_insert(w, c.client, c.mob, h, c.target)
                })
        });
        // The same on a food in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 491, |w, c| {
            c.modifiers.right
                && c.held.is_some_and(|h| {
                    crate::food_storage::try_insert(w, c.client, c.mob, h, c.target)
                })
        });
        app.add_systems(Update, ctrl_clicks);
    }
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// Whether `path` hides items.
pub fn holds_items(path: &str) -> bool {
    STORAGE_FOODS.iter().any(|root| is_under(path, root))
}

fn is_storage_food(world: &World, food: Entity) -> bool {
    world.get::<DmFood>(food).is_some() && holds_items(&path_of(world, food))
}

/// The component, made on first use from the food's own reagents (`food_reagents`).
fn storage_of(world: &World, food: Entity) -> FoodStorage {
    world.get::<FoodStorage>(food).copied().unwrap_or_else(|| {
        let initial = world.get::<DmFood>(food).map_or(0.0, |f| {
            f.food_reagents.iter().map(|r| r.amount).sum::<f32>()
        });
        let volume = if initial > 0.0 {
            initial
        } else {
            world
                .get::<Reagents>(food)
                .map_or(0.0, |r| r.0.total_volume)
        };
        FoodStorage {
            stored: None,
            initial_volume: volume,
        }
    })
}

/// `stored_item` if it is still inside the food (`update_stored_item`).
pub fn stored_item(world: &World, food: Entity) -> Option<Entity> {
    world
        .get::<FoodStorage>(food)?
        .stored
        .filter(|&i| world.get::<StoredIn>(i).is_some_and(|s| s.0 == food))
}

/// The do_after anchor: the food on the floor, else the mob holding it.
fn anchor(world: &World, mob: Entity, food: Entity) -> Entity {
    if world.get::<TilePos>(food).is_some() {
        food
    } else {
        mob
    }
}

/// `try_inserting_item`. True when the click was the component's.
pub fn try_insert(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    food: Entity,
) -> bool {
    if held == food || !is_storage_food(world, food) {
        return false;
    }
    // No matryoshka-ing food storage.
    if world.get::<crate::storage::Storage>(held).is_some() || world.get::<DmFood>(held).is_some() {
        return false;
    }
    if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    let (what, name) = (name_of(world, held), name_of(world, food));
    let w_class = world
        .get::<DmWeight>(held)
        .copied()
        .unwrap_or_default()
        .w_class;
    if w_class > MAX_W_CLASS {
        tell(
            world,
            client,
            Span::Warning,
            format!("{what} won't fit in {name}."),
        );
        return true;
    }
    if stored_item(world, food).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            format!("There's something in {name}."),
        );
        return true;
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        &format!("You start to insert the {what} into {name}."),
        &format!("{user} begins inserting {what} into {name}."),
        Span::Notice,
    );
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let target = anchor(world, mob, food);
    do_after(
        world,
        actor,
        target,
        INSERT_DS,
        0,
        Job::Food(FoodJob::Hide(food)),
    );
    true
}

/// `insert_item` after its `do_after`: the item still in the inserter's hands, the food still
/// there and empty.
pub fn complete_insert(world: &mut World, mob: Entity, action: &DoAfter, food: Entity) {
    let Some(item) = action.item else {
        return;
    };
    if world.get_entity(food).is_err()
        || world.get_entity(item).is_err()
        || world.get::<CarriedBy>(item).map(|c| c.0) != Some(mob)
        || stored_item(world, food).is_some()
    {
        return;
    }
    if crate::inventory::no_drop(world, item) {
        let (what, name) = (name_of(world, item), name_of(world, food));
        tell(
            world,
            action.client,
            Span::Warning,
            format!("You can't seem to insert {what} into {name}."),
        );
        return;
    }
    let mut storage = storage_of(world, food);
    world.entity_mut(item).remove::<CarriedBy>();
    put_inside(world, item, food);
    storage.stored = Some(item);
    world.entity_mut(food).insert(storage);
    let (what, name) = (name_of(world, item), name_of(world, food));
    tell(
        world,
        action.client,
        Span::Notice,
        format!("You slip {what} inside {name}."),
    );
}

/// tg `COMSIG_CLICK_CTRL` on a food that hides items: the 10 s pull. True when it is such a
/// food, so the ctrl-click does not pull it (`CLICK_ACTION_SUCCESS`).
pub fn takes_ctrl_click(world: &World, target: Entity) -> bool {
    is_storage_food(world, target)
}

fn ctrl_clicks(
    world: &mut World,
    mut clicks: Local<bevy::ecs::message::MessageCursor<crate::click::CtrlClick>>,
) {
    let pending: Vec<crate::click::CtrlClick> = clicks
        .read(world.resource::<bevy::ecs::message::Messages<crate::click::CtrlClick>>())
        .copied()
        .collect();
    for click in pending {
        if !takes_ctrl_click(world, click.target)
            || world.get::<TilePos>(click.target) != Some(&click.target_pos)
            || world.get::<DoAfter>(click.mob).is_some()
        {
            continue;
        }
        let (user, name) = (name_of(world, click.mob), name_of(world, click.target));
        visible(
            world,
            click.mob,
            &format!("You start to rip into {name}."),
            &format!("{user} begins tearing at {name}."),
            Span::Notice,
        );
        let actor = Actor {
            client: click.client,
            mob: click.mob,
            item: None,
        };
        do_after(
            world,
            actor,
            click.target,
            REMOVE_DS,
            0,
            Job::Food(FoodJob::Unhide(click.target)),
        );
    }
}

/// `begin_remove_item` after its `do_after`.
pub fn complete_remove(world: &mut World, mob: Entity, action: &DoAfter, food: Entity) {
    if world.get_entity(food).is_err() {
        return;
    }
    if stored_item(world, food).is_none() {
        let name = name_of(world, food);
        tell(
            world,
            action.client,
            Span::Warning,
            format!("There's nothing in {name}."),
        );
        return;
    }
    remove_item(world, mob, food);
}

/// `remove_item`: into the user's hands, else onto the floor.
fn remove_item(world: &mut World, user: Entity, food: Entity) {
    let Some(item) = stored_item(world, food) else {
        return;
    };
    let Some(pos) =
        crate::chem::map_pos(world, food).or_else(|| world.get::<TilePos>(user).copied())
    else {
        return;
    };
    let (what, name, who) = (
        name_of(world, item),
        name_of(world, food),
        name_of(world, user),
    );
    take_out(world, item, pos);
    if let Some(mut s) = world.get_mut::<FoodStorage>(food) {
        s.stored = None;
    }
    let free_hand = world
        .get::<Inventory>(user)
        .is_some_and(|inv| inv.hands.iter().any(Option::is_none));
    if free_hand && let Some(inventory) = world.get::<Inventory>(user).cloned() {
        crate::storage::put_in_hands(world, user, &inventory, item);
        visible(
            world,
            user,
            &format!("You slowly pull {what} out of {name}."),
            &format!("{who} slowly pulls {what} out of {name}."),
            Span::Warning,
        );
    } else {
        visible(
            world,
            item,
            "",
            &format!("{what} falls out of {name}."),
            Span::Warning,
        );
    }
}

/// `storage_consumed` (`COMSIG_FOOD_CONSUMED`, and the food cut up): the item drops where the
/// food was. Call before the food is deleted.
pub fn release(world: &mut World, food: Entity) {
    let Some(item) = stored_item(world, food) else {
        return;
    };
    if let Some(pos) = crate::chem::map_pos(world, food) {
        take_out(world, item, pos);
    }
    if let Some(mut s) = world.get_mut::<FoodStorage>(food) {
        s.stored = None;
    }
}

/// `bad_chance_of_discovery` for a bite: `bitecount / (initial_volume / bitesize) × 100`.
pub fn bad_chance(bitecount: u32, initial_volume: f32, bitesize: f32) -> f32 {
    if initial_volume <= 0.0 || bitesize <= 0.0 {
        return 100.0;
    }
    bitecount as f32 / (initial_volume / bitesize) * 100.0
}

/// `consume_food_storage` (`COMSIG_FOOD_EATEN`): after a bite of `food` by `eater`, fed by
/// `feeder`.
pub fn on_eaten(world: &mut World, eater: Entity, feeder: Entity, food: Entity) {
    if stored_item(world, food).is_none() {
        return;
    }
    let storage = storage_of(world, food);
    let bitecount = world
        .get::<crate::food::Food>(food)
        .map_or(0, |f| f.bitecount);
    let bitesize = world
        .get::<DmFood>(food)
        .map_or(2.0, |f| f.bite_consumption);
    let bad = bad_chance(bitecount, storage.initial_volume, bitesize);
    let good = bad - 50.0;
    let name = name_of(world, food);
    let mut discovered = false;
    if world.resource_mut::<CombatRng>().chance(good) {
        discovered = true;
        tell_mob(
            world,
            eater,
            Span::Warning,
            &format!("It feels like there's something in {name}...!"),
        );
    } else if world.resource_mut::<CombatRng>().chance(bad) {
        let item = stored_item(world, food).expect("checked");
        info!(
            "food storage: {} fed a hidden {} in {name}",
            name_of(world, feeder),
            name_of(world, item)
        );
        discovered = accidental_consumption(world, item, eater, food);
    }
    if discovered {
        remove_item(world, feeder, food);
    }
}

/// `/obj/item/on_accidental_consumption(victim, user, source_item)` and the reagent
/// containers' override. True when the item is found.
pub fn accidental_consumption(
    world: &mut World,
    item: Entity,
    victim: Entity,
    food: Entity,
) -> bool {
    let path = path_of(world, item);
    if is_under(&path, REAGENT_CONTAINER) {
        container_bitten(world, item, victim);
    }
    let sharp = world
        .get::<DmWounding>(item)
        .is_some_and(|w| w.sharpness != 0);
    let force = world.get::<DmItem>(item).map_or(0.0, |i| i.force);
    let source = name_of(world, food);
    if sharp && force >= 5.0 {
        let who = name_of(world, victim);
        visible(
            world,
            victim,
            "OH GOD! Was that a crunch? That didn't feel good at all!!",
            &format!("{who} looks like they've just bit something they shouldn't have!"),
            Span::Warning,
        );
        crate::mobs::defense::hurt(
            world,
            victim,
            DamageType::Brute,
            force.max(15.0),
            Some(crate::zone::BodyZone::Head),
        );
        lose_breath(world, victim, 2.0);
        add_reagent(world, food, BLOOD, 2.0);
        return true;
    }
    tell_mob(
        world,
        victim,
        Span::Warning,
        &format!("Something strange was in {source}..."),
    );
    true
}

/// `/obj/item/reagent_containers/on_accidental_consumption`: 2 breaths and a swallow of it.
fn container_bitten(world: &mut World, item: Entity, victim: Entity) {
    lose_breath(world, victim, 2.0);
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut held) = world.get::<Reagents>(item).map(|r| r.0.clone()) else {
        return;
    };
    let divisor = world.resource_mut::<CombatRng>().range(5, 10) as f32;
    let amount = (held.total_volume / divisor).min(15.0);
    if amount <= 0.0 {
        return;
    }
    if let Some(mut m) = world.get_mut::<MobReagents>(victim) {
        held.trans_to(&chem, &mut m.stomach, amount, 1.0, None, false, false);
    } else {
        let mut gone = Holder::new(amount, 0, Owner::default());
        held.trans_to(&chem, &mut gone, amount, 1.0, None, false, false);
    }
    world.entity_mut(item).insert(Reagents(held));
}

fn lose_breath(world: &mut World, mob: Entity, amount: f32) {
    if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
        r.losebreath += amount;
    }
}

fn add_reagent(world: &mut World, e: Entity, path: &str, amount: f32) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(id) = chem.id(path) else {
        return;
    };
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        r.0.add_reagent_list(&chem, &[(id, amount)], None, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_grows_with_the_bites() {
        // A 20 u loaf eaten 2 u a bite: the 5th bite is a 50% bite, the 10th a sure one.
        assert_eq!(bad_chance(5, 20.0, 2.0), 50.0);
        assert_eq!(bad_chance(10, 20.0, 2.0), 100.0);
        assert!(holds_items("/obj/item/food/bread/plain"));
        assert!(holds_items("/obj/item/food/cheese/wheel"));
        assert!(!holds_items("/obj/item/food/breadslice/plain"));
    }
}
